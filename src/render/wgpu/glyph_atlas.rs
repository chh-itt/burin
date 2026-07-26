//! Text: cosmic-text `Buffer::draw()` → CPU buffer → GPU texture → single quad.

use std::collections::HashMap;

use cosmic_text::{Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use crate::style::{Color, TextDirection};

pub(crate) struct TextImage { pub width: u32, pub height: u32, pub pixels: Vec<u8> }

pub(crate) struct CachedText { pub image: TextImage, pub logical_w: f32, pub logical_h: f32, pub offset_x: f32, pub offset_y: f32 }

/// Render text to a CPU pixel buffer via cosmic-text's own draw pipeline.
pub(crate) fn render_text_to_pixels(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    text: &str,
    font_size: f32, line_height: f32, max_width: f32,
    color: Color, scale_factor: f32, font_weight: u16, font_family: Option<String>,
    text_direction: Option<TextDirection>,
) -> Option<(TextImage, f32, f32, f32, f32)> {
    if text.is_empty() { return None; }

    let pf = font_size * scale_factor;
    let plh = line_height * scale_factor;
    let pmax_w = max_width * scale_factor;
    let metrics = Metrics::new(pf, plh);
    let mut buffer = Buffer::new(font_system, metrics);
    let cosmic_color = CosmicColor::rgb(
        (color.r * 255.0) as u8, (color.g * 255.0) as u8, (color.b * 255.0) as u8,
    );

    struct Span { x: i32, y: i32, w: u32, h: u32, r: u8, g: u8, b: u8, a: u8 }
    let mut spans: Vec<Span> = Vec::new();

    {
        let mut buf = buffer.borrow_with(font_system);
        buf.set_size(Some(pmax_w), Some(plh * 50.0));
        let mut attrs = Attrs::new().weight(Weight(font_weight));
        if let Some(ref family) = font_family {
            attrs = attrs.family(Family::Name(family));
        } else {
            // Fall back to system sans-serif for better script coverage (Arabic, CJK, etc.).
            attrs = attrs.family(Family::SansSerif);
        }
        buf.set_text(text, &attrs, Shaping::Advanced, None);
        buf.draw(swash_cache, cosmic_color, |x, y, pw, ph, c| {
            let a = (c.0 >> 24) as u8;
            if a == 0 { return; }
            spans.push(Span {
                x, y, w: pw, h: ph, a,
                r: ((c.0 >> 16) & 0xFF) as u8,
                g: ((c.0 >> 8) & 0xFF) as u8,
                b: (c.0 & 0xFF) as u8,
            });
        });
    }

    if spans.is_empty() {
        // Retry with Basic shaping — some scripts need simpler shaping.
        let mut buf2 = buffer.borrow_with(font_system);
        buf2.set_size(Some(pmax_w), Some(plh * 50.0));
        let mut attrs2 = Attrs::new().weight(Weight(font_weight));
        if let Some(ref family) = font_family {
            attrs2 = attrs2.family(Family::Name(family));
        } else {
            attrs2 = attrs2.family(Family::SansSerif);
        }
        buf2.set_text(text, &attrs2, Shaping::Basic, None);
        buf2.draw(swash_cache, cosmic_color, |x, y, pw, ph, c| {
            let a = (c.0 >> 24) as u8;
            if a == 0 { return; }
            spans.push(Span {
                x, y, w: pw, h: ph, a,
                r: ((c.0 >> 16) & 0xFF) as u8,
                g: ((c.0 >> 8) & 0xFF) as u8,
                b: (c.0 & 0xFF) as u8,
            });
        });
    }

    if spans.is_empty() {
        return None;
    }

    // RTL: explicit text_direction or span heuristic (spans far right in buffer).
    let is_rtl = text_direction == Some(TextDirection::Rtl)
        || (spans.first().map(|s| s.x).unwrap_or(0) > 0
            && spans.last().map(|s| s.x).unwrap_or(0) < spans.first().map(|s| s.x).unwrap_or(0) / 2);

    // bounding box offset and shift all spans so they start at (0,0).
    let min_x = spans.iter().map(|s| s.x).min().unwrap_or(0);
    let min_y = spans.iter().map(|s| s.y).min().unwrap_or(0);
    let shift_x = -min_x.max(0);
    let shift_y = -min_y.max(0);

    let max_x = spans.iter().map(|s| s.x + s.w as i32).max().unwrap_or(0);
    let max_y = spans.iter().map(|s| s.y + s.h as i32).max().unwrap_or(0);
    let content_w = (max_x + shift_x).max(1) as u32;
    let content_h = (max_y + shift_y).max(1) as u32;

    let bw = (content_w + 4).max(64).min(4096);
    let bh = (content_h + 4).max(32).min(4096);

    let mut pixels = vec![0u8; (bw * bh * 4) as usize];
    let mut content_max_x = 0u32;
    let mut content_max_y = 0u32;

    for s in &spans {
        let sx = (s.x + shift_x) as u32;
        let sy = (s.y + shift_y) as u32;
        let pw = s.w.min(bw.saturating_sub(sx));
        let ph = s.h.min(bh.saturating_sub(sy));
        if pw == 0 || ph == 0 { continue; }
        content_max_x = content_max_x.max(sx + pw);
        content_max_y = content_max_y.max(sy + ph);
        for dy in 0..ph { for dx in 0..pw {
            let idx = ((sy+dy) as usize * bw as usize + (sx+dx) as usize) * 4;
            let af = s.a as f32 / 255.0;
            pixels[idx]   = (s.r as f32 * af) as u8;
            pixels[idx+1] = (s.g as f32 * af) as u8;
            pixels[idx+2] = (s.b as f32 * af) as u8;
            pixels[idx+3] = s.a;
        }}
    }

    // Crop to content + small pad for descenders / subpixel overflow.
    let cw = (content_max_x + 2).min(bw).max(1);
    let ch = (content_max_y + 2).min(bh).max(1);
    let mut cropped = vec![0u8; (cw * ch * 4) as usize];
    for row in 0..ch {
        let src = (row as usize * bw as usize) * 4;
        let dst = (row as usize * cw as usize) * 4;
        cropped[dst..dst + cw as usize * 4]
            .copy_from_slice(&pixels[src..src + cw as usize * 4]);
    }

    let lw = cw as f32 / scale_factor;
    let lh = ch as f32 / scale_factor;
    // RTL: spans originated far right or element.text_direction=Rtl.
    // Ox = -lw shifts the quad left so its right edge aligns with position.x.
    // Caller (paint_element_tree) sets position.x to the right edge for RTL.
    let ox = if is_rtl { -(lw) } else { 0.0f32 };
    let oy = min_y as f32 / scale_factor;
    Some((TextImage { width: cw, height: ch, pixels: cropped }, lw, lh, ox, oy))
}

pub(crate) struct TextCache { entries: HashMap<u64, CachedText> }

impl TextCache {
    pub fn new() -> Self { Self { entries: HashMap::new() } }
    pub fn get_or_render(
        &mut self, font_system: &mut FontSystem, swash_cache: &mut SwashCache,
        text: &str, font_size: f32, line_height: f32, max_width: f32,
        color: Color, scale_factor: f32, font_weight: u16, font_family: Option<String>,
        text_direction: Option<TextDirection>,
    ) -> Option<&CachedText> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        text.hash(&mut h); font_size.to_bits().hash(&mut h);
        scale_factor.to_bits().hash(&mut h); font_weight.hash(&mut h);
        font_family.hash(&mut h);
        let hash = h.finish();
        if !self.entries.contains_key(&hash) {
            let (image, lw, lh, ox, oy) = render_text_to_pixels(
                font_system, swash_cache, text, font_size, line_height,
                max_width, color, scale_factor, font_weight, font_family, text_direction,
            )?;
            self.entries.insert(hash, CachedText { image, logical_w: lw, logical_h: lh, offset_x: ox, offset_y: oy });
        }
        self.entries.get(&hash)
    }
    pub fn clear(&mut self) { self.entries.clear(); }
}

/// Measure the exact pixel x-position of the cursor after the given character index.
pub(crate) fn measure_cursor_x(
    font_system: &mut FontSystem,
    text: &str,
    char_index: usize,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    font_family: Option<&str>,
) -> f32 {
    if text.is_empty() || char_index == 0 { return 0.0; }
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    let mut attrs = Attrs::new().weight(Weight(font_weight));
    if let Some(f) = font_family { attrs = attrs.family(Family::Name(f)); }
    {
        let mut buf = buffer.borrow_with(font_system);
        buf.set_size(Some(9999.0), Some(font_size * 2.0));
        buf.set_text(text, &attrs, Shaping::Advanced, None);
        let byte_idx = char_index_to_byte(text, char_index.min(text.chars().count()));
        let mut max_x: f32 = 0.0;
        for run in buf.layout_runs() {
            for glyph in run.glyphs {
                if glyph.start >= 0 && (glyph.start as usize) < byte_idx {
                    let gx = glyph.x as f32 + glyph.w as f32;
                    if gx > max_x { max_x = gx; }
                }
            }
        }
        max_x
    }
}

/// Find the character index closest to a given pixel x-position.
pub(crate) fn measure_char_at_x(
    font_system: &mut FontSystem,
    text: &str,
    x_pos: f32,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    font_family: Option<&str>,
) -> usize {
    if text.is_empty() { return 0; }
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    let mut attrs = Attrs::new().weight(Weight(font_weight));
    if let Some(f) = font_family { attrs = attrs.family(Family::Name(f)); }
    {
        let mut buf = buffer.borrow_with(font_system);
        buf.set_size(Some(9999.0), Some(font_size * 2.0));
        buf.set_text(text, &attrs, Shaping::Advanced, None);
        let char_count = text.chars().count();
        let mut best_idx = 0usize;
        let mut best_dist = f32::MAX;
        let mut found = false;
        for run in buf.layout_runs() {
            for glyph in run.glyphs {
                if glyph.start < 0 { continue; }
                found = true;
                let gx = glyph.x as f32;
                let gw = glyph.w as f32;
                let mid = gx + gw * 0.5;
                let dist = (x_pos - mid).abs();
                if dist < best_dist {
                    best_dist = dist;
                    let byte_idx = glyph.start as usize;
                    best_idx = byte_to_char_index(text, byte_idx).min(char_count);
                }
            }
        }
        if !found { return if x_pos > 0.0 { char_count } else { 0 }; }
        let total_w = measure_cursor_x(font_system, text, char_count, font_size, line_height, font_weight, font_family);
        if x_pos > total_w { return char_count; }
        best_idx
    }
}

fn char_index_to_byte(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(i, _)| i).unwrap_or(s.len())
}

fn byte_to_char_index(s: &str, bi: usize) -> usize {
    s.char_indices().take_while(|&(i, _)| i < bi).count()
}

/// Lightweight text measurer using its own FontSystem.
pub(crate) struct TextMeasurer {
    font_system: FontSystem,
    font_system_ok: bool,
}

impl TextMeasurer {
    pub fn new() -> Self {
        let (font_system, font_system_ok) = match std::panic::catch_unwind(|| FontSystem::new()) {
            Ok(fs) => (fs, true),
            Err(p) => {
                let msg = crate::core::error::panic_to_string(&p);
                crate::core::error::push_error(crate::core::error::UiError::FontLoad(
                    format!("TextMeasurer FontSystem::new panicked: {msg}"),
                ));
                // Fallback: empty database — no system font scanning.
                let fallback = std::panic::catch_unwind(|| {
                    FontSystem::new_with_locale_and_db("en".into(), fontdb::Database::new())
                });
                match fallback {
                    Ok(fs) => (fs, true),
                    Err(_) => {
                        // Deep fallback: catch_unwind around new_with_locale_and_db with a
                        // freshly rebuilt database (should not reach here in practice).
                        let msg2 = crate::core::error::panic_to_string(&p);
                        crate::core::error::push_error(crate::core::error::UiError::FontLoad(
                            format!("TextMeasurer fallback FontSystem also panicked: {msg2}"),
                        ));
                        (FontSystem::new_with_fonts(std::iter::empty()), false)
                    }
                }
            }
        };
        Self { font_system, font_system_ok }
    }

    pub fn cursor_x_at(&mut self, text: &str, char_index: usize, font_size: f32, line_height: f32, font_weight: u16, font_family: Option<&str>) -> f32 {
        if !self.font_system_ok { return 0.0; }
        measure_cursor_x(&mut self.font_system, text, char_index, font_size, line_height, font_weight, font_family)
    }

    pub fn char_at_x(&mut self, text: &str, x_pos: f32, font_size: f32, line_height: f32, font_weight: u16, font_family: Option<&str>) -> usize {
        if !self.font_system_ok { return 0; }
        measure_char_at_x(&mut self.font_system, text, x_pos, font_size, line_height, font_weight, font_family)
    }
}

impl Default for TextMeasurer {
    fn default() -> Self { Self::new() }
}
