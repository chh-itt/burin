use crate::core::config::StateFlags;
use crate::core::element::{DirtyFlags, ElementId};
use crate::style::Color;

/// Manages selection state via `StateFlags::CHECKED` for single/multi-selection widgets.
///
/// Widgets declare `StateStyle.checked.background = selected_bg` once during mount.
/// `SelectionBg` handles only the state flag: setting/clearing `CHECKED` and marking items dirty.
///
/// The framework automatically resolves the correct background at paint time
/// via the `StateStyle` priority chain (CHECKED beats HOVERED).
///
/// Usage:
/// ```ignore
/// // Phase 1 — during mount, declare state styles on each item:
/// el.with_state_style(|ss| {
///     ss.hovered.background = Some(hover_bg);
///     ss.pressed.background = Some(pressed_bg);
///     ss.checked.background = Some(selected_bg);
///     ss.focused.background = Some(focus_bg);
/// });
///
/// // Phase 2 — after mount, wrap item ids:
/// let sel_bg = SelectionBg::new(item_ids);
///
/// // On selection change:
/// sel_bg.set_selected(idx);
/// ```
pub struct SelectionBg {
    pub ids: Vec<ElementId>,
}

impl SelectionBg {
    pub fn new(ids: Vec<ElementId>) -> Self {
        Self { ids }
    }

    /// Select a single slot (clears CHECKED on all others).
    pub fn set_selected(&self, idx: usize) {
        for (i, &eid) in self.ids.iter().enumerate() {
            let on = i == idx;
            crate::core::dirty_registry::set_state(eid, StateFlags::CHECKED, on);
            dirty(eid);
        }
    }

    /// Sync: set CHECKED on items in `selected`, clear on others.
    pub fn sync(&self, selected: &std::collections::HashSet<usize>) {
        for (i, &eid) in self.ids.iter().enumerate() {
            let on = selected.contains(&i);
            crate::core::dirty_registry::set_state(eid, StateFlags::CHECKED, on);
            dirty(eid);
        }
    }

    /// Slot-domain sync: `on(pi)` decides CHECKED for pool slot `pi`.
    ///
    /// Virtualized widgets index `ids` by POOL slot while selection lives
    /// in DATA space — callers translate per slot (audit round 6:
    /// `slot_to_virtual[pi]`), e.g.
    /// `sel_bg.sync_by(|pi| selected.contains(&stv[pi].get()))`.
    pub fn sync_by(&self, on: impl Fn(usize) -> bool) {
        for (i, &eid) in self.ids.iter().enumerate() {
            crate::core::dirty_registry::set_state(eid, StateFlags::CHECKED, on(i));
            dirty(eid);
        }
    }

    pub fn mark(&self, idx: usize) {
        if let Some(&eid) = self.ids.get(idx) {
            dirty(eid);
        }
    }

    pub fn mark_all(&self) {
        for &eid in &self.ids {
            dirty(eid);
        }
    }
}

/// ── Public helpers for List/Table/Tree ──────────────────────────

/// Set keyboard navigation highlight on a list item.
///
/// Sets `StateFlags::FOCUSED` on the target item (clears from any
/// other items in the group).  The item's `StateStyle.focused.*`
/// provides the visual.  To avoid the auto focus ring, items should
/// typically set `outline_width` to a small but non-zero value
/// (e.g. `0.01`) so the framework skips focus-ring rendering.
///
/// Call this during keyboard navigation (up/down/home/end), in your
/// `on_action` / `on_key_down` handler, and in the container's
/// `on_focus_in` to initialise the highlight on Tab entry.
pub fn set_item_highlight(ids: &[ElementId], old_idx: Option<usize>, new_idx: usize) {
    if let Some(old) = old_idx {
        if let Some(&eid) = ids.get(old) {
            crate::core::dirty_registry::set_state(eid, StateFlags::FOCUSED, false);
            dirty(eid);
        }
    }
    if let Some(&eid) = ids.get(new_idx) {
        crate::core::dirty_registry::set_state(eid, StateFlags::FOCUSED, true);
        // Suppress auto focus ring — highlight is via StateStyle.focused.background.
        crate::core::element::with_ct_mut(|ct| {
            if let Some(s) = ct.style.get_mut(&eid) {
                s.outline_width = -1.0;
            }
        });
        dirty(eid);
    }
}

/// Sync both selection (CHECKED) and keyboard focus (FOCUSED)
/// for a list-like widget.  Call this in `on_focus_in` (when the
/// container receives keyboard focus) and after any selection change
/// that should be reflected immediately.
///
/// Priority: if `selected` is `Some`, that item gets CHECKED AND
/// the highlight.  Otherwise only the `focused` item is highlighted.
pub fn sync_list_selection_focus(sel_bg: &SelectionBg, selected: Option<usize>, focused: usize) {
    // Clear CHECKED on all items first, then set on selected if any
    if let Some(sel) = selected {
        sel_bg.set_selected(sel);
    } else {
        // No selection — clear all CHECKEDs
        for &eid in &sel_bg.ids {
            crate::core::dirty_registry::set_state(eid, StateFlags::CHECKED, false);
        }
    }

    // Set FOCUSED on the keyboard-focused item, suppress auto focus ring.
    for (i, &eid) in sel_bg.ids.iter().enumerate() {
        let on = i == focused;
        crate::core::dirty_registry::set_state(eid, StateFlags::FOCUSED, on);
        if on {
            crate::core::element::with_ct_mut(|ct| {
                if let Some(s) = ct.style.get_mut(&eid) {
                    s.outline_width = -1.0;
                }
            });
        }
    }
    dirty_set(&sel_bg.ids);
}

/// Apply or remove the disabled style on a single list-item element.
///
/// Sets/clears `StateFlags::DISABLED` and derives the disabled
/// foreground from the item's base foreground at 38% opacity
/// (Flutter's standard disabled opacity).
///
/// When disabled, the element suppresses all other interaction states
/// (hover, press, focus) — per Flutter's `WidgetState.disabled` semantics.
pub fn set_item_disabled(eid: ElementId, disabled: bool) {
    crate::core::dirty_registry::set_state(eid, StateFlags::DISABLED, disabled);
    crate::core::element::with_ct_mut(|ct| {
        if let Some(s) = ct.style.get_mut(&eid) {
            let ss = s
                .state_style
                .get_or_insert_with(crate::style::StateStyle::default);
            if disabled {
                // Flutter: disabled text = onSurface * 0.38 opacity
                let fg = s.foreground.unwrap_or(Color::rgba8(150, 150, 165, 255));
                ss.disabled.foreground = Some(fg.with_alpha(fg.a * 0.38));
                ss.disabled.background = s.background;
            } else {
                ss.disabled.foreground = None;
                ss.disabled.background = None;
            }
        }
    });
    dirty(eid);
}

/// Quick guard for click/focus handlers: skip if the item is disabled.
pub fn is_item_disabled(eid: ElementId) -> bool {
    crate::core::dirty_registry::has_state(eid, StateFlags::DISABLED)
}

// ── internal ──

fn dirty(eid: ElementId) {
    crate::core::dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
    crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
    crate::core::dirty_registry::bump_subtree_gen(eid);
    crate::core::dirty_registry::bump_surface_gen_remote(eid);
}

fn dirty_set(ids: &[ElementId]) {
    for &eid in ids {
        dirty(eid);
    }
}
