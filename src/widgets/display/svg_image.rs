use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::widgets::display::{ContentFit, ImageData};
use std::rc::Rc;

pub struct SvgImage {
    asset_id: crate::asset::AssetId,
    /// RAII reference covering construct→mount (audit 2026-07-18):
    /// `from_bytes` assets are element-owned — freed when the last
    /// referencing element (or an unmounted widget) dies.  `new()` takes
    /// an externally managed (pinned) id and carries no guard.
    _guard: Option<crate::asset::AssetGuard>,
    raster_w: u32,
    raster_h: u32,
    intrinsic_w: f32,
    intrinsic_h: f32,
    fit: ContentFit,
    style: StyleRefinement,
}

impl SvgImage {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let (asset_id, guard) = crate::asset::load_svg_owned(bytes)?;
        let svg = crate::asset::svg_asset(asset_id).ok_or("SVG asset not found")?;
        let (w, h) = svg.intrinsic_size();
        Ok(Self {
            asset_id,
            _guard: Some(guard),
            raster_w: w.ceil() as u32,
            raster_h: h.ceil() as u32,
            intrinsic_w: w,
            intrinsic_h: h,
            fit: ContentFit::Contain,
            style: StyleRefinement::default(),
        })
    }

    pub fn new(asset_id: crate::asset::AssetId, width: u32, height: u32) -> Self {
        Self {
            asset_id,
            _guard: None,
            raster_w: width.max(1),
            raster_h: height.max(1),
            intrinsic_w: width as f32,
            intrinsic_h: height as f32,
            fit: ContentFit::Contain,
            style: StyleRefinement::default(),
        }
    }

    pub fn size(mut self, w: u32, h: u32) -> Self {
        self.raster_w = w.max(1);
        self.raster_h = h.max(1);
        self
    }

    pub fn fit(mut self, f: ContentFit) -> Self {
        self.fit = f;
        self
    }
}

impl Styled for SvgImage {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for SvgImage {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let (raster_w, raster_h, pixels) =
            crate::asset::rasterize_svg(self.asset_id, self.raster_w, self.raster_h)
                .expect("SVG rasterization failed");

        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        // Hand asset ownership to the element BEFORE the widget (and its
        // AssetGuard) is dropped at the end of this call.
        crate::asset::retain_asset_for(id, self.asset_id);
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::Image);
            element.set_affected_by_child_size(false);

            element.set_flex_grow(0.0);
            element.set_flex_shrink(0.0);

            use crate::style::Dimension;
            let ar = self.intrinsic_w / self.intrinsic_h.max(1.0);
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
                    element.set_preferred_width(Some(self.raster_w as f32));
                    element.set_preferred_height(self.raster_h as f32);
                    if let Some(Dimension::Percent(_)) = self.style.width {
                        element.set_width_dim(self.style.width);
                    }
                    if let Some(Dimension::Percent(_)) = self.style.height {
                        element.set_height_dim(self.style.height.unwrap());
                    }
                }
            }

            let hash = image_hash(raster_w, raster_h, &pixels);
            let pixels_rc = Rc::new(pixels);

            #[cfg(feature = "backend-wgpu")]
            crate::render::wgpu::register_image_for(
                id,
                hash,
                raster_w,
                raster_h,
                pixels_rc.clone(),
            );

            element.insert_user_data(ImageData {
                pixels: pixels_rc,
                width: raster_w,
                height: raster_h,
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

impl std::fmt::Debug for SvgImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgImage")
            .field("raster_w", &self.raster_w)
            .field("raster_h", &self.raster_h)
            .field("fit", &self.fit)
            .finish_non_exhaustive()
    }
}

fn image_hash(width: u32, height: u32, pixels: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    width.hash(&mut h);
    height.hash(&mut h);
    for &b in &pixels[..64.min(pixels.len())] {
        b.hash(&mut h);
    }
    h.finish()
}
