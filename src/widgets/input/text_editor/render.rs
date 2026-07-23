use super::state::{EditorState, TextInputConfig};
use crate::render::text::{glyph_pos_at_ci, raw_to_expanded, visual_row_at_exp_ci};
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::TextAlign;
use crate::style::{Rect, Vec2};
use cosmic_text::Buffer;

/// Compute display text: applies password masking, composition inline, placeholder.
pub fn display_text(state: &EditorState) -> String {
    let text = state.text_for_display();
    if text.is_empty() && state.composition.is_none() {
        return state.config.placeholder.clone();
    }

    if let Some(ref comp) = state.composition {
        let range_start = comp.range.start.min(text.chars().count());
        let range_end = comp.range.end.min(text.chars().count());

        let mut chars: Vec<char> = text.chars().collect();
        if range_start <= range_end && range_end <= chars.len() {
            chars.drain(range_start..range_end);
            for (i, ch) in comp.text.chars().enumerate() {
                chars.insert(range_start + i, ch);
            }
        }
        chars.into_iter().collect()
    } else if state.config.is_password() {
        "•".repeat(text.chars().count())
    } else {
        text
    }
}

/// Build shaped text buffer from display text.
pub fn build_buffer(
    display: &str,
    config: &TextInputConfig,
    width: f32,
    padding_left: f32,
    text_align: TextAlign,
) -> Buffer {
    let cw = if config.is_multiline() {
        Some((width - padding_left * 2.0).max(config.font_size * 2.0))
    } else {
        None
    };
    create_buffer(
        display,
        config.font_size,
        config.line_height,
        config.font_weight,
        config.font_family.as_deref(),
        cw,
        text_align,
    )
}

/// Compute cursor pixel position from text buffer.
///
/// Returns `(visual_row_index, pixel_x)`. Uses the same `char_index -> Cursor
/// {line, byte}` mapping as vertical movement, then locates the position inside
/// cosmic-text's layout runs by matching `LayoutRun.line_i` and the glyph byte
/// offsets. This lets the caret render correctly on EMPTY lines (row = that
/// line's run, x = line start) instead of collapsing onto the next line — the
/// bug that made empty-line navigation look broken even though the logical
/// cursor was correct.
pub fn cursor_pixel_pos(buffer: &Buffer, text: &str, cursor: usize) -> (usize, f32) {
    let max_ci = text.chars().count();
    let ci_val = cursor.min(max_ci);
    let target = crate::render::text::char_index_to_cursor(text, ci_val);

    // Walk visual rows (layout runs). Each run knows its logical line (`line_i`).
    // We want the run for `target.line`; within it, place x by byte offset.
    let mut last_row_on_line: Option<(usize, f32)> = None;
    for (row_idx, run) in buffer.layout_runs().enumerate() {
        if run.line_i != target.line {
            // Once we've passed the target line, stop.
            if run.line_i > target.line {
                break;
            }
            continue;
        }
        // This run belongs to the target logical line (may be one of several if
        // soft-wrapped). Find the glyph whose byte range starts at/after target.
        let mut x_at_end = run.glyphs.last().map(|g| g.x + g.w).unwrap_or(0.0);
        // Row start x (first glyph x, or 0 for empty line).
        let row_start_x = run.glyphs.first().map(|g| g.x).unwrap_or(0.0);
        for g in run.glyphs.iter() {
            if g.start >= target.index {
                return (row_idx, g.x);
            }
        }
        // Target byte is at/after the last glyph on this run → end of run.
        if run.glyphs.is_empty() {
            // Empty line: caret sits at the line's start x on this visual row.
            return (row_idx, row_start_x);
        }
        x_at_end = run.glyphs.last().map(|g| g.x + g.w).unwrap_or(x_at_end);
        last_row_on_line = Some((row_idx, x_at_end));
    }
    // If the target byte was beyond the last glyph of the (wrapped) line, use the
    // end of the last row we saw for that line.
    if let Some(pos) = last_row_on_line {
        return pos;
    }
    // Fallback: line not found in layout (e.g. trailing empty line at EOF).
    let last_row = buffer.layout_runs().count().saturating_sub(1);
    (last_row, 0.0)
}

/// Compute selection highlight rects — one rect per visual row.
pub fn selection_rects(
    buffer: &Buffer,
    text: &str,
    start: usize,
    end: usize,
    row_height: f32,
    padding_left: f32,
    padding_top: f32,
) -> Vec<Rect> {
    use crate::render::text::char_index_to_cursor;

    if start >= end || text.is_empty() {
        return Vec::new();
    }

    // Selection endpoints as {line, byte} cursors.
    let sel_lo = char_index_to_cursor(text, start);
    let sel_hi = char_index_to_cursor(text, end);

    // Width used to hint "the newline at end of a selected line is included"
    // (also the visible width for a selected empty line).
    let newline_hint_w = (row_height * 0.4).max(4.0);

    let mut rects = Vec::new();
    for (row_idx, run) in buffer.layout_runs().enumerate() {
        let line = run.line_i;
        // Skip rows entirely outside the selected logical-line span.
        if line < sel_lo.line || line > sel_hi.line {
            continue;
        }

        // Determine the byte range of the selection ON THIS run.
        // A run covers one logical line (or a wrapped fragment of it). Compute
        // the run's byte extent from its glyphs.
        let run_first_byte = run.glyphs.first().map(|g| g.start);
        let run_last_byte_end = run.glyphs.last().map(|g| g.end);

        // Left x of the selection on this row.
        let start_x = if line == sel_lo.line {
            // Selection starts on this line — find x at sel_lo.index.
            byte_to_x(&run, sel_lo.index)
        } else {
            // Selection began on an earlier line → from row start.
            run.glyphs.first().map(|g| g.x).unwrap_or(0.0)
        };

        // Right x of the selection on this row.
        let (end_x, include_newline) = if line == sel_hi.line {
            // Selection ends on this line.
            (byte_to_x(&run, sel_hi.index), false)
        } else {
            // Selection continues past this line → cover to end of run + a hint
            // for the trailing newline.
            let ex = run_last_byte_end
                .map(|_| run.glyphs.last().map(|g| g.x + g.w).unwrap_or(0.0))
                .unwrap_or(0.0);
            (ex, true)
        };

        let _ = (run_first_byte,); // (reserved for future wrapped-run refinement)

        let mut w = (end_x - start_x).max(0.0);
        // Empty line or line whose newline is selected → show a small stub so
        // the selection is visible on empty/blank rows.
        if include_newline || run.glyphs.is_empty() {
            w = w.max(newline_hint_w);
        }

        if w > 0.0 {
            rects.push(Rect::new(
                padding_left + start_x,
                padding_top + row_idx as f32 * row_height,
                w,
                row_height,
            ));
        }
    }
    rects
}

/// Pixel x within a layout run at a given byte offset (relative to the line).
/// Empty run → 0. Byte past the last glyph → end of run.
fn byte_to_x(run: &cosmic_text::LayoutRun, byte_index: usize) -> f32 {
    for g in run.glyphs.iter() {
        if g.start >= byte_index {
            return g.x;
        }
    }
    // At/after the last glyph → right edge of the run.
    run.glyphs.last().map(|g| g.x + g.w).unwrap_or(0.0)
}

/// Compute IME composition underline rect.
pub fn composition_underline_rect(
    buffer: &Buffer,
    text: &str,
    comp_range: (usize, usize),
    row_height: f32,
    padding_left: f32,
    padding_top: f32,
) -> Option<Rect> {
    let (start, end) = comp_range;
    if start >= end {
        return None;
    }
    let elo = raw_to_expanded(text, start.min(text.chars().count()));
    let ehi = raw_to_expanded(text, end.min(text.chars().count()));
    let rlo = visual_row_at_exp_ci(buffer, elo);
    let start_x = glyph_pos_at_ci(buffer, elo, false).1;
    let end_x = glyph_pos_at_ci(buffer, ehi, false).1;

    Some(Rect::new(
        padding_left + start_x,
        padding_top + rlo as f32 * row_height + row_height - 2.0,
        (end_x - start_x).max(1.0),
        2.0,
    ))
}

/// Auto-scroll to keep cursor in view.
pub fn auto_scroll(
    cursor_x: f32,
    cursor_row: usize,
    row_h: f32,
    visible_w: f32,
    visible_h: f32,
    scroll_offset: &std::rc::Rc<std::cell::Cell<Vec2>>,
    max_scroll: &std::rc::Rc<std::cell::Cell<Vec2>>,
) {
    let mut o = scroll_offset.get();
    let max = max_scroll.get();

    if cursor_x > o.x + visible_w - 4.0 {
        o.x = cursor_x - visible_w + 4.0;
    }
    if cursor_x < o.x {
        o.x = cursor_x;
    }

    let cursor_y = cursor_row as f32 * row_h;
    if cursor_y > o.y + visible_h - row_h {
        o.y = cursor_y - visible_h + row_h;
    }
    if cursor_y < o.y {
        o.y = cursor_y;
    }

    o.x = o.x.max(0.0).min(max.x);
    o.y = o.y.max(0.0).min(max.y);

    scroll_offset.set(o);
}

#[cfg(test)]
mod selection_tests {
    use super::selection_rects;
    use crate::render::wgpu::glyphon_bridge::create_buffer;
    use crate::style::TextAlign;

    fn multiline_buffer(text: &str) -> cosmic_text::Buffer {
        // Wide enough that no soft-wrap occurs; only hard '\n' breaks lines.
        create_buffer(text, 16.0, 1.2, 400, None, Some(1000.0), TextAlign::Left)
    }

    #[test]
    fn selection_over_empty_line_has_visible_rect_on_that_row() {
        // "abc\n\ndef": selecting the whole thing must yield a rect on EACH of
        // the 3 visual rows — including the empty middle row (row 1).
        let text = "abc\n\ndef";
        let buf = multiline_buffer(text);
        let rects = selection_rects(&buf, text, 0, text.chars().count(), 20.0, 0.0, 0.0);
        assert_eq!(
            rects.len(),
            3,
            "expected one rect per visual row incl. empty, got {}: {rects:?}",
            rects.len()
        );
        // Row 1 is the empty line — it must have a visible (non-zero) width and
        // sit at y = 1 * row_height.
        let empty_row = rects
            .iter()
            .find(|r| (r.y - 20.0).abs() < 0.5)
            .expect("no rect on empty row (y=20)");
        assert!(
            empty_row.width > 0.0,
            "empty-line selection rect must be visible, got width {}",
            empty_row.width
        );
    }

    #[test]
    fn selection_when_first_line_empty() {
        // "\nabc": row 0 is empty. Selecting all → rect on row 0 (empty) visible.
        let text = "\nabc";
        let buf = multiline_buffer(text);
        let rects = selection_rects(&buf, text, 0, text.chars().count(), 20.0, 0.0, 0.0);
        assert!(
            rects.len() >= 2,
            "expected rects for empty first row + content row, got {}: {rects:?}",
            rects.len()
        );
        let first_row = rects
            .iter()
            .find(|r| r.y.abs() < 0.5)
            .expect("no rect on first (empty) row");
        assert!(
            first_row.width > 0.0,
            "empty first-line selection must be visible"
        );
    }

    #[test]
    fn single_line_selection_width_matches_content() {
        // Within one line, selecting "ab" of "abcdef" yields exactly one rect.
        let text = "abcdef";
        let buf = multiline_buffer(text);
        let rects = selection_rects(&buf, text, 0, 2, 20.0, 0.0, 0.0);
        assert_eq!(
            rects.len(),
            1,
            "single-line selection = one rect, got {rects:?}"
        );
        assert!(rects[0].width > 0.0);
    }
}
