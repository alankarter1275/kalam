use gtk::prelude::*;
use std::sync::OnceLock;

/// One symbolic icon from the current icon theme.
pub fn symbolic(name: &str, pixel_size: i32) -> gtk::Image {
    let image = gtk::Image::builder().icon_name(name).build();
    image.set_pixel_size(pixel_size);
    image
}

/// One symbolic icon with CSS classes attached.
pub fn symbolic_with_classes(name: &str, pixel_size: i32, classes: &[&str]) -> gtk::Image {
    let image = symbolic(name, pixel_size);
    for class in classes {
        image.add_css_class(class);
    }
    image
}

/// A small horizontal row: icon then plain label.
pub fn labelled(icon_name: &str, pixel_size: i32, label: &str, spacing: i32) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
    row.append(&symbolic_with_classes(
        icon_name,
        pixel_size,
        &["kalam-inline-icon"],
    ));
    row.append(&gtk::Label::new(Some(label)));
    row
}

/// Whether [`init`] has already done its work.
///
/// `OnceLock` rather than a `bool` because two callers reach this — the idle
/// callback in `main()` and the reader's toolbar — and both must be safe
/// whichever runs first.
static INSTALLED: OnceLock<()> = OnceLock::new();

/// Register custom Kalam symbolic icons with GTK's icon theme so they are
/// available across all desktop environments without missing glyph fallbacks.
///
/// **Idempotent, and deliberately no longer on the startup path.** The
/// icon-theme rescan this triggers measured **493–505 ms warm — 36% of a
/// 1.36 s start** — while writing the SVGs themselves measured **0.1 ms**, so
/// the cost is GTK rescanning every icon theme on the system, not these files.
/// Nothing draws one of these icons until the reader is opened, so `main()`
/// schedules this for the first idle moment after the window is up (roadmap
/// 1.13). The toolbar calls it as well, so if that has somehow not run yet the
/// cost lands exactly where it used to rather than the icons going missing.
pub fn init() {
    INSTALLED.get_or_init(install);
}

fn install() {
    crate::timing::span("icons_write");
    let icons_base = crate::paths::legacy_data_dir().join("icons");
    let actions_dir = icons_base.join("hicolor/scalable/actions");
    if let Err(e) = std::fs::create_dir_all(&actions_dir) {
        eprintln!("Failed to create icons directory: {e}");
        return;
    }

    const ICONS: &[(&str, &str)] = &[
        (
            "kalam-highlight-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 19h4l10.5 -10.5a2.828 2.828 0 1 0 -4 -4l-10.5 10.5v4" /><path d="M12.5 5.5l4 4" /><path d="M4.5 13.5l4 4" /><path d="M21 15v4h-8l4 -4z" /></svg>"#,
        ),
        (
            "kalam-quote-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M4.583 17.321c1.69 -1.524 2.877 -3.154 3.325 -4.821h-3.908c-.552 0 -1 -.448 -1 -1v-6c0 -.552 .448 -1 1 -1h6c.552 0 1 .448 1 1v6.764c0 3.208 -1.896 6.046 -4.823 7.557c-.49 .253 -1.092 .053 -1.345 -.437s-.053 -1.092 .437 -1.345l.309 -.159zm11 0c1.69 -1.524 2.877 -3.154 3.325 -4.821h-3.908c-.552 0 -1 -.448 -1 -1v-6c0 -.552 .448 -1 1 -1h6c.552 0 1 .448 1 1v6.764c0 3.208 -1.896 6.046 -4.823 7.557c-.49 .253 -1.092 .053 -1.345 -.437s-.053 -1.092 .437 -1.345l.309 -.159z" /></svg>"#,
        ),
        (
            "kalam-dictionary-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 4v16h-12a2 2 0 0 1 -2 -2v-12a2 2 0 0 1 2 -2h12z" /><path d="M19 16h-12a2 2 0 0 0 -2 2" /><path d="M9 8h6" /></svg>"#,
        ),
        (
            "kalam-copy-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 7m0 2.667a2.667 2.667 0 0 1 2.667 -2.667h8.666a2.667 2.667 0 0 1 2.667 2.667v8.666a2.667 2.667 0 0 1 -2.667 2.667h-8.666a2.667 2.667 0 0 1 -2.667 -2.667z" /><path d="M4.012 16.737a2.005 2.005 0 0 1 -1.012 -1.737v-10c0 -1.1 .9 -2 2 -2h10c.75 0 1.158 .385 1.5 1" /></svg>"#,
        ),
        (
            "kalam-pdf-page-scroll-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="3" width="14" height="18" rx="2" /><path d="M12 7v10" /><path d="M9.5 9.5L12 7l2.5 2.5" /><path d="M9.5 14.5L12 17l2.5-2.5" /></svg>"#,
        ),
        (
            "kalam-pdf-vertical-scroll-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6c4-2 12-2 16 0" /><path d="M4 18c4 2 12 2 16 0" /><line x1="4" y1="11" x2="20" y2="11" /><line x1="4" y1="13" x2="20" y2="13" /></svg>"#,
        ),
        (
            "kalam-pdf-horizontal-scroll-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4c-2 4-2 12 0 16" /><path d="M18 4c2 4 2 12 0 16" /><line x1="11" y1="4" x2="11" y2="20" /><line x1="13" y1="4" x2="13" y2="20" /></svg>"#,
        ),
        (
            "kalam-pdf-wrapped-scroll-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="6" height="6" rx="1" /><rect x="14" y="4" width="6" height="6" rx="1" /><rect x="4" y="14" width="6" height="6" rx="1" /><rect x="14" y="14" width="6" height="6" rx="1" /></svg>"#,
        ),
        (
            "kalam-pdf-no-spreads-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="3" width="12" height="18" rx="2" /></svg>"#,
        ),
        (
            "kalam-pdf-odd-spreads-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="9" height="16" rx="1.5" /><rect x="13" y="4" width="9" height="16" rx="1.5" /><path d="M6 9v6" /><path d="M16 10a1.5 1.5 0 0 1 1.5-1.5h.5a1.5 1.5 0 0 1 1.5 1.5c0 1.5-3 2-3 3.5h3" /></svg>"#,
        ),
        (
            "kalam-pdf-even-spreads-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="9" height="16" rx="1.5" /><rect x="13" y="4" width="9" height="16" rx="1.5" /><path d="M5 10a1.5 1.5 0 0 1 1.5-1.5h.5a1.5 1.5 0 0 1 1.5 1.5c0 1.5-3 2-3 3.5h3" /><path d="M16 9.5a1.5 1.5 0 0 1 2.5 1c0 1-1.5 1.5-1.5 1.5s1.5.5 1.5 1.5a1.5 1.5 0 0 1-2.5 1" /></svg>"#,
        ),
        // The PDF panels' pin (decision 57): the board's upright
        // outline thumbtack. It is one even-odd filled silhouette,
        // not a pair of stroked paths: symbolic-icon recolouring is
        // fill based, and the separate stroked stem vanished at
        // panel-header size.
        (
            "kalam-pin-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" d="M7.4 2.8h9.2l-1.4 5.4L19 11.4h-5.6v9.4h-2.8v-9.4H5l3.8-3.2zM9.4 4.8h5.2l-.7 3.1 1.2 1.7H8.9l1.2-1.7z" /></svg>"#,
        ),
        // The PDF Pages grid's density toggle (decision 59): the
        // board's two and three upright page columns, kept as
        // simple stroked rectangles so the count remains legible
        // at the 18px panel-header size.
        (
            "kalam-columns-2-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><rect x="4" y="5" width="6" height="14" rx="1" /><rect x="14" y="5" width="6" height="14" rx="1" /></svg>"#,
        ),
        (
            "kalam-columns-3-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><rect x="3" y="5" width="4" height="14" rx="1" /><rect x="10" y="5" width="4" height="14" rx="1" /><rect x="17" y="5" width="4" height="14" rx="1" /></svg>"#,
        ),
        // The PDF annotation toolbar (decision 66): cursor, pen and
        // eraser as filled silhouettes, like the pin — symbolic
        // recolouring is fill based, so each reads at toolbar size
        // without stroked detail to lose.
        (
            "kalam-draw-select-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M5.2 2.8 19.4 12l-7.8 1.4 3 7-2.4 1-3-6.9-4.6 4.2z" /></svg>"#,
        ),
        (
            "kalam-draw-pen-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" d="M2.8 21.2 4 15.8 15.9 3.9a2.25 2.25 0 0 1 3.2 0l1.2 1.2a2.25 2.25 0 0 1 0 3.2L8.4 20.2zM6 17.1l9.2-9.2 1.1 1.1L7.1 18.2z" /></svg>"#,
        ),
        (
            "kalam-draw-eraser-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" d="M3.5 15.2 13.2 5.5a1.8 1.8 0 0 1 2.5 0l4 4a1.8 1.8 0 0 1 0 2.5l-6.2 6.3H7.4l-3.9-3.1zm6.2.8h3l5-5-2-2z" /></svg>"#,
        ),
        // The reader's bookmark ribbon (decision 16): a V-notched strip
        // hanging from the top edge. Idle is an outline in the border
        // colour over the page; set is the same shape filled with the
        // accent. Two icons, recoloured by CSS `color` like the rest.
        (
            "kalam-ribbon-outline-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"><path d="M5 2.5h14V21l-7-4.6L5 21z" /></svg>"#,
        ),
        (
            "kalam-ribbon-filled-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M5 2.5h14V21l-7-4.6L5 21z" /></svg>"#,
        ),
        // The PDF open states (decision 73): one quiet line glyph
        // over each dashed card — the lock that asks, the damaged
        // file that cannot be read, and the file Kalam cannot find.
        // Like the pin above, each is an even-odd filled silhouette
        // rather than stroked paths: symbolic recolouring fills the
        // path geometry, so a stroked lock or file lands as a solid
        // blob with its keyhole and marks filled in.
        (
            "kalam-lock-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" d="M8 10.5V8a4 4 0 0 1 8 0v2.5h-1.6V8a2.4 2.4 0 0 0-4.8 0v2.5H8zM7 10.5h10a2 2 0 0 1 2 2v5.5a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-5.5a2 2 0 0 1 2-2zM7.2 12.1h9.6a.6.6 0 0 1 .6.6v5.1a.6.6 0 0 1-.6.6H7.2a.6.6 0 0 1-.6-.6v-5.1a.6.6 0 0 1 .6-.6zM12 14.1a1.15 1.15 0 1 1 0 2.3 1.15 1.15 0 0 1 0-2.3z" /></svg>"#,
        ),
        (
            "kalam-file-warning-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" d="M7 3h7l4 4v14H7zM8.6 4.6h4.6l3.2 3.2v11.6H8.6zM11.2 9.5h1.6V14h-1.6zM12 16.05a.95.95 0 1 1 0 1.9.95.95 0 0 1 0-1.9z" /></svg>"#,
        ),
        (
            "kalam-file-question-symbolic.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" d="M7 3h7l4 4v14H7zM8.6 4.6h4.6l3.2 3.2v11.6H8.6zM10.05 11.6c-.9-.35-1.45-1.15-1.45-2.1 0-1.75 1.5-3.2 3.4-3.2s3.4 1.35 3.4 3.05c0 1.2-.65 2.1-1.65 2.7-.75.45-1.05.8-1.05 1.55v.55h-1.6V13.3c0-1.2.6-2.05 1.55-2.6.6-.35.95-.75.95-1.4 0-.9-.75-1.6-1.65-1.6s-1.65.7-1.65 1.6c0 .45.2.85.55 1.1zM12 16.05a.95.95 0 1 1 0 1.9.95.95 0 0 1 0-1.9z" /></svg>"#,
        ),
    ];

    for (name, content) in ICONS {
        let dest = actions_dir.join(name);
        // Bundled glyphs are part of the app build. Refresh stale
        // copies from earlier builds, or a corrected icon would stay
        // shadowed by the obsolete file in the user's data directory.
        if std::fs::read_to_string(&dest).ok().as_deref() != Some(*content) {
            let _ = std::fs::write(&dest, content);
        }
    }
    crate::timing::span_end("icons_write");

    // `add_search_path` makes GTK rescan the icon theme, which is the likely
    // cost — but this span exists to find that out rather than assume it.
    crate::timing::span("icons_theme");
    if let Some(display) = gtk::gdk::Display::default() {
        let theme = gtk::IconTheme::for_display(&display);
        theme.add_search_path(&icons_base);
    }
    crate::timing::span_end("icons_theme");
}

