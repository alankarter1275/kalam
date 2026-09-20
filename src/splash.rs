//! A brief branded splash, so startup is never a blank screen.
//!
//! GTK cannot draw a window until the main loop spins, and the work that
//! follows `main()`'s GTK init — opening the catalogue, building the first
//! page — happens *before* `app.run()` enters the loop. A window merely
//! `present()`ed then would sit invisible until all of that finished. So the
//! splash shows itself and then *pumps* the loop by hand for a few frames,
//! guaranteeing it is on screen before the heavy work starts. It is closed
//! from the main window's first `realize`.
//!
//! The splash carries its own CSS provider rather than the saved theme: the
//! theme lives in the catalogue, which is not open yet when the splash shows.

use gtk::prelude::*;
use std::cell::RefCell;

thread_local! {
    static SPLASH: RefCell<Option<gtk::Window>> = RefCell::new(None);
}

const CSS: &str = "
window.kalam-splash { background: #0c110f; }
.kalam-splash-name {
  color: #e8e6e0;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: 5px;
}
.kalam-splash-sub { color: #78857e; font-size: 11px; }
.kalam-splash spinner { color: #35c08e; }
";

/// Show the splash. Call once, right after GTK is initialised.
pub fn show() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }

    let window = gtk::Window::new();
    window.add_css_class("kalam-splash");
    window.set_decorated(false);
    window.set_resizable(false);
    window.maximize();

    let col = gtk::Box::new(gtk::Orientation::Vertical, 14);
    col.set_valign(gtk::Align::Center);
    col.set_halign(gtk::Align::Center);

    let spinner = gtk::Spinner::new();
    spinner.set_size_request(44, 44);
    spinner.start();

    let name = gtk::Label::new(Some("KALAM"));
    name.add_css_class("kalam-splash-name");

    let sub = gtk::Label::new(Some("Opening your library…"));
    sub.add_css_class("kalam-splash-sub");

    col.append(&spinner);
    col.append(&name);
    col.append(&sub);
    window.set_child(Some(&col));
    window.present();

    SPLASH.with(|s| *s.borrow_mut() = Some(window));
}

/// Spin the main loop until the splash has actually been painted.
///
/// Bounded: a stuck compositor must not stall startup for longer than the
/// splash was meant to save.
pub fn pump() {
    let ctx = gtk::glib::MainContext::default();
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_millis(120) {
        // Drain whatever is ready — paint dispatch included — without
        // blocking, then yield so the compositor can draw the frame.
        while ctx.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
}

/// Close the splash once the main window is up. Safe to call when no splash
/// was ever shown.
pub fn close() {
    SPLASH.with(|s| {
        if let Some(window) = s.borrow_mut().take() {
            window.destroy();
        }
    });
}
