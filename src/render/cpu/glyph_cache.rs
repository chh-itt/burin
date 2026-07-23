use std::collections::HashMap;
use std::rc::Rc;

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct WidthKey {
    pub font_id: fontdb::ID,
    pub glyph_id: u16,
    pub size_bits: u16,
}

impl WidthKey {
    pub fn new(font_id: fontdb::ID, glyph_id: u16, size_bits: u16) -> Self {
        Self {
            font_id,
            glyph_id,
            size_bits,
        }
    }
}

#[derive(Clone)]
pub struct GlyphMetrics {
    pub advance_x: f32,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct BitmapKey {
    pub font_id: fontdb::ID,
    pub glyph_id: u16,
    pub size_bits: u16,
    pub color_rgb: [u8; 3],
}

impl BitmapKey {
    pub fn new(font_id: fontdb::ID, glyph_id: u16, size_bits: u16, color_rgb: [u8; 3]) -> Self {
        Self {
            font_id,
            glyph_id,
            size_bits,
            color_rgb,
        }
    }
}

pub struct CachedGlyph {
    pub pixmap: Rc<tiny_skia::Pixmap>,
    pub left: i32,
    pub top: i32,
    pub color_rgb: [u8; 3],
}

pub struct GlyphCache {
    width_cache: HashMap<WidthKey, GlyphMetrics>,
    bitmap_cache: HashMap<BitmapKey, CachedGlyph>,
    bitmap_bytes: u64,
}

impl GlyphCache {
    const MAX_BITMAP_BYTES: u64 = 4 * 1024 * 1024;
    #[allow(dead_code)]
    const TRIM_INTERVAL: u64 = 300;

    pub fn new() -> Self {
        Self {
            width_cache: HashMap::new(),
            bitmap_cache: HashMap::new(),
            bitmap_bytes: 0,
        }
    }

    pub fn measure(
        &mut self,
        font_id: fontdb::ID,
        glyph_id: u16,
        font_size: f32,
        compute: impl FnOnce() -> Option<GlyphMetrics>,
    ) -> Option<GlyphMetrics> {
        let key = WidthKey::new(font_id, glyph_id, (font_size * 10.0) as u16);
        if let Some(m) = self.width_cache.get(&key) {
            return Some(m.clone());
        }
        let m = compute()?;
        self.width_cache.insert(key, m.clone());
        Some(m)
    }

    pub fn rasterize(
        &mut self,
        font_id: fontdb::ID,
        glyph_id: u16,
        font_size: f32,
        color_rgb: [u8; 3],
        render: impl FnOnce() -> Option<CachedGlyph>,
    ) -> Option<CachedGlyph> {
        let key = BitmapKey::new(font_id, glyph_id, (font_size * 10.0) as u16, color_rgb);

        if let Some(g) = self.bitmap_cache.get(&key) {
            return Some(CachedGlyph {
                pixmap: Rc::clone(&g.pixmap),
                left: g.left,
                top: g.top,
                color_rgb: g.color_rgb,
            });
        }

        let g = render()?;
        let byte_count = (g.pixmap.width() * g.pixmap.height() * 4) as u64;

        if self.bitmap_bytes + byte_count > Self::MAX_BITMAP_BYTES {
            self.purge_half();
        }

        self.bitmap_bytes += byte_count;
        let result = CachedGlyph {
            pixmap: Rc::clone(&g.pixmap),
            left: g.left,
            top: g.top,
            color_rgb: g.color_rgb,
        };
        self.bitmap_cache.insert(key, g);
        Some(result)
    }

    fn purge_half(&mut self) {
        let target = self.bitmap_cache.len() / 2;
        let keys: Vec<BitmapKey> = self.bitmap_cache.keys().take(target).copied().collect();
        for k in keys {
            self.bitmap_cache.remove(&k);
        }
    }

    pub fn trim(&mut self, _frame_count: u64) {
        if self.bitmap_bytes > Self::MAX_BITMAP_BYTES {
            self.purge_half();
        }
    }

    pub fn clear_bitmaps(&mut self) {
        self.bitmap_cache.clear();
        self.bitmap_bytes = 0;
    }
}
