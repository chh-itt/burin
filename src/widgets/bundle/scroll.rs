use std::cell::{Cell, RefCell};
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::physics::platform_physics;
use crate::physics::ScrollPhysics;
use crate::physics::Simulation;
use crate::physics::FLING_VELOCITY_THRESHOLD;
use crate::style::{Padding, Vec2};
use crate::widgets::layout::ScrollDirection;

/// Per-window registry of active scroll-inertia simulations (audit
/// 2026-07-18 multi-window pass). Previously a process-global
/// thread_local: window A's frame, unable to resolve window B's element
/// ids in its own arena, silently deleted B's live simulations. The
/// `AppContext::extension` anymap keeps the value type (`Box<dyn
/// Simulation>`, a widget-layer trait object) out of `core` — no reverse
/// dependency.
#[derive(Default)]
struct ScrollSimDomain {
    active: RefCell<Vec<ScrollSimulationState>>,
}

fn sim_domain() -> Rc<ScrollSimDomain> {
    crate::core::app_context::current_app().extension::<ScrollSimDomain>()
}

struct ScrollSimulationState {
    container_id: ElementId,
    simulation: Box<dyn Simulation>,
    start_time: web_time::Instant,
    axis: OffsetAxis,
    start_value: f32,
}

enum OffsetAxis {
    X,
    Y,
}

/// Stored in container element's user_data so paint can read generation.
pub struct ScrollGeneration(pub Rc<Cell<u64>>);

/// Bump the `ScrollGeneration` counter on `eid`, if it has one.
/// This MUST be called whenever scroll_offset is written outside of
/// `ScrollBundle::apply_offset` (e.g. from A11y actions, kinetic scroll,
/// `resolve_pending_scrolls`). Missing this bump causes stale subtree
/// cache hits → ghost rendering.
pub fn bump_scroll_generation(
    arena: &crate::core::element::ElementArena,
    eid: crate::core::element::ElementId,
) {
    if let Some(el) = arena.get(eid) {
        if let Some(sg) = el.get_user_data::<ScrollGeneration>() {
            sg.0.set(sg.0.get() + 1);
        }
    }
}

/// Stored in container element's user_data so event handlers can access ScrollBundle.
pub struct ScrollBundleRef(pub Rc<ScrollBundle>);

#[derive(Clone)]
pub struct ScrollBundle {
    pub container_id: ElementId,
    pub clip_id: ElementId,
    pub scroll_offset: Rc<Cell<Vec2>>,
    pub content_bounds: Rc<Cell<crate::style::Rect>>,
    pub generation: Rc<Cell<u64>>,
    pub physics: Rc<RefCell<Box<dyn ScrollPhysics>>>,
}

impl std::fmt::Debug for ScrollBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollBundle")
            .field("container_id", &self.container_id)
            .finish_non_exhaustive()
    }
}

impl ScrollBundle {
    /// Allocate container + clip elements, preallocate scroll/layout components,
    /// set overflow:Scroll, scroll_offset, content_bounds, scrollbar_width.
    /// `extra_mask` lets the caller add more ECS components (e.g. STYLE, TEXT, LIFECYCLE).
    /// Returns an Rc<ScrollBundle> stored in user_data for event system access.
    pub fn new_rc(
        ctx: &mut MountContext<'_>,
        extra_mask: u64,
        direction: ScrollDirection,
        scrollbar_width: f32,
    ) -> Rc<Self> {
        Self::new_rc_with_physics(
            ctx,
            extra_mask,
            direction,
            scrollbar_width,
            platform_physics(),
        )
    }

    /// Like new_rc, but with a custom ScrollPhysics policy.
    pub fn new_rc_with_physics(
        ctx: &mut MountContext<'_>,
        extra_mask: u64,
        direction: ScrollDirection,
        scrollbar_width: f32,
        physics: Box<dyn ScrollPhysics>,
    ) -> Rc<Self> {
        let mask = components::SCROLL | components::LAYOUT | extra_mask;

        let container_id = ctx.arena.allocate();
        ctx.preallocate(container_id, mask);

        let scroll_offset = Rc::new(Cell::new(Vec2::ZERO));
        let content_bounds = Rc::new(Cell::new(crate::style::Rect::ZERO));
        let generation = Rc::new(Cell::new(0u64));

        let show_v = direction == ScrollDirection::Vertical || direction == ScrollDirection::Both;
        let show_h = direction == ScrollDirection::Horizontal || direction == ScrollDirection::Both;

        let clip_id = ctx.arena.allocate();
        let vbar_id = if show_v {
            Some(ctx.arena.allocate())
        } else {
            None
        };
        let hbar_id = if show_h {
            Some(ctx.arena.allocate())
        } else {
            None
        };

        let bundle = Rc::new(Self {
            container_id,
            clip_id,
            scroll_offset: scroll_offset.clone(),
            content_bounds: content_bounds.clone(),
            generation: generation.clone(),
            physics: Rc::new(RefCell::new(physics)),
        });

        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return bundle.clone();
            };
            el.set_affected_by_child_size(false);
            el.set_overflow(crate::core::config::Overflow::Scroll);
            el.set_scrollbar_width(scrollbar_width);
            el.set_padding(Padding {
                right: if show_v { scrollbar_width + 2.0 } else { 0.0 },
                bottom: if show_h { scrollbar_width + 2.0 } else { 0.0 },
                ..Padding::ZERO
            });
            el.set_flex_grow(1.0);
            el.set_flex_shrink(1.0);
            el.set_scroll_offset(scroll_offset.clone());
            el.set_content_bounds(content_bounds.clone());
            el.insert_user_data(ScrollGeneration(generation.clone()));
        }

        {
            let Some(clip) = ctx.arena.get_mut(clip_id) else {
                return bundle.clone();
            };
            clip.set_accessible_role(accesskit::Role::Group);
        }
        ctx.arena.add_child(container_id, clip_id);

        if let Some(vbar_id) = vbar_id {
            {
                let Some(vbar) = ctx.arena.get_mut(vbar_id) else {
                    return bundle.clone();
                };
                vbar.set_accessible_role(accesskit::Role::ScrollBar);
                vbar.set_accessible_label(String::from("Vertical scrollbar"));
            }
            ctx.arena.add_child(container_id, vbar_id);
        }
        if let Some(hbar_id) = hbar_id {
            {
                let Some(hbar) = ctx.arena.get_mut(hbar_id) else {
                    return bundle.clone();
                };
                hbar.set_accessible_role(accesskit::Role::ScrollBar);
                hbar.set_accessible_label(String::from("Horizontal scrollbar"));
            }
            ctx.arena.add_child(container_id, hbar_id);
        }

        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return bundle.clone();
            };
            el.insert_user_data(ScrollBundleRef(bundle.clone()));
        }

        // Touch-first scrolling (mobile-groundwork W2): a single finger
        // dragging this container IS the scroll gesture on touch. The
        // recognizer is touch-only (the arena filters it for mouse
        // pointers) and bows out when the swipe direction is not
        // scrollable here — nested opposite-direction scrolls disambiguate
        // naturally.
        crate::event::recognizer::register_recognizer(
            container_id,
            60, // below eager drag (100): a slider inside a list wins the finger
            crate::event::recognizer::RecognizerKind::Scroll,
            Box::new(crate::event::recognizer::ScrollRecognizer::new(
                show_h, show_v,
            )),
            None,
        );

        bundle
    }

    /// Keep `new` for backward compat (no user_data registration).
    pub fn new(
        ctx: &mut MountContext<'_>,
        extra_mask: u64,
        direction: ScrollDirection,
        scrollbar_width: f32,
    ) -> Self {
        Self::new_rc(ctx, extra_mask, direction, scrollbar_width)
            .as_ref()
            .clone()
    }

    /// Mount a widget as the scrollable content (inside clip_id).
    pub fn child(self, widget: impl Widget + 'static, ctx: &mut MountContext<'_>) -> Self {
        let mut child_ctx = ctx.child_with_events(self.clip_id);
        let child_id = Box::new(widget).mount_box(&mut child_ctx);
        ctx.arena.add_child(self.clip_id, child_id);
        self
    }

    // ── Offset management ──

    /// Helper: clamp value to content bounds.
    pub fn clamp(&self, new: Vec2, viewport: Vec2) -> Vec2 {
        let cb = self.content_bounds.get();
        let max_x = (cb.width - viewport.x).max(0.0);
        let max_y = (cb.height - viewport.y).max(0.0);
        Vec2::new(new.x.clamp(0.0, max_x), new.y.clamp(0.0, max_y))
    }

    /// Low-level offset write, shared by all setters.
    /// Skips clamping — caller is responsible for boundary management.
    /// Physics system uses this for fling/spring simulations (may overscroll).
    pub fn apply_offset(&self, new: Vec2) {
        // Phase guard: scroll offset must not be mutated during Layout.
        // Pre-pass simulations should route through defer_action; this guard
        // catches Layout-phase violations.
        crate::core::frame_pipeline::debug_assert_phase(&[
            crate::core::frame_pipeline::FramePhase::Prepass,
            crate::core::frame_pipeline::FramePhase::Paint,
            crate::core::frame_pipeline::FramePhase::None,
        ]);
        let old = self.scroll_offset.get();
        if (old.x - new.x).abs() < 0.01 && (old.y - new.y).abs() < 0.01 {
            return;
        }
        self.scroll_offset.set(new);
        dirty_registry::spatial_update_scroll(self.container_id, new.x, new.y);
        self.generation.set(self.generation.get() + 1);
        dirty_registry::mark_dirty(self.container_id, DirtyFlags::REPAINT);
        dirty_registry::register_dirty(self.container_id, DirtyFlags::REPAINT);
    }

    /// Safe entry with physics: applies boundary conditions, then writes.
    pub fn set_offset(&self, new: Vec2, viewport: Vec2) {
        self.set_offset_with_physics(new, viewport);
    }

    /// Physics-aware set_offset: applies boundary conditions from ScrollPhysics,
    /// then writes via apply_offset (no additional clamping).
    pub fn set_offset_with_physics(&self, new: Vec2, viewport: Vec2) {
        let physics = self.physics.borrow();
        let offset = self.scroll_offset.get();
        let cb = self.content_bounds.get();
        let max_x = (cb.width - viewport.x).max(0.0);
        let max_y = (cb.height - viewport.y).max(0.0);

        let apply_bound = |pos: f32, desired: f32, min: f32, max: f32| -> f32 {
            let overscroll = physics.apply_boundary_conditions(pos, desired, min, max);
            desired - overscroll
        };

        let clamped = Vec2::new(
            apply_bound(offset.x, new.x, 0.0, max_x),
            apply_bound(offset.y, new.y, 0.0, max_y),
        );
        self.apply_offset(clamped);
    }

    pub fn set_offset_x(&self, x: f32, viewport_w: f32) {
        let cb = self.content_bounds.get();
        let max_x = (cb.width - viewport_w).max(0.0);
        let mut cur = self.scroll_offset.get();
        cur.x = x.clamp(0.0, max_x);
        self.apply_offset(cur);
    }

    pub fn set_offset_y(&self, y: f32, viewport_h: f32) {
        let cb = self.content_bounds.get();
        let max_y = (cb.height - viewport_h).max(0.0);
        let mut cur = self.scroll_offset.get();
        cur.y = y.clamp(0.0, max_y);
        self.apply_offset(cur);
    }

    /// Scroll so that `row_idx` (at `row_height` per row) is visible.
    pub fn scroll_to_row(&self, row_idx: usize, row_height: f32, viewport_height: f32) {
        let target_y = row_idx as f32 * row_height;
        let vph = viewport_height.max(row_height);
        let mut o = self.scroll_offset.get();
        if target_y < o.y {
            o.y = target_y;
        } else if target_y + row_height > o.y + vph {
            o.y = target_y + row_height - vph;
        }
        o.y = o.y.max(0.0);
        let cb_h = self.content_bounds.get().height;
        if cb_h > 0.0 {
            let max_y = (cb_h - viewport_height).max(0.0);
            o.y = o.y.min(max_y);
        } else {
            o.y = o.y.min(target_y + row_height);
        }
        self.set_offset_y(o.y, viewport_height);
    }

    /// Scroll so that `child_id`'s actual taffy-computed bounds are visible.
    ///
    /// Unlike `scroll_to_row`, this reads real element positions from the
    /// layout cache (`dirty_registry::bounds_of`) instead of relying on a
    /// caller-estimated `row_height`. It is immune to padding, gaps, or
    /// other layout factors that can cause `row_height` to diverge from
    /// the actual rendered size.
    ///
    /// Safe to call before the first frame — early-returns when bounds
    /// have not been set yet.
    pub fn scroll_to_keep_visible(&self, child_id: ElementId) {
        use crate::core::dirty_registry;
        let container_bounds =
            dirty_registry::bounds_of(self.container_id).unwrap_or(crate::style::Rect::ZERO);
        let child_bounds = dirty_registry::bounds_of(child_id).unwrap_or(crate::style::Rect::ZERO);

        let vph = container_bounds.height;
        let row_h = child_bounds.height;
        if vph <= 0.0 || row_h <= 0.0 {
            return;
        }

        let target_y = child_bounds.y - container_bounds.y;
        let mut o = self.scroll_offset.get();
        if target_y < o.y {
            o.y = target_y;
        } else if target_y + row_h > o.y + vph {
            o.y = (target_y + row_h - vph).max(0.0);
        }
        o.y = o.y.max(0.0);
        self.set_offset_y(o.y, vph);
    }

    /// Start a physics fling from current velocity.
    /// Uses the bundle's configured ScrollPhysics.
    /// The simulation runs via `process_active_simulations` each frame.
    /// Passes through `apply_offset` (no clamp) so BouncePhysics can overscroll.
    pub fn fling(&self, velocity: Vec2) {
        let offset = self.scroll_offset.get();
        let viewport =
            dirty_registry::bounds_of(self.container_id).unwrap_or(crate::style::Rect::ZERO);
        let cb = self.content_bounds.get();
        let min_y = 0.0;
        let max_y = (cb.height - viewport.height).max(0.0);
        let min_x = 0.0;
        let max_x = (cb.width - viewport.width).max(0.0);

        let physics = self.physics.borrow();
        if velocity.y.abs() >= FLING_VELOCITY_THRESHOLD || offset.y < min_y || offset.y > max_y {
            if let Some(sim) = physics.ballistic_simulation(offset.y, velocity.y, min_y, max_y) {
                sim_domain()
                    .active
                    .borrow_mut()
                    .push(ScrollSimulationState {
                        container_id: self.container_id,
                        simulation: sim,
                        start_time: crate::core::clock::now(),
                        axis: OffsetAxis::Y,
                        start_value: offset.y,
                    });
            }
        }
        if velocity.x.abs() >= FLING_VELOCITY_THRESHOLD || offset.x < min_x || offset.x > max_x {
            if let Some(sim) = physics.ballistic_simulation(offset.x, velocity.x, min_x, max_x) {
                sim_domain()
                    .active
                    .borrow_mut()
                    .push(ScrollSimulationState {
                        container_id: self.container_id,
                        simulation: sim,
                        start_time: crate::core::clock::now(),
                        axis: OffsetAxis::X,
                        start_value: offset.x,
                    });
            }
        }
    }

    /// Subscribe to an external Signal to drive scroll offset.
    /// Fixes the existing bug where `so.set()` was called without `spatial_update_scroll`.
    pub fn bind_offset(&self, sig: Signal<Vec2>) {
        let container_id = self.container_id;
        let so = self.scroll_offset.clone();
        let cb = self.content_bounds.clone();
        let gen = self.generation.clone();
        let sig_clone = sig.clone();
        crate::core::signal_bridge::subscribe_owned(container_id, &sig, move || {
            let new = sig_clone.read();
            // Compute max from latest content_bounds
            let vp = dirty_registry::bounds_of(container_id).unwrap_or(crate::style::Rect::ZERO);
            let max_x = (cb.get().width - vp.width).max(0.0);
            let max_y = (cb.get().height - vp.height).max(0.0);
            let clamped = Vec2::new(new.x.clamp(0.0, max_x), new.y.clamp(0.0, max_y));
            so.set(clamped);
            dirty_registry::spatial_update_scroll(container_id, clamped.x, clamped.y);
            gen.set(gen.get() + 1);
            dirty_registry::mark_dirty(container_id, DirtyFlags::REPAINT);
            dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);
        });
    }

    /// Physics-aware scroll: apply user offset friction, then boundary conditions,
    /// then write via apply_offset (no additional clamping).
    /// Returns the unconsumed delta — the portion of (dx, dy) that was
    /// rejected by clamping / boundary conditions. Zero means fully consumed.
    pub fn scroll_by(&self, dx: f32, dy: f32, viewport: Vec2) -> (f32, f32) {
        let physics = self.physics.borrow();
        let offset = self.scroll_offset.get();
        let cb = self.content_bounds.get();
        let max_x = (cb.width - viewport.x).max(0.0);
        let max_y = (cb.height - viewport.y).max(0.0);

        let scroll_axis = |pos: f32, delta: f32, min: f32, max: f32| -> (f32, f32) {
            if delta == 0.0 {
                return (pos, 0.0);
            }
            let desired = pos - delta;
            let friction_applied = if pos < min || pos > max {
                pos + physics.apply_user_offset(pos, delta, min, max)
            } else {
                desired
            };
            let overscroll = physics.apply_boundary_conditions(pos, friction_applied, min, max);
            let result = friction_applied - overscroll;
            (result, delta + result - pos)
        };

        let (nx, ux) = scroll_axis(offset.x, dx, 0.0, max_x);
        let (ny, uy) = scroll_axis(offset.y, dy, 0.0, max_y);

        self.apply_offset(Vec2::new(nx, ny));
        (ux, uy)
    }

    /// Replace the scroll physics policy at runtime.
    pub fn set_physics(&self, physics: Box<dyn ScrollPhysics>) {
        *self.physics.borrow_mut() = physics;
    }
}

/// Try to set a scrollable element's offset through its ScrollBundle physics.
/// Returns true if the element has a ScrollBundle and the offset was applied.
pub fn try_set_offset(
    arena: &crate::core::element::ElementArena,
    eid: ElementId,
    apply: impl FnOnce(&ScrollBundle, Vec2),
) -> bool {
    if let Some(el) = arena.get(eid) {
        if let Some(refcell) = el.get_user_data::<ScrollBundleRef>() {
            let vp = el.screen_bounds;
            apply(&refcell.0, Vec2::new(vp.width, vp.height));
            return true;
        }
    }
    false
}

/// Try to scroll a scrollable element using its ScrollBundle physics.
/// Returns `Some((unconsumed_x, unconsumed_y))` if the element has a
/// ScrollBundle and the scroll was applied. The unconsumed delta is the
/// portion rejected by clamping/boundary conditions. `None` means the
/// element has no ScrollBundle.
pub fn try_scroll_by(
    arena: &crate::core::element::ElementArena,
    eid: ElementId,
    dx: f32,
    dy: f32,
    viewport_h: f32,
    viewport_w: f32,
) -> Option<(f32, f32)> {
    if let Some(el) = arena.get(eid) {
        if let Some(refcell) = el.get_user_data::<ScrollBundleRef>() {
            return Some(
                refcell
                    .0
                    .scroll_by(dx, dy, Vec2::new(viewport_w, viewport_h)),
            );
        }
    }
    None
}

/// Try to fling a scrollable element (called from scroll end handler in window.rs).
///
/// Desktop notes:
/// - Discrete mouse wheel (LineDelta) does NOT trigger fling — only TouchPhase::Ended does.
/// - On desktop with a mouse, fling is never fired; behavior is identical to before.
/// - On touchpad/trackpad, TouchPhase::Ended fires → fling runs normally.
///
/// Mobile / touchscreen: Should work but NOT YET TESTED. The Simulation trait,
/// ClampPhysics, BouncePhysics, and the O(k) frame_tick system are all platform-
/// agnostic. When mobile input is added, test TouchPhase::Ended + fling at that time.
pub fn try_fling(
    arena: &crate::core::element::ElementArena,
    eid: ElementId,
    velocity: Vec2,
) -> bool {
    if let Some(el) = arena.get(eid) {
        if let Some(refcell) = el.get_user_data::<ScrollBundleRef>() {
            refcell.0.fling(velocity);
            return true;
        }
    }
    false
}

/// Called from `process_frame_ticks` once per frame.
/// Steps all active scroll simulations + animations (O(k), k = number of active flings + animations).
pub fn process_active_simulations(arena: &crate::core::element::ElementArena) {
    let mut to_remove: Vec<usize> = Vec::new();
    let now = crate::core::clock::now();

    let dom = sim_domain();
    {
        let mut sims = dom.active.borrow_mut();
        for (i, state) in sims.iter().enumerate() {
            let dt = now.duration_since(state.start_time).as_secs_f32();

            // Retrieve bundle from user_data to get apply_offset + generation bump
            let bundle = arena
                .get(state.container_id)
                .and_then(|el| el.get_user_data::<ScrollBundleRef>())
                .map(|r| r.0.clone());

            let bundle = match bundle {
                Some(b) => b,
                None => {
                    // Element was removed — drop simulation
                    to_remove.push(i);
                    continue;
                }
            };

            let delta = state.simulation.x(dt);
            let new_value = state.start_value + delta;

            let mut current = bundle.scroll_offset.get();
            match state.axis {
                OffsetAxis::Y => current.y = new_value,
                OffsetAxis::X => current.x = new_value,
            }

            bundle.apply_offset(current);

            if state.simulation.is_done(dt) {
                to_remove.push(i);
            }
        }
        // Remove in reverse order
        for &i in to_remove.iter().rev() {
            sims.swap_remove(i);
        }
    }
}

// ── Scrollbar geometry + scroll application (moved from platform/window.rs
//    — audit round 5, phase 2: this is ScrollBundle domain logic) ──

/// Scrollbar axis (vertical/horizontal bar), used by scrollbar hit-testing
/// and the window's scrollbar-drag state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ScrollAxis {
    Vertical,
    Horizontal,
}

use crate::core::element::{Element, ElementArena};

pub(crate) fn find_scrollable_at(
    arena: &ElementArena,
    pos: crate::style::Point,
) -> Option<(ElementId, &Element)> {
    // Innermost wins == highest tree_order among containing candidates
    // (children are allocated after parents). Single max-selection pass —
    // this runs on every PointerMoved (scrollbar hover scan), so the former
    // full sort was O(S log S) per move for a single lookup.
    let mut best: Option<(u64, ElementId, &Element)> = None;
    for eid in crate::ecs::scrollable_elements() {
        if let Some(el) = arena.get(eid) {
            if !el.is_scrollable() {
                continue;
            }
            if best.as_ref().is_some_and(|&(bt, _, _)| el.tree_order <= bt) {
                continue;
            }
            let sb = el.screen_bounds;
            let rect = crate::style::Rect::new(sb.x, sb.y, sb.width.max(1.0), sb.height.max(1.0));
            let (sx, sy) = crate::core::dirty_registry::accumulated_scroll_cached(arena, eid);
            let own = el
                .scroll_offset()
                .as_ref()
                .map(|s| s.get())
                .unwrap_or(crate::style::Vec2::ZERO);
            let local = crate::style::Point::new(pos.x + sx - own.x, pos.y + sy - own.y);
            if rect.contains(local) {
                best = Some((el.tree_order, eid, el));
            }
        }
    }
    best.map(|(_, eid, el)| (eid, el))
}

/// Result of a combined scrollbar hit test (audit 2026-07-17: the hover and
/// pointer-down paths previously called `hit_scrollbar_thumb` then
/// `hit_scrollbar_track`, each running its own `find_scrollable_at` scan +
/// sort + `scroll_context` — twice per PointerMoved).
pub(crate) struct ScrollbarHit {
    pub eid: ElementId,
    pub axis: ScrollAxis,
    /// Thumb hits: grab point as a fraction of the thumb extent.
    /// Track hits: target fraction along the track.
    pub fraction: f32,
    pub on_thumb: bool,
}

/// Single-scan scrollbar hit test: thumb (both axes) takes precedence over
/// track (both axes) — identical precedence to the former thumb→track pair.
pub(crate) fn hit_scrollbar(
    arena: &ElementArena,
    pos: crate::style::Point,
) -> Option<ScrollbarHit> {
    let (eid, element) = find_scrollable_at(arena, pos)?;
    let sctx = scroll_context(arena, eid, element, pos);
    let sb = sctx.sb;
    let sb_w = sctx.sb_w;
    let hit_w = sctx.hit_w;
    let local = sctx.local;
    let cb = sctx.cb;

    let v_thumb = if cb.height > sb.height {
        let thumb_h = (sb.height / cb.height * sb.height).max(20.0);
        let thumb_y = sb.y + (sctx.so_y / (cb.height - sb.height)) * (sb.height - thumb_h);
        Some((thumb_y, thumb_h))
    } else {
        None
    };
    let h_thumb = if cb.width > sb.width {
        let gutter = if cb.height > sb.height {
            sb_w + 2.0
        } else {
            0.0
        };
        let thumb_w = (sb.width / cb.width * sb.width).max(20.0);
        let thumb_x = sb.x + (sctx.so_x / (cb.width - sb.width)) * (sb.width - thumb_w - gutter);
        Some((thumb_x, thumb_w))
    } else {
        None
    };

    if let Some((thumb_y, thumb_h)) = v_thumb {
        let ht = crate::style::Rect::new(
            sb.x + sb.width - sb_w - 2.0 - (hit_w - sb_w) * 0.5,
            thumb_y,
            hit_w,
            thumb_h,
        );
        if ht.contains(local) {
            return Some(ScrollbarHit {
                eid,
                axis: ScrollAxis::Vertical,
                on_thumb: true,
                fraction: ((local.y - thumb_y) / thumb_h.max(1.0)).clamp(0.0, 1.0),
            });
        }
    }
    if let Some((thumb_x, thumb_w)) = h_thumb {
        let ht = crate::style::Rect::new(
            thumb_x,
            sb.y + sb.height - sb_w - 2.0 - (hit_w - sb_w) * 0.5,
            thumb_w,
            hit_w,
        );
        if ht.contains(local) {
            return Some(ScrollbarHit {
                eid,
                axis: ScrollAxis::Horizontal,
                on_thumb: true,
                fraction: ((local.x - thumb_x) / thumb_w.max(1.0)).clamp(0.0, 1.0),
            });
        }
    }
    if v_thumb.is_some() {
        let bar_x = sb.x + sb.width - sb_w - 2.0;
        let bar_rect =
            crate::style::Rect::new(bar_x - (hit_w - sb_w) * 0.5, sb.y, hit_w, sb.height);
        if bar_rect.contains(local) {
            return Some(ScrollbarHit {
                eid,
                axis: ScrollAxis::Vertical,
                on_thumb: false,
                fraction: ((local.y - sb.y) / sb.height.max(1.0)).clamp(0.0, 1.0),
            });
        }
    }
    if h_thumb.is_some() {
        let bar_y = sb.y + sb.height - sb_w - 2.0;
        let bar_rect = crate::style::Rect::new(sb.x, bar_y - (hit_w - sb_w) * 0.5, sb.width, hit_w);
        if bar_rect.contains(local) {
            return Some(ScrollbarHit {
                eid,
                axis: ScrollAxis::Horizontal,
                on_thumb: false,
                fraction: ((local.x - sb.x) / sb.width.max(1.0)).clamp(0.0, 1.0),
            });
        }
    }
    None
}

struct ScrollContext {
    sb: crate::style::Rect,
    sb_w: f32,
    hit_w: f32,
    cb: crate::style::Rect,
    so_y: f32,
    so_x: f32,
    local: crate::style::Point,
}

fn scroll_context(
    arena: &ElementArena,
    eid: ElementId,
    element: &crate::core::element::Element,
    pos: crate::style::Point,
) -> ScrollContext {
    let sb = element.screen_bounds;
    let sb_w = element.scrollbar_width();
    let hit_w = sb_w.max(24.0);
    let cb = element
        .content_bounds()
        .as_ref()
        .map(|c| c.get())
        .unwrap_or(crate::style::Rect::ZERO);
    let (asx, asy) = crate::core::dirty_registry::accumulated_scroll_cached(arena, eid);
    let own = element
        .scroll_offset()
        .as_ref()
        .map(|s| s.get())
        .unwrap_or(crate::style::Vec2::ZERO);
    let local = crate::style::Point::new(pos.x + asx - own.x, pos.y + asy - own.y);
    let so_y = element.scroll_offset().as_ref().map_or(0.0, |s| s.get().y);
    let so_x = element.scroll_offset().as_ref().map_or(0.0, |s| s.get().x);
    ScrollContext {
        sb,
        sb_w,
        hit_w,
        cb,
        so_y,
        so_x,
        local,
    }
}

pub(crate) fn scrollbar_jump_to(
    arena: &ElementArena,
    eid: ElementId,
    axis: ScrollAxis,
    fraction: f32,
) {
    let sb = arena
        .get(eid)
        .map_or(crate::style::Rect::ZERO, |el| el.screen_bounds);
    let sc = match arena.comp_scroll(eid) {
        Some(sc) => sc,
        None => return,
    };
    let cb = sc.content_bounds.get();
    let so = sc.scroll_offset;
    match axis {
        ScrollAxis::Vertical if cb.height > sb.height => {
            let off = fraction * (cb.height - sb.height);
            let mut v = so.get();
            v.y = off;
            so.set(v);
            crate::core::dirty_registry::spatial_update_scroll(eid, v.x, v.y);
        }
        ScrollAxis::Horizontal if cb.width > sb.width => {
            let off = fraction * (cb.width - sb.width);
            let mut v = so.get();
            v.x = off;
            so.set(v);
            crate::core::dirty_registry::spatial_update_scroll(eid, v.x, v.y);
        }
        _ => {}
    }
    if let Some(el) = arena.get(eid) {
        el.mark_repaint();
    }
}

// (ease_in_out_cubic removed — kinetic AnimatedTo now uses the shared
//  animation::apply_easing(EaseInOut) curve; Phase 2, 2026-07-18.)

pub(crate) fn compute_scroll_velocity(history: &[(f32, f32, std::time::Instant)]) -> Vec2 {
    if history.len() < 2 {
        return Vec2::ZERO;
    }
    let (_first_dx, _first_dy, first_t) = history[0];
    let (_, _, last_t) = history[history.len() - 1];
    let total_dx: f32 = history.iter().map(|(x, _, _)| x).sum();
    let total_dy: f32 = history.iter().map(|(_, y, _)| y).sum();
    let dt = (last_t - first_t).as_secs_f32();
    if dt > 0.0 {
        Vec2::new(total_dx / dt, total_dy / dt)
    } else {
        Vec2::ZERO
    }
}

pub(crate) fn find_scrollable_at_position(
    arena: &ElementArena,
    _root: ElementId,
    pos: crate::style::Point,
) -> Option<ElementId> {
    find_scrollable_at(arena, pos).map(|(eid, _)| eid)
}

pub(crate) fn do_scroll(arena: &ElementArena, eid: ElementId, dx: f32, dy: f32) {
    let sb = arena
        .get(eid)
        .map_or(crate::style::Rect::ZERO, |el| el.screen_bounds);
    if let Some(sc) = arena.comp_scroll(eid) {
        let mut o = sc.scroll_offset.get();
        o.y -= dy;
        o.x -= dx;
        let max_y = (sc.content_bounds.get().height - sb.height).max(0.0);
        let max_x = (sc.content_bounds.get().width - sb.width).max(0.0);
        o.y = o.y.clamp(0.0, max_y);
        o.x = o.x.clamp(0.0, max_x);
        sc.scroll_offset.set(o);
        crate::core::dirty_registry::spatial_update_scroll(eid, o.x, o.y);
        // Invalidate subtree cache so children's DrawCommands get fresh clip
        crate::core::dirty_registry::bump_subtree_gen(eid);
        self::bump_scroll_generation(arena, eid);
    }
    if let Some(el) = arena.get(eid) {
        el.mark_repaint();
    }
}

/// Set an absolute scroll offset with the same dirty protocol and
/// clamping as [`do_scroll`] (audit round 5: the a11y SetScrollOffset
/// path previously wrote raw values without max-clamping or subtree-cache
/// invalidation).
pub(crate) fn set_scroll_offset_clamped(arena: &ElementArena, eid: ElementId, x: f32, y: f32) {
    let sb = arena
        .get(eid)
        .map_or(crate::style::Rect::ZERO, |el| el.screen_bounds);
    if let Some(sc) = arena.comp_scroll(eid) {
        let max_y = (sc.content_bounds.get().height - sb.height).max(0.0);
        let max_x = (sc.content_bounds.get().width - sb.width).max(0.0);
        let o = crate::style::Vec2::new(x.clamp(0.0, max_x), y.clamp(0.0, max_y));
        sc.scroll_offset.set(o);
        crate::core::dirty_registry::spatial_update_scroll(eid, o.x, o.y);
        crate::core::dirty_registry::bump_subtree_gen(eid);
        self::bump_scroll_generation(arena, eid);
    }
    if let Some(el) = arena.get(eid) {
        el.mark_repaint();
    }
}

pub(crate) fn resolve_pending_scrolls(arena: &mut ElementArena, _root_id: ElementId) {
    let eids = crate::ecs::pending_scroll_elements();
    for eid in eids {
        let pending_scroll = arena.comp_scroll(eid).map(|s| s.pending_scroll_to.clone());
        let current_off = arena
            .comp_scroll(eid)
            .map_or(crate::style::Vec2::ZERO, |s| s.scroll_offset.get());
        let viewport = arena
            .get(eid)
            .map_or(crate::style::Rect::ZERO, |el| el.screen_bounds);
        let target_id = pending_scroll.as_ref().and_then(|c| c.get());
        if let (Some(tid), Some(ref pending)) = (target_id, &pending_scroll) {
            let target_b = arena.get(tid).map(|t| t.screen_bounds).unwrap_or_default();
            if let Some(sc) = arena.comp_scroll(eid) {
                let new_x = current_off.x + target_b.x - viewport.x;
                let new_y = current_off.y + target_b.y - viewport.y;
                sc.scroll_offset.set(crate::style::Vec2::new(new_x, new_y));
                crate::core::dirty_registry::spatial_update_scroll(eid, new_x, new_y);
                self::bump_scroll_generation(arena, eid);
            }
            if let Some(el) = arena.get_mut(eid) {
                el.mark_repaint();
            }
            pending.set(None);
            crate::ecs::unregister_pending_scroll(eid);
        }
    }
}
