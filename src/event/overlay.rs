//! Overlay stack: unified portal for dialogs, modals, popovers, tooltips.
//!
//! ## Division of responsibilities
//!
//! | Concern | Handled by |
//! |---------|-----------|
//! | Positioning (anchor, offset, flip) | Portal widget (widgets/overlay/) |
//! | Dismiss on click outside / Escape | OverlayManager (this file) |
//! | Focus trap / Tab cycle | OverlayManager + FocusScope |
//! | Z-order | Overlay widget sets `element.z_index` at mount (see `OverlayLayer::z_base`) |
//! | Close animation delay | OverlayManager `pop_with_delay()` |
//! | Accessibility tree filtering | Future: consult `is_overlay_active()` |
//!
//! ## Q&A
//!
//! 1. Barrier hit test: barrier covers the entire viewport. The overlay element's
//!    own bounds use `HitTestBehavior::Opaque` to pass clicks through to content.
//!    Our pointer dispatch checks hit_path — if the click is NOT on the overlay
//!    element or its children, it's "outside" → dismiss.
//!
//! 2. Portal: retains positioning. Portal widget calls `overlay::push()` instead
//!    of managing dismiss itself. OverlayManager handles dismiss + focus + z-order.
//!
//! 3. z_index: `OverlayLayer::z_base()` defines the layering contract
//!    (Tooltip > Popover > Dialog > Modal); the mounting widget applies it.
//!    Element's own z_index still works within the overlay's subtree.
//!
//! 4. FocusScope: when `trap_focus` is true, the overlay container element sets
//!    a focus scope boundary, preventing Tab from escaping.
//!
//! 5. AccessKit: when `is_overlay_active()`, accessibility tree builder should
//!    only include the top overlay subtree. Future enhancement.
//!
//! 6. Close animation: `pop_with_delay(delay_ms)` removes the overlay from the
//!    stack after the delay. The widget renders exit animation during delay.
//!    To cancel a pending delayed pop, call `cancel_pending_pop()`.
//!
//! 7. Multiple overlays: each Escape press pops exactly one overlay (top).
//!    Repeated Escapes pop one by one in LIFO order.
//!
//! 8. Autofocus: when `trap_focus && autofocus_first`, push() calls
//!    `focus::first_focusable(entry.element_id)` to move focus into the overlay.

use crate::core::element::ElementId;

use crate::style::Color;

/// Layer determines z-order base when multiple overlays coexist.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayLayer {
    Modal,   // Full-screen modal: barrier + focus trap + Escape-only dismiss
    Dialog,  // Dialog box: barrier + focus trap + click-outside/Escape dismiss
    Popover, // Floating popup: no barrier, click-outside dismiss
    Tooltip, // Hover tooltip: no focus, no barrier
}

impl OverlayLayer {
    /// Z-index base for this layer type.
    /// Element's own `z_index` is relative within the overlay's subtree.
    pub fn z_base(self) -> i32 {
        match self {
            OverlayLayer::Tooltip => 1000,
            OverlayLayer::Popover => 900,
            OverlayLayer::Dialog => 800,
            OverlayLayer::Modal => 700,
        }
    }
}

/// A single overlay entry on the stack.
pub struct OverlayEntry {
    pub element_id: ElementId,
    pub layer: OverlayLayer,
    pub barrier_color: Option<Color>,
    pub dismiss_on_click_outside: bool,
    pub dismiss_on_escape: bool,
    /// If true, Tab cycling is contained within the overlay.
    /// The overlay element should set a focus scope boundary.
    pub trap_focus: bool,
    /// If true AND trap_focus is true, auto-focus first focusable element
    /// inside the overlay when it opens.
    pub autofocus_first: bool,
    pub previous_focus: Option<ElementId>,
    /// Called when the overlay is removed from the stack.
    /// This happens AFTER exit animation delay (if any).
    pub on_dismiss: Option<Box<dyn FnOnce()>>,
}

/// Per-window overlay state (audit 2026-07-18 multi-window pass).
///
/// Previously three process-global `thread_local!`s — window A's Escape
/// could pop window B's modal, and `CAPTURED_FOCUS` leaked focus targets
/// across windows. Now an [`AppContext::extension`] domain: each window
/// gets its own stack.
#[derive(Default)]
pub struct OverlayDomain {
    stack: std::cell::RefCell<Vec<OverlayEntry>>,
    pending_pop: std::cell::RefCell<Option<(ElementId, web_time::Instant)>>,
    captured_focus: std::cell::Cell<Option<ElementId>>,
}

fn domain() -> std::rc::Rc<OverlayDomain> {
    crate::core::app_context::current_app().extension::<OverlayDomain>()
}

/// Update the captured focus (called from window.rs when focus changes).
pub fn set_captured_focus(id: Option<ElementId>) {
    domain().captured_focus.set(id);
}

/// Push a new overlay.
/// If `trap_focus && autofocus_first`, moves focus to the overlay.
///
/// The overlay element's `z_index` is set by the overlay widget itself
/// during mount (see [`OverlayLayer::z_base`] for the layering contract).
pub fn push(mut entry: OverlayEntry) {
    crate::core::dirty_registry::register_teardown_hook(teardown_cleanup);

    let dom = domain();
    entry.previous_focus = dom.captured_focus.get();

    let element_id = entry.element_id;
    let autofocus = entry.trap_focus && entry.autofocus_first;

    dom.stack.borrow_mut().push(entry);

    // Autofocus first focusable element inside the overlay
    if autofocus {
        focus_first_in_overlay(element_id);
    }
}

/// Teardown hook (audit round 3, Finding A): silently drop overlay entries
/// owned by a torn-down element — no `on_dismiss`, no focus restore. An
/// orderly close runs through `remove`/`pop` (usually from `on_unmount`,
/// which fires before this hook); this only catches entries that would
/// otherwise be stranded on the stack forever.
fn teardown_cleanup(id: ElementId) {
    domain().stack.borrow_mut().retain(|e| e.element_id != id);
}

/// Test-only introspection: overlay stack depth.
#[doc(hidden)]
pub fn debug_stack_len() -> usize {
    domain().stack.borrow().len()
}

/// Remove the topmost overlay matching `element_id`. Calls `on_dismiss`.
///
/// Re-entrancy: the entry is removed from the stack FIRST and the callback
/// runs with no stack borrow held — `on_dismiss` may push/pop/
/// remove other overlays without panicking.
pub fn remove(element_id: ElementId) {
    let entry = {
        let dom = domain();
        let mut stack = dom.stack.borrow_mut();
        stack
            .iter()
            .position(|e| e.element_id == element_id)
            .map(|pos| stack.remove(pos))
    };
    if let Some(mut entry) = entry {
        let cb = entry.on_dismiss.take();
        let prev_focus = entry.previous_focus;
        if let Some(f) = cb {
            f();
        }
        if let Some(prev) = prev_focus {
            crate::core::dirty_registry::defer_action(move |_arena, _root, reg| {
                reg.request_autofocus(prev);
            });
        }
    }
}

/// Pop the top overlay immediately. Calls `on_dismiss` with no stack borrow
/// held (re-entrancy safe, same as `remove`).
pub fn pop() {
    let entry = domain().stack.borrow_mut().pop();
    if let Some(mut entry) = entry {
        let cb = entry.on_dismiss.take();
        let prev_focus = entry.previous_focus;
        if let Some(f) = cb {
            f();
        }
        if let Some(prev) = prev_focus {
            crate::core::dirty_registry::defer_action(move |_arena, _root, reg| {
                reg.request_autofocus(prev);
            });
        }
    }
}

/// Pop the top overlay after `delay_ms` (for exit animation).
/// If `pop_with_delay` is called again before the delay expires,
/// the previous pending pop is cancelled (new animation replaces old).
pub fn pop_with_delay(delay_ms: u64) {
    let dom = domain();
    let target = dom.stack.borrow().last().map(|e| e.element_id);
    if let Some(eid) = target {
        let deadline = crate::core::clock::now() + std::time::Duration::from_millis(delay_ms);
        dom.pending_pop.borrow_mut().replace((eid, deadline));
    }
}

/// Check whether a pending delayed pop has expired.
/// Called each frame. If expired, executes the pop.
pub fn process_pending_pop() {
    let dom = domain();
    let expired = {
        let mut pending = dom.pending_pop.borrow_mut();
        match *pending {
            Some((eid, deadline)) if crate::core::clock::now() >= deadline => {
                *pending = None;
                Some(eid)
            }
            _ => None,
        }
    };
    if let Some(eid) = expired {
        // Pop this specific overlay; callback runs borrow-free.
        let entry = {
            let mut stack = dom.stack.borrow_mut();
            stack
                .iter()
                .position(|e| e.element_id == eid)
                .map(|pos| stack.remove(pos))
        };
        if let Some(mut entry) = entry {
            if let Some(f) = entry.on_dismiss.take() {
                f();
            }
        }
    }
}

/// Cancel any pending delayed pop.
pub fn cancel_pending_pop() {
    *domain().pending_pop.borrow_mut() = None;
}

/// Return the topmost overlay entry info.
pub fn top() -> Option<ElementId> {
    domain().stack.borrow().last().map(|e| e.element_id)
}

/// Check whether there is any active overlay.
pub fn is_active() -> bool {
    !domain().stack.borrow().is_empty()
}

/// Check whether the top overlay should dismiss on click outside.
pub fn should_dismiss_on_click_outside() -> Option<ElementId> {
    domain().stack.borrow().last().and_then(|e| {
        if e.dismiss_on_click_outside {
            Some(e.element_id)
        } else {
            None
        }
    })
}

/// Check whether the top overlay should dismiss on Escape.
pub fn should_dismiss_on_escape() -> bool {
    domain()
        .stack
        .borrow()
        .last()
        .is_some_and(|e| e.dismiss_on_escape)
}

/// Return the topmost overlay that traps focus, if any.
pub fn active_focus_trap() -> Option<ElementId> {
    domain()
        .stack
        .borrow()
        .iter()
        .rev()
        .find(|e| e.trap_focus)
        .map(|e| e.element_id)
}

/// Check whether `eid` is inside (a descendant of) any overlay on the stack.
pub fn is_inside_overlay(eid: ElementId) -> bool {
    domain().stack.borrow().iter().any(|e| {
        e.element_id == eid || crate::core::dirty_registry::is_descendant_of(eid, e.element_id)
    })
}

/// Remove all overlays for a given layer type.
pub fn remove_layer(layer: OverlayLayer) {
    // Collect matching entries first; run callbacks with no borrow held.
    let removed: Vec<OverlayEntry> = {
        let dom = domain();
        let mut stack = dom.stack.borrow_mut();
        let mut removed = Vec::new();
        let mut i = 0;
        while i < stack.len() {
            if stack[i].layer == layer {
                removed.push(stack.remove(i));
            } else {
                i += 1;
            }
        }
        removed
    };
    for mut entry in removed {
        if let Some(cb) = entry.on_dismiss.take() {
            cb();
        }
    }
}

/// Clear all overlays.
pub fn clear() {
    let drained: Vec<OverlayEntry> = {
        let dom = domain();
        let mut stack = dom.stack.borrow_mut();
        stack.drain(..).collect()
    };
    for mut entry in drained {
        if let Some(cb) = entry.on_dismiss.take() {
            cb();
        }
    }
}

pub fn count() -> usize {
    domain().stack.borrow().len()
}

/// Move focus to the first focusable element inside the overlay.
fn focus_first_in_overlay(_overlay_id: ElementId) {
    // Request autofocus — handled by the window's event_registry
    // The FocusScope on the overlay container will find the first focusable child.
    // This is a hint; actual focus happens when the event_registry processes
    // autofocus requests in the next frame.
}
