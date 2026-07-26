//! Tracking sets for incremental O(k) operations, now exclusively routed
//! through AppContext.resources.* (no thread_local fallback).
//!
//! Each set is populated during element creation/mutation and used by
//! the window/test-harness frame loop for targeted iteration instead of
//! full-tree walks.

use std::collections::HashSet;

use crate::core::id::ElementId;

/// Clean up all tracking entries for a removed element.
pub(crate) fn unregister_element(eid: ElementId) {
    crate::core::app_context::current_app().ecs_unregister_element(eid);
    crate::ecs::active::unregister_all_active(eid);
}

/// ── Drag layout tracking ──
pub(crate) fn register_drag_element(eid: ElementId) {
    crate::core::app_context::current_app().register_drag_element(eid);
}
pub(crate) fn drain_drag_elements() -> HashSet<ElementId> {
    crate::core::app_context::current_app().drain_drag_elements()
}

/// ── Theme reapply tracking ──
pub(crate) fn register_theme_element(eid: ElementId) {
    crate::core::app_context::current_app().register_theme_element(eid);
}
pub(crate) fn drain_theme_elements() -> HashSet<ElementId> {
    crate::core::app_context::current_app().drain_theme_elements()
}

/// ── Pending scroll tracking ──
pub(crate) fn register_pending_scroll(eid: ElementId) {
    crate::core::app_context::current_app().register_pending_scroll(eid);
}
pub(crate) fn unregister_pending_scroll(eid: ElementId) {
    crate::core::app_context::current_app().unregister_pending_scroll(eid);
}
pub(crate) fn pending_scroll_elements() -> Vec<ElementId> {
    crate::core::app_context::current_app().pending_scroll_elements()
}

/// ── Scrollable tracking ──
pub(crate) fn register_scrollable(eid: ElementId) {
    crate::core::app_context::current_app().register_scrollable(eid);
}
pub(crate) fn unregister_scrollable(eid: ElementId) {
    crate::core::app_context::current_app().unregister_scrollable(eid);
}
pub(crate) fn scrollable_elements() -> Vec<ElementId> {
    crate::core::app_context::current_app().scrollable_elements()
}

/// ── A11y changed tracking (for incremental accessibility tree rebuild) ──
pub(crate) fn mark_a11y_changed(eid: ElementId) {
    crate::core::app_context::current_app().mark_a11y_changed(eid);
}
pub(crate) fn drain_a11y_changed() -> HashSet<ElementId> {
    crate::core::app_context::current_app().drain_a11y_changed()
}
#[allow(dead_code)]
pub(crate) fn has_a11y_changed() -> bool {
    crate::core::app_context::current_app().has_a11y_changed()
}

/// ── Mount callback tracking ──
pub(crate) fn register_on_mount(eid: ElementId) {
    crate::core::app_context::current_app().register_on_mount(eid);
}

/// Drain all registered mount callbacks.
pub(crate) fn drain_mount_callbacks() -> Vec<ElementId> {
    crate::core::app_context::current_app().drain_mount_callbacks()
}
