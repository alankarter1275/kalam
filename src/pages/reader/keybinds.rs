//! The reader's own keys, as a table a reader can edit (roadmap 2.9).
//!
//! The engine has a `KeyMap` of its own for the universal reading keys —
//! arrows, space, Page Up/Down — and those stay fixed: they are what every
//! reader expects, and they live in a different table in a different crate.
//! What lives here is Kalam's actions, the ones worth moving to a key that
//! suits the hand on the keyboard.
//!
//! One action, one key. Binding a key that another action holds takes it
//! from that action, because a key that does two things does neither
//! predictably.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gdk;

use super::types::{LeftSidebarTab, ReaderMsg, RightSidebarTab};
use crate::db::Catalog;

/// One of Kalam's reader actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ReaderAction {
    Define,
    NextChapter,
    PrevChapter,
    FontUp,
    FontDown,
    Contents,
    Settings,
    Highlights,
    Words,
    Bookmark,
    JumpBack,
    Search,
}

impl ReaderAction {
    /// Every action, in the order the settings panel lists them.
    pub(crate) const ALL: [ReaderAction; 12] = [
        ReaderAction::Define,
        ReaderAction::NextChapter,
        ReaderAction::PrevChapter,
        ReaderAction::FontUp,
        ReaderAction::FontDown,
        ReaderAction::Contents,
        ReaderAction::Settings,
        ReaderAction::Highlights,
        ReaderAction::Words,
        ReaderAction::Bookmark,
        ReaderAction::JumpBack,
        ReaderAction::Search,
    ];

    /// The preference key's suffix, and what a binding is stored under.
    /// Stable: renaming one orphans a reader's saved keys.
    fn id(self) -> &'static str {
        match self {
            ReaderAction::Define => "define",
            ReaderAction::NextChapter => "next_chapter",
            ReaderAction::PrevChapter => "prev_chapter",
            ReaderAction::FontUp => "font_up",
            ReaderAction::FontDown => "font_down",
            ReaderAction::Contents => "contents",
            ReaderAction::Settings => "settings",
            ReaderAction::Highlights => "highlights",
            ReaderAction::Words => "words",
            ReaderAction::Bookmark => "bookmark",
            ReaderAction::JumpBack => "jump_back",
            ReaderAction::Search => "search",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ReaderAction::Define => "Look up a word",
            ReaderAction::NextChapter => "Next chapter",
            ReaderAction::PrevChapter => "Previous chapter",
            ReaderAction::FontUp => "Larger text",
            ReaderAction::FontDown => "Smaller text",
            ReaderAction::Contents => "Table of contents",
            ReaderAction::Settings => "Settings",
            ReaderAction::Highlights => "Highlights",
            ReaderAction::Words => "Saved words",
            ReaderAction::Bookmark => "Add a bookmark",
            ReaderAction::JumpBack => "Jump back",
            ReaderAction::Search => "Search the book",
        }
    }

    /// What the key did before it was editable.
    fn default_binding(self) -> KeyBinding {
        match self {
            ReaderAction::Define => KeyBinding::plain(gdk::Key::d),
            ReaderAction::NextChapter => KeyBinding::plain(gdk::Key::n),
            ReaderAction::PrevChapter => KeyBinding::plain(gdk::Key::p),
            ReaderAction::FontUp => KeyBinding::plain(gdk::Key::plus),
            ReaderAction::FontDown => KeyBinding::plain(gdk::Key::minus),
            ReaderAction::Contents => KeyBinding::plain(gdk::Key::t),
            ReaderAction::Settings => KeyBinding::plain(gdk::Key::s),
            ReaderAction::Highlights => KeyBinding::plain(gdk::Key::h),
            ReaderAction::Words => KeyBinding::plain(gdk::Key::w),
            ReaderAction::Bookmark => KeyBinding::plain(gdk::Key::b),
            ReaderAction::JumpBack => KeyBinding::plain(gdk::Key::BackSpace),
            // Ctrl+F, as it has always been — and the one binding the
            // reader's key handler honours before it asks whether a text
            // field has focus, so search opens even from the search box.
            ReaderAction::Search => KeyBinding {
                keyval: gdk::Key::f,
                ctrl: true,
            },
        }
    }

    /// The message this action sends.
    pub(crate) fn message(self) -> ReaderMsg {
        match self {
            ReaderAction::Define => ReaderMsg::LookUpSelection,
            ReaderAction::NextChapter => ReaderMsg::NextChapter,
            ReaderAction::PrevChapter => ReaderMsg::PrevChapter,
            ReaderAction::FontUp => ReaderMsg::FontDelta(1),
            ReaderAction::FontDown => ReaderMsg::FontDelta(-1),
            ReaderAction::Contents => ReaderMsg::SwitchLeftTab(LeftSidebarTab::Toc),
            ReaderAction::Settings => ReaderMsg::SwitchLeftTab(LeftSidebarTab::Settings),
            ReaderAction::Highlights => ReaderMsg::SwitchRightTab(RightSidebarTab::Highlights),
            ReaderAction::Words => ReaderMsg::SwitchRightTab(RightSidebarTab::Words),
            ReaderAction::Bookmark => ReaderMsg::AddBookmark,
            ReaderAction::JumpBack => ReaderMsg::JumpBack,
            ReaderAction::Search => ReaderMsg::ToggleSearch,
        }
    }

    fn pref_key(self) -> String {
        format!("reader.key.{}", self.id())
    }
}

/// A key, and whether Ctrl goes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyBinding {
    pub(crate) keyval: u32,
    pub(crate) ctrl: bool,
}

impl KeyBinding {
    fn plain(keyval: u32) -> KeyBinding {
        KeyBinding { keyval, ctrl: false }
    }

    /// How the settings panel shows it: "D", "+", "Backspace", "Ctrl+F".
    pub(crate) fn label(&self) -> String {
        let name = display_name(self.keyval);
        if self.ctrl {
            format!("Ctrl+{name}")
        } else {
            name
        }
    }

    /// As stored: `<ctrl>f`, `d`, `plus`. The GDK key name round-trips
    /// through `keyval_from_name`, which is the point of using it.
    fn to_pref(&self) -> String {
        let name = gdk::keyval_name(self.keyval)
            .map(|n| n.to_string())
            .unwrap_or_default();
        if self.ctrl {
            format!("<ctrl>{name}")
        } else {
            name
        }
    }

    fn from_pref(value: &str) -> Option<KeyBinding> {
        let (ctrl, name) = match value.strip_prefix("<ctrl>") {
            Some(rest) => (true, rest),
            None => (false, value),
        };
        if name.is_empty() {
            return None;
        }
        let keyval = gdk::keyval_from_name(name);
        // `keyval_from_name` answers 0 for a name it does not know, which
        // is not a key anyone can press.
        (keyval != 0).then_some(KeyBinding { keyval, ctrl })
    }
}

/// A key name worth showing the way a keyboard writes it.
fn display_name(keyval: u32) -> String {
    match keyval {
        gdk::Key::plus => "+".to_string(),
        gdk::Key::equal => "=".to_string(),
        gdk::Key::minus => "−".to_string(),
        gdk::Key::BackSpace => "Backspace".to_string(),
        gdk::Key::Return | gdk::Key::KP_Enter => "Enter".to_string(),
        gdk::Key::Escape => "Escape".to_string(),
        gdk::Key::space => "Space".to_string(),
        gdk::Key::Tab => "Tab".to_string(),
        _ => match gdk::keyval_name(keyval) {
            Some(name) => {
                let name = name.to_string();
                let mut chars = name.chars();
                match chars.next() {
                    // One character is a letter or a digit: show it as the
                    // capital on the key.
                    Some(c) if name.chars().count() == 1 => c.to_uppercase().to_string(),
                    Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                    None => "?".to_string(),
                }
            }
            None => "?".to_string(),
        },
    }
}

/// The whole table: what each action is bound to.
#[derive(Debug, Clone)]
pub(crate) struct KeyBindings {
    map: HashMap<ReaderAction, KeyBinding>,
}

impl KeyBindings {
    /// Every action on its default key.
    pub(crate) fn defaults() -> KeyBindings {
        KeyBindings {
            map: ReaderAction::ALL
                .into_iter()
                .map(|action| (action, action.default_binding()))
                .collect(),
        }
    }

    /// Defaults, then whatever the reader has saved over them. A saved
    /// binding that no longer parses is dropped rather than trusted.
    pub(crate) fn load(catalog: &Catalog) -> KeyBindings {
        let mut bindings = KeyBindings::defaults();
        for action in ReaderAction::ALL {
            if let Some(saved) = catalog.get_pref(&action.pref_key()) {
                if let Some(binding) = KeyBinding::from_pref(&saved) {
                    bindings.map.insert(action, binding);
                }
            }
        }
        bindings
    }

    pub(crate) fn binding(&self, action: ReaderAction) -> KeyBinding {
        self.map
            .get(&action)
            .copied()
            .unwrap_or_else(|| action.default_binding())
    }

    /// Bind `action` to `binding`, taking the key from whatever held it.
    pub(crate) fn bind(&mut self, action: ReaderAction, binding: KeyBinding) {
        self.map.retain(|_, held| *held != binding);
        self.map.insert(action, binding);
    }

    /// Which action this keypress runs, if any.
    ///
    /// `+` needs Shift on most layouts and `=` does not, and they are the
    /// same physical key — so a binding on one answers the other unless the
    /// reader gave the other an action of its own. That keeps `=` making
    /// text larger, as it did before any of this was editable.
    pub(crate) fn action_for(&self, keyval: u32, ctrl: bool) -> Option<ReaderAction> {
        // Shift+D arrives as a different keyval from d and the table holds
        // one spelling, so the press is folded onto it.
        let keyval = gdk::keyval_to_lower(keyval);
        let wanted = KeyBinding { keyval, ctrl };
        let twin = match keyval {
            gdk::Key::plus => Some(gdk::Key::equal),
            gdk::Key::equal => Some(gdk::Key::plus),
            _ => None,
        };
        ReaderAction::ALL.into_iter().find(|action| {
            let held = self.binding(*action);
            held == wanted
                || twin.is_some_and(|twin| held == KeyBinding { keyval: twin, ctrl })
        })
    }

    /// Store one binding where [`KeyBindings::load`] will find it.
    pub(crate) fn save(&self, catalog: &Catalog, action: ReaderAction) {
        catalog.set_pref(&action.pref_key(), &self.binding(action).to_pref());
    }

    /// Every action back on the key it shipped with, stored.
    pub(crate) fn reset_all(&mut self, catalog: &Catalog) {
        *self = KeyBindings::defaults();
        for action in ReaderAction::ALL {
            self.save(catalog, action);
        }
    }

    /// The shared handle the model and the key controller both hold, so a
    /// change in settings reaches the handler without rebuilding it.
    pub(crate) fn shared(catalog: &Catalog) -> Rc<RefCell<KeyBindings>> {
        Rc::new(RefCell::new(KeyBindings::load(catalog)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_binding_round_trips_through_the_preference_text() {
        for action in ReaderAction::ALL {
            let binding = action.default_binding();
            let stored = binding.to_pref();
            assert_eq!(
                KeyBinding::from_pref(&stored),
                Some(binding),
                "{} does not survive its own encoding",
                action.label()
            );
        }
        assert_eq!(
            KeyBindings::defaults()
                .binding(ReaderAction::Search)
                .to_pref(),
            "<ctrl>f"
        );
    }

    #[test]
    fn a_key_that_is_not_a_key_is_refused() {
        assert_eq!(KeyBinding::from_pref("not-a-keyval"), None);
        assert_eq!(KeyBinding::from_pref(""), None);
        assert_eq!(KeyBinding::from_pref("<ctrl>"), None);
    }

    #[test]
    fn a_key_does_one_thing() {
        let mut bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.action_for(gdk::Key::d, false),
            Some(ReaderAction::Define)
        );
        // Moving "next chapter" onto D takes D from the dictionary.
        bindings.bind(ReaderAction::NextChapter, KeyBinding::plain(gdk::Key::d));
        assert_eq!(
            bindings.action_for(gdk::Key::d, false),
            Some(ReaderAction::NextChapter)
        );
        assert_eq!(bindings.action_for(gdk::Key::n, false), None);
    }

    #[test]
    fn equals_still_makes_text_larger() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.action_for(gdk::Key::plus, false),
            Some(ReaderAction::FontUp)
        );
        assert_eq!(
            bindings.action_for(gdk::Key::equal, false),
            Some(ReaderAction::FontUp),
            "the unshifted twin of + is the same physical key"
        );
        // ... unless the reader has given = an action of its own.
        let mut bindings = KeyBindings::defaults();
        bindings.bind(ReaderAction::Words, KeyBinding::plain(gdk::Key::equal));
        assert_eq!(
            bindings.action_for(gdk::Key::equal, false),
            Some(ReaderAction::Words)
        );
    }

    #[test]
    fn an_uppercase_letter_is_the_same_key() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.action_for(gdk::Key::D, false),
            Some(ReaderAction::Define)
        );
        assert_eq!(
            bindings.action_for(gdk::Key::N, false),
            Some(ReaderAction::NextChapter)
        );
    }

    #[test]
    fn ctrl_is_part_of_the_key() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.action_for(gdk::Key::f, true),
            Some(ReaderAction::Search)
        );
        assert_eq!(bindings.action_for(gdk::Key::f, false), None);
    }

    #[test]
    fn every_action_has_a_name_and_a_key_to_show() {
        for action in ReaderAction::ALL {
            assert!(!action.label().is_empty());
            let shown = KeyBindings::defaults().binding(action).label();
            assert!(!shown.is_empty(), "{} has no key to show", action.id());
            assert_ne!(shown, "?", "{} shows as an unknown key", action.id());
        }
    }
}
