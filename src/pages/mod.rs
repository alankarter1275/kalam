//! Top-level and nested pages mounted into the main content area.

use std::cell::RefCell;
use std::rc::Rc;

/// A rebuild closure that needs to call itself (a list re-renders, and each
/// rebuilt row's handler triggers another rebuild). The closure is stored in
/// a shared cell so it can be cloned into its own body without a cycle at
/// construction time.
///
/// Named because the bare form -- `Rc<RefCell<Option<Rc<dyn Fn()>>>>` -- trips
/// clippy::type_complexity in three separate pages.
pub type SelfRebuild = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub mod all_books;
pub mod analytics;
pub mod author;
pub mod book;
pub mod book_float;
pub mod comics;
pub mod comics_reader;
pub mod history;
pub mod home;
pub mod library;
pub mod lookup_history;
pub mod metadata_editor;
pub mod placeholder;
pub mod reader;
pub mod reading_list;
pub mod saved_quotes;
pub mod saved_words;
pub mod series_float;
pub mod settings;
pub mod shelf_detail;
pub mod shelf_editor;
pub mod shelves_grid;
pub mod tags;
pub mod task_manager;
pub mod browse;
pub mod remote_detail;
pub mod downloads;
pub mod pdf_reader;
