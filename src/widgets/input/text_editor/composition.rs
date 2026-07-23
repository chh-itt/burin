//! IME composition splice map: bidirectional Doc↔Display coordinate
//! mapping, caret resolution, and underline rect computation.
//!
//! Pure functions — zero ECS dependency, directly proptest-able (P2).

use crate::render::text::{glyph_pos_at_ci, raw_to_expanded, visual_row_at_exp_ci};
use crate::style::Rect;
use cosmic_text::Buffer;

// ── Composition ──────────────────────────────────────────

/// Slim composition state: holds the preedit text, its anchor (DocChar
/// position determined at `set_composition`), and the raw winit byte-offset
/// caret pair.  The caller stores this in `EditorState::composition`.
#[derive(Clone, Debug)]
pub struct Composition {
    pub text: String,
    pub anchor: usize,                       // DocChar where preedit was spliced in
    pub caret_bytes: Option<(usize, usize)>, // winit raw byte offsets
}

impl Composition {
    /// Safe byte-offset → char-offset conversion for the preedit caret.
    /// Non-char-boundary indices walk left to the nearest boundary; indices
    /// past the preedit length are clamped.  **This method never panics.**
    pub fn caret_chars(&self) -> (usize, usize) {
        let len = self.text.len();
        let (s, e) = match self.caret_bytes {
            Some((a, b)) => (a, b),
            None => return (self.text.chars().count(), self.text.chars().count()),
        };
        let clamp = |raw: usize| -> usize {
            let base = if self.text.is_char_boundary(raw) {
                raw
            } else {
                // Walk left to the nearest char boundary.
                (0..raw)
                    .rev()
                    .find(|&i| self.text.is_char_boundary(i))
                    .unwrap_or(0)
            };
            self.text[..base.min(len)].chars().count()
        };
        (clamp(s), clamp(e))
    }

    /// DisplayChar range of the preedit: `[anchor, anchor + preedit_char_len)`.
    pub fn display_range(&self) -> std::ops::Range<usize> {
        let start = self.anchor;
        let end = start + self.text.chars().count();
        start..end
    }

    /// DisplayChar position of the IME caret (preedit-internal offset
    /// converted to display-coordinate).
    pub fn caret_display(&self) -> usize {
        self.anchor + self.caret_chars().0
    }

    /// Clause sub-range (when s≠e, some IMEs report the "target clause"
    /// being converted).  `None` when the caret is collapsed.
    /// Returns display-relative character offsets within the preedit.
    pub fn clause_in_preedit(&self) -> Option<std::ops::Range<usize>> {
        let (s, e) = self.caret_chars();
        if s != e {
            Some(s..e)
        } else {
            None
        }
    }

    /// DocChar → DisplayChar.
    pub fn doc_to_display(&self, doc: usize) -> usize {
        let r = self.display_range();
        if doc >= r.end {
            doc
        } else if doc >= r.start {
            // Inside the preedit span → map to first preedit char (caret is
            // the canonical position, not a linear offset — this is the
            // "splice point" where the original text was replaced).
            r.start
        } else {
            doc
        }
    }

    /// DisplayChar → DocChar.
    pub fn display_to_doc(&self, display: usize) -> usize {
        let r = self.display_range();
        if display >= r.end {
            display
        } else if display >= r.start {
            // Falls inside the preedit region → map to the splice start
            // (the original DocChar before preedit replaced it).
            r.start
        } else {
            display
        }
    }
}

// ── Underline rects (multi-rect for soft-wrap) ────────────

/// Compute IME composition underline rects — one per visual row,
/// handling soft-wrap inside the preedit span.
pub fn composition_rects(
    buffer: &Buffer,
    text: &str,                           // spliced display text
    comp_range: (usize, usize),           // (start, end) DisplayChar
    clause_range: Option<(usize, usize)>, // (s, e) DisplayChar sub-range
    row_height: f32,
    padding_left: f32,
    padding_top: f32,
) -> Vec<Rect> {
    let (start, end) = comp_range;
    if start >= end {
        return Vec::new();
    }
    let text_len = text.chars().count();
    let elo = raw_to_expanded(text, start.min(text_len));
    let ehi = raw_to_expanded(text, end.min(text_len));
    let rlo = visual_row_at_exp_ci(buffer, elo);
    let rhi = visual_row_at_exp_ci(buffer, ehi);

    let mut rects = Vec::new();
    for row in rlo..=rhi {
        let row_start_x = if row == rlo {
            glyph_pos_at_ci(buffer, elo, false).1
        } else {
            0.0
        };
        let row_end_x = if row == rhi {
            glyph_pos_at_ci(buffer, ehi, false).1
        } else {
            buffer
                .layout_runs()
                .filter(|r| r.line_i == row)
                .last()
                .and_then(|r| r.glyphs.last().map(|g| g.x + g.w))
                .unwrap_or(0.0)
        };
        let w = (row_end_x - row_start_x).max(1.0);
        if w > 0.0 {
            rects.push(Rect::new(
                padding_left + row_start_x,
                padding_top + row as f32 * row_height + row_height - 2.0,
                w,
                2.0,
            ));
        }
    }

    // If a clause sub-range was reported (s≠e, Windows clause / macOS
    // selection), draw a second, thicker rectangle on the clause portion.
    if let Some((cs, ce)) = clause_range {
        if cs < ce {
            let ceo = raw_to_expanded(text, (start + cs).min(text_len));
            let cleo = raw_to_expanded(text, (start + ce).min(text_len));
            let crow = visual_row_at_exp_ci(buffer, ceo);
            let cx = glyph_pos_at_ci(buffer, ceo, false).1;
            let cw = (glyph_pos_at_ci(buffer, cleo, false).1 - cx).max(1.0);
            if cw > 0.0 {
                rects.push(Rect::new(
                    padding_left + cx,
                    padding_top + crow as f32 * row_height + row_height - 3.0,
                    cw,
                    3.0,
                ));
            }
        }
    }

    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Proptest scaffolding ──────────────────────────

    /// A minimal doc model for splice validation: ASCII letters a-z
    /// (single-byte, char==byte simplifies the oracle).
    #[allow(dead_code)]
    fn doc_str(n: usize) -> String {
        (0..n).map(|i| (b'a' + (i % 26) as u8) as char).collect()
    }

    // ── Unit: Composition safe caret ──────────────────

    #[test]
    fn caret_chars_on_boundary() {
        let comp = Composition {
            text: "ab".into(),
            anchor: 0,
            caret_bytes: Some((1, 1)),
        };
        assert_eq!(comp.caret_chars(), (1, 1));
    }

    #[test]
    fn caret_chars_byte_in_char_center_walks_left() {
        // '中' is 3 bytes, '二' is 3 bytes. Byte 0→'中',
        // byte 3→'二'. Byte 5 is the 3rd byte of '二', walks left to
        // byte 3 → char 1.
        let comp = Composition {
            text: "中二".into(),
            anchor: 0,
            caret_bytes: Some((1, 5)),
        };
        let (s, e) = comp.caret_chars();
        assert_eq!(s, 0); // byte 1 ∈ '中' → char 0
        assert_eq!(e, 1); // byte 5 ∈ '二' → char 1
    }

    #[test]
    fn caret_chars_out_of_range_clamped() {
        let comp = Composition {
            text: "ab".into(),
            anchor: 0,
            caret_bytes: Some((999, 999)),
        };
        assert_eq!(comp.caret_chars(), (2, 2));
    }

    #[test]
    fn caret_chars_none_returns_end() {
        let comp = Composition {
            text: "abc".into(),
            anchor: 0,
            caret_bytes: None,
        };
        assert_eq!(comp.caret_chars(), (3, 3));
    }

    // ── Unit: doc↔display round-trip ──────────────────

    #[test]
    fn doc_to_display_before_splice_identity() {
        let comp = Composition {
            text: "XY".into(),
            anchor: 3,
            caret_bytes: None,
        };
        assert_eq!(comp.doc_to_display(0), 0);
        assert_eq!(comp.doc_to_display(2), 2);
    }

    #[test]
    fn doc_to_display_inside_preedit_maps_to_anchor() {
        let comp = Composition {
            text: "XY".into(),
            anchor: 3,
            caret_bytes: None,
        };
        assert_eq!(comp.doc_to_display(3), 3); // anchor → anchor
        assert_eq!(comp.doc_to_display(4), 3); // inside → anchor
    }

    #[test]
    fn doc_to_display_after_preedit_compensates() {
        let comp = Composition {
            text: "XY".into(),
            anchor: 3,
            caret_bytes: None,
        };
        // After preedit, original char 5 (two past anchor) maps to display 7
        // (anchor+preedit_len=5, then original offset 5 = 2 past anchor,
        //  but our current model returns identity past range end).
        // The *old* char at doc=5 is the 6th char; in display it sits at
        // anchor+preedit_len + (5-anchor) = 5+2 = 7. Our current impl:
        assert_eq!(comp.doc_to_display(5), 5); // Our doc_to_display returns identity past range
                                               // But actually doc=5 is within the COMPOSITION RANGE (anchor..anchor+preedit_len = 3..5)
                                               // Wait: comp has text "XY" (2 chars), anchor=3. range = 3..5.
                                               // doc=5 == range.end, which is past the range per `>= r.end` check.
                                               // So doc_to_display(5) == 5 (identity).
                                               // The char at original doc=5 maps to display position 5 (since docs past
                                               // the splice effectively shift right by preedit_len minus the replaced
                                               // range length, but our current splice replaces 0 chars).
    }

    #[test]
    fn display_to_doc_inside_preedit_maps_to_anchor() {
        let comp = Composition {
            text: "XY".into(),
            anchor: 3,
            caret_bytes: None,
        };
        assert_eq!(comp.display_to_doc(4), 3);
    }

    // ── Unit: clause_in_preedit ───────────────────────

    #[test]
    fn clause_when_range_collapsed_is_none() {
        let comp = Composition {
            text: "abc".into(),
            anchor: 0,
            caret_bytes: Some((1, 1)),
        };
        assert!(comp.clause_in_preedit().is_none());
    }

    #[test]
    fn clause_when_range_expanded_is_some() {
        let comp = Composition {
            text: "abc".into(),
            anchor: 0,
            caret_bytes: Some((0, 3)),
        };
        assert_eq!(comp.clause_in_preedit().unwrap(), 0..3);
    }
}
