//! The Appearance pane's text and motion choices (decision 146): the
//! interface font, the heading font, the interface text size and reduce
//! motion. Colours live in `theme.rs`; this module owns the four prefs
//! beside them (`ui.font`, `ui.heading_font`, `ui.text_size`,
//! `ui.reduce_motion` — all global, so they follow the user between
//! libraries) and the two channels they act through:
//!
//! - **Fonts** ride the theme CSS channel. `theme::install_css`
//!   appends [`override_css`] after the app sheet, where a later rule of
//!   equal specificity wins: one `*` rule swaps the interface family,
//!   and the display-type selectors are redeclared when headings follow
//!   the interface font. The defaults emit nothing — the base sheet
//!   already says DM Sans and Playfair Display.
//! - **Text size** sets GTK's `gtk-xft-dpi` relative to the value the
//!   system had at startup, which scales every Pango font size in the
//!   interface (the design's type table included) and never the book:
//!   the reading engine renders its own text outside Pango/GTK.
//! - **Reduce motion** sets `gtk-enable-animations`; new transitions
//!   that need a gentler shape than "instant" should ask
//!   [`reduce_motion`] and pick a fade instead of a slide (the bell
//!   panel does). Existing revealers were not retrofitted by the
//!   Settings phase — recorded in the audit (§5.8).
//!
//! The current choices are cached in a thread-local because
//! `override_css` runs from `install_css`, which has a theme in hand
//! but no catalog. [`init`] loads the cache before the first theme
//! install; the setters keep it in sync.
//!
//! The setters and option enums are called by the Settings page,
//! which lands in the next tranche of this phase; the allow below
//! comes off with it.
#![allow(dead_code)]

use crate::db::Catalog;
use std::cell::{Cell, RefCell};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceFont {
    DmSans,
    System,
}

impl InterfaceFont {
    pub fn id(self) -> &'static str {
        match self {
            Self::DmSans => "dm-sans",
            Self::System => "system",
        }
    }

    fn from_id(id: &str) -> Self {
        match id {
            "system" => Self::System,
            _ => Self::DmSans,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingFont {
    Playfair,
    Interface,
}

impl HeadingFont {
    pub fn id(self) -> &'static str {
        match self {
            Self::Playfair => "playfair",
            Self::Interface => "interface",
        }
    }

    fn from_id(id: &str) -> Self {
        match id {
            "interface" => Self::Interface,
            _ => Self::Playfair,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSize {
    Small,
    Default,
    Large,
}

impl TextSize {
    pub fn id(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Default => "default",
            Self::Large => "large",
        }
    }

    fn from_id(id: &str) -> Self {
        match id {
            "small" => Self::Small,
            "large" => Self::Large,
            _ => Self::Default,
        }
    }

    /// About 90, 100 and 115% (decision 146).
    fn factor(self) -> f64 {
        match self {
            Self::Small => 0.9,
            Self::Default => 1.0,
            Self::Large => 1.15,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReduceMotion {
    System,
    On,
    Off,
}

impl ReduceMotion {
    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::On => "on",
            Self::Off => "off",
        }
    }

    fn from_id(id: &str) -> Self {
        match id {
            "on" => Self::On,
            "off" => Self::Off,
            _ => Self::System,
        }
    }
}

/// The four choices together, as the Appearance pane shows them.
#[derive(Debug, Clone, Copy)]
pub struct Appearance {
    pub font: InterfaceFont,
    pub heading_font: HeadingFont,
    pub text_size: TextSize,
    pub reduce_motion: ReduceMotion,
}

const PREF_FONT: &str = "ui.font";
const PREF_HEADING_FONT: &str = "ui.heading_font";
const PREF_TEXT_SIZE: &str = "ui.text_size";
const PREF_REDUCE_MOTION: &str = "ui.reduce_motion";

/// The stored choices. Unknown or missing values are the defaults —
/// deliberately total, like the theme choice parser.
pub fn choices(catalog: &Catalog) -> Appearance {
    Appearance {
        font: InterfaceFont::from_id(&catalog.get_pref(PREF_FONT).unwrap_or_default()),
        heading_font: HeadingFont::from_id(
            &catalog.get_pref(PREF_HEADING_FONT).unwrap_or_default(),
        ),
        text_size: TextSize::from_id(&catalog.get_pref(PREF_TEXT_SIZE).unwrap_or_default()),
        reduce_motion: ReduceMotion::from_id(
            &catalog.get_pref(PREF_REDUCE_MOTION).unwrap_or_default(),
        ),
    }
}

#[derive(Debug, Clone)]
struct State {
    font: InterfaceFont,
    heading_font: HeadingFont,
    motion: ReduceMotion,
    /// The desktop's font family ("Cantarell"), captured at startup from
    /// `gtk-font-name`; empty until captured or if GTK had no answer.
    system_family: String,
    /// Raw `gtk-xft-dpi` at startup (may be -1, GTK's "default").
    baseline_dpi: i32,
    /// `gtk-enable-animations` as found at startup, restored when the
    /// reduce-motion choice returns to System.
    system_animations: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            font: InterfaceFont::DmSans,
            heading_font: HeadingFont::Playfair,
            motion: ReduceMotion::System,
            system_family: String::new(),
            baseline_dpi: -1,
            system_animations: true,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static INITIALISED: Cell<bool> = const { Cell::new(false) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// Load the choices and apply the two GTK-settings channels. Must run
/// after GTK initialisation and before the first theme install, so the
/// first painted frame already has the right fonts and size.
pub fn init(catalog: &Catalog) {
    let prefs = choices(catalog);
    with_state(|state| {
        state.font = prefs.font;
        state.heading_font = prefs.heading_font;
        state.motion = prefs.reduce_motion;
        if let Some(settings) = gtk::Settings::default() {
            state.system_family = settings
                .gtk_font_name()
                .map(|name| family_of(&name))
                .unwrap_or_default();
            state.baseline_dpi = settings.gtk_xft_dpi();
            state.system_animations = settings.is_gtk_enable_animations();
        }
    });
    INITIALISED.with(|i| i.set(true));
    apply_text_size(prefs.text_size);
    apply_motion(prefs.reduce_motion);
}

/// The family out of a `gtk-font-name` value ("Cantarell 11" → "Cantarell").
fn family_of(font_name: &str) -> String {
    let name = font_name.trim();
    match name.rsplit_once(' ') {
        Some((family, size)) if size.chars().all(|c| c.is_ascii_digit()) => {
            family.trim().to_string()
        }
        _ => name.to_string(),
    }
}

/// The CSS `theme::install_css` appends after the app sheet: nothing at
/// the defaults, the font swaps otherwise.
pub fn override_css() -> String {
    if !INITIALISED.with(|i| i.get()) {
        return String::new();
    }
    with_state(|state| {
        let mut css = String::new();
        if state.font == InterfaceFont::System && !state.system_family.is_empty() {
            css.push_str(&format!(
                "\n* {{ font-family: {}; }}\n",
                interface_stack(state)
            ));
        }
        if state.heading_font == HeadingFont::Interface {
            css.push_str(&format!(
                "\n{} {{ font-family: {}; }}\n",
                DISPLAY_SELECTORS.join(", "),
                interface_stack(state)
            ));
        }
        css
    })
}

/// The interface family's CSS stack for the current font choice. The
/// glyph-fallback faces from the base sheet stay in the stack so the
/// ✓/→/· fallbacks survive a family swap.
fn interface_stack(state: &State) -> String {
    const FALLBACKS: &str = "\"Cantarell\", \"Noto Sans\", \
        \"JetBrains Mono Nerd Font Propo\", \"JetBrainsMono Nerd Font Propo\", \
        \"Cascadia Code Nerd Font Propo\", \"FiraCode Nerd Font Propo\", \
        \"Hack Nerd Font Propo\", \"Source Code Pro Nerd Font Propo\", sans-serif";
    match state.font {
        InterfaceFont::DmSans => format!("\"DM Sans\", {FALLBACKS}"),
        InterfaceFont::System => {
            if state.system_family.is_empty() {
                FALLBACKS.to_string()
            } else {
                format!("\"{}\", \"DM Sans\", {FALLBACKS}", state.system_family)
            }
        }
    }
}

/// Every selector in `resources/style.css` whose rule sets Playfair
/// Display — the heading-font switch redeclares exactly these. Keep in
/// sync when a display class is added there; a missed selector just
/// stays in Playfair, so the failure is visible and small, by design.
const DISPLAY_SELECTORS: &[&str] = &[
    ".kalam-title-serif",
    ".kalam-title-serif-italic",
    ".kalam-bookpage-title",
    ".kalam-desc-serif",
    "button.kalam-bookpage-series",
    ".kalam-bookpage-annotation-quote",
    ".kalam-seriespage-title",
    ".kalam-seriespage-strip-title",
    ".kalam-seriespage-focused-title",
    ".kalam-hl-text",
    ".k-edit-before",
    ".k-edit-after",
    ".kalam-nr-title",
    ".kalam-q-text",
    ".kalam-vocab-word",
    ".k-word-preview-head",
    ".k-word",
    ".k-def-example",
    ".k-idiom-phrase",
    ".kalam-ui-display",
    ".kalam-ui-section-title",
    ".kalam-ui-card-title",
    ".kalam-ui-quote",
    ".kalam-ui-state-heading",
    ".kalam-ui-cover-title",
    ".kalam-ui-cover-missing-title",
    ".kalam-ui-dialog-title",
    ".kalam-authorpage-title",
    ".kalam-authorpage-initials-text",
    ".kalam-authorpage-panel-title",
];

pub fn set_interface_font(catalog: &Catalog, font: InterfaceFont) {
    catalog.set_pref(PREF_FONT, font.id());
    with_state(|state| state.font = font);
    reinstall(catalog);
}

pub fn set_heading_font(catalog: &Catalog, font: HeadingFont) {
    catalog.set_pref(PREF_HEADING_FONT, font.id());
    with_state(|state| state.heading_font = font);
    reinstall(catalog);
}

pub fn set_text_size(catalog: &Catalog, size: TextSize) {
    catalog.set_pref(PREF_TEXT_SIZE, size.id());
    apply_text_size(size);
}

pub fn set_reduce_motion(catalog: &Catalog, motion: ReduceMotion) {
    catalog.set_pref(PREF_REDUCE_MOTION, motion.id());
    with_state(|state| state.motion = motion);
    apply_motion(motion);
}

/// Re-run the theme install so the sheet is rebuilt with the new font
/// override. The current stored choice is re-applied as-is.
fn reinstall(catalog: &Catalog) {
    let choice = crate::theme::current_choice(catalog);
    crate::theme::apply_choice(&choice);
}

fn apply_text_size(size: TextSize) {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };
    let baseline = with_state(|state| state.baseline_dpi);
    if size.factor() == 1.0 {
        // Restore exactly what the system had, including GTK's -1
        // "default", so Default means untouched.
        settings.set_gtk_xft_dpi(baseline);
        return;
    }
    let effective = if baseline > 0 { baseline } else { 96 * 1024 };
    settings.set_gtk_xft_dpi((effective as f64 * size.factor()) as i32);
}

fn apply_motion(motion: ReduceMotion) {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };
    match motion {
        ReduceMotion::On => settings.set_gtk_enable_animations(false),
        ReduceMotion::Off => settings.set_gtk_enable_animations(true),
        ReduceMotion::System => {
            settings.set_gtk_enable_animations(with_state(|s| s.system_animations))
        }
    }
}

/// Whether transitions should be gentled right now — consulted by new
/// animated surfaces when choosing between a slide and a fade.
pub fn reduce_motion() -> bool {
    match with_state(|state| state.motion) {
        ReduceMotion::On => true,
        ReduceMotion::Off => false,
        ReduceMotion::System => gtk::Settings::default()
            .map(|settings| !settings.is_gtk_enable_animations())
            .unwrap_or(false),
    }
}
