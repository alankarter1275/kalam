//! Generated, missing, finished and file-missing cover states (§20).
//!
//! Image decoding and its cache stay in `book_row.rs`. This module owns the
//! faces Kalam shows when there is no usable image—or when a real image
//! needs a state badge/treatment—and passes the generated face into the
//! existing synchronous/deferred cover paths as their fallback.

use super::book_row::{cover_widget_deferred_or_else, cover_widget_or_else};
use super::pill_button::icon_button;
use super::surface::state_card;
use crate::models::Book;
use gtk::prelude::*;
use std::path::Path;

/// Whether the book file behind a cover is available. This is deliberately
/// independent of `finished`: state composition belongs to the page, not to
/// a chain of mutually exclusive cover builders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverAvailability {
    Available,
    FileMissing,
}

/// Plain data needed to choose and build a cover face.
#[derive(Debug, Clone, Copy)]
pub struct CoverSpec<'a> {
    pub path: Option<&'a Path>,
    pub title: &'a str,
    pub author: &'a str,
    pub kind: &'a str,
    pub width: i32,
    pub height: i32,
    pub finished: bool,
    pub availability: CoverAvailability,
    pub deferred: bool,
}

impl<'a> CoverSpec<'a> {
    pub fn from_book(
        book: &'a Book,
        width: i32,
        height: i32,
        finished: bool,
        availability: CoverAvailability,
        deferred: bool,
    ) -> Self {
        Self {
            path: book.cover_path.as_deref(),
            title: &book.title,
            author: book.authors_display(),
            kind: book.format.as_str(),
            width,
            height,
            finished,
            availability,
            deferred,
        }
    }
}

/// Build a cover from a complete spec: a real image when one decodes, the
/// generated neutral face when it does not, then any finished/file-missing
/// treatment the spec asks for.
pub fn cover(spec: CoverSpec<'_>) -> gtk::Widget {
    let width = spec.width.max(1);
    let height = spec.height.max(1);
    let title = spec.title.to_owned();
    let author = spec.author.to_owned();
    let kind = spec.kind.to_owned();

    let base = if spec.deferred {
        cover_widget_deferred_or_else(spec.path, width, height, move || {
            fallback_cover(&title, &author, &kind, width, height).upcast::<gtk::Widget>()
        })
    } else {
        cover_widget_or_else(spec.path, width, height, move || {
            fallback_cover(&title, &author, &kind, width, height).upcast::<gtk::Widget>()
        })
    };

    let stated = match spec.availability {
        CoverAvailability::Available => base,
        CoverAvailability::FileMissing => file_missing_cover(base).upcast::<gtk::Widget>(),
    };
    cover_with_finished_badge(&stated, spec.finished).upcast::<gtk::Widget>()
}

/// Book-shaped convenience over [`cover`] for the common page call.
pub fn book_cover(
    book: &Book,
    width: i32,
    height: i32,
    finished: bool,
    availability: CoverAvailability,
    deferred: bool,
) -> gtk::Widget {
    cover(CoverSpec::from_book(
        book,
        width,
        height,
        finished,
        availability,
        deferred,
    ))
}

/// The generated no-cover face: neutral card fill, kind/title/author at the
/// bottom, and deliberately no palette accent tint.
pub fn fallback_cover(title: &str, author: &str, kind: &str, width: i32, height: i32) -> gtk::Box {
    let width = width.max(1);
    let height = height.max(1);
    let compact = cover_is_compact(width, height);

    let cover = gtk::Box::new(gtk::Orientation::Vertical, 6);
    cover.add_css_class("kalam-ui-cover-fallback");
    if compact {
        cover.add_css_class("compact");
    }
    cover.set_size_request(width, height);
    cover.set_hexpand(false);
    cover.set_vexpand(false);
    cover.set_overflow(gtk::Overflow::Hidden);

    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    cover.append(&spacer);

    let kind_label = gtk::Label::new(Some(&display_text(kind, "BOOK").to_ascii_uppercase()));
    kind_label.add_css_class("kalam-ui-cover-kind");
    kind_label.set_halign(gtk::Align::Start);
    kind_label.set_xalign(0.0);
    kind_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    cover.append(&kind_label);

    let title_label = gtk::Label::new(Some(display_text(title, "Untitled")));
    title_label.add_css_class("kalam-ui-cover-title");
    title_label.set_halign(gtk::Align::Start);
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.set_lines(if compact { 3 } else { 4 });
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    cover.append(&title_label);

    let author_label = gtk::Label::new(Some(display_text(author, "Unknown")));
    author_label.add_css_class("kalam-ui-cover-author");
    author_label.set_halign(gtk::Align::Start);
    author_label.set_xalign(0.0);
    author_label.set_wrap(true);
    author_label.set_lines(2);
    author_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    cover.append(&author_label);

    cover
}

/// The dashed face for a work that is not in the library.
pub fn missing_cover(title: &str, width: i32, height: i32) -> gtk::Box {
    missing_cover_inner(title, width, height, None)
}

/// The missing face with §28's centred fetch affordance below its caption.
pub fn missing_cover_with_fetch(
    title: &str,
    width: i32,
    height: i32,
    on_fetch: impl Fn() + 'static,
) -> gtk::Box {
    let fetch = fetch_cover_button(title, true, on_fetch);
    missing_cover_inner(title, width, height, Some(fetch))
}

/// The 36px circular fetch button used inside a dashed missing cover.
pub fn fetch_cover_button(
    title: &str,
    enabled: bool,
    on_fetch: impl Fn() + 'static,
) -> gtk::Button {
    let label = format!("Fetch {}", display_text(title, "this book"));
    let button = icon_button("list-add-symbolic", &label, 36, true, on_fetch);
    button.add_css_class("kalam-ui-fetch-cover");
    button.set_halign(gtk::Align::Center);
    button.set_valign(gtk::Align::Center);
    button.set_sensitive(enabled);
    if enabled {
        button.set_tooltip_text(Some(&label));
    } else {
        button.set_tooltip_text(Some("Online sources are not available yet"));
    }
    button
}

/// The 22px finished tick, positioned 8px from a cover's top-right corner
/// when placed by [`cover_with_finished_badge`].
pub fn finished_badge() -> gtk::Label {
    let badge = gtk::Label::new(Some("✓"));
    badge.add_css_class("kalam-ui-finished-badge");
    badge.set_size_request(22, 22);
    badge.set_halign(gtk::Align::End);
    badge.set_valign(gtk::Align::Start);
    badge.set_margin_top(crate::style::spacing::XS);
    badge.set_margin_end(crate::style::spacing::XS);
    badge.set_xalign(0.5);
    badge.set_yalign(0.5);
    badge
}

/// Overlay the finished badge on any cover-shaped widget.
pub fn cover_with_finished_badge(
    cover: &impl IsA<gtk::Widget>,
    finished: bool,
) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.set_overflow(gtk::Overflow::Hidden);
    overlay.set_halign(gtk::Align::Start);
    overlay.set_valign(gtk::Align::Start);
    overlay.set_child(Some(cover.as_ref()));
    if finished {
        overlay.add_overlay(&finished_badge());
    }
    overlay
}

/// Apply the book page's file-missing treatment: the cover at 45% opacity,
/// with the dashed state card directly beneath it.
pub fn file_missing_cover(cover: gtk::Widget) -> gtk::Box {
    cover.set_opacity(0.45);

    let stack = gtk::Box::new(
        gtk::Orientation::Vertical,
        crate::style::spacing::SM,
    );
    stack.set_halign(gtk::Align::Start);
    stack.set_valign(gtk::Align::Start);
    stack.append(&cover);
    stack.append(&state_card(
        "File not found",
        "This book's file could not be found.",
    ));
    stack
}

fn missing_cover_inner(
    title: &str,
    width: i32,
    height: i32,
    fetch: Option<gtk::Button>,
) -> gtk::Box {
    let width = width.max(1);
    let height = height.max(1);
    let compact = cover_is_compact(width, height);

    let cover = gtk::Box::new(gtk::Orientation::Vertical, 6);
    cover.add_css_class("kalam-ui-cover-missing");
    if compact {
        cover.add_css_class("compact");
    }
    cover.set_size_request(width, height);
    cover.set_hexpand(false);
    cover.set_vexpand(false);
    cover.set_overflow(gtk::Overflow::Hidden);
    cover.set_tooltip_text(Some(display_text(title, "Untitled")));

    let top_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    top_spacer.set_vexpand(true);
    cover.append(&top_spacer);

    // A 56×84 fan cover has room for the dashed state and a plus, not two
    // lines of copy squeezed into illegibility. The title remains available
    // as the widget tooltip and in the surrounding row/page copy.
    if width < 80 || height < 110 {
        let plus = gtk::Label::new(Some("+"));
        plus.add_css_class("kalam-ui-cover-missing-caption");
        plus.set_halign(gtk::Align::Center);
        plus.set_valign(gtk::Align::Center);
        cover.append(&plus);
    } else {
        let title_label = gtk::Label::new(Some(display_text(title, "Untitled")));
        title_label.add_css_class("kalam-ui-cover-missing-title");
        title_label.set_halign(gtk::Align::Center);
        title_label.set_xalign(0.5);
        title_label.set_wrap(true);
        title_label.set_justify(gtk::Justification::Center);
        title_label.set_lines(3);
        title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        cover.append(&title_label);

        let caption = gtk::Label::new(Some("Not in library"));
        caption.add_css_class("kalam-ui-cover-missing-caption");
        caption.set_halign(gtk::Align::Center);
        caption.set_xalign(0.5);
        cover.append(&caption);
    }

    if let Some(fetch) = fetch {
        cover.append(&fetch);
    }

    let bottom_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom_spacer.set_vexpand(true);
    cover.append(&bottom_spacer);
    cover
}

fn cover_is_compact(width: i32, height: i32) -> bool {
    width < 110 || height < 150
}

fn display_text<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{cover_is_compact, display_text};

    #[test]
    fn blank_cover_copy_has_stable_fallbacks() {
        assert_eq!(display_text("  Dune  ", "Untitled"), "Dune");
        assert_eq!(display_text("   ", "Untitled"), "Untitled");
    }

    #[test]
    fn fan_and_thumbnail_covers_use_the_compact_face() {
        assert!(cover_is_compact(56, 84));
        assert!(cover_is_compact(100, 180));
        assert!(!cover_is_compact(180, 270));
    }
}
