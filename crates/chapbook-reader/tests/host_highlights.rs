//! kalam: highlights the host keeps. What `annotations.rs` proved for the
//! engine's own store — a mark survives a reopen, holds still under a
//! font-size change, answers a tap, recolours and goes away — proved
//! again with the host holding the rows. A `Vec` stands in for Kalam's
//! `annotations` table.

mod common;
use chapbook_reader::{HostHighlight, SearchMatch, Session};
use common::*;

/// Mark the heading of the illustrated fixture the way a host does: select,
/// capture both ends as layered locators, then keep the row yourself.
fn mark_heading(s: &mut Session, id: i64) -> (HostHighlight, (u32, u32), String) {
    s.render().expect("page renders");
    assert!(s.selection_begin(100.0, 70.0), "press must hit the heading");
    s.selection_drag(400.0, 70.0);
    let (start, end) = s.selected_range().expect("non-empty selection");
    let text = s.selected_text().expect("selection carries text");
    let row = HostHighlight {
        id,
        start: s.layered_locator_at(start).expect("start captures"),
        end: s.layered_locator_at(end).expect("end captures"),
        color: None,
        style: None,
        text: Some(text.clone()),
        has_note: false,
    };
    s.selection_clear();
    (row, (start, end), text)
}

#[test]
fn a_host_highlight_survives_a_reopen_and_a_font_change() {
    let source = fixture("epub/illustrated.epub");
    let mut shelf: Vec<HostHighlight> = Vec::new();

    let (range, text) = {
        let mut s = open_isolated("host-highlight", &source);
        s.set_metrics(metrics());
        let (row, range, text) = mark_heading(&mut s, 41);
        shelf.push(row.clone());
        s.show_host_highlight(row);
        assert_eq!(s.host_highlights(0).len(), 1, "visible without a reload");
        (range, text)
    };

    // The host reopens the book and hands its rows back.
    let mut s = reopen_isolated("host-highlight", &source);
    s.set_metrics(metrics());
    s.set_host_highlights(shelf.clone());
    let shown = s.host_highlights(0).to_vec();
    assert_eq!(shown.len(), 1, "the row came back");
    assert_eq!(shown[0].id, 41, "under the host's id");
    assert_eq!(
        (shown[0].start, shown[0].end),
        range,
        "same edition resolves the exact offsets"
    );
    assert_eq!(shown[0].text.as_deref(), Some(text.as_str()));

    let painted = s.render().expect("page renders");
    // Locator space does not move under relayout, so the highlight is
    // unchanged at a different font size.
    s.adjust_font(4.0);
    assert_eq!(s.host_highlights(0), &shown[..]);
    s.adjust_font(-4.0);

    s.hide_host_highlight(41);
    assert!(
        s.host_highlights(0).is_empty(),
        "hidden clears the cache too"
    );
    let plain = s.render().expect("page renders");
    assert_ne!(painted.data(), plain.data(), "the highlight was painted");
}

#[test]
fn taps_find_a_host_highlight_and_recolouring_reaches_the_page() {
    let source = fixture("epub/illustrated.epub");
    let mut s = open_isolated("host-tap", &source);
    s.set_metrics(metrics());
    let (row, _, _) = mark_heading(&mut s, 7);
    s.show_host_highlight(row);
    s.render().expect("page renders");

    // A tap inside the marked words finds it; the margin beside them
    // does not.
    assert_eq!(s.host_highlight_at(110.0, 70.0), Some(7));
    assert_eq!(s.host_highlight_at(2.0, 70.0), None);

    let themed = s.render().expect("page renders").data().to_vec();
    s.recolor_host_highlight(7, Some("#ff0000"));
    assert_eq!(s.host_highlights(0)[0].color.as_deref(), Some("#ff0000"));
    assert_ne!(
        s.render().expect("page renders").data(),
        &themed[..],
        "the host's colour reached the page"
    );

    // Jumping to it lands in its unit, and a page turn away and back
    // still paints it.
    s.next_page();
    assert!(s.goto_host_highlight(7));
    s.render().expect("page renders");
    assert_eq!((s.spine(), s.page()), (0, 0));
    assert_eq!(s.host_highlight_at(110.0, 70.0), Some(7));
}

#[test]
fn a_note_dot_answers_a_hover_and_follows_the_note() {
    let source = fixture("epub/illustrated.epub");
    let mut s = open_isolated("host-note-dot", &source);
    s.set_metrics(metrics());
    let (row, range, _) = mark_heading(&mut s, 9);
    s.show_host_highlight(row);
    s.render().expect("page renders");

    // Just past the span's end, high in its band: where the dot sits.
    let last = s
        .range_rects_on_page(0, 0, range.0, range.1)
        .into_iter()
        .last()
        .expect("the span paints a band");
    let (x, y) = (last.max_x() + 4.0, last.origin.y + 4.0);
    assert_eq!(
        s.host_highlight_note_at_page(0, 0, x, y),
        None,
        "no note, no dot"
    );

    let plain = s.render().expect("page renders").data().to_vec();
    s.set_host_highlight_note(9, true);
    assert!(s.host_highlights(0)[0].has_note);
    assert_eq!(s.host_highlight_note_at_page(0, 0, x, y), Some(9));
    assert_ne!(
        s.render().expect("page renders").data(),
        &plain[..],
        "the dot reached the page"
    );

    s.set_host_highlight_note(9, false);
    assert_eq!(s.host_highlight_note_at_page(0, 0, x, y), None);
}

#[test]
fn search_matches_paint_without_becoming_highlights() {
    let source = fixture("epub/illustrated.epub");
    let mut s = open_isolated("search-wash", &source);
    s.set_metrics(metrics());
    let (row, _, _) = mark_heading(&mut s, 7);
    s.show_host_highlight(row.clone());
    let plain = s.render().expect("page renders").data().to_vec();

    // The same span twice is enough to prove the plumbing: the wash
    // resolves, flags its current match, and reaches the pixels —
    // while the host's own mark stays the only thing a tap can find.
    s.set_search_matches(vec![
        SearchMatch {
            start: row.start.clone(),
            end: row.end.clone(),
            current: false,
        },
        SearchMatch {
            start: row.start.clone(),
            end: row.end.clone(),
            current: true,
        },
    ]);
    let ranges = s.search_match_ranges(0).to_vec();
    assert_eq!(ranges.len(), 2, "both matches resolved");
    assert_eq!(
        ranges.iter().map(|m| m.current).collect::<Vec<_>>(),
        vec![false, true],
        "the current match keeps its flag"
    );
    assert_eq!(s.host_highlights(0).len(), 1, "the wash adds no highlights");
    assert_eq!(s.host_highlight_at(110.0, 70.0), Some(7));
    assert_ne!(
        s.render().expect("page renders").data(),
        &plain[..],
        "the wash reached the page"
    );

    s.clear_search_matches();
    assert!(s.search_match_ranges(0).is_empty(), "the wash is gone");
    assert_eq!(s.host_highlights(0).len(), 1, "the stored mark stays");
    assert_eq!(s.host_highlight_at(110.0, 70.0), Some(7));
}

#[test]
fn a_session_that_was_given_nothing_paints_nothing() {
    let mut s = open_isolated("host-none", &fixture("epub/illustrated.epub"));
    s.set_metrics(metrics());
    s.render().expect("page renders");
    assert!(s.host_highlights(0).is_empty());
    assert_eq!(s.host_highlight_at(110.0, 70.0), None);
    assert!(!s.goto_host_highlight(1), "an unknown id goes nowhere");
}
