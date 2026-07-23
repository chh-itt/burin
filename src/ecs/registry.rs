//! Tracking sets for incremental O(k) operations, now exclusively routed
//! through AppContext.resources.* (no thread_local fallback).
//!
//! Each set is populated during element creation/mutation and used by
//! the window/test-harness frame loop for targeted iteration instead of
//! full-tree walks.

use std::collections::HashSet;

use crate::core::id::ElementId;

/// Clean up all tracking entries for a removed element.
pub fn unregister_element(eid: ElementId) {
    crate::core::app_context::current_app().ecs_unregister_element(eid);
    crate::ecs::active::unregister_all_active(eid);
}

/// ── Drag layout tracking ──
pub fn register_drag_element(eid: ElementId) {
    crate::core::app_context::current_app().register_drag_element(eid);
}
pub fn drain_drag_elements() -> HashSet<ElementId> {
    crate::core::app_context::current_app().drain_drag_elements()
}

/// ── Theme reapply tracking ──
pub fn register_theme_element(eid: ElementId) {
    crate::core::app_context::current_app().register_theme_element(eid);
}
pub fn drain_theme_elements() -> HashSet<ElementId> {
    crate::core::app_context::current_app().drain_theme_elements()
}

/// ── Pending scroll tracking ──
pub fn register_pending_scroll(eid: ElementId) {
    crate::core::app_context::current_app().register_pending_scroll(eid);
}
pub fn unregister_pending_scroll(eid: ElementId) {
    crate::core::app_context::current_app().unregister_pending_scroll(eid);
}
pub fn pending_scroll_elements() -> Vec<ElementId> {
    crate::core::app_context::current_app().pending_scroll_elements()
}

/// ── Scrollable tracking ──
pub fn register_scrollable(eid: ElementId) {
    crate::core::app_context::current_app().register_scrollable(eid);
}
pub fn unregister_scrollable(eid: ElementId) {
    crate::core::app_context::current_app().unregister_scrollable(eid);
}
pub fn scrollable_elements() -> Vec<ElementId> {
    crate::core::app_context::current_app().scrollable_elements()
}

/// ── A11y changed tracking (for incremental accessibility tree rebuild) ──
pub fn mark_a11y_changed(eid: ElementId) {
    crate::core::app_context::current_app().mark_a11y_changed(eid);
}
pub fn drain_a11y_changed() -> HashSet<ElementId> {
    crate::core::app_context::current_app().drain_a11y_changed()
}
pub fn has_a11y_changed() -> bool {
    crate::core::app_context::current_app().has_a11y_changed()
}

/// ── Mount callback tracking ──
pub fn register_on_mount(eid: ElementId) {
    crate::core::app_context::current_app().register_on_mount(eid);
}

/// Drain all registered mount callbacks.
pub fn drain_mount_callbacks() -> Vec<ElementId> {
    crate::core::app_context::current_app().drain_mount_callbacks()
}
