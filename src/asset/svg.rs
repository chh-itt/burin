use std::cell::RefCell;

/// A parsed SVG asset that can be rasterized on demand.
pub struct SvgAsset {
    tree: usvg::Tree,
    default_width: f32,
    default_height: f32,
    raster_cache: RefCell<Option<(u32, u32, Vec<u8>)>>,
}

impl SvgAsset {
    /// Parse an SVG from raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_data(bytes, &opt).map_err(|e| e.to_string())?;
        let sz = tree.size();
        Ok(Self {
            tree,
            default_width: sz.width(),
            default_height: sz.height(),
            raster_cache: RefCell::new(None),
        })
    }

    /// Return the SVG's intrinsic size (from `width`/`height`/`viewBox`).
    pub fn intrinsic_size(&self) -> (f32, f32) {
        (self.default_width, self.default_height)
    }

    /// Rasterize the SVG to RGBA8 pixels at the given dimensions.
    /// Results are cached per-size; re-rasterizes only when size changes.
    pub fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
        // Check cache
        {
            let cache = self.raster_cache.borrow();
            if let Some((cw, ch, ref pixels)) = *cache {
                if cw == width && ch == height {
                    return pixels.clone();
                }
            }
        }

        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .unwrap_or_else(|| tiny_skia::Pixmap::new(1, 1).unwrap());

        let sx = width as f32 / self.default_width.max(1.0);
        let sy = height as f32 / self.default_height.max(1.0);
        let scale = sx.min(sy);
        let tx = (width as f32 - self.default_width * scale) * 0.5;
        let ty = (height as f32 - self.default_height * scale) * 0.5;
        let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);

        resvg::render(&self.tree, transform, &mut pixmap.as_mut());

        let result = pixmap.data().to_vec();
        *self.raster_cache.borrow_mut() = Some((width, height, result.clone()));
        result
    }
}
