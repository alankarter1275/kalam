//! A0 step 5 — preloaders.
//!
//! # Cover preloader
//!
//! The grid decodes a cover the first time a card needs one, on the UI thread.
//! Step 3 made each decode cheap (a 256×408 thumbnail instead of a 1000×1500+
//! cover), but cheap times four hundred is still a stall, and it lands at the
//! worst moment: the frame where the user just scrolled or switched pages.
//!
//! So decode on a worker instead. Cards go up with a placeholder immediately
//! and each image is swapped in as it becomes ready, which means building a
//! grid costs no decodes at all.
//!
//! # Why this is not just `tasks::spawn`
//!
//! Two reasons.
//!
//! **A texture cannot cross a thread.** `gdk::Texture` is a GObject and lives
//! on the main thread; `tasks::spawn` refuses to let a worker return one,
//! which is the seam doing its job. What *can* cross is the decoded pixel
//! buffer — a plain `Vec<u8>` — so the worker does the expensive part (read
//! the PNG, decode it, resize it) and the main thread does the cheap part
//! (wrap the bytes in a `MemoryTexture`).
//!
//! **The results arrive one at a time.** A preloader that reported only when
//! all twenty covers were done would be useless, so this uses
//! [`crate::tasks::spawn_stream`] and each cover is cached the moment it is
//! ready.
//!
//! # What it deliberately does not do
//!
//! It only ever replaces a **placeholder**. `book_row` refuses to overwrite a
//! texture that is already cached, so a preload can never change an image the
//! user is currently looking at — the failure mode that makes async image
//! loading feel glitchy.

use std::path::{Path, PathBuf};

/// `KALAM_NO_PRELOAD=1` turns the cover preloader off: cards decode
/// synchronously as they did before A0 step 5.
///
/// Same spirit as `KALAM_NO_WEBVIEW_POOL=1` — a one-variable way to A/B the
/// change against `KALAM_TIMING=1` in a single session, and an escape hatch if
/// deferred covers ever misbehave on a real machine.
pub fn preload_enabled() -> bool {
    preload_enabled_for(std::env::var_os("KALAM_NO_PRELOAD").as_deref())
}

/// The env-var rule as a pure function, so it is unit-testable without
/// mutating process-wide state mid-test-run.
fn preload_enabled_for(value: Option<&std::ffi::OsStr>) -> bool {
    match value {
        None => true,
        // Unset-but-present and an explicit "0" both mean "leave it on", so a
        // stray `KALAM_NO_PRELOAD=` in a shell profile cannot silently disable
        // it.
        Some(v) => v.is_empty() || v == "0",
    }
}

/// How many covers to decode in the first batch — roughly the first four rows
/// of a six-column grid, i.e. what the user can actually see.
///
/// This is a *batch* size, not a budget. [`warm_covers`] keeps going after the
/// first batch until the list is exhausted; the split exists so the visible
/// rows are decoded and swapped in first, rather than the user watching a
/// whole library decode in book order before the top of the screen fills.
pub const PRELOAD_AHEAD: usize = 24;

/// A decoded cover on its way back to the main thread.
///
/// Deliberately plain data: `Vec<u8>` crosses threads, `gdk::Texture` does not.
pub struct DecodedCover {
    /// The *original* cover path — the cache is keyed on it, so that
    /// invalidation on a cover change keeps working.
    pub cover: PathBuf,
    pub width: i32,
    pub height: i32,
    /// Tightly packed RGBA, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// Decode one cover to raw RGBA at `w`×`h`.
///
/// Headless and dependency-free — no GTK, no GDK — which is what makes it
/// testable in CI and legal to call from a worker thread. Returns `None`
/// rather than an error for anything unreadable: a missing cover is a
/// placeholder, not a failure worth reporting.
pub fn decode_rgba(path: &Path, w: i32, h: i32) -> Option<DecodedCover> {
    if w <= 0 || h <= 0 || !path.is_file() {
        return None;
    }
    let img = image::ImageReader::open(path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.decode().ok())
        .or_else(|| image::open(path).ok())?;
    let resized = img.resize_exact(w as u32, h as u32, image::imageops::FilterType::Triangle);
    Some(DecodedCover {
        cover: path.to_path_buf(),
        width: w,
        height: h,
        rgba: resized.to_rgba8().into_raw(),
    })
}

/// Which file to decode for a cover slot: the thumbnail when it exists and is
/// big enough, else the original.
///
/// Mirrors the same choice `book_row::scaled_cover_picture` makes, so the
/// preloader and the on-demand path always decode the same source and the
/// warmed entry is a real hit rather than a near miss.
fn source_for(cover: &Path, w: i32, h: i32) -> PathBuf {
    if w <= crate::thumbs::THUMB_W as i32 && h <= crate::thumbs::THUMB_H as i32 {
        if let Some(thumb) = crate::paths::thumbnail_for_cover(cover) {
            if thumb.is_file() {
                return thumb;
            }
        }
    }
    cover.to_path_buf()
}

/// Decode the covers in `covers` on a worker thread and warm the texture cache
/// as each one lands.
///
/// Call this from the main thread, after building any page whose cards use
/// `cover_widget_deferred` — those cards are showing placeholders and this is
/// the only thing that fills them.
///
/// Order matters: the list is decoded front to back, so pass it nearest-first
/// (which is what [`ahead_of`] returns).
pub fn warm_covers(covers: Vec<PathBuf>, w: i32, h: i32) {
    if covers.is_empty() || !preload_enabled() {
        return;
    }
    crate::tasks::spawn_stream(
        move |reporter, emit| {
            for (i, cover) in covers.into_iter().enumerate() {
                // Cheap to check and worth checking: closing the page should
                // not leave a thread decoding covers nobody will see.
                if reporter.cancelled() {
                    return;
                }
                // After the visible batch, yield briefly between covers. The
                // decode itself is off the UI thread, but each finished cover
                // swaps a widget *on* it, and a few hundred of those back to
                // back is its own stutter. Off-screen covers have no deadline,
                // so spending a little longer on them costs nothing visible.
                if i >= PRELOAD_AHEAD {
                    std::thread::sleep(std::time::Duration::from_millis(4));
                }
                let src = source_for(&cover, w, h);
                let Some(mut decoded) = decode_rgba(&src, w, h) else {
                    continue;
                };
                // Report the *cover* path even when a thumbnail was decoded,
                // so the cache key matches what the grid will ask for.
                decoded.cover = cover;
                // A closed channel means the UI is gone; stop rather than
                // decode the rest into nothing.
                if !emit.send(decoded) {
                    return;
                }
            }
        },
        |decoded| {
            crate::widgets::book_row::cache_decoded_cover(&decoded);
        },
    );
}

/// Warm every cover in `books`, nearest-first from `visible_from`.
///
/// The one call a page needs after building cards with
/// `cover_widget_deferred`. Wraps [`ahead_of`] + [`warm_covers`] so the two
/// cannot be separated by accident — Home originally built deferred cards and
/// never warmed them, which left its covers blank for the life of the page.
pub fn warm_books(books: &[crate::models::Book], visible_from: usize, w: i32, h: i32) {
    let queued = ahead_of(books, visible_from, w, h);
    crate::timing::note("covers_queued", queued.len());
    warm_covers(queued, w, h);
}

/// The covers worth preloading, given what is already on screen.
///
/// Skips books with no cover and anything already cached, so a second call
/// after a small scroll costs almost nothing.
///
/// Pure and headless apart from the cache probe, which is why the interesting
/// half — the windowing — is tested below.
pub fn ahead_of(
    books: &[crate::models::Book],
    visible_from: usize,
    w: i32,
    h: i32,
) -> Vec<PathBuf> {
    let start = visible_from.min(books.len());
    // Nearest-first, but *all* of them: every card on the page is showing a
    // placeholder, so anything left out of this list would keep showing one
    // for ever. `warm_covers` decodes the first `PRELOAD_AHEAD` as one batch
    // and the remainder after, so the visible rows still come first.
    //
    // Deduplicated because a page can show the same book twice -- Home lists
    // one in both "continue reading" and "recently added" -- and decoding a
    // cover a second time is pure waste.
    let mut seen = std::collections::HashSet::new();
    books
        .iter()
        .skip(start)
        .chain(books.iter().take(start))
        .filter_map(|b| b.cover_path.clone())
        .filter(|p| !crate::widgets::book_row::is_cover_cached(p, w, h))
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch dir, cleaned up on drop. Same shape as `thumbs.rs`:
    /// tests run in parallel threads of one process, so the pid alone is not
    /// unique enough.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "kalam-preload-{}-{}",
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

    /// `Book` has no `Default`, so spell one out once.
    fn book_with_cover(id: i64, cover: Option<PathBuf>) -> crate::models::Book {
        crate::models::Book {
            id,
            uuid: String::new(),
            title: String::new(),
            authors: String::new(),
            series: None,
            description: String::new(),
            format: crate::models::BookFormat::Epub,
            file_name: String::new(),
            file_hash: String::new(),
            cover_name: None,
            added_at: String::new(),
            progress: 0,
            rating: 0,
            publisher: String::new(),
            published: String::new(),
            series_index: 0.0,
            tags: Vec::new(),
            cover_path: cover,
            file_path: PathBuf::new(),
        }
    }

    /// Same shape as `thumbs.rs`'s test helper — `save_buffer` is the API
    /// known to work under this crate's cut-down `image` features.
    fn write_png(path: &Path, w: u32, h: u32) {
        let buf = vec![90u8; (w * h * 3) as usize];
        image::save_buffer(path, &buf, w, h, image::ExtendedColorType::Rgb8).expect("write png");
    }

    #[test]
    fn the_preloader_is_on_unless_explicitly_switched_off() {
        use std::ffi::OsStr;
        assert!(preload_enabled_for(None), "on by default");
        assert!(
            preload_enabled_for(Some(OsStr::new(""))),
            "an empty value is not a request to disable it"
        );
        assert!(preload_enabled_for(Some(OsStr::new("0"))), "0 means on");
        assert!(!preload_enabled_for(Some(OsStr::new("1"))), "1 disables");
    }

    #[test]
    fn decoding_gives_back_exactly_the_requested_size() {
        let dir = Scratch::new();
        let src = dir.join("cover.png");
        write_png(&src, 300, 500);

        let got = decode_rgba(&src, 128, 204).expect("a readable png decodes");
        assert_eq!((got.width, got.height), (128, 204));
        // The buffer must match the dimensions exactly, because the texture is
        // built with a stride computed from them -- a mismatch is a crash or
        // a garbled image, not a wrong-looking one.
        assert_eq!(got.rgba.len(), 128 * 204 * 4);
        assert_eq!(got.cover, src);
    }

    #[test]
    fn unreadable_sources_are_skipped_not_fatal() {
        // A preloader runs unattended over whatever is in the library, so
        // every one of these has to be a quiet `None`.
        let dir = Scratch::new();

        let missing = dir.join("nope.png");
        assert!(decode_rgba(&missing, 10, 10).is_none(), "missing file");

        let garbage = dir.join("garbage.png");
        std::fs::write(&garbage, b"this is not a png").unwrap();
        assert!(decode_rgba(&garbage, 10, 10).is_none(), "not an image");

        let real = dir.join("real.png");
        write_png(&real, 20, 20);
        assert!(decode_rgba(&real, 0, 10).is_none(), "zero width");
        assert!(decode_rgba(&real, 10, -1).is_none(), "negative height");
    }

    #[test]
    fn every_cover_is_queued_nearest_first() {
        // The bug this guards: `ahead_of` used to `.take(PRELOAD_AHEAD)`, so
        // on a 139-book library only the first 24 covers were ever decoded and
        // every card past the fourth row kept its placeholder for good.
        // Nothing is in the cache in a headless test, so every candidate
        // survives the filter and this measures the ordering alone.
        let books: Vec<crate::models::Book> = (0..100)
            .map(|i| book_with_cover(i, Some(PathBuf::from(format!("/covers/{i}.png")))))
            .collect();

        let got = ahead_of(&books, 30, 128, 204);
        assert_eq!(got.len(), 100, "every cover, not just the first batch");
        assert_eq!(
            got[0],
            PathBuf::from("/covers/30.png"),
            "starts at what is on screen"
        );
        // Having run to the end, it wraps to pick up what was skipped.
        assert_eq!(got[69], PathBuf::from("/covers/99.png"), "last one forward");
        assert_eq!(got[70], PathBuf::from("/covers/0.png"), "then wraps");
        assert_eq!(got[99], PathBuf::from("/covers/29.png"));
    }

    #[test]
    fn a_mark_past_the_end_still_queues_everything() {
        // Reachable: the grid can shrink under a search while a scroll
        // position from the longer list is still around. Clamping to the end
        // must not mean "give up" -- those cards are on screen too.
        let books: Vec<crate::models::Book> = (0..3)
            .map(|i| book_with_cover(i, Some(PathBuf::from(format!("/covers/{i}.png")))))
            .collect();
        let got = ahead_of(&books, 99, 128, 204);
        assert_eq!(got.len(), 3, "clamped to the end, then wrapped");
        assert_eq!(got[0], PathBuf::from("/covers/0.png"));
    }

    #[test]
    fn the_same_cover_is_never_queued_twice() {
        // Home shows a book in both "continue reading" and "recently added",
        // so the list it passes really can contain duplicates.
        let books = vec![
            book_with_cover(1, Some(PathBuf::from("/covers/a.png"))),
            book_with_cover(2, Some(PathBuf::from("/covers/b.png"))),
            book_with_cover(3, Some(PathBuf::from("/covers/a.png"))),
        ];
        let got = ahead_of(&books, 0, 128, 204);
        assert_eq!(got.len(), 2, "the repeat is dropped");
    }

    #[test]
    fn books_without_a_cover_are_not_queued() {
        let books: Vec<crate::models::Book> = (0..6)
            .map(|i| {
                book_with_cover(
                    i,
                    (i % 2 == 0).then(|| PathBuf::from(format!("/covers/{i}.png"))),
                )
            })
            .collect();
        let got = ahead_of(&books, 0, 128, 204);
        assert_eq!(got.len(), 3, "only the three with covers");
    }
}
