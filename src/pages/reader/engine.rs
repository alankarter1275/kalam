
//! The reading engine, wired to the reader page.
//!
//! This file replaces the old in-app browser page and its JS bridge. Every
//! line of JS the old reader shipped is now a method call on
//! [`ReaderView`], and everything the JS used to *send back* (progress,
//! taps, selections, links) arrives through the four callbacks
//! [`wire`] installs.
//!
//! Drop this file in as `src/pages/reader/engine.rs`, add `mod engine;`
//! to `src/pages/reader/mod.rs`, and follow docs/kalam/INTEGRATION.md
//! in the kalam-engine repo for the edits around it.
//!
//! Two things live here besides the wiring:
//!
//! * `locator_to_json` / `locator_from_json` — the engine's durable
//!   position record (`LayeredLocator`) written into and read back out
//!   of the `annotations.cfi` column, which was unused. The engine's
//!   type has no serde, so the JSON is built field by field.
//! * the GTK replacements for the two things the JS drew itself: the
//!   selection chip (highlight colours / quote / dictionary / copy) and
//!   the dictionary popover.

use super::mod_model::ReaderModel;
use super::types::*;
use crate::db::{Annotation, Catalog, HighlightColor as DbColor, AnnotationStyle, PatchRecord};
use crate::service::LibraryService;
use crate::epub_book::ReadingTheme;
use gtk::prelude::*;
use kalam_reader::{
    HighlightColor, KalamPrefs, KalamTheme, LayeredLocator, NewHighlight, Quote, ReaderOptions,
    ReaderView, ReadingMode, ReadingPosition, SelectedText, LOCATOR_VERSION,
};
use relm4::ComponentSender;
use std::path::Path;

// ---------------------------------------------------------------------
// Preferences: Kalam's stored values → the engine's
// ---------------------------------------------------------------------

/// Kalam's four reading theme names are the engine's four; the engine
/// took its colours from `ReadingTheme::swatch()`, so they match.
pub(crate) fn engine_theme(theme: ReadingTheme) -> KalamTheme {
    KalamTheme::from_name(theme.as_str()).unwrap_or_default()
}

// Nine knobs, one purpose: the persisted reading preferences the engine
// consumes. Grouping them into structs would only scatter what is plainly one
// call. Same allow as `settings_panel` and `db`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn engine_prefs(
    theme: ReadingTheme,
    font_px: u32,
    line_height: f32,
    column_px: u32,
    font_family: Option<String>,
    justify: bool,
    hyphenate: bool,
    publisher_styles: bool,
) -> KalamPrefs {
    KalamPrefs {
        theme: engine_theme(theme),
        font_px: font_px as f32,
        line_height,
        column_px: column_px as f32,
        font_family,
        justify,
        hyphenate,
        publisher_styles,
    }
    .clamped()
}

/// The reading preferences as currently stored. The same reads `init`
/// makes, in one place, so a mid-session reload reopens the book with
/// exactly the preferences the reader had: every settings handler
/// persists its pref before applying it, so the catalog is always the
/// freshest source. Hyphenation stays retired (round-1 field report).
pub(crate) fn current_engine_prefs(catalog: &crate::db::Catalog) -> KalamPrefs {
    let theme = catalog
        .get_pref("reader.theme")
        .map(|v| ReadingTheme::from_str_lossy(&v))
        .unwrap_or(ReadingTheme::Sepia);
    let font_px = catalog.get_pref_i64("reader.font_px", 17).clamp(13, 24) as u32;
    let line_height = catalog
        .get_pref("reader.line_height")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.8)
        .clamp(1.3, 2.5);
    let column_px = catalog.get_pref_i64("reader.column_px", 620).clamp(400, 860) as u32;
    let font_family = catalog.get_pref("reader.font_family");
    let justify = catalog.get_pref_i64("reader.justify", 0) != 0;
    let publisher_styles = catalog.get_pref_i64("reader.publisher_styles", 1) != 0;
    engine_prefs(
        theme,
        font_px,
        line_height,
        column_px,
        font_family,
        justify,
        false,
        publisher_styles,
    )
}

/// Kalam's stored colour name → the engine's. Unknown names (and the
/// legacy "rose") fall back the same way `HighlightColor::from_str_lossy`
/// does: through Kalam's own parser first.
pub(crate) fn engine_color(name: &str) -> HighlightColor {
    HighlightColor::from_name(DbColor::from_str_lossy(name).as_str()).unwrap_or_default()
}

/// Open the book for reading. Bundled fonts only, the default cache
/// budget — the settings the engine was tuned with on the target
/// machine. `Err` for a file the engine cannot read; the caller shows
/// the "Could not open book" placeholder as before.
///
/// `patches` are the book's pending edits (Phase 6). They are applied to
/// each chapter's bytes as the reader parses them — the book on disk is
/// untouched — so what the reader sees is the edited text. An empty list
/// installs no filter at all and reads the file verbatim.
pub(crate) fn open_engine(
    file_path: &Path,
    prefs: KalamPrefs,
    patches: Vec<PatchRecord>,
) -> Result<ReaderView, kalam_reader::ChapbookError> {
    crate::timing::span("book_open");
    // Roadmap 2.8: a reader's own typefaces, scanned when the book opens.
    let options = ReaderOptions {
        fonts_dir: Some(crate::paths::fonts_dir()),
        entry_filter: patch_filter(patches),
        ..ReaderOptions::default()
    };
    // Say what the cache ceiling actually is instead of leaving it to be
    // inferred through two layers of defaults — the engine's own is 192 MB,
    // but `kalam-reader` overrides it with a 32 MB figure chosen for a 4 GB
    // machine, and reading the code is not the same as seeing the number.
    // With RUST_LOG=info this is the line that answers "how much is one
    // open book allowed to hold".
    log::info!(
        "opening {} with a {} MB cache budget",
        file_path.display(),
        options
            .cache_budget
            .unwrap_or(kalam_reader::DEFAULT_CACHE_BUDGET)
            / (1024 * 1024)
    );
    let view = ReaderView::open(file_path, prefs, &options);
    crate::timing::span_end("book_open");
    view
}

/// The virtual-edit seam (Phase 6): wrap the book's pending patches in the
/// engine's entry filter. Every chapter the reader parses goes through
/// this — the edit exists only in memory, until a review-panel bake
/// writes it into the file for real.
///
/// An empty patch list yields an unset filter: the common book — no
/// pending edits — is read verbatim and keeps full-exactness locator
/// resolution, which the session deliberately gives up while a filter is
/// installed.
fn patch_filter(patches: Vec<PatchRecord>) -> kalam_reader::EntryFilter {
    if patches.is_empty() {
        return kalam_reader::EntryFilter::default();
    }
    kalam_reader::EntryFilter::new(move |href, bytes| {
        let (patched, outcomes) = crate::epub_patches::apply_text_patches(href, &bytes, &patches);
        // A patch that no longer matches its chapter is flagged, never
        // silently skipped: the review panel surfaces these (step 5); this
        // log line is the developer's trace of the same fact, at a level
        // that stays out of the way of a normal session.
        for outcome in &outcomes {
            if !matches!(
                outcome.resolution,
                crate::epub_patches::Resolution::Found(_)
            ) {
                log::debug!(
                    "patch {} no longer matches {}: {:?}",
                    outcome.id,
                    href,
                    outcome.resolution
                );
            }
        }
        patched
    })
}

/// The reader's "continuous scroll" preference, kept under the
/// `reader.` prefix so it is global like the other reading prefs.
pub(crate) const PREF_SCROLLED: &str = "reader.scrolled";

pub(crate) fn mode_from_pref(value: i64) -> ReadingMode {
    if value != 0 {
        ReadingMode::Scrolled
    } else {
        ReadingMode::Paged
    }
}

// ---------------------------------------------------------------------
// Wiring: what the engine tells the page
// ---------------------------------------------------------------------

/// Install the four callbacks. Each one only *sends a message*; the
/// model reacts in `update_with_view` like it did for the old payloads, so
/// borrow rules stay simple (a callback never touches the model).
pub(crate) fn wire(view: &ReaderView, sender: &ComponentSender<ReaderModel>) {
    let tx = sender.input_sender().clone();
    view.connect_position(move |pos: &ReadingPosition| {
        let _ = tx.send(ReaderMsg::EnginePosition(pos.chapter, pos.fraction));
    });

    let tx = sender.input_sender().clone();
    view.connect_selection(move |sel: Option<&SelectedText>| {
        let _ = tx.send(ReaderMsg::EngineSelection(
            sel.map(|s| (s.text.clone(), gdk_rect(s.rect))),
        ));
    });

    // Phase 6.4: where the selection sits after each draw — the inline
    // editor follows its text on scroll with this. The view fires it from
    // an idle after the frame that moved the text is already painted, so
    // repositioning converges before the next paint and nothing swims.
    let tx = sender.input_sender().clone();
    view.connect_selection_moved(move |rect: Option<kalam_reader::Rect>| {
        let _ = tx.send(ReaderMsg::EngineSelectionMoved(rect.map(gdk_rect)));
    });

    // Phase 6.7: a proofread tap's paragraph — its editing identity and
    // the rect its editor opens over, or None when the chapter has no
    // editable paragraph there. The view fires it after reporting the
    // tap's own selection, so an open editor commits as its click-away
    // before this opens the next one.
    let tx = sender.input_sender().clone();
    view.connect_paragraph_tap(move |tap: Option<kalam_reader::ParagraphTap>| {
        let _ = tx.send(ReaderMsg::EngineParagraphTap(tap.map(|tap| {
            (tap.identity, gdk_rect(tap.rect))
        })));
    });

    view.connect_external_link(|href: &str| {
        if href.starts_with("http://") || href.starts_with("https://") {
            let launcher = gtk::UriLauncher::new(href);
            launcher.launch(None::<&gtk::Window>, gtk::gio::Cancellable::NONE, |_| {});
        }
    });

    // Roadmap 2.5: the engine offers the note behind an internal link
    // before following it. Kalam always takes the offer — the card carries
    // a "go to the note" button, so nothing is lost by staying put.
    let tx = sender.input_sender().clone();
    view.connect_note(move |href: &str, text: &str, x: f64, y: f64| {
        let _ = tx.send(ReaderMsg::ShowNote {
            href: href.to_string(),
            text: text.to_string(),
            x,
            y,
        });
        true
    });

    let tx = sender.input_sender().clone();
    view.connect_image_tap(move |w: u32, h: u32, bytes: &[u8]| {
        let _ = tx.send(ReaderMsg::OpenImageLightbox(w, h, bytes.to_vec()));
    });

    let tx = sender.input_sender().clone();
    view.connect_highlight_tap(move |id: i64, x: f64, y: f64| {
        let _ = tx.send(ReaderMsg::HighlightTapped(id, x, y));
    });

    let tx = sender.input_sender().clone();
    view.connect_word_tap(move |word: &str, x: f64, y: f64| {
        let _ = tx.send(ReaderMsg::WordMemoryTapped(word.to_string(), x, y));
    });

    let tx = sender.input_sender().clone();
    view.connect_word_hover(move |word_opt: Option<&str>, x: f64, y: f64| {
        let _ = tx.send(ReaderMsg::WordMemoryHover(word_opt.map(str::to_string), x, y));
    });
}

/// An engine rect (widget coordinates, f32) as the rectangle a
/// `gtk::Popover::set_pointing_to` wants.
pub(crate) fn gdk_rect(rect: kalam_reader::Rect) -> gtk::gdk::Rectangle {
    gtk::gdk::Rectangle::new(
        rect.origin.x.floor() as i32,
        rect.origin.y.floor() as i32,
        (rect.size.w.ceil() as i32).max(1),
        (rect.size.h.ceil() as i32).max(1),
    )
}

// ---------------------------------------------------------------------
// Highlights: annotation rows ↔ engine highlights
// ---------------------------------------------------------------------

/// The engine's highlight for a stored row, if the row was made by the
/// engine (its `cfi` column holds the two locators). Rows from the
/// WebKit reader have DOM paths instead and cannot be placed; they stay
/// in the sidebar list but are not painted.
pub(crate) fn highlight_of(annotation: &Annotation) -> Option<NewHighlight> {
    if annotation.kind != "highlight" {
        return None;
    }
    let (start, end) = range_from_json(annotation.cfi.as_deref()?)?;
    Some(NewHighlight {
        color: engine_color(&annotation.color),
        style: annotation.style.clone(),
        text: annotation.text_excerpt.clone(),
        start,
        end,
    })
}

/// Paint every placeable highlight of the book. Call after any change
/// to the annotations table that the widget did not make itself.
pub(crate) fn show_all_highlights(view: &ReaderView, annotations: &[Annotation]) {
    let placed: Vec<(i64, NewHighlight)> = annotations
        .iter()
        .filter_map(|a| highlight_of(a).map(|h| (a.id, h)))
        .collect();
    view.set_highlights(placed.iter().map(|(id, h)| (*id, h)));
}

// ---------------------------------------------------------------------
// Locator JSON — the `cfi` column's new contents
// ---------------------------------------------------------------------

/// One locator as JSON. Field names are the engine's, so the column
/// reads back with `locator_from_json` for as long as the engine keeps
/// them (it versions the offset with `locator_version`).
pub(crate) fn locator_to_json(l: &LayeredLocator) -> serde_json::Value {
    serde_json::json!({
        "spine_href": l.spine_href,
        "spine_index": l.spine_index,
        "char_offset": l.char_offset,
        "locator_version": l.locator_version,
        "quote": {
            "prefix": l.quote.prefix,
            "exact": l.quote.exact,
            "suffix": l.quote.suffix,
        },
        "spine_fraction": l.spine_fraction,
        "book_progression": l.book_progression,
    })
}

pub(crate) fn locator_from_json(v: &serde_json::Value) -> Option<LayeredLocator> {
    let s = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_owned);
    let quote = v.get("quote")?;
    let q = |key: &str| {
        quote
            .get(key)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    Some(LayeredLocator {
        spine_href: s("spine_href")?,
        spine_index: v.get("spine_index")?.as_u64()? as usize,
        char_offset: v.get("char_offset")?.as_u64()? as u32,
        locator_version: v
            .get("locator_version")
            .and_then(|x| x.as_u64())
            .unwrap_or(LOCATOR_VERSION as u64) as u32,
        quote: Quote {
            prefix: q("prefix"),
            exact: q("exact"),
            suffix: q("suffix"),
        },
        spine_fraction: v.get("spine_fraction").and_then(|x| x.as_f64()).unwrap_or(0.0),
        book_progression: v
            .get("book_progression")
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0),
    })
}

/// The `cfi` text for a highlight: both ends, under a version tag so a
/// later format can tell itself apart.
pub(crate) fn range_to_json(start: &LayeredLocator, end: &LayeredLocator) -> String {
    serde_json::json!({
        "kalam_locator": 1,
        "start": locator_to_json(start),
        "end": locator_to_json(end),
    })
    .to_string()
}

pub(crate) fn range_from_json(text: &str) -> Option<(LayeredLocator, LayeredLocator)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("kalam_locator")?.as_u64()? != 1 {
        return None;
    }
    Some((
        locator_from_json(v.get("start")?)?,
        locator_from_json(v.get("end")?)?,
    ))
}

/// The `cfi` text for a *position* (one locator) — what `save_progress`
/// can store beside `(chapter_index, fraction)` if you add a column for
/// it later. Not used in v1; the pair is enough for the engine's
/// `goto_chapter`.
#[allow(dead_code)] // serializes a locator for position persistence, wired later
pub(crate) fn position_to_json(l: &LayeredLocator) -> String {
    serde_json::json!({ "kalam_locator": 1, "at": locator_to_json(l) }).to_string()
}

// ---------------------------------------------------------------------
// The selection chip and the dictionary popover (GTK, no JS)
// ---------------------------------------------------------------------

/// The chip that appears over a finished selection — the old WebKit
/// reader's `#kalam-chip`, rebuilt in GTK.
///
/// The JS chip is the spec, button for button: a highlighter button that
/// opens the five swatches inline (the row starts hidden), then quote,
/// dictionary and copy, 1 px separators between the groups, 32 px round
/// buttons carrying the same 17 px icons. `resources/style.css` under
/// `kalam-reader-chip*` holds the old chip's numbers.
///
/// The engine now hands the selection's *band* rects (glyph box plus 2 px)
/// rather than the line boxes, so the chip sits closer to the text than it
/// used to — close enough to cover the start handle's grip, which stands a
/// few px above the first band. The popover is pointed at the band with
/// that much headroom added, so the chip clears the grip.
///
/// Returns a popover already pointed at the selection; the caller keeps it
/// in the model so `None` (selection cleared) can pop it down.
fn action_button(icon_name: &str, tooltip: &str, accent: bool) -> gtk::Button {
    let btn = gtk::Button::from_icon_name(icon_name);
    btn.add_css_class("k-sel-action");
    if accent {
        btn.add_css_class("accent");
    }
    btn.set_tooltip_text(Some(tooltip));
    btn.set_size_request(26, 26);
    btn.set_valign(gtk::Align::Center);
    btn.set_halign(gtk::Align::Center);
    btn
}

pub(crate) fn build_calibre_drawer_box(
    initial_color: DbColor,
    initial_style: AnnotationStyle,
    initial_note: &str,
    existing_id: Option<i64>,
    on_save: impl Fn(DbColor, AnnotationStyle, String) + 'static,
    on_delete: Option<Box<dyn Fn(i64) + 'static>>,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("k-annotation-drawer");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("k-annotation-header");

    let cur_color = std::rc::Rc::new(std::cell::Cell::new(initial_color));
    let cur_style = std::rc::Rc::new(std::cell::Cell::new(initial_style));

    let colors_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    colors_row.add_css_class("k-color-bar");

    let mut color_dots: Vec<(DbColor, gtk::Button)> = Vec::new();

    for color in DbColor::SOFT_FIVE {
        let dot = gtk::Button::new();
        dot.add_css_class("k-color-dot");
        dot.add_css_class(&format!("k-color-dot-{}", color.as_str()));
        if *color == initial_color {
            dot.add_css_class("active");
        }
        dot.set_tooltip_text(Some(&format!("Highlight {}", color.as_str())));
        dot.set_size_request(20, 20);
        dot.set_valign(gtk::Align::Center);
        dot.set_halign(gtk::Align::Center);
        colors_row.append(&dot);
        color_dots.push((*color, dot));
    }

    let color_dots_rc = std::rc::Rc::new(color_dots);
    for (color, dot) in color_dots_rc.iter() {
        let cur = cur_color.clone();
        let dots = color_dots_rc.clone();
        let c = *color;
        dot.connect_clicked(move |_| {
            cur.set(c);
            for (dot_c, btn) in dots.iter() {
                if *dot_c == c {
                    btn.add_css_class("active");
                } else {
                    btn.remove_css_class("active");
                }
            }
        });
    }
    header.append(&colors_row);

    let sep = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sep.add_css_class("k-sel-divider");
    header.append(&sep);

    let styles_row = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    styles_row.add_css_class("k-style-group");

    let mut style_buttons: Vec<(AnnotationStyle, gtk::Button)> = Vec::new();

    for style in AnnotationStyle::ALL {
        let btn = gtk::Button::with_label(style.label());
        btn.add_css_class("k-style-btn");
        if *style == initial_style {
            btn.add_css_class("active");
        }
        btn.set_tooltip_text(Some(style.label()));
        btn.set_valign(gtk::Align::Center);
        styles_row.append(&btn);
        style_buttons.push((*style, btn));
    }

    let style_buttons_rc = std::rc::Rc::new(style_buttons);
    for (style, btn) in style_buttons_rc.iter() {
        let cur = cur_style.clone();
        let btns = style_buttons_rc.clone();
        let st = *style;
        btn.connect_clicked(move |_| {
            cur.set(st);
            for (btn_s, b) in btns.iter() {
                if *btn_s == st {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
        });
    }
    header.append(&styles_row);
    card.append(&header);

    // Note text view
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_size_request(280, 56);
    scrolled.add_css_class("k-annotation-note-scroll");

    let note_view = gtk::TextView::new();
    note_view.add_css_class("k-annotation-note-area");
    note_view.set_wrap_mode(gtk::WrapMode::WordChar);
    let buffer = note_view.buffer();
    if !initial_note.is_empty() {
        buffer.set_text(initial_note);
    }
    scrolled.set_child(Some(&note_view));
    card.append(&scrolled);

    // Footer actions
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.add_css_class("k-annotation-actions");

    if let (Some(id), Some(del_cb)) = (existing_id, on_delete) {
        let del_btn = gtk::Button::new();
        del_btn.set_child(Some(&crate::icons::labelled(
            "user-trash-symbolic",
            14,
            "Delete",
            4,
        )));
        del_btn.add_css_class("k-annotation-del-btn");
        del_btn.add_css_class("danger");
        del_btn.set_tooltip_text(Some("Delete this annotation"));
        del_btn.connect_clicked(move |_| {
            del_cb(id);
        });
        footer.append(&del_btn);
    }

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    footer.append(&spacer);

    let save_btn = gtk::Button::with_label("Save");
    save_btn.add_css_class("k-annotation-save-btn");
    save_btn.add_css_class("accent");
    let buf_clone = buffer.clone();
    save_btn.connect_clicked(move |_| {
        let text = buf_clone.text(&buf_clone.start_iter(), &buf_clone.end_iter(), false).to_string();
        on_save(cur_color.get(), cur_style.get(), text);
    });
    footer.append(&save_btn);

    card.append(&footer);
    card
}

pub(crate) fn build_selection_chip(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Popover {
    crate::icons::init();

    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(false);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Top);

    let mut anchor = *rect;
    anchor.set_y(anchor.y() - HANDLE_HEADROOM);
    anchor.set_height(anchor.height() + HANDLE_HEADROOM);
    popover.set_pointing_to(Some(&anchor));
    popover.add_css_class("k-sel-toolbar-popover");

    let pill = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    pill.add_css_class("k-sel-toolbar");

    // 1. Highlight button
    let highlight_btn = action_button("kalam-highlight-symbolic", "Highlight", true);
    let root_ref = root_box.clone();
    let pill_ref = pill.clone();
    let pop_ref = popover.clone();
    let tx_save = sender.input_sender().clone();
    highlight_btn.connect_clicked(move |_| {
        pill_ref.set_visible(false);
        let tx = tx_save.clone();
        let drawer = build_calibre_drawer_box(
            DbColor::Yellow,
            AnnotationStyle::Solid,
            "",
            None,
            move |color, style, note| {
                let _ = tx.send(ReaderMsg::SaveAnnotationDetails {
                    color: color.as_str().to_string(),
                    style: style.as_str().to_string(),
                    note,
                });
            },
            None,
        );
        root_ref.append(&drawer);
        pop_ref.present();
    });
    pill.append(&highlight_btn);

    let sep = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sep.add_css_class("k-sel-divider");
    pill.append(&sep);

    // 2. Define button
    let define_btn = action_button("accessories-dictionary-symbolic", "Define", false);
    let tx_def = sender.input_sender().clone();
    define_btn.connect_clicked(move |_| {
        let _ = tx_def.send(ReaderMsg::LookUpSelection);
    });
    pill.append(&define_btn);

    // 3. Fix typo button (phase 6.4): the selection itself becomes
    // editable — an entry laid exactly over it in the reader's own
    // typeface. The model opens the editor; the chip stands down for it.
    let edit_btn = action_button("document-edit-symbolic", "Fix typo", false);
    let tx_edit = sender.input_sender().clone();
    edit_btn.connect_clicked(move |_| {
        let _ = tx_edit.send(ReaderMsg::BeginInlineEdit);
    });
    pill.append(&edit_btn);

    root_box.append(&pill);
    popover.set_child(Some(&root_box));
    popover
}

/// The inline editor (phase 6.4): a single-line entry laid exactly over
/// the selected text, pre-filled with it, set in the reader's own
/// typeface and size — the correction happens where the text is, not in
/// a box beside it. Enter commits, Escape abandons, focus leaving
/// commits; whichever fires first settles the edit (the done flag) and
/// the model re-checks state on arrival, so a doubled message is benign.
///
/// Returns the entry and the CssProvider that typesets it. The entry is
/// unparented — the model adds it to the reader overlay with margins
/// for the selection rect; the provider is registered on the display
/// here (house style: the widget-local StyleContext is the deprecated
/// API) and the model unregisters it when the editor closes, so a
/// session does not accumulate one per edit.
pub(crate) fn build_inline_editor(
    text: &str,
    font_family: Option<&str>,
    font_px: u32,
    sender: &ComponentSender<ReaderModel>,
) -> (gtk::Entry, gtk::CssProvider) {
    let entry = gtk::Entry::new();
    entry.add_css_class("k-inline-edit");
    entry.set_text(text);
    // Positioned by margins against the overlay's start edges — see
    // `position_inline_editor`.
    entry.set_halign(gtk::Align::Start);
    entry.set_valign(gtk::Align::Start);

    // The reader's own type at the selection, so the correction reads as
    // part of the line it is fixing. A reader-chosen family with a quote
    // in its name is clipped rather than allowed to break out of the
    // CSS string.
    let family = font_family
        .filter(|f| !f.is_empty())
        .map(|f| f.replace('\'', ""))
        .unwrap_or_else(|| kalam_reader::BODY_FONT.to_string());
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&format!(
        "entry.k-inline-edit {{ font-family: '{}'; font-size: {}px; }}",
        family,
        font_px.max(1)
    ));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // One flag, three closures: whichever of Enter, Escape, or focus-out
    // lands first settles the edit; the other two stay quiet. Each
    // closure owns its own handle on the flag (the drawer's `cur_color`
    // shape — an Rc is moved once, cloned once per owner).
    let done = std::rc::Rc::new(std::cell::Cell::new(false));

    let tx = sender.input_sender().clone();
    let entry_activate = entry.clone();
    let done_activate = done.clone();
    entry.connect_activate(move |_| {
        if done_activate.replace(true) {
            return;
        }
        let _ = tx.send(ReaderMsg::CommitInlineEdit(
            entry_activate.text().to_string(),
        ));
    });

    let tx = sender.input_sender().clone();
    let key = gtk::EventControllerKey::new();
    let done_key = done.clone();
    key.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            if !done_key.replace(true) {
                let _ = tx.send(ReaderMsg::CancelInlineEdit);
            }
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    entry.add_controller(key);

    // Clicking away is a commit: the reader is done with the box and
    // expects the text to have been taken seriously. Leaving by Enter or
    // Escape already set the flag, so this stays quiet after them.
    let tx = sender.input_sender().clone();
    let entry_focus = entry.clone();
    let done_focus = done.clone();
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(move |_| {
        if done_focus.replace(true) {
            return;
        }
        let _ = tx.send(ReaderMsg::CommitInlineEdit(
            entry_focus.text().to_string(),
        ));
    });
    entry.add_controller(focus);

    (entry, provider)
}

/// Take the provider back off the display when an editor closes — the
/// counterpart of the registration in [`build_inline_editor`].
pub(crate) fn remove_inline_edit_provider(provider: &gtk::CssProvider) {
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_remove_provider_for_display(&display, provider);
    }
}

/// The paragraph editor (phase 6.7): the inline editor's paragraph
/// scope. A wrapped, multi-line view rather than an entry — a paragraph
/// keeps its `<br>` newlines, which an entry cannot hold — with the
/// same contract as the selection editor: the reader's own typeface,
/// Enter commits, Escape cancels, clicking away commits, and one flag
/// settles whichever lands first. Enter commits rather than inserting a
/// line break (the shared editing feel); the paragraph's existing line
/// breaks survive in the buffer, and commit serializes them back as
/// `<br/>`.
pub(crate) fn build_paragraph_editor(
    text: &str,
    font_family: Option<&str>,
    font_px: u32,
    on_commit: impl Fn(String) + Clone + 'static,
    on_cancel: impl Fn() + 'static,
) -> (gtk::TextView, gtk::CssProvider) {
    let view = gtk::TextView::new();
    view.add_css_class("k-inline-edit");
    view.set_halign(gtk::Align::Start);
    view.set_valign(gtk::Align::Start);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    // Tab belongs to the reader (focus moves), not the paragraph.
    view.set_accepts_tab(false);
    view.buffer().set_text(text);

    // The reader's own type, as the selection editor typesets it.
    let family = font_family
        .filter(|f| !f.is_empty())
        .map(|f| f.replace('\'', ""))
        .unwrap_or_else(|| kalam_reader::BODY_FONT.to_string());
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&format!(
        "textview.k-inline-edit {{ font-family: '{}'; font-size: {}px; }}",
        family,
        font_px.max(1)
    ));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // One flag, the selection editor's shape. The callers' commits and
    // cancels arrive as closures (Phase 6.8: the full editor builds
    // the same box around its own messages), so the two surfaces keep
    // one editing feel by sharing this one implementation.
    let done = std::rc::Rc::new(std::cell::Cell::new(false));

    // The focus controller below shares the commit callback with the
    // key controller, and a `move` closure can only take it once —
    // the clone happens before either does.
    let on_commit_focus = on_commit.clone();

    let view_keys = view.clone();
    let done_keys = done.clone();
    let key = gtk::EventControllerKey::new();
    key.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
            if !done_keys.replace(true) {
                on_commit(textview_text(&view_keys));
            }
            return gtk::glib::Propagation::Stop;
        }
        if keyval == gtk::gdk::Key::Escape {
            if !done_keys.replace(true) {
                on_cancel();
            }
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    view.add_controller(key);

    // Clicking away is a commit, as in the selection editor.
    let view_focus = view.clone();
    let done_focus = done.clone();
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(move |_| {
        if done_focus.replace(true) {
            return;
        }
        on_commit_focus(textview_text(&view_focus));
    });
    view.add_controller(focus);

    (view, provider)
}

/// A text view's whole buffer, as plain text.
fn textview_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}

/// Lay the editor over its selection. Overlay coordinates are the view
/// widget's own — the same space the selection rect is measured in (the
/// strip scrollbar is placed the same way) — so start-aligned margins
/// land the editor exactly on the text. Width has a floor: a one-word
/// selection still gets a box worth typing into. The same geometry
/// places the selection editor (an entry) and the paragraph editor (a
/// text view).
pub(crate) fn position_inline_editor(
    widget: &impl gtk::prelude::IsA<gtk::Widget>,
    rect: &gtk::gdk::Rectangle,
) {
    widget.set_margin_start(rect.x());
    widget.set_margin_top(rect.y());
    widget.set_size_request(rect.width().max(160), rect.height().max(28));
}

/// Outcome for a whole-resource save from the deep editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DirectWriteOutcome {
    Saved {
        entry_hash: String,
        book_hash: String,
        warning: Option<String>,
    },
    Stale,
}

/// Plan and permanently save an inline edit off the GTK thread. New saves
/// refuse while old pending patches exist; those must be reviewed through
/// the legacy panel first so a direct write cannot silently bake or lose them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn save_inline_edit(
    service: LibraryService,
    book_id: i64,
    book_path: std::path::PathBuf,
    expected_book_hash: String,
    chapter: usize,
    original: String,
    paragraph_context: Option<(Option<String>, Option<String>)>,
    corrected: String,
    backup: crate::epub_sanitizer::DirectSaveBackup,
    serial: u64,
    verdict_tx: async_channel::Sender<Result<DirectEditSaved, (String, u64)>>,
) {
    std::thread::spawn(move || {
        let verdict = save_inline_edit_in_thread(
            service.catalog().as_ref(),
            book_id,
            &book_path,
            &expected_book_hash,
            chapter,
            &original,
            paragraph_context,
            &corrected,
            backup,
            serial,
        )
        .map_err(|message| (message, serial));
        let _ = verdict_tx.send_blocking(verdict);
    });
}

#[allow(clippy::too_many_arguments)]
fn save_inline_edit_in_thread(
    catalog: &Catalog,
    book_id: i64,
    book_path: &Path,
    expected_book_hash: &str,
    chapter: usize,
    original: &str,
    paragraph_context: Option<(Option<String>, Option<String>)>,
    corrected: &str,
    backup: crate::epub_sanitizer::DirectSaveBackup,
    serial: u64,
) -> Result<DirectEditSaved, String> {
    use chapbook_core::Publication;

    let pending = catalog
        .get_pending_patches_for_book(book_id)
        .map_err(|e| format!("Could not check legacy edits before saving: {e}"))?;
    if !pending.is_empty() {
        return Err(
            "This book has pending edits from an earlier version. Review and apply or discard them before making a permanent edit.".into(),
        );
    }
    let book_row = catalog
        .get_book(book_id)
        .map_err(|e| format!("Could not read this book's library record: {e}"))?
        .ok_or_else(|| "This book is no longer in the library.".to_string())?;
    if book_row.file_path.as_path() != book_path {
        return Err("This book moved after it opened. Reopen it before editing.".into());
    }

    let book = chapbook_epub::Book::open(book_path)
        .map_err(|_| "The book could not be opened to save this edit.".to_string())?;
    let href = book
        .spine()
        .get(chapter)
        .map(|item| item.href.clone())
        .ok_or_else(|| "The chapter being edited is no longer in the book.".to_string())?;
    let bytes = book
        .unit_bytes(chapter)
        .map_err(|_| "The chapter's text could not be read to save this edit.".to_string())?;
    let planned = match paragraph_context {
        Some((prev, next)) => crate::epub_spans::plan_paragraph_patch(
            &bytes,
            original,
            prev.as_deref(),
            next.as_deref(),
            corrected,
        )
        .map_err(|refusal| paragraph_refusal_to_toast(&refusal))?,
        None => crate::epub_patches::plan_text_patch(&bytes, original, corrected)
            .map_err(|refusal| refusal_to_toast(&refusal))?,
    };
    let replacement = crate::epub_patches::apply_planned_patch(&bytes, &planned)
        .map_err(|refusal| refusal_to_toast(&refusal))?;
    let expected_hash = crate::epub_patches::hash_bytes(&bytes);
    let outcome = write_editor_resource(DirectResourceWrite {
        catalog,
        book_id,
        book_path,
        href: &href,
        expected_book_hash,
        before_hash: &expected_hash,
        replacement: &replacement,
        backup,
    })?;
    let (book_hash, warning) = match outcome {
        DirectWriteOutcome::Saved {
            book_hash, warning, ..
        } => (book_hash, warning),
        DirectWriteOutcome::Stale => {
            return Err(
                "The chapter changed after this edit opened. Your text is still here; reload the book and try again.".into(),
            )
        }
    };
    Ok(DirectEditSaved {
        serial,
        book_hash,
        warning,
    })
}

/// Save a raw source buffer directly into its existing EPUB resource. The
/// worker checks for legacy pending patches and uses the loaded entry and
/// archive hashes as optimistic guards, then updates the catalog and search
/// index after the atomic archive replacement succeeds.
#[allow(clippy::too_many_arguments)]
pub(crate) fn save_source_entry_direct(
    service: LibraryService,
    book_id: i64,
    book_path: std::path::PathBuf,
    expected_book_hash: String,
    href: String,
    before_hash: String,
    text: String,
    backup: crate::epub_sanitizer::DirectSaveBackup,
    tx: async_channel::Sender<Result<DirectWriteOutcome, String>>,
) {
    std::thread::spawn(move || {
        let result = write_editor_resource(DirectResourceWrite {
            catalog: service.catalog().as_ref(),
            book_id,
            book_path: &book_path,
            href: &href,
            expected_book_hash: &expected_book_hash,
            before_hash: &before_hash,
            replacement: text.as_bytes(),
            backup,
        });
        let _ = tx.send_blocking(result);
    });
}

struct DirectResourceWrite<'a> {
    catalog: &'a Catalog,
    book_id: i64,
    book_path: &'a Path,
    href: &'a str,
    expected_book_hash: &'a str,
    before_hash: &'a str,
    replacement: &'a [u8],
    backup: crate::epub_sanitizer::DirectSaveBackup,
}

fn write_editor_resource(write: DirectResourceWrite<'_>) -> Result<DirectWriteOutcome, String> {
    let DirectResourceWrite {
        catalog,
        book_id,
        book_path,
        href,
        expected_book_hash,
        before_hash,
        replacement,
        backup,
    } = write;
    let _timing = crate::timing::measure("direct_epub_save");
    let _save = crate::epub_sanitizer::EPUB_WRITE_LOCK
        .lock()
        .map_err(|_| "another EPUB save failed unexpectedly".to_string())?;
    let pending = catalog
        .get_pending_patches_for_book(book_id)
        .map_err(|e| format!("Could not check legacy edits before saving: {e}"))?;
    if !pending.is_empty() {
        return Err(
            "This book has pending edits from an earlier version. Review and apply or discard them before making a permanent edit.".into(),
        );
    }
    let book_row = catalog
        .get_book(book_id)
        .map_err(|e| format!("Could not read this book's library record: {e}"))?
        .ok_or_else(|| "This book is no longer in the library.".to_string())?;
    if book_row.file_path.as_path() != book_path {
        return Err("This book moved after it opened. Reopen it before editing.".into());
    }
    let report = match crate::epub_sanitizer::save_epub_entry_direct_locked(
        book_path,
        href,
        Some(expected_book_hash),
        before_hash,
        replacement,
        backup,
    )
    .map_err(|e| format!("Could not write the EPUB: {e:#}"))?
    {
        crate::epub_sanitizer::DirectSaveOutcome::Stale => {
            return Ok(DirectWriteOutcome::Stale)
        }
        crate::epub_sanitizer::DirectSaveOutcome::Saved(report) => report,
    };

    let mut warnings = Vec::new();
    if let Err(err) = catalog.rehash_book_after_direct_save(book_id, &report.book_hash) {
        warnings.push(format!("The EPUB was saved, but its library hash needs repair: {err}"));
    }
    if let Err(err) = crate::content_index::index_book(catalog, book_id) {
        warnings.push(format!("The EPUB was saved, but search indexing failed: {err}"));
    }
    crate::sidecar::refresh_for_book(catalog, book_id);
    Ok(DirectWriteOutcome::Saved {
        entry_hash: report.entry_hash,
        book_hash: report.book_hash,
        warning: (!warnings.is_empty()).then(|| warnings.join(" ")),
    })
}

/// A paragraph refusal as the text the toast shows. Each one names the
/// way out where there is one.
fn paragraph_refusal_to_toast(refusal: &crate::epub_spans::ParagraphRefusal) -> String {
    use crate::epub_spans::ParagraphRefusal;
    match refusal {
        ParagraphRefusal::Unchanged => {
            "No change to save — the paragraph already says that.".to_string()
        }
        ParagraphRefusal::NotFound => {
            "Couldn't locate that paragraph in the chapter's source — it may have \
             changed underneath the edit."
                .to_string()
        }
        ParagraphRefusal::Ambiguous(n) => format!(
            "That paragraph cannot be told apart from {n} others — even its \
             neighbours repeat. Move on past this one; the full editor will \
             be able to name it."
        ),
        ParagraphRefusal::NotMappable => {
            "This chapter's file is not well-formed enough for paragraph \
             editing — its typos can still be fixed by selecting them."
                .to_string()
        }
    }
}

/// A refusal as the text the toast shows. Each one names the way out:
/// paragraph editing for markup-crossing selections (step 7), a wider
/// selection for ambiguity.
fn refusal_to_toast(refusal: &crate::epub_patches::PatchRefusal) -> String {
    use crate::epub_patches::PatchRefusal;
    match refusal {
        PatchRefusal::Unchanged => "No change to save — the text already says that.".to_string(),
        PatchRefusal::NotFound => {
            "Couldn't locate that text in the chapter's source — it may cross formatting \
             (italics and the like), which paragraph editing will handle."
                .to_string()
        }
        PatchRefusal::Ambiguous(n) => format!(
            "That text appears {n} times — select a wider stretch so the fix lands on the right one."
        ),
    }
}

pub(crate) fn build_annotation_edit_popover(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    anno: &Annotation,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Popover {
    crate::icons::init();

    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(true);
    popover.set_position(gtk::PositionType::Top);

    let mut anchor = *rect;
    anchor.set_y(anchor.y() - HANDLE_HEADROOM);
    anchor.set_height(anchor.height() + HANDLE_HEADROOM);
    popover.set_pointing_to(Some(&anchor));
    popover.add_css_class("k-sel-toolbar-popover");

    let tx_save = sender.input_sender().clone();
    let tx_del = sender.input_sender().clone();
    let id = anno.id;

    let initial_color = DbColor::from_str_lossy(&anno.color);
    let initial_style = AnnotationStyle::from_str_lossy(&anno.style);

    let drawer = build_calibre_drawer_box(
        initial_color,
        initial_style,
        &anno.note,
        Some(id),
        move |color, style, note| {
            let _ = tx_save.send(ReaderMsg::EditAnnotationDetails {
                id,
                color: color.as_str().to_string(),
                style: style.as_str().to_string(),
                note,
            });
        },
        Some(Box::new(move |del_id| {
            let _ = tx_del.send(ReaderMsg::DeleteAnnotation(del_id));
        })),
    );

    popover.set_child(Some(&drawer));
    popover
}

pub(crate) fn build_word_preview_tooltip(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    word: &str,
    catalog: &crate::db::Catalog,
) -> Option<gtk::Popover> {
    let clean = word.trim().to_string();
    if clean.is_empty() {
        return None;
    }
    let data = catalog.lookup_entry(&clean).ok()?;
    let def = data.senses.first().map(|s| s.def.as_str())?;

    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(true);
    popover.set_position(gtk::PositionType::Top);
    popover.set_pointing_to(Some(rect));
    popover.add_css_class("k-word-preview-popover");

    let card = gtk::Box::new(gtk::Orientation::Vertical, 3);
    card.add_css_class("k-word-preview-card");

    let head = gtk::Label::new(Some(&clean));
    head.add_css_class("k-word-preview-head");
    head.set_xalign(0.0);
    card.append(&head);

    let preview = if def.len() > 120 {
        format!("{}…", &def[..118])
    } else {
        def.to_string()
    };
    let body = gtk::Label::new(Some(&preview));
    body.add_css_class("k-word-preview-def");
    body.set_wrap(true);
    body.set_xalign(0.0);
    card.append(&body);

    popover.set_child(Some(&card));
    Some(popover)
}

const HANDLE_HEADROOM: i32 = 8;
/// One sense as the card draws it.
///
/// `hinted` is the Lesk "likely here" sense, which keeps its index into
/// the *flat* sense list however the card then groups them.
pub(crate) struct DictSense {
    pub(crate) def: String,
    pub(crate) example: Option<String>,
    pub(crate) hinted: bool,
}

/// What the dictionary card shows — the same fields the JS popup was fed,
/// flattened to plain strings so this file does not depend on the exact
/// shape of `EntryData`'s optional fields.
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

impl DictCard {
    /// `Sense.example` is an `Option<String>` in this tree, so the empty
    /// filter applies to it directly; wrapping it in `Option::<String>::from`
    /// is a useless conversion, which `-D warnings` rejects.
    /// `EntryData.pos` is a `Vec<String>`, joined with middots below.
    pub(crate) fn from_entry(
        data: &crate::db::EntryData,
        pronunciation: Option<String>,
        saved: bool,
        hint: Option<usize>,
    ) -> DictCard {
        let senses: Vec<DictSense> = data
            .senses
            .iter()
            .enumerate()
            .map(|(i, s)| DictSense {
                def: s.def.clone(),
                example: s.example.clone().filter(|e| !e.is_empty()),
                hinted: hint == Some(i),
            })
            .collect();
        DictCard {
            word: data.word.clone(),
            pronunciation,
            // Kalam's `EntryData.pos` is the word-level list (`Vec<String>`),
            // not a lone `String`/`Option<String>`; the old WebKit popup
            // showed it joined with a middot, so this does too.
            pos: if data.pos.is_empty() {
                None
            } else {
                Some(data.pos.join(" \u{00b7} "))
            },
            senses,
            synonyms: data.synonyms.iter().map(|s| s.to_string()).collect(),
            antonyms: data.antonyms.iter().map(|s| s.to_string()).collect(),
            idioms: data
                .idioms
                .iter()
                .map(|(phrase, def)| (phrase.to_string(), def.to_string()))
                .collect(),
            suggestions: data.suggestions.iter().map(|s| s.to_string()).collect(),
            saved,
        }
    }
}

/// Best-effort text-to-speech for the pronunciation button. Tries the common
/// Linux speech engines in turn; if none is present, tells the user which to
/// install. Runs off the UI thread so a missing binary never blocks the popup.
fn speak_word(word: String) {
    std::thread::spawn(move || {
        let attempts: Vec<(String, Vec<String>)> = vec![
            ("espeak-ng".to_string(), vec!["-v".into(), "en".into(), word.clone()]),
            ("espeak".to_string(), vec!["-v".into(), "en".into(), word.clone()]),
            ("spd-say".to_string(), vec!["-w".into(), word.clone()]),
        ];
        for (cmd, args) in attempts {
            if let Ok(status) = std::process::Command::new(&cmd).args(&args).status() {
                if status.success() {
                    return;
                }
            }
        }
        gtk::glib::MainContext::default().invoke(|| {
            crate::notify::error(
                "No speech engine",
                "Install espeak-ng or speech-dispatcher (spd-say) to hear words aloud.",
            );
        });
    });
}

/// The parts of speech the old popup listed first, in this order. Anything
/// else keeps its first-seen order; senses with no part of speech land last
/// and get no divider row at all.
fn dict_header(
    _host: &gtk::Widget,
    card: &DictCard,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Box {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.add_css_class("k-header");

    let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
    left.add_css_class("k-header-left");
    left.set_hexpand(true);

    let word = gtk::Label::new(Some(&card.word));
    word.add_css_class("k-word");
    word.set_xalign(0.0);
    word.set_halign(gtk::Align::Start);
    word.set_wrap(true);
    left.append(&word);

    if let Some(pronunciation) = &card.pronunciation {
        let pron = gtk::Label::new(Some(pronunciation));
        pron.add_css_class("k-pronunciation");
        pron.set_xalign(0.0);
        pron.set_halign(gtk::Align::Start);
        left.append(&pron);
    }

    if let Some(pos) = &card.pos {
        let pill = gtk::Label::new(Some(pos));
        pill.add_css_class("k-pos");
        pill.set_halign(gtk::Align::Start);
        left.append(&pill);
    }
    header.append(&left);

    // Audio pronunciation — best-effort via a system speech engine.
    let audio = gtk::Button::new();
    audio.add_css_class("k-audio-btn");
    audio.set_child(Some(&gtk::Image::from_icon_name(
        "audio-volume-high-symbolic",
    )));
    audio.set_tooltip_text(Some("Hear it"));
    {
        let w = card.word.clone();
        audio.connect_clicked(move |_| speak_word(w.clone()));
    }
    header.append(&audio);

    let save = gtk::Button::new();
    save.add_css_class("k-save-btn");
    let icon = gtk::Image::from_icon_name("bookmark-new-symbolic");
    save.set_child(Some(&icon));
    save.set_valign(gtk::Align::Start);
    save.set_halign(gtk::Align::Center);
    save.set_size_request(24, 24);
    save.set_tooltip_text(Some(if card.saved { "Saved" } else { "Save word" }));
    if card.saved {
        save.add_css_class("saved");
    }
    save.set_sensitive(!card.saved && !card.senses.is_empty());
    if !card.saved {
        let tx = sender.input_sender().clone();
        save.connect_clicked(move |_| {
            let _ = tx.send(ReaderMsg::SaveCurrentWord);
        });
    }
    header.append(&save);

    header
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&text.to_uppercase()));
    label.add_css_class("k-section-label");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label
}

fn sense_row(sense: &DictSense, number: usize) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("k-def-item");

    let num = gtk::Label::new(Some(&format!("{}.", number)));
    num.add_css_class("k-def-num");
    num.set_valign(gtk::Align::Start);
    row.append(&num);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text_box.set_hexpand(true);

    if sense.hinted {
        let badge = gtk::Label::new(Some("LIKELY HERE"));
        badge.add_css_class("k-def-badge");
        badge.set_valign(gtk::Align::Start);
        badge.set_halign(gtk::Align::Start);
        text_box.append(&badge);
    }

    let def = gtk::Label::new(Some(&sense.def));
    def.add_css_class("k-def-text");
    def.set_wrap(true);
    def.set_xalign(0.0);
    def.set_halign(gtk::Align::Start);
    text_box.append(&def);

    if let Some(example) = &sense.example {
        let ex = gtk::Label::new(Some(example));
        ex.add_css_class("k-def-example");
        ex.set_wrap(true);
        ex.set_xalign(0.0);
        ex.set_halign(gtk::Align::Start);
        text_box.append(&ex);
    }
    row.append(&text_box);
    row
}

fn append_definitions(body: &gtk::Box, card: &DictCard) {
    body.append(&section_label("Definitions"));

    let defs_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    defs_box.add_css_class("k-defs");

    let shown = card.senses.len().min(3);
    for (i, sense) in card.senses.iter().take(shown).enumerate() {
        defs_box.append(&sense_row(sense, i + 1));
    }

    let extra = &card.senses[shown..];
    if !extra.is_empty() {
        let hidden = gtk::Box::new(gtk::Orientation::Vertical, 10);
        hidden.add_css_class("k-extra-defs");
        for (offset, sense) in extra.iter().enumerate() {
            hidden.append(&sense_row(sense, shown + offset + 1));
        }

        let revealer = gtk::Revealer::new();
        revealer.set_transition_type(gtk::RevealerTransitionType::None);
        revealer.set_reveal_child(false);
        revealer.set_child(Some(&hidden));
        defs_box.append(&revealer);

        let more = gtk::Button::with_label(&format!("Show {} more", extra.len()));
        more.add_css_class("k-show-more");
        more.set_halign(gtk::Align::Start);
        let count = extra.len();
        let showing = std::rc::Rc::new(std::cell::Cell::new(false));
        more.connect_clicked(move |btn| {
            let next = !showing.get();
            showing.set(next);
            revealer.set_reveal_child(next);
            let label = if next {
                "Show less".to_string()
            } else {
                format!("Show {} more", count)
            };
            btn.set_label(&label);
        });
        defs_box.append(&more);
    }

    body.append(&defs_box);
}

fn append_chips(body: &gtk::Box, words: &[String], antonym: bool, sender: &ComponentSender<ReaderModel>) {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(6)
        .row_spacing(6)
        .max_children_per_line(3)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .build();
    flow.add_css_class("k-chips");
    for word in words {
        let chip = gtk::Button::with_label(word);
        chip.add_css_class("k-chip");
        chip.add_css_class(if antonym { "k-chip-ant" } else { "k-chip-syn" });
        let tx = sender.input_sender().clone();
        let word = word.clone();
        chip.connect_clicked(move |_| {
            let _ = tx.send(ReaderMsg::DictSearchSelect(word.clone()));
        });
        flow.insert(&chip, -1);
    }
    body.append(&flow);
}

fn append_sections(body: &gtk::Box, card: &DictCard, sender: &ComponentSender<ReaderModel>) {
    if !card.senses.is_empty() {
        append_definitions(body, card);
    }
    if !card.synonyms.is_empty() {
        body.append(&section_label("Synonyms"));
        append_chips(body, &card.synonyms, false, sender);
    }
    if !card.antonyms.is_empty() {
        body.append(&section_label("Antonyms"));
        append_chips(body, &card.antonyms, true, sender);
    }
    if !card.idioms.is_empty() {
        body.append(&section_label("Idioms"));
        let idioms_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
        idioms_box.add_css_class("k-idioms");
        for (phrase, def) in &card.idioms {
            let item = gtk::Box::new(gtk::Orientation::Vertical, 3);
            item.add_css_class("k-idiom-item");
            let phrase_label = gtk::Label::new(Some(phrase));
            phrase_label.add_css_class("k-idiom-phrase");
            phrase_label.set_xalign(0.0);
            phrase_label.set_halign(gtk::Align::Start);
            phrase_label.set_wrap(true);
            item.append(&phrase_label);
            let def_label = gtk::Label::new(Some(def));
            def_label.add_css_class("k-idiom-def");
            def_label.set_xalign(0.0);
            def_label.set_halign(gtk::Align::Start);
            def_label.set_wrap(true);
            item.append(&def_label);
            idioms_box.append(&item);
        }
        body.append(&idioms_box);
    }
    if card.senses.is_empty() && !card.suggestions.is_empty() {
        let label = if card.word.split_whitespace().count() > 1 {
            "Words in this phrase"
        } else {
            "Did you mean"
        };
        body.append(&section_label(label));
        append_chips(body, &card.suggestions, false, sender);
    }
    if card.senses.is_empty() && card.suggestions.is_empty() {
        let empty = gtk::Label::new(Some(&format!("No entry for \u{2018}{}\u{2019}.", card.word)));
        empty.add_css_class("k-section-label");
        empty.set_xalign(0.0);
        empty.set_halign(gtk::Align::Start);
        body.append(&empty);
    }
}

pub(crate) fn build_dict_popover(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    card: &DictCard,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Popover {
    let popup = gtk::Box::new(gtk::Orientation::Vertical, 0);
    popup.add_css_class("k-popup");

    let width = 380.min((host.width() - 32).max(300));
    popup.set_size_request(width, -1);

    let header = dict_header(host, card, sender);
    popup.append(&header);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(true)
        .overlay_scrolling(true)
        .max_content_height(380)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("k-body");
    append_sections(&body, card, sender);
    scroll.set_child(Some(&body));
    
    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("k-fade-bottom");
    fade.set_valign(gtk::Align::End);
    fade.set_size_request(-1, 16);
    fade.set_can_target(false);
    
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&scroll));
    overlay.add_overlay(&fade);

    popup.append(&overlay);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&popup));
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Bottom);
    let mut anchor = *rect;
    anchor.set_y(anchor.y() - HANDLE_HEADROOM);
    anchor.set_height(anchor.height() + HANDLE_HEADROOM);
    popover.set_pointing_to(Some(&anchor));
    popover.add_css_class("kalam-reader-dict-popover");

    let tx = sender.input_sender().clone();
    popover.connect_closed(move |_| {
        let _ = tx.send(ReaderMsg::ClearDict);
    });

    popover
}
/// Take a popover down and off its parent. A popover with `set_parent`
/// must be `unparent`ed before it is dropped, or GTK complains.
/// Roadmap 2.5: a footnote read where the reader already is. The engine
/// hands over the note's plain text; this is the card that shows it, with
/// one escape hatch — go to the note itself. Styled by the same classes as
/// the dictionary card, so the two read as the same kind of thing.
pub(crate) fn build_note_popover(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    text: &str,
    href: &str,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Popover {
    let popup = gtk::Box::new(gtk::Orientation::Vertical, 0);
    popup.add_css_class("k-popup");
    let width = 380.min((host.width() - 32).max(280));
    popup.set_size_request(width, -1);

    let note = gtk::Label::new(Some(text));
    note.add_css_class("k-note-text");
    note.set_wrap(true);
    note.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    note.set_xalign(0.0);
    note.set_yalign(0.0);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(true)
        .overlay_scrolling(true)
        .max_content_height(260)
        .build();
    scroll.set_child(Some(&note));

    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("k-fade-bottom");
    fade.set_valign(gtk::Align::End);
    fade.set_size_request(-1, 16);
    fade.set_can_target(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&scroll));
    overlay.add_overlay(&fade);
    popup.append(&overlay);

    let go = gtk::Button::builder().label("Go to the note").build();
    go.add_css_class("flat");
    go.add_css_class("k-note-go");
    let tx = sender.input_sender().clone();
    let target = href.to_string();
    go.connect_clicked(move |_| {
        let _ = tx.send(ReaderMsg::GoToNote(target.clone()));
    });
    popup.append(&go);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&popup));
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Top);
    popover.set_pointing_to(Some(rect));
    popover.add_css_class("kalam-reader-dict-popover");

    let tx = sender.input_sender().clone();
    popover.connect_closed(move |_| {
        let _ = tx.send(ReaderMsg::ClearNote);
    });

    popover
}

pub(crate) fn dismiss(popover: Option<gtk::Popover>) {
    if let Some(p) = popover {
        p.popdown();
        p.unparent();
    }
}

// ---------------------------------------------------------------------
// The deep editor's resource browser and raw-source workers
// ---------------------------------------------------------------------

/// One safe text resource that the deep editor can open. `spine_index` is
/// present for chapters, absent for supporting XHTML/CSS such as a nav file
/// or stylesheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditableResource {
    pub(crate) href: String,
    pub(crate) spine_index: Option<usize>,
}

/// A raw-pane load: the selected resource as the reader shows it (pending
/// legacy text patches included where applicable), the hash of those bytes,
/// and the chapter the native preview should render.
#[derive(Debug, Clone)]
pub(crate) struct SourceEntry {
    pub(crate) href: String,
    pub(crate) text: String,
    pub(crate) hash: String,
    pub(crate) preview_chapter: usize,
}

fn safe_editable_href(href: &str) -> bool {
    let safe_relative_path = |path: &str| {
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && !path
                .split('/')
                .any(|part| part == ".." || part.is_empty())
    };
    safe_relative_path(href)
        && safe_relative_path(&crate::epub_sanitizer::percent_decode_path(href))
}

/// Enumerate editable EPUB text resources away from the GTK thread. Paths
/// come from the ZIP directory, are constrained to safe relative names, and
/// only XHTML/HTML/CSS are exposed in this first file-tree slice.
pub(crate) fn load_editable_resources(
    book_path: std::path::PathBuf,
    tx: async_channel::Sender<Result<Vec<EditableResource>, String>>,
) {
    std::thread::spawn(move || {
        let verdict = load_editable_resources_in_thread(&book_path);
        let _ = tx.send_blocking(verdict);
    });
}

fn load_editable_resources_in_thread(
    book_path: &std::path::Path,
) -> Result<Vec<EditableResource>, String> {
    use chapbook_core::Publication;

    let book = chapbook_epub::Book::open(book_path)
        .map_err(|_| "The book could not be opened to list its editable files.".to_string())?;
    let file = std::fs::File::open(book_path)
        .map_err(|e| format!("Could not open the EPUB file list: {e}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Could not read the EPUB file list: {e}"))?;
    let mut resources = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Could not inspect an EPUB file: {e}"))?;
        let href = entry.name().to_string();
        let lower = href.to_ascii_lowercase();
        let text_resource = lower.ends_with(".xhtml")
            || lower.ends_with(".html")
            || lower.ends_with(".htm")
            || lower.ends_with(".css");
        let safe_path = safe_editable_href(&href);
        let is_file = !entry.is_dir() && entry.enclosed_name().is_some();
        drop(entry);
        if !text_resource || !safe_path || !is_file || !seen.insert(href.clone()) {
            continue;
        }
        let spine_index = book.spine().iter().position(|item| {
            item.href == href
                || crate::epub_sanitizer::percent_decode_path(&item.href) == href
                || item.href == crate::epub_sanitizer::percent_decode_path(&href)
        });
        resources.push(EditableResource { href, spine_index });
    }
    resources.sort_by_key(|resource| resource.href.to_lowercase());
    Ok(resources)
}

/// Load one editable resource and its baseline off the UI thread. For spine
/// documents legacy text patches are reflected in the buffer; new writes are
/// still blocked until those old patches are reviewed.
pub(crate) fn load_source_resource(
    book_path: std::path::PathBuf,
    href: String,
    preview_chapter: usize,
    prior: Vec<PatchRecord>,
    tx: async_channel::Sender<Result<SourceEntry, String>>,
) {
    std::thread::spawn(move || {
        let verdict = load_source_resource_in_thread(
            &book_path,
            &href,
            preview_chapter,
            &prior,
        );
        let _ = tx.send_blocking(verdict);
    });
}

fn load_source_resource_in_thread(
    book_path: &std::path::Path,
    href: &str,
    preview_chapter: usize,
    prior: &[PatchRecord],
) -> Result<SourceEntry, String> {
    use chapbook_core::Publication;
    use std::io::Read as _;

    const MAX_EDITABLE_RESOURCE_BYTES: u64 = 32 * 1024 * 1024;
    if !safe_editable_href(href) {
        return Err("This EPUB file has an unsafe path and cannot be edited.".into());
    }
    let book = chapbook_epub::Book::open(book_path)
        .map_err(|_| "The book could not be opened to read this file.".to_string())?;
    let spine_item = book.spine().iter().find(|item| {
        item.href == href
            || crate::epub_sanitizer::percent_decode_path(&item.href) == href
            || item.href == crate::epub_sanitizer::percent_decode_path(href)
    });
    let patch_href = spine_item.map(|item| item.href.as_str()).unwrap_or(href);

    let file = std::fs::File::open(book_path)
        .map_err(|e| format!("Could not open the EPUB to read this file: {e}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("The EPUB archive could not be read: {e}"))?;
    let mut matches = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Could not inspect this EPUB file: {e}"))?;
        if entry.name() == href {
            matches.push(index);
        }
    }
    if matches.len() != 1 {
        return Err(if matches.is_empty() {
            format!("{href} is no longer present in the EPUB.")
        } else {
            format!("{href} appears more than once in the EPUB and cannot be edited safely.")
        });
    }
    let mut entry = archive
        .by_index(matches[0])
        .map_err(|e| format!("Could not read {href}: {e}"))?;
    if entry.size() > MAX_EDITABLE_RESOURCE_BYTES {
        return Err(format!(
            "{href} is too large to edit here (limit {} MiB).",
            MAX_EDITABLE_RESOURCE_BYTES / (1024 * 1024)
        ));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .by_ref()
        .take(MAX_EDITABLE_RESOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read {href}: {e}"))?;
    if bytes.len() as u64 > MAX_EDITABLE_RESOURCE_BYTES {
        return Err(format!("{href} is too large to edit safely."));
    }
    let (virtual_bytes, _) = crate::epub_patches::apply_text_patches(patch_href, &bytes, prior);
    let text = String::from_utf8(virtual_bytes).map_err(|_| {
        format!("{href} is not UTF-8 text, so it cannot be edited in the source pane.")
    })?;
    Ok(SourceEntry {
        hash: crate::epub_patches::hash_bytes(text.as_bytes()),
        text,
        href: href.to_string(),
        preview_chapter,
    })
}

/// One preview raster, already plain bytes: the pixmap's RGBA taken
/// out of the worker thread and handed to the main loop, which turns
/// it into a texture. No GTK type crosses threads — the tasks
/// manager's worker→main-loop shape.
#[derive(Debug, Clone)]
pub(crate) struct PreviewPixels {
    /// The debounce generation this raster answers; the pane drops
    /// anything older than its own.
    pub(crate) generation: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Premultiplied RGBA8888, row-major, stride `width * 4`.
    pub(crate) rgba: Vec<u8>,
}

/// Render the selected XHTML/HTML/CSS resource through a headless
/// session: edited bytes are substituted when that entry is read (including
/// external stylesheets), with pending patches applied to other entries.
/// One debounce tick = one open + a few pages — the engine has no
/// cache-invalidation API, so the preview simply does not share a cache.
// Ten inputs, one purpose — the same allow as the paragraph verifier
// above; they all travel to the same thread body together.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_source_preview(
    book_path: std::path::PathBuf,
    prefs: KalamPrefs,
    fonts_dir: Option<std::path::PathBuf>,
    href: String,
    text: String,
    prior: Vec<PatchRecord>,
    spine: usize,
    generation: u64,
    page: (u32, u32),
    tx: async_channel::Sender<Result<PreviewPixels, (u64, String)>>,
) {
    std::thread::spawn(move || {
        let verdict = render_source_preview_in_thread(
            &book_path, prefs, fonts_dir, &href, &text, &prior, spine, generation, page,
        )
        .map_err(|message| (generation, message));
        let _ = tx.send_blocking(verdict);
    });
}

#[allow(clippy::too_many_arguments)]
fn render_source_preview_in_thread(
    book_path: &std::path::Path,
    prefs: KalamPrefs,
    fonts_dir: Option<std::path::PathBuf>,
    href: &str,
    text: &str,
    prior: &[PatchRecord],
    spine: usize,
    generation: u64,
    page: (u32, u32),
) -> Result<PreviewPixels, String> {
    let prior = prior.to_vec();
    let edited_href = href.to_string();
    let decoded_edited_href = crate::epub_sanitizer::percent_decode_path(&edited_href);
    let edited = text.as_bytes().to_vec();
    let filter = kalam_reader::EntryFilter::new(move |entry_href, bytes| {
        let decoded_entry_href = crate::epub_sanitizer::percent_decode_path(entry_href);
        if entry_href == edited_href
            || entry_href == decoded_edited_href
            || decoded_entry_href == edited_href
            || decoded_entry_href == decoded_edited_href
        {
            return edited.clone();
        }
        let (patched, _) = crate::epub_patches::apply_text_patches(entry_href, &bytes, &prior);
        patched
    });
    let pixmap = kalam_reader::preview::render(
        book_path,
        &prefs,
        fonts_dir,
        kalam_reader::preview::Preview {
            filter,
            spine,
            start_page: 0,
            pages: kalam_reader::preview::PREVIEW_PAGES,
            page_width: page.0,
            page_height: page.1,
        },
    )
    .map_err(|err| format!("The preview could not be rendered:\n{err:#}"))?;
    let (width, height) = (pixmap.width(), pixmap.height());
    let rgba = pixmap.take();
    Ok(PreviewPixels {
        generation,
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(offset: u32) -> LayeredLocator {
        LayeredLocator {
            spine_href: "OEBPS/ch03.xhtml".into(),
            spine_index: 3,
            char_offset: offset,
            locator_version: LOCATOR_VERSION,
            quote: Quote {
                prefix: "the quick ".into(),
                exact: "brown".into(),
                suffix: " fox".into(),
            },
            spine_fraction: 0.25,
            book_progression: 0.1,
        }
    }

    #[test]
    fn a_range_survives_the_column() {
        let text = range_to_json(&sample(10), &sample(15));
        let (start, end) = range_from_json(&text).expect("parses");
        assert_eq!(start, sample(10));
        assert_eq!(end, sample(15));
    }

    #[test]
    fn legacy_rows_are_not_mistaken_for_locators() {
        assert!(range_from_json("").is_none());
        assert!(range_from_json("epubcfi(/6/4!/4/2/1:0)").is_none());
        assert!(range_from_json("{\"kalam_locator\":2}").is_none());
    }

    fn editor_test_epub(path: &std::path::Path) {
        use std::io::Write as _;
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let file = std::fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        let compressed =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("META-INF/container.xml", compressed).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
        )
        .unwrap();
        zip.start_file("OEBPS/content.opf", compressed).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">editor-test</dc:identifier><dc:title>Editor Test</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="chapter" href="text/chapter.xhtml" media-type="application/xhtml+xml"/>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="style" href="styles/main.css" media-type="text/css"/>
    <item id="cover" href="images/cover.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="chapter"/></spine>
</package>"#,
        )
        .unwrap();
        zip.start_file("OEBPS/text/chapter.xhtml", compressed)
            .unwrap();
        zip.write_all(b"<html><body><p>Chapter text.</p></body></html>")
            .unwrap();
        zip.start_file("OEBPS/nav.xhtml", compressed).unwrap();
        zip.write_all(
            br#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
  <head><title>Navigation</title></head>
  <body><nav epub:type="toc" id="toc"><h1>Contents</h1><ol>
    <li><a href="text/chapter.xhtml">Chapter</a></li>
  </ol></nav></body>
</html>"#,
        )
            .unwrap();
        zip.start_file("OEBPS/styles/main.css", compressed).unwrap();
        zip.write_all(b"body { color: navy; }").unwrap();
        zip.start_file("OEBPS/images/cover.png", compressed)
            .unwrap();
        zip.write_all(b"not-a-real-image").unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn editable_resource_paths_reject_encoded_traversal() {
        assert!(safe_editable_href("OEBPS/text/chapter.xhtml"));
        assert!(safe_editable_href("OEBPS/styles/main%20book.css"));
        assert!(!safe_editable_href("../outside.xhtml"));
        assert!(!safe_editable_href("OEBPS/%2e%2e/outside.xhtml"));
        assert!(!safe_editable_href("OEBPS%2f..%2foutside.css"));
    }

    #[test]
    fn editable_resource_tree_lists_xhtml_and_css_and_marks_spine_files() {
        let path = std::env::temp_dir().join(format!(
            "kalam-editor-resources-{}.epub",
            uuid::Uuid::new_v4()
        ));
        editor_test_epub(&path);
        let resources = load_editable_resources_in_thread(&path).unwrap();
        let by_href: std::collections::HashMap<_, _> = resources
            .iter()
            .map(|resource| (resource.href.as_str(), resource.spine_index))
            .collect();
        assert_eq!(by_href.get("OEBPS/text/chapter.xhtml"), Some(&Some(0)));
        assert_eq!(by_href.get("OEBPS/nav.xhtml"), Some(&None));
        assert_eq!(by_href.get("OEBPS/styles/main.css"), Some(&None));
        assert!(!by_href.contains_key("OEBPS/images/cover.png"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn source_loader_reads_a_spine_document_and_a_stylesheet() {
        let path = std::env::temp_dir().join(format!(
            "kalam-editor-source-{}.epub",
            uuid::Uuid::new_v4()
        ));
        editor_test_epub(&path);
        let chapter = load_source_resource_in_thread(
            &path,
            "OEBPS/text/chapter.xhtml",
            0,
            &[],
        )
        .unwrap();
        assert_eq!(chapter.preview_chapter, 0);
        assert!(chapter.text.contains("Chapter text."));
        let css = load_source_resource_in_thread(
            &path,
            "OEBPS/styles/main.css",
            0,
            &[],
        )
        .unwrap();
        assert_eq!(css.text, "body { color: navy; }");
        assert_eq!(css.href, "OEBPS/styles/main.css");
        let _ = std::fs::remove_file(path);
    }

    fn entry(word: &str) -> crate::db::EntryData {
        crate::db::EntryData {
            word: word.to_string(),
            senses: vec![
                crate::db::Sense {
                    number: 1,
                    pos: Some("noun".into()),
                    def: "a feeling".into(),
                    example: Some("an air of it".into()),
                },
                crate::db::Sense {
                    number: 2,
                    pos: None,
                    def: "a mood".into(),
                    example: Some(String::new()),
                },
            ],
            pos: vec!["noun".into(), "adjective".into()],
            synonyms: vec!["sorrow".into()],
            antonyms: vec!["joy".into()],
            idioms: vec![("in a mood".into(), "sad".into())],
            suggestions: Vec::new(),
        }
    }

    #[test]
    fn card_joins_word_level_pos_with_middots() {
        let card = DictCard::from_entry(&entry("melancholy"), Some("/m/".into()), false, None);
        assert_eq!(card.word, "melancholy");
        assert_eq!(card.pos.as_deref(), Some("noun \u{00b7} adjective"));
        assert_eq!(card.pronunciation.as_deref(), Some("/m/"));
    }

    #[test]
    fn card_pos_is_none_when_entry_has_no_pos() {
        let mut data = entry("x");
        data.pos.clear();
        let card = DictCard::from_entry(&data, None, false, None);
        assert!(card.pos.is_none());
    }

    #[test]
    fn card_maps_senses_drops_empty_examples_and_marks_hint() {
        let card = DictCard::from_entry(&entry("m"), None, false, Some(1));
        assert_eq!(card.senses.len(), 2);
        assert_eq!(card.senses[0].example.as_deref(), Some("an air of it"));
        assert!(!card.senses[0].hinted);
        assert_eq!(card.senses[1].example, None, "empty example is dropped");
        assert!(card.senses[1].hinted, "hint index lands on sense 1");
    }

    #[test]
    fn card_carries_synonyms_antonyms_idioms_and_saved() {
        let card = DictCard::from_entry(&entry("m"), None, true, None);
        assert_eq!(card.synonyms, vec!["sorrow".to_string()]);
        assert_eq!(card.antonyms, vec!["joy".to_string()]);
        assert_eq!(card.idioms, vec![("in a mood".into(), "sad".into())]);
        assert!(card.saved);
    }
}
