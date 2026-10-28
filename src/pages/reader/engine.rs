
//! The reading engine, wired to the reader page.
//!
//! This file replaces the old in-app browser page and its JS bridge. Every
//! line of JS the old reader shipped is now a method call on
//! [`ReaderView`], and everything the JS used to *send back* (progress,
//! taps, selections, links) arrives through the callbacks [`wire`]
//! installs.
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
use crate::db::{Annotation, AnnotationStyle, HighlightColor as DbColor};
use gtk::prelude::*;
use kalam_reader::{
    HighlightColor, KalamPrefs, KalamTheme, LayeredLocator, NewHighlight, Quote, ReaderOptions,
    ReaderPalette, ReaderView, ReadingMode, ReadingPosition, SelectedText, LOCATOR_VERSION,
};
use relm4::ComponentSender;
use std::path::Path;

// ---------------------------------------------------------------------
// Preferences: Kalam's stored values → the engine's
// ---------------------------------------------------------------------

/// The app's resolved theme as the engine's palette (decisions 19 and
/// 145): the page is painted in the palette's own `bg`/`text`/`accent`,
/// in whichever mode the app is in. Token values are `#rrggbb` literals
/// from `theme.rs`; anything else (a token table edited into a shape
/// this does not know) falls back per-channel rather than failing the
/// open — a wrong colour is a bug, a refused book is a disaster.
pub(crate) fn reader_palette(theme: &crate::theme::Theme) -> ReaderPalette {
    ReaderPalette {
        background: token_rgba(theme.bg, KalamTheme::Sepia.background()),
        foreground: token_rgba(theme.text, KalamTheme::Sepia.foreground()),
        accent: token_rgba(theme.accent, KalamTheme::Sepia.reader_palette().accent),
        dark: theme.dark,
    }
}

fn token_rgba(token: &str, fallback: kalam_reader::Rgba) -> kalam_reader::Rgba {
    kalam_reader::Rgba::from_hex(token, 255).unwrap_or(fallback)
}

// Nine knobs, one purpose: the persisted reading preferences the engine
// consumes. Grouping them into structs would only scatter what is plainly one
// call. Same allow as `settings_panel` and `db`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn engine_prefs(
    palette: ReaderPalette,
    font_px: u32,
    line_height: f32,
    column_px: u32,
    font_family: Option<String>,
    justify: bool,
    hyphenate: bool,
    publisher_styles: bool,
) -> KalamPrefs {
    KalamPrefs {
        palette,
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

/// Kalam's stored colour name → the engine's. Unknown names (and the
/// legacy "rose") fall back the same way `HighlightColor::from_str_lossy`
/// does: through Kalam's own parser first.
pub(crate) fn engine_color(name: &str) -> HighlightColor {
    HighlightColor::from_name(DbColor::from_str_lossy(name).as_str()).unwrap_or_default()
}

/// Open the book for reading with the reader's bundled and user fonts.
/// `Err` for a file the engine cannot read; the caller shows the existing
/// "Could not open book" placeholder.
pub(crate) fn open_engine(
    file_path: &Path,
    prefs: KalamPrefs,
) -> Result<ReaderView, kalam_reader::ChapbookError> {
    crate::timing::span("book_open");
    let options = ReaderOptions {
        fonts_dir: Some(crate::paths::fonts_dir()),
        ..ReaderOptions::default()
    };
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

/// Install the reader callbacks. Each one only *sends a message*; the
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

    let tx = sender.input_sender().clone();
    view.connect_selection_moved(move |rect| {
        let _ = tx.send(ReaderMsg::EngineSelectionMoved(rect.map(gdk_rect)));
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

    let tx = sender.input_sender().clone();
    view.connect_note_hover(move |id: Option<i64>, x: f64, y: f64| {
        let _ = tx.send(ReaderMsg::NoteDotHover(id, x, y));
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
        has_note: !annotation.note.trim().is_empty(),
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
    last_color: DbColor,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Popover {
    crate::icons::init();

    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(false);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Top);

    let mut anchor = *rect;
    anchor.set_y(anchor.y() - HANDLE_HEADROOM);
    anchor.set_height(anchor.height() + HANDLE_HEADROOM);
    popover.set_pointing_to(Some(&anchor));
    popover.add_css_class("kalam-ui-selection-popover");

    // The shared shell (decision 26): five one-tap colours, a divider,
    // Note, Define. Fix typo is the deferred editor's (spec §6), and
    // Copy stays Ctrl+C. The ring opens on the colour the last
    // highlight took.
    let tx = sender.input_sender().clone();
    let chip = crate::widgets::selection_chip::selection_chip_with_colors(
        DbColor::SOFT_FIVE,
        last_color,
        move |action| {
            use crate::widgets::selection_chip::SelectionChipAction;
            let _ = tx.send(match action {
                SelectionChipAction::Highlight(color) => {
                    ReaderMsg::HighlightSelection(color.as_str().to_string())
                }
                SelectionChipAction::Note => ReaderMsg::NoteSelection,
                SelectionChipAction::Define => ReaderMsg::LookUpSelection,
            });
        },
    );
    popover.set_child(Some(&chip.widget()));
    popover
}

/// The "Aa" a style sample wears, in the standing colour (decision
/// 29's four style samples): the solid band at its painted alpha, the
/// line styles at full strength — the same split the page itself uses.
fn style_sample_markup(style: AnnotationStyle, color: DbColor) -> String {
    let band = HighlightColor::from_name(color.as_str())
        .unwrap_or_default()
        .css();
    let line = &band[..band.len().min(7)];
    match style {
        AnnotationStyle::Solid => format!("<span background=\"{band}\">Aa</span>"),
        AnnotationStyle::Underline => {
            format!("<span underline=\"single\" underline_color=\"{line}\">Aa</span>")
        }
        AnnotationStyle::Squiggly => {
            format!("<span underline=\"error\" underline_color=\"{line}\">Aa</span>")
        }
        AnnotationStyle::Strikeout => "<s>Aa</s>".to_string(),
    }
}

/// One popover for one highlight (decision 29): colour dots, the four
/// style samples, the note box and delete. Colour and style land the
/// moment they are picked, so the page behind shows them; the note
/// (and anything left) autosaves when the popover closes. There is
/// no Save button to forget.
pub(crate) fn build_annotation_edit_popover(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    anno: &Annotation,
    focus_note: bool,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Popover {
    crate::icons::init();

    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Top);

    let mut anchor = *rect;
    anchor.set_y(anchor.y() - HANDLE_HEADROOM);
    anchor.set_height(anchor.height() + HANDLE_HEADROOM);
    popover.set_pointing_to(Some(&anchor));
    popover.add_css_class("kalam-reader-annotation-popover");

    let id = anno.id;
    let cur_color = std::rc::Rc::new(std::cell::Cell::new(DbColor::from_str_lossy(&anno.color)));
    let cur_style =
        std::rc::Rc::new(std::cell::Cell::new(AnnotationStyle::from_str_lossy(&anno.style)));
    let deleted = std::rc::Rc::new(std::cell::Cell::new(false));

    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("kalam-reader-annotation-editor");

    // The five colours.
    let colors_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mut dots: Vec<(DbColor, gtk::Button)> = Vec::new();
    for color in DbColor::SOFT_FIVE {
        let dot = gtk::Button::new();
        dot.add_css_class("kalam-reader-color-dot");
        dot.add_css_class(&format!("kalam-reader-dot-{}", color.as_str()));
        if *color == cur_color.get() {
            dot.add_css_class("active");
        }
        dot.set_tooltip_text(Some(&format!("Highlight {}", color.as_str())));
        dot.set_valign(gtk::Align::Center);
        colors_row.append(&dot);
        dots.push((*color, dot));
    }
    card.append(&colors_row);
    let dots = std::rc::Rc::new(dots);

    // The four styles, as samples wearing the standing colour.
    let styles_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let mut samples: Vec<(AnnotationStyle, gtk::Button, gtk::Label)> = Vec::new();
    for style in AnnotationStyle::ALL {
        let label = gtk::Label::new(None);
        label.set_markup(&style_sample_markup(*style, cur_color.get()));
        label.add_css_class("kalam-reader-style-sample-text");
        let btn = gtk::Button::new();
        btn.add_css_class("kalam-reader-style-sample");
        if *style == cur_style.get() {
            btn.add_css_class("active");
        }
        btn.set_child(Some(&label));
        btn.set_tooltip_text(Some(style.label()));
        btn.set_hexpand(true);
        styles_row.append(&btn);
        samples.push((*style, btn, label));
    }
    card.append(&styles_row);
    let samples = std::rc::Rc::new(samples);

    // Picking a colour or a style restyles at once.
    let restyle = {
        let tx = sender.input_sender().clone();
        let cur_color = cur_color.clone();
        let cur_style = cur_style.clone();
        std::rc::Rc::new(move || {
            let _ = tx.send(ReaderMsg::RestyleAnnotation {
                id,
                color: cur_color.get().as_str().to_string(),
                style: cur_style.get().as_str().to_string(),
            });
        })
    };
    for (color, dot) in dots.iter() {
        let c = *color;
        let cur_color = cur_color.clone();
        let dots = dots.clone();
        let samples = samples.clone();
        let restyle = restyle.clone();
        dot.connect_clicked(move |_| {
            cur_color.set(c);
            for (dot_c, btn) in dots.iter() {
                if *dot_c == c {
                    btn.add_css_class("active");
                } else {
                    btn.remove_css_class("active");
                }
            }
            for (style, _, label) in samples.iter() {
                label.set_markup(&style_sample_markup(*style, c));
            }
            restyle();
        });
    }
    for (style, btn, _) in samples.iter() {
        let st = *style;
        let cur_style = cur_style.clone();
        let samples = samples.clone();
        let restyle = restyle.clone();
        btn.connect_clicked(move |_| {
            cur_style.set(st);
            for (btn_s, b, _) in samples.iter() {
                if *btn_s == st {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
            restyle();
        });
    }

    // The note box.
    let note_view = gtk::TextView::new();
    note_view.add_css_class("kalam-reader-note-view");
    note_view.set_wrap_mode(gtk::WrapMode::WordChar);
    note_view.set_hexpand(true);
    let buffer = note_view.buffer();
    if !anno.note.is_empty() {
        buffer.set_text(&anno.note);
    }
    let note_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_width(280)
        .min_content_height(64)
        .max_content_height(120)
        .hexpand(true)
        .child(&note_view)
        .build();
    note_scroll.add_css_class("kalam-reader-note-scroll");
    card.append(&note_scroll);

    // Delete, alone in the footer: everything else already saved.
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let del_btn = gtk::Button::new();
    del_btn.add_css_class("kalam-reader-annotation-delete");
    del_btn.set_child(Some(&crate::icons::labelled(
        "user-trash-symbolic",
        14,
        "Delete",
        6,
    )));
    del_btn.set_tooltip_text(Some("Delete this highlight"));
    {
        let tx = sender.input_sender().clone();
        let deleted = deleted.clone();
        // Weakly: the button lives inside the popover, so a strong
        // handle here would keep the whole tree alive for good.
        let pop = popover.downgrade();
        del_btn.connect_clicked(move |_| {
            deleted.set(true);
            let _ = tx.send(ReaderMsg::DeleteAnnotation(id));
            if let Some(pop) = pop.upgrade() {
                pop.popdown();
            }
        });
    }
    footer.append(&del_btn);
    card.append(&footer);

    popover.set_child(Some(&card));

    // Autosave on close: whatever the popover holds when it goes away
    // is the annotation, unless Delete already took it.
    {
        let tx = sender.input_sender().clone();
        let cur_color = cur_color.clone();
        let cur_style = cur_style.clone();
        let buffer = buffer.clone();
        popover.connect_closed(move |_| {
            if deleted.get() {
                return;
            }
            let note = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            let _ = tx.send(ReaderMsg::AnnotationEditClosed {
                id,
                color: cur_color.get().as_str().to_string(),
                style: cur_style.get().as_str().to_string(),
                note,
            });
        });
    }

    if focus_note {
        let view = note_view.clone();
        let buffer = buffer.clone();
        gtk::glib::idle_add_local_once(move || {
            buffer.place_cursor(&buffer.end_iter());
            view.grab_focus();
        });
    }

    popover
}

/// The saved-word hover card (decision 33). Only words Kalam keeps are
/// ever hovered, so the card says so — headword, the saved tick, and
/// every meaning the entry has, not just the first. It is a reminder,
/// not a lookup: no chips, no buttons; a tap opens the full card.
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
    if data.senses.is_empty() {
        return None;
    }

    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Top);
    popover.set_pointing_to(Some(rect));
    popover.add_css_class("k-word-preview-popover");

    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("k-word-preview-card");
    card.set_size_request(300, -1);

    let head_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let head = gtk::Label::new(Some(&data.word));
    head.add_css_class("k-word-preview-head");
    head.set_xalign(0.0);
    head.set_hexpand(true);
    head_row.append(&head);
    let tick = gtk::Image::from_icon_name("object-select-symbolic");
    tick.add_css_class("k-word-preview-tick");
    tick.set_valign(gtk::Align::Center);
    head_row.append(&tick);
    let saved = gtk::Label::new(Some("Saved"));
    saved.add_css_class("k-word-preview-saved");
    saved.set_valign(gtk::Align::Center);
    head_row.append(&saved);
    card.append(&head_row);

    let senses_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    for (i, sense) in data.senses.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let num = gtk::Label::new(Some(&format!("{}.", i + 1)));
        num.add_css_class("k-def-num");
        num.set_valign(gtk::Align::Start);
        row.append(&num);
        let def = gtk::Label::new(Some(&sense.def));
        def.add_css_class("k-word-preview-def");
        def.set_wrap(true);
        def.set_xalign(0.0);
        def.set_halign(gtk::Align::Fill);
        def.set_hexpand(true);
        row.append(&def);
        senses_box.append(&row);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(true)
        .overlay_scrolling(true)
        .max_content_height(220)
        .build();
    scroll.set_child(Some(&senses_box));
    card.append(&scroll);

    popover.set_child(Some(&card));
    Some(popover)
}

/// The note itself, read where it hangs (decision 29): hovering a
/// highlight's end dot shows what was written, nothing to edit and
/// nothing to click. It borrows the saved-word preview's card, so
/// the two hovers read as the same kind of glance.
pub(crate) fn build_annotation_note_preview(
    host: &gtk::Widget,
    rect: &gtk::gdk::Rectangle,
    note: &str,
) -> Option<gtk::Popover> {
    let note = note.trim();
    if note.is_empty() {
        return None;
    }

    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Top);
    popover.set_pointing_to(Some(rect));
    popover.add_css_class("k-word-preview-popover");

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("k-word-preview-card");
    let width = 300.min((host.width() - 32).max(240));
    card.set_size_request(width, -1);

    let text = gtk::Label::new(Some(note));
    text.add_css_class("k-word-preview-def");
    text.set_wrap(true);
    text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    text.set_xalign(0.0);
    text.set_halign(gtk::Align::Fill);
    text.set_hexpand(true);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(true)
        .overlay_scrolling(true)
        .max_content_height(220)
        .build();
    scroll.set_child(Some(&text));
    card.append(&scroll);

    popover.set_child(Some(&card));
    Some(popover)
}

const HANDLE_HEADROOM: i32 = 8;
/// One sense as the card draws it. `pos` rides along so the card can
/// group senses under part-of-speech dividers (decision 31) instead
/// of numbering straight through the whole entry.
pub(crate) struct DictSense {
    pub(crate) pos: Option<String>,
    pub(crate) def: String,
    pub(crate) example: Option<String>,
}

/// What the dictionary card shows — the same fields the JS popup was fed,
/// flattened to plain strings so this file does not depend on the exact
/// shape of `EntryData`'s optional fields.
pub(crate) struct DictCard {
    pub(crate) word: String,
    pub(crate) pronunciation: Option<String>,
    pub(crate) senses: Vec<DictSense>,
    pub(crate) synonyms: Vec<String>,
    pub(crate) antonyms: Vec<String>,
    pub(crate) idioms: Vec<(String, String)>,
    pub(crate) suggestions: Vec<String>,
    pub(crate) saved: bool,
    /// The reader wandered here through the card's own chips, so the
    /// header grows a way back (decision 31).
    pub(crate) can_go_back: bool,
}

impl DictCard {
    /// `Sense.example` is an `Option<String>` in this tree, so the empty
    /// filter applies to it directly; wrapping it in `Option::<String>::from`
    /// is a useless conversion, which `-D warnings` rejects.
    pub(crate) fn from_entry(
        data: &crate::db::EntryData,
        pronunciation: Option<String>,
        saved: bool,
        can_go_back: bool,
    ) -> DictCard {
        let senses: Vec<DictSense> = data
            .senses
            .iter()
            .map(|s| DictSense {
                pos: s.pos.clone(),
                def: s.def.clone(),
                example: s.example.clone().filter(|e| !e.is_empty()),
            })
            .collect();
        DictCard {
            word: data.word.clone(),
            pronunciation,
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
            can_go_back,
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

/// The card's head (decision 31): back arrow when the reader wandered
/// here, Playfair headword with its IPA, the speaker, and the circular
/// save toggle — ring and plus while unsaved, filled and ticked once
/// the word is kept. Parts of speech no longer ride here; they divide
/// the senses below.
fn dict_header(
    _host: &gtk::Widget,
    card: &DictCard,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Box {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    header.add_css_class("k-header");

    if card.can_go_back {
        let back = gtk::Button::new();
        back.add_css_class("k-back-btn");
        back.set_child(Some(&gtk::Image::from_icon_name(
            "go-previous-symbolic",
        )));
        back.set_valign(gtk::Align::Center);
        back.set_tooltip_text(Some("Back to the word you came from"));
        let tx = sender.input_sender().clone();
        back.connect_clicked(move |_| {
            let _ = tx.send(ReaderMsg::DictGoBack);
        });
        header.append(&back);
    }

    let left = gtk::Box::new(gtk::Orientation::Vertical, 2);
    left.add_css_class("k-header-left");
    left.set_hexpand(true);
    left.set_valign(gtk::Align::Center);

    let word = gtk::Label::new(Some(&card.word));
    word.add_css_class("k-word");
    word.set_xalign(0.0);
    word.set_halign(gtk::Align::Fill);
    word.set_wrap(true);
    left.append(&word);

    if let Some(pronunciation) = &card.pronunciation {
        let pron = gtk::Label::new(Some(pronunciation));
        pron.add_css_class("k-pronunciation");
        pron.set_xalign(0.0);
        pron.set_halign(gtk::Align::Start);
        left.append(&pron);
    }
    header.append(&left);

    // Audio pronunciation — best-effort via a system speech engine.
    let audio = gtk::Button::new();
    audio.add_css_class("k-audio-btn");
    audio.set_child(Some(&gtk::Image::from_icon_name(
        "audio-volume-high-symbolic",
    )));
    audio.set_valign(gtk::Align::Center);
    audio.set_tooltip_text(Some("Hear it"));
    {
        let w = card.word.clone();
        audio.connect_clicked(move |_| speak_word(w.clone()));
    }
    header.append(&audio);

    // The save toggle flips its own face on the tap and lets the model
    // land the same answer in the store; the popover stays up.
    let save = gtk::Button::new();
    save.add_css_class("k-save-btn");
    let icon = gtk::Image::from_icon_name("list-add-symbolic");
    save.set_child(Some(&icon));
    save.set_valign(gtk::Align::Center);
    save.set_sensitive(!card.senses.is_empty());
    let apply = |btn: &gtk::Button, icon: &gtk::Image, saved: bool| {
        if saved {
            icon.set_icon_name(Some("object-select-symbolic"));
            btn.add_css_class("saved");
            btn.set_tooltip_text(Some("Saved — tap to forget"));
        } else {
            icon.set_icon_name(Some("list-add-symbolic"));
            btn.remove_css_class("saved");
            btn.set_tooltip_text(Some("Save word"));
        }
    };
    apply(&save, &icon, card.saved);
    let is_saved = std::rc::Rc::new(std::cell::Cell::new(card.saved));
    {
        let tx = sender.input_sender().clone();
        let icon = icon.clone();
        save.connect_clicked(move |btn| {
            let next = !is_saved.get();
            is_saved.set(next);
            apply(btn, &icon, next);
            let _ = tx.send(ReaderMsg::ToggleSaveWord);
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

    let def = gtk::Label::new(Some(&sense.def));
    def.add_css_class("k-def-text");
    def.set_wrap(true);
    def.set_xalign(0.0);
    def.set_halign(gtk::Align::Fill);
    text_box.append(&def);

    if let Some(example) = &sense.example {
        let ex = gtk::Label::new(Some(example));
        ex.add_css_class("k-def-example");
        ex.set_wrap(true);
        ex.set_xalign(0.0);
        ex.set_halign(gtk::Align::Fill);
        text_box.append(&ex);
    }
    row.append(&text_box);
    row
}

/// Senses grouped by part of speech (decision 31): WordNet's four in
/// their usual order, anything else first-seen, senses with no part of
/// speech last — that group gets no divider row at all.
fn pos_groups(senses: &[DictSense]) -> Vec<(Option<String>, Vec<&DictSense>)> {
    const ORDER: [&str; 4] = ["noun", "adjective", "verb", "adverb"];
    let mut names: Vec<Option<String>> = Vec::new();
    for sense in senses {
        if !names.contains(&sense.pos) {
            names.push(sense.pos.clone());
        }
    }
    names.sort_by_key(|name| match name {
        Some(pos) => ORDER.iter().position(|o| o == pos).unwrap_or(ORDER.len()),
        None => ORDER.len() + 1,
    });
    names
        .into_iter()
        .map(|name| {
            let group = senses.iter().filter(|s| s.pos == name).collect();
            (name, group)
        })
        .collect()
}

fn pos_divider(name: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("k-pos-divider");
    let label = gtk::Label::new(Some(&name.to_uppercase()));
    label.add_css_class("k-pos-name");
    label.set_valign(gtk::Align::Center);
    row.append(&label);
    let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
    rule.set_hexpand(true);
    rule.set_valign(gtk::Align::Center);
    row.append(&rule);
    row
}

fn append_definitions(body: &gtk::Box, card: &DictCard) {
    let groups = pos_groups(&card.senses);
    // Senses that carry no part of speech at all keep the plain heading;
    // once groups are named, the dividers speak for themselves.
    if groups.iter().all(|(name, _)| name.is_none()) {
        body.append(&section_label("Definitions"));
    }

    let defs_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    defs_box.add_css_class("k-defs");

    for (name, group) in &groups {
        if let Some(name) = name {
            defs_box.append(&pos_divider(name));
        }
        // Three senses to a group, then the fold (decision 31);
        // numbering restarts with each part of speech.
        let shown = group.len().min(3);
        for (i, sense) in group.iter().take(shown).enumerate() {
            defs_box.append(&sense_row(sense, i + 1));
        }
        let extra = &group[shown..];
        if !extra.is_empty() {
            let hidden = gtk::Box::new(gtk::Orientation::Vertical, 10);
            hidden.add_css_class("k-extra-defs");
            for (offset, sense) in extra.iter().enumerate() {
                hidden.append(&sense_row(sense, shown + offset + 1));
            }

            let revealer = gtk::Revealer::new();
            // SlideDown, not None: GTK measures a None/Crossfade
            // revealer at its child's full height even while hidden
            // (`get_child_size_scale` returns 1.0 for those types),
            // which left a blank gap above "Show N more". A vertical
            // slide scales the measured height by the reveal
            // position, so the folded senses take no room at all.
            revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
            revealer.set_transition_duration(180);
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
            phrase_label.set_halign(gtk::Align::Fill);
            phrase_label.set_wrap(true);
            item.append(&phrase_label);
            let def_label = gtk::Label::new(Some(def));
            def_label.add_css_class("k-idiom-def");
            def_label.set_xalign(0.0);
            def_label.set_halign(gtk::Align::Fill);
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
        empty.set_halign(gtk::Align::Fill);
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
            synonyms: vec!["sorrow".into()],
            antonyms: vec!["joy".into()],
            idioms: vec![("in a mood".into(), "sad".into())],
            suggestions: Vec::new(),
        }
    }

    #[test]
    fn card_carries_word_pronunciation_and_the_way_back() {
        let card = DictCard::from_entry(&entry("melancholy"), Some("/m/".into()), false, true);
        assert_eq!(card.word, "melancholy");
        assert_eq!(card.pronunciation.as_deref(), Some("/m/"));
        assert!(card.can_go_back);
    }

    #[test]
    fn card_maps_senses_with_pos_and_drops_empty_examples() {
        let card = DictCard::from_entry(&entry("m"), None, false, false);
        assert_eq!(card.senses.len(), 2);
        assert_eq!(card.senses[0].pos.as_deref(), Some("noun"));
        assert_eq!(card.senses[0].example.as_deref(), Some("an air of it"));
        assert_eq!(card.senses[1].pos, None);
        assert_eq!(card.senses[1].example, None, "empty example is dropped");
    }

    #[test]
    fn pos_groups_orders_wordnets_four_and_lands_unnamed_last() {
        let senses = vec![
            DictSense { pos: Some("verb".into()), def: "v".into(), example: None },
            DictSense { pos: None, def: "bare".into(), example: None },
            DictSense { pos: Some("noun".into()), def: "n1".into(), example: None },
            DictSense { pos: Some("noun".into()), def: "n2".into(), example: None },
        ];
        let groups = pos_groups(&senses);
        let shape: Vec<(Option<&str>, usize)> = groups
            .iter()
            .map(|(name, group)| (name.as_deref(), group.len()))
            .collect();
        assert_eq!(shape, vec![(Some("noun"), 2), (Some("verb"), 1), (None, 1)]);
    }

    #[test]
    fn card_carries_synonyms_antonyms_idioms_and_saved() {
        let card = DictCard::from_entry(&entry("m"), None, true, false);
        assert_eq!(card.synonyms, vec!["sorrow".to_string()]);
        assert_eq!(card.antonyms, vec!["joy".to_string()]);
        assert_eq!(card.idioms, vec![("in a mood".into(), "sad".into())]);
        assert!(card.saved);
    }
}
