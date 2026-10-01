//! A0 step 3 — persistent cover thumbnails.
//!
//! The library grid was decoding the *full* cover (often 1000×1500+) on the UI
//! thread for every card, on every rebuild. The in-memory `COVER_CACHE` died at
//! relaunch, so a fresh launch re-decoded everything again.
//!
//! This module generates a small thumbnail at import time (a pure, headless,
//! CI-testable function) into `cache/thumbs/<uuid>.png`. The grid then decodes
//! that tiny PNG instead of the full cover, and because it survives relaunch,
//! the first grid render after startup is cheap too.
//!
//! gdk-pixbuf in this toolchain can decode but not encode PNG, so we use the
//! `image` crate (pure Rust, no system deps) for the resize + PNG encode.

use std::path::{Path, PathBuf};

/// Thumbnail size. 2× the grid card (128×204) to stay crisp on HiDPI, while
/// still being tiny enough to decode and cache cheaply.
pub const THUMB_W: u32 = 256;
pub const THUMB_H: u32 = 408;

/// Generate a thumbnail for `source` (any image `image` can read) at
/// `THUMB_W`×`THUMB_H`, writing a PNG to `dest`.
///
/// Returns `false` (never panics) when the source is missing, unreadable, or
/// not a supported image — the caller then simply falls back to decoding the
/// full cover. Purely best-effort: a missing thumbnail is not an error.
pub fn generate_thumbnail(source: &Path, dest: &Path) -> bool {
    if !source.is_file() {
        return false;
    }
    let img = image::ImageReader::open(source)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.decode().ok())
        .or_else(|| image::open(source).ok());
    let Some(img) = img else {
        return false;
    };
    let resized = img.resize_exact(THUMB_W, THUMB_H, image::imageops::FilterType::Triangle);
    if let Some(parent) = dest.parent() {
        if !parent.exists() && std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    resized.save(dest).is_ok()
}

/// Remove a book's thumbnail (used on book deletion so the cache cannot grow).
pub fn remove_thumbnail(uuid: &str) {
    let _ = std::fs::remove_file(crate::paths::thumbnail_path(uuid));
}

/// Generate the thumbnail for one book if it is missing. Returns `true` when
/// it produced the file (or it already existed — i.e. the book is covered).
/// `thumb_of` computes the destination path so tests can point somewhere inert.
fn backfill_one(uuid: &str, cover: &Path, thumb_of: &dyn Fn(&str) -> PathBuf) -> bool {
    let thumb = thumb_of(uuid);
    if thumb.is_file() {
        return true;
    }
    generate_thumbnail(cover, &thumb)
}

/// Backfill thumbnails for every book that has a cover but no thumbnail yet.
///
/// Runs off the UI thread at startup so a library imported *before* this change
/// gains thumbnails without re-importing, and only missing files are generated
/// (so it is cheap after the first pass). Best-effort: a failure for one book
/// is skipped and the grid falls back to the full cover for that one.
///
/// Takes a [`crate::tasks::Reporter`] so a big first pass can be abandoned when
/// the window closes. Without that, quitting during the very first launch of a
/// large library left a thread decoding covers with nothing left to show them
/// to, and the process lingered until it finished.
/// Returns how many thumbnails were actually generated.
///
/// Skips the whole pass when the library has not changed since the last
/// complete run — see [`should_skip`] for why that check is a book count and
/// not a "done" flag.
pub fn backfill_missing(cat: &crate::db::Catalog, reporter: &crate::tasks::Reporter) -> usize {
    // Only uuid + cover_name: this used to call `list_books`, which builds a
    // full `Book` for every row and runs a second query to join tags, none of
    // which is read here.
    let Ok(books) = cat.books_with_covers() else {
        return 0;
    };

    if should_skip(cat.get_pref(BACKFILL_DONE_PREF).as_deref(), books.len()) {
        return 0;
    }

    let total = books.len();
    let mut generated = 0;
    let mut all_present = true;
    for (i, (uuid, cover_name)) in books.iter().enumerate() {
        if reporter.cancelled() {
            // Deliberately does not record the marker: a cancelled pass has
            // not verified the rest of the library, and claiming otherwise
            // would leave those books without thumbnails for good.
            return generated;
        }
        // Item 2.22: a comic chapter's cover is stored library-relative
        // ("Series/covers/0010.jpg"); the same stored name that points a
        // book at its cover points the backfill at it.
        let cover = crate::paths::resolve_library_file(uuid, cover_name);
        // `backfill_one` returns true when the thumbnail is *present*, which
        // includes "was already there" — so count only the ones that were
        // actually missing beforehand, or the number is just the library size.
        let existed = crate::paths::thumbnail_path(uuid).is_file();
        if backfill_one(uuid, &cover, &crate::paths::thumbnail_path) {
            if !existed {
                generated += 1;
            }
        } else {
            // A cover that cannot be thumbnailed (missing file, unsupported
            // format) must not count as covered, or the marker would promise
            // a complete library that is not one.
            all_present = false;
        }
        reporter.step(i + 1, total, uuid.clone());
    }

    if all_present {
        cat.set_pref(BACKFILL_DONE_PREF, &total.to_string());
    }
    generated
}

/// Pref holding the book count at the last *complete* backfill.
const BACKFILL_DONE_PREF: &str = "thumbs.backfill_done_count";

/// Forget the skip marker, forcing a full pass on the next launch.
///
/// Called when a book is deleted. The marker is a book *count*, so a delete
/// plus an import nets to zero and would wrongly skip the new book's
/// thumbnail. An unnecessary pass costs one query and a few stat calls; a
/// missed one costs a book its thumbnail for good.
pub fn invalidate_backfill_marker(cat: &crate::db::Catalog) {
    cat.set_pref(BACKFILL_DONE_PREF, "");
}

/// Whether the startup backfill can be skipped entirely.
///
/// The problem being solved: on a settled library every launch listed all the
/// books and ran one `is_file()` per book to confirm thumbnails that were
/// already there. Harmless at 139 books; at 2,000 it is a query plus 2,000
/// stat calls on every start, to do nothing.
///
/// A plain "done" flag would be wrong, because importing a book must trigger a
/// new pass. Storing the *count* at the last complete run gives that for free:
/// any import changes the count and the pass runs again.
///
/// Deliberately conservative in both directions:
/// * a differing count (more **or** fewer books) re-runs, because a deletion
///   could have been a delete-and-reimport that netted to zero,
/// * an unparseable or missing pref re-runs.
///
/// The failure mode this cannot catch is a count-preserving change — deleting
/// one book and importing another between launches, with the thumbnail cache
/// wiped. The cost of missing it is one book decoding its full cover instead
/// of a thumbnail, which is the pre-A0-step-3 behaviour, not a bug.
fn should_skip(recorded: Option<&str>, current: usize) -> bool {
    matches!(recorded.and_then(|v| v.parse::<usize>().ok()), Some(n) if n == current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A unique scratch dir under the OS temp dir, cleaned up on drop.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "kalam-thumbs-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&p).expect("scratch dir");
            Scratch(p)
        }
        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_solid_png(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
        let buf: Vec<u8> = (0..(w * h)).flat_map(|_| rgb).collect();
        image::save_buffer(path, &buf, w, h, image::ExtendedColorType::Rgb8).expect("write png");
    }

    #[test]
    fn generates_an_exact_size_png() {
        let dir = Scratch::new();
        let src = dir.join("cover.png");
        let dst = dir.join("thumb.png");
        // A cover larger than the thumbnail, e.g. 600×900.
        write_solid_png(&src, 600, 900, [30, 60, 120]);

        assert!(generate_thumbnail(&src, &dst));
        assert!(dst.is_file());

        let reopened = image::open(&dst).expect("reopen thumb");
        assert_eq!((reopened.width(), reopened.height()), (THUMB_W, THUMB_H));
    }

    #[test]
    fn missing_source_is_not_an_error() {
        let dir = Scratch::new();
        let dst = dir.join("thumb.png");
        assert!(!generate_thumbnail(&dir.join("nope.png"), &dst));
        assert!(!dst.exists());
    }

    #[test]
    fn non_image_source_returns_false() {
        use std::io::Write;
        let dir = Scratch::new();
        let src = dir.join("cover.txt");
        let mut f = std::fs::File::create(&src).unwrap();
        f.write_all(b"this is not an image").unwrap();
        let dst = dir.join("thumb.png");
        assert!(!generate_thumbnail(&src, &dst));
        assert!(!dst.exists());
    }

    #[test]
    fn backfill_generates_missing_and_skips_present() {
        let dir = Scratch::new();
        let cover = dir.join("cover.png");
        write_solid_png(&cover, 600, 900, [20, 40, 60]);
        let thumb_of = |uuid: &str| dir.join(&format!("{uuid}.png"));

        // First call produces the thumbnail.
        assert!(backfill_one("abc", &cover, &thumb_of));
        assert!(thumb_of("abc").is_file());

        // Second call sees the file is present and does no work.
        assert!(backfill_one("abc", &cover, &thumb_of));
    }

    #[test]
    fn backfill_missing_source_is_a_noop() {
        let dir = Scratch::new();
        let thumb_of = |uuid: &str| dir.join(&format!("{uuid}.png"));
        // A missing source is not created, but it is not an error either.
        let got = backfill_one("nope", &dir.join("absent.png"), &thumb_of);
        assert!(!got);
        assert!(!thumb_of("nope").exists());
    }

    // `should_skip` is a pure function precisely so the interesting decision
    // can be tested without a database, a display, or a filesystem.

    #[test]
    fn an_unchanged_library_is_skipped() {
        // The whole point: on a settled library the startup pass should do
        // nothing at all, rather than stat every book to confirm what it
        // already knows.
        assert!(should_skip(Some("139"), 139));
    }

    #[test]
    fn importing_or_deleting_forces_another_pass() {
        // More books than last time: the new one needs a thumbnail.
        assert!(!should_skip(Some("139"), 140), "an import must re-run");
        // Fewer: a deletion could have been half of a delete-and-reimport, so
        // being conservative is right. An unnecessary pass is cheap.
        assert!(!should_skip(Some("139"), 138), "a deletion must re-run");
    }

    #[test]
    fn a_missing_or_corrupt_marker_re_runs() {
        // Never trust an absent or unreadable marker: the cost of an extra
        // pass is a query and some stat calls, the cost of wrongly skipping is
        // a book with no thumbnail for good.
        assert!(!should_skip(None, 139), "first ever launch");
        assert!(!should_skip(Some(""), 139), "cleared by a deletion");
        assert!(!should_skip(Some("not a number"), 139), "corrupt value");
        assert!(
            !should_skip(Some("-1"), 139),
            "negative cannot parse to usize"
        );
    }

    #[test]
    fn an_empty_library_is_skipped_once_recorded() {
        // Zero is a legitimate count, not a stand-in for "unknown" -- an empty
        // library should settle like any other. Kept as a test because it is
        // exactly the case a `> 0` guard would break.
        assert!(should_skip(Some("0"), 0));
        assert!(!should_skip(None, 0), "but not before the first pass");
    }

    #[test]
    fn mismatched_extension_is_thumbnailed_successfully() {
        let dir = Scratch::new();
        let src = dir.join("cover.jpg"); // Named .jpg, but actual content is PNG
        let dst = dir.join("thumb.png");
        let img = image::RgbaImage::new(10, 10);
        img.save_with_format(&src, image::ImageFormat::Png).unwrap();
        assert!(generate_thumbnail(&src, &dst));
        assert!(dst.is_file());
    }
}
