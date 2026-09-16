//! Kalam — personal ebook manager & reader (Linux).
//!
//! Phase 5: metadata editing, cover replacement and Open Library lookup, on
//! top of the P1 catalog, P2 reader, P3 annotations and P4 library depth.

mod app;
mod author;
mod comics;
mod db;
mod dict;
mod downloads;
mod epub;
mod epub_book;
mod epub_write;
mod plugins;
mod epub_writer;
mod icons;
mod libraries;
mod metadata;
mod models;
mod notify;
mod pages;
mod paths;
mod perf;
mod preload;
mod service;
mod shelf_rules;
mod sidecar;
mod sources;
mod style;
mod tasks;
mod theme;
mod thumbs;
mod timing;
mod widgets;

use app::AppModel;
use relm4::RelmApp;

fn main() {
    // A0 step 1: GUI timing. KALAM_TIMING=1 prints cold-start / book-open /
    // chapter-turn / dict-lookup milliseconds to the terminal (no-op otherwise).
    timing::start();

    // RelmApp::new initializes GTK; only touch Adwaita/GTK after that.
    let app = RelmApp::new("app.kalam.Kalam");

    // Dark baseline via Adwaita (GtkSettings prefer-dark is unsupported with libadwaita).
    let style = adw::StyleManager::default();
    style.set_color_scheme(adw::ColorScheme::ForceDark);

    // P6.5: put the pre-existing library into the library list, if it is not
    // there already. Must run before anything calls `paths::data_dir()`, which
    // caches the active library for the life of the process.
    //
    // Without this the user's own books are the one library that never appears
    // in Settings — the app falls back to that folder, so everything works,
    // but Open and Forget would apply to every library except theirs, and
    // adding a second would make the first seem to disappear.
    libraries::adopt_legacy_library_if_needed();

    // If the selected library's folder is gone -- unplugged drive, unmounted
    // share, folder renamed -- say so before `ensure_data_dirs()` recreates it
    // and `Catalog::open` fills it with an empty database. Without this the
    // user sees an empty library and concludes their books are lost, when the
    // drive is merely not plugged in.
    //
    // Refusing to start is the right call here, not a fallback to some other
    // library: writing into a *different* library than the one the user chose
    // is how you scatter books across two places.
    if let Some(missing) = libraries::missing_active_library() {
        eprintln!("kalam: the selected library folder is not there.");
        eprintln!("  {}", missing.display());
        eprintln!();
        eprintln!("If it lives on a removable drive or a network share, connect");
        eprintln!("it and start Kalam again. Nothing has been changed or deleted.");
        eprintln!();
        eprintln!("To use a different library instead, edit or remove:");
        eprintln!("  {}", libraries::registry_path().display());
        std::process::exit(1);
    }

    // Ensure data dirs exist before the catalog is opened to read the theme.
    if let Err(err) = paths::ensure_data_dirs() {
        // Nothing will work if this failed, so say so on screen rather than
        // only on a terminal the user probably did not launch from. The
        // toast queues until the window exists.
        eprintln!("kalam: failed to create data directories: {err}");
        crate::notify::error("Could not create Kalam's data folders", &err.to_string());
    }

    // Open the catalog **once**, here, and hand the same handle to everything
    // that needs it.
    //
    // Failing here rather than inside `AppModel::init` is deliberate and
    // predates this change: the app cannot do anything without a catalog, and
    // `init()` runs inside a GTK signal callback where a panic cannot unwind
    // — it aborts the process with a core dump and a backtrace instead of
    // saying what is wrong. Checking first turns "Aborted (core dumped)" into
    // one readable line and exit code 1.
    //
    // What *did* change: this used to be one of three separate opens per
    // startup (this check, then `startup_theme`, then `AppModel::init`), each
    // running the whole of `migrate()`. Now the handle is shared.
    // Same span name as before, now measuring the single real open rather
    // than the third of three — so the A0 numbers stay comparable.
    timing::span("startup_db_open");
    let opened = db::Catalog::open();
    timing::span_end("startup_db_open");
    let catalog = match opened {
        Ok(catalog) => std::sync::Arc::new(catalog),
        Err(err) => {
            eprintln!("kalam: cannot open the library database.");
            eprintln!("  {err}");
            eprintln!("  file: {}", paths::catalog_db().display());
            eprintln!();
            eprintln!("If that file is corrupt, move it aside and restart:");
            let db = paths::catalog_db();
            eprintln!("  mv {} {}.broken", db.display(), db.display());
            eprintln!("Kalam will build a fresh library. Your book files are kept");
            eprintln!(
                "separately in {} and are not affected.",
                paths::library_dir().display()
            );
            std::process::exit(1);
        }
    };

    // Install the saved theme before the first window is drawn, so the app
    // never flashes the default palette on the way to the chosen one.
    //
    // KALAM_NO_CSS=1 skips the stylesheet entirely. Kept as a diagnostic: it is
    // how the pixman scrollbar bug was finally pinned on this file rather than
    // on GTK, after several wrong guesses.
    if std::env::var_os("KALAM_NO_CSS").is_none() {
        theme::apply(&theme::current(&catalog));
    } else {
        eprintln!("kalam: KALAM_NO_CSS set — running with stock GTK styling");
    }

    app.run::<AppModel>(catalog);
}

// Re-export adw for StyleManager (relm4 enables libadwaita).
use relm4::adw;
