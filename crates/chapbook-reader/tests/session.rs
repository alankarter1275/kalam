//! Headless session behavior over the fixture books — the point of the
//! session extraction: reading logic testable without a window. The
//! per-topic suites live beside this file; this one keeps the core
//! read/render/select pass per format and the session's construction
//! contract.

mod common;
use chapbook_core::{BookKind, Rect};
use chapbook_reader::chapbook_paint::ImageTreatment;
use chapbook_reader::{Session, SessionConfig};
use common::*;

#[test]
fn epub_session_renders_navigates_and_selects() {
    let mut s = open_isolated("epub-nav", &fixture("epub/illustrated.epub"));
    assert_eq!(s.kind(), BookKind::Epub);
    s.set_metrics(metrics());
    let pixmap = s.render().expect("page renders");
    assert_eq!((pixmap.width(), pixmap.height()), (600, 800));
    assert!(s.page_count() >= 2);

    // Selection by hit test: press near the top text line, drag right and
    // down a line. Which character that lands on is a function of the
    // host's fonts, so this half asserts the shape of the result — it runs
    // from the heading into the paragraph below it — and the deterministic
    // half below asserts the exact text.
    assert!(s.selection_begin(100.0, 70.0), "press must hit the heading");
    s.selection_drag(400.0, 140.0);
    let (start, end) = s.selected_range().expect("non-empty selection");
    assert!(end > start);
    let text = s.selected_text().expect("selection carries text");
    assert!(
        text.contains("Illustrated Chapter"),
        "selection starts in the heading: {text:?}"
    );
    assert!(
        text.contains("Text before the picture"),
        "selection runs into the first paragraph: {text:?}"
    );
    assert!(!text.contains('\n'), "pasteable text has no line breaks");

    // The selected text comes back ready to paste: the locator space is
    // the raw source text, so its line breaks and indentation collapse.
    // chapter1.xhtml ends a source line after the link and indents the
    // next, so a range spanning the two proves the collapse exactly.
    let (link_start, _) = only_hit(&mut s, "underlined link");
    let (_, after_end) = only_hit(&mut s, "and some");
    s.select_range(link_start, after_end);
    assert_eq!(
        s.selected_text().as_deref(),
        Some("underlined link and some"),
        "the source breaks the line and indents between these two"
    );
    s.selection_clear();
    // The selected page renders with the highlight without panicking.
    s.render().unwrap();
    // Page navigation clears the selection.
    s.next_page();
    assert_eq!(s.selected_range(), None);
    assert_eq!(s.page(), 1);
    s.prev_page();
    assert_eq!(s.page(), 0);
}

#[test]
#[cfg(feature = "cbz")]
fn cbz_session_pages_through_images() {
    let mut s = open_isolated("cbz-pages", &fixture("cbz/minimal.cbz"));
    assert_eq!(s.kind(), BookKind::Comic);
    assert_eq!(s.spine_len(), 3);
    s.set_metrics(metrics());
    assert_eq!(s.page_count(), 1, "one page per comic unit");

    // Page 1 is solid red (196,64,48): sample the center.
    let px = render_loaded(&mut s).pixel(300, 400).unwrap();
    assert!(
        px.red() > 150 && px.blue() < 90,
        "expected red page: {px:?}"
    );

    // Selection never engages on image pages, so there is nothing to copy.
    assert!(!s.selection_begin(300.0, 400.0));
    assert_eq!(s.selected_text(), None);

    s.next_page();
    assert_eq!(s.spine(), 1, "page turn advances the spine for comics");
    let px = render_loaded(&mut s).pixel(300, 400).unwrap();
    assert!(
        px.green() > 90 && px.red() < 90,
        "expected green page: {px:?}"
    );
    s.next_page();
    let px = render_loaded(&mut s).pixel(300, 400).unwrap();
    assert!(
        px.blue() > 120 && px.red() < 90,
        "expected blue page 10 last: {px:?}"
    );
    // End of book: stays put.
    s.next_page();
    assert_eq!(s.spine(), 2);
}

#[test]
#[cfg(feature = "pdf")]
fn pdf_session_reads_like_an_image_book() {
    let mut s = open_isolated("pdf-read", &fixture("pdf/minimal.pdf"));
    assert_eq!(s.kind(), BookKind::Pdf);
    assert_eq!(s.spine_len(), 3);
    s.set_metrics(metrics());
    // Red first page, blue second; the rect pages carry no text.
    let px = render_loaded(&mut s).pixel(300, 400).unwrap();
    assert!(px.red() > 150 && px.blue() < 100, "red PDF page: {px:?}");
    assert!(!s.selection_begin(300.0, 400.0));
    s.next_page();
    assert_eq!(s.spine(), 1);
    let px = render_loaded(&mut s).pixel(300, 400).unwrap();
    assert!(px.blue() > 120 && px.red() < 100, "blue PDF page: {px:?}");
}

#[test]
#[cfg(feature = "pdf")]
fn pdf_text_selection_highlights() {
    let mut s = open_isolated("pdf-select", &fixture("pdf/minimal.pdf"));
    s.set_metrics(metrics());
    // Page 3 carries the Helvetica text lines.
    s.next_page();
    s.next_page();
    assert_eq!(s.spine(), 2);
    let before = render_loaded(&mut s);

    let (ax, ay) = sweep_for_text(&mut s);
    s.selection_drag(ax + 150.0, ay);
    let (start, end) = s.selected_range().expect("selection over PDF text");
    assert!(end > start);
    let text = s.selected_text().expect("PDF selection carries text");
    assert!(
        "Hello selection".starts_with(&text),
        "expected a prefix of the first text line, got {text:?}"
    );

    // The highlight visibly changes the render.
    let after = render_loaded(&mut s);
    assert_ne!(before.data(), after.data(), "selection must paint");
    s.selection_clear();
}

/// A session may cross threads but may not be shared across them, which is
/// the shape every binding is built on: a host holds one
/// opaque handle, moves it freely, and needs no lock of its own. `Sync`
/// fails today on the loader's receiver and on rusqlite's connection, so
/// only the half we actually rely on is asserted here — if `Send` ever
/// goes, the FFI's threading contract goes with it.
#[test]
fn a_session_can_move_between_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<Session>();
}

/// The failure `FontSource` exists to convert into an error.
///
/// A session that finds no faces is not an ereader that looks wrong; it is
/// an ereader that cannot move. Every book paginates to one blank page, so
/// there is nowhere to navigate to, nothing for search to find, and no page
/// for a TOC entry to land on — and the session lays out, renders, paints
/// and *conforms* the whole time. fontdb has no Android, iOS or wasm
/// branch, which makes that the ordinary case on three platforms rather
/// than a corner.
#[test]
fn a_session_with_no_faces_is_refused_at_construction() {
    let empty = chapbook_core::FontSource::embedded("/nonexistent/fonts", "Nothing");
    let result = Session::open_with(fixture("epub/minimal.epub"), SessionConfig::new(empty));

    let err = result.err().expect("a fontless session must not open");
    let message = err.to_string();
    assert!(message.contains("no faces"), "{message}");
}

/// The read-back half: a shell cannot offer a font-family picker over a
/// list it cannot obtain.
#[test]
fn the_session_can_enumerate_the_families_it_was_given() {
    let session = open_isolated("families", &fixture("epub/minimal.epub"));

    let families = session.font_families();
    assert_eq!(families, vec!["Crimson Text".to_string()]);
    // And the source resolved cleanly, which is what a shell would print.
    assert!(
        session.font_report().is_clean(),
        "{}",
        session.font_report()
    );
    assert_eq!(session.font_report().faces, 4);
}

/// The host capabilities a session used to reach for on its own now arrive
/// through `SessionConfig`. This covers the plumbing; the credential retry
/// flow itself lives in `transport.rs`, which can drive it now that the
/// transport is injectable too.
#[test]
fn a_session_takes_its_host_capabilities_explicitly() {
    use chapbook_core::{Credential, CredentialKey, CredentialStore, Freshness, MemoryCredentials};

    let store = std::sync::Arc::new(MemoryCredentials::new());
    let key = CredentialKey::http_origin("https://cat.example.com/opds/abc123secret/").unwrap();
    store
        .store(&key, &Credential::basic("reader", "pw"))
        .unwrap();

    let config = SessionConfig::new(fixture_fonts()).with_credentials(store.clone());
    // A store in the config is not a store the local path consults.
    let session = Session::open_with(fixture("epub/minimal.epub"), config).unwrap();
    assert!(session.spine_len() > 0);

    // And the config's Debug is safe to log: no secret in it.
    let shown = format!("{:?}", SessionConfig::new(fixture_fonts()));
    assert!(!shown.contains("pw"), "{shown}");

    assert!(matches!(
        store.get(&key, Freshness::Cached),
        chapbook_core::CredentialLookup::Found(_)
    ));
}

// ---- kalam: selection handles ----

/// Taking hold of one end of a selection and dragging moves that end
/// only; the other stays where it was. A drag past the other end crosses
/// and keeps following — the range is normalised when read.
#[test]
fn grabbing_an_end_moves_only_that_end() {
    let mut s = open_isolated("epub-grab", &fixture("epub/illustrated.epub"));
    s.set_metrics(metrics());
    s.render().expect("page renders");

    // Nothing to grab without a selection.
    assert!(!s.selection_grab_end(true));
    assert_eq!(s.selected_range(), None);

    let (a, b) = only_hit(&mut s, "Text before the picture");
    let (later, _) = only_hit(&mut s, "aliased embedded font");
    assert!(later > b, "the later paragraph is further into the chapter");
    let later_line = s
        .range_rects(later, later + 7)
        .first()
        .copied()
        .expect("the later paragraph is on this page");
    let later_point = (
        later_line.min_x() + 2.0,
        later_line.min_y() + later_line.size.h / 2.0,
    );

    // Grab the end and drag it to the later paragraph: the start stays.
    s.select_range(a, b);
    assert!(s.selection_grab_end(false));
    s.selection_drag(later_point.0, later_point.1);
    let (start2, end2) = s.selected_range().expect("still a selection");
    assert_eq!(start2, a, "the start did not move");
    assert!(
        end2 >= later,
        "the end followed the pointer: {start2}..{end2}"
    );

    // Grab the start of a fresh range and drag it there instead: the
    // ends cross, the old end is now the start.
    s.select_range(a, b);
    assert!(s.selection_grab_end(true));
    s.selection_drag(later_point.0, later_point.1);
    let (start3, end3) = s.selected_range().expect("a crossed drag still selects");
    assert_eq!(start3, b, "the fixed end became the start");
    assert!(
        end3 >= later,
        "and the moved end passed it: {start3}..{end3}"
    );
}

/// The live selection's blend follows the page ground: multiply over a
/// light palette, screen over a dark one. Stored highlights stay normal.
#[test]
fn selection_blend_follows_the_page_ground() {
    use chapbook_core::{Palette, Rgba, Theme};
    use chapbook_reader::chapbook_paint::{Blend, DisplayOp};

    let mut s = open_isolated("epub-blend", &fixture("epub/illustrated.epub"));
    s.set_metrics(metrics());
    assert_eq!(
        s.selection_blend(),
        Blend::Multiply,
        "the default theme is light"
    );

    let mut settings = s.settings().clone();
    settings.theme = Theme::Dark;
    settings.palette = Some(Palette {
        background: Rgba::new(0x1b, 0x1e, 0x24, 255),
        ..Palette::of(Theme::Dark)
    });
    s.set_settings(settings, chapbook_reader::SettingsScope::Global);
    assert_eq!(s.selection_blend(), Blend::Screen);

    s.render().expect("page renders");
    let (a, b) = only_hit(&mut s, "Text before the picture");
    s.select_range(a, b);
    let frame = s.frame().expect("a frame");
    let band = frame
        .list
        .ops
        .iter()
        .find_map(|op| match op {
            DisplayOp::Band { blend, .. } => Some(*blend),
            _ => None,
        })
        .expect("the selection is a band op");
    assert_eq!(band, Blend::Screen);
}

// ---- kalam: how images meet the page ground ----

/// An image op: resource, destination rect, treatment.
type ImageOp = (u64, Rect, ImageTreatment);

/// The image ops of the current frame, in page order.
fn image_ops(s: &mut Session) -> Vec<ImageOp> {
    use chapbook_reader::chapbook_paint::DisplayOp;
    let frame = s.frame().expect("a frame");
    frame
        .list
        .ops
        .iter()
        .filter_map(|op| match op {
            DisplayOp::Image {
                resource,
                dest,
                treatment,
            } => Some((*resource, *dest, *treatment)),
            _ => None,
        })
        .collect()
}

/// One of Kalam's palettes on the matching engine theme: its Sepia paper
/// (`#f5f0e8`) or its Dark ground (`#1b1e24`).
fn go(s: &mut Session, theme: chapbook_core::Theme) {
    use chapbook_core::{Palette, Rgba, Theme};
    let background = if theme == Theme::Dark {
        Rgba::new(0x1b, 0x1e, 0x24, 255)
    } else {
        Rgba::new(0xf5, 0xf0, 0xe8, 255)
    };
    let mut settings = s.settings().clone();
    settings.theme = theme;
    settings.palette = Some(Palette {
        background,
        ..Palette::of(theme)
    });
    s.set_settings(settings, chapbook_reader::SettingsScope::Global);
}

fn rgb(px: chapbook_reader::tiny_skia::PremultipliedColorU8) -> (u8, u8, u8) {
    (px.red(), px.green(), px.blue())
}

/// A picture — the illustrated fixture's saturated two-tone square —
/// paints plain on every ground: never tinted, never a negative. The
/// store's own verdict is what the frame carries.
#[test]
fn a_picture_keeps_its_colours_on_every_ground() {
    use chapbook_reader::chapbook_paint::ImageLook;

    let mut s = open_isolated("epub-picture", &fixture("epub/illustrated.epub"));
    s.set_metrics(metrics());
    s.render().expect("page renders");

    let light = image_ops(&mut s);
    assert!(!light.is_empty(), "the first page carries the square");
    for (resource, _, treatment) in &light {
        let look = s.image_store().look(*resource);
        assert_eq!(look, Some(ImageLook::Picture), "a saturated square");
        assert_eq!(*treatment, ImageTreatment::Plain, "light ground");
    }

    go(&mut s, chapbook_core::Theme::Dark);
    let dark = image_ops(&mut s);
    assert_eq!(dark.len(), light.len(), "same images after the relayout");
    for (_, _, treatment) in &dark {
        assert_eq!(*treatment, ImageTreatment::Plain, "dark ground");
    }

    // The pixels agree: the square's red is still red on the dark page.
    let pixmap = s.render().expect("dark page renders");
    let dest = dark[0].1;
    let px = pixmap.pixel(dest.origin.x as u32 + 4, dest.origin.y as u32 + 4);
    let (r, g, b) = rgb(px.expect("inside the page"));
    assert!(r > 150 && g < 100 && b < 100, "stays red: {r} {g} {b}");
}

/// The whole chain on the plates fixture — a PRH-shaped part-title plate
/// (black lettering on white, two inline-block wrappers deep) and a
/// colour picture. On white the plate is painted as decoded; on sepia
/// paper it multiplies, so its white takes the paper's tint; on a dark
/// ground its white vanishes into the page and its lettering comes out
/// light. The picture keeps its colours throughout.
#[test]
fn a_plate_follows_the_page_ground_and_a_picture_does_not() {
    use chapbook_core::Theme;
    use chapbook_reader::chapbook_paint::ImageLook;

    let mut s = open_isolated("epub-plates", &fixture("epub/plates.epub"));
    s.set_metrics(metrics());
    s.render().expect("page renders");

    // Two image ops, in page order: the plate, then the picture.
    let white = image_ops(&mut s);
    assert_eq!(white.len(), 2, "both images are placed: {white:?}");
    assert_eq!(s.image_store().look(white[0].0), Some(ImageLook::Paper));
    assert_eq!(s.image_store().look(white[1].0), Some(ImageLook::Picture));
    assert_eq!(white[0].2, ImageTreatment::Plain, "white has no tint");
    assert_eq!(white[1].2, ImageTreatment::Plain);

    // The plate's ink bar spans rows 24..36 of 60 and columns 20..100 of
    // 120: sample the bar's centre, and a corner of the white ground.
    let lettering = |pixmap: &chapbook_reader::tiny_skia::Pixmap, rect: &Rect| {
        let cx = (rect.origin.x + rect.size.w * 0.5) as u32;
        let cy = (rect.origin.y + rect.size.h * 0.5) as u32;
        let gx = (rect.origin.x + rect.size.w * 0.05) as u32;
        let gy = (rect.origin.y + rect.size.h * 0.1) as u32;
        let ink = rgb(pixmap.pixel(cx, cy).unwrap());
        let ground = rgb(pixmap.pixel(gx, gy).unwrap());
        (ink, ground)
    };

    // White page: black lettering on a white ground.
    let pixmap = s.render().expect("white page renders");
    let (ink, ground) = lettering(&pixmap, &white[0].1);
    assert_eq!(ink, (0, 0, 0));
    assert_eq!(ground, (255, 255, 255));

    // Sepia paper: the lettering stays black, the plate's white becomes
    // the paper — no white rectangle on a cream page.
    go(&mut s, Theme::Sepia);
    let sepia = image_ops(&mut s);
    assert_eq!(sepia.len(), 2, "{sepia:?}");
    assert_eq!(sepia[0].2, ImageTreatment::Multiply, "plate multiplies");
    assert_eq!(sepia[1].2, ImageTreatment::Plain, "the picture does not");
    let pixmap = s.render().expect("sepia page renders");
    let (ink, ground) = lettering(&pixmap, &sepia[0].1);
    assert_eq!(ink, (0, 0, 0));
    assert_eq!(ground, (0xf5, 0xf0, 0xe8), "the white is the paper");

    go(&mut s, Theme::Dark);
    let dark = image_ops(&mut s);
    assert_eq!(dark.len(), 2, "{dark:?}");
    assert_eq!(dark[0].2, ImageTreatment::Invert, "the plate inverts");
    assert_eq!(dark[1].2, ImageTreatment::Plain, "the picture does not");
    // The dark session stored the plate as a negative, the picture as is.
    assert!(s.image_store().get(dark[0].0).unwrap().negative);
    assert!(!s.image_store().get(dark[1].0).unwrap().negative);

    // Dark page: light lettering, and the plate's white ground is the page.
    let pixmap = s.render().expect("dark page renders");
    let (ink, ground) = lettering(&pixmap, &dark[0].1);
    assert_eq!(ink, (255, 255, 255), "lettering is light");
    assert_eq!(ground, (0x1b, 0x1e, 0x24), "no slab");
    // The picture's top-left is its own red-ish colour, not a negative.
    let photo = dark[1].1;
    let px = pixmap.pixel(photo.origin.x as u32 + 2, photo.origin.y as u32 + 2);
    let (r, g, _) = rgb(px.expect("inside the page"));
    assert!(r > 150 && g < 100, "picture keeps its colour: {r} {g}");
}
