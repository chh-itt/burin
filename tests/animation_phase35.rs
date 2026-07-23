//! Phase 3.5 guards: layout-class height animation for Accordion.
//!
//! Height animation runs in the Prepass phase (frame_tick + MEASURE) —
//! the animation-driver phase runs after layout, so a driver-side height
//! channel could never reach taffy in the same frame. The tick computes
//! `f(clock::animation_millis())` (pure time query) and renews an
//! element wake, so the animation self-sustains and decays on completion.

use auralis_signal::Signal;
use burin::animation::set_animations_enabled;
use burin::testing::TestHarness;
use burin::widgets::composite::Accordion;
use burin::widgets::display::Skeleton;
use std::collections::HashSet;

fn mount_accordion(h: &mut TestHarness, open: &Signal<HashSet<usize>>) -> burin::core::ElementId {
    h.mount(Accordion::new(open.clone()).section(
        "Section A",
        Skeleton::new().rect(120.0, 80.0).animated(false),
    ))
}

fn content_id(h: &TestHarness, acc: burin::core::ElementId) -> burin::core::ElementId {
    // Accordion children: [header button, content container] per section.
    let el = h.find(acc).expect("accordion mounted");
    *el.children.get(1).expect("content container exists")
}

fn content_height(h: &TestHarness, id: burin::core::ElementId) -> f32 {
    h.find(id).map(|el| el.screen_bounds.height).unwrap_or(0.0)
}

/// Collapse must animate: mid-transition the content still occupies an
/// intermediate height (slot stays active), and only on completion does
/// it deactivate to zero.
#[test]
fn accordion_collapse_animates_height() {
    set_animations_enabled(true);
    let mut h = TestHarness::new(500.0, 400.0);
    let open = Signal::new(HashSet::from([0usize]));
    let acc = mount_accordion(&mut h, &open);
    h.run_frame();
    h.run_frame();

    let cid = content_id(&h, acc);
    let full = content_height(&h, cid);
    assert!(full > 40.0, "expanded content has real height, got {full}");

    // Toggle closed.
    open.update(|s| {
        s.clear();
    });
    h.run_frame(); // animation starts

    h.advance_time(100); // mid-flight of the 200ms transition
    h.run_frame();
    let mid = content_height(&h, cid);
    assert!(
        mid > 4.0 && mid < full - 4.0,
        "mid-collapse height must be intermediate: {mid} of {full}"
    );

    h.advance_time(300); // past the end
    h.run_frame();
    h.run_frame();
    let el = h.find(cid).unwrap();
    assert!(
        el.slot_inactive.get(),
        "collapsed content deactivates after the animation"
    );
    set_animations_enabled(false);
}

/// Re-expanding animates from 0 back to the recorded height, and settles
/// at the natural (child-driven) height afterwards.
#[test]
fn accordion_expand_animates_height() {
    set_animations_enabled(true);
    let mut h = TestHarness::new(500.0, 400.0);
    let open = Signal::new(HashSet::from([0usize]));
    let acc = mount_accordion(&mut h, &open);
    h.run_frame();
    h.run_frame();
    let cid = content_id(&h, acc);
    let full = content_height(&h, cid);

    // Collapse fully (records the height), then re-expand.
    open.update(|s| {
        s.clear();
    });
    h.run_frame();
    h.advance_time(500);
    h.run_frame();
    h.run_frame();

    open.update(|s| {
        s.insert(0);
    });
    h.run_frame(); // expand animation starts

    h.advance_time(100); // mid-flight
    h.run_frame();
    let mid = content_height(&h, cid);
    assert!(
        mid > 4.0 && mid < full - 4.0,
        "mid-expand height must be intermediate: {mid} of {full}"
    );

    h.advance_time(300);
    h.run_frame();
    h.run_frame();
    let settled = content_height(&h, cid);
    assert!(
        (settled - full).abs() < 2.0,
        "expanded content settles at natural height {full}, got {settled}"
    );
    set_animations_enabled(false);
}

/// With animations disabled (the default), toggling stays instant —
/// the pre-Phase-3.5 semantics are preserved bit-for-bit.
#[test]
fn accordion_instant_when_animations_disabled() {
    set_animations_enabled(false);
    let mut h = TestHarness::new(500.0, 400.0);
    let open = Signal::new(HashSet::from([0usize]));
    let acc = mount_accordion(&mut h, &open);
    h.run_frame();
    h.run_frame();
    let cid = content_id(&h, acc);

    open.update(|s| {
        s.clear();
    });
    h.run_frame();
    let el = h.find(cid).unwrap();
    assert!(
        el.slot_inactive.get(),
        "instant collapse with animations off"
    );
}
