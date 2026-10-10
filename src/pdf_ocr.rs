//! Import-time whole-book OCR for scanned PDFs (2.20, plan step 4).
//!
//! Owner decision, 2026-10-01: importing a scanned PDF recognizes the whole
//! book, page by page, and each page is stored as its own `page_ocr_cache`
//! row the moment it finishes -- so a book opened right after import already
//! has its first pages selectable while later pages keep filling in.
//!
//! The scan runs as a task-manager job: low-priority housekeeping the user
//! did not ask for by name, but visible, with progress, and cancellable.
//! Only pages whose vector text is empty are enqueued; results are
//! deduplicated through the existing `page_ocr_cache`.
//!
//! Priority promotion: when the reader lands on a page whose text is not
//! ready -- including a jump straight to page 5000 -- that page and its
//! window move to the FRONT of the queue and the background scan continues
//! after them. The reader serves the page it is standing on through its own
//! OCR worker (that path drives the pending-selection signal), so promotion
//! is about the window around it: the next pages are ready before the
//! reader gets there.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::db::Catalog;

/// One book's scan, as the worker and the reader see it. Pure state: the
/// tests cover the ordering rules, the worker applies them.
#[derive(Debug)]
pub struct PdfOcrJobState {
    /// Pages still to recognize, in file order.
    pending: VecDeque<usize>,
    /// Reader-promoted pages. Drained before `pending`, in reading order.
    promoted: VecDeque<usize>,
}

impl PdfOcrJobState {
    /// Build the queue for the pages that need work.
    pub fn new(pages: Vec<usize>) -> Self {
        Self {
            pending: pages.into(),
            promoted: VecDeque::new(),
        }
    }

    /// The next page to recognize: promoted pages first (front of the
    /// line), then the background scan's file order.
    pub fn next_page(&mut self) -> Option<usize> {
        self.promoted.pop_front().or_else(|| self.pending.pop_front())
    }

    /// Move `pages` to the front of the queue, in the order given (the
    /// reader passes reading order). Idempotent, and it can only reorder
    /// what is still queued: a page the scan already finished must not be
    /// re-done (that would inflate the progress counter), and a page that
    /// never needed recognition (it has vector text) must not be added by
    /// a promotion -- so pages that are in neither queue are ignored.
    pub fn promote(&mut self, pages: &[usize]) {
        for page in pages {
            if self.pending.contains(page) || self.promoted.contains(page) {
                self.pending.retain(|p| p != page);
                self.promoted.retain(|p| p != page);
                self.promoted.push_back(*page);
            }
        }
    }
}

/// The registry of running scans, keyed by book. The reader looks its book
/// up here to promote pages; an absent entry simply means nobody is
/// scanning that book right now.
fn jobs() -> &'static Mutex<HashMap<i64, Arc<Mutex<PdfOcrJobState>>>> {
    static JOBS: OnceLock<Mutex<HashMap<i64, Arc<Mutex<PdfOcrJobState>>>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A global page-permit: only one background page is recognized at a time.
/// Import scans are low-priority housekeeping and must not stack CPU (and
/// OCR engine memory) across a batch import. Held per page, not per book,
/// so a promoted page never waits behind more than one in-flight page. The
/// reader's own OCR worker does not take this permit: it is the
/// user-facing fast path and must never queue behind housekeeping.
fn page_permit() -> &'static Mutex<()> {
    static PERMIT: OnceLock<Mutex<()>> = OnceLock::new();
    PERMIT.get_or_init(|| Mutex::new(()))
}

/// Promote `pages` for `book_id` to the front of its scan queue, if a scan
/// is running for that book. Cheap no-op otherwise (no lock held on the
/// registry for longer than a lookup, never on a book's queue the reader
/// does not care about).
pub fn promote(book_id: i64, pages: &[usize]) {
    let job = {
        let registry = jobs().lock().unwrap_or_else(|e| e.into_inner());
        registry.get(&book_id).cloned()
    };
    if let Some(state) = job {
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        state.promote(pages);
    }
}

/// Enqueue the whole-book scan for a freshly imported PDF.
///
/// Called from import worker threads (the file picker's batch import and
/// the watch folder), so the task itself is spawned on the main thread --
/// the task system attaches its receiver to the GLib main loop and must
/// not be driven from a worker.
///
/// The book is opened, the pages whose vector text is empty and that are
/// not already in `page_ocr_cache` become the queue, and each page is
/// recognized and persisted the moment it finishes. A digital PDF (every
/// page has vector text) ends the task immediately with a note.
pub fn enqueue_import_scan(catalog: Arc<Catalog>, book_id: i64, title: String, path: PathBuf) {
    gtk::glib::MainContext::default().invoke(move || {
        crate::tasks::spawn(
            format!("Recognizing text in {title}"),
            move |reporter| run_scan(catalog, book_id, path, reporter),
            |_update| {},
            |summary: String| {
                if !summary.is_empty() {
                    log::info!("PDF scan finished: {summary}");
                }
            },
        );
    });
}

/// The scan itself, on the task's worker thread.
fn run_scan(catalog: Arc<Catalog>, book_id: i64, path: PathBuf, reporter: crate::tasks::Reporter) -> String {
    // The fingerprint the cache rows are keyed by. Computed here, on the
    // worker -- the UI thread never stats files (2.20).
    let Some(fingerprint) = crate::pages::pdf_reader::pdf_path_fingerprint(&path) else {
        reporter.fail();
        return format!("could not read {}", path.display());
    };

    let Ok(doc) = crate::pdf::PdfDocument::open(&path) else {
        reporter.fail();
        return format!("could not open {}", path.display());
    };

    let page_count = doc.page_count();
    let mut pages: Vec<usize> = Vec::new();
    for page in 1..=page_count {
        if reporter.cancelled() {
            return String::new();
        }
        // A 5,000-page scan takes a while just to check; show that too,
        // so the task never looks stuck before recognition even starts.
        if page % 100 == 0 {
            reporter.step(page, page_count, format!("checking page {page} of {page_count}"));
        }
        // Only pages whose vector text is empty are enqueued...
        let has_vector_text = doc
            .extract_page_text(page)
            .map(|t| t.total_chars() > 0)
            .unwrap_or(false);
        if !has_vector_text {
            // ...and the persistent cache deduplicates: a page recognized
            // before (this run or a previous one) is skipped, not redone.
            if !cached(&catalog, book_id, page, &fingerprint) {
                pages.push(page);
            }
        }
    }

    if pages.is_empty() {
        // Digital PDF, or everything is already recognized. One finished
        // row saying so is honest; there is nothing to show progress for.
        return "no scanned pages to recognize".to_string();
    }

    // One engine per scan, created once (the models are megabytes; loading
    // them per page would be minutes of pure waste). The reader's own OCR
    // worker creates its own the same way. Created before the job is
    // registered: a scan that cannot run must not leave a dead queue the
    // reader keeps promoting into.
    let Some(engine) = crate::ocr::init_ocr_engine() else {
        reporter.fail();
        return "OCR engine unavailable (models missing)".to_string();
    };

    let state = Arc::new(Mutex::new(PdfOcrJobState::new(pages.clone())));
    {
        let mut registry = jobs().lock().unwrap_or_else(|e| e.into_inner());
        registry.insert(book_id, state.clone());
    }

    let total = pages.len();
    let mut done = 0usize;
    let mut recognized = 0usize;
    let summary = loop {
        let next = {
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            st.next_page()
        };
        let Some(page) = next else {
            break format!("{recognized} of {total} pages recognized");
        };
        if reporter.cancelled() {
            break format!(
                "cancelled after {done} of {total} pages ({recognized} recognized)"
            );
        }

        // Re-check the cache under the permit: the reader's own worker may
        // have recognized this page while the scan was elsewhere.
        let permit = page_permit().lock().unwrap_or_else(|e| e.into_inner());
        if cached(&catalog, book_id, page, &fingerprint) {
            drop(permit);
            done += 1;
            reporter.step(done, total, format!("page {page} already recognized"));
            continue;
        }
        let outcome = recognize_page(&engine, &doc, &catalog, book_id, page, &fingerprint);
        drop(permit);
        done += 1;
        match outcome {
            Ok(true) => {
                recognized += 1;
                reporter.step(done, total, format!("page {page} of {page_count}"));
            }
            Ok(false) => {
                reporter.step(done, total, format!("page {page} of {page_count} (no text found)"));
            }
            Err(err) => {
                log::warn!("Import OCR failed on page {page}: {err}");
                reporter.step(done, total, format!("page {page} failed"));
            }
        }
    };

    {
        let mut registry = jobs().lock().unwrap_or_else(|e| e.into_inner());
        registry.remove(&book_id);
    }
    summary
}

/// Is this page already in the persistent OCR cache for this fingerprint?
fn cached(catalog: &Catalog, book_id: i64, page: usize, fingerprint: &str) -> bool {
    matches!(
        catalog.load_page_ocr(book_id, page, fingerprint),
        Ok(Some(_))
    )
}

/// Render and recognize one page; persist it the moment it finishes.
/// `Ok(false)` means recognition ran but found no text (a blank page --
/// not an error, and worth recording so it is not retried... the cache
/// row with zero lines serves as that record).
fn recognize_page(
    engine: &crate::ocr::OcrEngine,
    doc: &crate::pdf::PdfDocument,
    catalog: &Catalog,
    book_id: i64,
    page: usize,
    fingerprint: &str,
) -> Result<bool, String> {
    let scale = 2.0f32; // 144 DPI, the reader's own OCR worker uses the same
    let rendered = doc
        .render_page_rgba(page, scale, false)
        .map_err(|e| e.to_string())?;
    let (width_pts, height_pts) = doc
        .page_dimensions(page)
        .unwrap_or((rendered.width as f32 / scale, rendered.height as f32 / scale));

    let text = crate::ocr::perform_ocr(engine, &rendered, page, width_pts, height_pts)
        .map_err(|e| e.to_string())?;
    catalog
        .save_page_ocr(book_id, page, fingerprint, &text)
        .map_err(|e| e.to_string())?;
    Ok(text.total_chars() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promoted_pages_go_first_and_the_scan_continues_after() {
        let mut state = PdfOcrJobState::new(vec![1, 2, 3, 4, 5]);
        state.promote(&[4]);
        assert_eq!(state.next_page(), Some(4));
        assert_eq!(state.next_page(), Some(1));
        assert_eq!(state.next_page(), Some(2));
        assert_eq!(state.next_page(), Some(3));
        assert_eq!(state.next_page(), Some(5));
        assert_eq!(state.next_page(), None);
    }

    /// Drain the queue the way the worker does; the drained order is the
    /// recognition order.
    fn drain(state: &mut PdfOcrJobState) -> Vec<usize> {
        std::iter::from_fn(|| state.next_page()).collect()
    }

    #[test]
    fn promotion_is_idempotent() {
        let mut state = PdfOcrJobState::new(vec![1, 2, 3]);
        state.promote(&[3, 2]);
        state.promote(&[3, 2]);
        assert_eq!(drain(&mut state), vec![3, 2, 1], "no page may be queued twice");
    }

    #[test]
    fn re_promoting_reorders_the_last_call_wins() {
        let mut state = PdfOcrJobState::new(vec![1, 2, 3, 4]);
        state.promote(&[4, 3]);
        state.promote(&[3, 4]);
        // The most recent promotion's order is the one that runs.
        assert_eq!(drain(&mut state), vec![3, 4, 1, 2]);
    }

    #[test]
    fn promoting_done_pages_is_a_no_op() {
        let mut state = PdfOcrJobState::new(vec![1, 2, 3]);
        assert_eq!(state.next_page(), Some(1));
        // Page 1 is done; promoting it again must not requeue it.
        state.promote(&[1]);
        assert_eq!(drain(&mut state), vec![2, 3]);
    }

    #[test]
    fn a_jump_to_page_5000_promotes_its_window_in_reading_order() {
        let mut state = PdfOcrJobState::new((1..=6000).collect());
        state.promote(&[5000, 5001, 5002, 5003, 4999]);
        assert_eq!(state.next_page(), Some(5000));
        assert_eq!(state.next_page(), Some(5001));
        assert_eq!(state.next_page(), Some(5002));
        assert_eq!(state.next_page(), Some(5003));
        assert_eq!(state.next_page(), Some(4999));
        // The background scan resumes at page 1 after the window.
        assert_eq!(state.next_page(), Some(1));
        assert_eq!(state.next_page(), Some(2));
    }

    #[test]
    fn empty_queue_promotes_nothing_and_finishes() {
        let mut state = PdfOcrJobState::new(Vec::new());
        state.promote(&[1, 2]);
        assert!(drain(&mut state).is_empty(), "promotion cannot resurrect a finished scan");
    }
}
