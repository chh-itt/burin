//! Phase 3 animation-architecture guards: visibility gating.
//!
//! Animations targeting reactive-hidden subtrees must cost nothing:
//! no dirty registration, no repaint, no continuous wake. Time keeps
//! flowing (values are pure functions of the clock), so revealing the
//! subtree resumes at the correct phase, and completions fire on the
//! next wake after their deadline.

use auralis_signal::Signal;
use burin::animation::{self, AnimatedProperty, AnimatedValue, Animation, EasingCurve};
use burin::core::config::StateFlags;
use burin::core::element::Element;
use burin::core::scheduler;
use burin::testing::TestHarness;
use burin::widgets::display::{Progress, Text};
use burin::widgets::layout::{ScrollView, SizedBox, VStack};
use std::cell::Cell;
use std::rc::Rc;

fn resolved_opacity(id: burin::core::ElementId) -> f32 {
    match Element::resolve_anim_property(id, StateFlags::NONE, AnimatedProperty::Opacity) {
        AnimatedValue::Float(f) => f,
        other => panic!("expected Float opacity, got {other:?}"),
    }
}

/// An animation on a hidden subtree must not paint or dirty anything.
#[test]
fn hidden_animation_produces_no_frames() {
    let mut h = TestHarness::new(300.0, 200.0);
    let vis: Rc<Cell<bool>> = Rc::new(Cell::new(true));
    let id = h.mount(Text::new("ghost"));
    h.find_mut(id).unwrap().set_reactive_visible(vis.clone());
    h.run_frame();

    vis.set(false);
    h.find_mut(id).unwrap().mark_repaint();
    h.run_frame(); // settle the hide
    h.run_frame();

    animation::request_anim(
        id,
        AnimatedProperty::Opacity,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(1.0),
        Animation {
            curve: EasingCurve::Linear,
            duration_secs: 0.5,
        },
    );
    for i in 0..3 {
        h.advance_time(50);
        h.run_frame();
        assert!(
            !h.last_painted(),
            "frame {i}: hidden animation target must not repaint"
        );
    }
}

/// Revealing a subtree mid-animation resumes at the correct phase —
/// time did NOT pause while hidden (pure-function timeline).
#[test]
fn revealed_animation_resumes_at_correct_phase() {
    let mut h = TestHarness::new(300.0, 200.0);
    let vis: Rc<Cell<bool>> = Rc::new(Cell::new(true));
    let id = h.mount(Text::new("phase"));
    h.find_mut(id).unwrap().set_reactive_visible(vis.clone());
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
    h.run_frame(); // anchor at t=0 (visible)

    vis.set(false);
    h.find_mut(id).unwrap().mark_repaint();
    h.run_frame();

    h.advance_time(200); // hidden for half the duration
    h.run_frame();

    vis.set(true);
    h.find_mut(id).unwrap().mark_repaint();
    h.advance_time(100); // t = 300ms of 400ms
    h.run_frame();

    let v = resolved_opacity(id);
    assert!(
        (v - 0.75).abs() < 0.05,
        "revealed at t=300ms of 400ms linear → 0.75, got {v} (time must not pause while hidden)"
    );
}

/// A hidden indeterminate spinner must release its continuous wake
/// (renewal model: the wake only survives frames where its tick ran).
#[test]
fn hidden_spinner_releases_wake() {
    let mut h = TestHarness::new(300.0, 200.0);
    let vis: Rc<Cell<bool>> = Rc::new(Cell::new(true));
    let spinner = h.mount(VStack::new().push(Progress::new(Signal::new(0.0)).indeterminate()));
    h.find_mut(spinner)
        .unwrap()
        .set_reactive_visible(vis.clone());
    h.run_frame();
    h.run_frame();
    assert!(
        scheduler::has_continuous(),
        "visible spinner holds the wake"
    );

    vis.set(false);
    h.find_mut(spinner).unwrap().mark_repaint();
    h.run_frame(); // hide frame — tick skipped, wake not renewed
    h.run_frame(); // sweep frame
    assert!(
        !scheduler::has_continuous(),
        "hidden spinner must release its continuous wake (renewal model)"
    );

    vis.set(true);
    h.find_mut(spinner).unwrap().mark_repaint();
    h.run_frame();
    assert!(
        scheduler::has_continuous(),
        "re-revealed spinner re-acquires the wake"
    );
}

/// A spinner scrolled out of the viewport must stop producing frames and
/// release its wake — an offscreen animation costs nothing. Scrolling it
/// back revives it (scrolling always produces frames, so the tick gets a
/// chance to re-evaluate viewport intersection).
#[test]
fn offscreen_spinner_sleeps_and_scrolling_back_revives_it() {
    let mut h = TestHarness::new(300.0, 200.0);
    let scroll_id = h.mount(
        ScrollView::new().child(
            VStack::new()
                .push(SizedBox::new().width(100.0).height(1200.0))
                .push(Progress::new(Signal::new(0.0)).indeterminate()),
        ),
    );
    h.run_frame();
    h.run_frame();
    // The spinner sits below a 1200px spacer in a 200px viewport → offscreen.
    h.run_frame();
    h.run_frame();
    assert!(
        !scheduler::has_continuous(),
        "offscreen spinner must not hold a continuous wake"
    );
    h.run_frame();
    assert!(
        !h.last_painted(),
        "offscreen spinner must not produce paint frames"
    );

    // Scroll it into view (wheel semantics: negative dy scrolls down).
    h.scroll(scroll_id, 0.0, -1150.0);
    h.run_frame();
    h.run_frame();
    assert!(
        scheduler::has_continuous(),
        "spinner scrolled into view re-acquires the wake"
    );
    h.run_frame();
    assert!(h.last_painted(), "on-screen spinner repaints again");
}

/// A driver animation whose target is scrolled offscreen skips its
/// per-frame dirty work (no repaint churn) while time keeps flowing.
#[test]
fn offscreen_driver_animation_is_quiescent() {
    fn find_below(
        h: &TestHarness,
        root: burin::core::ElementId,
        y_min: f32,
    ) -> Option<burin::core::ElementId> {
        let el = h.find(root)?;
        if el.children.is_empty() && el.screen_bounds.y >= y_min {
            return Some(root);
        }
        let children = el.children.clone();
        for c in children {
            if let Some(hit) = find_below(h, c, y_min) {
                return Some(hit);
            }
        }
        None
    }

    let mut h = TestHarness::new(300.0, 200.0);
    let id = h.mount(
        ScrollView::new().child(
            VStack::new()
                .push(SizedBox::new().width(100.0).height(1200.0))
                .push(Text::new("way below the fold")),
        ),
    );
    h.run_frame();
    h.run_frame();
    let text_id = find_below(&h, id, 1100.0).expect("offscreen text located");

    animation::request_anim(
        text_id,
        AnimatedProperty::Opacity,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(1.0),
        Animation {
            curve: EasingCurve::Linear,
            duration_secs: 10.0,
        }, // long-running
    );
    h.run_frame(); // anchor
    for i in 0..3 {
        h.advance_time(50);
        h.run_frame();
        assert!(
            !h.last_painted(),
            "frame {i}: offscreen driver animation must not repaint"
        );
    }
}
