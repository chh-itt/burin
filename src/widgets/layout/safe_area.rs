//! SafeArea — pads its child by the window's effective insets
//! (safe area ∪ IME keyboard), per-edge opt-out.
//!
//! Mobile: notch / home-indicator / keyboard avoidance. Desktop: the
//! custom-drawn titlebar scenario (decorations=false injects its bar
//! height as `safe_area.top`).
//!
//! Reconciliation: a Prepass frame_tick diffs `insets_generation()` and
//! re-pads via defer_action + MEASURE — the same phase-discipline
//! pattern as the Accordion height transition. `set_window_insets`
//! queues a deferred root MEASURE, so a change always produces the
//! frame this tick runs in; when insets are stable the tick is a
//! generation compare (O(1), no allocation).

use std::cell::Cell;
use std::rc::Rc;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::platform::insets;
use crate::style::Padding;

/// A widget that insets its child by the window's safe area.
pub struct SafeArea {
    child: Option<Box<dyn Widget>>,
    left: bool,
    top: bool,
    right: bool,
    bottom: bool,
}

impl SafeArea {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Some(Box::new(child)),
            left: true,
            top: true,
            right: true,
            bottom: true,
        }
    }

    pub fn left(mut self, v: bool) -> Self {
        self.left = v;
        self
    }
    pub fn top(mut self, v: bool) -> Self {
        self.top = v;
        self
    }
    pub fn right(mut self, v: bool) -> Self {
        self.right = v;
        self
    }
    pub fn bottom(mut self, v: bool) -> Self {
        self.bottom = v;
        self
    }
}

impl Widget for SafeArea {
    fn component_mask(&self) -> u64 {
        components::LAYOUT | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());

        let edges = (self.left, self.top, self.right, self.bottom);
        let pad_for = move |e: insets::EdgeInsets| Padding {
            left: if edges.0 { e.left } else { 0.0 },
            top: if edges.1 { e.top } else { 0.0 },
            right: if edges.2 { e.right } else { 0.0 },
            bottom: if edges.3 { e.bottom } else { 0.0 },
        };

        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::GenericContainer);
            element.set_affected_by_child_size(false);
            element.set_flex_grow(1.0);
            element.set_flex_shrink(1.0);
            element.set_padding(pad_for(insets::window_insets().effective()));

            // Reconcile against the insets generation each frame.
            let seen: Rc<Cell<u64>> = Rc::new(Cell::new(insets::insets_generation()));
            let sid = id;
            element.set_frame_tick(Box::new(move || {
                let gen = insets::insets_generation();
                if seen.get() == gen {
                    return;
                }
                seen.set(gen);
                let pad = pad_for(insets::window_insets().effective());
                crate::core::dirty_registry::defer_action(move |arena, _, _| {
                    if let Some(el) = arena.get_mut(sid) {
                        el.set_padding(pad);
                    }
                    crate::core::dirty_registry::register_dirty(
                        sid,
                        crate::core::element::DirtyFlags::MEASURE,
                    );
                    crate::core::dirty_registry::bump_subtree_gen(sid);
                });
            }));
        }

        if let Some(child) = self.child.take() {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = child.mount_box(&mut child_ctx);
            if let Some(child_el) = ctx.arena.get_mut(child_id) {
                child_el.set_flex_grow(1.0);
                child_el.set_flex_shrink(1.0);
            }
            ctx.arena.add_child(id, child_id);
        }
        id
    }
}

impl std::fmt::Debug for SafeArea {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafeArea")
            .field("edges", &(self.left, self.top, self.right, self.bottom))
            .finish_non_exhaustive()
    }
}
