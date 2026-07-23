//! Focus types shared by FocusManager and the event system.
//!
//! State management moved to `focus_manager.rs` (per-window `FocusManager`).

use crate::core::dirty_registry;
use crate::core::ElementId;

/// Why a focus change occurred. Exposed to widgets via `on_focus_in(reason)`
/// and `on_focus_out(reason)` so they can react differently (e.g. select all
/// on keyboard focus but not on mouse focus).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusReason {
    TabNavigation,
    PointerClick,
    Programmatic,
    WindowActivation,
}

/// Controls whether focus highlights (focus rings) are shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusHighlightMode {
    Traditional,
    Touch,
}

impl FocusHighlightMode {
    pub fn show_focus_ring(self) -> bool {
        matches!(self, FocusHighlightMode::Traditional)
    }
}

/// What happens when Tab reaches the edge of a focus scope.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TraversalEdgeBehavior {
    /// Cycle within the scope (Tab wraps around).
    Wrap,
    /// Move to the next element in the parent scope.
    #[default]
    Leave,
    /// Stay at the edge (no movement).
    Stop,
}

// ═══════════════════════════════════════════════════════════════════
// Modal scope stack — per-window (AppContext extension), for
// widget-level push/pop. FocusManager mirrors and checks this stack
// during navigation. (audit 2026-07-18 multi-window pass: window B's
// Tab traversal must not be trapped by window A's modal.)
// ═══════════════════════════════════════════════════════════════════

/// Per-window modal-scope stack.
#[derive(Default)]
pub(crate) struct ModalScopeDomain {
    stack: std::cell::RefCell<Vec<ModalScopeEntry>>,
}

fn scope_domain() -> std::rc::Rc<ModalScopeDomain> {
    crate::core::app_context::current_app().extension::<ModalScopeDomain>()
}

struct ModalScopeEntry {
    root: ElementId,
    edge_behavior: TraversalEdgeBehavior,
}

/// Push a modal-level focus scope. Tab navigation will be restricted to
/// descendants of `root`.
pub fn push_modal_scope(root: ElementId, edge_behavior: TraversalEdgeBehavior) {
    scope_domain().stack.borrow_mut().push(ModalScopeEntry {
        root,
        edge_behavior,
    });
}

/// Pop the innermost modal-level focus scope. Returns the popped root.
pub fn pop_modal_scope() -> Option<ElementId> {
    scope_domain().stack.borrow_mut().pop().map(|s| s.root)
}

/// Remove every modal scope owned by `root`, wherever it sits in the
/// stack. Safe to call unconditionally from unmount cleanup: unlike
/// [`pop_modal_scope`], it never disturbs scopes pushed by other overlays
/// (audit 2026-07-16 round 3 — unmount cleanup used to blind-pop and
/// could evict an unrelated overlay's scope).
pub fn remove_modal_scopes_of(root: ElementId) {
    scope_domain().stack.borrow_mut().retain(|s| s.root != root);
}

/// Returns the innermost modal-level focus scope's root, if any.
pub fn current_modal_scope_root() -> Option<ElementId> {
    scope_domain().stack.borrow().last().map(|s| s.root)
}

/// Returns true if `eid` is a descendant of the innermost modal scope root
/// (or if no modal scope is active).
///
/// Automatically removes stale scopes whose root element no longer exists.
pub fn is_in_modal_scope(eid: ElementId) -> bool {
    let dom = scope_domain();
    let mut sb = dom.stack.borrow_mut();
    while let Some(scope) = sb.last() {
        if dirty_registry::parent_of(scope.root).is_some() {
            break;
        }
        sb.pop();
    }
    if let Some(scope) = sb.last() {
        dirty_registry::is_descendant_of(eid, scope.root)
    } else {
        true
    }
}

/// Returns the edge behavior of the innermost modal scope, or default.
pub fn modal_edge_behavior() -> TraversalEdgeBehavior {
    scope_domain()
        .stack
        .borrow()
        .last()
        .map(|s| s.edge_behavior)
        .unwrap_or_default()
}
