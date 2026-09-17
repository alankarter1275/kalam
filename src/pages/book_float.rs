//! Floating book detail — compact book-first panel.
//!
//! Cover and reading progress live on the left; story info and actions live on
//! the right. Clicking the cover opens the full book page. Q / Esc closes.
//!
//! **Fixed size, deliberately.** `set_size_request(720, 420)` in `app.rs` is a
//! *floor*, not a size — GTK grows a widget past its request whenever content
//! needs the room. Every variable-length child here is therefore bounded on
//! purpose: the tag row scrolls, the title wraps to two lines and then
//! ellipsises, the series and fact labels ellipsise, the author line is
//! clipped, and both side columns clip their overflow. Removing any one of
//! those bounds brings back "the panel changes size depending on the book".
//!
//! **No "Read more".** The description box is a fixed size and the full text
//! is always in it; long descriptions scroll. The old expand/collapse toggle
//! was removed after it turned out to work backwards — see the comment in
//! `fill()`. A scroll wheel is a better answer than a button that changes the
//! shape of a panel whose whole point is that it does not change shape.
//!
//! Two traps in particular, both learned the hard way:
//!
//! - **Ellipsising a label does not stop it widening its parent.** The label
//!   still reports the full string as its natural width. `max_width_chars` is
//!   what actually caps it; ellipsize only decides how the overflow is drawn.
//! - **Exactly one child carries `vexpand`** — `desc_section`. It absorbs all
//!   the leftover height, which both keeps the tag row and action buttons
//!   pinned to the bottom and hands the spare space to the description instead
//!   of wasting it on a blank gap.
//!
//! See `docs/pitfalls.md` §3 and §4.

use crate::db::Catalog;
use crate::models::Book;
use crate::pages::metadata_editor::open_metadata_editor;
use crate::service::LibraryService;
use crate::widgets::book_row::{cover_widget, invalidate_cover_cache};
use crate::widgets::charts::star_picker;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

/// Height of the tag row. No scrollbar is drawn (the scroller uses
/// `PolicyType::External`), so this is just chip height plus breathing room.
/// Fixed on purpose, and the row is never hidden: it reserves the space that
/// keeps the action buttons below it in the same place for every book.
const TAGS_ROW_H: i32 = 26;
/// Height of the author line. One row of author links; extra authors are
/// clipped rather than allowed to grow the panel.
const AUTHOR_ROW_H: i32 = 22;

const COVER_W: i32 = 120;
const COVER_H: i32 = 176;
/// Floor for the description block. Only a floor: the section carries
/// `vexpand`, so it takes whatever the rest of the panel does not use — which
/// is how a one-line title donates its spare row to the description instead of
/// changing the panel's height.
///
/// There is no "preview" height and no character limit any more. The whole
/// description is always in the box and long ones simply scroll; see the
/// module doc.
const DESC_SECTION_HEIGHT: i32 = 182;
/// The title wraps to at most this many lines, then ellipsises.
const TITLE_MAX_LINES: i32 = 2;
/// Caps the natural width of the fact values in the left column (publisher,
/// published, format, progress). Same trap as the title: these labels wrap,
/// and a wrapping label with no cap still asks for its whole text on one line,
/// so a long publisher name widened the cover column and with it the float.
const FACT_MAX_CHARS: i32 = 16;
/// Caps the title's *natural* width. Without this a long title reports its
/// whole length as the width it wants and widens the float, because the 720px
/// in `app.rs` is a floor rather than a fixed size. See `docs/pitfalls.md` §3.
const TITLE_MAX_CHARS: i32 = 30;

#[derive(Debug)]
pub enum BookFloatOut {
    Close,
    OpenFullPage {
        book_id: i64,
    },
    OpenReader {
        book_id: i64,
    },
    OpenAuthor {
        name: String,
    },
    /// Open the shelves checklist panel (in-app float).
    ShowShelves,
    Deleted {
        #[allow(dead_code)]
        book_id: i64,
    },
}

#[derive(Debug)]
pub enum BookFloatMsg {
    Close,
    OpenFull,
    Read,
    OpenAuthor(String),
    Remove,
    ToggleReadingList,
    ToggleFinished,
    SetRating(u8),
    EditMetadata,
    ShowShelfMenu,
    Refresh,
}

pub struct BookFloatModel {
    service: LibraryService,
    book: Option<Book>,
    in_reading_list: bool,
    finished: bool,
}

#[relm4::component(pub)]
impl Component for BookFloatModel {
    type Init = (Arc<Catalog>, i64);
    type Input = BookFloatMsg;
    type Output = BookFloatOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "kalam-float",
            set_overflow: gtk::Overflow::Hidden,
            set_hexpand: true,
            set_vexpand: true,

            // `Overflow::Hidden` is the backstop for this column: the labels
            // below are capped individually, but clipping here means anything
            // added later cannot silently widen the float either.
            #[name = "cover_col"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-float-cover-col",
                set_spacing: 16,
                set_overflow: gtk::Overflow::Hidden,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,

                #[name = "cover_host"]
                gtk::Box {
                    add_css_class: "kalam-float-cover-host",
                    set_orientation: gtk::Orientation::Vertical,
                    set_halign: gtk::Align::Center,
                },

                #[name = "progress_wrap"]
                gtk::Box {
                    add_css_class: "kalam-float-progress-wrap",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 5,

                    gtk::Box {
                        add_css_class: "kalam-float-progress-row",
                        set_orientation: gtk::Orientation::Horizontal,

                        #[name = "progress_pct"]
                        gtk::Label {
                            add_css_class: "kalam-float-progress-pct",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_height_request: 18,
                            set_hexpand: true,
                            set_xalign: 0.0,
                        },

                        #[name = "progress_loc"]
                        gtk::Label {
                            add_css_class: "kalam-float-progress-loc",
                            set_halign: gtk::Align::End,
                            set_valign: gtk::Align::Center,
                            set_height_request: 18,
                            set_max_width_chars: FACT_MAX_CHARS,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_xalign: 1.0,
                        },
                    },

                    #[name = "progress_bar"]
                    gtk::ProgressBar {
                        add_css_class: "kalam-float-progress",
                        set_hexpand: true,
                        set_show_text: false,
                    },
                },

                #[name = "left_meta"]
                gtk::Box {
                    add_css_class: "kalam-float-facts",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,

                    gtk::Box {
                        add_css_class: "kalam-float-fact",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 0,

                        gtk::Label {
                            set_label: "FORMAT",
                            add_css_class: "kalam-float-fact-label",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_height_request: 16,
                            set_margin_top: 2,
                            set_margin_bottom: 0,
                            set_xalign: 0.0,
                        },

                        #[name = "format_val"]
                        gtk::Label {
                            add_css_class: "kalam-float-fact-val",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_height_request: 18,
                            set_max_width_chars: FACT_MAX_CHARS,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_margin_top: 0,
                            set_margin_bottom: 0,
                            set_xalign: 0.0,
                        },
                    },

                    gtk::Box {
                        add_css_class: "kalam-float-fact",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 0,

                        gtk::Label {
                            set_label: "PUBLISHER",
                            add_css_class: "kalam-float-fact-label",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_height_request: 16,
                            set_margin_top: 2,
                            set_margin_bottom: 0,
                            set_xalign: 0.0,
                        },

                        #[name = "publisher_val"]
                        gtk::Label {
                            add_css_class: "kalam-float-fact-val",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_height_request: 18,
                            set_max_width_chars: FACT_MAX_CHARS,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_margin_top: 0,
                            set_margin_bottom: 0,
                            set_wrap: true,
                            set_xalign: 0.0,
                        },
                    },

                    gtk::Box {
                        add_css_class: "kalam-float-fact",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 0,

                        gtk::Label {
                            set_label: "PUBLISHED",
                            add_css_class: "kalam-float-fact-label",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_height_request: 16,
                            set_margin_top: 2,
                            set_margin_bottom: 0,
                            set_xalign: 0.0,
                        },

                        #[name = "published_val"]
                        gtk::Label {
                            add_css_class: "kalam-float-fact-val",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_height_request: 18,
                            set_max_width_chars: FACT_MAX_CHARS,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_margin_top: 0,
                            set_margin_bottom: 0,
                            set_wrap: true,
                            set_xalign: 0.0,
                        },
                    },
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-float-right",
                set_hexpand: true,
                set_vexpand: true,
                set_spacing: 0,

                gtk::Box {
                    add_css_class: "kalam-float-topbar",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,
                        set_hexpand: true,

                        // Up to TWO lines, then ellipsised.
                        //
                        // `set_max_width_chars` is the load-bearing line. A
                        // `Label` with `ellipsize` still reports its *whole*
                        // text as its natural width, and since the float's
                        // 720px is a floor, that natural width pushed the
                        // panel wider for a long title. Capping max-width-chars
                        // caps the natural width; `hexpand` means the label is
                        // still allocated the full column, so it wraps at the
                        // real width rather than at 30 characters.
                        //
                        // Height is free to be one or two lines: `desc_section`
                        // below expands, so the difference comes out of the
                        // description instead of changing the panel's size.
                        #[name = "header_title"]
                        gtk::Label {
                            add_css_class: "kalam-float-title",
                            add_css_class: "kalam-title-serif",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                            set_lines: TITLE_MAX_LINES,
                            set_max_width_chars: TITLE_MAX_CHARS,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_xalign: 0.0,
                        },

                        // `replace_author_links` fills this with a FlowBox,
                        // which wraps — so three authors made the panel taller
                        // than one. The helper is shared with the book page and
                        // the reader, where wrapping is right, so the height is
                        // pinned here instead of changing it for everyone.
                        #[name = "author_val"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_halign: gtk::Align::Start,
                            set_spacing: 0,
                            set_margin_top: 1,
                            set_margin_bottom: 1,
                            set_height_request: AUTHOR_ROW_H,
                            set_valign: gtk::Align::Start,
                            set_overflow: gtk::Overflow::Hidden,
                        },

                        // One line, ellipsised — and capped the same way as
                        // the title. Ellipsising alone does NOT stop a label
                        // widening its parent: the label still reports the
                        // full string as its natural width, and the float's
                        // 720px is only a floor. A long series name was a
                        // second way to stretch the panel sideways.
                        #[name = "series_val"]
                        gtk::Label {
                            add_css_class: "kalam-float-series",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            set_max_width_chars: TITLE_MAX_CHARS,
                            set_wrap: false,
                            set_single_line_mode: true,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_xalign: 0.0,
                        },
                    },

                    // No close button by user request (2026-09-03): clicking
                    // the dimmed backdrop or pressing Esc dismisses this, and
                    // nothing is lost either way — it is a read-only detour.
                },

                gtk::Box {
                    add_css_class: "kalam-float-body",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 10,
                    set_hexpand: true,
                    set_vexpand: true,

                    #[name = "rating_host"]
                    gtk::Box {
                        add_css_class: "kalam-float-rating",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_halign: gtk::Align::Start,
                    },

                    // This section takes ALL the leftover height in the body.
                    //
                    // It replaces the blank `vexpand` spacer that used to do
                    // the anchoring: the slack has to go somewhere, and giving
                    // it to the description is strictly better than leaving it
                    // empty. Everything below is still pinned to the bottom for
                    // exactly the same reason as before — one expanding child,
                    // so nothing beneath it can drift.
                    //
                    // The height request is only a floor, and a small one, so
                    // there is headroom for a two-line title.
                    #[name = "desc_section"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_hexpand: true,
                        set_vexpand: true,

                        // `vexpand` here too, so the text area takes the whole
                        // section. Without it the scroller collapses to its own
                        // minimum and the description shows a single line.
                        #[name = "desc_scroll"]
                        gtk::ScrolledWindow {
                            add_css_class: "kalam-float-desc-scroll",
                            set_hexpand: true,
                            set_vexpand: true,
                            set_hscrollbar_policy: gtk::PolicyType::Never,

                            #[name = "description"]
                            gtk::Label {
                                add_css_class: "kalam-float-desc",
                                add_css_class: "kalam-title-serif-italic",
                                set_halign: gtk::Align::Start,
                                set_valign: gtk::Align::Start,
                                set_wrap: true,
                                set_xalign: 0.0,
                                set_selectable: true,
                            },
                        },
                    },

                    // Tags slide sideways on ONE line; they never wrap.
                    //
                    // This was a `gtk::FlowBox` with `max_children_per_line: 8`
                    // and up to 12 chips, so a book with 9+ tags wrapped to a
                    // second row and made the float taller than a book with 8.
                    // `set_size_request(720, 420)` in `app.rs` is a *floor*, not
                    // a fixed size, so the panel grew to fit and its height
                    // visibly changed from book to book.
                    //
                    // A scroller with a fixed height pins that: one row,
                    // always the same height, overflow slides horizontally
                    // instead of reflowing.
                    //
                    // `External`, not `Automatic`: the row still scrolls by
                    // wheel, touchpad and drag, but GTK draws no scrollbar at
                    // all. `Automatic` reserved space for a bar that appeared
                    // only for heavily-tagged books, which is another way for
                    // the panel to change shape.
                    //
                    // This is never hidden — see `fill()`. An empty tag row
                    // still occupies its height so the action buttons below
                    // sit in the same place for every book.

                    // No spacer here any more. `desc_section` above carries
                    // `vexpand`, so it is the single expanding child and the
                    // tags and buttons are still pinned to the bottom — but the
                    // slack now goes into the description rather than into a
                    // blank gap above the tag row.
                    #[name = "tags_scroll"]
                    gtk::ScrolledWindow {
                        add_css_class: "kalam-float-tags-scroll",
                        set_hscrollbar_policy: gtk::PolicyType::External,
                        set_vscrollbar_policy: gtk::PolicyType::External,
                        // `External`, NOT `Never`. This is the bug that made
                        // the action buttons keep moving: with `Never`, GTK
                        // guarantees the content is fully visible in that
                        // direction, so it propagates the child's *whole*
                        // minimum height and `min/max_content_height` cannot
                        // shrink it. A chip taller than TAGS_ROW_H therefore
                        // still grew the row, and the buttons below moved.
                        // `External` keeps scrolling but lets the row be the
                        // exact height asked for.
                        set_vscrollbar_policy: gtk::PolicyType::External,
                        // Belt and braces: request, min and max all the same,
                        // so the row is one height, always.
                        set_height_request: TAGS_ROW_H,
                        set_min_content_height: TAGS_ROW_H,
                        set_max_content_height: TAGS_ROW_H,
                        set_hexpand: true,
                        set_vexpand: false,
                        // End, not Start: the spacer above has already pushed
                        // this row down to the buttons, and Start would let it
                        // drift back up inside its own allocation.
                        set_valign: gtk::Align::End,

                        #[name = "tags"]
                        gtk::Box {
                            add_css_class: "kalam-float-tags",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                        },
                    },

                    gtk::Box {
                        add_css_class: "kalam-float-actions",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_halign: gtk::Align::Fill,
                        set_valign: gtk::Align::End,
                        set_vexpand: false,

                        #[name = "read_btn"]
                        gtk::Button {
                            set_child: Some(&crate::icons::labelled(
                                "media-playback-start-symbolic",
                                16,
                                "Read",
                                6,
                            )),
                            add_css_class: "kalam-btn-filled",
                            add_css_class: "kalam-float-read",
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Center,
                            set_vexpand: false,
                            connect_clicked => BookFloatMsg::Read,
                        },

                        #[name = "tbr_btn"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "view-list-symbolic",
                                16,
                                &["kalam-inline-icon"],
                            )),
                            set_has_frame: false,
                            add_css_class: "kalam-btn-icon",
                            add_css_class: "kalam-float-icon-btn",
                            set_valign: gtk::Align::Center,
                            set_vexpand: false,
                            connect_clicked => BookFloatMsg::ToggleReadingList,
                        },

                        #[name = "shelf_btn"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "view-grid-symbolic",
                                16,
                                &["kalam-inline-icon"],
                            )),
                            set_has_frame: false,
                            add_css_class: "kalam-btn-icon",
                            add_css_class: "kalam-float-icon-btn",
                            set_valign: gtk::Align::Center,
                            set_vexpand: false,
                            connect_clicked => BookFloatMsg::ShowShelfMenu,
                        },

                        #[name = "edit_btn"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "document-edit-symbolic",
                                16,
                                &["kalam-inline-icon"],
                            )),
                            set_has_frame: false,
                            add_css_class: "kalam-btn-icon",
                            add_css_class: "kalam-float-icon-btn",
                            set_valign: gtk::Align::Center,
                            set_vexpand: false,
                            connect_clicked => BookFloatMsg::EditMetadata,
                        },

                        #[name = "finish_btn"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "object-select-symbolic",
                                16,
                                &["kalam-inline-icon"],
                            )),
                            set_has_frame: false,
                            add_css_class: "kalam-btn-icon",
                            add_css_class: "kalam-float-icon-btn",
                            set_valign: gtk::Align::Center,
                            set_vexpand: false,
                            connect_clicked => BookFloatMsg::ToggleFinished,
                        },

                        gtk::Box {
                            set_hexpand: true,
                        },

                        #[name = "remove_btn"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "edit-delete-symbolic",
                                16,
                                &["kalam-inline-icon"],
                            )),
                            set_has_frame: false,
                            add_css_class: "kalam-btn-icon",
                            add_css_class: "kalam-float-icon-btn",
                            add_css_class: "danger",
                            add_css_class: "kalam-float-icon-btn-danger",
                            set_valign: gtk::Align::Center,
                            set_vexpand: false,
                            set_tooltip_text: Some("Remove"),
                            connect_clicked => BookFloatMsg::Remove,
                        },
                    },
                },
            },
        }
    }

    fn init(
        (catalog, book_id): Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = LibraryService::new(catalog);
        let snap = service.book_detail(book_id);
        report_errors(&snap.errors);
        let model = BookFloatModel {
            service,
            book: snap.book,
            in_reading_list: snap.in_reading_list,
            finished: snap.finished,
        };
        let widgets = view_output!();
        root.set_size_request(720, 420);
        widgets.cover_col.set_size_request(150, -1);
        widgets.progress_wrap.set_size_request(108, -1);
        widgets.progress_wrap.set_halign(gtk::Align::Center);
        widgets.left_meta.set_size_request(120, -1);
        widgets.left_meta.set_halign(gtk::Align::Center);
        widgets.read_btn.set_size_request(-1, 40);
        widgets.tbr_btn.set_size_request(40, 40);
        widgets.shelf_btn.set_size_request(40, 40);
        widgets.edit_btn.set_size_request(40, 40);
        widgets.finish_btn.set_size_request(40, 40);
        widgets.remove_btn.set_size_request(40, 40);
        fill(&widgets, &model, &sender);

        let key = gtk::EventControllerKey::new();
        let s = sender.clone();
        key.connect_key_pressed(move |_, keyval, _keycode, _state| {
            use gtk::gdk::Key;
            if keyval == Key::q || keyval == Key::Q || keyval == Key::Escape {
                s.input(BookFloatMsg::Close);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(key);
        root.set_can_focus(true);
        root.grab_focus();

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            BookFloatMsg::Close => {
                sender.output(BookFloatOut::Close).ok();
            }
            BookFloatMsg::OpenFull => {
                if let Some(book) = &self.book {
                    sender
                        .output(BookFloatOut::OpenFullPage { book_id: book.id })
                        .ok();
                }
            }
            BookFloatMsg::Read => {
                if let Some(book) = &self.book {
                    sender
                        .output(BookFloatOut::OpenReader { book_id: book.id })
                        .ok();
                }
            }
            BookFloatMsg::OpenAuthor(name) => {
                sender.output(BookFloatOut::OpenAuthor { name }).ok();
            }
            BookFloatMsg::Remove => {
                if let Some(book) = &self.book {
                    if let Some(path) = &book.cover_path {
                        invalidate_cover_cache(path);
                    }
                    let id = book.id;
                    let title = book.title.clone();
                    if crate::notify::outcome(
                        self.service.catalog().delete_book(id),
                        "Book removed",
                        &title,
                        "Could not remove the book",
                    ) {
                        self.book = None;
                        sender.output(BookFloatOut::Deleted { book_id: id }).ok();
                    }
                }
            }
            BookFloatMsg::ToggleReadingList => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let title = book.title.clone();
                    if self.in_reading_list {
                        if crate::notify::report(
                            self.service.catalog().remove_from_reading_list(id),
                            "Could not update the reading list",
                        ) {
                            crate::notify::info("Removed from reading list", &title);
                        }
                    } else if crate::notify::report(
                        self.service.catalog().add_to_reading_list(id),
                        "Could not update the reading list",
                    ) {
                        crate::notify::success("Added to reading list", &title);
                    }
                    self.reload_state(id);
                }
            }
            BookFloatMsg::ToggleFinished => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let title = book.title.clone();
                    let becoming = !self.finished;
                    if crate::notify::report(
                        self.service.catalog().set_book_finished(id, becoming),
                        "Could not update the book",
                    ) {
                        if becoming {
                            crate::notify::success("Marked as finished", &title);
                        } else {
                            crate::notify::info("Marked as unread", &title);
                        }
                    }
                    self.reload_state(id);
                }
            }
            BookFloatMsg::SetRating(half_stars) => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let detail = if half_stars == 0 {
                        "Rating cleared".to_string()
                    } else {
                        format!("{:.1} / 5", half_stars as f32 / 2.0)
                    };
                    crate::notify::outcome(
                        self.service.catalog().set_book_rating(id, half_stars),
                        "Rating saved",
                        &detail,
                        "Could not save the rating",
                    );
                    self.reload_state(id);
                }
            }
            BookFloatMsg::EditMetadata => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let s = sender.clone();
                    open_metadata_editor(root, self.service.catalog().clone(), id, move || {
                        s.input(BookFloatMsg::Refresh)
                    });
                }
            }
            BookFloatMsg::ShowShelfMenu => {
                sender.output(BookFloatOut::ShowShelves).ok();
            }
            BookFloatMsg::Refresh => {
                if let Some(book) = &self.book {
                    self.reload_state(book.id);
                }
            }
        }

        fill(widgets, self, &sender);
        self.update_view(widgets, sender);
    }
}

/// Surface read failures. Without this a database problem looked exactly
/// like "this book was deleted".
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read this book", err);
    }
}

impl BookFloatModel {
    fn reload_state(&mut self, book_id: i64) {
        let snap = self.service.book_detail(book_id);
        report_errors(&snap.errors);
        self.book = snap.book;
        self.in_reading_list = snap.in_reading_list;
        self.finished = snap.finished;
    }
}

fn fill(
    widgets: &BookFloatModelWidgets,
    model: &BookFloatModel,
    sender: &ComponentSender<BookFloatModel>,
) {
    clear_box(&widgets.cover_host);
    clear_box(&widgets.author_val);
    clear_box(&widgets.tags);
    clear_box(&widgets.rating_host);

    let has_book = model.book.is_some();
    widgets.read_btn.set_visible(has_book);
    widgets.tbr_btn.set_visible(has_book);
    widgets.shelf_btn.set_visible(has_book);
    widgets.edit_btn.set_visible(has_book);
    widgets.finish_btn.set_visible(has_book);
    widgets.remove_btn.set_visible(has_book);
    widgets.progress_wrap.set_visible(has_book);
    widgets.left_meta.set_visible(has_book);
    widgets.rating_host.set_visible(has_book);
    widgets.desc_section.set_visible(has_book);
    widgets.desc_scroll.set_visible(has_book);
    // Always visible, tags or not, and note it does NOT follow `has_book`:
    // this row reserves the space that keeps the action buttons below it in a
    // fixed position. Hiding it collapsed that space and moved the buttons.
    widgets.tags.set_visible(true);
    widgets.tags_scroll.set_visible(true);

    let Some(book) = model.book.as_ref() else {
        widgets.header_title.set_label("Book not found");
        widgets.series_val.set_visible(false);
        widgets.description.set_label("This book was removed.");
        widgets.progress_pct.set_label("");
        widgets.progress_loc.set_label("");
        widgets.progress_bar.set_fraction(0.0);
        widgets.format_val.set_label("");
        widgets.publisher_val.set_label("");
        widgets.published_val.set_label("");
        widgets
            .cover_host
            .append(&build_cover_display(None, false, sender));
        return;
    };

    widgets.header_title.set_label(&book.title);
    // The label ellipsises, so the full title has to stay reachable.
    widgets.header_title.set_tooltip_text(Some(&book.title));

    let tx = sender.input_sender().clone();
    crate::widgets::author_links::replace_author_links(
        &widgets.author_val,
        book.authors_display(),
        "kalam-author-link-float",
        std::rc::Rc::new(move |name| {
            let _ = tx.send(BookFloatMsg::OpenAuthor(name));
        }),
    );

    if let Some(series) = book.series_display() {
        widgets.series_val.set_label(&series);
        widgets.series_val.set_tooltip_text(Some(&series));
        widgets.series_val.set_visible(true);
    } else {
        widgets.series_val.set_visible(false);
    }

    widgets.cover_host.append(&build_cover_display(
        book.cover_path.as_deref(),
        true,
        sender,
    ));

    let progress_fraction = (book.progress as f64 / 100.0).clamp(0.0, 1.0);
    widgets.progress_bar.set_fraction(progress_fraction);
    widgets
        .progress_pct
        .set_label(&format!("{}%", book.progress.min(100)));
    widgets
        .progress_loc
        .set_label(&progress_location_text(model.service.catalog(), book));

    widgets.format_val.set_label(book.format.as_str());
    widgets.publisher_val.set_label(blank_dash(&book.publisher));
    widgets.published_val.set_label(blank_dash(&book.published));

    // The full description, always. No truncation and no expand/collapse:
    // the box is a fixed size and long text scrolls inside it.
    //
    // The old toggle was also *backwards*. `fill()` ran on every update and
    // called `set_vexpand(false)` here, quietly overriding the `vexpand: true`
    // set in the view. The section still expanded but the text area inside it
    // did not, so pressing "Read more" swapped the scrollbar policy from
    // `Never` to `Automatic`, the scroller dropped to its own minimum, and the
    // description *shrank to one line* instead of growing.
    widgets.description.set_label(&clean_description(book));
    widgets.desc_scroll.set_vexpand(true);
    widgets.desc_section.set_height_request(DESC_SECTION_HEIGHT);
    widgets.desc_scroll.set_propagate_natural_height(false);
    widgets.desc_scroll.set_min_content_height(-1);
    widgets.desc_scroll.set_max_content_height(-1);
    widgets.desc_scroll.set_height_request(-1);
    widgets
        .desc_scroll
        .set_vscrollbar_policy(gtk::PolicyType::Automatic);

    // No `.take(12)` any more: the row scrolls, so every tag can be shown
    // without changing the panel's size. Truncating was only ever a way to
    // limit how far the FlowBox could grow.
    for tag in &book.tags {
        widgets.tags.append(&chip(tag, "kalam-chip"));
    }
    // No visibility toggle here on purpose: the row is already shown
    // unconditionally above, tags or not. Hiding it when a book had none
    // collapsed its height and pulled the action buttons up, so the buttons
    // moved depending on whether the book happened to be tagged.

    let s = sender.clone();
    widgets
        .rating_host
        .append(&star_picker(book.rating, move |v| {
            s.input(BookFloatMsg::SetRating(v))
        }));
    let rating_text = gtk::Label::new(Some(&rating_text(book)));
    rating_text.add_css_class("kalam-float-rating-text");
    rating_text.set_valign(gtk::Align::Center);
    widgets.rating_host.append(&rating_text);

    sync_action_buttons(widgets, model);
}

fn build_cover_display(
    path: Option<&std::path::Path>,
    clickable: bool,
    sender: &ComponentSender<BookFloatModel>,
) -> gtk::Widget {
    let shell = gtk::Overlay::new();
    shell.add_css_class("kalam-float-book-shell");
    shell.set_size_request(COVER_W + 6, COVER_H + 6);
    shell.set_halign(gtk::Align::Center);
    shell.set_valign(gtk::Align::Start);

    let edge = gtk::Box::new(gtk::Orientation::Vertical, 0);
    edge.add_css_class("kalam-float-book-edge");
    edge.set_size_request(COVER_W, COVER_H);
    edge.set_halign(gtk::Align::Start);
    edge.set_valign(gtk::Align::Start);
    edge.set_margin_start(5);
    edge.set_margin_top(5);
    shell.set_child(Some(&edge));

    let cover = cover_widget(path, COVER_W, COVER_H);
    cover.add_css_class("kalam-float-cover");
    cover.set_halign(gtk::Align::Start);
    cover.set_valign(gtk::Align::Start);
    shell.add_overlay(&cover);

    if clickable {
        cover.set_cursor_from_name(Some("pointer"));
        let click = gtk::GestureClick::new();
        let tx = sender.input_sender().clone();
        click.connect_released(move |_, _, _, _| {
            let _ = tx.send(BookFloatMsg::OpenFull);
        });
        shell.add_controller(click);
        shell.set_tooltip_text(Some("Open full details"));
        shell.set_cursor_from_name(Some("pointer"));
    }

    shell.upcast()
}

fn sync_action_buttons(widgets: &BookFloatModelWidgets, model: &BookFloatModel) {
    widgets.tbr_btn.remove_css_class("active");
    widgets.finish_btn.remove_css_class("active");
    widgets.finish_btn.remove_css_class("done-active");

    if model.in_reading_list {
        widgets.tbr_btn.add_css_class("active");
        widgets
            .tbr_btn
            .set_tooltip_text(Some("Remove from reading list"));
    } else {
        widgets
            .tbr_btn
            .set_tooltip_text(Some("Add to reading list"));
    }

    widgets.shelf_btn.set_tooltip_text(Some("Shelves"));
    widgets.edit_btn.set_tooltip_text(Some("Edit metadata"));

    if model.finished {
        widgets.finish_btn.add_css_class("active");
        widgets.finish_btn.add_css_class("done-active");
        widgets.finish_btn.set_tooltip_text(Some("Mark unread"));
    } else {
        widgets.finish_btn.set_tooltip_text(Some("Mark finished"));
    }
}

fn progress_location_text(catalog: &Catalog, book: &Book) -> String {
    if book.progress >= 100 {
        return "Finished".into();
    }
    if let Ok(Some((chapter, _fraction))) = catalog.get_reading_progress(book.id) {
        return format!("Ch. {}", chapter + 1);
    }
    if book.progress > 0 {
        "In progress".into()
    } else {
        "Not started".into()
    }
}

fn blank_dash(text: &str) -> &str {
    let text = text.trim();
    if text.is_empty() {
        "—"
    } else {
        text
    }
}

fn clean_description(book: &Book) -> String {
    let desc = crate::epub::strip_html(&book.description);
    let desc = desc.trim();
    if desc.is_empty() {
        "No description.".into()
    } else {
        desc.to_string()
    }
}

fn rating_text(book: &Book) -> String {
    match book.rating_stars() {
        Some(v) => format!("{v:.1} / 5"),
        None => "Not rated yet".into(),
    }
}

fn chip(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class(class);
    l
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}
