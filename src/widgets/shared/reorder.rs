//! ReorderController — reusable drag-to-reorder for list-style layouts.
//!
//! Widgets (List, Table, TabBar, Tree) supply their ordered item ElementIds
//! and per-item position_offset / bg_override cells at mount time, then call
//! the four lifecycle methods from their drag-start/update/end handlers.
//!
//! Visuals (ghost following + dim) are driven by the controller — the widget
//! only supplies the cells and the data-mutation callback.  Because
//! `position_offset` is now hit-aware (framework Pass 2), the ghost correctly
//! tracks the pointer even when the source item moves off its original slot.
//!
//! ```text
//! drag_start  →  begin(src_eid, abs)    dim(src) + ghost anchor
//! drag_update →  update(abs, origin_y)  ghost follow
//! drag_end    →  end()                  reset visuals → on_reorder(src,dst)
//! ```

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config::StateFlags;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::ElementId;
use crate::style::{Color, Point, Vec2};

// DRAG_Z_REQUEST migrated to AppContext.interaction.drag_z_request:
// `(eid, elevate)` — `begin` sets `(src, true)`, `end`/`cancel` sets
// `(src, false)`. Consumed by the window loop, which raises/restores the
// dragged row's `z_index_floor` so it occludes the rows it overlaps.
// (Same-z rows can't occlude each other's text; raising the floor — not
// `z_index`, which would force absolute layout — moves the dragged row to
// its own render layer.)

/// Drained by the window loop each frame to elevate/restore the dragged row.
pub fn take_drag_z_request() -> Option<(ElementId, bool)> {
    crate::core::app_context::current_app().take_drag_z_request()
}

fn request_drag_z(eid: ElementId, elevate: bool) {
    crate::core::app_context::current_app().request_drag_z(eid, elevate);
}

/// Dimmed background colour for the dragged item source slot.
pub const GHOST_DIM: Color = Color::rgba8(0, 0, 0, 35);
/// Drop-target highlight background.
pub const DROP_HIGHLIGHT: Color = Color::rgba8(59, 130, 246, 50);

/// Reusable drag-to-reorder controller for list-like widgets.
pub struct ReorderController {
    /// Ordered list of item ElementIds (stable across pool re-labels).
    item_ids: Rc<RefCell<Vec<ElementId>>>,
    /// Per-item position_offset cells (indexed like `item_ids`).
    offsets: Rc<RefCell<Vec<Rc<Cell<Vec2>>>>>,
    /// Callback when a reorder is committed.
    on_reorder: Rc<RefCell<Option<Box<dyn Fn(usize, usize)>>>>,
    /// Item dimension along the drag axis (row height, or column width).
    item_size: Cell<f32>,
    /// When true the drag axis is horizontal (e.g. Table columns); default false = vertical.
    horizontal: Cell<bool>,
    source: Cell<Option<usize>>,
    target: Cell<Option<usize>>,
    /// Press coordinate along the drag axis (y for vertical, x for horizontal).
    press_anchor: Cell<f32>,
    /// Snapshot of the source item's background before dimming.
    saved_src_bg: Cell<Option<Color>>,
}

impl ReorderController {
    pub fn new() -> Self {
        Self {
            item_ids: Rc::new(RefCell::new(Vec::new())),
            offsets: Rc::new(RefCell::new(Vec::new())),
            on_reorder: Rc::new(RefCell::new(None)),
            item_size: Cell::new(36.0),
            horizontal: Cell::new(false),
            source: Cell::new(None),
            target: Cell::new(None),
            press_anchor: Cell::new(0.0),
            saved_src_bg: Cell::new(None),
        }
    }

    // ── Configuration ──────────────────────────────────────────────

    /// Register the ordered list of item ElementIds and their position_offset cells.
    /// Call after all items are mounted (re-call after a signal-triggered pool
    /// rebuild that changes the eid array).
    /// Drag dimming/ highlighting uses StateFlags (DRAG_OVER) and direct style
    /// modification — no bg_override cells needed.
    pub fn configure(&self, ids: Vec<ElementId>, offsets: Vec<Rc<Cell<Vec2>>>) {
        *self.item_ids.borrow_mut() = ids;
        *self.offsets.borrow_mut() = offsets;
    }

    /// The uniform item size (height in vertical mode, width in horizontal mode).
    pub fn set_item_height(&self, h: f32) {
        self.item_size.set(h);
    }

    /// Switch to horizontal drag mode (e.g., Table column reorder).
    /// `w` is the column width used for target-index arithmetic.
    pub fn set_horizontal(&self, w: f32) {
        self.horizontal.set(true);
        self.item_size.set(w);
    }

    /// Callback when a reorder is committed: `on_reorder(src_idx, dst_idx)`.
    /// The widget should mutate the underlying data signal inside this callback.
    pub fn on_reorder(&self, f: impl Fn(usize, usize) + 'static) {
        self.on_reorder.borrow_mut().replace(Box::new(f));
    }

    // ── Drag lifecycle ─────────────────────────────────────────────

    /// Call from the widget's `register_drag_start` handler.
    /// Returns the source index found for `pressed_id`, or `None`.
    pub fn begin(&self, pressed_id: ElementId, abs: Point) -> Option<usize> {
        let ids = self.item_ids.borrow();
        let src = ids.iter().position(|&id| id == pressed_id)?;

        // Save and dim the source item's background directly.
        self.saved_src_bg.set(crate::core::element::with_ct(|ct| {
            ct.style.get(&pressed_id).and_then(|s| s.background)
        }));
        crate::core::element::with_ct_mut(|ct| {
            if let Some(s) = ct.style.get_mut(&pressed_id) {
                s.background = Some(GHOST_DIM);
            }
        });

        self.source.set(Some(src));
        self.target.set(Some(src));
        let anchor = if self.horizontal.get() { abs.x } else { abs.y };
        self.press_anchor.set(anchor);

        self.set_ghost(0.0);

        request_drag_z(pressed_id, true);

        Some(src)
    }

    /// Call from the widget's `register_drag_update` handler.
    /// `abs` is the global cursor position.
    /// Computes the target index from the cursor's displacement relative to the
    /// press position (screen-space relative — works correctly with outer scroll).
    /// Returns the computed target index, or `None` if no drag is in progress.
    pub fn update(&self, abs: Point) -> Option<usize> {
        let src = self.source.get()?;

        let horiz = self.horizontal.get();
        let cursor = if horiz { abs.x } else { abs.y };
        let delta = cursor - self.press_anchor.get();
        self.set_ghost(delta);

        // ── Compute target index (relative to source) ──
        let sz = self.item_size.get();
        let raw = src as f32 + delta / sz;
        let tgt_idx = (if raw < 0.0 { raw - 0.5 } else { raw + 0.5 }) as isize;
        let len = self.item_ids.borrow().len() as isize;
        let new_tgt = tgt_idx.max(0).min(len.saturating_sub(1)) as usize;

        // ── Drop-target highlight via DRAG_OVER flag ──
        let old_tgt = self.target.get();
        if old_tgt != Some(new_tgt) {
            if let Some(old) = old_tgt {
                if let Some(&eid) = self.item_ids.borrow().get(old) {
                    dirty_registry::set_state(eid, StateFlags::DRAG_OVER, false);
                }
            }
            if let Some(&eid) = self.item_ids.borrow().get(new_tgt) {
                dirty_registry::set_state(eid, StateFlags::DRAG_OVER, true);
            }
        }
        self.target.set(Some(new_tgt));

        Some(new_tgt)
    }

    /// Call from the widget's `register_drag_end` handler.
    /// Resets visuals and fires `on_reorder` if src ≠ dst.
    /// Returns `Some((src, dst))` when a reorder was committed.
    pub fn end(&self) -> Option<(usize, usize)> {
        let src = self.source.get()?;
        let dst = self.target.get()?;
        self.source.set(None);
        self.target.set(None);

        // ── Reset visuals ──
        // Clear DRAG_OVER on the target
        if let Some(&eid) = self.item_ids.borrow().get(dst) {
            dirty_registry::set_state(eid, StateFlags::DRAG_OVER, false);
        }
        self.reset_ghost(src);

        if src == dst {
            return None;
        }

        if let Some(ref cb) = *self.on_reorder.borrow() {
            cb(src, dst);
        }
        let ids = self.item_ids.borrow();
        if let Some(&eid) = ids.get(src) {
            request_drag_z(eid, false);
        }
        Some((src, dst))
    }

    // ── Accessors ──────────────────────────────────────────────────

    pub fn source(&self) -> Option<usize> {
        self.source.get()
    }
    pub fn target(&self) -> Option<usize> {
        self.target.get()
    }

    /// Forcibly reset internal state (e.g., drag was cancelled externally).
    pub fn cancel(&self) {
        if let Some(src) = self.source.get() {
            if let Some(dst) = self.target.get() {
                if let Some(&eid) = self.item_ids.borrow().get(dst) {
                    dirty_registry::set_state(eid, StateFlags::DRAG_OVER, false);
                }
            }
            self.reset_ghost(src);
            let ids = self.item_ids.borrow();
            if let Some(&eid) = ids.get(src) {
                request_drag_z(eid, false);
            }
        }
        self.source.set(None);
        self.target.set(None);
    }

    // ── Internal helpers ───────────────────────────────────────────

    fn set_ghost(&self, offset: f32) {
        let src = match self.source.get() {
            Some(s) => s,
            None => return,
        };
        let ids = self.item_ids.borrow();
        let eid = match ids.get(src) {
            Some(&id) => id,
            None => return,
        };
        let offs = self.offsets.borrow();
        if let Some(off_cell) = offs.get(src) {
            let v = if self.horizontal.get() {
                Vec2::new(offset, 0.0)
            } else {
                Vec2::new(0.0, offset)
            };
            off_cell.set(v);
        }
        // position_offset Cell::set doesn't auto-mark dirty — the ghost
        // needs a repaint every frame while the cursor moves.
        // Also bump surface_gen so the scene cache knows the visual changed
        // (the cache keys only include gen/scoll, not position_offset).
        dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
        dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
        dirty_registry::bump_surface_gen_remote(eid);
    }

    fn reset_ghost(&self, src: usize) {
        let ids = self.item_ids.borrow();
        let eid = match ids.get(src) {
            Some(&id) => id,
            None => return,
        };
        let offs = self.offsets.borrow();
        if let Some(off_cell) = offs.get(src) {
            off_cell.set(Vec2::ZERO);
        }
        // Restore the source item's original background.
        crate::core::element::with_ct_mut(|ct| {
            if let Some(s) = ct.style.get_mut(&eid) {
                s.background = self.saved_src_bg.get();
            }
        });
        dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
        dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
    }
}

impl Default for ReorderController {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ReorderController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReorderController")
            .field("items", &self.item_ids.borrow().len())
            .field("source", &self.source.get())
            .field("target", &self.target.get())
            .finish()
    }
}
