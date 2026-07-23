//! Animation system: easing curves, spring physics, AnimationDriver,
//! and automatic state-transition animations.
//!
//! ## Global toggle
//!
//! `set_animations_enabled(false)` disables ALL animations across the app
//! in one call — useful for `prefers-reduced-motion` or unit tests.
//!
//! ## Auto state transitions
//!
//! When an element has a `TransitionConfig` and `ANIMATIONS_ENABLED` is
//! true, state changes (HOVERED, PRESSED, FOCUSED, etc.) automatically
//! animate.  The animation system writes interpolated values to
//! `StateStyle.animated`, which `resolve_style` reads with highest priority.

pub mod animator;
mod curves;

pub use curves::{apply_easing, Animation, EasingCurve};

use crate::core::element::{with_ct_mut, DirtyFlags};
use crate::core::ElementId;
use crate::style::Color;
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

// ── Global toggle ──────────────────────────────────────────────

thread_local! {
    static ANIMATIONS_ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// Enable or disable ALL animations app-wide.  Disabled by default —
/// call `set_animations_enabled(true)` at startup.
pub fn set_animations_enabled(v: bool) {
    ANIMATIONS_ENABLED.with(|c| c.set(v));
}

pub fn animations_enabled() -> bool {
    ANIMATIONS_ENABLED.with(|c| c.get())
}

// ── Animation driver ───────────────────────────────────────────

/// Drives all active animations, producing interpolated values each frame.
///
/// **Pure-function timeline** (Phase 2, audit 2026-07-18 animation pass):
/// every animation stores its anchor `start` instant and evaluates
/// `f(now - start)` — no per-frame `dt` accumulation, so a dropped or
/// giant frame lands exactly on the analytic value and completion fires
/// exactly once. The anchor is set on the first tick after the request.
pub struct AnimationDriver {
    active: Vec<ActiveAnimation>,
}

struct ActiveAnimation {
    target: ElementId,
    property: AnimatedProperty,
    start_value: AnimatedValue,
    end_value: AnimatedValue,
    /// Progress source: fixed-duration easing curve or a physics simulation.
    progress: ProgressSource,
    /// Anchor instant — `None` until the first tick evaluates this entry.
    start: Option<web_time::Instant>,
    /// Fired (via `defer_action`, once) when the animation completes.
    on_complete: Option<Box<dyn FnOnce()>>,
    /// Exit animation — set to `true` when the exit is complete so
    /// process_exits can immediately finalize the element without
    /// waiting for opacity-zero checks.
    exit_complete: Option<Rc<std::cell::Cell<bool>>>,
}

/// How an animation's eased progress (the `t` fed to `interpolate`) is
/// produced from elapsed seconds.
pub enum ProgressSource {
    /// Fixed-duration easing curve: `ease(elapsed / duration)`, done at
    /// `elapsed >= duration`.
    Curve {
        curve: EasingCurve,
        duration_secs: f32,
    },
    /// Physics simulation in normalized 0..1 space (may overshoot):
    /// progress = `sim.x(elapsed)`, done when `sim.is_done(elapsed)`.
    Sim(Box<dyn crate::physics::simulation::Simulation>),
}

impl ProgressSource {
    /// `(eased_progress, done)` at `elapsed` seconds. Done snaps progress
    /// to its terminal value so the final frame is exact.
    fn eval(&self, elapsed: f32) -> (f32, bool) {
        match self {
            ProgressSource::Curve {
                curve,
                duration_secs,
            } => {
                let progress = if *duration_secs > 0.0 {
                    (elapsed / duration_secs).min(1.0)
                } else {
                    1.0
                };
                (curves::apply_easing(progress, *curve), progress >= 1.0)
            }
            ProgressSource::Sim(sim) => {
                if sim.is_done(elapsed) {
                    (1.0, true)
                } else {
                    (sim.x(elapsed), false)
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AnimatedProperty {
    Opacity,
    Background,
    /// Text / icon color — written to `StateStyle.animated.foreground`,
    /// consumed by `record_element_text` via `resolve_style`.
    Foreground,
    BorderColor,
    BorderWidth,
    /// Full shadow interpolation (color + offset + blur). The damage AABB
    /// already grows by `animated.shadow`, so mid-animation blur never
    /// leaves trails.
    Shadow,
    Position,
    Size,
    /// Rotation in DEGREES about the element's transform origin (default:
    /// center). Writes `xform.transform`; cleared back to `None` on
    /// completion. Constraint: mutually exclusive with a static
    /// `set_transform` on the same element (the animation overwrites it
    /// and completion clears it).
    Rotation,
    CornerRadius,
    /// Third-party animatable property identified by a unique key.
    Custom(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnimatedValue {
    Float(f32),
    Color(Color),
    CornerRadii(crate::style::CornerRadii),
    Vec2(crate::style::Vec2),
    Shadow(crate::style::styled::Shadow),
}

/// Configuration for automatic property transitions.
#[derive(Clone, Debug)]
pub struct TransitionConfig {
    pub transitions: Vec<TransitionDef>,
}

impl TransitionConfig {
    pub fn new(property: AnimatedProperty, curve: EasingCurve, duration_ms: u64) -> Self {
        Self {
            transitions: vec![TransitionDef {
                property,
                curve,
                duration_ms,
            }],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TransitionDef {
    pub property: AnimatedProperty,
    pub curve: EasingCurve,
    pub duration_ms: u64,
}

/// Declarative animation binding.
#[derive(Clone, Debug)]
pub struct AnimationConfig {
    pub property: AnimatedProperty,
    pub from: AnimatedValue,
    pub to: AnimatedValue,
    pub animation: Animation,
}

// ── Thread-local request queues ────────────────────────────────

pub struct AnimRequest {
    pub target: ElementId,
    pub property: AnimatedProperty,
    pub from: AnimatedValue,
    pub to: AnimatedValue,
    pub progress: ProgressSource,
    pub on_complete: Option<Box<dyn FnOnce()>>,
    pub exit_complete: Option<Rc<std::cell::Cell<bool>>>,
}

pub struct ExitRequest {
    pub target: ElementId,
    pub property: AnimatedProperty,
    pub to: AnimatedValue,
    pub animation: Animation,
}

thread_local! {
    static PENDING_ANIMS: RefCell<Vec<AnimRequest>> = const { RefCell::new(Vec::new()) };
    static PENDING_EXITS: RefCell<Vec<ExitRequest>> = const { RefCell::new(Vec::new()) };
    static PENDING_CANCELS: RefCell<Vec<ElementId>> = const { RefCell::new(Vec::new()) };
    static IN_ANIM_APPLY: Cell<bool> = const { Cell::new(false) };
}

/// Enqueue an exit animation from a widget callback.
pub fn request_exit(
    target: ElementId,
    property: AnimatedProperty,
    to: AnimatedValue,
    animation: Animation,
) {
    PENDING_EXITS.with(|q| {
        q.borrow_mut().push(ExitRequest {
            target,
            property,
            to,
            animation,
        })
    });
}

pub fn drain_exit_requests() -> Vec<ExitRequest> {
    PENDING_EXITS.with(|q| q.borrow_mut().drain(..).collect())
}

/// Cancel all active + pending animations targeting `id` (applied at the
/// next `animation_phase`).
pub fn request_cancel(target: ElementId) {
    PENDING_CANCELS.with(|q| q.borrow_mut().push(target));
}

pub(crate) fn set_in_anim_apply(v: bool) {
    IN_ANIM_APPLY.set(v);
}

/// If element has a transition for this property, auto-animate old→new.
pub fn apply_transition(
    element: &crate::core::element::Element,
    property: AnimatedProperty,
    old_val: AnimatedValue,
    new_val: AnimatedValue,
) {
    if IN_ANIM_APPLY.get() {
        return;
    }
    if !animations_enabled() {
        return;
    }
    if old_val == new_val {
        return;
    }
    if let Some(ref tc) = element.transition_config() {
        for t in &tc.transitions {
            if t.property == property {
                let anim = Animation {
                    curve: t.curve,
                    duration_secs: t.duration_ms as f32 / 1000.0,
                };
                request_anim(element.id(), property, old_val, new_val, anim);
                break;
            }
        }
    }
}

/// Enqueue a fixed-duration curve animation from a widget callback.
pub fn request_anim(
    target: ElementId,
    property: AnimatedProperty,
    from: AnimatedValue,
    to: AnimatedValue,
    animation: Animation,
) {
    PENDING_ANIMS.with(|q| {
        q.borrow_mut().push(AnimRequest {
            target,
            property,
            from,
            to,
            progress: ProgressSource::Curve {
                curve: animation.curve,
                duration_secs: animation.duration_secs,
            },
            on_complete: None,
            exit_complete: None,
        })
    });
}

/// Enqueue an animation with an explicit progress source and optional
/// completion callback (fired once, deferred to the next phase boundary).
pub fn request_anim_with(
    target: ElementId,
    property: AnimatedProperty,
    from: AnimatedValue,
    to: AnimatedValue,
    progress: ProgressSource,
    on_complete: Option<Box<dyn FnOnce()>>,
) {
    PENDING_ANIMS.with(|q| {
        q.borrow_mut().push(AnimRequest {
            target,
            property,
            from,
            to,
            progress,
            on_complete,
            exit_complete: None,
        })
    });
}

/// Enqueue an exit animation — the `exit_complete` cell is set to `true`
/// when the animation finishes, and the final animated value is NOT
/// cleared so `process_exits` can detect completion even for
/// non-Opacity properties (e.g. Background→transparent).
pub fn request_exit_anim(
    target: ElementId,
    property: AnimatedProperty,
    from: AnimatedValue,
    to: AnimatedValue,
    animation: Animation,
    exit_complete: Rc<std::cell::Cell<bool>>,
) {
    PENDING_ANIMS.with(|q| {
        q.borrow_mut().push(AnimRequest {
            target,
            property,
            from,
            to,
            progress: ProgressSource::Curve {
                curve: animation.curve,
                duration_secs: animation.duration_secs,
            },
            on_complete: None,
            exit_complete: Some(exit_complete),
        })
    });
}

/// Drain pending requests into the driver. Called by Window::on_frame.
pub fn drain_requests(driver: &mut AnimationDriver) {
    PENDING_CANCELS.with(|q| {
        for target in q.borrow_mut().drain(..) {
            driver.cancel_target(target);
            PENDING_ANIMS.with(|p| p.borrow_mut().retain(|r| r.target != target));
        }
    });
    PENDING_ANIMS.with(|q| {
        for req in q.borrow_mut().drain(..) {
            driver.push(req);
        }
    });
}

// ── AnimationDriver impl ──────────────────────────────────────

impl AnimationDriver {
    pub fn new() -> Self {
        Self { active: Vec::new() }
    }

    /// Legacy entry — fixed-duration curve animation.
    pub fn animate(
        &mut self,
        target: ElementId,
        property: AnimatedProperty,
        start: AnimatedValue,
        end: AnimatedValue,
        animation: Animation,
        exit_complete: Option<Rc<std::cell::Cell<bool>>>,
    ) {
        self.push(AnimRequest {
            target,
            property,
            from: start,
            to: end,
            progress: ProgressSource::Curve {
                curve: animation.curve,
                duration_secs: animation.duration_secs,
            },
            on_complete: None,
            exit_complete,
        });
    }

    fn push(&mut self, req: AnimRequest) {
        // Property-level replace: a new animation for (target, property)
        // supersedes the running one (matches transition semantics — the
        // latest request wins; prevents two writers fighting over one cell).
        self.active
            .retain(|a| !(a.target == req.target && a.property == req.property));
        self.active.push(ActiveAnimation {
            target: req.target,
            property: req.property,
            start_value: req.from,
            end_value: req.to,
            progress: req.progress,
            start: None,
            on_complete: req.on_complete,
            exit_complete: req.exit_complete,
        });
    }

    /// Drop all animations targeting `id` (element unmounted / cancelled).
    /// Animated overrides are cleared so the base style shows through.
    pub fn cancel_target(&mut self, id: ElementId) {
        let mut i = 0;
        while i < self.active.len() {
            if self.active[i].target == id {
                let anim = self.active.remove(i);
                if anim.exit_complete.is_none() {
                    clear_anim_value(anim.target, anim.property);
                }
            } else {
                i += 1;
            }
        }
    }

    /// Tick the driver: evaluate every active animation at `now` (pure
    /// function of its anchor), apply interpolated values, register O(1)
    /// dirty, and fire completions exactly once.
    ///
    /// **Visibility gating** (Phase 3): animations whose target sits in a
    /// reactive-hidden or invisible subtree skip the apply/dirty work —
    /// time keeps flowing (values are `f(now)`), so a reveal resumes at
    /// the exact correct phase and completions still fire.
    pub fn tick(&mut self, arena: &crate::core::element::ElementArena, now: web_time::Instant) {
        let mut completed = Vec::new();

        for (i, anim) in self.active.iter_mut().enumerate() {
            let start = *anim.start.get_or_insert(now);
            let elapsed = now.saturating_duration_since(start).as_secs_f32();
            let (eased, done) = anim.progress.eval(elapsed);

            let viewport = crate::core::frame_driver::CURRENT_VIEWPORT.with(|c| c.get());
            let hidden = dirty_registry::is_reactive_hidden_in_ancestry(anim.target)
                || !dirty_registry::is_visible_chain_fast(anim.target)
                || dirty_registry::is_slot_inactive_in_ancestry(anim.target, arena)
                || dirty_registry::is_offscreen(anim.target, viewport);

            if !hidden || done {
                // Done always applies: the terminal value must be committed
                // even if completion happens while hidden, so the reveal
                // shows the settled state (and exit cells observe it).
                let value = interpolate(&anim.start_value, &anim.end_value, eased);

                set_in_anim_apply(true);
                apply_anim_value(anim.target, anim.property, value);
                set_in_anim_apply(false);
            }

            if !hidden {
                dirty_registry::mark_dirty(anim.target, DirtyFlags::REPAINT);
                dirty_registry::register_dirty(anim.target, DirtyFlags::REPAINT);
                dirty_registry::bump_subtree_gen(anim.target);
            }

            if done {
                // Signal exit completion BEFORE we clear the value.
                if let Some(ref done_cell) = anim.exit_complete {
                    done_cell.set(true);
                }
                if let Some(cb) = anim.on_complete.take() {
                    // User callbacks may mutate the tree — defer past the
                    // Paint phase (same contract as widget callbacks).
                    // defer_action takes `Fn`; wrap the FnOnce in a slot.
                    let slot = std::cell::RefCell::new(Some(cb));
                    dirty_registry::defer_action(move |_, _, _| {
                        if let Some(f) = slot.borrow_mut().take() {
                            f();
                        }
                    });
                }
                completed.push(i);
            }
        }

        // Clear completed animations from `animated` overrides.
        // Skip exit animations — their final value (e.g. transparent
        // background) must persist until process_exits removes the
        // element, otherwise resolve_style falls back to the base
        // (opaque) style and a one-frame flash appears.
        for i in completed.iter().rev() {
            let anim = &self.active[*i];
            if anim.exit_complete.is_none() {
                clear_anim_value(anim.target, anim.property);
            }
        }

        for i in completed.into_iter().rev() {
            self.active.remove(i);
        }
    }

    pub fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    /// Whether any active animation targets a currently-visible subtree.
    /// The window's wake reconciliation uses this so a fully-hidden set of
    /// animations lets the event loop sleep (they resume via the dirty
    /// that reveals them — reactive_visible flips always register dirty,
    /// and scrolling always produces frames).
    pub fn has_active_visible(&self, arena: &crate::core::element::ElementArena) -> bool {
        let viewport = crate::core::frame_driver::CURRENT_VIEWPORT.with(|c| c.get());
        self.active.iter().any(|a| {
            !dirty_registry::is_reactive_hidden_in_ancestry(a.target)
                && dirty_registry::is_visible_chain_fast(a.target)
                && !dirty_registry::is_slot_inactive_in_ancestry(a.target, arena)
                && !dirty_registry::is_offscreen(a.target, viewport)
        })
    }
}

impl Default for AnimationDriver {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal: apply / clear animated overrides ────────────────

fn apply_anim_value(target: ElementId, property: AnimatedProperty, value: AnimatedValue) {
    use crate::style::Vec2;
    with_ct_mut(|ct| {
        match (&property, &value) {
            (AnimatedProperty::Background, AnimatedValue::Color(c)) => {
                ct.style
                    .entry(target)
                    .or_default()
                    .state_style
                    .get_or_insert_with(Default::default)
                    .animated
                    .background = Some(*c);
            }
            (AnimatedProperty::Opacity, AnimatedValue::Float(f)) => {
                ct.style
                    .entry(target)
                    .or_default()
                    .state_style
                    .get_or_insert_with(Default::default)
                    .animated
                    .opacity = Some(f.max(0.0).min(1.0));
            }
            (AnimatedProperty::Foreground, AnimatedValue::Color(c)) => {
                ct.style
                    .entry(target)
                    .or_default()
                    .state_style
                    .get_or_insert_with(Default::default)
                    .animated
                    .foreground = Some(*c);
            }
            (AnimatedProperty::BorderColor, AnimatedValue::Color(c)) => {
                ct.style
                    .entry(target)
                    .or_default()
                    .state_style
                    .get_or_insert_with(Default::default)
                    .animated
                    .border_color = Some(*c);
            }
            (AnimatedProperty::BorderWidth, AnimatedValue::Float(f)) => {
                ct.style
                    .entry(target)
                    .or_default()
                    .state_style
                    .get_or_insert_with(Default::default)
                    .animated
                    .border_width = Some(f.max(0.0));
            }
            (AnimatedProperty::Shadow, AnimatedValue::Shadow(sh)) => {
                ct.style
                    .entry(target)
                    .or_default()
                    .state_style
                    .get_or_insert_with(Default::default)
                    .animated
                    .shadow = Some(*sh);
            }
            // Rotation (degrees): write the affine into xform.transform.
            // Paint composes it about (transform_origin_x/y) — center by
            // default — and the damage AABB corner-unions the transform.
            (AnimatedProperty::Rotation, AnimatedValue::Float(deg)) => {
                let m = glam::Affine2::from_angle(deg.to_radians()).to_cols_array();
                ct.xform.entry(target).or_default().transform = Some(m);
            }
            // Position: Float = legacy x-axis offset; Vec2 = both axes.
            // The xform component is created on demand, and the offset cell
            // is registered for hit-test parity (spatial index compensates
            // visual offsets — audit "framework Pass 2").
            (AnimatedProperty::Position, AnimatedValue::Float(f)) => {
                ct.xform
                    .entry(target)
                    .or_default()
                    .position_offset
                    .set(Vec2::new(*f, 0.0));
                crate::core::dirty_registry::spatial_register_position_offset(target);
            }
            (AnimatedProperty::Position, AnimatedValue::Vec2(v)) => {
                ct.xform.entry(target).or_default().position_offset.set(*v);
                crate::core::dirty_registry::spatial_register_position_offset(target);
            }
            // Size: Float = uniform scale; Vec2 = per-axis scale.
            (AnimatedProperty::Size, AnimatedValue::Float(f)) => {
                ct.xform
                    .entry(target)
                    .or_default()
                    .size_scale
                    .set(Vec2::new(*f, *f));
            }
            (AnimatedProperty::Size, AnimatedValue::Vec2(v)) => {
                ct.xform.entry(target).or_default().size_scale.set(*v);
            }
            (AnimatedProperty::CornerRadius, AnimatedValue::CornerRadii(cr)) => {
                ct.style
                    .entry(target)
                    .or_default()
                    .state_style
                    .get_or_insert_with(Default::default)
                    .animated
                    .corner_radius = Some(*cr);
            }
            _ => {}
        }
    });
}

fn clear_anim_value(target: ElementId, property: AnimatedProperty) {
    with_ct_mut(|ct| {
        if let Some(s) = ct.style.get_mut(&target) {
            if let Some(ss) = s.state_style.as_mut() {
                match property {
                    AnimatedProperty::Background => ss.animated.background = None,
                    AnimatedProperty::Opacity => ss.animated.opacity = None,
                    AnimatedProperty::Foreground => ss.animated.foreground = None,
                    AnimatedProperty::BorderColor => ss.animated.border_color = None,
                    AnimatedProperty::BorderWidth => ss.animated.border_width = None,
                    AnimatedProperty::Shadow => ss.animated.shadow = None,
                    AnimatedProperty::CornerRadius => ss.animated.corner_radius = None,
                    _ => {} // Position/Size hold final value in xform cell — no cleanup needed
                }
            }
        }
        if matches!(property, AnimatedProperty::Rotation) {
            if let Some(xf) = ct.xform.get_mut(&target) {
                xf.transform = None;
            }
        }
    });
}

// ── Interpolation ──────────────────────────────────────────────

fn interpolate(from: &AnimatedValue, to: &AnimatedValue, t: f32) -> AnimatedValue {
    match (from, to) {
        (AnimatedValue::Float(a), AnimatedValue::Float(b)) => {
            AnimatedValue::Float(*a + (*b - *a) * t)
        }
        (AnimatedValue::Color(a), AnimatedValue::Color(b)) => AnimatedValue::Color(a.lerp(b, t)),
        (AnimatedValue::Vec2(a), AnimatedValue::Vec2(b)) => AnimatedValue::Vec2(
            crate::style::Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t),
        ),
        (AnimatedValue::Shadow(a), AnimatedValue::Shadow(b)) => {
            AnimatedValue::Shadow(crate::style::styled::Shadow {
                color: a.color.lerp(&b.color, t),
                offset_x: a.offset_x + (b.offset_x - a.offset_x) * t,
                offset_y: a.offset_y + (b.offset_y - a.offset_y) * t,
                blur: a.blur + (b.blur - a.blur) * t,
            })
        }
        (AnimatedValue::CornerRadii(a), AnimatedValue::CornerRadii(b)) => {
            AnimatedValue::CornerRadii(crate::style::CornerRadii {
                top_left: a.top_left + (b.top_left - a.top_left) * t,
                top_right: a.top_right + (b.top_right - a.top_right) * t,
                bottom_right: a.bottom_right + (b.bottom_right - a.bottom_right) * t,
                bottom_left: a.bottom_left + (b.bottom_left - a.bottom_left) * t,
            })
        }
        _ => to.clone(),
    }
}

// ── LayoutTween: Prepass layout-animation primitive ───────────

/// A pure-function tween for **layout-class** animations (height, width,
/// padding — anything that must reach taffy). The AnimationDriver phase
/// runs after layout, so layout animations live in a Prepass `frame_tick`
/// instead: store a `LayoutTween`, read `value_now()` each tick, write the
/// property via `defer_action` + register MEASURE, and renew an element
/// wake (`scheduler::acquire_element_continuous`). See
/// `widgets/composite/accordion.rs` for the canonical consumer.
///
/// Anchored on `clock::animation_millis()` — deterministic under the
/// virtual clock, exact under frame drops.
#[derive(Clone, Copy, Debug)]
pub struct LayoutTween {
    pub from: f32,
    pub to: f32,
    start_ms: u64,
    duration_ms: f32,
    curve: EasingCurve,
}

impl LayoutTween {
    /// Start now (anchored on the animation timeline).
    pub fn start(from: f32, to: f32, duration_ms: f32, curve: EasingCurve) -> Self {
        Self {
            from,
            to,
            start_ms: crate::core::clock::animation_millis(),
            duration_ms,
            curve,
        }
    }

    /// `(value, done)` at the current instant. Done snaps to the target.
    pub fn value_now(&self) -> (f32, bool) {
        let elapsed = crate::core::clock::animation_millis().saturating_sub(self.start_ms) as f32;
        let t = if self.duration_ms > 0.0 {
            (elapsed / self.duration_ms).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let eased = curves::apply_easing(t, self.curve);
        (self.from + (self.to - self.from) * eased, t >= 1.0)
    }
}

// ── Dirty registry helper ─────────────────────────────────────
// (import to avoid circular dependency issues in this module —
//  dirty_registry is used above but defined in core/.)

use crate::core::dirty_registry;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::clock;
    use std::time::Duration;

    #[test]
    fn layout_tween_is_pure_function_of_the_animation_timeline() {
        clock::install_virtual();
        let tw = LayoutTween::start(0.0, 100.0, 200.0, EasingCurve::Linear);
        assert_eq!(tw.value_now(), (0.0, false));

        clock::advance(Duration::from_millis(100));
        let (v, done) = tw.value_now();
        assert!((v - 50.0).abs() < 0.5, "midpoint 50, got {v}");
        assert!(!done);

        clock::advance(Duration::from_millis(500)); // giant jump past the end
        let (v, done) = tw.value_now();
        assert_eq!(v, 100.0, "clamps exactly at the target");
        assert!(done, "done after duration");
        clock::reset_to_wall();
    }

    #[test]
    fn layout_tween_zero_duration_is_instantly_done() {
        clock::install_virtual();
        let tw = LayoutTween::start(5.0, 25.0, 0.0, EasingCurve::EaseInOut);
        assert_eq!(tw.value_now(), (25.0, true));
        clock::reset_to_wall();
    }
}
