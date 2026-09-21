//! The reader's own keys, as a table a reader can edit (roadmap 2.9).
//!
//! The engine has a `KeyMap` of its own for the universal reading keys —
//! arrows, space, Page Up/Down — and those stay fixed: they are what every
//! reader expects, and they live in a different table in a different crate.
//! What lives here is Kalam's actions, the ones worth moving to a key that
//! suits the hand on the keyboard.
//!
//! A key is held by the name the toolkit gives it — `d`, `plus`,
//! `BackSpace` — which is what [`gdk::Key::name`] returns and what the
//! reader's own key handling already matches on. Names round-trip through a
//! preference with no lookup table of my own, and they make Shift+D the same
//! key as d rather than a second spelling to remember.
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
        let key = match self {
            ReaderAction::Define => gdk::Key::d,
            ReaderAction::NextChapter => gdk::Key::n,
            ReaderAction::PrevChapter => gdk::Key::p,
            ReaderAction::FontUp => gdk::Key::plus,
            ReaderAction::FontDown => gdk::Key::minus,
            ReaderAction::Contents => gdk::Key::t,
            ReaderAction::Settings => gdk::Key::s,
            ReaderAction::Highlights => gdk::Key::h,
            ReaderAction::Words => gdk::Key::w,
            ReaderAction::Bookmark => gdk::Key::b,
            ReaderAction::JumpBack => gdk::Key::BackSpace,
            // Ctrl+F, as it has always been — and the one binding the
            // reader's key handler honours before it asks whether a text
            // field has focus, so search opens even from the search box.
            ReaderAction::Search => return KeyBinding::ctrl(gdk::Key::f),
        };
        KeyBinding::plain(key)
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyBinding {
    /// The toolkit's name for the key: `d`, `plus`, `BackSpace`.
    name: String,
    ctrl: bool,
}

impl KeyBinding {
    pub(crate) fn plain(key: gdk::Key) -> KeyBinding {
        KeyBinding {
            name: key_name(key),
            ctrl: false,
        }
    }

    pub(crate) fn ctrl(key: gdk::Key) -> KeyBinding {
        KeyBinding {
            name: key_name(key),
            ctrl: true,
        }
    }

    /// No key at all — what an action holds after its key was given to
    /// another. `none` is not a GDK key name, so it never matches a press,
    /// and it shows as "None" in the panel.
    pub(crate) fn unbound() -> KeyBinding {
        KeyBinding {
            name: UNBOUND.to_string(),
            ctrl: false,
        }
    }

    /// From a captured press. `None` for a key the toolkit will not name,
    /// which is not one a reader can be asked to press again.
    pub(crate) fn capture(key: gdk::Key, ctrl: bool) -> Option<KeyBinding> {
        let name = key.name()?.to_string();
        Some(KeyBinding { name, ctrl })
    }

    /// How the settings panel shows it: "D", "+", "Backspace", "Ctrl+F".
    pub(crate) fn label(&self) -> String {
        let name = display_name(&self.name);
        if self.ctrl {
            format!("Ctrl+{name}")
        } else {
            name
        }
    }

    /// Is this binding's key the one pressed, whatever Ctrl is doing?
    pub(crate) fn is_key(&self, key: gdk::Key) -> bool {
        key.name()
            .is_some_and(|pressed| pressed.eq_ignore_ascii_case(&self.name))
    }

    /// As stored: `<ctrl>f`, `d`, `plus`.
    pub(crate) fn to_pref(&self) -> String {
        if self.ctrl {
            format!("<ctrl>{}", self.name)
        } else {
            self.name.clone()
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
        // Not checked against a list of real keys: a name the toolkit does
        // not know simply never matches a press, so it cannot hijack one
        // that can, and "Reset to defaults" puts everything back.
        Some(KeyBinding {
            name: name.to_string(),
            ctrl,
        })
    }
}

/// The name an action with no key is stored and shown under.
pub(crate) const UNBOUND: &str = "none";

/// The toolkit's name for a key, or nothing for one it will not name.
fn key_name(key: gdk::Key) -> String {
    key.name().map(|name| name.to_string()).unwrap_or_default()
}

/// A key name worth showing the way a keyboard writes it.
fn display_name(name: &str) -> String {
    match name {
        "plus" => "+".to_string(),
        "equal" => "=".to_string(),
        "minus" => "−".to_string(),
        "BackSpace" => "Backspace".to_string(),
        "Return" | "KP_Enter" => "Enter".to_string(),
        "Escape" => "Escape".to_string(),
        "space" => "Space".to_string(),
        "Tab" => "Tab".to_string(),
        UNBOUND => "None".to_string(),
        _ => {
            let mut chars = name.chars();
            match chars.next() {
                // One character is a letter or a digit: show the capital
                // that is on the key.
                Some(c) if name.chars().count() == 1 => c.to_uppercase().to_string(),
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => "?".to_string(),
            }
        }
    }
}

/// `+` needs Shift on most layouts and `=` does not, and they are the same
/// physical key. A binding on one answers the other unless the reader gave
/// the other an action of its own, which keeps `=` making text larger as it
/// did before any of this was editable.
fn twin_of(name: &str) -> Option<&'static str> {
    match name {
        "plus" => Some("equal"),
        "equal" => Some("plus"),
        _ => None,
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
            .cloned()
            .unwrap_or_else(|| action.default_binding())
    }

    /// Bind `action` to `binding`, taking the key from whatever held it.
    ///
    /// The action that loses its key becomes unbound rather than being
    /// dropped from the table: dropped, it would fall back to the key it
    /// shipped with and quietly take the key back.
    pub(crate) fn bind(&mut self, action: ReaderAction, binding: KeyBinding) {
        for slot in self.map.values_mut() {
            if *slot == binding {
                *slot = KeyBinding::unbound();
            }
        }
        self.map.insert(action, binding);
    }

    /// Which action this keypress runs, if any.
    ///
    /// In two passes: a key somebody claimed exactly wins, and the
    /// physical-key twin of `+`/`=` is only a fallback. Without that order,
    /// giving `=` an action of its own would lose to the `+` binding's
    /// claim on it.
    pub(crate) fn action_for(&self, key: gdk::Key, ctrl: bool) -> Option<ReaderAction> {
        ReaderAction::ALL
            .into_iter()
            .find(|action| self.claims_exact(*action, key, ctrl))
            .or_else(|| {
                ReaderAction::ALL
                    .into_iter()
                    .find(|action| self.claims_twin(*action, key, ctrl))
            })
    }

    fn claims_exact(&self, action: ReaderAction, key: gdk::Key, ctrl: bool) -> bool {
        let held = self.binding(action);
        held.ctrl == ctrl && held.is_key(key)
    }

    fn claims_twin(&self, action: ReaderAction, key: gdk::Key, ctrl: bool) -> bool {
        let held = self.binding(action);
        held.ctrl == ctrl
            && twin_of(&held.name).is_some_and(|twin| {
                KeyBinding {
                    name: twin.to_string(),
                    ctrl: held.ctrl,
                }
                .is_key(key)
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
    fn an_empty_name_is_refused() {
        assert_eq!(KeyBinding::from_pref(""), None);
        assert_eq!(KeyBinding::from_pref("<ctrl>"), None);
    }

    #[test]
    fn a_stored_name_that_is_not_a_key_cannot_fire() {
        let mut bindings = KeyBindings::defaults();
        let bogus = KeyBinding::from_pref("not-a-keyval").expect("a name is a name");
        bindings.bind(ReaderAction::Words, bogus);
        // It cannot be pressed, so it takes nothing with it.
        assert_eq!(bindings.action_for(gdk::Key::w, false), None);
        assert_eq!(
            bindings.action_for(gdk::Key::d, false),
            Some(ReaderAction::Define)
        );
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
        // And the dictionary does not sneak its default key back.
        assert_eq!(
            bindings.binding(ReaderAction::Define).label(),
            "None",
            "a displaced action is unbound, not restored to its default"
        );
    }

    #[test]
    fn an_uppercase_letter_is_the_same_key() {
        let bindings = KeyBindings::defaults();
        assert_eq!(
            bindings.action_for(gdk::Key::D, false),
            Some(ReaderAction::Define),
            "Shift+D is the key that says d"
        );
        assert_eq!(
            bindings.action_for(gdk::Key::N, false),
            Some(ReaderAction::NextChapter)
        );
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

    #[test]
    fn a_captured_key_becomes_a_binding() {
        let captured = KeyBinding::capture(gdk::Key::BackSpace, true).expect("a nameable key");
        assert_eq!(captured.label(), "Ctrl+Backspace");
        assert_eq!(captured.to_pref(), "<ctrl>BackSpace");
        assert_eq!(KeyBinding::from_pref(&captured.to_pref()), Some(captured));
    }
}
