//! FocusManager — unified per-window focus controller.
//!
//! Owned by `WindowState`.  Tracks the focused element, scope stack
//! (modal/popover traps), deferred blurs (IME), autofocus queue, and
//! provides focus navigation (`focus_next` / `focus_prev` / `focus_in_direction`).
//!
//! The manager itself is a pure state machine. The full transfer
//! orchestration (event firing, `StateFlags::FOCUSED`, overlay capture,
//! scroll-into-view) lives in [`transfer_focus`] below, shared by the
//! window and the TestHarness so the test path IS the production path
//! (audit 2026-07-16 round 4, SEAM-2 unification). Platform follow-ups
//! (IME enable) are injected by the caller via `FrameHook`.

use crate::core::config::StateFlags;
use crate::core::dirty_registry;
use crate::core::element::{ElementArena, ElementId};
use crate::event::focus::{FocusHighlightMode, TraversalEdgeBehavior};
use crate::event::focus_traversal::{Direction, TraversalPolicy};
use crate::event::{EventRegistry, FocusReason};

/// Transfer focus to `new_id`: fires `focus_out` on the previously
/// focused element, flips `StateFlags::FOCUSED` on both, fires
/// `focus_in`, updates the manager and the overlay captured-focus slot,
/// and scrolls the new element into view.
///
/// Shared by `WindowState::transfer_focus` (which adds IME enable on
/// top) and the TestHarness / frame-driver SEAM-2 path.
pub fn transfer_focus(
    arena: &mut ElementArena,
    events: &mut EventRegistry,
    focus: &mut FocusManager,
    new_id: ElementId,
    reason: FocusReason,
) {
    if let Some(old_id) = focus.focused() {
        if let Some(el) = arena.get_mut(old_id) {
            el.set_state_dirty(StateFlags::FOCUSED, false);
            el.last_focus_reason.set(Some(reason));
        }
        events.fire_focus_out(old_id, reason);
    }
    if let Some(el) = arena.get_mut(new_id) {
        el.set_state_dirty(StateFlags::FOCUSED, true);
        el.last_focus_reason.set(Some(reason));
    }
    events.fire_focus_in(new_id, reason);
    focus.set_focused(Some(new_id));
    if !crate::event::overlay::is_inside_overlay(new_id) {
        crate::event::overlay::set_captured_focus(Some(new_id));
    }
    if !matches!(reason, FocusReason::Programmatic) {
        scroll_focused_into_view(arena, new_id);
    }
}

/// Scroll the nearest scrollable ancestor so `focused_id` is visible
/// (30 px padding). Pure arena + dirty-registry work; shared by the
/// window and the frame-driver SEAM-2 path.
pub fn scroll_focused_into_view(arena: &mut ElementArena, focused_id: ElementId) {
    const PADDING: f32 = 30.0;
    let mut scroll_ancestor: Option<ElementId> = None;
    let mut cur = dirty_registry::parent_of(focused_id);
    while let Some(pid) = cur {
        if arena.comp_scroll(pid).is_some() {
            scroll_ancestor = Some(pid);
            break;
        }
        cur = dirty_registry::parent_of(pid);
    }
    let sid = match scroll_ancestor {
        Some(s) => s,
        None => return,
    };

    let viewport = match arena.get(sid) {
        Some(el) => el.screen_bounds,
        None => return,
    };
    let so_cell = match arena.comp_scroll(sid) {
        Some(s) => s.scroll_offset.clone(),
        None => return,
    };
    let el_bounds = match arena.get(focused_id) {
        Some(el) => el.screen_bounds,
        None => return,
    };

    let max_vy = arena
        .comp_scroll(sid)
        .map(|s| (s.content_bounds.get().height - viewport.height).max(0.0))
        .unwrap_or(0.0);
    let so = so_cell.get();
    let mut new_so = so;

    // screen_bounds is the taffy-computed static position in content space.
    // Scroll is a paint transform: rendered_y = content_y - so.
    // Valid so range: [lower_limit, upper_limit].
    //   upper_limit = max(content_top - PADDING, 0)  – keep element below top padding
    //   lower_limit = max(content_bottom - viewport_h + PADDING, 0) – keep element above bottom padding
    // If so is outside this range, clamp to nearest bound.
    // Absolute "=" semantics: same-row elements share the same content_bottom → same lower_limit → no double-scroll.
    if el_bounds.height < viewport.height {
        let content_top = el_bounds.y - viewport.y;
        let content_bottom = content_top + el_bounds.height;
        let viewport_h = viewport.height;
        let upper_limit = (content_top - PADDING).max(0.0);
        let lower_limit = (content_bottom - viewport_h + PADDING).max(0.0).min(max_vy);
        if so.y > upper_limit {
            new_so.y = upper_limit.min(max_vy);
        } else if so.y < lower_limit {
            new_so.y = lower_limit;
        }
    }
    if el_bounds.width < viewport.width {
        let max_vx = arena
            .comp_scroll(sid)
            .map(|s| (s.content_bounds.get().width - viewport.width).max(0.0))
            .unwrap_or(0.0);
        let content_left = el_bounds.x - viewport.x;
        let content_right = content_left + el_bounds.width;
        let viewport_w = viewport.width;
        let upper_limit = (content_left - PADDING).max(0.0);
        let lower_limit = (content_right - viewport_w + PADDING).max(0.0).min(max_vx);
        if so.x > upper_limit {
            new_so.x = upper_limit.min(max_vx);
        } else if so.x < lower_limit {
            new_so.x = lower_limit;
        }
    }

    if new_so != so {
        so_cell.set(new_so);
        dirty_registry::spatial_update_scroll(sid, new_so.x, new_so.y);
        if let Some(el) = arena.get_mut(sid) {
            el.mark_repaint();
        }
    }
}

/// Per-window focus controller.
pub struct FocusManager {
    focused: Option<ElementId>,
    /// Stack of active focus scopes (innermost = last).
    scope_stack: Vec<ScopeEntry>,
    /// Deferred blur targets (IME composition — one-frame lag).
    deferred_blurs: Vec<ElementId>,
    /// Pending autofocus requests.
    pending_autofocus: Vec<ElementId>,
    /// Focus-ring visibility mode.
    highlight_mode: FocusHighlightMode,
}

struct ScopeEntry {
    root: ElementId,
    saved_focus: Option<ElementId>,
    edge_behavior: TraversalEdgeBehavior,
    policy: Option<Box<dyn TraversalPolicy>>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            focused: None,
            scope_stack: Vec::new(),
            deferred_blurs: Vec::new(),
            pending_autofocus: Vec::new(),
            highlight_mode: FocusHighlightMode::Traditional,
        }
    }

    // ── current focus ────────────────────────────────────────────

    pub fn focused(&self) -> Option<ElementId> {
        self.focused
    }
    pub fn set_focused(&mut self, id: Option<ElementId>) {
        self.focused = id;
    }

    // ── deferred blur (IME) ────────────────────────────────────

    /// True when at least one blur is deferred (replaces `FocusState::is_ime_blur_pending`).
    pub fn has_deferred_blurs(&self) -> bool {
        !self.deferred_blurs.is_empty()
    }

    /// Defer a blur by one frame.  Used when a TextInput with active IME
    /// composition is clicked away from.
    pub fn defer_blur(&mut self, old_id: ElementId) {
        self.deferred_blurs.push(old_id);
    }

    /// Drain all deferred blurs (called from `about_to_wait`).
    pub fn drain_deferred_blurs(&mut self) -> Vec<ElementId> {
        std::mem::take(&mut self.deferred_blurs)
    }

    // ── highlight mode ──────────────────────────────────────────

    pub fn highlight_mode(&self) -> FocusHighlightMode {
        self.highlight_mode
    }
    pub fn set_highlight_mode(&mut self, mode: FocusHighlightMode) {
        self.highlight_mode = mode;
    }

    // ── scope (modal / popover) ────────────────────────────────

    pub fn push_scope(&mut self, root: ElementId, edge_behavior: TraversalEdgeBehavior) {
        self.scope_stack.push(ScopeEntry {
            root,
            saved_focus: self.focused,
            edge_behavior,
            policy: None,
        });
    }

    pub fn pop_scope(&mut self) -> Option<ElementId> {
        let popped = self.scope_stack.pop();
        if let Some(ref scope) = popped {
            self.focused = scope.saved_focus;
        }
        popped.map(|s| s.root)
    }

    pub fn set_scope_policy(&mut self, scope_root: ElementId, policy: Box<dyn TraversalPolicy>) {
        if let Some(scope) = self
            .scope_stack
            .iter_mut()
            .rev()
            .find(|s| s.root == scope_root)
        {
            scope.policy = Some(policy);
        }
    }

    pub fn current_scope_root(&self) -> Option<ElementId> {
        self.scope_stack.last().map(|s| s.root)
    }

    /// Returns true if the element is within the innermost scope (or no scope active).
    pub fn is_in_current_scope(&self, eid: ElementId) -> bool {
        if let Some(scope) = self.scope_stack.last() {
            dirty_registry::is_descendant_of(eid, scope.root)
        } else {
            true
        }
    }

    /// Returns true if the element is within ANY active scope.
    pub fn is_in_any_scope(&self, eid: ElementId) -> bool {
        self.scope_stack
            .iter()
            .any(|s| dirty_registry::is_descendant_of(eid, s.root))
    }

    /// Combined scope + modal + disabled + visibility check for navigation candidates.
    fn is_focusable_in_scope(&self, _arena: &ElementArena, eid: ElementId) -> bool {
        if !self.is_in_current_scope(eid) {
            return false;
        }
        if !crate::event::focus::is_in_modal_scope(eid) {
            return false;
        }
        if dirty_registry::has_state(eid, StateFlags::DISABLED) {
            return false;
        }
        dirty_registry::is_visible_chain_fast(eid)
    }

    // ── autofocus ───────────────────────────────────────────────

    pub fn queue_autofocus(&mut self, eid: ElementId) {
        self.pending_autofocus.push(eid);
    }

    pub fn drain_autofocus(&mut self) -> Vec<ElementId> {
        std::mem::take(&mut self.pending_autofocus)
    }

    // ── focus order ─────────────────────────────────────────────

    fn focus_order(&self, arena: &ElementArena) -> Vec<ElementId> {
        let scope = self.scope_stack.last();
        if let Some(s) = scope {
            if let Some(ref policy) = s.policy {
                return policy.sorted(arena, s.root);
            }
        }
        dirty_registry::ensure_focus_order(arena)
    }

    // ── navigation ──────────────────────────────────────────────

    pub fn focus_next(&mut self, arena: &ElementArena) -> Option<ElementId> {
        let order = self.focus_order(arena);
        let len = order.len().max(1);
        let edge = self
            .scope_stack
            .last()
            .map(|s| s.edge_behavior)
            .unwrap_or(TraversalEdgeBehavior::Wrap);

        let next_idx = match self.focused {
            Some(id) => order
                .iter()
                .position(|&eid| eid == id)
                .map_or(0, |i| (i + 1) % len),
            None => 0,
        };
        let mut idx = next_idx;
        for _ in 0..len {
            if let Some(&next_id) = order.get(idx) {
                if self.is_focusable_in_scope(arena, next_id) {
                    return Some(next_id);
                }
            }
            idx = (idx + 1) % len;
        }
        if matches!(edge, TraversalEdgeBehavior::Wrap) && self.focused.is_some() {
            for idx in 0..len {
                if let Some(&next_id) = order.get(idx) {
                    if self.is_focusable_in_scope(arena, next_id) {
                        return Some(next_id);
                    }
                }
            }
        }
        None
    }

    pub fn focus_prev(&mut self, arena: &ElementArena) -> Option<ElementId> {
        let order = self.focus_order(arena);
        let len = order.len().max(1);
        let current_idx = order
            .iter()
            .position(|&eid| Some(eid) == self.focused)
            .unwrap_or(0);
        let edge = self
            .scope_stack
            .last()
            .map(|s| s.edge_behavior)
            .unwrap_or(TraversalEdgeBehavior::Wrap);
        let mut idx = if current_idx == 0 {
            len - 1
        } else {
            current_idx - 1
        };
        for _ in 0..len {
            if let Some(&prev_id) = order.get(idx) {
                if self.is_focusable_in_scope(arena, prev_id) {
                    return Some(prev_id);
                }
            }
            idx = if idx == 0 { len - 1 } else { idx - 1 };
        }
        if matches!(edge, TraversalEdgeBehavior::Wrap) && self.focused.is_some() {
            for idx in (0..len).rev() {
                if let Some(&prev_id) = order.get(idx) {
                    if self.is_focusable_in_scope(arena, prev_id) {
                        return Some(prev_id);
                    }
                }
            }
        }
        None
    }

    pub fn focus_in_direction(
        &mut self,
        arena: &ElementArena,
        direction: Direction,
    ) -> Option<ElementId> {
        let focused = self.focused?;
        let scope = self.scope_stack.last();
        if let Some(s) = scope {
            if let Some(ref policy) = s.policy {
                return policy.in_direction(arena, focused, direction);
            }
        }
        let candidates: Vec<ElementId> = dirty_registry::ensure_focus_order(arena)
            .into_iter()
            .filter(|&eid| {
                eid != focused
                    && self.is_focusable_in_scope(arena, eid)
                    && dirty_registry::bounds_of(eid).is_some()
            })
            .collect();
        crate::event::focus_traversal::nearest_in_direction(arena, focused, &candidates, direction)
    }

    // ── lifecycle ───────────────────────────────────────────────

    pub fn clear(&mut self) {
        self.focused = None;
        self.scope_stack.clear();
        self.deferred_blurs.clear();
        self.pending_autofocus.clear();
    }

    pub fn check_alive(&mut self, is_alive: impl Fn(ElementId) -> bool) {
        if let Some(id) = self.focused {
            if !is_alive(id) {
                self.focused = None;
            }
        }
    }

    /// Prune stale scopes whose root no longer exists in the tree.
    pub fn prune_stale_scopes(&mut self) {
        self.scope_stack
            .retain(|s| dirty_registry::parent_of(s.root).is_some());
    }
}
