//! Text cursor and selection state for input widgets.

/// Tracks cursor position and text selection for an input field.
#[derive(Clone, Debug)]
pub struct TextCursor {
    /// Character offset of the cursor (between 0 and len).
    pub position: usize,
    /// Start of the selection range, if any.
    pub selection_start: Option<usize>,
    /// Horizontal pixel offset for scrolling.
    pub scroll_offset: f32,
    /// Preferred x position for vertical navigation.
    pub preferred_x: f32,
}

impl TextCursor {
    pub fn new() -> Self {
        Self { position: 0, selection_start: None, scroll_offset: 0.0, preferred_x: 0.0 }
    }

    /// Move cursor left by one character.
    pub fn move_left(&mut self, text: &str) {
        if self.position > 0 {
            self.position -= 1;
            while self.position > 0 && !text.is_char_boundary(self.position) {
                self.position -= 1;
            }
        }
        self.clear_selection();
    }

    /// Move cursor right by one character.
    pub fn move_right(&mut self, text: &str) {
        let len = text.len();
        if self.position < len {
            self.position += 1;
            while self.position < len && !text.is_char_boundary(self.position) {
                self.position += 1;
            }
        }
        self.clear_selection();
    }

    /// Move cursor to the start of the text.
    pub fn move_home(&mut self) {
        self.position = 0;
        self.clear_selection();
    }

    /// Move cursor to the end of the text.
    pub fn move_end(&mut self, text: &str) {
        self.position = text.len();
        self.clear_selection();
    }

    /// Start or extend a selection.
    pub fn select_to(&mut self, position: usize, text_len: usize) {
        let pos = position.min(text_len);
        if self.selection_start.is_none() {
            self.selection_start = Some(self.position);
        }
        self.position = pos;
    }

    /// Select all text.
    pub fn select_all(&mut self, text_len: usize) {
        self.selection_start = Some(0);
        self.position = text_len;
    }

    /// Clear the current selection.
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
    }

    /// Get the selection range (start, end) in character offsets.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_start.map(|s| {
            if s < self.position { (s, self.position) } else { (self.position, s) }
        })
    }

    /// Check if there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some_and(|s| s != self.position)
    }

    /// Delete the selected text, or the character before the cursor.
    pub fn delete_backward(&mut self, text: &mut String) {
        if let Some((start, end)) = self.selection_range() {
            text.replace_range(start..end, "");
            self.position = start;
            self.clear_selection();
        } else if self.position > 0 {
            let prev = self.position - 1;
            while prev > 0 && !text.is_char_boundary(prev) { /* walk back */ }
            text.remove(prev);
            self.position = prev;
        }
    }

    /// Delete the selected text, or the character after the cursor.
    pub fn delete_forward(&mut self, text: &mut String) {
        if let Some((start, end)) = self.selection_range() {
            text.replace_range(start..end, "");
            self.position = start;
            self.clear_selection();
        } else if self.position < text.len() {
            text.remove(self.position);
        }
    }

    /// Insert text at the cursor, replacing any selection.
    pub fn insert(&mut self, text: &mut String, ch: &str) {
        if let Some((start, end)) = self.selection_range() {
            text.replace_range(start..end, ch);
            self.position = start + ch.len();
            self.clear_selection();
        } else {
            text.insert_str(self.position, ch);
            self.position += ch.len();
        }
    }
}

impl Default for TextCursor {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_move_and_insert() {
        let mut cursor = TextCursor::new();
        let mut text = String::new();
        cursor.insert(&mut text, "hello");
        assert_eq!(text, "hello");
        assert_eq!(cursor.position, 5);
        cursor.move_left(&text);
        assert_eq!(cursor.position, 4);
    }

    #[test]
    fn selection_delete() {
        let mut cursor = TextCursor::new();
        let mut text = String::from("hello world");
        cursor.select_all(text.len());
        cursor.insert(&mut text, "hi");
        assert_eq!(text, "hi");
        assert_eq!(cursor.position, 2);
    }
}
