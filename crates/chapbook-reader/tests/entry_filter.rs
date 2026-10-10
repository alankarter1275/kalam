//! The virtual-edit seam (Kalam Phase 6): a host entry filter installed
//! over an EPUB's bytes, and what it does to the session — the text it
//! lays out, the page it paints, and the stored locators that must
//! re-anchor over the edited text rather than trust offsets a filter may
//! have shifted.
//!
//! These are the fixtures the phase plan promised: a patched render;
//! annotation re-anchor survival on patched text; graceful degradation
//! when the edit consumed the quote itself.

mod common;

use chapbook_reader::{EntryFilter, HostHighlight};
use common::*;

/// "truth" → "verity" in minimal.epub's first chapter: one character
/// longer, early in the text, so every locator offset after it shifts by
/// one. The shape of a real pending text patch, minus the patch store.
fn verity_patch() -> EntryFilter {
    EntryFilter::new(|href, bytes| {
        if href != "OEBPS/chapter1.xhtml" {
            return bytes;
        }
        String::from_utf8(bytes)
            .expect("fixture chapter is utf-8")
            .replace("truth", "verity")
            .into_bytes()
    })
}

/// A chapter's worth of different prose, wholesale: no quote captured
/// against the original can exist in it.
const REPLACEMENT_CHAPTER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
<body>\n\
<p>Replacement prose for the degraded fixture, plain and different from the \
original chapter.</p>\n\
</body>\n\
</html>\n";

fn replaced_chapter_patch() -> EntryFilter {
    EntryFilter::new(|href, bytes| {
        if href != "OEBPS/chapter1.xhtml" {
            return bytes;
        }
        REPLACEMENT_CHAPTER.as_bytes().to_vec()
    })
}

#[test]
fn patched_text_reaches_the_session_layer() {
    let source = fixture("epub/minimal.epub");
    let plain = {
        let s = open_isolated("counts-plain", &source);
        s.chapter_char_counts().to_vec()
    };
    let mut s = open_isolated("counts-patched", &source);
    s.set_entry_filter(verity_patch());
    let patched = s.chapter_char_counts().to_vec();
    assert_eq!(patched.len(), plain.len());
    assert_eq!(
        patched[0],
        plain[0] + 1,
        "the one-character fix is in the text the session parsed"
    );
    assert_eq!(patched[1], plain[1], "the untouched chapter is untouched");
}

#[test]
fn the_patched_view_paints() {
    let source = fixture("epub/minimal.epub");
    let plain = {
        let mut s = open_isolated("paint-plain", &source);
        s.set_metrics(metrics());
        s.render().expect("plain page renders")
    };
    let mut s = open_isolated("paint-patched", &source);
    s.set_metrics(metrics());
    s.set_entry_filter(verity_patch());
    let patched = s.render().expect("patched page renders");
    assert_ne!(
        plain.data(),
        patched.data(),
        "the fix reaches the painted page"
    );
}

#[test]
fn a_highlight_re_anchors_over_edits_before_it() {
    let source = fixture("epub/minimal.epub");

    // A highlight captured mid-chapter on the unpatched text.
    let (row, span) = {
        let mut s = open_isolated("anchor-capture", &source);
        s.set_metrics(metrics());
        let total = s.chapter_char_counts()[0] as u32;
        let start = total / 2;
        let end = start + 10;
        let row = HostHighlight {
            id: 7,
            start: s.layered_locator_at(start).expect("start captures"),
            end: s.layered_locator_at(end).expect("end captures"),
            color: None,
            style: None,
            text: None,
        };
        (row, (start, end))
    };

    // The same file with "truth" → "verity" applied before parse: one
    // character added early in the chapter shifts every offset after it.
    // The stored exact offsets are now wrong; the quote context is not,
    // and that is the layer that must answer.
    let mut s = open_isolated("anchor-patched", &source);
    s.set_metrics(metrics());
    s.set_entry_filter(verity_patch());
    s.render().expect("patched session renders");
    s.set_host_highlights(vec![row]);
    let shown = s.host_highlights(0).to_vec();
    assert_eq!(shown.len(), 1, "the row re-anchored instead of vanishing");
    assert_eq!(
        (shown[0].start, shown[0].end),
        (span.0 + 1, span.1 + 1),
        "the highlight moved with its words, not with its stale offsets"
    );
}

#[test]
fn a_highlight_whose_words_were_edited_degrades_gracefully() {
    let source = fixture("epub/minimal.epub");

    // Endpoints captured at the very start of the chapter.
    let row = {
        let mut s = open_isolated("degrade-capture", &source);
        s.set_metrics(metrics());
        HostHighlight {
            id: 8,
            start: s.layered_locator_at(0).expect("start captures"),
            end: s.layered_locator_at(40).expect("end captures"),
            color: None,
            style: None,
            text: None,
        }
    };

    // The edit replaces the whole chapter with different prose, so neither
    // endpoint's quote context exists anywhere in the new text — and the
    // exact offsets are wrong too. The only layer left is the fraction,
    // and a fraction always answers: the highlight survives, approximately
    // where it was, instead of vanishing or painting over unrelated words
    // at its stale offsets.
    let mut s = open_isolated("degrade-patched", &source);
    s.set_metrics(metrics());
    s.set_entry_filter(replaced_chapter_patch());
    s.render().expect("patched session renders");
    s.set_host_highlights(vec![row]);
    let shown = s.host_highlights(0).to_vec();
    assert_eq!(
        shown.len(),
        1,
        "a destroyed quote degrades, it does not vanish"
    );
    assert_eq!(shown[0].start, 0, "the start's fraction lands where it was");
    assert!(
        shown[0].end > 0 && shown[0].end < 40,
        "the end scales with the new text ({}), not the stale offset",
        shown[0].end
    );
}
