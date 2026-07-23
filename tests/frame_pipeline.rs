//! Tests for the shared frame_pipeline phases (extracted from on_frame/run_frame).

use burin::testing::TestHarness;
use burin::widgets::display::Text;
use std::cell::Cell;
use std::rc::Rc;
#[test]
fn run_pre_passes_fires_frame_tick_in_harness() {
    // A frame_tick callback must fire every frame via run_pre_passes.
    // Before harness integration the harness never ran the pre-passes, so
    // the tick would never fire — this test goes red until integration.
    let counter = Rc::new(Cell::new(0u32));
    let c = counter.clone();

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Text::new("tick"));
    h.find_mut(id)
        .unwrap()
        .set_frame_tick(Box::new(move || c.set(c.get() + 1)));

    h.run_frame();
    assert!(
        counter.get() >= 1,
        "frame_tick must fire via run_pre_passes, got {}",
        counter.get(),
    );
}

#[test]
fn animation_frame_propagates_dirty_and_triggers_paint() {
    use burin::animation::{request_anim, AnimatedProperty, AnimatedValue, Animation, EasingCurve};

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Text::new("anim"));
    h.run_frame(); // initial paint
    h.run_frame(); // settle

    request_anim(
        id,
        AnimatedProperty::Opacity,
        AnimatedValue::Float(1.0),
        AnimatedValue::Float(0.5),
        Animation {
            curve: EasingCurve::Linear,
            duration_secs: 1.0,
        },
    );
    h.run_frame(); // drain + first tick
    h.advance_time(500);
    h.run_frame(); // mid-animation tick

    let el = h.find(id).unwrap();
    // Opacity must have progressed (mid-animation).
    assert!(
        el.resolved_opacity() > 0.5 && el.resolved_opacity() < 0.85,
        "opacity must progress, got {}",
        el.resolved_opacity(),
    );
    // The REPAINT flag must be cleared — paint actually happened.
    // Without recheck_dirty_phase the animation's mark_repaint never reaches
    // root_flags, the harness paint gate stays closed, and needs_repaint()
    // remains true after the frame.
    assert!(
        !el.needs_repaint(),
        "animation element must be painted clean (recheck propagates dirty)",
    );
}
