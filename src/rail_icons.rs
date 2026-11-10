//! The rail's glyphs: bundled Lucide drawings, rasterized in theme colours.
//!
//! design-system §56 asks for 22px outline icons at stroke 1.8 and leaves the
//! set unchosen ("Lucide-style outline; set not chosen"). Lucide draws on a 24
//! grid at stroke 2, so at 22px the stroke is 1.83 — the set the spec was
//! already describing (build-decisions B6). The glyphs the rail needs ship
//! inside the binary under `assets/icons/`, with Lucide's ISC notice beside
//! them, so no icon theme has to provide them.
//!
//! They are rasterized to textures rather than installed as symbolic theme
//! icons for two reasons this codebase measured. `icons::init`'s theme rescan
//! costs 493–505 ms and is deferred to the first idle moment, while the rail
//! paints first; and GTK recolours a symbolic icon by its *fill*, which erases
//! a stroke-only drawing. A texture carries its colour with it, so the colour
//! goes into the SVG before rasterizing — the route the brand mark already
//! takes (see `brand_texture` in `app.rs`).
//!
//! The cost of that is repainting by hand: a texture cannot inherit a CSS
//! colour, so [`set_theme`] and [`repaint_all`] exist, and `init` registers a
//! [`crate::theme::on_theme_applied`] listener that calls both.
//!
//! Three faces, all from the design set. Idle is `text-2` (§56). The rail's
//! active entry is the *page* colour, so the glyph reads as cut out of the
//! accent-fill disc (§56, and the owner's ruling after the two options were
//! measured — see `docs/build-decisions.md`). The bell while its panel is open
//! is white, because §45 says so and no ruling supersedes it; the two states
//! look alike and deliberately are not.

use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// §56: icons 22. A 24-grid Lucide drawing at 22px is stroke 1.83.
pub const GLYPH_PX: i32 = 22;

/// Which colour a glyph is rasterized in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Idle: `text-2` (§56).
    Idle,
    /// The rail's active entry: the page colour, cut out of accent-fill
    /// (§56). Also the Home book card's play triangle on its accent disc
    /// (§60) — same cut-out, different page.
    CutOut,
    /// The bell while its panel is open: white (§45).
    OnAccent,
}

/// One glyph the rail has drawn, kept so a theme install can repaint it.
struct Drawn {
    /// The state lives on the button's CSS classes, which the rail already
    /// maintains; reading it back from there means this module cannot
    /// disagree with the shell about which entry is active. `None` for
    /// glyphs whose face is fixed instead (the play button is always cut
    /// out, whatever its classes say).
    button: Option<gtk::Button>,
    image: gtk::Image,
    /// A cell rather than a plain key so a glyph that changes while it is on
    /// screen still repaints the right drawing after a theme install. §65's
    /// maximize button was the one that did — it swapped between the expand
    /// and restore pair — and it left with the rest of the window controls
    /// (B18). `remember_key` keeps the general shape for the next one.
    key: Rc<Cell<&'static str>>,
    /// A face that overrides the button's CSS classes.
    forced: Option<Face>,
}

thread_local! {
    /// The theme in force, so repainting costs no database read:
    /// `theme::current` resolves a stored preference, and the rail repaints on
    /// every navigation as well as on every theme install. Seeded with the
    /// first-run theme (`theme::DEFAULT` is test-only); `init` writes the real
    /// one before the rail's entries are drawn.
    static THEME: RefCell<crate::theme::Theme> =
        const { RefCell::new(crate::theme::TERRACOTTA_DARK) };

    /// Rasterized glyphs by (key, colour). Thirteen keys and three colours, so
    /// this stays tiny; an entry for a theme no longer in force is simply
    /// never asked for again.
    static CACHE: RefCell<HashMap<(&'static str, String), gtk::gdk::Texture>> =
        RefCell::new(HashMap::new());

    /// Every glyph the rail has drawn. The rail lives as long as the process,
    /// as does the brand mark's theme listener, so nothing here needs
    /// releasing; a widget left behind by a second window in one process
    /// repaints harmlessly.
    static DRAWN: RefCell<Vec<Drawn>> = const { RefCell::new(Vec::new()) };
}

/// A bundled glyph's SVG, or `None` for a key this build does not carry.
///
/// Embedded with `include_str!` rather than read from disk: the binary is
/// self-contained and there is no install step that would place an asset
/// directory next to it. Every [`crate::models::NavItem`] has a key here,
/// including the four hidden from the rail, so putting one back stays the
/// one-line change `NavItem::ALL`'s doc promises.
fn svg(key: &'static str) -> Option<&'static str> {
    Some(match key {
        "house" => include_str!("../assets/icons/house.svg"),
        "library" => include_str!("../assets/icons/library.svg"),
        "layout-grid" => include_str!("../assets/icons/layout-grid.svg"),
        "user-round" => include_str!("../assets/icons/user-round.svg"),
        "images" => include_str!("../assets/icons/images.svg"),
        "globe" => include_str!("../assets/icons/globe.svg"),
        "square-pen" => include_str!("../assets/icons/square-pen.svg"),
        "bell" => include_str!("../assets/icons/bell.svg"),
        "settings" => include_str!("../assets/icons/settings.svg"),
        // Hidden entries (build-decisions B1, B2): not drawn today, bundled so
        // restoring one to `NavItem::ALL` needs no new asset.
        "archive" => include_str!("../assets/icons/archive.svg"),
        "download" => include_str!("../assets/icons/download.svg"),
        "compass" => include_str!("../assets/icons/compass.svg"),
        "feather" => include_str!("../assets/icons/feather.svg"),
        // The Home book card's play button (§60): drawn at 16px through
        // `image_px`, rasterized from the same 24 grid as everything else.
        // §65's four window-control glyphs left with its buttons (B18): on a
        // tiling compositor the window manager owns minimize, maximize and
        // close, so nothing here drew them. They are in the history if a
        // decorated window ever comes back.
        "play" => include_str!("../assets/icons/play.svg"),
        // §57's search bar and the result box under it: the magnifier in the
        // field, the picker's three-row sliders, the clock on a history row
        // and the cross that deletes one. Bundled now so the bar can be
        // drawn; the box's two glyphs wait with it.
        "search" => include_str!("../assets/icons/search.svg"),
        "sliders-horizontal" => {
            include_str!("../assets/icons/sliders-horizontal.svg")
        }
        "clock" => include_str!("../assets/icons/clock.svg"),
        "x" => include_str!("../assets/icons/x.svg"),
        _ => return None,
    })
}

/// Record the theme the glyphs are drawn in.
pub fn set_theme(theme: &crate::theme::Theme) {
    THEME.with(|slot| *slot.borrow_mut() = *theme);
}

/// The colour a face is rasterized in, from the theme in force.
fn colour(face: Face) -> String {
    THEME.with(|slot| {
        let theme = *slot.borrow();
        match face {
            Face::Idle => theme.text_dim.to_string(),
            Face::CutOut => theme.bg.to_string(),
            // §45 says white, not the page colour. Kept literal so a palette
            // whose accent-fill is light does not lose the bell's contrast.
            Face::OnAccent => "white".to_string(),
        }
    })
}

/// One glyph in one colour, rasterized once and cached.
///
/// `None` only when the key is not bundled or the SVG will not load; the rail
/// then draws an empty disc, which the tests below exist to prevent.
pub fn texture(key: &'static str, colour: &str) -> Option<gtk::gdk::Texture> {
    let cache_key = (key, colour.to_string());
    if let Some(hit) = CACHE.with(|cache| cache.borrow().get(&cache_key).cloned()) {
        return Some(hit);
    }
    let svg = svg(key)?.replace("currentColor", colour);
    let bytes = gtk::glib::Bytes::from_owned(svg.into_bytes());
    let texture = gtk::gdk::Texture::from_bytes(&bytes).ok()?;
    CACHE.with(|cache| {
        cache.borrow_mut().insert(cache_key, texture.clone());
    });
    Some(texture)
}

/// A rail glyph at §56's 22px, painted in `face`'s colour.
///
/// `gtk::Image` with `set_pixel_size`, not `gtk::Picture`: a Picture takes the
/// texture's own 24px as its natural size, and both `set_size_request` and CSS
/// `min-width` are only floors — that is how the brand mark once stretched the
/// whole rail (see `brand_logo` in `app.rs`).
pub fn image(key: &'static str, face: Face) -> gtk::Image {
    let image = image_px(key, face, GLYPH_PX);
    image.add_css_class("kalam-nav-icon");
    image
}

/// A glyph at any pixel size, without the rail's CSS class — the Home book
/// card's play button (§60) draws at 16px, not §56's 22px. Same texture cache: the raster is 24px and
/// `set_pixel_size` scales it, so a size is free.
pub fn image_px(key: &'static str, face: Face, px: i32) -> gtk::Image {
    let image = gtk::Image::new();
    image.set_pixel_size(px);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    paint(&image, key, face);
    image
}

/// Rasterize `key` in `face`'s colour into `image`.
///
/// Returns false when the glyph is not bundled, so a missing drawing is
/// something a test can catch rather than a silent hole in the rail.
pub fn paint(image: &gtk::Image, key: &'static str, face: Face) -> bool {
    match texture(key, &colour(face)) {
        Some(texture) => {
            image.set_paintable(Some(&texture));
            true
        }
        None => false,
    }
}

/// Remember a rail glyph so [`repaint_all`] can re-rasterize it later.
pub fn remember(button: &gtk::Button, image: &gtk::Image, key: &'static str) {
    remember_key(button, image, Rc::new(Cell::new(key)));
}

/// Like [`remember`], but the caller keeps the key cell — for glyphs that
/// swap their drawing while the app runs (the window's maximize button).
pub fn remember_key(button: &gtk::Button, image: &gtk::Image, key: Rc<Cell<&'static str>>) {
    DRAWN.with(|drawn| {
        drawn.borrow_mut().push(Drawn {
            button: Some(button.clone()),
            image: image.clone(),
            key,
            forced: None,
        })
    });
}

/// Remember a glyph whose face never follows button state — the Home book
/// card's play triangle (§60) is always cut out of its accent disc.
pub fn remember_fixed(image: &gtk::Image, key: &'static str, face: Face) {
    DRAWN.with(|drawn| {
        drawn.borrow_mut().push(Drawn {
            button: None,
            image: image.clone(),
            key: Rc::new(Cell::new(key)),
            forced: Some(face),
        })
    });
}

/// Re-rasterize every remembered glyph in the theme now in force, keeping each
/// one's state: an entry wearing `active` is cut out, the bell wearing `open`
/// is white, everything else is idle.
///
/// Called after the rail swaps its `active` classes and after a theme install,
/// because neither a CSS class nor a new stylesheet can recolour a texture.
pub fn repaint_all() {
    DRAWN.with(|drawn| {
        for entry in drawn.borrow().iter() {
            let face = match (entry.forced, entry.button.as_ref()) {
                (Some(face), _) => face,
                (None, Some(button)) => face_of(button),
                (None, None) => Face::Idle,
            };
            paint(&entry.image, entry.key.get(), face);
        }
    });
}

/// The face a button's CSS classes put its glyph in.
fn face_of(button: &gtk::Button) -> Face {
    if button.has_css_class("active") {
        Face::CutOut
    } else if button.has_css_class("open") {
        Face::OnAccent
    } else {
        Face::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NavItem;

    /// Every entry the rail can hold, plus the bell, has a bundled glyph. A
    /// missing one draws an empty disc, and the rail is the first thing on
    /// screen — so this is checked for all twelve `NavItem`s, not just the
    /// eight in `NavItem::ALL`, because restoring a hidden entry is meant to
    /// be a one-line change.
    #[test]
    fn every_nav_glyph_is_bundled() {
        let all = [
            NavItem::Home,
            NavItem::Library,
            NavItem::AllBooks,
            NavItem::Authors,
            NavItem::Shelves,
            NavItem::Downloads,
            NavItem::Comics,
            NavItem::Online,
            NavItem::Editor,
            NavItem::RemoteBrowse,
            NavItem::Fanfiction,
            NavItem::Settings,
        ];
        for item in all {
            assert!(
                svg(item.icon()).is_some(),
                "{item:?} names {:?}, which is not bundled",
                item.icon()
            );
        }
        assert!(svg("bell").is_some(), "the rail bell has no bundled glyph");
    }

    /// The 16px glyphs that are not the rail's: §60's play button today. The
    /// four window-control glyphs that shipped beside it left with the buttons
    /// (B18). A slice rather than an inline list so the next small drawing is
    /// added here instead of in a second test.
    const HOME_GLYPHS: &[&str] = &["play"];

    /// Those glyphs are bundled and stroked like the rail's, so the two tests
    /// above would catch an upstream redraw that filled them in.
    #[test]
    fn home_glyphs_are_bundled_outlines() {
        for &key in HOME_GLYPHS {
            let body = svg(key).unwrap_or_else(|| panic!("{key} is not bundled"));
            assert!(
                body.contains("stroke=\"currentColor\""),
                "{key} is not stroked in currentColor"
            );
            assert!(body.contains("fill=\"none\""), "{key} is not an outline");
        }
    }

    /// The glyphs are recoloured by substituting `currentColor` into a stroke.
    /// An upstream redraw that filled instead would rasterize as a solid blob
    /// in every theme, so the two properties this module depends on are
    /// pinned rather than assumed.
    #[test]
    fn bundled_glyphs_are_stroked_outlines_in_current_colour() {
        for item in NavItem::ALL {
            let key = item.icon();
            let body = svg(key).expect("bundled, per the test above");
            assert!(
                body.contains("stroke=\"currentColor\""),
                "{key} is not stroked in currentColor"
            );
            assert!(body.contains("fill=\"none\""), "{key} is not an outline");
            assert!(
                body.contains("stroke-width=\"2\""),
                "{key} is not drawn on Lucide's 24 grid at stroke 2, so 22px is not stroke 1.83"
            );
        }
    }

    /// §56 and §45 in one place: idle is `text-2`, active is the page colour
    /// (the cut-out the owner ruled on), the open bell is white.
    #[test]
    fn faces_take_their_colour_from_the_theme_in_force() {
        set_theme(&crate::theme::TERRACOTTA_DARK);
        assert_eq!(colour(Face::Idle), crate::theme::TERRACOTTA_DARK.text_dim);
        assert_eq!(colour(Face::CutOut), crate::theme::TERRACOTTA_DARK.bg);
        assert_eq!(colour(Face::OnAccent), "white");

        // A palette switch has to move the cut-out colour with the page, or
        // the active glyph stops matching the disc it sits in.
        set_theme(&crate::theme::TERRACOTTA_LIGHT);
        assert_eq!(colour(Face::CutOut), crate::theme::TERRACOTTA_LIGHT.bg);
        assert_ne!(
            colour(Face::CutOut),
            crate::theme::TERRACOTTA_DARK.bg,
            "light and dark share a page colour, so the cut-out cannot differ"
        );
        set_theme(&crate::theme::DEFAULT);
    }

    /// A key this build does not carry must fail rather than draw nothing and
    /// look like a rendering bug.
    #[test]
    fn an_unknown_glyph_key_is_refused() {
        assert!(svg("not-an-icon").is_none());
    }
}
