//! Bundled user-interface fonts: Playfair Display + DM Sans.
//!
//! The design system (`docs/design/design-system.md` §3) sets the UI in
//! DM Sans with Playfair Display for display text, and requires the fonts
//! to be bundled, not loaded from the network. The reading engine bundles
//! its own faces into its font database (`crates/kalam-reader/src/fonts.rs`);
//! GTK text goes through Pango/fontconfig instead, so these faces are
//! registered as *application fonts*: visible to this process only, never
//! installed into the user's system font folders.
//!
//! The files in `assets/fonts/` are the unmodified upstream variable
//! fonts from google/fonts (Playfair Display carries a Reserved Font
//! Name, so they must stay unmodified to keep the name), each with its
//! OFL license text beside it. Pango/fontconfig serve the named weight
//! instances (400/500/600/700) from the variable files.
//!
//! Registration must happen before GTK creates its first Pango font map,
//! i.e. before `RelmApp::new` in `main()`: fontconfig application fonts
//! added later are not picked up by an already-created font map. The
//! embedded bytes are written to a cache directory because
//! `FcConfigAppFontAddDir` registers a directory, not memory.

use std::path::Path;

/// (file name, embedded bytes) for each bundled face.
const FONT_FILES: &[(&str, &[u8])] = &[
    (
        "PlayfairDisplay[wght].ttf",
        include_bytes!("../assets/fonts/PlayfairDisplay[wght].ttf"),
    ),
    (
        "PlayfairDisplay-Italic[wght].ttf",
        include_bytes!("../assets/fonts/PlayfairDisplay-Italic[wght].ttf"),
    ),
    (
        "DMSans[opsz,wght].ttf",
        include_bytes!("../assets/fonts/DMSans[opsz,wght].ttf"),
    ),
    (
        "DMSans-Italic[opsz,wght].ttf",
        include_bytes!("../assets/fonts/DMSans-Italic[opsz,wght].ttf"),
    ),
];

/// Write the bundled fonts to the cache and register them with
/// fontconfig for this process. Failures are reported on stderr and the
/// UI falls back to the system fonts named later in the CSS stacks —
/// a missing display face must never stop the app from starting.
pub fn init_ui_fonts() {
    let dir = gtk::glib::user_cache_dir().join("kalam").join("fonts");
    if let Err(err) = write_fonts(&dir) {
        eprintln!("kalam: could not write bundled fonts to {}: {err}", dir.display());
        return;
    }
    if !register_font_dir(&dir) {
        eprintln!(
            "kalam: fontconfig refused the bundled font directory {}",
            dir.display()
        );
    }
}

fn write_fonts(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, bytes) in FONT_FILES {
        let path = dir.join(name);
        let stale = std::fs::metadata(&path)
            .map(|m| m.len() != bytes.len() as u64)
            .unwrap_or(true);
        if stale {
            std::fs::write(&path, bytes)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn register_font_dir(dir: &Path) -> bool {
    use std::ffi::{c_void, CString};
    use std::os::unix::ffi::OsStrExt;

    // The two fontconfig entry points this needs. libfontconfig is
    // already a transitive link dependency of Pango; CI and the
    // devcontainer install its headers explicitly (libfontconfig1-dev).
    // `FcConfig` is opaque, hence `c_void` for the config pointer.
    #[allow(non_snake_case)]
    #[link(name = "fontconfig")]
    extern "C" {
        fn FcConfigGetCurrent() -> *mut c_void;
        fn FcConfigAppFontAddDir(config: *mut c_void, dir: *const u8) -> i32;
    }

    let Ok(c_dir) = CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // Safety: `c_dir` outlives the call; fontconfig copies the path.
    // A null config is never dereferenced — the add is skipped then.
    unsafe {
        let config = FcConfigGetCurrent();
        if config.is_null() {
            return false;
        }
        FcConfigAppFontAddDir(config, c_dir.as_ptr().cast::<u8>()) != 0
    }
}

#[cfg(not(target_os = "linux"))]
fn register_font_dir(_dir: &Path) -> bool {
    false
}
