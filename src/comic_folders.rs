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
use std::path::{Path, PathBuf};

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
                // Placed — but its cover may still be stranded in the old
                // per-book folder (the field state of 2026-10-02). The
                // check is one directory lookup per chapter.
                adopt_leftovers(catalog, chapter.chapter_number, &chapter.book);
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

    // Sweep the old folder after the file: the cover and the sidecar move
    // by what is on disk, not by what the row claims. The owner's field
    // test (2026-10-02) found every cover still sitting in the old folders
    // after the chapter files had moved — whatever the rows said, the
    // directory listing was the truth. Anything in a book's own folder
    // that is ours (a cover image, kalam.json) moves; anything else stays,
    // and so does the folder.
    let new_cover_rel = sweep_old_folder(book, old_dir_of(book), &series_dir, &folder, &stem);

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

    true
}

/// The folder a legacy chapter's files live in: the parent of the
/// (not yet repointed) file path.
fn old_dir_of(book: &crate::models::Book) -> &Path {
    book.file_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
}

/// Move a book's cover and sidecar out of `old_dir` into the series
/// folder, and remove `old_dir` when nothing but ours ever lived there.
/// Returns the stored (library-relative) name for the moved cover, or
/// `None` when no cover was found.
///
/// Driven by the directory listing: the row's cover name, when it matches
/// a file that is there, only decides *which* image takes the chapter's
/// stem name; the rest move alongside under suffixed names so nothing is
/// overwritten and nothing is left behind.
fn sweep_old_folder(
    book: &crate::models::Book,
    old_dir: &Path,
    series_dir: &Path,
    folder: &str,
    stem: &str,
) -> Option<String> {
    let entries = std::fs::read_dir(old_dir).ok()?;
    let covers_dir = series_dir.join("covers");

    let mut images: Vec<(String, PathBuf)> = Vec::new();
    let mut has_sidecar = false;
    let mut foreign = false;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if lower == "kalam.json" {
            has_sidecar = true;
        } else if matches!(
            lower.rsplit('.').next(),
            Some("jpg") | Some("jpeg") | Some("png") | Some("webp") | Some("gif")
        ) {
            images.push((name, entry.path()));
        } else {
            // A stale archive, or a file the user put here: not ours to
            // move or delete. The folder stays so it stays visible.
            foreign = true;
        }
    }

    // The row's named cover, when it is actually there, takes the stem
    // name; anything else moves under a suffix.
    images.sort_by_key(|(name, _)| {
        if book.cover_name.as_deref() == Some(name.as_str()) {
            0
        } else {
            1
        }
    });

    if !images.is_empty() {
        let _ = std::fs::create_dir_all(&covers_dir);
    }
    let mut stored: Option<String> = None;
    for (i, (name, path)) in images.iter().enumerate() {
        let ext = name
            .rsplit('.')
            .next()
            .unwrap_or("jpg")
            .to_ascii_lowercase();
        // Never overwrite: a file already answering to this chapter's name
        // (another chapter claiming the same number) gets the next suffix.
        let mut file_part = if i == 0 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} {i}.{ext}")
        };
        let mut n = i;
        while covers_dir.join(&file_part).exists() {
            n += 1;
            file_part = format!("{stem} {n}.{ext}");
        }
        let dst = covers_dir.join(&file_part);
        match std::fs::rename(path, &dst) {
            Ok(()) => {
                log::info!(
                    "comic chapter {}: moved {} into {}",
                    book.uuid,
                    path.display(),
                    dst.display()
                );
                if i == 0 {
                    stored = Some(format!("{folder}/covers/{file_part}"));
                }
            }
            Err(err) => log::warn!(
                "comic chapter {}: could not move cover {} ({err})",
                book.uuid,
                path.display()
            ),
        }
    }

    if has_sidecar {
        let new_sidecar = series_dir.join(".kalam").join(format!("{}.json", book.uuid));
        if std::fs::create_dir_all(series_dir.join(".kalam")).is_ok() {
            let _ = std::fs::rename(old_dir.join("kalam.json"), &new_sidecar);
        }
    }

    if !foreign {
        // Nothing of anyone's is left in the book's own folder.
        let _ = std::fs::remove_dir(old_dir);
    }
    stored
}

/// Heal a placed chapter whose cover (or sidecar) is still sitting in its
/// old per-book folder — the exact state the owner's field test found on
/// 2026-10-02: every chapter file had moved into its series folder, every
/// cover had stayed behind. Cheap when there is nothing to do: one
/// directory existence check per chapter.
fn adopt_leftovers(
    catalog: &Catalog,
    chapter_number: f32,
    book: &crate::models::Book,
) -> bool {
    // Only a row that still names its cover the legacy way — a bare name,
    // or none at all — can have leftovers to adopt.
    let cover_is_legacy = book
        .cover_name
        .as_deref()
        .map(|n| !n.contains('/'))
        .unwrap_or(true);
    if !cover_is_legacy {
        return false;
    }
    let old_dir = crate::paths::book_dir(&book.uuid);
    if !old_dir.is_dir() {
        return false;
    }
    let Some((series_prefix, _)) = book.file_name.rsplit_once('/') else {
        return false;
    };
    let series_dir = crate::paths::library_dir().join(series_prefix);
    let stem = crate::paths::comic_chapter_stem(chapter_number);
    let Some(cover_rel) = sweep_old_folder(book, &old_dir, &series_dir, series_prefix, &stem)
    else {
        return false;
    };
    if let Err(err) = catalog.set_book_file_names(book.id, None, Some(&cover_rel)) {
        log::warn!(
            "comic chapter {}: adopted its cover but the row could not be updated ({err})",
            book.uuid
        );
        return false;
    }
    log::info!("comic chapter {}: adopted leftover cover from {}", book.uuid, old_dir.display());
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
    fn sweep_adopts_a_cover_the_row_does_not_know_about() {
        // The field state behind the 2026-10-02 report, shape one: the
        // cover file exists on disk while the row names no cover at all.
        // The directory listing is the truth; the row catches up.
        let cat = Catalog::open_in_memory().unwrap();
        let series = unique_series("Placement Five");
        let uuid = format!("{}-0005", uuid::Uuid::new_v4());
        let old_dir = crate::paths::book_dir(&uuid);
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join("book.cbz"), b"pages").unwrap();
        std::fs::write(old_dir.join("cover.jpg"), b"unknown cover").unwrap();
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
        cat.add_comic_chapter(s_id, id, 4.0, None, &series).unwrap();

        let book = cat.get_book(id).unwrap().unwrap();
        assert!(place_chapter(&cat, s_id, 4.0, &book));

        let folder = cat.series_folder_name_for(s_id).unwrap();
        let cover = crate::paths::library_dir()
            .join(&folder)
            .join("covers")
            .join("0004.jpg");
        assert!(cover.is_file(), "cover must be adopted into {cover:?}");
        let updated = cat.get_book(id).unwrap().unwrap();
        assert_eq!(
            updated.cover_name.as_deref(),
            Some(format!("{folder}/covers/0004.jpg").as_str())
        );
        assert!(!old_dir.exists(), "the old folder must be gone");

        let _ = std::fs::remove_dir_all(crate::paths::library_dir().join(&folder));
    }

    #[test]
    fn placed_chapter_adopts_its_stranded_cover() {
        // The field state behind the 2026-10-02 report, shape two: the
        // chapter file moved into its series folder, but the cover is
        // still sitting in the old per-book folder and the row still
        // names it the legacy way. The adoption pass heals it.
        let cat = Catalog::open_in_memory().unwrap();
        let series = unique_series("Placement Six");
        let uuid = format!("{}-0006", uuid::Uuid::new_v4());
        let (id, s_id) = seed_legacy_chapter(&cat, &series, &uuid, 10.0);
        assert!(place_chapter(&cat, s_id, 10.0, &cat.get_book(id).unwrap().unwrap()));

        // Recreate the stranded state: cover back in a legacy folder, row
        // back to the bare name.
        let legacy = crate::paths::book_dir(&uuid);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("cover.jpg"), b"stranded cover").unwrap();
        cat.set_book_file_names(id, None, Some("cover.jpg")).unwrap();

        let placed = cat.get_book(id).unwrap().unwrap();
        assert!(adopt_leftovers(&cat, 10.0, &placed), "adoption must run");

        let folder = cat.series_folder_name_for(s_id).unwrap();
        let cover = crate::paths::library_dir()
            .join(&folder)
            .join("covers")
            .join("0010.jpg");
        assert_eq!(
            std::fs::read(&cover).unwrap(),
            b"stranded cover",
            "the stranded cover must now live in the series folder"
        );
        let healed = cat.get_book(id).unwrap().unwrap();
        assert_eq!(
            healed.cover_name.as_deref(),
            Some(format!("{folder}/covers/0010.jpg").as_str())
        );
        assert!(!legacy.exists(), "the legacy folder must be cleaned up");
        assert!(healed.file_path.is_file(), "the chapter file stays put");

        let _ = std::fs::remove_dir_all(crate::paths::library_dir().join(&folder));
    }

    #[test]
    fn deleting_a_comic_chapter_removes_the_whole_series() {
        // Field report 2026-10-02: deleting a comic from the library
        // removed one chapter and left the rest. A comic is deleted at
        // its series' scope.
        let cat = Catalog::open_in_memory().unwrap();
        let series_a = unique_series("Placement Seven A");
        let series_b = unique_series("Placement Seven B");
        let (a1, s_a) = seed_legacy_chapter(&cat, &series_a, "aaaa1111-1111-1111-1111-111111111111", 1.0);
        let (a2, _) = seed_legacy_chapter(&cat, &series_a, "aaaa2222-2222-2222-2222-222222222222", 2.0);
        let (b1, s_b) = seed_legacy_chapter(&cat, &series_b, "bbbb3333-3333-3333-3333-333333333333", 1.0);
        // Place everything, covers included. The stem comes from the
        // chapter number, same as the real pass.
        for (id, s_id, ch) in [(a1, s_a, 1.0), (a2, s_a, 2.0), (b1, s_b, 1.0)] {
            let book = cat.get_book(id).unwrap().unwrap();
            assert!(place_chapter(&cat, s_id, ch, &book));
        }
        let folder_a = cat.series_folder_name_for(s_a).unwrap();

        // The scope count tells the confirmation the truth: two chapters
        // of A plus one of B.
        assert_eq!(cat.delete_scope_count(&[a1, b1]), 3);

        cat.delete_book(a1).unwrap();
        assert!(cat.get_book(a1).unwrap().is_none());
        assert!(cat.get_book(a2).unwrap().is_none(), "the sibling chapter must go too");
        assert!(cat.get_comic_series(s_a).unwrap().is_none(), "the series row must go");
        assert!(
            !crate::paths::library_dir().join(&folder_a).exists(),
            "the series folder must be gone"
        );
        assert!(cat.get_book(b1).unwrap().is_some(), "other series untouched");
        assert!(cat.get_comic_series(s_b).unwrap().is_some());

        let _ = std::fs::remove_dir_all(crate::paths::library_dir().join(
            cat.series_folder_name_for(s_b).unwrap(),
        ));
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
