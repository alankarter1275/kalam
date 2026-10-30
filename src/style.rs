//! The shape of the interface: spacing, radii, type scale, borders.
//!
//! Colours are **not** defined here. `theme.rs` prepends an `@define-color`
//! block at runtime, so every rule below must refer to colours by `@kalam_*`
//! name. A literal hex here is a bug unless it is deliberately theme-independent
//! (the highlight marker palette, the reader's paper swatches, and the reader
//! stage) — those are commented where they appear.
//!
//! # GTK styling rules learned the hard way
//!
//! These cost roughly a dozen rounds of debugging between them. Please read
//! before editing, especially the scrollbar block.
//!
//! ### 1. This sheet already outranks the system theme
//! `relm4::set_global_css` installs at `STYLE_PROVIDER_PRIORITY_APPLICATION`
//! (600); Adwaita loads at `THEME` (200). **Priority beats specificity**, so a
//! plain `scrollbar slider` already wins against Adwaita's
//! `scrollbar.overlay-indicator:not(.dragging):not(.hovering) slider`. Do not
//! write long `:not()` chains to "win" — they are unnecessary, and GTK drops an
//! *entire* comma-separated rule if any one selector in the group fails to
//! parse, silently taking working declarations down with it.
//!
//! ### 2. Never set `opacity` below 1 on a widget that can collapse
//! Any opacity < 1 makes GTK render the widget through an offscreen surface. A
//! collapsed overlay scrollbar's surface is zero-sized, and pixman rejects it:
//! ```text
//! *** BUG *** In pixman_region32_init_rect: Invalid rectangle passed
//! ```
//! One error per affected widget, so the count varies per run and looks random.
//! To hide something, make its `background-color` transparent instead.
//!
//! ### 3. `margin`, `border` and `padding` are subtracted from the allocation
//! GTK computes the painted box as *size − border − margin − padding*. If the
//! result is negative you get:
//! ```text
//! GtkGizmo (slider) reported min width -12, but sizes must be >= 0
//! ```
//! Adwaita's slider ships `margin: 4px` **and** `border: 4px solid transparent`
//! — 16px in total. Resetting only the border leaves 8px, which is why every
//! `min-width` set for several rounds still came out negative. **Zero all
//! three**, then set the size.
//!
//! Corollary: to inset something, prefer sizing the parent node over adding a
//! margin to the child. A margin on a child inside a narrow allocation is the
//! single most reliable way to reproduce both errors above.
//!
//! ### 4. `scrolledwindow:hover` is not "hovering the scrollbar"
//! A `scrolledwindow` is the whole content area, so that selector matches
//! whenever the pointer is anywhere in the page. For "pointer is on the bar",
//! use `scrollbar:hover` or GTK's own `scrollbar.hovering` class.
//!
//! ### 5. Diagnostics
//! `KALAM_NO_CSS=1 cargo run` starts the app with no custom stylesheet. If a
//! rendering warning still appears, the cause is not in this file. Use it
//! *before* theorising. `GTK_DEBUG=interactive` shows which rule actually wins
//! on a given node.

pub const APP_CSS: &str = include_str!("../resources/style.css");

/// The approved spacing scale, for widget construction code that needs the
/// same numbers the component CSS uses (`docs/design/design-system.md` §4).
pub mod spacing {
    pub const XXS: i32 = 4;
    pub const XS: i32 = 8;
    pub const SM: i32 = 12;
    pub const MD: i32 = 16;
    pub const LG: i32 = 24;
    pub const XL: i32 = 32;
    pub const XXL: i32 = 48;
}

/// Fixed component sizes that have to be requested from Rust rather than
/// left to CSS min-size rules.
pub mod size {
    pub const TOUCH_TARGET: i32 = 44;
    pub const CONTROL: i32 = 40;
    pub const COMPACT: i32 = 32;
    pub const SIDE_PANEL_WIDTH: i32 = 340;
}
