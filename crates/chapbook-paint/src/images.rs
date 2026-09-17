//! Decoded raster images, keyed by opaque producer ids (chapbook-layout
//! keys by DOM node tag; a comic producer would key by page index). Shared
//! between layout (intrinsic dimensions) and renderers (pixels).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Store identity for renderer-side caches (see [`ImageStore::id`]).
static NEXT_STORE_ID: AtomicU64 = AtomicU64::new(1);

pub struct ImageStore {
    id: u64,
    images: HashMap<u64, StoredImage>,
}

impl Default for ImageStore {
    fn default() -> Self {
        ImageStore {
            id: NEXT_STORE_ID.fetch_add(1, Ordering::Relaxed),
            images: HashMap::new(),
        }
    }
}

/// Premultiplied RGBA8, converted once at [`ImageStore::insert`] — a frame
/// composites images without touching the pixels again (tiny-skia reads
/// them in place; vello marks the upload premultiplied).
pub struct StoredImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// kalam: what the pixels look like, judged once at insert. Decides
    /// how a dark page paints the image (see `DisplayOp::Image::treatment`).
    pub look: ImageLook,
    /// kalam: `rgba` holds the colour negative of the decoded image (see
    /// [`ImageStore::negate_paper`]). A backend asked to paint the image
    /// inverted draws these bytes as they are; asked to paint it plain,
    /// it inverts them back.
    pub negative: bool,
}

/// kalam: a coarse reading of an image's pixels, made once when it is
/// stored, so a dark theme can decide how to paint it without knowing
/// where it came from.
///
/// Publishers ship a great deal of *text* as pictures — part-title
/// plates, chapter ornaments, letters, tables, line drawings — nearly
/// always black ink on a white ground. Painted as-is on a dark page those
/// are glaring white slabs; inverted they read like the text around them.
/// A photograph or a colour illustration inverted is a negative, which
/// nobody wants, so the two have to be told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLook {
    /// Ink on a light ground: mostly light or transparent pixels, next to
    /// no saturated colour, little mid-grey. Safe to invert.
    Paper,
    /// Anything else — photographs, colour art, dark plates, gradients.
    Picture,
}

impl ImageLook {
    /// Judge straight (non-premultiplied) RGBA8 pixels; `rgba` must hold
    /// `width * height * 4` bytes, but only a sample is read so a
    /// full-page plate costs the same as a thumbnail.
    ///
    /// Three measures over the sample. A pixel is *light* when its luma
    /// reaches 176 of 255 or it is transparent (the page shows through);
    /// *vivid* when its chroma — max minus min channel — reaches 64; and
    /// the *tones* are the number of luma bins (of 64) that hold at least
    /// 1 % of the opaque, unvivid pixels. Ink on paper is a few spikes —
    /// the ground, the ink, perhaps a grey fill — with the anti-aliased
    /// edges too thin to register; a photograph is continuous tone and
    /// lights up a run of bins even when it is mostly bright. Paper is at
    /// least 60 % light, at most 5 % vivid and at most 10 tones.
    ///
    /// Set against 24 PRH plates (all `Paper`: 90–99 % light, 1–5 tones),
    /// their colour cover (`Picture`: 2 % light), scanned engravings and
    /// black-and-white portraits, high-key ones included (`Picture`:
    /// 12–43 tones), a colour diagram on white (`Picture`: 12 % vivid),
    /// and grey-on-white tables and line drawings (`Paper`: 2–6 tones).
    pub fn of_rgba(width: u32, height: u32, rgba: &[u8]) -> ImageLook {
        let pixels = (width as usize) * (height as usize);
        if pixels == 0 || rgba.len() < pixels * 4 {
            return ImageLook::Picture;
        }
        // About 4 k samples, on an odd stride so the walk does not lock
        // onto a column of a regular pattern (a ruled table, a stripe).
        let stride = (pixels / 4096).max(1) | 1;
        let (mut total, mut light, mut vivid, mut opaque) = (0usize, 0usize, 0usize, 0usize);
        let mut bins = [0usize; 64];
        for i in (0..pixels).step_by(stride) {
            let px = &rgba[i * 4..i * 4 + 4];
            total += 1;
            if px[3] < 128 {
                light += 1;
                continue;
            }
            let (r, g, b) = (u32::from(px[0]), u32::from(px[1]), u32::from(px[2]));
            if r.max(g).max(b) - r.min(g).min(b) >= 64 {
                vivid += 1;
                continue;
            }
            let luma = (77 * r + 150 * g + 29 * b) >> 8;
            if luma >= 176 {
                light += 1;
            }
            opaque += 1;
            bins[(luma >> 2) as usize] += 1;
        }
        let tones = if opaque == 0 {
            0
        } else {
            bins.iter().filter(|&&n| n * 100 >= opaque).count()
        };
        if light * 10 >= total * 6 && vivid * 20 <= total && tones <= 10 {
            ImageLook::Paper
        } else {
            ImageLook::Picture
        }
    }
}

impl ImageStore {
    /// Store straight (non-premultiplied) RGBA8 — what decoders produce.
    /// Premultiplication happens here, once, instead of per frame in every
    /// backend. Opaque images (comics, PDF pages) pass through unchanged.
    ///
    /// kalam: the image's [`ImageLook`] is judged here too, from the
    /// straight pixels, before they are premultiplied.
    pub fn insert(&mut self, id: u64, width: u32, height: u32, mut rgba: Vec<u8>) {
        debug_assert_eq!(rgba.len(), (width * height * 4) as usize);
        let look = ImageLook::of_rgba(width, height, &rgba);
        for px in rgba.as_chunks_mut::<4>().0 {
            let a = u16::from(px[3]);
            if a < 255 {
                px[0] = (u16::from(px[0]) * a / 255) as u8;
                px[1] = (u16::from(px[1]) * a / 255) as u8;
                px[2] = (u16::from(px[2]) * a / 255) as u8;
            }
        }
        self.images.insert(
            id,
            StoredImage {
                width,
                height,
                rgba,
                look,
                negative: false,
            },
        );
    }

    /// kalam: how the pixels of `id` read — paper or picture. `None` when
    /// the id is not in the store.
    pub fn look(&self, id: u64) -> Option<ImageLook> {
        self.images.get(&id).map(|i| i.look)
    }

    /// kalam: turn every [`ImageLook::Paper`] image into its colour
    /// negative, in place, and mark it so. For a session whose page ground
    /// is dark: its frames paint paper inverted (`ImageTreatment::Invert`),
    /// and a backend that finds the negative already stored screens it
    /// straight from the store instead of copying and inverting a
    /// full-page plate on every draw — which, in a scrolling shell that
    /// repaints whole pages per frame, is the difference between a plate
    /// costing nothing and costing a frame. Idempotent. The store is
    /// rebuilt when the theme changes, so a light session never sees
    /// negatives; a backend copes if one does.
    ///
    /// On premultiplied pixels the negative of channel `c` under alpha
    /// `a` is `a - c`, which is premultiplied still; transparent pixels
    /// stay transparent.
    pub fn negate_paper(&mut self) {
        for image in self.images.values_mut() {
            if image.look != ImageLook::Paper || image.negative {
                continue;
            }
            for px in image.rgba.as_chunks_mut::<4>().0 {
                let a = px[3];
                px[0] = a.saturating_sub(px[0]);
                px[1] = a.saturating_sub(px[1]);
                px[2] = a.saturating_sub(px[2]);
            }
            image.negative = true;
        }
    }

    /// A process-unique identity, changing with every new store. What a
    /// renderer-side cache (e.g. vello's image blobs) keys on to know its
    /// entries describe *this* store — resource ids alone repeat across
    /// chapters, since they come from per-document arenas.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn get(&self, id: u64) -> Option<&StoredImage> {
        self.images.get(&id)
    }

    /// Intrinsic size in CSS px (1 image px = 1 CSS px).
    pub fn dims(&self, id: u64) -> Option<(u32, u32)> {
        self.images.get(&id).map(|i| (i.width, i.height))
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Bytes of decoded pixels held here.
    ///
    /// Exact, and the term that matters: a 1600x2400 comic page is 15.4 MB
    /// of RGBA whatever the display can show, so this is what a session's
    /// cache budget is mostly spending.
    pub fn bytes(&self) -> usize {
        self.images.values().map(|i| i.rgba.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w`×`h` straight-RGBA image of one colour.
    fn flat(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.repeat((w * h) as usize)
    }

    /// `frac` of the rows painted `ink`, the rest `ground`.
    fn ruled(w: u32, h: u32, ground: [u8; 4], ink: [u8; 4], frac: f32) -> Vec<u8> {
        let ink_rows = (h as f32 * frac).round() as u32;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let px = if y < ink_rows { ink } else { ground };
            out.extend_from_slice(&px.repeat(w as usize));
        }
        out
    }

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    #[test]
    fn ink_on_a_light_ground_is_paper() {
        // A text plate: 12 % black rows on white.
        let px = ruled(64, 50, WHITE, BLACK, 0.12);
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Paper);
        // Cream paper with dark ink (a scanned letter).
        let px = ruled(64, 50, [216, 200, 168, 255], [34, 34, 34, 255], 0.12);
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Paper);
        // Ink on transparency: the page is the ground.
        let px = ruled(64, 50, CLEAR, BLACK, 0.12);
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Paper);
        // A grey fill (a table's shaded rows) is still paper: few tones.
        let mut px = ruled(64, 40, WHITE, BLACK, 0.15);
        px.extend(flat(64, 10, [232, 232, 232, 255]));
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Paper);
        // A mostly-white image with a thin coloured rule stays paper.
        let px = ruled(64, 50, WHITE, [200, 0, 0, 255], 0.04);
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Paper);
    }

    #[test]
    fn colour_shading_and_dark_grounds_are_pictures() {
        // The illustrated fixture's square: saturated red over blue.
        let mut px = flat(64, 24, [200, 60, 60, 255]);
        px.extend(flat(64, 24, [60, 60, 200, 255]));
        assert_eq!(ImageLook::of_rgba(64, 48, &px), ImageLook::Picture);
        // White on black (a night plate): inverting it would blind.
        let px = ruled(64, 50, BLACK, WHITE, 0.12);
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Picture);
        // Continuous tone on a bright ground, as a high-key portrait is:
        // 62 % white, the rest sixteen greys in even stripes.
        let mut px = flat(64, 31, WHITE);
        for _ in 0..19 {
            for col in 0..64u8 {
                let grey = 10 + (col / 4) * 10;
                px.extend_from_slice(&[grey, grey, grey, 255]);
            }
        }
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Picture);
        // Colour that is more than an accent — a diagram on white.
        let px = ruled(64, 50, WHITE, [30, 100, 220, 255], 0.12);
        assert_eq!(ImageLook::of_rgba(64, 50, &px), ImageLook::Picture);
    }

    #[test]
    fn degenerate_input_is_a_picture() {
        assert_eq!(ImageLook::of_rgba(0, 0, &[]), ImageLook::Picture);
        // Short buffer: refuse rather than read past it.
        assert_eq!(ImageLook::of_rgba(4, 4, &[255; 16]), ImageLook::Picture);
    }

    #[test]
    fn the_store_keeps_the_look_and_premultiplies() {
        let mut store = ImageStore::default();
        // Half-transparent white over a dark ink row: the look is judged
        // on the straight pixels, the stored bytes are premultiplied.
        let px = ruled(8, 8, [255, 255, 255, 128], BLACK, 0.12);
        store.insert(7, 8, 8, px);
        assert_eq!(store.look(7), Some(ImageLook::Paper));
        assert_eq!(store.look(8), None);
        let stored = store.get(7).unwrap();
        assert_eq!(stored.look, ImageLook::Paper);
        assert!(!stored.negative);
        // Last pixel: 255 * 128 / 255 = 128 in each channel.
        let last = &stored.rgba[stored.rgba.len() - 4..];
        assert_eq!(last, &[128, 128, 128, 128]);
    }

    #[test]
    fn negating_flips_paper_in_place_and_leaves_pictures_alone() {
        let mut store = ImageStore::default();
        store.insert(1, 8, 8, ruled(8, 8, [255, 255, 255, 128], BLACK, 0.12));
        store.insert(2, 8, 8, flat(8, 8, [200, 60, 60, 255]));
        store.negate_paper();

        let paper = store.get(1).unwrap();
        assert!(paper.negative);
        // Black ink (0, 0, 0, 255) becomes white; the half-transparent
        // white ground (128, 128, 128, 128) becomes transparent black
        // at the same alpha — still premultiplied.
        assert_eq!(&paper.rgba[..4], &[255, 255, 255, 255]);
        let last = &paper.rgba[paper.rgba.len() - 4..];
        assert_eq!(last, &[0, 0, 0, 128]);

        let picture = store.get(2).unwrap();
        assert!(!picture.negative);
        assert_eq!(&picture.rgba[..4], &[200, 60, 60, 255]);

        // Idempotent: a second call does not flip the paper back.
        store.negate_paper();
        assert_eq!(&store.get(1).unwrap().rgba[..4], &[255, 255, 255, 255]);
    }
}
