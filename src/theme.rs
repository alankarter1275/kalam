//! Colour themes: the redesign palette system (`docs/design/design-system.md`).
//!
//! Every colour in Kalam's interface comes from here. `style.rs` holds the
//! *shape* of the UI — padding, radii, font sizes, borders — and refers to
//! colours only through `@kalam_*` names. This file defines what those names
//! mean for each palette, so adding a palette means adding one entry and
//! nothing else.
//!
//! Four palettes (Terracotta, Forest, Plum, Teal), each in light and dark,
//! replace the previous thirteen dark-only themes. A stored id from the old
//! system is unrecognised and quietly falls back to Terracotta Dark — every
//! old theme was dark, so nobody is surprised by a light window after
//! upgrading.
//!
//! Stored form is a *choice* — palette plus Light / Dark / System — under the
//! same `ui.theme` pref (`"terracotta-dark"`, `"forest-system"`, …). A
//! [`Theme`] is one resolved palette/mode combination with concrete colours.
//!
//! Two deliberate deviations from a literal reading of the design tokens,
//! both so the existing stylesheet keeps working while pages are rebuilt:
//!
//! - `border` is stored as the *solid equivalent* of the design's
//!   "text at 10% alpha" composited over `bg`, because the stylesheet applies
//!   `alpha(@kalam_border, …)` in dozens of places and GTK cannot nest
//!   `alpha()` inside `alpha()`. New components should use
//!   `alpha(@kalam_text, 0.10)` directly where the design asks for it.
//! - `danger` / `success` / `warning` / `info` are internal builder tokens
//!   for toasts, badges and charts. The design system leaves status colours
//!   TBD and uses the accent for destructive items in components; these stay
//!   defined per mode (not per palette) so existing chrome keeps working.
//!
//! The reading pane still keeps its own separate themes for now
//! (`KalamTheme` in `kalam-reader`); decision 19 re-bases them on these
//! palettes as part of the EPUB reader phase, not here. `paper` /
//! `paper_ink` below are the design's PDF page tokens, defined now so the
//! PDF chrome phase consumes tokens instead of inventing values.

/// One resolved palette/mode combination. Every field is a CSS colour
/// literal (or a GTK colour function over one, for the alpha tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Stable resolved id: `<palette>-light` or `<palette>-dark`.
    pub id: &'static str,
    /// Shown in Settings.
    pub label: &'static str,
    /// Palette family id (`"terracotta"`, …); the Settings picker groups by it.
    pub palette: &'static str,
    /// Whether this is the dark mode of its palette.
    pub dark: bool,

    // ── surfaces, darkest to lightest within the mode ──
    /// The window behind everything. Design token `bg`.
    pub bg: &'static str,
    /// Navigation rail.
    pub sidebar: &'static str,
    /// Cards, top bar, dialogs. Design token `card`.
    pub surface: &'static str,
    /// Raised elements: hover states, inputs, cover placeholders.
    pub surface_2: &'static str,
    /// Hairlines and card outlines (solid equivalent; see module docs).
    pub border: &'static str,

    // ── text ──
    /// Design token `text`.
    pub text: &'static str,
    /// Secondary text. Design token `text-2`.
    pub text_dim: &'static str,
    /// Decoration and placeholders only — never information. Design `text-3`.
    pub text_3: &'static str,

    // ── accents ──
    /// Design token `accent`: text, links, rings, bars, outlines.
    pub accent: &'static str,
    /// Design token `accent-fill`: fills that carry white text at 4.5:1.
    pub accent_fill: &'static str,
    /// Accent at low opacity, for the active nav pill.
    pub accent_dim: &'static str,

    /// PDF page paper under Tint (design-system §13). Always pale.
    pub paper: &'static str,

    // ── internal semantic tokens (see module docs) ──
    pub danger: &'static str,
    pub success: &'static str,
    pub warning: &'static str,
    pub info: &'static str,
}

/// Terracotta Light — warm paper, clay accent.
pub const TERRACOTTA_LIGHT: Theme = Theme {
    id: "terracotta-light",
    label: "Terracotta Light",
    palette: "terracotta",
    dark: false,
    bg: "#f7f3ec",
    sidebar: "#f7f3ec",
    surface: "#ffffff",
    surface_2: "#f4f4f3",
    border: "#d8d4ce",
    text: "#1c1917",
    text_dim: "#726b66",
    text_3: "#a8a29e",
    accent: "#c1440e",
    accent_fill: "#c1440e",
    accent_dim: "alpha(#c1440e, 0.16)",
    paper: "#ffffff",
    danger: "#b3261e",
    success: "#2f7d3a",
    warning: "#9a6700",
    info: "#2f5da8",
};

/// Terracotta Dark — the default.
pub const TERRACOTTA_DARK: Theme = Theme {
    id: "terracotta-dark",
    label: "Terracotta Dark",
    palette: "terracotta",
    dark: true,
    bg: "#1a1512",
    sidebar: "#14100e",
    surface: "#241d19",
    surface_2: "#322a26",
    border: "#3b3531",
    text: "#e8ddd5",
    text_dim: "#b3a8a0",
    text_3: "#807670",
    accent: "#d4552a",
    accent_fill: "#c54f27",
    accent_dim: "alpha(#d4552a, 0.16)",
    paper: "#cfc6bb",
    danger: "#e9897b",
    success: "#8fc97f",
    warning: "#e3b766",
    info: "#8ab4e8",
};

/// Forest Light — quiet green accent.
pub const FOREST_LIGHT: Theme = Theme {
    id: "forest-light",
    label: "Forest Light",
    palette: "forest",
    dark: false,
    bg: "#f3f6f3",
    sidebar: "#f3f6f3",
    surface: "#ffffff",
    surface_2: "#f4f4f4",
    border: "#d4d8d5",
    text: "#191e1a",
    text_dim: "#627165",
    text_3: "#9ca89e",
    accent: "#3a7d44",
    accent_fill: "#3a7d44",
    accent_dim: "alpha(#3a7d44, 0.16)",
    paper: "#ffffff",
    danger: "#b3261e",
    success: "#2f7d3a",
    warning: "#9a6700",
    info: "#2f5da8",
};

/// Forest Dark.
pub const FOREST_DARK: Theme = Theme {
    id: "forest-dark",
    label: "Forest Dark",
    palette: "forest",
    dark: true,
    bg: "#121a13",
    sidebar: "#0e140e",
    surface: "#202821",
    surface_2: "#2d352e",
    border: "#323b33",
    text: "#dce8dc",
    text_dim: "#9cb0a0",
    text_3: "#6b7a6e",
    accent: "#5a9e64",
    accent_fill: "#498051",
    accent_dim: "alpha(#5a9e64, 0.16)",
    paper: "#d0d8d1",
    danger: "#e9897b",
    success: "#8fc97f",
    warning: "#e3b766",
    info: "#8ab4e8",
};

/// Plum Light — muted plum accent.
pub const PLUM_LIGHT: Theme = Theme {
    id: "plum-light",
    label: "Plum Light",
    palette: "plum",
    dark: false,
    bg: "#f6f2f4",
    sidebar: "#f6f2f4",
    surface: "#ffffff",
    surface_2: "#f4f3f4",
    border: "#d8d3d6",
    text: "#1e1720",
    text_dim: "#776875",
    text_3: "#a89ba6",
    accent: "#7b2d5b",
    accent_fill: "#7b2d5b",
    accent_dim: "alpha(#7b2d5b, 0.16)",
    paper: "#ffffff",
    danger: "#b3261e",
    success: "#2f7d3a",
    warning: "#9a6700",
    info: "#2f5da8",
};

/// Plum Dark.
pub const PLUM_DARK: Theme = Theme {
    id: "plum-dark",
    label: "Plum Dark",
    palette: "plum",
    dark: true,
    bg: "#181017",
    sidebar: "#120c11",
    surface: "#271e25",
    surface_2: "#352b33",
    border: "#393138",
    text: "#e8dce6",
    text_dim: "#b3a3b0",
    text_3: "#7d6e7b",
    accent: "#b8609a",
    accent_fill: "#ab598f",
    accent_dim: "alpha(#b8609a, 0.16)",
    paper: "#d6d0d5",
    danger: "#e9897b",
    success: "#8fc97f",
    warning: "#e3b766",
    info: "#8ab4e8",
};

/// Teal Light — calm teal accent.
pub const TEAL_LIGHT: Theme = Theme {
    id: "teal-light",
    label: "Teal Light",
    palette: "teal",
    dark: false,
    bg: "#f1f5f5",
    sidebar: "#f1f5f5",
    surface: "#ffffff",
    surface_2: "#f3f4f4",
    border: "#d2d7d7",
    text: "#171e1e",
    text_dim: "#5f7171",
    text_3: "#9aaaaa",
    accent: "#1b6f70",
    accent_fill: "#1b6f70",
    accent_dim: "alpha(#1b6f70, 0.16)",
    paper: "#ffffff",
    danger: "#b3261e",
    success: "#2f7d3a",
    warning: "#9a6700",
    info: "#2f5da8",
};

/// Teal Dark.
pub const TEAL_DARK: Theme = Theme {
    id: "teal-dark",
    label: "Teal Dark",
    palette: "teal",
    dark: true,
    bg: "#0f1819",
    sidebar: "#0b1213",
    surface: "#1d2627",
    surface_2: "#2a3334",
    border: "#2f393a",
    text: "#d6e6e6",
    text_dim: "#98b0b0",
    text_3: "#678080",
    accent: "#3fa6a6",
    accent_fill: "#318080",
    accent_dim: "alpha(#3fa6a6, 0.16)",
    paper: "#cfd8d8",
    danger: "#e9897b",
    success: "#8fc97f",
    warning: "#e3b766",
    info: "#8ab4e8",
};

/// Every resolved theme, Light then Dark per palette. Production code
/// walks `Palette::ALL` and asks each palette for its pair; this flat
/// list exists for the tests that pin that ordering and the id set.
#[cfg(test)]
const ALL: &[Theme] = &[
    TERRACOTTA_LIGHT,
    TERRACOTTA_DARK,
    FOREST_LIGHT,
    FOREST_DARK,
    PLUM_LIGHT,
    PLUM_DARK,
    TEAL_LIGHT,
    TEAL_DARK,
];

/// The theme used on first run and whenever a stored id is unrecognised
/// (including every id from the previous thirteen-theme system).
pub const DEFAULT: Theme = TERRACOTTA_DARK;

/// One palette family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    Terracotta,
    Forest,
    Plum,
    Teal,
}

impl Palette {
    pub const ALL: &'static [Palette] = &[
        Palette::Terracotta,
        Palette::Forest,
        Palette::Plum,
        Palette::Teal,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Palette::Terracotta => "terracotta",
            Palette::Forest => "forest",
            Palette::Plum => "plum",
            Palette::Teal => "teal",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Palette::Terracotta => "Terracotta",
            Palette::Forest => "Forest",
            Palette::Plum => "Plum",
            Palette::Teal => "Teal",
        }
    }

    pub fn from_id(id: &str) -> Option<Palette> {
        Palette::ALL.iter().copied().find(|p| p.id() == id)
    }

    pub fn light(self) -> Theme {
        match self {
            Palette::Terracotta => TERRACOTTA_LIGHT,
            Palette::Forest => FOREST_LIGHT,
            Palette::Plum => PLUM_LIGHT,
            Palette::Teal => TEAL_LIGHT,
        }
    }

    pub fn dark(self) -> Theme {
        match self {
            Palette::Terracotta => TERRACOTTA_DARK,
            Palette::Forest => FOREST_DARK,
            Palette::Plum => PLUM_DARK,
            Palette::Teal => TEAL_DARK,
        }
    }

    pub fn theme(self, dark: bool) -> Theme {
        if dark {
            self.dark()
        } else {
            self.light()
        }
    }
}

/// How the mode is chosen: explicitly, or by following the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModePref {
    Light,
    Dark,
    System,
}

impl ModePref {
    pub fn id(self) -> &'static str {
        match self {
            ModePref::Light => "light",
            ModePref::Dark => "dark",
            ModePref::System => "system",
        }
    }
}

/// What is actually stored: a palette and a mode preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeChoice {
    pub palette: Palette,
    pub mode: ModePref,
}

impl ThemeChoice {
    pub const DEFAULT: ThemeChoice = ThemeChoice {
        palette: Palette::Terracotta,
        mode: ModePref::Dark,
    };

    /// Stored form, e.g. `"terracotta-dark"` or `"forest-system"`.
    pub fn id(self) -> String {
        format!("{}-{}", self.palette.id(), self.mode.id())
    }

    pub fn label(self) -> String {
        match self.mode {
            ModePref::System => format!("{} · System", self.palette.label()),
            ModePref::Light => self.palette.light().label.to_string(),
            ModePref::Dark => self.palette.dark().label.to_string(),
        }
    }

    /// The concrete theme, given whether the system is currently dark.
    pub fn resolve_with(self, system_dark: bool) -> Theme {
        match self.mode {
            ModePref::Light => self.palette.light(),
            ModePref::Dark => self.palette.dark(),
            ModePref::System => self.palette.theme(system_dark),
        }
    }

    /// The concrete theme right now.
    pub fn resolve(self) -> Theme {
        match self.mode {
            ModePref::System => self.resolve_with(system_is_dark()),
            _ => self.resolve_with(false),
        }
    }
}

impl Default for ThemeChoice {
    fn default() -> Self {
        ThemeChoice::DEFAULT
    }
}

impl From<&Theme> for ThemeChoice {
    fn from(theme: &Theme) -> Self {
        ThemeChoice {
            palette: Palette::from_id(theme.palette).unwrap_or(Palette::Terracotta),
            mode: if theme.dark {
                ModePref::Dark
            } else {
                ModePref::Light
            },
        }
    }
}

/// Parse a stored choice id. Anything unrecognised — including every id
/// from the old theme system — is the default choice.
///
/// Deliberately total: a pref left behind by a renamed or removed theme
/// should quietly fall back, never fail to start the app.
pub fn choice_by_id(id: &str) -> ThemeChoice {
    let Some((palette_id, mode_id)) = id.trim().rsplit_once('-') else {
        return ThemeChoice::DEFAULT;
    };
    let Some(palette) = Palette::from_id(palette_id) else {
        return ThemeChoice::DEFAULT;
    };
    let mode = match mode_id {
        "light" => ModePref::Light,
        "dark" => ModePref::Dark,
        "system" => ModePref::System,
        _ => return ThemeChoice::DEFAULT,
    };
    ThemeChoice { palette, mode }
}

/// Look up a resolved theme by stored id (`-system` ids resolve against
/// the current system mode), falling back to the default.
pub fn by_id(id: &str) -> Theme {
    choice_by_id(id).resolve()
}

/// Whether the system currently prefers dark, via Adwaita. Outside a
/// running GTK application (unit tests) there is no display to ask;
/// answer dark, which matches the default choice.
fn system_is_dark() -> bool {
    if gtk::gdk::Display::default().is_none() {
        return true;
    }
    relm4::adw::StyleManager::default().is_dark()
}

fn set_color_scheme(scheme: relm4::adw::ColorScheme) {
    if gtk::gdk::Display::default().is_some() {
        relm4::adw::StyleManager::default().set_color_scheme(scheme);
    }
}

/// The `@define-color` block prepended to the stylesheet.
///
/// GTK resolves `@name` references at parse time, so re-parsing the sheet
/// with a different block is all a theme switch requires.
pub fn css_variables(t: &Theme) -> String {
    // Derived alpha tokens (design-system §2, §9): computed from the core
    // colours here so each palette states its core values exactly once.
    let selection_alpha = if t.dark { 0.16 } else { 0.14 };
    let match_alpha = if t.dark { 0.10 } else { 0.08 };
    let hover_alpha = if t.dark { 0.08 } else { 0.06 };
    format!(
        "@define-color kalam_bg {bg};\n\
         @define-color kalam_sidebar {sidebar};\n\
         @define-color kalam_surface {surface};\n\
         @define-color kalam_surface_2 {surface_2};\n\
         @define-color kalam_card {surface};\n\
         @define-color kalam_border {border};\n\
         @define-color kalam_hover alpha({text}, {hover_alpha});\n\
         @define-color kalam_overlay alpha({bg}, 0.85);\n\
         @define-color kalam_text {text};\n\
         @define-color kalam_text_dim {text_dim};\n\
         @define-color kalam_text_3 {text_3};\n\
         @define-color kalam_text_bright {text};\n\
         @define-color kalam_accent {accent};\n\
         @define-color kalam_accent_fill {accent_fill};\n\
         @define-color kalam_accent_dim {accent_dim};\n\
         @define-color kalam_selection alpha({text}, {selection_alpha});\n\
         @define-color kalam_search_match alpha({text}, {match_alpha});\n\
         @define-color kalam_search_current alpha({accent}, 0.38);\n\
         @define-color kalam_paper {paper};\n\
         @define-color kalam_paper_ink #2a2420;\n\
         @define-color kalam_danger {danger};\n\
         @define-color kalam_success {success};\n\
         @define-color kalam_warning {warning};\n\
         @define-color kalam_info {info};\n",
        bg = t.bg,
        sidebar = t.sidebar,
        surface = t.surface,
        surface_2 = t.surface_2,
        border = t.border,
        text = t.text,
        text_dim = t.text_dim,
        text_3 = t.text_3,
        accent = t.accent,
        accent_fill = t.accent_fill,
        accent_dim = t.accent_dim,
        paper = t.paper,
        danger = t.danger,
        success = t.success,
        warning = t.warning,
        info = t.info,
    )
}

/// Pref key holding the chosen theme choice id.
pub const PREF_KEY: &str = "ui.theme";

/// The choice most recently installed by [`apply_choice`].
///
/// This is deliberately process-wide, not thread-local: the Settings
/// snapshot is built on a service worker, and it must report the palette
/// that is actually on screen. That matters when `KALAM_THEME` applies a
/// run-only choice without writing `ui.theme`; reading only the pref made
/// the Appearance picker mark the saved (dark) card in a light screenshot.
static APPLIED_CHOICE: std::sync::Mutex<Option<ThemeChoice>> = std::sync::Mutex::new(None);

fn applied_choice() -> Option<ThemeChoice> {
    match APPLIED_CHOICE.lock() {
        Ok(guard) => *guard,
        Err(poisoned) => *poisoned.into_inner(),
    }
}

fn remember_applied_choice(choice: &ThemeChoice) {
    let mut guard = match APPLIED_CHOICE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = Some(*choice);
}

/// Install one resolved theme's colours as the application's CSS.
fn install_css(theme: &Theme) {
    // The appearance override rides last: the Appearance pane's font
    // choices (decision 146) are generated rules appended after the app
    // sheet, where equal specificity means the later rule wins.
    let css = format!(
        "{}{}{}",
        css_variables(theme),
        crate::style::APP_CSS,
        crate::appearance::override_css()
    );
    relm4::set_global_css(&css);
    notify_theme_applied(theme);
}

// Listeners told each time a theme is installed.
//
// Almost everything recolours by CSS when the sheet is replaced. The one
// exception is art that had to be *rasterized* with a colour baked in —
// the rail's brand mark is an SVG path rendered to a texture with the
// accent substituted, because a texture has no CSS colour to inherit.
// Such pieces register here and rebuild themselves from the new theme.
//
// Listeners live for the process: the shell outlives every page, and a
// registry that never removes is the honest shape of that. Register from
// shell-lifetime widgets only.

/// A callback run after each theme install; see [`on_theme_applied`].
pub type ThemeAppliedListener = std::rc::Rc<dyn Fn(&Theme)>;

thread_local! {
    static THEME_APPLIED_LISTENERS: std::cell::RefCell<Vec<ThemeAppliedListener>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Register `listener` to run after every future theme install — Settings
/// changes and system light/dark flips alike, since both funnel through
/// [`apply_choice`]. The already-installed theme is *not* replayed; build
/// from [`current`] first, then register for changes.
pub fn on_theme_applied(listener: ThemeAppliedListener) {
    THEME_APPLIED_LISTENERS.with(|listeners| listeners.borrow_mut().push(listener));
}

fn notify_theme_applied(theme: &Theme) {
    // Clone the handles out first: a listener may itself register another
    // listener, and holding the borrow across the calls would panic.
    let listeners: Vec<ThemeAppliedListener> =
        THEME_APPLIED_LISTENERS.with(|listeners| listeners.borrow().clone());
    for listener in listeners {
        listener(theme);
    }
}

/// Parse and install `theme` + the stylesheet as the application's CSS,
/// forcing Adwaita's colour scheme to agree so stock widgets match.
///
/// Safe to call repeatedly: `set_global_css` replaces the previous sheet,
/// and GTK re-resolves `@name` references on every parse, so switching
/// themes is just another call. Widgets restyle in place.
pub fn apply(theme: &Theme) {
    set_color_scheme(if theme.dark {
        relm4::adw::ColorScheme::ForceDark
    } else {
        relm4::adw::ColorScheme::ForceLight
    });
    install_css(theme);
}

/// Apply a stored choice. `System` hands Adwaita back its default
/// (follow-the-desktop) behaviour *before* resolving, so the resolved
/// colours are the ones the desktop is actually asking for.
pub fn apply_choice(choice: &ThemeChoice) {
    remember_applied_choice(choice);
    match choice.mode {
        ModePref::Light => apply(&choice.palette.light()),
        ModePref::Dark => apply(&choice.palette.dark()),
        ModePref::System => {
            set_color_scheme(relm4::adw::ColorScheme::Default);
            install_css(&choice.resolve());
        }
    }
}

/// Re-apply the saved choice after the system flips light/dark. A no-op
/// unless the saved choice follows the system.
pub fn refresh_for_system_change(catalog: &crate::db::Catalog) {
    let choice = current_choice(catalog);
    if choice.mode == ModePref::System {
        apply_choice(&choice);
    }
}

/// Read the choice in force: the last one [`apply_choice`] installed,
/// or the saved choice when nothing has been applied yet (startup).
/// Falls back to the default when neither exists.
pub fn current_choice(catalog: &crate::db::Catalog) -> ThemeChoice {
    applied_choice().unwrap_or_else(|| {
        catalog
            .get_pref(PREF_KEY)
            .map(|id| choice_by_id(&id))
            .unwrap_or_default()
    })
}

/// Read the theme in force, resolved.
pub fn current(catalog: &crate::db::Catalog) -> Theme {
    current_choice(catalog).resolve()
}

/// Persist and apply a choice in one step.
pub fn save_and_apply_choice(catalog: &crate::db::Catalog, choice: &ThemeChoice) {
    catalog.set_pref(PREF_KEY, &choice.id());
    apply_choice(choice);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut seen = Vec::new();
        for t in ALL {
            assert!(!seen.contains(&t.id), "duplicate theme id: {}", t.id);
            seen.push(t.id);
        }
    }

    #[test]
    fn light_dark_pairs_are_adjacent_per_palette() {
        // The picker relies on this ordering to render each palette as a pair.
        assert_eq!(ALL.len(), 8);
        for pair in ALL.chunks(2) {
            assert_eq!(pair[0].palette, pair[1].palette);
            assert!(!pair[0].dark, "{} should be the light variant", pair[0].id);
            assert!(pair[1].dark, "{} should be the dark variant", pair[1].id);
        }
    }

    #[test]
    fn default_is_terracotta_dark() {
        assert_eq!(DEFAULT.id, TERRACOTTA_DARK.id);
        assert_eq!(ThemeChoice::default().id(), DEFAULT.id);
    }

    #[test]
    fn unknown_and_legacy_ids_fall_back_to_default() {
        assert_eq!(by_id("no-such-theme").id, DEFAULT.id);
        assert_eq!(by_id("").id, DEFAULT.id);
        // Ids stored by the previous thirteen-theme system.
        assert_eq!(by_id("onedark-darker").id, DEFAULT.id);
        assert_eq!(by_id("nord").id, DEFAULT.id);
        assert_eq!(choice_by_id("catppuccin"), ThemeChoice::DEFAULT);
    }

    #[test]
    fn every_theme_round_trips_by_id() {
        for t in ALL {
            assert_eq!(by_id(t.id).id, t.id);
        }
    }

    #[test]
    fn choice_parsing_covers_system_and_palettes() {
        assert_eq!(
            choice_by_id("forest-system"),
            ThemeChoice {
                palette: Palette::Forest,
                mode: ModePref::System,
            }
        );
        assert_eq!(
            choice_by_id("teal-light"),
            ThemeChoice {
                palette: Palette::Teal,
                mode: ModePref::Light,
            }
        );
        for p in Palette::ALL {
            assert_eq!(p.theme(true).palette, p.id());
            assert_eq!(p.theme(false).palette, p.id());
        }
    }

    #[test]
    fn system_choice_resolves_against_the_given_mode() {
        let choice = ThemeChoice {
            palette: Palette::Plum,
            mode: ModePref::System,
        };
        assert_eq!(choice.resolve_with(true).id, PLUM_DARK.id);
        assert_eq!(choice.resolve_with(false).id, PLUM_LIGHT.id);
    }

    #[test]
    fn variables_cover_every_token_the_stylesheet_uses() {
        // If a token is used in style.css but not emitted here, GTK logs a
        // parser error and the colour silently falls back to black. This
        // has bitten before (@kalam_hover / @kalam_text_bright were used
        // for a long time without ever being defined), so collect the
        // tokens from the stylesheet itself instead of listing them.
        let css = css_variables(&DEFAULT);
        let mut used = std::collections::BTreeSet::new();
        for part in crate::style::APP_CSS.split("@kalam_").skip(1) {
            let token: String = part
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !token.is_empty() {
                used.insert(format!("kalam_{token}"));
            }
        }
        assert!(!used.is_empty(), "token scan found nothing to check");
        for token in &used {
            assert!(
                css.contains(&format!("@define-color {token} ")),
                "stylesheet uses @{token} but css_variables does not define it"
            );
        }
    }

    #[test]
    fn all_themes_define_every_colour() {
        // A theme with an empty field would produce invalid CSS.
        for t in ALL {
            for (name, value) in [
                ("bg", t.bg),
                ("sidebar", t.sidebar),
                ("surface", t.surface),
                ("surface_2", t.surface_2),
                ("border", t.border),
                ("text", t.text),
                ("text_dim", t.text_dim),
                ("text_3", t.text_3),
                ("accent", t.accent),
                ("accent_fill", t.accent_fill),
                ("accent_dim", t.accent_dim),
                ("paper", t.paper),
                ("danger", t.danger),
                ("success", t.success),
                ("warning", t.warning),
                ("info", t.info),
            ] {
                assert!(!value.is_empty(), "{} has empty {name}", t.id);
            }
        }
    }

    #[test]
    fn theme_applied_listeners_receive_the_installed_theme() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen_clone = seen.clone();
        on_theme_applied(std::rc::Rc::new(move |theme: &Theme| {
            seen_clone.borrow_mut().push(theme.id);
        }));

        notify_theme_applied(&TERRACOTTA_LIGHT);
        notify_theme_applied(&FOREST_DARK);

        assert_eq!(
            seen.borrow().as_slice(),
            [TERRACOTTA_LIGHT.id, FOREST_DARK.id]
        );
    }
}
