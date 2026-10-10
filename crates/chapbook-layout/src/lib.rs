//! **Internal to chapbook — no API stability.** Depend on
//! `chapbook-reader`; see `docs/STABILITY.md`.
//!
//! The pagination-first layout engine — chapbook's differentiator.
//!
//! The whole stylo-facing half of chapbook lives here, because it moves as
//! one: [`dom`] is the arena DOM and stylo's DOM-trait bindings, [`cascade`]
//! drives stylo's `Stylist` over it, and the rest of the crate turns the
//! result into pages. A stylo upgrade rewrites all three together.
//!
//! Pipeline: parse → cascade → fragmentation sidecar cascade (servo-mode stylo
//! lacks the break properties) → box tree (CSS 2.1 §9.2 anonymous boxes) →
//! cosmic-text inline layout per inline formatting context → streaming page
//! cursor applying break rules and widows/orphans → [`ChapterLayout`].
//!
//! One spine item is one layout run. Locator offsets (see chapbook-core's
//! locator docs) thread through every line fragment, so positions survive
//! relayout via [`ChapterLayout::page_of`].
//!
//! Gaps, documented in the module that owns each (all in `crate::boxtree`):
//! `counter()`/`counters()` and images in generated `content`, and
//! shrink-to-fit non-replaced floats, which stay in flow.

mod boxtree;
pub mod cascade;
pub mod dom;
mod fonts;
mod fragmentation;
mod hyphenate;
#[cfg(feature = "mathml")]
mod mathml;
mod paginate;
mod style_to_attrs;
#[cfg(feature = "svg")]
mod svg;
mod table;
mod webfonts;

use std::collections::HashMap;

use cosmic_text::FontSystem;

use chapbook_core::PageMetrics;
use chapbook_paint::{ImageStore, Page};

use crate::dom::Document;

pub use fonts::build_font_system;
#[cfg(feature = "mathml")]
pub use fonts::MATH_FONT_FAMILY;
pub use fragmentation::{BreakRule, FragRules, FragStyle};
pub use webfonts::{extract_font_faces, register_font, FontFace};

/// The paginated result of laying out one spine item.
pub struct ChapterLayout {
    pub pages: Vec<Page>,
    /// Per page: locator-text char offset at which the page starts
    /// (monotonic non-decreasing). The relayout-survival map for
    /// `Locator::char_offset`.
    pub char_map: Vec<u32>,
    /// Element `id` attribute → page index (TOC fragment jumps).
    pub anchors: HashMap<String, usize>,
    /// kalam: per page, the flow space (CSS px) the page break before it
    /// discarded — the block margin that would have separated the last
    /// content of the previous page from the first content of this one,
    /// had there been no break. `gaps[0]` is always `0.0`.
    ///
    /// Pagination throws that space away, as CSS says it must
    /// (margins truncate at fragmentainer boundaries). A shell that
    /// shows the pages glued end to end as one scroll needs it back:
    /// without it, a paragraph that starts a page sits flush against
    /// the paragraph that ended the page before, and every page seam
    /// reads as a missing blank line. See [`ChapterLayout::used_height`].
    pub gaps: Vec<f32>,
}

impl ChapterLayout {
    /// Roughly how many heap bytes this laid-out chapter holds. See
    /// [`Page::approx_bytes`](chapbook_paint::Page::approx_bytes) for why
    /// approximate is the right precision here.
    pub fn approx_bytes(&self) -> usize {
        use std::mem::size_of;
        self.pages.capacity() * size_of::<chapbook_paint::Page>()
            + self.pages.iter().map(|p| p.approx_bytes()).sum::<usize>()
            + self.char_map.capacity() * size_of::<u32>()
            + self.gaps.capacity() * size_of::<f32>()
            + self
                .anchors
                .keys()
                .map(|k| k.len() + size_of::<usize>())
                .sum::<usize>()
    }

    /// Page containing the given locator offset.
    pub fn page_of(&self, char_offset: u32) -> usize {
        page_of(&self.char_map, char_offset)
    }

    /// kalam: how much of a page's content box is actually used — the
    /// content-relative bottom of its lowest fragment, in CSS px. A page
    /// that a forced break or a kept heading left half empty says so
    /// here, where `Page::size` says only what the page *could* hold.
    ///
    /// Box slices that continue on to the next page are not counted:
    /// they reach the page bottom because the box goes on, not because
    /// anything is there. `0.0` for an empty page. Can exceed the content
    /// height by a line where pagination allowed one to overflow rather
    /// than leave a page empty; a strip should show it, not clip it.
    pub fn used_height(&self, page: usize) -> f32 {
        let Some(page) = self.pages.get(page) else {
            return 0.0;
        };
        let top = page.content.origin.y;
        page.fragments
            .iter()
            .filter(|f| match &f.kind {
                chapbook_paint::FragmentKind::Box(slice) => slice.last_slice,
                _ => true,
            })
            .map(|f| f.rect.max_y() - top)
            .fold(0.0f32, f32::max)
    }

    /// kalam: the flow space discarded before `page` — see
    /// [`ChapterLayout::gaps`]. `0.0` for page 0 and for any page index
    /// the layout does not have.
    pub fn gap_before(&self, page: usize) -> f32 {
        self.gaps.get(page).copied().unwrap_or(0.0)
    }
}

fn page_of(char_map: &[u32], char_offset: u32) -> usize {
    if char_map.is_empty() {
        return 0;
    }
    char_map
        .partition_point(|start| *start <= char_offset)
        .saturating_sub(1)
}

/// Paginate a styled document (the cascade must have run: see
/// [`cascade::StyleEngine::style_document`]).
///
/// `css_sources` are the same author sheets given to the style engine — the
/// fragmentation sidecar re-reads them for the break properties stylo
/// doesn't carry.
pub fn paginate(
    doc: &Document,
    css_sources: &[String],
    page: &PageMetrics,
    fonts: &mut FontSystem,
    images: &ImageStore,
) -> ChapterLayout {
    let frag = FragRules::parse(css_sources).resolve(doc);
    let locator = crate::dom::locator_offsets(doc);
    #[cfg(feature = "mathml")]
    let math = crate::mathml::prepare(doc, fonts, &locator);
    let input = boxtree::BoxTreeInput {
        doc,
        frag: &frag,
        locator: &locator,
        images,
        #[cfg(feature = "mathml")]
        math: &math,
        quote_depth: std::cell::Cell::new(0),
    };

    let mut paginator = paginate::Paginator::new(fonts, *page);
    if let Some(root) = boxtree::build_box_tree(&input) {
        paginator.place_block(&root, 0.0, page.content_width());
    }
    let (pages, char_map, gaps) = paginator.finish();

    // Anchors: element id → page, via each element's locator offset.
    let mut anchors = HashMap::new();
    for id in doc.descendants(doc.root()) {
        if let crate::dom::NodeData::Element(el) = &doc.node(id).data {
            if let (Some(id_attr), Some(offset)) = (&el.id, locator.get(&id)) {
                anchors.insert(id_attr.to_string(), page_of(&char_map, *offset));
            }
        }
    }

    ChapterLayout {
        pages,
        char_map,
        anchors,
        gaps,
    }
}

/// Decode every `<img>`'s bytes — and rasterize every inline `<svg>` — into
/// an [`ImageStore`] keyed by node tag. `fetch` resolves an `src` attribute
/// or SVG `<image>` href (as written) to raw bytes — callers close over
/// their container (e.g. `Book::resource` against the chapter path).
/// Undecodable or unresolvable images are skipped; layout degrades them to
/// nothing (an inline `<svg>` degrades to its flattened text).
///
/// `fonts` shapes any `<text>` inside SVG content; `None` renders SVG
/// without text. Ignored entirely when the `svg` feature is off.
#[cfg_attr(not(feature = "svg"), allow(unused_variables, unused_mut))]
pub fn collect_images(
    doc: &Document,
    fonts: Option<&FontSystem>,
    max_edge: Option<u32>,
    mut fetch: impl FnMut(&str) -> Option<Vec<u8>>,
) -> ImageStore {
    let mut store = ImageStore::default();
    // Built on first use: rebuilding the session's faces as usvg's fontdb
    // is not free, and most books have no SVG at all — and of those that
    // do, most carry no `<text>`, so `rasterize` may never ask.
    #[cfg(feature = "svg")]
    let mut svg_fonts: Option<std::sync::Arc<resvg::usvg::fontdb::Database>> = None;
    #[cfg(feature = "svg")]
    let mut svg_db = || {
        svg_fonts
            .get_or_insert_with(|| match fonts {
                Some(fonts) => crate::svg::svg_fontdb(fonts),
                None => std::sync::Arc::new(resvg::usvg::fontdb::Database::new()),
            })
            .clone()
    };

    // `<img>` nodes and their sources up front, so a source referenced by
    // several elements (a repeated ornament, a shared figure) fetches and
    // decodes once. Only repeated sources pay for a cache entry, and the
    // cache dies with this call — the single-use image costs what it did.
    let mut img_nodes: Vec<(crate::dom::NodeId, &str)> = Vec::new();
    let mut uses: HashMap<&str, u32> = HashMap::new();
    for id in doc.descendants(doc.root()) {
        let crate::dom::NodeData::Element(el) = &doc.node(id).data else {
            continue;
        };
        if *el.local_name() != markup5ever::local_name!("img") {
            continue;
        }
        let Some(src) = el.attr(&markup5ever::local_name!("src")) else {
            continue;
        };
        img_nodes.push((id, src));
        *uses.entry(src).or_insert(0) += 1;
    }

    type Decoded = Option<(u32, u32, Vec<u8>)>;
    let mut decoded_cache: HashMap<&str, Decoded> = HashMap::new();
    // What the images cost as decoded against what they cost after being
    // scaled down to what a page can actually draw. The gap is the entire
    // point of `max_edge`, and without reporting it there is no way to tell
    // whether the limit is doing anything for a given book.
    let (mut decoded_px, mut stored_px) = (0u64, 0u64);
    for (id, src) in img_nodes {
        if let Some(cached) = decoded_cache.get(src) {
            if let Some((w, h, rgba)) = cached {
                store.insert(crate::dom::node_tag(id), *w, *h, rgba.clone());
            }
            continue;
        }
        let decoded = fetch(src).and_then(|bytes| {
            #[cfg(feature = "svg")]
            if crate::svg::sniff(&bytes) {
                // Hrefs inside the SVG file resolve relative to the file,
                // not the chapter that embedded it.
                let mut nested = |href: &str| fetch(&join_href(src, href));
                return crate::svg::rasterize(&bytes, &mut nested, &mut svg_db);
            }
            let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            Some((w, h, rgba.into_raw()))
        });
        let decoded = decoded.map(|(w, h, rgba)| {
            decoded_px += u64::from(w) * u64::from(h);
            let (w, h, rgba) = shrink_to_edge(w, h, rgba, max_edge);
            stored_px += u64::from(w) * u64::from(h);
            (w, h, rgba)
        });
        if uses[src] > 1 {
            decoded_cache.insert(src, decoded.clone());
        }
        if let Some((w, h, rgba)) = decoded {
            store.insert(crate::dom::node_tag(id), w, h, rgba);
        }
    }

    if decoded_px > stored_px {
        log::info!(
            "images: {:.1} MB decoded, {:.1} MB kept (max edge {} px)",
            decoded_px as f64 * 4.0 / 1_048_576.0,
            stored_px as f64 * 4.0 / 1_048_576.0,
            max_edge.unwrap_or(0)
        );
    }

    // Inline `<svg>` subtrees, serialized at parse time. Their hrefs are
    // chapter-relative, exactly like an `<img>` src.
    #[cfg(feature = "svg")]
    for (id, xml) in doc.svg_sources() {
        if let Some((w, h, rgba)) = crate::svg::rasterize(xml.as_bytes(), &mut fetch, &mut svg_db) {
            store.insert(crate::dom::node_tag(id), w, h, rgba);
        }
    }
    store
}

/// Resolve `href` against the directory of `src` (both as written in the
/// book). The container's own resolution handles any `..` segments.
#[cfg(feature = "svg")]
fn join_href(src: &str, href: &str) -> String {
    match src.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{href}"),
        None => href.to_string(),
    }
}

/// Scale raw RGBA down so neither edge exceeds `max_edge`, keeping the aspect
/// ratio.
///
/// Returns the input **untouched** when it already fits, or when `max_edge` is
/// `None` or `0`. Untouched means literally the same bytes, not "resized to the
/// same size": a resize that changes nothing still resamples, and a golden
/// render compared byte for byte would notice.
///
/// The point is memory, and it is large. Raw RGBA costs four bytes a pixel, so
/// a 3000×4000 scan is 48 MB decoded while occupying at most a page of screen —
/// roughly 1200 px wide on the machine this was written for. Nothing can draw
/// wider than the content box, so the extra pixels are pure waste, and on a
/// machine with 4 GB in total the waste is what pushes a single chapter past
/// the reader's whole cache budget.
///
/// Bilinear (`Triangle`) rather than Lanczos: the difference is hard to see at
/// reading size and Lanczos is meaningfully slower on a weak CPU.
fn shrink_to_edge(w: u32, h: u32, rgba: Vec<u8>, max_edge: Option<u32>) -> (u32, u32, Vec<u8>) {
    let Some(max) = max_edge.filter(|m| *m > 0) else {
        return (w, h, rgba);
    };
    let longest = w.max(h);
    if longest <= max {
        return (w, h, rgba);
    }
    // `from_raw` only fails on a length mismatch, which a buffer this module
    // produced cannot have. Check anyway so a mismatch returns the image
    // untouched instead of dropping it.
    if rgba.len() != w as usize * h as usize * 4 {
        return (w, h, rgba);
    }
    let Some(img) = image::RgbaImage::from_raw(w, h, rgba) else {
        return (w, h, Vec::new());
    };
    let scale = u64::from(max);
    let longest = u64::from(longest);
    let nw = (u64::from(w) * scale / longest).max(1) as u32;
    let nh = (u64::from(h) * scale / longest).max(1) as u32;
    let out = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    let (w, h) = (out.width(), out.height());
    (w, h, out.into_raw())
}

#[cfg(test)]
mod tests {
    use super::shrink_to_edge;

    fn solid(w: u32, h: u32) -> Vec<u8> {
        vec![120u8; w as usize * h as usize * 4]
    }

    #[test]
    fn an_oversized_image_is_scaled_to_the_limit() {
        let (w, h, rgba) = shrink_to_edge(3000, 4000, solid(3000, 4000), Some(1000));
        assert_eq!((w, h), (750, 1000), "longest edge lands on the limit");
        assert_eq!(rgba.len(), 750 * 1000 * 4);
        // 48 MB of pixels down to 3 MB — the whole reason this exists.
        assert!(rgba.len() < 4_000_000);
    }

    #[test]
    fn an_image_that_already_fits_is_returned_untouched() {
        let pixels = solid(100, 50);
        let kept = pixels.clone();
        let (w, h, rgba) = shrink_to_edge(100, 50, pixels, Some(1000));
        assert_eq!((w, h), (100, 50));
        assert_eq!(rgba, kept, "not resampled: the same bytes back");
    }

    #[test]
    fn no_limit_means_no_change() {
        let pixels = solid(3000, 4000);
        let kept = pixels.clone();
        for limit in [None, Some(0)] {
            let (w, h, rgba) = shrink_to_edge(3000, 4000, pixels.clone(), limit);
            assert_eq!((w, h), (3000, 4000));
            assert_eq!(rgba, kept);
        }
    }

    /// A wide strip must keep its shape. Scaling by the longer edge is what
    /// makes a 4000×200 image become 1000×50 rather than 1000×200 stretched.
    #[test]
    fn the_aspect_ratio_survives() {
        let (w, h, _) = shrink_to_edge(4000, 200, solid(4000, 200), Some(1000));
        assert_eq!((w, h), (1000, 50));
    }

    #[test]
    fn a_tiny_image_is_never_upscaled() {
        let (w, h, rgba) = shrink_to_edge(2, 1, solid(2, 1), Some(1000));
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgba.len(), 8);
    }

    /// A buffer that does not match its stated size must not panic in a
    /// renderer. Unreachable from `collect_images`, which builds both from the
    /// same decode, but the function is total rather than trusting that.
    #[test]
    fn a_mismatched_buffer_is_returned_untouched() {
        let (w, h, rgba) = shrink_to_edge(100, 100, vec![0u8; 16], Some(10));
        assert_eq!((w, h), (100, 100));
        assert_eq!(rgba.len(), 16);
    }
}
