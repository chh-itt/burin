//! Per-frame active-set tracking — replaces O(k) full-table scans during
//! the prepass with O(active) targeted iteration.
//!
//! ## How it works
//!
//! 1. **Register** — Widgets / platform code call `register_active(eid, tag)`
//!    when an element begins needing per-frame work (e.g. a text input gains
//!    focus → `CursorBlink`; a calendar starts animating → `FrameTick`).
//!
//! 2. **Process** — The prepass calls `drain_active(tag)` which atomically
//!    takes the set *and* re-fills it with elements that still need work
//!    (done by the processor).
//!
//! 3. **Cleanup** — `unregister_element` clears all tags for a removed
//!    element.  No stale entries accumulate.
//!
//! ## Benefits
//!
//! Idle frames (no focused input, no animation) iterate **zero** active
//! elements instead of scanning every `CursorComponent` / `LifecycleComponent`
//! entry in the component tables.  Measured: ~286 µs → ~5 µs for the prepass
//! on a 2000-element stress test.

use crate::core::id::ElementId;
use std::collections::HashSet;

/// Active-work tags.  A single element can carry multiple tags simultaneously.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ActiveTag {
    /// Cursor blink — focused text input.
    CursorBlink,
    /// General-purpose per-frame callback (via `LifecycleComponent::frame_tick`).
    FrameTick,
    /// Exit animation in progress.
    ExitAnimation,
    /// Third-party extension.  Use `ActiveTag::Custom("my-plugin")`.
    Custom(&'static str),
}

/// Register an element as needing per-frame processing for the given tag.
/// **Idempotent** — calling twice with the same tag for the same element is
/// a harmless no-op.
///
/// Element teardown automatically removes all tags; callers do **not** need
/// to unregister on unmount (though eager `unregister_active` is cheaper than
/// the idempotent check during the prepass).
pub fn register_active(eid: ElementId, tag: ActiveTag) {
    crate::core::app_context::current_app().register_active(eid, tag);
}

/// Remove an element from the active-set for the given tag.
pub fn unregister_active(eid: ElementId, tag: ActiveTag) {
    crate::core::app_context::current_app().unregister_active(eid, tag);
}

/// Check whether an element is currently active for the given tag.
pub fn is_active(eid: ElementId, tag: ActiveTag) -> bool {
    crate::core::app_context::current_app().is_active(eid, tag)
}

/// Drain (take) all elements registered for `tag`, returning them as a
/// `HashSet`.  The internal set is *cleared* — it is the caller's
/// responsibility to re-register elements that still need processing
/// next frame.
pub(crate) fn drain_active(tag: ActiveTag) -> HashSet<ElementId> {
    crate::core::app_context::current_app().drain_active(tag)
}

/// Remove every active-set entry for an element (called during teardown).
pub(crate) fn unregister_all_active(eid: ElementId) {
    crate::core::app_context::current_app().unregister_all_active(eid);
}
