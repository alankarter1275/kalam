=== KALAM REPORT ===
engine: 6eff3d5
step: R12f dictionary card rebuilt; TOC nesting fixed
status: DONE

Both items from your last reply are in, on CI-green commits.

--- 1. the dictionary card ---------------------------------------------------
Built to R12f, from the old showDictPopup + #kalam-dict-popup CSS in this
repo's history (base commit src/epub_book.rs). Same DictCard in, same three
messages out (SaveCurrentWord / DictSearchSelect / ClearDict), same anchoring
(gtk::Popover, position Bottom, pointing_to the word's rect; GTK flips it
above when there is no room below, which is the "never covering the word"
rule). Everything drawn is the spec's: 320 px frame radius 18, header with
22 px serif word / 10 px mono pronunciation / POS pill only below two groups,
three 26 px round icon buttons, 300 px scrolling body with no scrollbar and a
40 px fade, 9 px letter-spaced section labels, numbered senses with examples,
LIKELY HERE pill, first three + "Show N more", POS dividers in the specified
order over the *flat* sense list, chips (accent / #e06c75), idiom cards,
"Did you mean" vs "Words in this phrase", "No entry for 'word'."

Two deviations, both deliberate:
  * "Find in chapter" exists but is disabled -- ReaderView has no search, as
    your R12f says. It is not silently absent.
  * 320 px is a minimum, not a maximum: GTK has no max-width, so a narrow
    window clamps the popover itself rather than the card.

One shape change, because the POS grouping needs it: DictSense now carries
its own `pos`, so `DictCard.senses` is `Vec<DictSense>` instead of a 3-tuple.
No call site changed (js_bridge.rs still calls from_entry then
build_dict_popover). The save button sends SaveCurrentWord and does not flip
its own glyph: mod.rs pops the card down on success and the toast confirms,
which is the existing behaviour and better than a card that lies until the
next lookup.

What CI proves: it compiles under -D warnings, the new CSS parses (GTK reports
parse errors on stderr at load and the reader run's stderr is clean), and the
reader still opens a book and lays out. What CI cannot prove: the card has
never been *drawn* -- opening it needs a tap on a word, and no route can
simulate that. The owner's next step-4 pass is the first time it renders.

--- 2. the TOC ---------------------------------------------------------------
rebuild_toc listed top-level entries only. Now one depth-first walk
(toc_rows) is the single source for both the list and the scroll position, so
they cannot disagree: children recurse (any depth), each row is indented
16 px per level, and the flat spine_index is kept for TocSelect. A part
heading with no target of its own is drawn as a non-clickable heading instead
of vanishing. The old flat fallback (spine titles) stays for a TOC with
nothing clickable.

Verification, given the owner's book is flat:
  * three unit tests in lists.rs, run in CI: parts-and-chapters order and
    depth, the fallback, and three levels of nesting. All pass (343 tests
    total, 0 failed).
  * CI's seeder now writes a nested TOC (two parts, three chapters each, in
    both the nav and the NCX -- playOrder 1..8, verified well-formed) for
    book 1, which is the book the reader screenshot opens. So every CI run
    now re-opens a nested-TOC book through the code path that was broken.

--- evidence ------------------------------------------------------------------
commits: 1e0dfd3 (card + TOC + CSS + nested seeder), 171bb66 / 6eeefa8 /
6f99c09 (the three build fixes below, each one CI round).
run 34554781006 on 6f99c09: build job success (clippy -D warnings clean,
343 passed / 0 failed / 2 ignored), screenshots job success. Reader pass:
route read-1, stderr is GTK's a11y note only, book_open 4.2 ms,
window_shown 114 ms, three byte-identical samples.

--- the three errors CI found in this work, so the next reader of this repo
--- knows the shape of them -----------------------------------------------
  1. engine.rs:411  can't compare `String` with `&&str`  (group filter)
  2. lists.rs:64    no field `spine` on `&&TocRow`       (it is spine_index)
  3. engine.rs:405  clippy: iter().any() -> Vec::contains
All three were mine, in the new code. The first took two attempts because
Iterator::filter hands the closure `&Item`, and Item here was already `&&str`;
it is a plain `for` loop over the array now, with a comment saying why.

--- your file, to mirror into patch/engine.rs ---------------------------------
DictSense and DictCard first (the grouping needs the per-sense pos), then the
final build_dict_popover, verbatim from src/pages/reader/engine.rs. The
helpers it calls (pos_group_keys, pos_dividers, dict_header, section_label,
pos_divider, sense_row, append_definitions, append_chips, append_sections,
icon_button, DictCard::clipboard_text) are all in the same file immediately
above/below it.

---- DictSense / DictCard (8 lines + fields; the rest of the card code is
---- context for these) ----
pub(crate) struct DictSense {
    pub(crate) pos: Option<String>,
    pub(crate) def: String,
    pub(crate) example: Option<String>,
    pub(crate) hinted: bool,
}

pub(crate) struct DictCard {
    pub(crate) word: String,
    pub(crate) pronunciation: Option<String>,
    /// The word-level part-of-speech list, already joined with middots.
    pub(crate) pos: Option<String>,
    pub(crate) senses: Vec<DictSense>,
    pub(crate) synonyms: Vec<String>,
    pub(crate) antonyms: Vec<String>,
    pub(crate) idioms: Vec<(String, String)>,
    pub(crate) suggestions: Vec<String>,
    pub(crate) saved: bool,
}

---- build_dict_popover, final ----
pub(crate) fn build_dict_popover(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    card: &DictCard,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Popover {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("kalam-reader-dict");
    root.set_size_request(320, -1);
    root.append(&dict_header(host, card, sender));

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("kalam-reader-dict-body");
    append_sections(&body, card, sender);

    // The body scrolls at 300 px and never shows a scrollbar; a 40 px
    // gradient covers the cut so it reads as a fade, not a clip.
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .max_content_height(300)
        .propagate_natural_height(true)
        .child(&body)
        .build();
    scroll.add_css_class("kalam-reader-dict-scroll");

    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("kalam-reader-dict-fade");
    fade.set_valign(gtk::Align::End);
    fade.set_size_request(-1, 40);
    fade.set_can_target(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&scroll));
    overlay.add_overlay(&fade);
    root.append(&overlay);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&root));
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_pointing_to(Some(rect));
    popover.add_css_class("kalam-reader-dict-popover");
    let tx = sender.input_sender().clone();
    popover.connect_closed(move |_| {
        let _ = tx.send(ReaderMsg::ClearDict);
    });
    popover
}

tried: rebuilt the card against the spec and the base-commit JS/CSS rather
than the recipe's stand-in; verified every gtk4 API against the gtk4-rs 0.11.0
source tree (ScrolledWindow max_content_height/propagate_natural_height,
RevealerTransitionType::None, FlowBox max_children_per_line,
Widget::clipboard) before pushing, precisely because there is no compiler in
this sandbox; checked that every CSS class the Rust uses exists in style.css
and vice versa; validated the generated nested EPUB with python (XML
well-formed, playOrder unique and ordered) before letting CI depend on it.
=== END ===
