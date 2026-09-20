//! Spaced-repetition review of saved words (roadmap #5).
//!
//! A focused flashcard page: the word is shown, the reader recalls the
//! definition, reveals it, then grades the recall (Again / Good / Easy). The
//! grade feeds a small SM-2-style schedule in [`crate::db::Catalog`] that
//! decides when the word next comes due.

use crate::db::{Catalog, ReviewCard};
use crate::service::LibraryService;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub enum ReviewOut {
    #[allow(dead_code)] // reserved for jumping to the word's book later
    OpenBook { book_id: i64 },
}

#[derive(Debug, Clone, Copy)]
pub enum ReviewMsg {
    Reveal,
    /// 0 = again, 1 = good, 2 = easy.
    Answer(i32),
    Refresh,
}

pub struct ReviewModel {
    service: LibraryService,
    current: Option<ReviewCard>,
    revealed: bool,
    status: String,
}

impl ReviewModel {
    fn word_text(&self) -> String {
        self.current
            .as_ref()
            .map(|c| c.word.clone())
            .unwrap_or_default()
    }

    fn def_text(&self) -> String {
        self.current
            .as_ref()
            .map(|c| c.definition.clone())
            .unwrap_or_default()
    }

    fn load_next(&mut self) {
        let due = self.service.catalog().due_review_count().unwrap_or(0);
        match self.service.catalog().due_review_words(1) {
            Ok(mut cards) => {
                self.current = cards.pop();
                if self.current.is_none() {
                    self.status = "All caught up — nothing due right now.".to_string();
                } else {
                    self.status = format!("{due} due");
                }
            }
            Err(err) => {
                self.current = None;
                self.status = format!("Could not load review: {err}");
            }
        }
        self.revealed = false;
    }
}

#[relm4::component(pub)]
impl Component for ReviewModel {
    type Init = Arc<Catalog>;
    type Input = ReviewMsg;
    type Output = ReviewOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,
            set_margin_all: 18,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                gtk::Label {
                    set_label: "Review",
                    add_css_class: "title-2",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },
                #[name = "due_label"]
                gtk::Label {
                    #[watch]
                    set_label: &model.status,
                    add_css_class: "kalam-muted",
                    set_halign: gtk::Align::End,
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 14,
                set_valign: gtk::Align::Center,
                set_vexpand: true,

                #[name = "word_label"]
                gtk::Label {
                    #[watch]
                    set_label: model.word_text().as_str(),
                    add_css_class: "k-word",
                    set_wrap: true,
                    set_halign: gtk::Align::Center,
                },

                #[name = "reveal_btn"]
                gtk::Button {
                    set_label: "Show definition",
                    add_css_class: "kalam-btn-tonal",
                    set_halign: gtk::Align::Center,
                    #[watch]
                    set_visible: model.current.is_some() && !model.revealed,
                    connect_clicked => ReviewMsg::Reveal,
                },

                #[name = "def_label"]
                gtk::Label {
                    #[watch]
                    set_label: model.def_text().as_str(),
                    #[watch]
                    set_visible: model.revealed,
                    set_wrap: true,
                    set_halign: gtk::Align::Center,
                    set_max_width_chars: 60,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_halign: gtk::Align::Center,
                    #[watch]
                    set_visible: model.revealed,

                    gtk::Button {
                        set_label: "Again",
                        add_css_class: "kalam-btn-outlined",
                        connect_clicked => ReviewMsg::Answer(0),
                    },
                    gtk::Button {
                        set_label: "Good",
                        add_css_class: "kalam-btn-tonal",
                        connect_clicked => ReviewMsg::Answer(1),
                    },
                    gtk::Button {
                        set_label: "Easy",
                        add_css_class: "kalam-btn-tonal",
                        connect_clicked => ReviewMsg::Answer(2),
                    },
                },

                #[name = "empty_label"]
                gtk::Label {
                    set_label: "Save some words from the dictionary, then come back to review them.",
                    add_css_class: "kalam-muted",
                    set_wrap: true,
                    #[watch]
                    set_visible: model.current.is_none(),
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = ReviewModel {
            service: LibraryService::new(catalog),
            current: None,
            revealed: false,
            status: String::new(),
        };
        model.load_next();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(
        &mut self,
        message: Self::Input,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            ReviewMsg::Reveal => self.revealed = true,
            ReviewMsg::Answer(quality) => {
                if let Some(card) = self.current.take() {
                    if let Err(err) = self.service.catalog().record_review(card.id, quality) {
                        crate::notify::error("Could not record the review", &err.to_string());
                    }
                }
                self.load_next();
            }
            ReviewMsg::Refresh => self.load_next(),
        }
    }
}
