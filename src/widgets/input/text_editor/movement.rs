use super::state::EditorState;
use cosmic_text::Buffer;

impl EditorState {
    pub fn move_left(&mut self, extend: bool) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
        self.preferred_x = -1.0;
    }

    pub fn move_right(&mut self, extend: bool) {
        let max = self.text_rope.len_chars();
        if self.cursor < max {
            self.cursor += 1;
        }
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
        self.preferred_x = -1.0;
    }

    pub fn move_to_start(&mut self, extend: bool) {
        self.cursor = 0;
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
        self.preferred_x = -1.0;
    }

    pub fn move_to_end(&mut self, extend: bool) {
        self.cursor = self.text_rope.len_chars();
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
        self.preferred_x = -1.0;
    }

    pub fn move_word_left(&mut self, extend: bool) {
        let text = self.text_rope.to_string();
        self.cursor = Self::word_left(&text, self.cursor);
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
        self.preferred_x = -1.0;
    }

    pub fn move_word_right(&mut self, extend: bool) {
        let text = self.text_rope.to_string();
        self.cursor = Self::word_right(&text, self.cursor);
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
        self.preferred_x = -1.0;
    }

    pub fn move_line_start(&mut self, extend: bool) {
        let text = self.text_rope.to_string();
        let before: String = text.chars().take(self.cursor).collect();
        let line_start = before.rfind('\n').map_or(0, |pos| pos + 1);
        self.cursor = line_start;
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
    }

    pub fn move_line_end(&mut self, extend: bool) {
        let text = self.text_rope.to_string();
        let after: String = text.chars().skip(self.cursor).collect();
        let line_end = after
            .find('\n')
            .map_or(text.chars().count(), |pos| self.cursor + pos);
        self.cursor = line_end;
        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
    }

    /// Move the cursor up/down by one visual row using cosmic-text's native
    /// `cursor_motion`, which correctly handles empty lines, line-start/end
    /// convergence, soft-wrapped rows, and desired-column preservation. `delta`
    /// is -1 (up) or +1 (down). Requires `&mut Buffer` because cosmic-text may
    /// (re)shape/scroll the buffer while resolving the motion.
    pub fn move_visual_row(&mut self, delta: isize, extend: bool, buffer: &mut Buffer) {
        use crate::render::text::{char_index_to_cursor, cursor_to_char_index};
        use cosmic_text::Motion;

        let text = self.text_rope.to_string();
        let char_count = text.chars().count();
        let raw_ci = self.cursor.min(char_count);
        let cursor = char_index_to_cursor(&text, raw_ci);
        let motion = if delta < 0 { Motion::Up } else { Motion::Down };

        // preferred_x carries the desired column pixel-x across successive
        // vertical moves (< 0.0 means "unset — use current cursor x").
        let cursor_x_opt = if self.preferred_x >= 0.0 {
            Some(self.preferred_x as i32)
        } else {
            None
        };

        let result = crate::render::wgpu::glyphon_bridge::FONT_SYSTEM.with(|fs_cell| {
            let mut guard = fs_cell.borrow_mut();
            let fs = guard
                .as_mut()
                .expect("FONT_SYSTEM must be initialised before text_editor use");
            buffer.cursor_motion(fs, cursor, cursor_x_opt, motion)
        });

        match result {
            Some((new_cursor, new_cursor_x)) if new_cursor.line != cursor.line => {
                // Row actually changed — take the native result.
                self.cursor = cursor_to_char_index(&text, new_cursor).min(char_count);
                // Establish the desired column ONCE at the start of a vertical
                // run; keep it fixed across subsequent moves so passing through a
                // (zero-width) empty line doesn't reset the target column.
                if self.preferred_x < 0.0 {
                    self.preferred_x = new_cursor_x.map(|x| x as f32).unwrap_or(-1.0);
                }
            }
            _ => {
                // No row change possible → we're at a document boundary. Standard
                // editor behavior: Up on the first row converges to text start,
                // Down on the last row converges to text end.
                if delta < 0 {
                    self.cursor = 0;
                } else if delta > 0 {
                    self.cursor = char_count;
                }
                self.preferred_x = -1.0;
            }
        }

        if !extend {
            self.selection_anchor = self.cursor;
            self.has_selection = false;
        } else {
            self.has_selection = self.cursor != self.selection_anchor;
        }
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = 0;
        self.cursor = self.text_rope.len_chars();
        self.has_selection = true;
    }
}
