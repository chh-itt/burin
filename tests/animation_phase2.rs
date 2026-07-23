//! Phase 2 animation-architecture guards: pure-function AnimationDriver.
//!
//! - Animation values are `f(start_instant, now)` — a giant frame jump
//!   (suspend, breakpoint, dropped frames) lands exactly on the analytic
//!   value, and completion fires exactly once.
//! - `animator::register_animation` is a real API (it silently dropped
//!   simulations before Phase 2).
//! - `AnimatedValue::Vec2` drives both axes of position/size transforms.

use burin::animation::{self, animator, AnimatedProperty, AnimatedValue, Animation, EasingCurve};
use burin::core::config::StateFlags;
use burin::core::element::Element;
use burin::physics::simulation::SpringSimulation;
use burin::style::Vec2;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use std::cell::Cell;
use std::rc::Rc;

fn resolved_opacity(id: burin::core::ElementId) -> f32 {
    match Element::resolve_anim_property(id, StateFlags::NONE, AnimatedProperty::Opacity) {
        AnimatedValue::Float(f) => f,
        other => panic!("expected Float opacity, got {other:?}"),
    }
}

/// Linear opacity 0→1 over 400ms: the resolved value at t=100ms must be
/// 0.25 regardless of how many frames ran in between (pure function of
/// the virtual clock).
#[test]
fn opacity_midpoint_follows_virtual_clock() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("fade me"));
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::Opacity,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(1.0),
        Animation {
            curve: EasingCurve::Linear,
            duration_secs: 0.4,
        },
    );
    h.run_frame(); // anchor frame (t = 0)

    h.advance_time(100);
    h.run_frame();
    let v = resolved_opacity(id);
    assert!(
        (v - 0.25).abs() < 0.02,
        "t=100ms of 400ms linear → 0.25, got {v}"
    );

    h.advance_time(100);
    h.run_frame();
    let v = resolved_opacity(id);
    assert!((v - 0.5).abs() < 0.02, "t=200ms → 0.5, got {v}");
}

/// A single giant jump past the end must complete the animation (value
/// clamped to the target, override cleared afterwards) without drift.
#[test]
fn giant_frame_jump_completes_curve_animation() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("jump"));
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::Opacity,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(1.0),
        Animation {
            curve: EasingCurve::EaseInOut,
            duration_secs: 0.3,
        },
    );
    h.run_frame(); // anchor

    h.advance_time(10_000); // way past the 300ms duration
    h.run_frame();

    // Completed → the animated override is cleared; resolve falls back to
    // the base style (fully opaque = 1.0, same as the target).
    let v = resolved_opacity(id);
    assert!(
        (v - 1.0).abs() < 1e-4,
        "after completion resolve = target 1.0, got {v}"
    );
}

/// `animator::register_animation` must actually drive a simulation-backed
/// animation and fire `on_finish` exactly once. (It was a silent no-op
/// compat shim before Phase 2.)
#[test]
fn simulation_animation_fires_on_finish_exactly_once() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("spring"));
    h.run_frame();

    let fired = Rc::new(Cell::new(0u32));
    let fired2 = fired.clone();
    let spring = SpringSimulation::with_damping_ratio(
        1.0, 180.0, 1.0, // critically damped
        0.0, 1.0, 0.0, 0.001,
    );
    animator::register_animation(
        id,
        animator::AnimatedProperty::Opacity,
        Box::new(spring),
        Some(Box::new(move || fired2.set(fired2.get() + 1))),
    );
    h.run_frame(); // anchor

    // Step in coarse chunks well past settling.
    for _ in 0..10 {
        h.advance_time(200);
        h.run_frame();
    }
    assert_eq!(fired.get(), 1, "on_finish must fire exactly once");

    let v = resolved_opacity(id);
    assert!((v - 1.0).abs() < 0.01, "spring settled at target, got {v}");
}

/// Vec2 payloads drive both axes of the position transform.
#[test]
fn position_vec2_animates_both_axes() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("move"));
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::Position,
        AnimatedValue::Vec2(Vec2::ZERO),
        AnimatedValue::Vec2(Vec2::new(40.0, 20.0)),
        Animation {
            curve: EasingCurve::Linear,
            duration_secs: 0.2,
        },
    );
    h.run_frame(); // anchor
    h.advance_time(100); // halfway
    h.run_frame();

    let off = h
        .find(id)
        .and_then(|el| el.position_offset())
        .expect("offset cell");
    let v = off.get();
    assert!((v.x - 20.0).abs() < 1.0, "x halfway = 20, got {}", v.x);
    assert!((v.y - 10.0).abs() < 1.0, "y halfway = 10, got {}", v.y);
}
