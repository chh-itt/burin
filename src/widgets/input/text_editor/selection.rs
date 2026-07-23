use super::state::EditorState;
use crate::render::text::{ci_at_visual_x, expanded_to_raw, visual_row_from_y};
use crate::style::Point;
use cosmic_text::Buffer;

impl EditorState {
    /// Move cursor to pixel position (click).
    pub fn click_at(
        &mut self,
        pixel_pos: Point,
        buffer: &Buffer,
        padding: (f32, f32),
        is_multiline: bool,
        scroll_x: f32,
    ) {
        let text = self.text_rope.to_string();
        let vrow = if is_multiline {
            visual_row_from_y(
                buffer,
                pixel_pos.y - padding.1 + 0.0, /* scroll_y for vertical */
            )
        } else {
            0
        };
        let exp_idx = ci_at_visual_x(buffer, vrow, pixel_pos.x - padding.0 + scroll_x);
        let raw_idx = expanded_to_raw(&text, exp_idx).min(text.chars().count());
        self.cursor = raw_idx;
        self.selection_anchor = raw_idx;
        self.has_selection = false;
        self.preferred_x = -1.0;
    }

    /// Extend selection to pixel position (drag).
    pub fn extend_selection_to(
        &mut self,
        pixel_pos: Point,
        buffer: &Buffer,
        padding: (f32, f32),
        is_multiline: bool,
        scroll_x: f32,
    ) {
        let text = self.text_rope.to_string();
        let vrow = if is_multiline {
            visual_row_from_y(buffer, pixel_pos.y - padding.1 + 0.0 /* scroll_y */)
        } else {
            0
        };
        let exp_idx = ci_at_visual_x(buffer, vrow, pixel_pos.x - padding.0 + scroll_x);
        let raw_idx = expanded_to_raw(&text, exp_idx).min(text.chars().count());
        self.cursor = raw_idx;
        self.has_selection = self.cursor != self.selection_anchor;
    }

    /// Select word at pixel position (double-click).
    pub fn select_word_at(
        &mut self,
        _pixel_pos: Point,
        _buffer: &Buffer,
        _padding: (f32, f32),
        _scroll_y: f32,
    ) {
        let text = self.text_rope.to_string();
        let chars: Vec<char> = text.chars().collect();
        let raw_idx = self.cursor.min(chars.len());

        let mut start = raw_idx.min(chars.len());
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let mut end = raw_idx.min(chars.len());
        while end < chars.len() && !chars[end].is_whitespace() {
            end += 1;
        }

        self.cursor = end;
        self.selection_anchor = start;
        self.has_selection = start != end;
    }

    /// Select line at pixel position (triple-click).
    pub fn select_line_at(
        &mut self,
        _pixel_pos: Point,
        _buffer: &Buffer,
        _padding: (f32, f32),
        _scroll_y: f32,
    ) {
        let text = self.text_rope.to_string();
        let raw_idx = self.cursor.min(text.chars().count());

        let before: String = text.chars().take(raw_idx).collect();
        let line_start = before.rfind('\n').map_or(0, |pos| pos + 1);
        let after: String = text.chars().skip(raw_idx).collect();
        let line_end = after
            .find('\n')
            .map_or(text.chars().count(), |pos| raw_idx + pos);

        self.cursor = line_end;
        self.selection_anchor = line_start;
        self.has_selection = line_start != line_end;
    }
}
