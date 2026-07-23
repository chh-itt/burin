//! Deterministic time tests — verify that time-driven features progress
//! when the virtual clock is advanced, without relying on wall-clock.

use burin::animation::{request_anim, AnimatedProperty, AnimatedValue, Animation, EasingCurve};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::VStack;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn virtual_time_drives_animation() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(VStack::new().push(Text::new("animated")));
    h.run_frame();

    // Verify virtual clock is active.
    assert!(burin::core::clock::is_virtual());

    // Request an opacity animation: 0.0 → 1.0 over 300ms.
    request_anim(
        id,
        AnimatedProperty::Opacity,
        AnimatedValue::Float(0.0),
        AnimatedValue::Float(1.0),
        Animation {
            curve: EasingCurve::Linear,
            duration_secs: 0.3,
        },
    );

    // Run one frame to drain the pending request into the driver.
    h.run_frame();

    // At t ≈ 0 (first tick has dt=0 since last_time was None), opacity
    // should still be the starting value or very close to 0.
    let el = h.find(id).unwrap();
    assert!(
        el.resolved_opacity() < 0.05,
        "opacity at t≈0 should be near 0, got {}",
        el.resolved_opacity()
    );

    // Advance virtual time by 150ms (half of 300ms). With linear easing,
    // opacity should be ~0.5.
    h.advance_time(150).run_frame();
    let el = h.find(id).unwrap();
    assert!(
        el.resolved_opacity() > 0.4 && el.resolved_opacity() < 0.6,
        "opacity at t=150ms should be ~0.5, got {}",
        el.resolved_opacity(),
    );

    // Advance to 300ms — animation should complete → opacity = 1.0.
    h.advance_time(150).run_frame();
    let el = h.find(id).unwrap();
    assert!(
        el.resolved_opacity() > 0.95,
        "opacity at t=300ms should be near 1.0, got {}",
        el.resolved_opacity(),
    );
}

#[test]
fn virtual_clock_is_installed_by_harness() {
    let _h = TestHarness::new(100.0, 100.0);
    assert!(burin::core::clock::is_virtual());
    let t0 = burin::core::clock::now();
    // Without advancing, repeated reads return the same instant.
    assert_eq!(burin::core::clock::now(), t0);
}

#[test]
fn advance_time_moves_virtual_clock() {
    let mut h = TestHarness::new(100.0, 100.0);
    let t0 = burin::core::clock::now();
    h.advance_time(500);
    let t1 = burin::core::clock::now();
    let delta = t1.duration_since(t0);
    assert_eq!(delta.as_millis(), 500);
}

#[test]
fn virtual_time_drives_async_timer() {
    let mut h = TestHarness::new(400.0, 200.0);
    let done = Rc::new(Cell::new(false));
    let d = Rc::clone(&done);

    // Spawn an async task that sleeps for 200ms then sets the flag.
    auralis_task::spawn_global(async move {
        auralis_task::timer::sleep(std::time::Duration::from_millis(200)).await;
        d.set(true);
    });

    // Not yet: the timer hasn't expired.
    h.run_frame();
    assert!(!done.get(), "timer should not have fired yet");

    // Advance well past the deadline and run a frame.
    // The VirtualTimeSource bridges clock::now() → task timer,
    // and flush_all() in run_frame() processes expired timers.
    h.advance_time(300).run_frame();
    assert!(done.get(), "timer should have fired after 300ms advance");
}

#[test]
fn async_timer_does_not_fire_before_deadline() {
    let mut h = TestHarness::new(400.0, 200.0);
    let done = Rc::new(Cell::new(false));
    let d = Rc::clone(&done);

    auralis_task::spawn_global(async move {
        auralis_task::timer::sleep(std::time::Duration::from_millis(500)).await;
        d.set(true);
    });

    h.run_frame();
    // Advance only 100ms — deadline is 500ms, should not fire.
    h.advance_time(100).run_frame();
    assert!(
        !done.get(),
        "timer should NOT fire at 100ms (deadline is 500ms)"
    );

    // Advance past the remaining 400ms.
    h.advance_time(500).run_frame();
    assert!(done.get(), "timer should fire after 600ms total");
}
