//! The typefaces compiled into the widget.
//!
//! Kalam ships no font files and its reader page named `Literata, Georgia,
//! serif`. Depending on the host having Literata installed is how a book
//! ends up in whatever fontconfig picks; compiling the faces in is how it
//! ends up in Literata on every machine, with no font directory to find
//! at run time. Eight faces, about 3 MB of binary:
//!
//! * **Literata** regular / italic / bold / bold italic — body text.
//! * **Noto Sans** regular / italic / bold / bold italic — what a book's
//!   `font-family: sans-serif` resolves to (tables, captions), and the
//!   widest Latin/Greek/Cyrillic coverage of any open sans.
//!
//! Both are SIL Open Font License 1.1; the licence texts sit beside the
//! files in `fonts/` and travel with any binary (see `NOTICE`).
//!
//! The host's own fonts are *not* scanned unless asked
//! ([`ReaderOptions::host_fonts`](crate::ReaderOptions::host_fonts)).
//! Scanning means touching every font file on the disk — a thousand of
//! them on a desktop with Nerd Fonts installed — and on a cold hard disk
//! that alone is seconds before the first page. The price of skipping it
//! is that characters neither face covers (CJK, Devanagari, Arabic,
//! emoji) draw as empty boxes until the host fonts are switched on.

use std::sync::Arc;

use chapbook_core::{Faces, Fallbacks, FontSource, GenericFamilies, Generics};

/// Family name of the bundled sans face.
pub const SANS_FONT: &str = "Noto Sans";

macro_rules! face {
    ($file:literal) => {
        Faces::Bytes(Arc::from(&include_bytes!(concat!("../fonts/", $file))[..]))
    };
}

/// The font source a session opens with.
///
/// `fonts_dir` adds every face under one folder of the reader's own —
/// roadmap 2.8, a typeface without a system-wide install — and
/// `host_fonts` adds the whole system after that, for script fallback
/// only. Both come *after* the bundled faces, so a name Literata or Noto
/// Sans already answers keeps the face that shipped, and anything dropped
/// in simply joins the picker.
pub fn font_source(host_fonts: bool, fonts_dir: Option<std::path::PathBuf>) -> FontSource {
    let mut faces = vec![
        face!("Literata-Regular.ttf"),
        face!("Literata-Italic.ttf"),
        face!("Literata-Bold.ttf"),
        face!("Literata-BoldItalic.ttf"),
        face!("NotoSans-Regular.ttf"),
        face!("NotoSans-Italic.ttf"),
        face!("NotoSans-Bold.ttf"),
        face!("NotoSans-BoldItalic.ttf"),
    ];
    if let Some(dir) = fonts_dir {
        // Recursive, and a folder that does not exist is simply no faces:
        // the reader creates it, and an empty one costs one directory read.
        faces.push(Faces::Dir(dir));
    }
    if host_fonts {
        faces.push(Faces::Host);
    }
    // Say what the five CSS generics mean rather than let fontconfig
    // answer: `serif` is Literata, everything else Noto Sans. With the
    // host's fonts loaded, `monospace` is left to them (a code listing in
    // a proportional face comes apart); without them it is Noto Sans too,
    // the only sans there is.
    let monospace = if host_fonts { "monospace" } else { SANS_FONT };
    FontSource {
        faces,
        generics: Generics::Explicit(GenericFamilies {
            serif: crate::prefs::BODY_FONT.to_string(),
            sans_serif: SANS_FONT.to_string(),
            monospace: monospace.to_string(),
            cursive: SANS_FONT.to_string(),
            fantasy: SANS_FONT.to_string(),
        }),
        // The platform's fallback list is names; a name that was never
        // loaded is skipped, so with the host off this quietly reduces to
        // "whichever bundled face has the glyph".
        fallback: Fallbacks::Platform,
        locale: None,
    }
}

/// Every typeface a picker may offer when no book is open: the bundled
/// faces plus whatever the user has dropped into `fonts_dir`, matching
/// the session's own list (`Session::font_families`) minus the
/// families only a book's `@font-face` rules can add.
///
/// This builds the same font system a session would open with, just
/// to enumerate it — directory scan included — so call it off the UI
/// thread. A build failure falls back to the two bundled names: a
/// picker with the defaults beats an empty one.
pub fn available_font_families(fonts_dir: Option<std::path::PathBuf>) -> Vec<String> {
    let source = font_source(false, fonts_dir);
    let Ok((system, _report)) = chapbook_layout::build_font_system(&source) else {
        return vec![crate::prefs::BODY_FONT.to_string(), SANS_FONT.to_string()];
    };
    let mut families: Vec<String> = system
        .db()
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    // The embedded math face is a rendering resource, not a reading
    // typeface: a family picker must not offer it (the session's list
    // filters it the same way).
    families.retain(|name| name != chapbook_layout::MATH_FONT_FAMILY);
    families.sort_unstable();
    families.dedup();
    families
}
