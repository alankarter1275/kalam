//! Comic series folders on disk (item 2.22).
//!
//! Owner decision, 2026-10-02: a comic series is one folder under the
//! library root — `library/<Series>/` — holding every chapter file, named
//! for its chapter number, with `covers/` beside them and a hidden
//! `.kalam/` for the sidecars. The in-app half (series hub, chapter
//! drawer, reader flow, the `comic_series`/`comic_chapters` hierarchy)
//! already existed; this module is the disk half.
//!
//! [`migrate_comic_library`] is the startup pass that brings an existing
//! library into that shape. It is idempotent and self-healing rather than
//! flag-guarded: a chapter's stored `file_name` says whether it has been
//! placed (`Naruto – Digital Colored Comics/0003.cbz`) or not
//! (`book.cbz`), so the pass skips what is done, retries what failed, and
//! costs one string comparison per chapter once the library is in shape.
//!
//! Atomicity is per chapter, not per series: the file moves first, then
//! the row's stored names change. A failure between the two leaves the
//! row pointing at the old location, which still resolves; the next pass
//! finds the file already at its destination and adopts it. Nothing is
//! ever half-visible, and no book ever resolves to nowhere.

use crate::db::Catalog;
use crate::tasks::Reporter;

/// The first eight characters of a uuid, or the whole thing when it is
/// shorter — the same short id book folders end in.
fn short_id(uuid: &str) -> &str {
    uuid.get(0..8).unwrap_or(uuid)
}

/// Move every unplaced comic chapter into its series folder. Returns how
/// many chapters moved; zero is the steady state.
pub fn migrate_comic_library(catalog: &Catalog, reporter: &Reporter) -> usize {
    // The catalog half first: a comic that predates the hierarchy needs its
    // series and chapter rows before placement can group it. Idempotent,
    // and cheap once every comic has them.
    let _ = catalog.migrate_comic_series_and_chapters();

    let Ok(series_list) = catalog.list_comic_series("") else {
        return 0;
    };
    let total: usize = series_list.iter().map(|s| s.total_chapters).sum();
    let mut done = 0usize;
    let mut moved = 0usize;

    for series in &series_list {
        let Ok(chapters) = catalog.chapters_for_series(series.id) else {
            continue;
        };
        for chapter in chapters {
            if reporter.cancelled() {
                // Moves already made stay made, and the pass is idempotent,
                // so stopping early loses nothing.
                return moved;
            }
            done += 1;
            reporter.step(done, total, chapter.book.uuid.clone());

            if chapter.book.file_name.contains('/') {
                // Already placed.
                continue;
            }
            if place_chapter(catalog, series.id, chapter.chapter_number, &chapter.book) {
                moved += 1;
            }
        }
    }
    moved
}

/// Move one chapter, its cover and its sidecar into the series folder and
/// repoint the row. Returns `true` when the chapter ended up placed.
///
/// Only for a chapter still in the legacy shape (`book.<ext>` in its own
/// folder); a chapter whose stored name is already series-relative is
/// placed by definition and is returned as such.
fn place_chapter(
    catalog: &Catalog,
    series_id: i64,
    chapter_number: f32,
    book: &crate::models::Book,
) -> bool {
    if book.file_name.contains('/') {
        return true;
    }

    // The folder name comes from the series — reusing the name any placed
    // sibling already carries, so a series can never split across folders.
    // Nothing is created yet: a series whose files are all missing must not
    // leave an empty folder behind.
    let Ok(folder) = catalog.series_folder_name_for(series_id) else {
        log::warn!("comic series {series_id}: no folder name could be resolved; chapters stay put");
        return false;
    };
    let lib = crate::paths::library_dir();
    let series_dir = lib.join(&folder);
    let stem = crate::paths::comic_chapter_stem(chapter_number);
    let ext = book
        .file_name
        .rsplit('.')
        .next()
        .filter(|e| !e.is_empty() && e.len() <= 4)
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "cbz".to_string());

    // The chapter file: number-named, with the short id appended when a
    // file already answers to that name (two chapters claiming one number,
    // or a leftover). Never overwrite what is already there. A file that
    // is already at the plain name while the source is gone is a move from
    // a previous run whose row update failed — adopt it rather than
    // suffixed-duplicating it.
    let plain_rel = format!("{folder}/{stem}.{ext}");
    let plain_dst = lib.join(&plain_rel);
    let new_file_rel = if !book.file_path.exists() && plain_dst.is_file() {
        plain_rel
    } else {
        if !book.file_path.exists() {
            log::warn!(
                "comic chapter {}: {} is missing; the row keeps pointing at it and the \
                 next pass will retry",
                book.uuid,
                book.file_path.display()
            );
            return false;
        }
        if std::fs::create_dir_all(series_dir.join("covers")).is_err() {
            log::warn!(
                "comic series folder {}: could not be created; chapters stay put",
                series_dir.display()
            );
            return false;
        }
        let mut file_part = format!("{stem}.{ext}");
        if series_dir.join(&file_part).exists() {
            file_part = format!("{stem} {}.{ext}", short_id(&book.uuid));
        }
        let rel = format!("{folder}/{file_part}");
        match std::fs::rename(&book.file_path, lib.join(&rel)) {
            Ok(()) => rel,
            Err(err) => {
                log::warn!(
                    "comic chapter {}: could not move {} ({}); the row still points there, \
                     and the next pass will retry",
                    book.uuid,
                    book.file_path.display(),
                    err
                );
                return false;
            }
        }
    };
    let new_file = lib.join(&new_file_rel);

    // The cover, named for the same stem inside `covers/`. A failure here
    // loses the cover from the moved chapter but not the chapter: the row
    // keeps its old cover name only when the new file is not there.
    let mut new_cover_rel: Option<String> = None;
    if let (Some(old_cover_path), Some(old_cover_name)) = (&book.cover_path, &book.cover_name) {
        let img_ext = old_cover_name
            .rsplit('.')
            .next()
            .filter(|e| !e.is_empty() && e.len() <= 4)
            .unwrap_or("jpg");
        let cover_rel = format!("{folder}/covers/{stem}.{img_ext}");
        let cover_dst = lib.join(&cover_rel);
        if old_cover_path != &cover_dst {
            let _ = std::fs::rename(old_cover_path, &cover_dst);
        }
        if cover_dst.is_file() {
            new_cover_rel = Some(cover_rel);
        }
    }

    // The sidecar, from the per-book folder into the series' hidden
    // `.kalam/` directory. Best-effort: a lost sidecar is a lost backup,
    // not a lost book.
    let old_sidecar = crate::sidecar::sidecar_path(&book.uuid);
    if old_sidecar.is_file() {
        let new_sidecar = series_dir.join(".kalam").join(format!("{}.json", book.uuid));
        if std::fs::create_dir_all(series_dir.join(".kalam")).is_ok() {
            let _ = std::fs::rename(&old_sidecar, &new_sidecar);
        }
    }

    // Repoint the row. When this fails, the file is already placed and the
    // row still says `book.cbz` — resolution finds the old (missing)
    // location this run, and the next pass adopts the moved file above.
    if let Err(err) = catalog.set_book_file_names(
        book.id,
        Some(&new_file_rel),
        new_cover_rel.as_deref(),
    ) {
        log::warn!(
            "comic chapter {}: moved to {} but the row could not be updated ({err}); \
             the next pass will adopt it",
            book.uuid,
            new_file.display()
        );
    }

    // The per-book folder is garbage once nothing is left in it.
    if let Some(old_dir) = book.file_path.parent() {
        let empty = std::fs::read_dir(old_dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if empty {
            let _ = std::fs::remove_dir(old_dir);
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookFormat;

    /// A series title unique to this test run, so parallel tests — and any
    /// real library this machine happens to be carrying — cannot collide.
    fn unique_series(tag: &str) -> String {
        format!("{tag} {}", uuid::Uuid::new_v4())
    }

    /// Seed one legacy-shaped comic chapter: a row whose file is still
    /// `book.cbz` inside its own uuid folder, with the folder and its files
    /// actually on disk. Returns (book_id, series_id).
    fn seed_legacy_chapter(
        cat: &Catalog,
        series_title: &str,
        uuid: &str,
        chapter_number: f32,
    ) -> (i64, i64) {
        let old_dir = crate::paths::book_dir(uuid);
        std::fs::create_dir_all(&old_dir).expect("create legacy book folder");
        std::fs::write(old_dir.join("book.cbz"), format!("pages of {uuid}"))
            .expect("write chapter file");
        std::fs::write(old_dir.join("cover.jpg"), b"cover bytes").expect("write cover");
        std::fs::write(old_dir.join("kalam.json"), b"{}").expect("write sidecar");

        let id = cat
            .insert_book(
                uuid,
                series_title,
                "Hero",
                Some(series_title),
                "",
                BookFormat::Cbz,
                "book.cbz",
                &format!("hash-{uuid}"),
                Some("cover.jpg"),
                &[],
            )
            .expect("insert chapter row");
        let s_id = cat
            .get_or_create_comic_series(series_title, Some("Hero"), None)
            .expect("create series row");
        cat.add_comic_chapter(s_id, id, chapter_number, None, series_title)
            .expect("add chapter");
        (id, s_id)
    }

    #[test]
    fn places_a_chapter_with_cover_and_sidecar() {
        let cat = Catalog::open_in_memory().unwrap();
        let series = unique_series("Placement One");
        let uuid = format!("{}-0001", uuid::Uuid::new_v4());
        let (id, s_id) = seed_legacy_chapter(&cat, &series, &uuid, 10.0);

        let book = cat.get_book(id).unwrap().unwrap();
        assert!(place_chapter(&cat, s_id, 10.0, &book), "chapter must place");

        let folder = cat.series_folder_name_for(s_id).unwrap();
        let lib = crate::paths::library_dir();
        let stem = crate::paths::comic_chapter_stem(10.0);
        let chapter = lib.join(&folder).join(format!("{stem}.cbz"));
        assert!(chapter.is_file(), "chapter file at {}", chapter.display());
        assert_eq!(
            std::fs::read(&chapter).unwrap(),
            format!("pages of {uuid}").into_bytes()
        );
        let cover = lib.join(&folder).join("covers").join(format!("{stem}.jpg"));
        assert!(cover.is_file(), "cover at {}", cover.display());
        assert!(lib
            .join(&folder)
            .join(".kalam")
            .join(format!("{uuid}.json"))
            .is_file());

        // The row now stores library-relative names and resolves to the
        // placed files.
        let updated = cat.get_book(id).unwrap().unwrap();
        assert_eq!(updated.file_name, format!("{folder}/{stem}.cbz"));
        assert_eq!(
            updated.cover_name.as_deref(),
            Some(format!("{folder}/covers/{stem}.jpg").as_str())
        );
        assert!(updated.file_path.is_file());
        assert!(updated.cover_path.as_ref().is_some_and(|p| p.is_file()));

        // The legacy per-book folder is gone.
        assert!(!crate::paths::book_dir(&uuid).exists());

        let _ = std::fs::remove_dir_all(lib.join(&folder));
    }

    #[test]
    fn a_placed_chapter_is_left_alone() {
        let cat = Catalog::open_in_memory().unwrap();
        let series = unique_series("Placement Two");
        let uuid = format!("{}-0002", uuid::Uuid::new_v4());
        let (id, s_id) = seed_legacy_chapter(&cat, &series, &uuid, 3.0);

        let book = cat.get_book(id).unwrap().unwrap();
        assert!(place_chapter(&cat, s_id, 3.0, &book));
        let placed = cat.get_book(id).unwrap().unwrap();
        let before = placed.file_name.clone();

        // The second call is a no-op that reports success: the stored name
        // already carries the series folder.
        assert!(place_chapter(&cat, s_id, 3.0, &placed));
        let after = cat.get_book(id).unwrap().unwrap();
        assert_eq!(after.file_name, before, "a placed chapter must not move again");

        let _ = std::fs::remove_dir_all(crate::paths::library_dir().join(
            cat.series_folder_name_for(s_id).unwrap(),
        ));
    }

    #[test]
    fn missing_files_leave_the_row_and_the_library_alone() {
        let cat = Catalog::open_in_memory().unwrap();
        let series = unique_series("Placement Three");
        let uuid = format!("{}-0003", uuid::Uuid::new_v4());
        // A row, but no folder and no files on disk.
        let id = cat
            .insert_book(
                &uuid,
                &series,
                "Hero",
                Some(&series),
                "",
                BookFormat::Cbz,
                "book.cbz",
                &format!("hash-{uuid}"),
                None,
                &[],
            )
            .unwrap();
        let s_id = cat.get_or_create_comic_series(&series, Some("Hero"), None).unwrap();
        cat.add_comic_chapter(s_id, id, 7.0, None, &series).unwrap();

        let book = cat.get_book(id).unwrap().unwrap();
        assert!(!place_chapter(&cat, s_id, 7.0, &book));

        let unchanged = cat.get_book(id).unwrap().unwrap();
        assert_eq!(unchanged.file_name, "book.cbz");
        // No empty series folder was created for a chapter with no files.
        let folder = cat.series_folder_name_for(s_id).unwrap();
        assert!(
            !crate::paths::library_dir().join(&folder).exists(),
            "no folder should materialize without files"
        );
    }

    #[test]
    fn two_chapters_claiming_one_number_do_not_collide() {
        let cat = Catalog::open_in_memory().unwrap();
        let series = unique_series("Placement Four");
        let uuid_a = "aaaaaaaa-1111-1111-1111-111111111111";
        let uuid_b = "22222222-2222-2222-2222-222222222222";
        let (id_a, s_id) = seed_legacy_chapter(&cat, &series, uuid_a, 5.0);
        let (id_b, _) = seed_legacy_chapter(&cat, &series, uuid_b, 5.0);

        let book_a = cat.get_book(id_a).unwrap().unwrap();
        assert!(place_chapter(&cat, s_id, 5.0, &book_a));
        let book_b = cat.get_book(id_b).unwrap().unwrap();
        assert!(place_chapter(&cat, s_id, 5.0, &book_b));

        let folder = cat.series_folder_name_for(s_id).unwrap();
        let series_dir = crate::paths::library_dir().join(&folder);
        assert!(series_dir.join("0005.cbz").is_file());
        let suffixed = series_dir.join(format!("0005 {}.cbz", short_id(uuid_b)));
        assert!(suffixed.is_file(), "second chapter at {}", suffixed.display());

        // Both rows resolve to their own file.
        for id in [id_a, id_b] {
            let book = cat.get_book(id).unwrap().unwrap();
            assert!(book.file_path.is_file(), "row {} must resolve", book.uuid);
        }

        let _ = std::fs::remove_dir_all(series_dir);
    }
}
