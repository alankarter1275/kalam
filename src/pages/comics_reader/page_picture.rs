//! A comic page painted through the Image pane's draw-time settings.
//!
//! `GtkPicture` can scale a texture, but it has nowhere to put a GSK
//! colour-matrix node or a sub-rectangle of the source. This small
//! widget keeps those two operations in the snapshot itself: the full
//! texture is placed so that the analysed crop rectangle lands on the
//! contained destination rectangle, the destination clips away the
//! rest, and the colour transform wraps that one texture node. The
//! source texture is never copied or rewritten when a slider moves.

use std::cell::{Cell, RefCell};

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use super::image_processing::color_transform;
use super::types::{ComicImageSettings, CropRect};

glib::wrapper! {
    pub struct ComicPagePicture(ObjectSubclass<imp::ComicPagePicture>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ComicPagePicture {
    pub fn new(texture: &gdk::Texture, settings: ComicImageSettings, crop: CropRect) -> Self {
        let picture: Self = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Img)
            .build();
        picture.imp().texture.replace(Some(texture.clone()));
        picture.imp().settings.set(settings.canonical());
        picture.imp().crop.set(crop);
        picture.set_can_target(false);
        picture.set_can_focus(false);
        picture.set_focusable(false);
        picture.set_overflow(gtk::Overflow::Hidden);
        picture
    }

    /// Repaint with new colour settings. This is intentionally only a
    /// queue-draw: dragging Brightness must not rebuild (or rescroll)
    /// a whole Long strip.
    pub fn set_image_settings(&self, settings: ComicImageSettings) {
        let settings = settings.canonical();
        if self.imp().settings.replace(settings) != settings {
            self.queue_draw();
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ComicPagePicture {
        pub(super) texture: RefCell<Option<gdk::Texture>>,
        pub(super) settings: Cell<ComicImageSettings>,
        pub(super) crop: Cell<CropRect>,
    }

    impl ComicPagePicture {
        fn cropped_size(&self) -> Option<(f32, f32)> {
            let texture = self.texture.borrow();
            let texture = texture.as_ref()?;
            let crop = self.crop.get();
            Some((
                texture.width().max(1) as f32 * crop.width,
                texture.height().max(1) as f32 * crop.height,
            ))
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ComicPagePicture {
        const NAME: &'static str = "KalamComicPagePicture";
        type Type = super::ComicPagePicture;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ComicPagePicture {}

    impl WidgetImpl for ComicPagePicture {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some((source_w, source_h)) = self.cropped_size() else {
                return (0, 0, -1, -1);
            };
            if source_w <= 0.0 || source_h <= 0.0 {
                return (0, 0, -1, -1);
            }

            // Like GtkPicture with can-shrink: one pixel is a legal
            // minimum, the cropped page's own aspect gives the
            // natural size on the axis being asked about.
            let natural = match orientation {
                gtk::Orientation::Horizontal => {
                    if for_size > 0 {
                        for_size as f32 * source_w / source_h
                    } else {
                        source_w
                    }
                }
                _ => {
                    if for_size > 0 {
                        for_size as f32 * source_h / source_w
                    } else {
                        source_h
                    }
                }
            };
            (1, natural.round().max(1.0) as i32, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            let widget = self.obj();
            let allocated_w = widget.width() as f32;
            let allocated_h = widget.height() as f32;
            let texture_w = texture.width().max(1) as f32;
            let texture_h = texture.height().max(1) as f32;
            let crop = self.crop.get();
            let cropped_w = texture_w * crop.width;
            let cropped_h = texture_h * crop.height;
            if allocated_w <= 0.0 || allocated_h <= 0.0 || cropped_w <= 0.0 || cropped_h <= 0.0 {
                return;
            }

            // Contain the cropped page in the allocation, centred.
            let scale = (allocated_w / cropped_w).min(allocated_h / cropped_h);
            let dest_w = cropped_w * scale;
            let dest_h = cropped_h * scale;
            let dest_x = (allocated_w - dest_w) / 2.0;
            let dest_y = (allocated_h - dest_h) / 2.0;
            let destination = gtk::graphene::Rect::new(dest_x, dest_y, dest_w, dest_h);

            // Place the whole texture at the same scale, shifted so
            // its crop rectangle coincides with the destination. The
            // clip then reveals exactly the analysed part of the page.
            let full_bounds = gtk::graphene::Rect::new(
                dest_x - texture_w * crop.x * scale,
                dest_y - texture_h * crop.y * scale,
                texture_w * scale,
                texture_h * scale,
            );

            snapshot.push_clip(&destination);
            let transform = color_transform(self.settings.get());
            let colour_pushed = !transform.is_identity();
            if colour_pushed {
                // GtkSnapshot applies transpose(matrix) to each
                // pixel. Our transform rows are already the output
                // channels, so hand GSK the transpose of that
                // row-major matrix.
                let matrix =
                    gtk::graphene::Matrix::from_float(transform.row_major_floats()).transpose();
                let offset = gtk::graphene::Vec4::new(
                    transform.offset[0],
                    transform.offset[1],
                    transform.offset[2],
                    transform.offset[3],
                );
                snapshot.push_color_matrix(&matrix, &offset);
            }
            snapshot.append_scaled_texture(
                &texture,
                gtk::gsk::ScalingFilter::Trilinear,
                &full_bounds,
            );
            if colour_pushed {
                snapshot.pop();
            }
            snapshot.pop();
        }
    }
}
