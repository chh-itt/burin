//! Shared glyph atlas for CPU text rendering.
//!
//! A fixed-size A8 texture (2048×2048, 4 MB) that stores monochrome
//! glyph masks.  Glyphs are rasterised once via Swash and packed into
//! the atlas with simple row-based allocation.  Rendering reads alpha
//! from the atlas and composites it onto the destination pixmap with
//! the desired text colour — no per-glyph `Pixmap` allocations needed.

use std::collections::HashMap;

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct AtlasKey {
    pub font_id: fontdb::ID,
    pub glyph_id: u16,
    pub font_size_bits: u32,
}

impl AtlasKey {
    pub fn from_cache_key(ck: cosmic_text::CacheKey, font_id: fontdb::ID) -> Self {
        Self {
            font_id,
            glyph_id: ck.glyph_id,
            font_size_bits: ck.font_size_bits,
        }
    }
}

#[derive(Clone, Copy)]
pub struct AtlasEntry {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Glyph left bearing (pixels, at the rasterised size).
    pub left: i32,
    /// Glyph top bearing (pixels).
    pub top: i32,
}

pub struct GlyphAtlas {
    /// A8 alpha values, row-major.  Each byte is the glyph coverage (0-255).
    alpha: Vec<u8>,
    width: u32,
    height: u32,
    /// Current allocation cursor.
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    entries: HashMap<AtlasKey, AtlasEntry>,
}

impl GlyphAtlas {
    /// Create a new atlas with the given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            alpha: vec![0u8; (width * height) as usize],
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            entries: HashMap::new(),
        }
    }

    /// Return the atlas entry for a glyph, rasterising it if necessary.
    ///
    /// The `rasterize` closure receives (cache_key, font_id) and must return
    /// the Swash image data plus placement for a *Mask* glyph.  The image
    /// data is 1 byte per pixel (alpha).
    pub fn get_or_insert(
        &mut self,
        key: AtlasKey,
        rasterize: impl FnOnce() -> Option<(Vec<u8>, u32, u32, i32, i32)>,
    ) -> Option<AtlasEntry> {
        if let Some(entry) = self.entries.get(&key) {
            return Some(*entry);
        }

        let (data, w, h, left, top) = rasterize()?;
        if w == 0 || h == 0 {
            return None;
        }

        let entry = self.allocate(w, h, left, top)?;

        // Copy glyph alpha into atlas
        for row in 0..h {
            let src_start = (row * w) as usize;
            let dst_start = ((entry.y + row) * self.width + entry.x) as usize;
            let len = w as usize;
            self.alpha[dst_start..dst_start + len]
                .copy_from_slice(&data[src_start..src_start + len]);
        }

        self.entries.insert(key, entry);
        Some(entry)
    }

    /// Allocate a rectangle in the atlas.  Returns None if the atlas is full
    /// (clears and retries once).
    fn allocate(&mut self, w: u32, h: u32, left: i32, top: i32) -> Option<AtlasEntry> {
        if w > self.width || h > self.height {
            return None; // Glyph too large for atlas
        }

        // Advance to next row if needed
        if self.cursor_x + w > self.width {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }

        // Out of space — clear and restart
        if self.cursor_y + h > self.height {
            self.clear();
        }

        let entry = AtlasEntry {
            x: self.cursor_x,
            y: self.cursor_y,
            width: w,
            height: h,
            left,
            top,
        };

        self.cursor_x += w;
        self.row_height = self.row_height.max(h);
        Some(entry)
    }

    /// Clear all atlas entries and reset the allocation cursor.
    pub fn clear(&mut self) {
        // No need to zero the buffer — it will be overwritten on re-insert.
        self.entries.clear();
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_height = 0;
    }

    /// Blit a glyph from the atlas onto a destination pixel buffer.
    ///
    /// `dst` is the frame-buffer as `[u32]` in the canonical format
    /// (tiny-skia premultiplied RGBA bytes = LE u32 `A<<24|B<<16|G<<8|R`),
    /// row-major.  `dst_w`, `dst_h` are the buffer dimensions.  `dx`, `dy`
    /// are the top-left destination coordinates.
    /// `color_rgba` is `[r, g, b, a]` where each component is 0-255.
    pub fn blit_to(
        &self,
        entry: &AtlasEntry,
        dx: i32,
        dy: i32,
        dst: &mut [u32],
        dst_w: u32,
        dst_h: u32,
        color_rgba: [u8; 4],
        clip: Option<(i32, i32, i32, i32)>,
    ) {
        let dst_w_i = dst_w as i32;
        let dst_h_i = dst_h as i32;
        if dx >= dst_w_i || dy >= dst_h_i {
            return;
        }

        let aw = entry.width as i32;
        let ah = entry.height as i32;
        let src_stride = self.width as usize;

        let mut clip_x1 = 0i32.max(-dx);
        let mut clip_y1 = 0i32.max(-dy);
        let mut clip_x2 = aw.min(dst_w_i - dx);
        let mut clip_y2 = ah.min(dst_h_i - dy);

        // Per-pixel rectangular clip in destination (device) pixels — the CPU
        // analogue of glyphon's `TextBounds`. A dst pixel for atlas column `c`
        // lands at `dx + c`, so the valid atlas-column range is shifted by `dx`.
        if let Some((cx0, cy0, cx1, cy1)) = clip {
            clip_x1 = clip_x1.max(cx0 - dx);
            clip_y1 = clip_y1.max(cy0 - dy);
            clip_x2 = clip_x2.min(cx1 - dx);
            clip_y2 = clip_y2.min(cy1 - dy);
        }

        if clip_x2 <= clip_x1 || clip_y2 <= clip_y1 {
            return;
        }

        let src_base = (entry.y as usize) * src_stride + (entry.x as usize);
        let ca = color_rgba[3] as u32;

        for row in clip_y1..clip_y2 {
            let src_start = src_base + (row as usize) * src_stride + (clip_x1 as usize);
            let len = (clip_x2 - clip_x1) as usize;
            let src_row = &self.alpha[src_start..src_start + len];
            let dst_base = ((dy + row) as u32 as usize) * (dst_w as usize);

            for (col, &alpha) in src_row.iter().enumerate() {
                let px = (dx + clip_x1 + col as i32) as u32 as usize;
                let di = dst_base + px;
                let dst_px = dst[di];

                let a = alpha as u32 * ca / 255;
                if a == 0 {
                    continue;
                }
                let inv_a = 255 - a;

                let sr = color_rgba[0] as u32 * a / 255;
                let sg = color_rgba[1] as u32 * a / 255;
                let sb = color_rgba[2] as u32 * a / 255;

                // Canonical RGBA-mem u32: R in bits 0-7, B in bits 16-23.
                let dr = dst_px & 0xFF;
                let dg = (dst_px >> 8) & 0xFF;
                let db = (dst_px >> 16) & 0xFF;
                let da = (dst_px >> 24) & 0xFF;

                let out_r = sr + (dr * inv_a / 255);
                let out_g = sg + (dg * inv_a / 255);
                let out_b = sb + (db * inv_a / 255);
                let out_a = a + (da * inv_a / 255);

                dst[di] = (out_a << 24) | (out_b << 16) | (out_g << 8) | out_r;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×4 opaque glyph blitted with a clip rect must only write destination
    /// pixels inside that rect — the per-pixel text clipping that mirrors
    /// glyphon's `TextBounds` on the GPU.
    #[test]
    fn blit_to_clips_per_pixel() {
        let mut atlas = GlyphAtlas::new(16, 16);
        // Paint a 4×4 fully-opaque glyph at atlas (0,0).
        for y in 0..4 {
            for x in 0..4 {
                atlas.alpha[y * 16 + x] = 255;
            }
        }
        let entry = AtlasEntry {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            left: 0,
            top: 0,
        };
        let white = [255u8, 255, 255, 255];

        // No clip: the whole 4×4 glyph lands at dst (2,2)..(6,6).
        let mut dst = vec![0u32; 16 * 16];
        atlas.blit_to(&entry, 2, 2, &mut dst, 16, 16, white, None);
        assert_ne!(
            dst[3 * 16 + 3],
            0,
            "unclipped glyph pixel should be written"
        );
        assert_eq!(dst[0], 0, "pixel outside the glyph should be untouched");

        // Clip to x>=4, y>=4: only the bottom-right of the glyph survives.
        let mut dst = vec![0u32; 16 * 16];
        atlas.blit_to(&entry, 2, 2, &mut dst, 16, 16, white, Some((4, 4, 16, 16)));
        assert_eq!(
            dst[2 * 16 + 2],
            0,
            "pixel outside the clip must NOT be written"
        );
        assert_eq!(
            dst[3 * 16 + 3],
            0,
            "pixel outside the clip must NOT be written"
        );
        assert_ne!(
            dst[4 * 16 + 4],
            0,
            "pixel inside the clip should be written"
        );
        assert_ne!(
            dst[5 * 16 + 5],
            0,
            "pixel inside the clip should be written"
        );
    }
}
