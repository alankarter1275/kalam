//! Book folders with names a human can read.
//!
//! Owner decision, 2026-09-29: a book is stored in
//! `library/Author - Title <short-id>/` instead of `library/<uuid>/`. The
//! database still knows books by uuid — the folder name is a label for the
//! person browsing their library with a file manager, and nothing in the
//! app parses it.
//!
//! Two things keep folders and metadata in step:
//!
//! - **The startup pass** ([`align_book_folders`]) renames every folder
//!   whose book's author or title has changed since the folder was last
//!   seen. It runs as background housekeeping after the first paint, and
//!   on a library that is already in line it is a no-op that costs one
//!   database query and one name comparison per book. It is also the
//!   one-time upgrade: an old library whose folders are bare uuids is
//!   renamed on the first launch after this change.
//! - **The edit hook** (`rename_book_folder_if_needed`, called from
//!   `update_book_metadata`) renames a folder right away when you edit its
//!   book's author or title, so the folder always matches what the app
//!   shows. Best-effort by design: a failed rename logs a warning and
//!   never fails the metadata edit.
//!
//! Renaming a folder cannot strand anything. No path is stored anywhere —
//! every file path is rebuilt from the uuid through `paths::book_dir` at
//! the moment it is needed. Covers, thumbnails and the OCR page cache are
//! keyed by uuid or by file content, not by folder name. And because a
//! rename keeps a file's contents and its modification time, even the OCR
//! cache fingerprint survives a rename untouched.

use crate::db::Catalog;
use crate::tasks::Reporter;
use std::path::Path;

/// Rename one book's folder to match its current author and title.
/// Returns `true` when a rename actually happened.
///
/// Skips quietly (returns `false`) when there is nothing to do: the folder
/// already has the right name, or the book's files are missing, or every
/// name this book could take is already occupied.
pub fn rename_book_folder_if_needed(uuid: &str, authors: &str, title: &str) -> bool {
    let lib = crate::paths::library_dir();
    let current = crate::paths::book_dir(uuid);
    rename_folder(&lib, &current, uuid, authors, title)
}

/// The rename itself, with the locations passed in so tests can run this
/// exact code against a scratch directory instead of the real library.
///
/// `current` is where the book's files live now (the caller resolves it
/// through `paths::book_dir`, which knows about renamed folders); `lib` is
/// the library root the new name is built under.
fn rename_folder(
    lib: &Path,
    current: &Path,
    uuid: &str,
    authors: &str,
    title: &str,
) -> bool {
    if !current.is_dir() {
        // No files on disk (a missing book, or a unit test that never
        // created them). Nothing to rename.
        return false;
    }
    let Some(current_name) = current.file_name().and_then(|n| n.to_str()) else {
        return false; // not valid UTF-8: certainly not one of our names
    };

    let desired = crate::paths::book_folder_name(authors, title, uuid);
    if current_name == desired {
        return false;
    }

    // The name we want is taken. The usual cause is a genuine clash: same
    // author, same title, and two uuids that agree on their first eight
    // characters. Fall back to the full uuid, which cannot clash with
    // another book.
    let target_name = if lib.join(&desired).exists() {
        let full = crate::paths::book_folder_name_full(authors, title, uuid);
        if current_name == full {
            // Already living under the clash fallback — this is exactly
            // where this book belongs. (Without this check the pass would
            // find its own name "taken" on every startup.)
            return false;
        }
        if lib.join(&full).exists() {
            log::warn!(
                "cannot rename book folder {current_name:?}: \
                 both {desired:?} and {full:?} are taken"
            );
            return false;
        }
        full
    } else {
        desired
    };

    match std::fs::rename(current, lib.join(&target_name)) {
        Ok(()) => {
            // Tell the path resolver immediately, so the rest of this
            // session finds the book in its new home.
            crate::paths::note_folder(uuid, &target_name);
            log::info!("renamed book folder {current_name:?} → {target_name:?}");
            true
        }
        Err(err) => {
            log::warn!("could not rename book folder {current_name:?}: {err}");
            false
        }
    }
}

/// Startup housekeeping: bring every book folder in line with its
/// metadata. Returns how many folders were renamed. Idempotent — folders
/// that already match cost one string comparison each.
pub fn align_book_folders(catalog: &Catalog, reporter: &Reporter) -> usize {
    let Ok(books) = catalog.list_books(crate::db::SortKey::Title, "") else {
        return 0;
    };
    let total = books.len();
    let mut renamed = 0;
    for (i, book) in books.iter().enumerate() {
        if reporter.cancelled() {
            // Renames already made stay made, and the pass is idempotent,
            // so stopping early loses nothing.
            break;
        }
        // Comics live in series folders (item 2.22), not per-book folders;
        // the comic pass owns them. Skipping here also keeps the two
        // background passes from racing over the same files.
        if matches!(
            book.format,
            crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
        ) {
            reporter.step(i + 1, total, book.uuid.clone());
            continue;
        }
        if rename_book_folder_if_needed(&book.uuid, &book.authors, &book.title) {
            renamed += 1;
        }
        reporter.step(i + 1, total, book.uuid.clone());
    }
    renamed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch library directory under the system temp dir. Each test
    /// gets its own, so parallel tests cannot see each other's folders.
    fn scratch_library(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kalam-folder-test-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Run the real rename logic against a scratch library. `uuid` is also
    /// the folder's current name, which is exactly the pre-upgrade shape.
    fn rename_scratch(lib: &Path, uuid: &str, authors: &str, title: &str) -> bool {
        let current = lib.join(uuid);
        rename_folder(lib, &current, uuid, authors, title)
    }

    #[test]
    fn folder_name_uses_author_title_and_short_id() {
        let name = crate::paths::book_folder_name(
            "Hayao Miyazaki",
            "Nausicaä of the Valley of the Wind",
            "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6",
        );
        assert!(name.starts_with("Hayao Miyazaki - Nausicaä"));
        assert!(name.ends_with(" 3f2ab91c"));
    }

    #[test]
    fn folder_name_drops_unknown_and_empty_authors() {
        // "Unknown" is what the importer writes for authorless PDFs and
        // comics; the owner chose a title-only folder for those.
        let with_unknown = crate::paths::book_folder_name(
            "Unknown",
            "Nausicaä",
            "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6",
        );
        let with_empty = crate::paths::book_folder_name(
            "",
            "Nausicaä",
            "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6",
        );
        assert!(with_unknown.starts_with("Nausicaä "), "got {with_unknown}");
        assert!(with_empty.starts_with("Nausicaä "), "got {with_empty}");
    }

    #[test]
    fn folder_name_survives_illegal_characters_and_long_titles() {
        let name = crate::paths::book_folder_name(
            "A/B: Writer",
            "Re: Zero \u{7f} Starting Life\tin Another World. A Very Long Subtitle",
            "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6",
        );
        assert!(!name.contains('/'), "slash must go: {name}");
        assert!(!name.contains('\u{7f}'), "control character must go: {name}");
        assert!(!name.contains('\t'), "tab becomes a single space: {name}");
        // 255 bytes is the usual file-system name limit; stay well under.
        assert!(name.len() < 160, "name too long at {} bytes: {name}", name.len());
        // The short id is never sacrificed to truncation.
        assert!(name.ends_with(" 3f2ab91c"), "got {name}");
    }

    #[test]
    fn folder_name_falls_back_to_untitled() {
        let name = crate::paths::book_folder_name(
            "Author",
            "",
            "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6",
        );
        assert!(name.starts_with("Author - Untitled "), "got {name}");
    }

    #[test]
    fn folder_name_keeps_bengali_intact() {
        let name = crate::paths::book_folder_name(
            "রবীন্দ্রনাথ ঠাকুর",
            "গোরা",
            "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6",
        );
        assert!(name.starts_with("রবীন্দ্রনাথ ঠাকুর - গোরা "), "got {name}");
        assert!(name.ends_with(" 3f2ab91c"));
    }

    #[test]
    fn renaming_moves_a_uuid_folder_to_a_readable_one() {
        let lib = scratch_library("rename");
        let uuid = "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6";
        std::fs::create_dir_all(lib.join(uuid)).expect("create old folder");
        std::fs::write(lib.join(uuid).join("book.epub"), b"book").expect("write book");

        assert!(rename_scratch(
            &lib,
            uuid,
            "Ursula K. Le Guin",
            "A Wizard of Earthsea"
        ));
        let expected =
            crate::paths::book_folder_name("Ursula K. Le Guin", "A Wizard of Earthsea", uuid);
        let new_path = lib.join(&expected);
        assert!(new_path.is_dir(), "expected {new_path:?}");
        // The book's files moved with the folder.
        assert!(new_path.join("book.epub").is_file());
        // The old uuid folder is gone, not left behind as a duplicate.
        assert!(!lib.join(uuid).exists());

        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn a_clash_falls_back_to_the_full_uuid_and_then_stops() {
        let lib = scratch_library("clash");
        // Two books: same author and title, uuids that agree on their
        // first eight characters. Both will want the same folder name.
        let uuid_a = "3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6";
        let uuid_b = "3f2ab91c-aaaa-bbbb-cccc-dddddddddddd";
        std::fs::create_dir_all(lib.join(uuid_a)).expect("create a");
        std::fs::create_dir_all(lib.join(uuid_b)).expect("create b");

        assert!(rename_scratch(&lib, uuid_a, "Author", "Same Title"));
        // The second must not clobber the first: it takes the full uuid.
        assert!(rename_scratch(&lib, uuid_b, "Author", "Same Title"));
        let short = lib.join(crate::paths::book_folder_name("Author", "Same Title", uuid_b));
        let full = lib.join(crate::paths::book_folder_name_full("Author", "Same Title", uuid_b));
        assert!(short.is_dir(), "first book keeps the short name");
        assert!(full.is_dir(), "second book falls back to the full uuid");
        assert!(full.to_string_lossy().ends_with(uuid_b));

        // A third run on an already-correct folder changes nothing, and a
        // book whose every name is taken is left alone rather than moved
        // somewhere wrong.
        assert!(!rename_scratch(&lib, uuid_b, "Author", "Same Title"));

        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn an_edited_title_renames_again_from_the_readable_name() {
        // First rename gives the folder a readable name. A later metadata
        // edit must find the book there — not at the old uuid path — and
        // rename it again. This mirrors what the edit hook does.
        let lib = scratch_library("edit");
        let uuid = "4a5b6c7d-77d0-4c2e-9a10-52f1b3d4e5f6";
        std::fs::create_dir_all(lib.join(uuid)).expect("create");

        assert!(rename_scratch(&lib, uuid, "Author", "First Title"));
        let after_first = crate::paths::book_folder_name("Author", "First Title", uuid);
        assert!(lib.join(&after_first).is_dir());

        // The second pass starts from the readable folder, the way
        // `book_dir` resolves it after `note_folder` has done its job.
        assert!(rename_folder(
            &lib,
            &lib.join(&after_first),
            uuid,
            "Author",
            "Second Title"
        ));
        let after_second = crate::paths::book_folder_name("Author", "Second Title", uuid);
        assert!(lib.join(&after_second).is_dir(), "renamed to {after_second}");
        assert!(!lib.join(&after_first).exists(), "old readable name is gone");

        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn missing_files_are_skipped_not_invented() {
        let lib = scratch_library("missing");
        // A uuid with no folder on disk: nothing to rename, no error, and
        // crucially no folder is created for it.
        assert!(!rename_scratch(
            &lib,
            "11111111-2222-3333-4444-555555555555",
            "A",
            "T"
        ));
        assert!(!lib.join("11111111-2222-3333-4444-555555555555").exists());

        let _ = std::fs::remove_dir_all(&lib);
    }
}
