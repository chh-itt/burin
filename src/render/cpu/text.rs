//! CPU text rendering helpers.
//!
//! These functions are available for use by the CPU renderer
//! when it needs per-glyph measurement or rasterization outside
//! the cosmic-text direct path.

use super::glyph_cache::{GlyphCache, GlyphMetrics, CachedGlyph};

#[allow(dead_code)]
pub fn measure_glyph_metrics(
    _font_db: &fontdb::Database,
    font_id: fontdb::ID,
    glyph_id: u16,
    font_size: f32,
    glyph_cache: &mut GlyphCache,
) -> Option<GlyphMetrics> {
    glyph_cache.measure(font_id, glyph_id, font_size, || {
        None
    })
}

#[allow(dead_code)]
pub fn rasterize_glyph(
    _font_db: &fontdb::Database,
    font_id: fontdb::ID,
    glyph_id: u16,
    font_size: f32,
    _subpixel_bin: u8,
    color_rgb: [u8; 3],
    swash_cache: &mut cosmic_text::SwashCache,
    font_system: &mut cosmic_text::FontSystem,
    glyph_cache: &mut GlyphCache,
) -> Option<CachedGlyph> {
    glyph_cache.rasterize(font_id, glyph_id, font_size, color_rgb, || {
        let (cache_key, _, _) = cosmic_text::CacheKey::new(
            font_id,
            glyph_id,
            font_size,
            (0.0_f32, 0.0_f32),
            fontdb::Weight::NORMAL,
            cosmic_text::CacheKeyFlags::empty(),
        );

        let image = swash_cache.get_image_uncached(font_system, cache_key)?;

        let gw = image.placement.width as u32;
        let gh = image.placement.height as u32;
        if gw == 0 || gh == 0 {
            return None;
        }

        let mut pixmap = tiny_skia::Pixmap::new(gw, gh)?;
        let dst = bytemuck::cast_slice_mut::<u8, u32>(pixmap.data_mut());

        match image.content {
            cosmic_text::SwashContent::Mask => {
                for i in 0..(gw * gh) as usize {
                    let a = image.data[i] as u32;
                    let r = (color_rgb[0] as u32 * a / 255) as u32;
                    let g = (color_rgb[1] as u32 * a / 255) as u32;
                    let b = (color_rgb[2] as u32 * a / 255) as u32;
                    dst[i] = (a << 24) | (r << 16) | (g << 8) | b;
                }
            }
            cosmic_text::SwashContent::Color => {
                let mut si = 0;
                for di in 0..(gw * gh) as usize {
                    let cb = image.data[si];
                    let cg = image.data[si + 1];
                    let cr = image.data[si + 2];
                    let ca = image.data[si + 3];
                    let tr = (cr as u32 * color_rgb[0] as u32 / 255) as u32;
                    let tg = (cg as u32 * color_rgb[1] as u32 / 255) as u32;
                    let tb = (cb as u32 * color_rgb[2] as u32 / 255) as u32;
                    dst[di] = (ca as u32) << 24 | tr << 16 | tg << 8 | tb;
                    si += 4;
                }
            }
            cosmic_text::SwashContent::SubpixelMask => {}
        }

        Some(CachedGlyph {
            pixmap: std::rc::Rc::new(pixmap),
            left: image.placement.left,
            top: image.placement.top,
            color_rgb,
        })
    })
}
