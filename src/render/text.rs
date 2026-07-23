//! Text utilities: glyph position measurement from shaped Buffers.

use crate::render::wgpu::glyphon_bridge::{ensure_font_system, FONT_SYSTEM};
use cosmic_text::{Attrs, Buffer, Cursor, Family, Metrics, Shaping, Weight};

/// Convert a whole-text character index into a cosmic-text `Cursor`
/// (`{ line, index }` where `line` is the logical line and `index` is the
/// **byte** offset within that line). Lines are split on `\n`. Empty lines are
/// representable (index 0 on that line), which the glyph-counting model could
/// not express — this is what makes empty lines reachable by the cursor.
pub fn char_index_to_cursor(text: &str, char_index: usize) -> Cursor {
    let mut line = 0usize;
    let mut byte_in_line = 0usize;
    let mut chars_seen = 0usize;
    for ch in text.chars() {
        if chars_seen == char_index {
            return Cursor::new(line, byte_in_line);
        }
        if ch == '\n' {
            line += 1;
            byte_in_line = 0;
        } else {
            byte_in_line += ch.len_utf8();
        }
        chars_seen += 1;
    }
    // char_index at or beyond end of text → end position.
    Cursor::new(line, byte_in_line)
}

/// Inverse of [`char_index_to_cursor`]: map a cosmic-text `Cursor`
/// (`line` + byte offset within line) back to a whole-text character index.
pub fn cursor_to_char_index(text: &str, cursor: Cursor) -> usize {
    let mut line = 0usize;
    let mut byte_in_line = 0usize;
    let mut chars_seen = 0usize;
    for ch in text.chars() {
        if line == cursor.line && byte_in_line == cursor.index {
            return chars_seen;
        }
        if ch == '\n' {
            // If the target is at/after end of this line (byte index past the
            // line's content), resolve to the newline position before advancing.
            if line == cursor.line && byte_in_line < cursor.index {
                return chars_seen;
            }
            line += 1;
            byte_in_line = 0;
        } else {
            byte_in_line += ch.len_utf8();
        }
        chars_seen += 1;
    }
    chars_seen
}

/// A shaped glyph bounding box.
#[derive(Clone, Debug)]
pub struct TextGlyph {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Shaped glyph bounding boxes from cosmic-text layout runs.
pub fn shape_text(
    text: &str,
    font_size: f32,
    line_height: f32,
    max_width: Option<f32>,
) -> Vec<TextGlyph> {
    if !ensure_font_system() {
        return Vec::new();
    }
    FONT_SYSTEM.with(|fs_cell| {
        let mut guard = fs_cell.borrow_mut();
        let fs = guard.as_mut().unwrap();
        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(fs, metrics);
        let mut buf = buffer.borrow_with(fs);
        buf.set_size(max_width, Some(line_height * 50.0));
        buf.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        let mut glyphs = Vec::new();
        for run in buf.layout_runs() {
            for glyph in run.glyphs.iter() {
                glyphs.push(TextGlyph {
                    x: glyph.x,
                    y: glyph.y,
                    w: glyph.w.max(1.0),
                    h: run.line_height,
                });
            }
        }
        glyphs
    })
}

/// Lightweight text width measurement via cosmic-text shaping.
pub fn measure_text_width(
    text: &str,
    font_size: f32,
    font_weight: u16,
    font_family: Option<String>,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    if !ensure_font_system() {
        return font_size + 2.0;
    }
    FONT_SYSTEM.with(|fs_cell| {
        let mut guard = fs_cell.borrow_mut();
        let fs = guard.as_mut().unwrap();
        let metrics = Metrics::new(font_size, font_size * 1.5);
        let mut buffer = Buffer::new(fs, metrics);
        let mut buf = buffer.borrow_with(fs);
        buf.set_size(None, Some(font_size * 100.0));
        let mut attrs = Attrs::new().weight(Weight(font_weight));
        if let Some(ref f) = font_family {
            attrs = attrs.family(Family::Name(f));
        } else {
            attrs = attrs.family(Family::SansSerif);
        }
        buf.set_text(text, &attrs, Shaping::Advanced, None);
        drop(buf); // release &mut fs for shape_until_scroll
        buffer.shape_until_scroll(fs, false); // Advanced shaping completes here
        let mut buf = buffer.borrow_with(fs);
        let (mut min_x, mut max_x) = (f32::MAX, 0.0f32);
        for run in buf.layout_runs() {
            for glyph in run.glyphs.iter() {
                min_x = min_x.min(glyph.x);
                max_x = max_x.max(glyph.x + glyph.w);
            }
        }
        if min_x < max_x && max_x > 0.0 {
            (max_x - min_x) + 2.0
        } else {
            font_size + 2.0
        }
    })
}

/// Find visual row and pixel x for a given expanded character index,
/// by sequentially counting glyphs across all layout runs.
/// `at_newline` should be true when the raw cursor is just past a \n
/// (cursor should appear at the start of the next glyph, not its end).
/// Returns (visual_row_ordinal, pixel_x_position).
pub fn glyph_pos_at_ci(
    buffer: &cosmic_text::Buffer,
    exp_ci: usize,
    at_newline: bool,
) -> (usize, f32) {
    if exp_ci == 0 {
        for (i, run) in buffer.layout_runs().enumerate() {
            if let Some(g) = run.glyphs.first() {
                return (i, g.x);
            }
        }
        return (0, 0.0);
    }
    let mut glyph_idx = 0usize;
    for (i, run) in buffer.layout_runs().enumerate() {
        for g in run.glyphs.iter() {
            if glyph_idx + 1 >= exp_ci {
                if at_newline && glyph_idx + 1 == exp_ci {
                    // Jump to the start of the next visual line.
                    for (j, nr) in buffer.layout_runs().enumerate().skip(i + 1) {
                        if let Some(ng) = nr.glyphs.first() {
                            return (j, ng.x);
                        }
                    }
                    // No more layout runs — cursor at start of last empty line.
                    let last_row = buffer.layout_runs().count().saturating_sub(1);
                    return (last_row, 0.0);
                }
                return (i, g.x + g.w);
            }
            glyph_idx += 1;
        }
    }
    // ci beyond all glyphs (e.g. after \n at end-of-text with empty trailing line)
    let last_row = buffer.layout_runs().count().saturating_sub(1);
    (last_row, 0.0)
}

/// Find visual row for a given expanded character index.
pub fn visual_row_at_exp_ci(buffer: &cosmic_text::Buffer, exp_ci: usize) -> usize {
    if exp_ci == 0 {
        return 0;
    }
    let total_glyphs: usize = buffer.layout_runs().map(|r| r.glyphs.len()).sum();
    if exp_ci > total_glyphs {
        // Beyond all glyphs — use last visual row (empty trailing line etc.)
        return buffer.layout_runs().count().saturating_sub(1);
    }
    let mut glyph_idx = 0usize;
    for (i, run) in buffer.layout_runs().enumerate() {
        for _g in run.glyphs.iter() {
            if glyph_idx + 1 >= exp_ci {
                return i;
            }
            glyph_idx += 1;
        }
    }
    buffer.layout_runs().count().saturating_sub(1)
}

/// Get pixel x for cursor at `ci` characters (legacy wrapper — delegates to glyph_pos_at_ci).
pub fn glyph_x_at(buffer: &cosmic_text::Buffer, ci: usize) -> f32 {
    glyph_pos_at_ci(buffer, ci, false).1
}

/// Total number of visual rows in the buffer.
pub fn visual_row_count(buffer: &cosmic_text::Buffer) -> usize {
    buffer.layout_runs().count()
}

/// Find the visual row that contains a given pixel Y offset.
pub fn visual_row_from_y(buffer: &cosmic_text::Buffer, y: f32) -> usize {
    let mut acc = 0.0f32;
    for (i, run) in buffer.layout_runs().enumerate() {
        acc += run.line_height;
        if y < acc {
            return i;
        }
    }
    buffer.layout_runs().count().saturating_sub(1)
}

/// Map (visual_row, pixel_x) → expanded character index, by scanning glyphs
/// in the visual row and counting their ordinal positions.
pub fn ci_at_visual_x(buffer: &cosmic_text::Buffer, visual_row: usize, target_x: f32) -> usize {
    let mut glyph_idx = 0usize;
    for (i, run) in buffer.layout_runs().enumerate() {
        if i < visual_row {
            glyph_idx += run.glyphs.len();
            continue;
        }
        if i > visual_row {
            continue;
        }
        for g in run.glyphs.iter() {
            if target_x < g.x + g.w * 0.5 {
                return glyph_idx;
            }
            glyph_idx += 1;
        }
        // beyond last glyph in row → end of row
        return glyph_idx;
    }
    0
}

/// Move cursor up/down by one visual row, trying to preserve pixel x.
/// Returns (new_visual_row, new_exp_ci).
pub fn move_visual_row(
    buffer: &cosmic_text::Buffer,
    cur_row: usize,
    _cur_ci: usize,
    cur_x: f32,
    delta: isize,
) -> (usize, usize) {
    let total = visual_row_count(buffer);
    if total == 0 {
        return (0, 0);
    }
    let new_row = ((cur_row as isize + delta).max(0) as usize).min(total - 1);
    if new_row == cur_row {
        return (cur_row, _cur_ci);
    }
    let new_ci = ci_at_visual_x(buffer, new_row, cur_x);
    (new_row, new_ci)
}
pub fn glyph_char_x(buffer: &cosmic_text::Buffer, target_x: f32) -> usize {
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            if target_x < g.x + g.w * 0.5 {
                return g.start;
            }
        }
    }
    let mut last = 0;
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            last = g.end;
        }
    }
    last
}

/// Map a raw-text character index to expanded-text index
/// (tabs → 4 spaces; \n → 0 since it has no glyph).
pub fn raw_to_expanded(text: &str, ci: usize) -> usize {
    let mut exp = 0usize;
    for (i, c) in text.chars().enumerate() {
        if i >= ci {
            break;
        }
        exp += match c {
            '\t' => 4,
            '\n' | '\r' => 0,
            _ => 1,
        };
    }
    exp
}

/// Map an expanded-text character index back to raw-text index
/// (mirrors raw_to_expanded — skips \n, tab→4).
pub fn expanded_to_raw(text: &str, exp_ci: usize) -> usize {
    let mut exp = 0usize;
    for (i, c) in text.chars().enumerate() {
        let w = match c {
            '\t' => 4,
            '\n' | '\r' => 0,
            _ => 1,
        };
        if exp >= exp_ci {
            return i;
        }
        exp += w;
    }
    text.chars().count()
}

/// Cursor position measurement for TextInput — kept for backward compat with TextInput click handler.
pub struct TextMeasurer;

impl TextMeasurer {
    pub fn new() -> Self {
        Self
    }
    pub fn cursor_x_at(
        &mut self,
        text: &str,
        char_index: usize,
        font_size: f32,
        _line_height: f32,
        font_weight: u16,
        font_family: Option<&str>,
    ) -> f32 {
        let prefix: String = text.chars().take(char_index).collect();
        measure_text_width(
            &prefix,
            font_size,
            font_weight,
            font_family.map(|s| s.to_string()),
        )
    }
    pub fn char_at_x(
        &mut self,
        text: &str,
        x_pos: f32,
        font_size: f32,
        line_height: f32,
        font_weight: u16,
        font_family: Option<&str>,
    ) -> usize {
        let mut lo = 0;
        let mut hi = text.chars().count();
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let w = self.cursor_x_at(text, mid, font_size, line_height, font_weight, font_family);
            if w <= x_pos {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo
    }
}

#[cfg(test)]
mod cursor_conversion_tests {
    use super::{char_index_to_cursor, cursor_to_char_index};
    use cosmic_text::Cursor;

    fn roundtrip(text: &str) {
        let n = text.chars().count();
        for ci in 0..=n {
            let cur = char_index_to_cursor(text, ci);
            let back = cursor_to_char_index(text, cur);
            assert_eq!(
                back, ci,
                "roundtrip failed at char_index {ci} for {text:?} (cursor={cur:?})"
            );
        }
    }

    #[test]
    fn roundtrip_single_line() {
        roundtrip("abcdef");
    }

    #[test]
    fn roundtrip_multiline() {
        roundtrip("abc\ndef\nghi");
    }

    #[test]
    fn roundtrip_with_empty_lines() {
        roundtrip("abc\n\ndef\n\n\nx");
    }

    #[test]
    fn roundtrip_multibyte() {
        roundtrip("中文\n简体字\n한글");
    }

    #[test]
    fn roundtrip_leading_and_trailing_newline() {
        roundtrip("\nabc\n");
    }

    #[test]
    fn empty_line_is_representable() {
        // "abc\n\ndef": char 4 is the start of the empty line (line 1, byte 0).
        let text = "abc\n\ndef";
        let cur = char_index_to_cursor(text, 4);
        assert_eq!(
            cur,
            Cursor::new(1, 0),
            "start of empty line must be line 1 byte 0"
        );
        assert_eq!(cursor_to_char_index(text, Cursor::new(1, 0)), 4);
    }

    #[test]
    fn line_start_positions() {
        // "abc\ndef": char 4 = 'd' = line 1 byte 0.
        let text = "abc\ndef";
        assert_eq!(char_index_to_cursor(text, 4), Cursor::new(1, 0));
        // char 0 = line 0 byte 0.
        assert_eq!(char_index_to_cursor(text, 0), Cursor::new(0, 0));
    }
}
