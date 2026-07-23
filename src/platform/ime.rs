//! IME (Input Method Editor) integration for CJK text input.

use crate::style::Rect;

/// Compose an element-local IME caret rect into window/surface coordinates.
///
/// `local_caret` comes from `CursorComponent::ime_cursor_rect` (element-local,
/// padding baked in, text-scroll-independent). `None` — the caret has not been
/// measured yet — falls back to the whole element bounds, which is still a
/// valid "don't obscure this area" hint per the winit cursor-area contract.
///
/// Pure function: unit-testable without ECS or a live window (splice P0).
pub fn compose_ime_surface_rect(
    element_bounds: Rect,
    local_caret: Option<Rect>,
    text_scroll: (f32, f32),
) -> Rect {
    match local_caret {
        Some(r) => Rect::new(
            element_bounds.x + r.x - text_scroll.0,
            element_bounds.y + r.y - text_scroll.1,
            r.width,
            r.height,
        ),
        None => element_bounds,
    }
}

/// Tracks IME preedit state for a text input.
#[derive(Clone, Debug)]
pub struct ImeState {
    /// The preedit (composing) text.
    pub preedit_text: Option<String>,
    /// Cursor range within the preedit text.
    pub cursor_range: Option<(usize, usize)>,
    /// Whether IME is currently active.
    pub enabled: bool,
}

impl ImeState {
    pub fn new() -> Self {
        Self {
            preedit_text: None,
            cursor_range: None,
            enabled: false,
        }
    }

    /// Handle an IME preedit event from winit.
    pub fn handle_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        self.preedit_text = if text.is_empty() { None } else { Some(text) };
        self.cursor_range = cursor;
    }

    /// Handle an IME commit event from winit.
    pub fn handle_commit(&mut self) -> Option<String> {
        let committed = self.preedit_text.take();
        self.cursor_range = None;
        committed
    }

    /// Enable IME.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable IME.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.preedit_text = None;
        self.cursor_range = None;
    }

    /// Check if there is active preedit text to render.
    pub fn has_preedit(&self) -> bool {
        self.preedit_text.is_some()
    }
}

impl Default for ImeState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_offsets_by_bounds_and_scroll() {
        let r = compose_ime_surface_rect(
            Rect::new(100.0, 50.0, 200.0, 32.0),
            Some(Rect::new(24.0, 6.0, 2.0, 20.0)),
            (10.0, 0.0),
        );
        assert_eq!((r.x, r.y, r.width, r.height), (114.0, 56.0, 2.0, 20.0));
    }

    #[test]
    fn compose_falls_back_to_element_bounds() {
        let b = Rect::new(5.0, 6.0, 7.0, 8.0);
        let r = compose_ime_surface_rect(b, None, (99.0, 99.0));
        assert_eq!((r.x, r.y, r.width, r.height), (5.0, 6.0, 7.0, 8.0));
    }
}
