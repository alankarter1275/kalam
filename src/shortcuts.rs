//! App-wide keyboard shortcuts (decision 155).
//!
//! This is the shell's registry, separate from the EPUB reader's own
//! `keybinds` table: these actions navigate the app, open its panels
//! and start library work. Settings renders and records this table;
//! the root window's key handler reads the same shared copy, so a
//! successful rebind takes effect immediately and survives through
//! the global `ui.shortcut.*` preferences.
//!
//! A taken key is refused rather than stolen (decision 24). The
//! Settings row stays in its recording state and names the action
//! that already owns the key.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gdk::{self, ModifierType};

use crate::db::Catalog;

/// One action in Settings' four drawn shortcut groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    GoHome,
    GoLibrary,
    OpenSettings,
    Back,
    Notifications,
    SearchLibrary,
    ImportFiles,
    ImportFolder,
    Fullscreen,
    ShowShortcuts,
    Quit,
    UndoDelete,
    ClosePanel,
}

impl ShortcutAction {
    pub const ALL: [Self; 13] = [
        Self::GoHome,
        Self::GoLibrary,
        Self::OpenSettings,
        Self::Back,
        Self::Notifications,
        Self::SearchLibrary,
        Self::ImportFiles,
        Self::ImportFolder,
        Self::Fullscreen,
        Self::ShowShortcuts,
        Self::Quit,
        Self::UndoDelete,
        Self::ClosePanel,
    ];

    /// Stable preference suffix and widget-name fragment. Renaming one
    /// would orphan a user's saved binding.
    pub fn id(self) -> &'static str {
        match self {
            Self::GoHome => "nav-home",
            Self::GoLibrary => "nav-library",
            Self::GoEditor => "nav-editor",
            Self::OpenSettings => "nav-settings",
            Self::Back => "nav-back",
            Self::Notifications => "nav-notifications",
            Self::SearchLibrary => "library-search",
            Self::ImportFiles => "library-import",
            Self::ImportFolder => "library-import-folder",
            Self::Fullscreen => "window-fullscreen",
            Self::ShowShortcuts => "window-shortcuts",
            Self::Quit => "window-quit",
            Self::UndoDelete => "app-undo-delete",
            Self::ClosePanel => "app-close-panel",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GoHome => "Go to Home",
            Self::GoLibrary => "Go to Library",
            Self::OpenSettings => "Open Settings",
            Self::Back => "Back",
            Self::Notifications => "Notification history",
            Self::SearchLibrary => "Search the library",
            Self::ImportFiles => "Import EPUB",
            Self::ImportFolder => "Import from a folder",
            Self::Fullscreen => "Full screen",
            Self::ShowShortcuts => "Show shortcuts",
            Self::Quit => "Quit",
            Self::UndoDelete => "Undo the last delete",
            Self::ClosePanel => "Close the open panel",
        }
    }

    fn default_binding(self) -> ShortcutBinding {
        match self {
            Self::GoHome => ShortcutBinding::alt(gdk::Key::_1),
            Self::GoLibrary => ShortcutBinding::alt(gdk::Key::_2),
            Self::OpenSettings => ShortcutBinding::ctrl(gdk::Key::comma),
            Self::Back => ShortcutBinding::alt(gdk::Key::Left),
            Self::Notifications => ShortcutBinding::ctrl_shift(gdk::Key::n),
            Self::SearchLibrary => ShortcutBinding::ctrl(gdk::Key::f),
            Self::ImportFiles => ShortcutBinding::ctrl(gdk::Key::i),
            Self::ImportFolder => ShortcutBinding::ctrl_shift(gdk::Key::i),
            Self::Fullscreen => ShortcutBinding::plain(gdk::Key::F11),
            Self::ShowShortcuts => ShortcutBinding::ctrl(gdk::Key::slash),
            Self::Quit => ShortcutBinding::ctrl(gdk::Key::q),
            Self::UndoDelete => ShortcutBinding::ctrl(gdk::Key::z),
            Self::ClosePanel => ShortcutBinding::plain(gdk::Key::Escape),
        }
    }

    pub fn pref_key(self) -> String {
        format!("ui.shortcut.{}", self.id())
    }
}

/// The four groups exactly as decision 155 draws them.
pub const SHORTCUT_GROUPS: &[(&str, &[ShortcutAction])] = &[
    (
        "Navigation",
        &[
            ShortcutAction::GoHome,
            ShortcutAction::GoLibrary,
            ShortcutAction::OpenSettings,
            ShortcutAction::Back,
            ShortcutAction::Notifications,
        ],
    ),
    (
        "Library",
        &[
            ShortcutAction::SearchLibrary,
            ShortcutAction::ImportFiles,
            ShortcutAction::ImportFolder,
        ],
    ),
    (
        "Window",
        &[
            ShortcutAction::Fullscreen,
            ShortcutAction::ShowShortcuts,
            ShortcutAction::Quit,
        ],
    ),
    (
        "Everywhere",
        &[ShortcutAction::UndoDelete, ShortcutAction::ClosePanel],
    ),
];

/// A named GDK key plus the modifiers that belong with it.
///
/// The key is stored by toolkit name (`n`, `comma`, `Left`, `F11`),
/// with one-character letter names lower-cased: Shift is carried in
/// its own field, so `Ctrl+Shift+N` and a Caps-Lock `Ctrl+N` remain
/// different bindings even though both presses report the `N` keyval.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShortcutBinding {
    key: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    super_key: bool,
}

impl ShortcutBinding {
    fn new(key: gdk::Key, ctrl: bool, alt: bool, shift: bool, super_key: bool) -> Self {
        Self {
            key: normalized_key_name(key).unwrap_or_default(),
            ctrl,
            alt,
            shift,
            super_key,
        }
    }

    fn plain(key: gdk::Key) -> Self {
        Self::new(key, false, false, false, false)
    }

    fn ctrl(key: gdk::Key) -> Self {
        Self::new(key, true, false, false, false)
    }

    fn alt(key: gdk::Key) -> Self {
        Self::new(key, false, true, false, false)
    }

    fn ctrl_shift(key: gdk::Key) -> Self {
        Self::new(key, true, false, true, false)
    }

    /// The binding described by one key press, or `None` while only a
    /// modifier (or a lock key) is down. Returning `None` there is
    /// what lets the Settings recorder wait for the rest of the chord.
    pub fn capture(key: gdk::Key, state: ModifierType) -> Option<Self> {
        let name = normalized_key_name(key)?;
        if is_modifier_or_lock_key(&name) {
            return None;
        }
        Some(Self {
            key: name,
            ctrl: state.contains(ModifierType::CONTROL_MASK),
            alt: state.contains(ModifierType::ALT_MASK),
            shift: state.contains(ModifierType::SHIFT_MASK),
            super_key: state.contains(ModifierType::SUPER_MASK)
                || state.contains(ModifierType::META_MASK),
        })
    }

    /// Whether this chord may take precedence over text entry. A
    /// command modifier or a non-printing function/Escape key can;
    /// a plain letter, arrow or Space must leave typing alone.
    pub fn fires_while_typing(&self) -> bool {
        if self.ctrl || self.alt || self.super_key || self.key == "Escape" {
            return true;
        }
        let Some(number) = self.key.strip_prefix('F') else {
            return false;
        };
        !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit())
            && number.parse::<u32>().is_ok_and(|n| (1..=35).contains(&n))
    }

    /// One display part per key chip: modifiers first, then the key.
    pub fn display_parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.super_key {
            parts.push("Super".to_string());
        }
        parts.push(display_name(&self.key));
        parts
    }

    /// The one-line spelling used by conflict messages and tooltips.
    pub fn display_label(&self) -> String {
        self.display_parts().join("+")
    }

    /// Preference spelling, in the same family as the reader's
    /// `<ctrl>f`: `<ctrl><shift>n`, `<alt>1`, `F11`, `Escape`.
    pub fn to_pref(&self) -> String {
        let mut stored = String::new();
        if self.ctrl {
            stored.push_str("<ctrl>");
        }
        if self.alt {
            stored.push_str("<alt>");
        }
        if self.shift {
            stored.push_str("<shift>");
        }
        if self.super_key {
            stored.push_str("<super>");
        }
        stored.push_str(&self.key);
        stored
    }

    pub fn from_pref(value: &str) -> Option<Self> {
        let mut rest = value.trim();
        let (mut ctrl, mut alt, mut shift, mut super_key) = (false, false, false, false);

        loop {
            let lower = rest.to_ascii_lowercase();
            let (len, flag) = if lower.starts_with("<ctrl>") {
                ("<ctrl>".len(), 0)
            } else if lower.starts_with("<control>") {
                ("<control>".len(), 0)
            } else if lower.starts_with("<alt>") {
                ("<alt>".len(), 1)
            } else if lower.starts_with("<mod1>") {
                ("<mod1>".len(), 1)
            } else if lower.starts_with("<shift>") {
                ("<shift>".len(), 2)
            } else if lower.starts_with("<super>") {
                ("<super>".len(), 3)
            } else if lower.starts_with("<meta>") {
                ("<meta>".len(), 3)
            } else {
                break;
            };
            match flag {
                0 => ctrl = true,
                1 => alt = true,
                2 => shift = true,
                _ => super_key = true,
            }
            rest = &rest[len..];
        }

        if rest.is_empty()
            || rest.chars().any(|c| c.is_whitespace() || c == '<' || c == '>')
            || is_modifier_or_lock_key(rest)
        {
            return None;
        }
        Some(Self {
            key: normalize_stored_name(rest),
            ctrl,
            alt,
            shift,
            super_key,
        })
    }
}

/// The complete app-shortcut table.
#[derive(Debug, Clone)]
pub struct ShortcutBindings {
    map: HashMap<ShortcutAction, ShortcutBinding>,
}

impl ShortcutBindings {
    pub fn defaults() -> Self {
        Self {
            map: ShortcutAction::ALL
                .into_iter()
                .map(|action| (action, action.default_binding()))
                .collect(),
        }
    }

    /// Defaults overlaid with any parseable saved bindings. A bad
    /// stored value falls back to the default instead of inventing a
    /// key that can never be pressed.
    pub fn load(catalog: &Catalog) -> Self {
        let mut bindings = Self::defaults();
        for action in ShortcutAction::ALL {
            if let Some(saved) = catalog
                .get_pref(&action.pref_key())
                .and_then(|value| ShortcutBinding::from_pref(&value))
            {
                bindings.map.insert(action, saved);
            }
        }
        bindings
    }

    pub fn binding(&self, action: ShortcutAction) -> ShortcutBinding {
        self.map
            .get(&action)
            .cloned()
            .unwrap_or_else(|| action.default_binding())
    }

    /// The first action, in drawn order, that owns `binding`.
    pub fn owner(&self, binding: &ShortcutBinding) -> Option<ShortcutAction> {
        ShortcutAction::ALL
            .into_iter()
            .find(|action| &self.binding(*action) == binding)
    }

    /// Bind only if no other action owns the chord. On conflict the
    /// table is untouched and the owner is returned for the message.
    pub fn try_bind(
        &mut self,
        action: ShortcutAction,
        binding: ShortcutBinding,
    ) -> Result<(), ShortcutAction> {
        if let Some(owner) = self.owner(&binding) {
            if owner != action {
                return Err(owner);
            }
        }
        self.map.insert(action, binding);
        Ok(())
    }

    pub fn action_for(&self, key: gdk::Key, state: ModifierType) -> Option<ShortcutAction> {
        let pressed = ShortcutBinding::capture(key, state)?;
        self.owner(&pressed)
    }
}

thread_local! {
    /// The main thread's one live table. App and Settings are both
    /// GTK-main-thread components; sharing the `Rc` is what makes a
    /// rebind effective without rebuilding the root key handler.
    static SHARED_BINDINGS: RefCell<Option<Rc<RefCell<ShortcutBindings>>>> =
        const { RefCell::new(None) };
}

/// The shell-wide bindings, loaded from `catalog` on first use.
pub fn shared(catalog: &Catalog) -> Rc<RefCell<ShortcutBindings>> {
    SHARED_BINDINGS.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| Rc::new(RefCell::new(ShortcutBindings::load(catalog))))
            .clone()
    })
}

fn normalized_key_name(key: gdk::Key) -> Option<String> {
    key.name().map(|name| normalize_stored_name(&name))
}

fn normalize_stored_name(name: &str) -> String {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => c.to_ascii_lowercase().to_string(),
        _ => name.to_string(),
    }
}

fn is_modifier_or_lock_key(name: &str) -> bool {
    matches!(
        name,
        "Control_L"
            | "Control_R"
            | "Shift_L"
            | "Shift_R"
            | "Alt_L"
            | "Alt_R"
            | "Super_L"
            | "Super_R"
            | "Meta_L"
            | "Meta_R"
            | "Hyper_L"
            | "Hyper_R"
            | "Caps_Lock"
            | "Num_Lock"
            | "Scroll_Lock"
            | "ISO_Level3_Shift"
            | "Mode_switch"
            | "VoidSymbol"
    )
}

/// A key name as the keyboard (and the drawn chips) spell it.
fn display_name(name: &str) -> String {
    match name {
        "comma" => ",".to_string(),
        "period" => ".".to_string(),
        "slash" => "/".to_string(),
        "backslash" => "\\".to_string(),
        "semicolon" => ";".to_string(),
        "apostrophe" => "'".to_string(),
        "bracketleft" => "[".to_string(),
        "bracketright" => "]".to_string(),
        "grave" => "`".to_string(),
        "minus" => "−".to_string(),
        "equal" => "=".to_string(),
        "plus" => "+".to_string(),
        "Left" => "←".to_string(),
        "Right" => "→".to_string(),
        "Up" => "↑".to_string(),
        "Down" => "↓".to_string(),
        "Escape" => "Esc".to_string(),
        "BackSpace" => "Backspace".to_string(),
        "Return" | "KP_Enter" => "Enter".to_string(),
        "space" => "Space".to_string(),
        "Tab" | "ISO_Left_Tab" => "Tab".to_string(),
        "Page_Up" => "Page Up".to_string(),
        "Page_Down" => "Page Down".to_string(),
        _ => {
            let mut chars = name.chars();
            match chars.next() {
                Some(c) if name.chars().count() == 1 => c.to_uppercase().to_string(),
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str().replace('_', " ")),
                None => "?".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawn_defaults_are_exact_and_unique() {
        let bindings = ShortcutBindings::defaults();
        let expected = [
            (ShortcutAction::GoHome, "Alt+1"),
            (ShortcutAction::GoLibrary, "Alt+2"),
            (ShortcutAction::OpenSettings, "Ctrl+,"),
            (ShortcutAction::Back, "Alt+←"),
            (ShortcutAction::Notifications, "Ctrl+Shift+N"),
            (ShortcutAction::SearchLibrary, "Ctrl+F"),
            (ShortcutAction::ImportFiles, "Ctrl+I"),
            (ShortcutAction::ImportFolder, "Ctrl+Shift+I"),
            (ShortcutAction::Fullscreen, "F11"),
            (ShortcutAction::ShowShortcuts, "Ctrl+/"),
            (ShortcutAction::Quit, "Ctrl+Q"),
            (ShortcutAction::UndoDelete, "Ctrl+Z"),
            (ShortcutAction::ClosePanel, "Esc"),
        ];
        for (action, label) in expected {
            assert_eq!(bindings.binding(action).display_label(), label);
        }

        for action in ShortcutAction::ALL {
            assert_eq!(bindings.owner(&bindings.binding(action)), Some(action));
        }
    }

    #[test]
    fn every_default_round_trips_through_its_preference_spelling() {
        for action in ShortcutAction::ALL {
            let binding = action.default_binding();
            assert_eq!(ShortcutBinding::from_pref(&binding.to_pref()), Some(binding));
        }
        assert_eq!(
            ShortcutAction::Notifications.default_binding().to_pref(),
            "<ctrl><shift>n"
        );
        assert_eq!(ShortcutBinding::from_pref("<ctrl>"), None);
        assert_eq!(ShortcutBinding::from_pref(""), None);
    }

    #[test]
    fn capture_keeps_shift_separate_from_the_letter() {
        let pressed = ShortcutBinding::capture(
            gdk::Key::N,
            ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK,
        );
        assert_eq!(
            pressed,
            Some(ShortcutAction::Notifications.default_binding())
        );
        assert_eq!(
            ShortcutBindings::defaults().action_for(
                gdk::Key::N,
                ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK,
            ),
            Some(ShortcutAction::Notifications)
        );
        assert_eq!(
            ShortcutBindings::defaults().action_for(gdk::Key::N, ModifierType::CONTROL_MASK),
            None,
            "Caps Lock must not turn Ctrl+N into Ctrl+Shift+N"
        );
    }

    #[test]
    fn a_conflict_is_refused_without_moving_either_binding() {
        let mut bindings = ShortcutBindings::defaults();
        let taken = bindings.binding(ShortcutAction::SearchLibrary);
        let original = bindings.binding(ShortcutAction::ImportFiles);

        assert_eq!(
            bindings.try_bind(ShortcutAction::ImportFiles, taken.clone()),
            Err(ShortcutAction::SearchLibrary)
        );
        assert_eq!(bindings.binding(ShortcutAction::ImportFiles), original);
        assert_eq!(bindings.binding(ShortcutAction::SearchLibrary), taken);

        let free = ShortcutBinding::ctrl_shift(gdk::Key::f);
        assert_eq!(
            bindings.try_bind(ShortcutAction::ImportFiles, free.clone()),
            Ok(())
        );
        assert_eq!(bindings.binding(ShortcutAction::ImportFiles), free);
    }

    #[test]
    fn modifier_only_presses_do_not_complete_a_chord() {
        assert_eq!(
            ShortcutBinding::capture(gdk::Key::Control_L, ModifierType::CONTROL_MASK),
            None
        );
        assert_eq!(
            ShortcutBinding::capture(gdk::Key::Shift_L, ModifierType::SHIFT_MASK),
            None
        );
    }

    #[test]
    fn plain_typing_keys_do_not_fire_from_a_text_field() {
        assert!(ShortcutAction::SearchLibrary
            .default_binding()
            .fires_while_typing());
        assert!(ShortcutAction::Fullscreen
            .default_binding()
            .fires_while_typing());
        assert!(ShortcutAction::ClosePanel
            .default_binding()
            .fires_while_typing());
        assert!(!ShortcutBinding::plain(gdk::Key::h).fires_while_typing());
        assert!(!ShortcutBinding::plain(gdk::Key::Left).fires_while_typing());
    }
}
