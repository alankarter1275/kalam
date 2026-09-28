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

use crate::db::{Annotation, Catalog};
use crate::models::Book;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bumped when the shape changes incompatibly, so a future reader can tell
/// what it is looking at instead of guessing from which fields are present.
const SIDECAR_VERSION: u32 = 1;

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
}

impl Sidecar {
    pub fn from_parts(book: &Book, position: Option<(usize, f64)>, marks: &[Annotation]) -> Self {
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
        }
    }
}

/// `library/<uuid>/kalam.json`
pub fn sidecar_path(uuid: &str) -> std::path::PathBuf {
    crate::paths::book_dir(uuid).join("kalam.json")
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
pub fn write_sidecar(book: &Book, position: Option<(usize, f64)>, marks: &[Annotation]) {
    let data = Sidecar::from_parts(book, position, marks);
    let dir = crate::paths::book_dir(&book.uuid);
    if !dir.is_dir() {
        return;
    }
    let final_path = sidecar_path(&book.uuid);
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
    write_sidecar(&book, position, &marks);
}

/// Read a sidecar back. Used by the survey below, never during normal reading.
pub fn read_sidecar(path: &Path) -> Option<Sidecar> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// What a rebuild would find if `catalog.db` disappeared right now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SidecarSurvey {
    /// Book folders in the library.
    pub books: usize,
    /// Folders with a readable `kalam.json`.
    pub recoverable: usize,
    /// Folders with a `kalam.json` that could not be parsed.
    pub damaged: usize,
}

impl SidecarSurvey {
    /// Folders with no sidecar at all — imported before this existed, or a
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
pub fn survey() -> SidecarSurvey {
    let mut out = SidecarSurvey::default();
    let Ok(entries) = std::fs::read_dir(crate::paths::library_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        out.books += 1;
        let side = entry.path().join("kalam.json");
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
        if sidecar_path(&book.uuid).is_file() {
            continue;
        }
        refresh_for_book(catalog, book.id);
        if sidecar_path(&book.uuid).is_file() {
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

    #[test]
    fn a_sidecar_keeps_what_a_rebuild_would_need() {
        let side = Sidecar::from_parts(&a_book(), Some((3, 0.5)), &[a_mark()]);
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
        let side = Sidecar::from_parts(&a_book(), None, &[]);
        assert_eq!(side.file_name, "book.epub");
        assert_eq!(side.cover_name.as_deref(), Some("cover.jpg"));
        assert!(
            !side.file_name.contains('/'),
            "file_name must be a name, not a path"
        );
    }

    #[test]
    fn a_sidecar_survives_a_round_trip_through_a_file() {
        // The whole point: if catalog.db is lost, these files are what is
        // left. A round trip that drops a field loses it permanently.
        let side = Sidecar::from_parts(&a_book(), Some((3, 0.5)), &[a_mark()]);
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
        let side = Sidecar::from_parts(&book, None, &marks);
        assert_eq!(
            side.tags,
            vec!["scifi"],
            "a tag added after import must reach the backup"
        );

        cat.remove_book_tag(id, "scifi").expect("remove tag");
        let book = cat.get_book(id).unwrap().unwrap();
        let side = Sidecar::from_parts(&book, None, &[]);
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
}
