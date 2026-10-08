//! The paragraph identity a proofread tap produces (Phase 6.7).
//!
//! The reader names a tapped paragraph by its own text plus its
//! neighbours' — the handle the app's source-span mapper locates the
//! paragraph by. These tests pin the reader half of that contract on
//! the minimal fixture: the identity text is the reader's extraction of
//! the paragraph (inline markup and source wrapping collapsed away),
//! the neighbours are the chapter's adjacent paragraphs in document
//! order, and the chapter's first paragraph asserts first-ness with a
//! `None` predecessor.

mod common;

use chapbook_core::{EdgeSizes, PageMetrics, Rotation, Size};
use chapbook_reader::Session;

use common::{fixture, open_isolated, render_loaded};

fn metrics() -> PageMetrics {
    PageMetrics {
        size: Size::new(600.0, 800.0),
        margins: EdgeSizes::uniform(40.0),
        dpi_scale: 1.0,
        rotation: Rotation::None,
    }
}

/// The paragraph identity for the paragraph whose first laid-out line
/// begins with `starts_with`, tapped at that line's centre. Found by
/// text, not by locator numbers, so an edit to the fixture moves the
/// expectation with it instead of silently shifting a number.
fn tapped_identity(s: &mut Session, starts_with: &str) -> chapbook_reader::ParagraphIdentity {
    let runs = s.page_text_runs().expect("page is laid out");
    let run = runs
        .iter()
        .find(|r| r.text.starts_with(starts_with))
        .unwrap_or_else(|| panic!("no line beginning {starts_with:?} on the page"));
    let x = run.rect.origin.x + run.rect.size.w * 0.5;
    let y = run.rect.origin.y + run.rect.size.h * 0.5;
    let (_start, _end, tag) = s
        .paragraph_tag_at_page(0, 0, x, y)
        .expect("a tap on a line hits its paragraph");
    s.paragraph_identity(0, tag)
        .unwrap_or_else(|| panic!("identity for the paragraph {starts_with:?} begins"))
}

/// The fixture's paragraphs in reader extraction space, spelled out in
/// full because they ARE the contract: the app's span mapper is given
/// exactly these strings.
const FIRST_PARAGRAPH: &str = "It was a truth universally acknowledged that a fixture \
     file, however small, must be in want of some emphasis, a bit of \
     weight, and at least one hyperlink to the next chapter.";
const SECOND_PARAGRAPH: &str = "A second paragraph exercises inheritance: the chapter \
     class sets a serif family and this paragraph should inherit it, \
     while its inner annotated span overrides the color.";
const THIRD_PARAGRAPH: &str = "Block quotations get their own margins, and their text \
     should be indented on both sides according to the UA stylesheet.";
const HEADING: &str = "Chapter One: A Beginning";

#[test]
fn identity_text_is_the_extraction_inline_markup_collapsed_in() {
    let mut s = open_isolated("epub-identity-markup", &fixture("epub/minimal.epub"));
    s.set_metrics(metrics());
    render_loaded(&mut s);

    // The paragraph carries an <em>, a <strong>, and a link — the
    // rewording-across-markup case the whole step exists for. Its
    // identity text is the words, markup gone, source wrapping
    // collapsed to single spaces.
    let id = tapped_identity(&mut s, "It was a truth");
    assert_eq!(id.text, FIRST_PARAGRAPH);
    assert_eq!(id.prev.as_deref(), Some(HEADING));
    assert_eq!(id.next.as_deref(), Some(SECOND_PARAGRAPH));
}

#[test]
fn identity_neighbours_walk_the_document_order() {
    let mut s = open_isolated("epub-identity-neighbours", &fixture("epub/minimal.epub"));
    s.set_metrics(metrics());
    render_loaded(&mut s);

    let id = tapped_identity(&mut s, "A second paragraph");
    assert_eq!(id.text, SECOND_PARAGRAPH);
    assert_eq!(id.prev.as_deref(), Some(FIRST_PARAGRAPH));
    // The next paragraph is the one inside the blockquote — a nested
    // bearer, still the next paragraph in document order.
    assert_eq!(id.next.as_deref(), Some(THIRD_PARAGRAPH));
}

#[test]
fn identity_first_paragraph_asserts_firstness_with_no_prev() {
    let mut s = open_isolated("epub-identity-first", &fixture("epub/minimal.epub"));
    s.set_metrics(metrics());
    render_loaded(&mut s);

    // The heading is the chapter's first tagged paragraph: `prev: None`
    // is the assertion "this is the first", which the app's locate
    // treats as a hard fact about the chapter, not a missing value.
    let id = tapped_identity(&mut s, "Chapter One");
    assert_eq!(id.text, HEADING);
    assert_eq!(id.prev, None);
    assert_eq!(id.next.as_deref(), Some(FIRST_PARAGRAPH));
}

#[test]
fn identity_is_none_for_a_tag_the_chapter_does_not_carry() {
    let mut s = open_isolated("epub-identity-unknown", &fixture("epub/minimal.epub"));
    s.set_metrics(metrics());
    render_loaded(&mut s);

    // A tag no element of the chapter has — a stale tap, or a chapter
    // swapped underneath the caller — refuses rather than guesses.
    assert_eq!(s.paragraph_identity(0, u64::MAX), None);
    // So does a spine index past the book's end.
    assert_eq!(s.paragraph_identity(9, 1), None);
}
