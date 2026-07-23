//! TableColumns — centralized column-offset helper.
//!
//! When a checkbox column is present (multi-selection), the data columns start
//! at grid index 1.  All column access goes through this helper so the offset
//! is handled in one place instead of 18 scattered `if has_cb { ci - 1 }` blocks.

use std::cell::Cell;
use std::rc::Rc;

use crate::core::element::{ElementArena, ElementId};
use crate::style::{Alignment, Color, Padding, TextAlign};
use crate::widgets::display::table::{ColRuntime, ColumnWidth};
use crate::widgets::shared::TextCellState;

/// Shared column runtime state, plus the checkbox-column offset.
#[derive(Clone)]
pub struct TableColumns {
    /// Data-column configs (length = data column count).
    pub cfgs: Vec<ColRuntime>,
    /// Overall grid column count (includes the checkbox column).
    pub grid_cols: usize,
    /// Width of the (optional) checkbox column.
    pub cb_w: f32,
    pub has_cb: bool,
}

impl TableColumns {
    pub fn new(cfgs: Vec<ColRuntime>, has_cb: bool) -> Self {
        let cb_w = 40.0_f32;
        let grid_cols = cfgs.len() + if has_cb { 1 } else { 0 };
        Self {
            cfgs,
            grid_cols,
            cb_w,
            has_cb,
        }
    }

    /// The data-column index for a raw grid column `ci`.  `ci == 0` is the
    /// checkbox column when it exists; for data columns return `ci - 1`.
    /// (Documents the checkbox-offset convention; not yet consumed.)
    #[inline]
    #[allow(dead_code)]
    pub fn dci(&self, ci: usize) -> usize {
        if self.has_cb {
            ci - 1
        } else {
            ci
        }
    }

    /// Data column config by grid column index.
    #[allow(dead_code)]
    pub fn col(&self, ci: usize) -> &ColRuntime {
        &self.cfgs[self.dci(ci)]
    }

    /// Grid track widths for taffy (includes the checkbox width when present).
    pub fn grid_widths(&self) -> Vec<f32> {
        let mut w = Vec::with_capacity(self.grid_cols);
        if self.has_cb {
            w.push(self.cb_w);
        }
        for c in &self.cfgs {
            w.push(match c.spec {
                ColumnWidth::Fixed(_) => c.current_width.get(),
                ColumnWidth::Flex(_) => 0.0,
            });
        }
        w
    }

    /// Number of data columns (excludes checkbox).
    #[allow(dead_code)]
    pub fn data_cols(&self) -> usize {
        self.cfgs.len()
    }

    /// Is this grid column the checkbox?
    #[inline]
    #[allow(dead_code)]
    pub fn is_cb(&self, ci: usize) -> bool {
        self.has_cb && ci == 0
    }

    /// Grid column offset for a data column at original index `dci`.
    #[inline]
    pub fn grid_off(&self, dci: usize) -> u32 {
        if self.has_cb {
            (dci + 1) as u32
        } else {
            dci as u32
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Row / Cell construction seam — the SINGLE place grid rows are built.
// Used by body (initial pool + grow) and footer. Header keeps its flex
// layout but reuses `build_cell` for cell-level construction.
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Body,
    Footer,
}

/// Theme-derived colors + metrics for one row (one bag instead of 11 args).
#[derive(Clone, Copy)]
pub struct RowStyle {
    pub grid_border: Color,
    pub surface_bg: Color,
    pub even_bg: Color,
    pub foreground: Color,
    pub row_h: f32,
}

/// Per-row state style colors (owned by the caller).
#[derive(Default, Clone, Copy)]
pub struct RowOverrides {
    pub hover_bg: Option<Color>,
    pub pressed_bg: Option<Color>,
    pub checked_bg: Option<Color>,
    pub focused_bg: Option<Color>,
    /// Whether this row should paint the zebra-stripe background.
    pub striped_even: bool,
}

/// Handles returned from [`build_data_row`] so the caller can wire events
/// (row click/hover, checkbox `on_click`) and store per-row state.
pub struct RowParts {
    pub eid: ElementId,
    /// `has_cb` ⇒ `cell_ids[0]` is the checkbox cell.
    pub cell_ids: Vec<ElementId>,
    pub cell_states: Vec<TextCellState>,
}

/// Where a cell sits: grid (body/footer) or flex (header).
pub enum CellPlacement {
    Grid {
        offset: u32,
    },
    Flex {
        preferred_width: Option<f32>,
        flex_grow: f32,
    },
}

/// Build one cell element + its [`TextCellState`]. Shared by body/footer
/// (grid) and header (flex), so cell styling lives in exactly one place.
#[allow(clippy::too_many_arguments)]
pub fn build_cell(
    arena: &mut ElementArena,
    role: accesskit::Role,
    text: &str,
    font_size: f32,
    font_weight: u16,
    text_align: TextAlign,
    foreground: Color,
    border: Color,
    height: f32,
    max_width: Option<f32>,
    accepts_mouse: bool,
    placement: CellPlacement,
) -> (ElementId, TextCellState) {
    let cid = arena.allocate();
    let Some(el) = arena.get_mut(cid) else {
        return (
            cid,
            TextCellState {
                eid: cid,
                lazy_label: Rc::new(Cell::new(String::new())),
                text_gen: Rc::new(Cell::new(0)),
                font_size: 0.0,
                line_height: 0.0,
                font_weight: 0,
                font_family: None,
                max_width: None,
                text_align: crate::style::TextAlign::Start,
            },
        );
    };
    el.set_accessible_role(role);
    el.set_accessible_label(text.to_owned());
    el.set_font_size(font_size);
    el.set_font_weight(font_weight);
    el.set_text_align(text_align);
    el.set_text_vertical_center(true);
    el.set_accepts_mouse(accepts_mouse);
    el.set_foreground(foreground);
    el.set_border_width(1.0);
    el.set_border_color(border);
    el.set_padding(Padding {
        left: 8.0,
        right: 8.0,
        top: 0.0,
        bottom: 0.0,
    });
    el.set_affected_by_child_size(false);
    el.set_preferred_height(height);
    match placement {
        CellPlacement::Grid { offset } => {
            el.set_grid_column_offset(offset);
            el.set_grid_column_span(1);
        }
        CellPlacement::Flex {
            preferred_width,
            flex_grow,
        } => {
            el.set_flex_shrink(0.0);
            if flex_grow > 0.0 {
                el.set_flex_grow(flex_grow);
                el.set_preferred_width(Some(0.0));
            } else if let Some(pw) = preferred_width {
                el.set_preferred_width(Some(pw));
            }
        }
    }
    let cs = TextCellState::mount(
        cid,
        el,
        text,
        font_size,
        1.0,
        font_weight,
        None,
        max_width,
        text_align,
    );
    (cid, cs)
}

/// THE single grid-row builder. Used by body (initial + grow) and footer.
/// `cell_texts.len()` must equal `cols.data_cols()`.
///
/// When `cols.has_cb`, a checkbox cell is created at grid column 0 (the only
/// place body checkbox cells are created); its glyph follows `checkbox_checked`.
pub fn build_data_row(
    arena: &mut ElementArena,
    cols: &TableColumns,
    kind: RowKind,
    style: &RowStyle,
    cell_texts: &[String],
    checkbox_checked: Option<bool>,
    overrides: RowOverrides,
) -> RowParts {
    let row_id = arena.allocate();
    {
        let Some(el) = arena.get_mut(row_id) else {
            return RowParts {
                eid: row_id,
                cell_ids: Vec::new(),
                cell_states: Vec::new(),
            };
        };
        el.set_accessible_role(accesskit::Role::Row);
        el.set_grid_columns(cols.grid_cols as u32);
        el.set_grid_column_widths(cols.grid_widths());
        el.set_flex_shrink(0.0);
        el.set_affected_by_child_size(false);
        el.set_content_align(Alignment::Start);
        el.set_preferred_height(style.row_h);
        el.with_state_style(|ss| {
            if let Some(bg) = overrides.checked_bg {
                ss.checked.background = Some(bg);
            }
            if let Some(bg) = overrides.hover_bg {
                ss.hovered.background = Some(bg);
            }
            if let Some(bg) = overrides.pressed_bg {
                ss.pressed.background = Some(bg);
            }
            if let Some(bg) = overrides.focused_bg {
                ss.focused.background = Some(bg);
            }
        });
        if kind == RowKind::Footer {
            el.set_background(style.surface_bg);
        } else if overrides.striped_even {
            el.set_background(style.even_bg);
        }
    }

    let mut cell_ids = Vec::with_capacity(cols.grid_cols);
    let mut cell_states = Vec::with_capacity(cols.grid_cols);

    // Checkbox cell (body only; header's static box lives in build_header).
    if cols.has_cb {
        if let Some(checked) = checkbox_checked {
            let glyph = if checked { "\u{2611}" } else { "\u{2610}" };
            let (cb_id, cb_cs) = build_cell(
                arena,
                accesskit::Role::GridCell,
                glyph,
                13.0,
                600,
                TextAlign::Center,
                style.foreground,
                style.grid_border,
                style.row_h,
                Some(16.0),
                true, // checkbox must accept mouse for its on_click
                CellPlacement::Grid { offset: 0 },
            );
            arena.add_child(row_id, cb_id);
            cell_ids.push(cb_id);
            cell_states.push(cb_cs);
        }
    }

    // Data cells.
    for (dci, text) in cell_texts.iter().enumerate() {
        let cfg = &cols.cfgs[dci];
        let font_weight = if kind == RowKind::Footer {
            cfg.font_weight + 200
        } else {
            cfg.font_weight
        };
        let max_w = match cfg.spec {
            ColumnWidth::Fixed(_) => Some((cfg.current_width.get() - 16.0).max(4.0)),
            ColumnWidth::Flex(_) => None,
        };
        let (cid, cs) = build_cell(
            arena,
            accesskit::Role::GridCell,
            text,
            cfg.font_size,
            font_weight,
            cfg.text_align,
            style.foreground,
            style.grid_border,
            style.row_h,
            max_w,
            false,
            CellPlacement::Grid {
                offset: cols.grid_off(dci),
            },
        );
        arena.add_child(row_id, cid);
        cell_ids.push(cid);
        cell_states.push(cs);
    }

    RowParts {
        eid: row_id,
        cell_ids,
        cell_states,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widgets::display::table::{ColRuntime, ColumnWidth, TableColumn};

    fn style() -> RowStyle {
        let c = Color::rgba8(0, 0, 0, 255);
        RowStyle {
            grid_border: c,
            surface_bg: c,
            even_bg: c,
            foreground: c,
            row_h: 28.0,
        }
    }

    fn make_cols(has_cb: bool) -> TableColumns {
        let cfgs: Vec<ColRuntime> = (0..3)
            .map(|_| {
                ColRuntime::init_from(&TableColumn::<String>::new("X", ColumnWidth::Fixed(80.0)))
            })
            .collect();
        TableColumns::new(cfgs, has_cb)
    }

    #[test]
    fn data_row_cell_count_with_checkbox() {
        let mut h = TestHarness::new(400.0, 200.0);
        let c = make_cols(true);
        let texts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let parts = build_data_row(
            &mut h.arena,
            &c,
            RowKind::Body,
            &style(),
            &texts,
            Some(false),
            RowOverrides::default(),
        );
        assert_eq!(parts.cell_ids.len(), 4, "3 data + 1 checkbox");
        assert_eq!(parts.cell_ids.len(), c.grid_cols);
        assert_eq!(parts.cell_states.len(), parts.cell_ids.len());
    }

    #[test]
    fn data_row_cell_count_without_checkbox() {
        let mut h = TestHarness::new(400.0, 200.0);
        let c = make_cols(false);
        let texts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let parts = build_data_row(
            &mut h.arena,
            &c,
            RowKind::Body,
            &style(),
            &texts,
            None,
            RowOverrides::default(),
        );
        assert_eq!(parts.cell_ids.len(), 3);
        assert_eq!(parts.cell_ids.len(), c.grid_cols);
    }

    #[test]
    fn footer_uses_heavier_weight_does_not_panic() {
        let mut h = TestHarness::new(400.0, 200.0);
        let c = make_cols(true);
        let texts = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let parts = build_data_row(
            &mut h.arena,
            &c,
            RowKind::Footer,
            &style(),
            &texts,
            Some(false),
            RowOverrides::default(),
        );
        // Footer with checkbox column present still aligns the grid.
        assert_eq!(parts.cell_ids.len(), c.grid_cols);
    }
}
