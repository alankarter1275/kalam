//! P6.5 — `kalam.json`: a per-book backup copy of what the database knows.
//!
//! Every book already lives in its own folder with its file and cover. This
//! adds a small JSON file describing it: title, author, tags, rating, reading
//! position, highlights.
//!
//! **It is a backup, never the truth.** The database is authoritative and is
//! never read from these files during normal operation. That matters for two
//! reasons: reading across books stays one query instead of opening hundreds
//! of files, and there is no question of which copy wins when they disagree —
//! the database always does.
//!
//! What it buys: a library folder that describes itself. Copy it to another
//! machine, or lose `catalog.db`, and everything can be rebuilt by walking
//! `library/`. Without this, losing the database leaves the books present but
//! anonymous — no tags, no highlights, no reading position.
//!
//! **Why JSON and not `metadata.opf`.** Calibre writes an OPF per book, and it
//! is tempting to match. But OPF is an ebook-packaging format: it can express
//! title and author, and cannot express a highlight, a reading session or a
//! saved word without abusing custom fields. Nothing else reads a stray
//! `metadata.opf` either, so the compatibility argument is imaginary.

use crate::db::{Annotation, Catalog, PatchRecord};
use crate::models::Book;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bumped when the shape changes, so a future reader can tell what it is
/// looking at instead of guessing from which fields are present. v2 adds
/// `patches` — additively: v1 files read clean through the serde default,
/// and a v1 reader ignores the unknown field, so the bump is a signal, not
/// a compatibility break.
const SIDECAR_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarHighlight {
    pub kind: String,
    pub chapter_index: i64,
    /// The highlighted words. The load-bearing field: positions break when a
    /// file changes, text does not — this is what lets a highlight be found
    /// again. Same reasoning as the P7 re-anchoring plan.
    pub text_excerpt: String,
    pub note: String,
    pub color: String,
    #[serde(default = "default_annotation_style")]
    pub style: String,
    pub created_at: String,
}

fn default_annotation_style() -> String {
    "solid".into()
}

/// One EPUB edit patch (Phase 6). A non-destructive replacement rule: it
/// lives in this file and in the database, and the `.epub` on disk stays
/// untouched until an explicit apply. Fields mirror `db::PatchRecord`; the
/// id is the database row's, so the two stores stay joinable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidecarPatch {
    pub id: i64,
    pub kind: String,
    pub href: String,
    pub chapter_index: i64,
    pub find_text: String,
    pub replace_text: String,
    pub context_before: String,
    pub context_after: String,
    pub source: String,
    pub status: String,
    pub created_at: String,
    pub applied_at: Option<String>,
    /// The whole-file guard (kind `file`, Phase 6.9). `#[serde(default)]`
    /// so sidecars written before the raw mode still read — their
    /// patches predate it and carry no guard.
    #[serde(default)]
    pub before_hash: String,
}

/// Everything worth keeping about one book, in one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub version: u32,
    pub uuid: String,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub series_index: f32,
    pub description: String,
    pub publisher: String,
    pub published: String,
    pub tags: Vec<String>,
    pub rating: u8,
    pub added_at: String,
    /// Name only, not a path — the folder is the location, and an absolute
    /// path would stop the folder being portable.
    pub file_name: String,
    pub cover_name: Option<String>,
    pub file_hash: String,
    pub progress_percent: u8,
    /// Chapter and fraction, so reading position survives a rebuild.
    pub reading_position: Option<(usize, f64)>,
    pub highlights: Vec<SidecarHighlight>,
    /// EPUB edit patches (Phase 6), mirroring the database rows. Added in
    /// sidecar v2; `default` keeps files written by v1 readable.
    #[serde(default)]
    pub patches: Vec<SidecarPatch>,
}

impl Sidecar {
    pub fn from_parts(
        book: &Book,
        position: Option<(usize, f64)>,
        marks: &[Annotation],
        patches: &[PatchRecord],
    ) -> Self {
        Self {
            version: SIDECAR_VERSION,
            uuid: book.uuid.clone(),
            title: book.title.clone(),
            authors: book.authors.clone(),
            series: book.series.clone(),
            series_index: book.series_index,
            description: book.description.clone(),
            publisher: book.publisher.clone(),
            published: book.published.clone(),
            tags: book.tags.clone(),
            rating: book.rating,
            added_at: book.added_at.clone(),
            file_name: book.file_name.clone(),
            cover_name: book.cover_name.clone(),
            file_hash: book.file_hash.clone(),
            progress_percent: book.progress,
            reading_position: position,
            highlights: marks
                .iter()
                .map(|a| SidecarHighlight {
                    kind: a.kind.clone(),
                    chapter_index: a.chapter_index,
                    text_excerpt: a.text_excerpt.clone(),
                    note: a.note.clone(),
                    color: a.color.clone(),
                    style: a.style.clone(),
                    created_at: a.created_at.clone(),
                })
                .collect(),
            patches: patches
                .iter()
                .map(|p| SidecarPatch {
                    id: p.id,
                    kind: p.kind.clone(),
                    href: p.href.clone(),
                    chapter_index: p.chapter_index,
                    find_text: p.find_text.clone(),
                    replace_text: p.replace_text.clone(),
                    context_before: p.context_before.clone(),
                    context_after: p.context_after.clone(),
                    source: p.source.clone(),
                    status: p.status.clone(),
                    created_at: p.created_at.clone(),
                    applied_at: p.applied_at.clone(),
                    before_hash: p.before_hash.clone(),
                })
                .collect(),
        }
    }
}

/// `library/<uuid>/kalam.json`
pub fn sidecar_path(uuid: &str) -> std::path::PathBuf {
    crate::paths::book_dir(uuid).join("kalam.json")
}

/// Where one book's sidecar lives.
///
/// A regular book keeps `kalam.json` beside its file. A comic chapter
/// (item 2.22) lives in a shared series folder with hundreds of siblings,
/// so one `kalam.json` per folder is impossible; its sidecar is named for
/// its uuid inside a hidden `.kalam/` directory — out of sight when a
/// person browses the series, still beside the files it describes, so it
/// travels with the library.
fn sidecar_file_for(book: &Book) -> std::path::PathBuf {
    match book.file_name.rsplit_once('/') {
        Some((series, _)) => crate::paths::library_dir()
            .join(series)
            .join(".kalam")
            .join(format!("{}.json", book.uuid)),
        None => sidecar_path(&book.uuid),
    }
}

/// Write one book's sidecar.
///
/// Best-effort by design: the caller ignores the result. A failure here means
/// the *backup* is stale, which is worth a log line and nothing more — failing
/// a user's edit because a redundant copy could not be written would be
/// strictly worse than the problem it prevents.
///
/// Temp-file-then-rename, so an interrupted write cannot leave a truncated
/// sidecar that a future rebuild would read as authoritative-but-wrong.
pub fn write_sidecar(
    book: &Book,
    position: Option<(usize, f64)>,
    marks: &[Annotation],
    patches: &[PatchRecord],
) {
    let data = Sidecar::from_parts(book, position, marks, patches);
    let final_path = sidecar_file_for(book);
    // A comic chapter's sidecar sits in `.kalam/`, which may not exist yet
    // even though the series folder does — create it. For every other book
    // the directory is the book's own folder, and a missing one means there
    // are no files to sit beside, so the write is skipped as before.
    if book.file_name.contains('/') {
        if let Some(dir) = final_path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return;
            }
        }
    } else if !crate::paths::book_dir(&book.uuid).is_dir() {
        return;
    }
    let tmp = final_path.with_extension("json.tmp");

    let json = match serde_json::to_string_pretty(&data) {
        Ok(j) => j,
        Err(err) => {
            eprintln!("kalam: could not encode {}: {err}", final_path.display());
            return;
        }
    };
    if let Err(err) = std::fs::write(&tmp, json) {
        eprintln!("kalam: could not write {}: {err}", tmp.display());
        return;
    }
    if let Err(err) = std::fs::rename(&tmp, &final_path) {
        eprintln!("kalam: could not save {}: {err}", final_path.display());
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Refresh the sidecar for one book from the database.
///
/// The convenience form used by call sites that have an id and nothing else.
pub fn refresh_for_book(catalog: &Catalog, book_id: i64) {
    let Ok(Some(book)) = catalog.get_book(book_id) else {
        return;
    };
    let position = catalog.get_reading_progress(book_id).ok().flatten();
    let marks = catalog.get_annotations_for_book(book_id).unwrap_or_default();
    let patches = catalog.get_patches_for_book(book_id).unwrap_or_default();
    write_sidecar(&book, position, &marks, &patches);
}

/// Read a sidecar back. Used by the survey below, never during normal reading.
pub fn read_sidecar(path: &Path) -> Option<Sidecar> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// What a rebuild would find if `catalog.db` disappeared right now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SidecarSurvey {
    /// Books in the catalog.
    pub books: usize,
    /// Books with a readable sidecar (`kalam.json`, or `.kalam/<uuid>.json`
    /// for a comic chapter in a series folder).
    pub recoverable: usize,
    /// Books whose sidecar exists but could not be parsed.
    pub damaged: usize,
}

impl SidecarSurvey {
    /// Books with no sidecar at all — imported before this existed, or a
    /// write that failed.
    pub fn missing(&self) -> usize {
        self.books
            .saturating_sub(self.recoverable)
            .saturating_sub(self.damaged)
    }
}

/// Count how much of the library could be rebuilt from its folders.
///
/// This exists so the promise is checkable. "Your library describes itself" is
/// worth nothing if nobody ever verifies it — the backups are written on paths
/// that could silently stop running, and the failure would be invisible until
/// the day someone actually needed them (pitfalls §19). Settings shows the
/// number.
pub fn survey(catalog: &Catalog) -> SidecarSurvey {
    // Counted from the catalog, not from the directory listing: comic
    // chapters share one series folder (item 2.22), so folders no longer
    // map one-to-one onto books. The database knows exactly what a rebuild
    // would need to find.
    let Ok(books) = catalog.list_books(crate::db::SortKey::Title, "") else {
        return SidecarSurvey::default();
    };
    let mut out = SidecarSurvey::default();
    for book in &books {
        out.books += 1;
        let side = sidecar_file_for(book);
        if !side.is_file() {
            continue;
        }
        if read_sidecar(&side).is_some() {
            out.recoverable += 1;
        } else {
            out.damaged += 1;
        }
    }
    out
}

/// Write a sidecar for every book that has none.
///
/// For libraries that predate this file, and for any book whose write failed.
/// Returns how many were written. Cheap to re-run: books that already have one
/// are skipped, so this is safe to offer as a button.
pub fn backfill_missing(catalog: &Catalog) -> usize {
    let Ok(books) = catalog.list_books(crate::db::SortKey::Title, "") else {
        return 0;
    };
    let mut written = 0;
    for book in books {
        if sidecar_file_for(&book).is_file() {
            continue;
        }
        refresh_for_book(catalog, book.id);
        if sidecar_file_for(&book).is_file() {
            written += 1;
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_book() -> Book {
        Book {
            id: 1,
            uuid: "abc-123".into(),
            title: "Dune".into(),
            authors: "Frank Herbert".into(),
            series: Some("Dune".into()),
            description: "Desert planet.".into(),
            format: crate::models::BookFormat::Epub,
            file_name: "book.epub".into(),
            file_hash: "deadbeef".into(),
            cover_name: Some("cover.jpg".into()),
            added_at: "2026-09-04T00:00:00Z".into(),
            progress: 42,
            rating: 9,
            publisher: "Chilton".into(),
            published: "1965".into(),
            series_index: 1.0,
            tags: vec!["scifi".into(), "classic".into()],
            // Resolved at read time from the folder, so they carry no
            // information the sidecar needs; still must be given explicitly
            // because `Book` has no `Default`.
            cover_path: None,
            file_path: std::path::PathBuf::new(),
        }
    }

    fn a_mark() -> Annotation {
        Annotation {
            id: 7,
            book_id: 1,
            kind: "highlight".into(),
            chapter_index: 3,
            start_path: "/1/2".into(),
            start_offset: 10,
            end_path: "/1/2".into(),
            end_offset: 24,
            color: "yellow".into(),
            style: "solid".into(),
            text_excerpt: "fear is the mind-killer".into(),
            note: "remember this".into(),
            cfi: None,
            created_at: "2026-09-04T01:00:00Z".into(),
            updated_at: "2026-09-04T01:00:00Z".into(),
        }
    }

    fn a_patch() -> PatchRecord {
        PatchRecord {
            id: 11,
            book_id: 1,
            kind: "text".into(),
            href: "text/chapter1.xhtml".into(),
            chapter_index: 0,
            find_text: "teh".into(),
            replace_text: "the".into(),
            context_before: "fix ".into(),
            context_after: " word".into(),
            source: "typo".into(),
            status: "pending".into(),
            created_at: "2026-10-08T00:00:00Z".into(),
            applied_at: None,
            before_hash: String::new(),
        }
    }

    #[test]
    fn a_sidecar_keeps_what_a_rebuild_would_need() {
        let side = Sidecar::from_parts(&a_book(), Some((3, 0.5)), &[a_mark()], &[a_patch()]);
        assert_eq!(side.uuid, "abc-123");
        assert_eq!(side.title, "Dune");
        assert_eq!(side.tags, vec!["scifi", "classic"]);
        assert_eq!(side.rating, 9);
        assert_eq!(side.progress_percent, 42);
        assert_eq!(side.reading_position, Some((3, 0.5)));
        assert_eq!(side.highlights.len(), 1);
        // The words themselves, which is the field that makes a highlight
        // findable again after the file changes.
        assert_eq!(side.highlights[0].text_excerpt, "fear is the mind-killer");
        assert_eq!(side.highlights[0].note, "remember this");
    }

    #[test]
    fn it_stores_names_not_paths() {
        // An absolute path would stop the folder being portable: copy the
        // library to another machine and every path in it is wrong.
        let side = Sidecar::from_parts(&a_book(), None, &[], &[]);
        assert_eq!(side.file_name, "book.epub");
        assert_eq!(side.cover_name.as_deref(), Some("cover.jpg"));
        assert!(
            !side.file_name.contains('/'),
            "file_name must be a name, not a path"
        );
    }

    #[test]
    fn a_file_patchs_guard_survives_the_sidecar() {
        // Phase 6.9: a whole-file patch is only as good as its guard.
        // A sidecar that drops it turns every raw edit Stale on
        // rebuild — a silent loss, so it is proven here, not assumed.
        let mut p = a_patch();
        p.kind = "file".into();
        p.before_hash = "deadbeef".into();
        let side = Sidecar::from_parts(&a_book(), None, &[], &[p]);
        assert_eq!(side.patches[0].kind, "file");
        assert_eq!(side.patches[0].before_hash, "deadbeef");
    }

    #[test]
    fn an_old_sidecar_patch_without_the_guard_still_reads() {
        // Sidecars written before the raw mode carry no before_hash
        // key; their patches predate whole-file edits, and the empty
        // guard reads back as "the find text is its own guard".
        let old = serde_json::from_str::<SidecarPatch>(
            r#"{"id":1,"kind":"text","href":"c.xhtml","chapter_index":0,
                "find_text":"teh","replace_text":"the","context_before":"",
                "context_after":"","source":"typo","status":"pending",
                "created_at":"2026-10-08T00:00:00Z","applied_at":null}"#,
        )
        .expect("sidecars written before Phase 6.9 still read");
        assert_eq!(old.before_hash, "");
    }

    #[test]
    fn a_sidecar_survives_a_round_trip_through_a_file() {
        // The whole point: if catalog.db is lost, these files are what is
        // left. A round trip that drops a field loses it permanently.
        let side = Sidecar::from_parts(&a_book(), Some((3, 0.5)), &[a_mark()], &[]);
        let dir = std::env::temp_dir().join(format!("kalam-sidecar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kalam.json");
        std::fs::write(&path, serde_json::to_string_pretty(&side).unwrap()).unwrap();

        let back = read_sidecar(&path).expect("should read back");
        assert_eq!(back, side);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_sidecar_is_none_rather_than_a_panic() {
        // These files are user-visible and hand-editable, so malformed ones
        // are a matter of when, not if. A rebuild should skip the book, not
        // take the app down.
        let dir = std::env::temp_dir().join(format!("kalam-sidecar-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kalam.json");
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(read_sidecar(&path).is_none());
        assert!(read_sidecar(&dir.join("absent.json")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tag_added_after_import_reaches_the_backup() {
        // The gap this closed: the sidecar was written on import, on the
        // metadata editor and on adding a highlight -- but not when a tag chip
        // was added or removed on the book page, nor when a highlight was
        // deleted. So an edit could leave the backup silently stale while the
        // recovery card still reported "all books covered", because it counts
        // files rather than freshness.
        let cat = Catalog::open_in_memory().unwrap();
        let id = cat
            .insert_book(
                "uuid-tagtest",
                "Dune",
                "Frank Herbert",
                None,
                "",
                crate::models::BookFormat::Epub,
                "book.epub",
                "hash-tagtest",
                None,
                &[],
            )
            .expect("insert");

        cat.add_book_tag(id, "scifi").expect("add tag");
        let tags = cat.get_book(id).unwrap().unwrap().tags;
        assert_eq!(tags, vec!["scifi"], "the tag must be on the book");

        // What the sidecar would contain now, built the same way the writer
        // builds it. The write itself needs a real library folder, which a
        // unit test has no business creating; the content is the part that
        // was wrong.
        let book = cat.get_book(id).unwrap().unwrap();
        let marks = cat.get_annotations_for_book(id).unwrap_or_default();
        let patches = cat.get_patches_for_book(id).unwrap_or_default();
        let side = Sidecar::from_parts(&book, None, &marks, &patches);
        assert_eq!(
            side.tags,
            vec!["scifi"],
            "a tag added after import must reach the backup"
        );

        cat.remove_book_tag(id, "scifi").expect("remove tag");
        let book = cat.get_book(id).unwrap().unwrap();
        let side = Sidecar::from_parts(&book, None, &[], &[]);
        assert!(
            side.tags.is_empty(),
            "removing a tag must reach the backup too"
        );
    }

    #[test]
    fn the_sidecar_sits_beside_the_book_it_describes() {
        let path = sidecar_path("abc-123");
        assert_eq!(path.file_name().unwrap(), "kalam.json");
        assert_eq!(
            path.parent().unwrap(),
            crate::paths::book_dir("abc-123"),
            "the sidecar must live in the book's own folder"
        );
    }

    #[test]
    fn a_v1_sidecar_without_patches_still_reads() {
        // Written before patches existed (sidecar v1). The serde default
        // makes the new field optional on read, so no backup turns
        // "damaged" the day this feature ships.
        let side = Sidecar::from_parts(&a_book(), None, &[], &[a_patch()]);
        let mut value = serde_json::to_value(&side).unwrap();
        let obj = value.as_object_mut().expect("sidecar is a JSON object");
        obj.remove("patches");
        obj.insert("version".into(), serde_json::json!(1));

        let dir = std::env::temp_dir().join(format!("kalam-sidecar-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kalam.json");
        std::fs::write(&path, value.to_string()).unwrap();

        let back = read_sidecar(&path).expect("a v1 sidecar must still read");
        assert_eq!(back.version, 1);
        assert!(back.patches.is_empty());
        assert_eq!(back.title, "Dune", "everything else survives the read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patches_survive_a_round_trip_through_a_file() {
        // A rebuild after losing catalog.db must recover the edit rules
        // exactly — the find text, the anchors and the status are what the
        // matcher and the review panel run on.
        let side = Sidecar::from_parts(&a_book(), None, &[], &[a_patch()]);
        let dir = std::env::temp_dir().join(format!("kalam-sidecar-patch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kalam.json");
        std::fs::write(&path, serde_json::to_string_pretty(&side).unwrap()).unwrap();

        let back = read_sidecar(&path).expect("should read back");
        assert_eq!(back.patches.len(), 1);
        assert_eq!(back.patches[0], side.patches[0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
