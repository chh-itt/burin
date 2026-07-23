use std::rc::Rc;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};

pub struct Image {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub fit: ContentFit,
    pub style: StyleRefinement,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum ContentFit {
    #[default]
    Contain,
    Fill,
    Cover,
    None,
    ScaleDown,
}

impl Image {
    #[cfg(feature = "ext-image")]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let img = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| format!("format detect: {}", e))?
            .decode()
            .map_err(|e| format!("decode: {}", e))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(Self {
            pixels: rgba.into_raw(),
            width: w,
            height: h,
            fit: ContentFit::Contain,
            style: StyleRefinement::default(),
        })
    }

    #[cfg(not(feature = "ext-image"))]
    pub fn from_bytes(_bytes: &[u8]) -> Result<Self, String> {
        Err("ext-image feature not enabled".into())
    }

    pub fn fit(mut self, f: ContentFit) -> Self {
        self.fit = f;
        self
    }

    pub fn from_rgba(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
            fit: ContentFit::Contain,
            style: StyleRefinement::default(),
        }
    }

    fn image_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.width.hash(&mut h);
        self.height.hash(&mut h);
        for &b in &self.pixels[..64.min(self.pixels.len())] {
            b.hash(&mut h);
        }
        h.finish()
    }
}

impl Styled for Image {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Image {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::Image);
            element.set_affected_by_child_size(false);

            // Pin flex so taffy never shrinks/stretches the explicit size.
            element.set_flex_grow(0.0);
            element.set_flex_shrink(0.0);

            // Compute explicit width+height from aspect ratio so taffy's
            // stretch alignment never overrides the natural proportions.
            use crate::style::Dimension;
            let ar = self.width as f32 / self.height as f32;
            element.set_aspect_ratio(Some(ar));
            match (self.style.width, self.style.height) {
                (Some(Dimension::Pixels(w)), Some(Dimension::Pixels(h))) => {
                    element.set_preferred_width(Some(w));
                    element.set_preferred_height(h);
                }
                (Some(Dimension::Pixels(w)), _) => {
                    element.set_preferred_width(Some(w));
                    element.set_preferred_height(w / ar);
                    if let Some(Dimension::Percent(_)) = self.style.height {
                        element.set_height_dim(self.style.height.unwrap());
                    }
                }
                (_, Some(Dimension::Pixels(h))) => {
                    element.set_preferred_height(h);
                    element.set_preferred_width(Some(h * ar));
                    if let Some(Dimension::Percent(_)) = self.style.width {
                        element.set_width_dim(self.style.width);
                    }
                }
                _ => {
                    element.set_preferred_width(Some(self.width as f32));
                    element.set_preferred_height(self.height as f32);
                    if let Some(Dimension::Percent(_)) = self.style.width {
                        element.set_width_dim(self.style.width);
                    }
                    if let Some(Dimension::Percent(_)) = self.style.height {
                        element.set_height_dim(self.style.height.unwrap());
                    }
                }
            }

            let hash = self.image_hash();
            let pixels = Rc::new(self.pixels);
            crate::render::wgpu::register_image_for(
                id,
                hash,
                self.width,
                self.height,
                pixels.clone(),
            );

            element.insert_user_data(ImageData {
                pixels,
                width: self.width,
                height: self.height,
                hash,
                fit: self.fit,
            });

            if let Some(o) = self.style.opacity {
                element.set_opacity(o);
            }
            if let Some(bg) = self.style.background {
                element.set_background(bg);
            }
            if let Some(cr) = self.style.corner_radius {
                element.set_corner_radii(cr);
            }
            if let Some(zi) = self.style.z_index {
                element.set_z_index(zi);
            }
            if let Some(tx) = self.style.transform {
                element.set_transform(Some(tx));
            }
        }
        id
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("fit", &self.fit)
            .finish_non_exhaustive()
    }
}

pub struct ImageData {
    pub pixels: Rc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub hash: u64,
    pub fit: ContentFit,
}
