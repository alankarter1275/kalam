use gtk::prelude::*;

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

/// Register custom Kalam symbolic icons with GTK's icon theme so they are
/// guaranteed to be available across all desktop environments without missing
/// glyph fallbacks.
pub fn init() {
    // Roadmap 1.12: the two halves of this function cost very different
    // amounts and the combined `startup_icons` span could not say which was
    // which — it measured 859 ms for four tiny SVG files.
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
    ];

    for (name, content) in ICONS {
        let dest = actions_dir.join(name);
        if !dest.exists() {
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

