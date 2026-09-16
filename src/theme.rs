//! Colour themes.
//!
//! Every colour in Kalam's interface comes from here. `style.rs` holds the
//! *shape* of the UI — padding, radii, font sizes, borders — and refers to
//! colours only through `@kalam_*` names. This file defines what those names
//! mean for each theme, so adding a palette means adding one `Theme` entry
//! and nothing else.
//!
//! All seven are dark. That is deliberate: Kalam is a reading app used at
//! night, and supporting light variants would double the surface area of
//! every future design change for a mode nobody asked for.
//!
//! The reading pane keeps its own separate Light/Sepia/Dark
//! setting (see `epub_book::ReadingTheme`). Paper colour and chrome colour are
//! different decisions: plenty of people want a sepia page inside a dark app.

/// One palette. Every field is a CSS colour literal.
///
/// The set is deliberately small. A theme that needs a colour not in this
/// list is a sign the UI is inventing colours instead of reusing tokens,
/// which is exactly what made the old stylesheet hard to retheme.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Stable key stored in `app_prefs`; never shown to the user.
    pub id: &'static str,
    /// Shown in Settings.
    pub label: &'static str,

    // ── surfaces, darkest to lightest ──
    /// The window behind everything.
    pub bg: &'static str,
    /// Navigation rail. Usually a shade darker than `bg`.
    pub sidebar: &'static str,
    /// Cards, top bar, dialogs.
    pub surface: &'static str,
    /// Raised elements: hover states, inputs, cover placeholders.
    pub surface_2: &'static str,
    /// Hairlines and card outlines.
    pub border: &'static str,

    // ── text ──
    pub text: &'static str,
    /// Secondary text: authors, captions, metadata.
    pub text_dim: &'static str,

    // ── accents ──
    /// Primary accent: active nav, links, progress, chart lines.
    pub accent: &'static str,
    /// Accent at low opacity, for the active nav pill.
    pub accent_dim: &'static str,
    /// Destructive actions and error toasts.
    pub danger: &'static str,
    /// Success toasts and finished badges.
    pub success: &'static str,
    /// Warnings and in-progress badges.
    pub warning: &'static str,
    /// A fourth hue, for charts and tag chips that must not read as accent.
    pub info: &'static str,
}

/// One Dark — the default. Warm-tinted greys, soft blue accent.
pub const ONEDARK: Theme = Theme {
    id: "onedark",
    label: "One Dark",
    bg: "#282c34",
    sidebar: "#21252b",
    surface: "#2c313a",
    surface_2: "#333842",
    border: "#3e4451",
    text: "#abb2bf",
    text_dim: "#7f8794",
    accent: "#61afef",
    accent_dim: "alpha(#61afef, 0.16)",
    danger: "#e06c75",
    success: "#98c379",
    warning: "#e5c07b",
    info: "#c678dd",
};

/// Tokyo Night — deep indigo, high-chroma accents.
pub const TOKYONIGHT: Theme = Theme {
    id: "tokyonight",
    label: "Tokyo Night",
    bg: "#1a1b26",
    sidebar: "#16161e",
    surface: "#1f2335",
    surface_2: "#272b3f",
    border: "#2f334d",
    text: "#c0caf5",
    text_dim: "#787c99",
    accent: "#7aa2f7",
    accent_dim: "alpha(#7aa2f7, 0.16)",
    danger: "#f7768e",
    success: "#9ece6a",
    warning: "#e0af68",
    info: "#bb9af7",
};

/// Everforest Dark — low-saturation green-grey, easiest on the eyes.
pub const EVERFOREST: Theme = Theme {
    id: "everforest",
    label: "Everforest",
    bg: "#2d353b",
    sidebar: "#272e33",
    surface: "#343f44",
    surface_2: "#3d484d",
    border: "#475258",
    text: "#d3c6aa",
    text_dim: "#9da9a0",
    accent: "#a7c080",
    accent_dim: "alpha(#a7c080, 0.16)",
    danger: "#e67e80",
    success: "#a7c080",
    warning: "#dbbc7f",
    info: "#7fbbb3",
};

/// Catppuccin Mocha — soft pastels on a near-black lavender base.
pub const CATPPUCCIN: Theme = Theme {
    id: "catppuccin",
    label: "Catppuccin Mocha",
    bg: "#1e1e2e",
    sidebar: "#181825",
    surface: "#232338",
    surface_2: "#313244",
    border: "#45475a",
    text: "#cdd6f4",
    text_dim: "#9399b2",
    accent: "#89b4fa",
    accent_dim: "alpha(#89b4fa, 0.16)",
    danger: "#f38ba8",
    success: "#a6e3a1",
    warning: "#f9e2af",
    info: "#cba6f7",
};

/// Gruvbox Dark — warm retro browns and ochres.
pub const GRUVBOX: Theme = Theme {
    id: "gruvbox",
    label: "Gruvbox",
    bg: "#282828",
    sidebar: "#1d2021",
    surface: "#32302f",
    surface_2: "#3c3836",
    border: "#504945",
    text: "#ebdbb2",
    text_dim: "#a89984",
    accent: "#83a598",
    accent_dim: "alpha(#83a598, 0.16)",
    danger: "#fb4934",
    success: "#b8bb26",
    warning: "#fabd2f",
    info: "#d3869b",
};

/// Nord — cool arctic blue-greys.
pub const NORD: Theme = Theme {
    id: "nord",
    label: "Nord",
    bg: "#2e3440",
    sidebar: "#272b35",
    surface: "#3b4252",
    surface_2: "#434c5e",
    border: "#4c566a",
    text: "#eceff4",
    text_dim: "#a0a8b7",
    accent: "#88c0d0",
    accent_dim: "alpha(#88c0d0, 0.16)",
    danger: "#bf616a",
    success: "#a3be8c",
    warning: "#ebcb8b",
    info: "#b48ead",
};

/// Ayu Mirage — muted slate with a distinctive amber accent.
pub const AYU_MIRAGE: Theme = Theme {
    id: "ayumirage",
    label: "Ayu Mirage",
    bg: "#1f2430",
    sidebar: "#1a1f29",
    surface: "#242936",
    surface_2: "#2d3441",
    border: "#3a4251",
    text: "#cbccc6",
    text_dim: "#8a9199",
    accent: "#ffcc66",
    accent_dim: "alpha(#ffcc66, 0.16)",
    danger: "#f28779",
    success: "#bae67e",
    warning: "#ffd580",
    info: "#73d0ff",
};

// ── Darker variants ────────────────────────────────────────────────────────
// Where upstream publishes an official darker mode (Tokyo Night "Night",
// Everforest "hard", Gruvbox "hard", Ayu "Dark") those exact values are used.
// The rest deepen the surfaces while keeping hue and accents intact, so a pair
// still reads as the same theme.

/// One Dark Darker — the default. Same palette, deeper surfaces.
pub const ONEDARK_DARKER: Theme = Theme {
    id: "onedark-darker",
    label: "One Dark Darker",
    bg: "#1b1e24",
    sidebar: "#15171c",
    surface: "#21242b",
    surface_2: "#282c34",
    border: "#343842",
    text: "#abb2bf",
    text_dim: "#767d8a",
    accent: "#61afef",
    accent_dim: "alpha(#61afef, 0.16)",
    danger: "#e06c75",
    success: "#98c379",
    warning: "#e5c07b",
    info: "#c678dd",
};

/// Tokyo Night Night — upstream's darker variant.
pub const TOKYONIGHT_DARKER: Theme = Theme {
    id: "tokyonight-darker",
    label: "Tokyo Night (Night)",
    bg: "#16161e",
    sidebar: "#101014",
    surface: "#1a1b26",
    surface_2: "#22232f",
    border: "#292e42",
    text: "#c0caf5",
    text_dim: "#737aa2",
    accent: "#7aa2f7",
    accent_dim: "alpha(#7aa2f7, 0.16)",
    danger: "#f7768e",
    success: "#9ece6a",
    warning: "#e0af68",
    info: "#bb9af7",
};

/// Everforest Hard — upstream's darkest background.
pub const EVERFOREST_DARKER: Theme = Theme {
    id: "everforest-darker",
    label: "Everforest (Hard)",
    bg: "#232a2e",
    sidebar: "#1e2326",
    surface: "#2d353b",
    surface_2: "#343f44",
    border: "#3d484d",
    text: "#d3c6aa",
    text_dim: "#859289",
    accent: "#a7c080",
    accent_dim: "alpha(#a7c080, 0.16)",
    danger: "#e67e80",
    success: "#a7c080",
    warning: "#dbbc7f",
    info: "#7fbbb3",
};

/// Catppuccin Crust — the darkest base in the Mocha family.
pub const CATPPUCCIN_DARKER: Theme = Theme {
    id: "catppuccin-darker",
    label: "Catppuccin (Crust)",
    bg: "#11111b",
    sidebar: "#0b0b13",
    surface: "#181825",
    surface_2: "#1e1e2e",
    border: "#313244",
    text: "#cdd6f4",
    text_dim: "#7f849c",
    accent: "#89b4fa",
    accent_dim: "alpha(#89b4fa, 0.16)",
    danger: "#f38ba8",
    success: "#a6e3a1",
    warning: "#f9e2af",
    info: "#cba6f7",
};

/// Gruvbox Hard — upstream's hard contrast background.
pub const GRUVBOX_DARKER: Theme = Theme {
    id: "gruvbox-darker",
    label: "Gruvbox (Hard)",
    bg: "#1d2021",
    sidebar: "#141617",
    surface: "#282828",
    surface_2: "#32302f",
    border: "#3c3836",
    text: "#ebdbb2",
    text_dim: "#928374",
    accent: "#83a598",
    accent_dim: "alpha(#83a598, 0.16)",
    danger: "#fb4934",
    success: "#b8bb26",
    warning: "#fabd2f",
    info: "#d3869b",
};

/// Ayu Dark — upstream's darkest Ayu.
pub const AYU_DARKER: Theme = Theme {
    id: "ayu-darker",
    label: "Ayu Dark",
    bg: "#0f1419",
    sidebar: "#0b0e13",
    surface: "#151a1e",
    surface_2: "#1c2228",
    border: "#273038",
    text: "#bfbdb6",
    text_dim: "#7b8288",
    accent: "#e6b450",
    accent_dim: "alpha(#e6b450, 0.16)",
    danger: "#f07178",
    success: "#aad94c",
    warning: "#ffb454",
    info: "#59c2ff",
};

/// Every theme, in the order Settings lists them.
pub const ALL: &[Theme] = &[
    // Grouped by family, standard then darker, so a pair sits side by side in
    // the picker. Nord has no darker variant: its Polar Night base is already
    // the darkest of these palettes and a deeper one lost the character.
    ONEDARK,
    ONEDARK_DARKER,
    TOKYONIGHT,
    TOKYONIGHT_DARKER,
    EVERFOREST,
    EVERFOREST_DARKER,
    CATPPUCCIN,
    CATPPUCCIN_DARKER,
    GRUVBOX,
    GRUVBOX_DARKER,
    AYU_MIRAGE,
    AYU_DARKER,
    NORD,
];

/// The theme used on first run and whenever a stored id is unrecognised.
pub const DEFAULT: Theme = ONEDARK_DARKER;

/// Look up by stored id, falling back to the default.
///
/// Deliberately total: a pref left behind by a renamed or removed theme
/// should quietly fall back, never fail to start the app.
pub fn by_id(id: &str) -> Theme {
    ALL.iter().find(|t| t.id == id).copied().unwrap_or(DEFAULT)
}

/// The `@define-color` block prepended to the stylesheet.
///
/// GTK resolves `@name` references at parse time, so re-parsing the sheet
/// with a different block is all a theme switch requires.
pub fn css_variables(t: &Theme) -> String {
    format!(
        "@define-color kalam_bg {bg};\n\
         @define-color kalam_sidebar {sidebar};\n\
         @define-color kalam_surface {surface};\n\
         @define-color kalam_surface_2 {surface_2};\n\
         @define-color kalam_border {border};\n\
         @define-color kalam_text {text};\n\
         @define-color kalam_text_dim {text_dim};\n\
         @define-color kalam_accent {accent};\n\
         @define-color kalam_accent_dim {accent_dim};\n\
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
        accent = t.accent,
        accent_dim = t.accent_dim,
        danger = t.danger,
        success = t.success,
        warning = t.warning,
        info = t.info,
    )
}

/// Pref key holding the chosen theme id.
pub const PREF_KEY: &str = "ui.theme";

/// Parse and install `theme` + the stylesheet as the application's CSS.
///
/// Safe to call repeatedly: `set_global_css` replaces the previous sheet, and
/// GTK re-resolves `@name` references on every parse, so switching themes is
/// just another call. Widgets restyle in place — nothing needs rebuilding.
pub fn apply(theme: &Theme) {
    let css = format!("{}{}", css_variables(theme), crate::style::APP_CSS);
    relm4::set_global_css(&css);
}

/// Read the saved theme, falling back to the default when unset.
pub fn current(catalog: &crate::db::Catalog) -> Theme {
    catalog
        .get_pref(PREF_KEY)
        .map(|id| by_id(&id))
        .unwrap_or(DEFAULT)
}

/// Persist and apply in one step.
pub fn save_and_apply(catalog: &crate::db::Catalog, theme: &Theme) {
    catalog.set_pref(PREF_KEY, theme.id);
    apply(theme);
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
    fn darker_variants_follow_their_standard() {
        // The picker relies on this ordering to render each family as a pair.
        for (i, t) in ALL.iter().enumerate() {
            if let Some(base) = t.id.strip_suffix("-darker") {
                let prev = ALL[i - 1].id;
                // Ayu's darker variant is upstream's "Ayu Dark", whose id does
                // not share the "ayumirage" stem.
                let paired = prev.starts_with(base) || (base == "ayu" && prev == "ayumirage");
                assert!(paired, "{} does not follow its standard ({prev})", t.id);
            }
        }
    }

    #[test]
    fn default_is_a_darker_variant() {
        assert_eq!(DEFAULT.id, ONEDARK_DARKER.id);
    }

    #[test]
    fn unknown_id_falls_back_to_default() {
        assert_eq!(by_id("no-such-theme").id, DEFAULT.id);
        assert_eq!(by_id("").id, DEFAULT.id);
    }

    #[test]
    fn every_theme_round_trips_by_id() {
        for t in ALL {
            assert_eq!(by_id(t.id).id, t.id);
        }
    }

    #[test]
    fn variables_cover_every_token_the_stylesheet_uses() {
        // If a token is added to style.rs but not emitted here, GTK logs a
        // parser error and the colour silently falls back to black.
        let css = css_variables(&DEFAULT);
        for token in [
            "kalam_bg",
            "kalam_sidebar",
            "kalam_surface",
            "kalam_surface_2",
            "kalam_border",
            "kalam_text",
            "kalam_text_dim",
            "kalam_accent",
            "kalam_accent_dim",
            "kalam_danger",
            "kalam_success",
            "kalam_warning",
            "kalam_info",
        ] {
            assert!(css.contains(token), "missing {token}");
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
                ("accent", t.accent),
                ("accent_dim", t.accent_dim),
                ("danger", t.danger),
                ("success", t.success),
                ("warning", t.warning),
                ("info", t.info),
            ] {
                assert!(!value.is_empty(), "{} has empty {name}", t.id);
            }
        }
    }
}
