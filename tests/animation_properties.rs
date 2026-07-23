//! Property-surface expansion guards (audit 2026-07-18 animation pass,
//! "full property surface"): Foreground / BorderColor / BorderWidth /
//! Shadow / Rotation channels on the pure-function AnimationDriver.
//!
//! All five ride the existing machinery: StateStyle.animated fields are
//! already resolved by `resolve_style` (paint reads them — text fg via
//! `record_element_text`, borders/shadow via `paint_element_surface`),
//! and the damage AABB already includes `animated.shadow` growth plus
//! full transform corner-union. No paint-side changes are required.

use burin::animation::{self, AnimatedProperty, AnimatedValue, Animation, EasingCurve};
use burin::core::config::StateFlags;
use burin::core::element::Element;
use burin::style::styled::Shadow;
use burin::style::Color;
use burin::testing::TestHarness;
use burin::widgets::display::Text;

fn resolve(id: burin::core::ElementId, p: AnimatedProperty) -> AnimatedValue {
    Element::resolve_anim_property(id, StateFlags::NONE, p)
}

fn anim(curve: EasingCurve, secs: f32) -> Animation {
    Animation {
        curve,
        duration_secs: secs,
    }
}

/// Foreground (text color): black→white over 400ms, midpoint is mid-gray.
#[test]
fn foreground_color_animates_midpoint() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("colored"));
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::Foreground,
        AnimatedValue::Color(Color::rgba8(0, 0, 0, 255)),
        AnimatedValue::Color(Color::rgba8(255, 255, 255, 255)),
        anim(EasingCurve::Linear, 0.4),
    );
    h.run_frame(); // anchor
    h.advance_time(200);
    h.run_frame();

    match resolve(id, AnimatedProperty::Foreground) {
        AnimatedValue::Color(c) => {
            assert!((c.r - 0.5).abs() < 0.05, "midpoint fg.r ≈ 0.5, got {}", c.r);
        }
        other => panic!("expected Color, got {other:?}"),
    }
}

/// BorderWidth: 0→8 over 200ms, midpoint 4. BorderColor lerps alongside.
#[test]
fn border_width_and_color_animate() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("bordered"));
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::BorderWidth,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(8.0),
        anim(EasingCurve::Linear, 0.2),
    );
    animation::request_anim(
        id,
        AnimatedProperty::BorderColor,
        AnimatedValue::Color(Color::rgba8(255, 0, 0, 255)),
        AnimatedValue::Color(Color::rgba8(0, 0, 255, 255)),
        anim(EasingCurve::Linear, 0.2),
    );
    h.run_frame(); // anchor
    h.advance_time(100);
    h.run_frame();

    match resolve(id, AnimatedProperty::BorderWidth) {
        AnimatedValue::Float(w) => assert!((w - 4.0).abs() < 0.5, "midpoint width ≈ 4, got {w}"),
        other => panic!("expected Float, got {other:?}"),
    }
    match resolve(id, AnimatedProperty::BorderColor) {
        AnimatedValue::Color(c) => {
            assert!(
                (c.r - 0.5).abs() < 0.05 && (c.b - 0.5).abs() < 0.05,
                "midpoint border color ≈ mid red/blue, got r={} b={}",
                c.r,
                c.b
            );
        }
        other => panic!("expected Color, got {other:?}"),
    }
}

/// Shadow: blur 0→20 + color fade, midpoint blur 10.
#[test]
fn shadow_animates_midpoint() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("shadowed"));
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::Shadow,
        AnimatedValue::Shadow(Shadow::new(Color::rgba8(0, 0, 0, 0), 0.0, 0.0, 0.0)),
        AnimatedValue::Shadow(Shadow::new(Color::rgba8(0, 0, 0, 200), 0.0, 6.0, 20.0)),
        anim(EasingCurve::Linear, 0.2),
    );
    h.run_frame(); // anchor
    h.advance_time(100);
    h.run_frame();

    match resolve(id, AnimatedProperty::Shadow) {
        AnimatedValue::Shadow(s) => {
            assert!(
                (s.blur - 10.0).abs() < 1.0,
                "midpoint blur ≈ 10, got {}",
                s.blur
            );
            assert!(
                (s.offset_y - 3.0).abs() < 0.5,
                "midpoint offset_y ≈ 3, got {}",
                s.offset_y
            );
        }
        other => panic!("expected Shadow, got {other:?}"),
    }
}

/// Rotation: 0→90° over 200ms, midpoint ≈45°; the transform matrix is
/// cleared (back to None / 0°) once the animation completes.
#[test]
fn rotation_animates_and_clears_on_completion() {
    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(Text::new("spinning"));
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::Rotation,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(90.0),
        anim(EasingCurve::Linear, 0.2),
    );
    h.run_frame(); // anchor
    h.advance_time(100);
    h.run_frame();

    match resolve(id, AnimatedProperty::Rotation) {
        AnimatedValue::Float(deg) => {
            assert!(
                (deg - 45.0).abs() < 2.0,
                "midpoint rotation ≈ 45°, got {deg}"
            );
        }
        other => panic!("expected Float degrees, got {other:?}"),
    }

    h.advance_time(500); // past the end
    h.run_frame();
    h.run_frame();
    match resolve(id, AnimatedProperty::Rotation) {
        AnimatedValue::Float(deg) => {
            assert!(
                deg.abs() < 0.01,
                "rotation override cleared after completion, got {deg}"
            );
        }
        other => panic!("expected Float degrees, got {other:?}"),
    }
}
