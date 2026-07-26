//! Comprehensive test suite for the incremental Taffy layout system.
//!
//! Covers: first-frame full build, MEASURE-only, REPOSITION-only,
//! structural changes (add/remove/slot_inactive), cross-check vs full rebuild,
//! and regression tests for bugs #1 and #2.

use auralis_signal::Signal;
use crate::core::dirty_registry;
use crate::core::ElementId;
use crate::layout::taffy_bridge::TaffyBridge;
use crate::style::Rect;
use crate::style::Styled;
use crate::testing::TestHarness;
use crate::widgets::display::Text;
use crate::widgets::input::{Button, TextInput};
use crate::widgets::layout::{Conditional, HStack, VStack};
use std::collections::HashMap;

// ── Helper: run a full rebuild from scratch on a separate TaffyBridge ──

fn compute_full_bounds(harness: &TestHarness) -> HashMap<ElementId, Rect> {
    let mut taffy = TaffyBridge::new();
    let root_id = harness.root_id();
    taffy.clear();
    let root_node = taffy.build_full_tree(harness.root(), root_id);
    let size = harness.size();
    // Preserve the existing flex style, only update size
    if let Ok(mut s) = taffy.tree.style(root_node).cloned() {
        s.size.width = taffy::style::Dimension::length(size.width);
        s.size.height = taffy::style::Dimension::length(size.height);
        let _ = taffy.tree.set_style(root_node, s);
    }
    let results = taffy.compute_layout(root_node, size);
    results.into_iter().collect()
}

fn bounds_of(harness: &TestHarness, id: ElementId) -> Rect {
    harness.find(id).unwrap().screen_bounds
}

// ═══════════════════════ A. Layout Path Correctness ═══════════════════════

#[test]
fn first_frame_full_build_has_bounds() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(
        VStack::new()
            .push(Text::new("Hello"))
            .push(Button::new("OK").primary()),
    );
    h.run_frame();

    let root = h.find(h.root_id()).unwrap();
    // Root should have children
    assert!(
        !root.children.is_empty(),
        "Root should have children after mount"
    );
    // All elements should have non-zero bounds
    let mut count = 0;
    for (eid, el) in h.root().iter() {
        let b = el.screen_bounds;
        assert!(
            b.width > 0.0 && b.height > 0.0,
            "Element {:?} bounds should be positive, got {:?}",
            eid,
            b
        );
        count += 1;
    }
    assert!(
        count >= 3,
        "Expected at least 3 elements (root + VStack + Text + Button)"
    );
}

#[test]
fn first_frame_taffy_nodes_match_elements() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(
        VStack::new()
            .push(Text::new("A"))
            .push(Text::new("B"))
            .push(Text::new("C")),
    );
    h.run_frame();

    // Every element in the arena (except those with slot_inactive) should
    // have a taffy node
    for (eid, el) in h.root().iter() {
        if el.is_visible() && !el.slot_inactive.get() {
            assert!(
                h.taffy_node(eid).is_some(),
                "Visible element {:?} should have a taffy node",
                eid
            );
        }
    }
}

#[test]
fn measure_only_text_width_grows() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sig = Signal::new(String::from("Hi"));
    let _text_id = h.mount(VStack::new().push(Text::new(String::new()).bind(sig.clone())));
    h.run_frame();

    // Find the Text child inside VStack
    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let text_eid = h.find(vstack_id).unwrap().children[0];
    let old_width = bounds_of(&h, text_eid).width;

    // Change to a much longer string
    h.set_signal(&sig, String::from("This is a much longer text string"));
    h.run_frame(); // Frame N: paint measures new width, marks MEASURE
    h.run_frame(); // Frame N+1: layout picks up new width

    let new_width = bounds_of(&h, text_eid).width;
    assert!(
        new_width > old_width + 5.0,
        "Text width should grow after signal change: {} -> {}",
        old_width,
        new_width
    );
}

#[test]
fn repaint_only_skips_taffy() {
    let mut h = TestHarness::new(800.0, 600.0);
    let _btn_id = h.mount(VStack::new().push(Button::new("Hover me").primary()));
    h.run_frame();

    // First, clear all dirty state
    let _ = dirty_registry::take_dirty();
    // Simulate hover (should set HOVERED state, which marks REPAINT)
    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let btn_eid = h.find(vstack_id).unwrap().children[0];
    h.find_mut(btn_eid)
        .unwrap()
        .set_state_dirty(crate::core::config::StateFlags::HOVERED, true);

    // Run frame — should not trigger MEASURE
    h.run_frame();
    let has_measure = h
        .find(h.root_id())
        .map_or(false, |r| r.dirty.get().has_measure());
    assert!(
        !has_measure,
        "REPAINT-only change should NOT trigger MEASURE/taffy layout"
    );

    // Bounds should remain the same
    let b = bounds_of(&h, btn_eid);
    assert!(b.width > 0.0, "Button should still have valid bounds");
}

#[test]
fn reposition_only_resize_updates_root_bounds() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(VStack::new().push(Text::new("Resize test")));
    h.run_frame();

    // Clear dirty state
    let _ = dirty_registry::take_dirty();
    dirty_registry::drain_structurally_changed();

    // Resize and run frame
    h.resize(400.0, 300.0);
    h.run_frame();

    let root_bounds = bounds_of(&h, h.root_id());
    assert_eq!(root_bounds.width, 400.0, "Root width should match new size");
    assert_eq!(
        root_bounds.height, 300.0,
        "Root height should match new size"
    );
}

// ═══════════════════════ B. Structural Changes ═══════════════════════

#[test]
fn conditional_toggle_swaps_visible_child() {
    let mut h = TestHarness::new(800.0, 600.0);
    let condition = Signal::new(true);
    h.mount(VStack::new().push(Conditional::new(
        condition.clone(),
        Text::new("TRUE"),
        Text::new("FALSE"),
    )));
    h.run_frame();

    // Find the Conditional container and its children
    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let cond_id = h.find(vstack_id).unwrap().children[0];
    let cond = h.find(cond_id).unwrap();
    assert_eq!(cond.children.len(), 2, "Conditional should have 2 children");

    let child_a = cond.children[0];
    let child_b = cond.children[1];

    // Initially, child_a (TRUE) should be active (slot_inactive = false)
    assert!(
        !h.find(child_a).unwrap().slot_inactive.get(),
        "TRUE child should be active initially"
    );
    assert!(
        h.find(child_b).unwrap().slot_inactive.get(),
        "FALSE child should be inactive initially"
    );

    // Toggle to false
    h.set_signal(&condition, false);
    h.run_frame();

    // Now child_b (FALSE) should be active
    assert!(
        h.find(child_a).unwrap().slot_inactive.get(),
        "TRUE child should now be inactive"
    );
    assert!(
        !h.find(child_b).unwrap().slot_inactive.get(),
        "FALSE child should now be active"
    );
}

#[test]
fn conditional_node_id_preserved_after_toggle() {
    let mut h = TestHarness::new(800.0, 600.0);
    let condition = Signal::new(true);
    let _cond_id = h.mount(VStack::new().push(Conditional::new(
        condition.clone(),
        Text::new("A"),
        Text::new("B"),
    )));
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let cond_eid = h.find(vstack_id).unwrap().children[0];

    let node_before = h.taffy_node(cond_eid);
    assert!(
        node_before.is_some(),
        "Conditional should have a taffy node"
    );

    // Toggle
    h.set_signal(&condition, false);
    h.run_frame();

    let node_after = h.taffy_node(cond_eid);
    assert_eq!(node_before, node_after,
        "Conditional NodeId should be preserved after structural change (same node, children updated)");
}

#[test]
fn conditional_when_single_branch_toggle() {
    let mut h = TestHarness::new(800.0, 600.0);
    let condition = Signal::new(true);
    h.mount(VStack::new().push(Conditional::when(condition.clone(), Text::new("VISIBLE"))));
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let cond_id = h.find(vstack_id).unwrap().children[0];
    let cond = h.find(cond_id).unwrap();
    assert_eq!(cond.children.len(), 1);

    let child = cond.children[0];
    assert!(
        !h.find(child).unwrap().slot_inactive.get(),
        "Single child should be active"
    );

    // Toggle off
    h.set_signal(&condition, false);
    h.run_frame();

    assert!(
        h.find(child).unwrap().slot_inactive.get(),
        "Child should now be inactive"
    );
}

#[test]
fn multiple_conditionals_toggle_in_same_frame() {
    let mut h = TestHarness::new(800.0, 600.0);
    let cond_a = Signal::new(true);
    let cond_b = Signal::new(false);

    h.mount(
        VStack::new()
            .push(Conditional::new(
                cond_a.clone(),
                Text::new("A-TRUE"),
                Text::new("A-FALSE"),
            ))
            .push(Conditional::new(
                cond_b.clone(),
                Text::new("B-TRUE"),
                Text::new("B-FALSE"),
            )),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let ca_id = h.find(vstack_id).unwrap().children[0];
    let cb_id = h.find(vstack_id).unwrap().children[1];

    {
        let ca = h.find(ca_id).unwrap();
        let cb = h.find(cb_id).unwrap();
        // cond_a=true → child[0] (A-TRUE) active, child[1] (A-FALSE) inactive
        assert!(!h.find(ca.children[0]).unwrap().slot_inactive.get());
        assert!(h.find(ca.children[1]).unwrap().slot_inactive.get());
        // cond_b=false → child[0] (B-TRUE) inactive, child[1] (B-FALSE) active
        assert!(h.find(cb.children[0]).unwrap().slot_inactive.get());
        assert!(!h.find(cb.children[1]).unwrap().slot_inactive.get());
    }

    // Flip both in same frame
    h.set_signal(&cond_a, false);
    h.set_signal(&cond_b, true);
    h.run_frame();

    let ca = h.find(ca_id).unwrap();
    let cb = h.find(cb_id).unwrap();
    // cond_a=false → A-TRUE inactive, A-FALSE active
    assert!(h.find(ca.children[0]).unwrap().slot_inactive.get());
    assert!(!h.find(ca.children[1]).unwrap().slot_inactive.get());
    // cond_b=true → B-TRUE active, B-FALSE inactive
    assert!(!h.find(cb.children[0]).unwrap().slot_inactive.get());
    assert!(h.find(cb.children[1]).unwrap().slot_inactive.get());
}

#[test]
fn structural_after_text_binding_coexists() {
    // MEASURE dirty (text change) + structural change in same frame
    let mut h = TestHarness::new(800.0, 600.0);
    let label = Signal::new(String::from("Short"));
    let cond = Signal::new(true);

    h.mount(
        VStack::new()
            .push(Text::new(String::new()).bind(label.clone()))
            .push(Conditional::when(cond.clone(), Text::new("Extra"))),
    );
    h.run_frame();

    // Change both signals in same frame
    h.set_signal(&label, String::from("Much longer text here"));
    h.set_signal(&cond, false);
    h.run_frame();

    // Should not panic — both paths should work
    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    assert!(h.find(h.find(vstack_id).unwrap().children[0]).is_some());
}

// ═══════════════════════ C. Cross-Check: Incremental vs Full Rebuild ══════

#[test]
fn cross_check_measure_incremental_equals_full() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sig = Signal::new(String::from("A"));
    h.mount(VStack::new().push(Text::new(String::new()).bind(sig.clone())));
    h.run_frame();

    // Change text
    h.set_signal(&sig, String::from("ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
    h.run_frame(); // Frame N: paint measures
    h.run_frame(); // Frame N+1: layout applies new width

    let incremental_bounds: HashMap<ElementId, Rect> = h
        .root()
        .iter()
        .map(|(eid, el)| (eid, el.screen_bounds))
        .collect();

    // Full rebuild bounds
    let full_bounds = compute_full_bounds(&h);

    for (eid, ib) in &incremental_bounds {
        let fb = full_bounds.get(eid);
        if let Some(fb) = fb {
            assert!(
                (ib.x - fb.x).abs() < 2.0
                    && (ib.y - fb.y).abs() < 2.0
                    && (ib.width - fb.width).abs() < 2.0
                    && (ib.height - fb.height).abs() < 2.0,
                "Incremental bounds {:?} differ from full rebuild {:?} for {:?}",
                ib,
                fb,
                eid,
            );
        }
    }
}

#[test]
fn cross_check_structural_incremental_equals_full() {
    let mut h = TestHarness::new(800.0, 600.0);
    let condition = Signal::new(true);
    h.mount(VStack::new().push(Conditional::new(
        condition.clone(),
        Text::new("ON"),
        Text::new("OFF"),
    )));
    h.run_frame();

    // Toggle
    h.set_signal(&condition, false);
    h.run_frame();

    let incremental_bounds: HashMap<ElementId, Rect> = h
        .root()
        .iter()
        .map(|(eid, el)| (eid, el.screen_bounds))
        .collect();

    let full_bounds = compute_full_bounds(&h);

    for (eid, ib) in &incremental_bounds {
        let fb = full_bounds.get(eid);
        if let Some(fb) = fb {
            assert!(
                (ib.x - fb.x).abs() < 2.0
                    && (ib.y - fb.y).abs() < 2.0
                    && (ib.width - fb.width).abs() < 2.0
                    && (ib.height - fb.height).abs() < 2.0,
                "Incremental bounds {:?} differ from full rebuild {:?} for {:?}",
                ib,
                fb,
                eid,
            );
        }
    }
}

#[test]
fn cross_check_reposition_incremental_equals_full() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(
        VStack::new()
            .push(Text::new("Top"))
            .push(Text::new("Middle"))
            .push(Text::new("Bottom")),
    );
    h.run_frame();

    // Resize
    h.resize(400.0, 300.0);
    h.run_frame();

    let incremental_bounds: HashMap<ElementId, Rect> = h
        .root()
        .iter()
        .map(|(eid, el)| (eid, el.screen_bounds))
        .collect();

    let full_bounds = compute_full_bounds(&h);

    for (eid, ib) in &incremental_bounds {
        let fb = full_bounds.get(eid);
        if let Some(fb) = fb {
            assert!(
                (ib.x - fb.x).abs() < 2.0
                    && (ib.y - fb.y).abs() < 2.0
                    && (ib.width - fb.width).abs() < 2.0
                    && (ib.height - fb.height).abs() < 2.0,
                "Incremental bounds {:?} differ from full rebuild {:?} for {:?}",
                ib,
                fb,
                eid,
            );
        }
    }
}

// ═══════════════════════ D. Regression Tests ═══════════════════════

#[test]
fn regression_bug1_text_width_grows_with_signal() {
    // Bug #1: text width didn't update after signal change because
    // only REPAINT was marked (not MEASURE), and LazyFontParams.max_width
    // was a stale fixed value causing text wrapping.
    let mut h = TestHarness::new(800.0, 600.0);
    let sig = Signal::new(String::from("Clicks: 0"));
    h.mount(VStack::new().push(Text::new(String::new()).bind(sig.clone())));
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let text_eid = h.find(vstack_id).unwrap().children[0];
    let w9 = bounds_of(&h, text_eid).width;

    // Change to "Clicks: 10" — longer text, width must grow
    h.set_signal(&sig, String::from("Clicks: 10"));
    h.run_frame(); // Frame N: paint measures new width, marks MEASURE
    h.run_frame(); // Frame N+1: layout applies new width
    let w10 = bounds_of(&h, text_eid).width;

    assert!(
        w10 > w9 + 2.0,
        "Text width must grow when text content lengthens: {} -> {}",
        w9,
        w10
    );
}

#[test]
fn regression_bug2_conditional_toggle_no_panic() {
    // Bug #2: Conditional toggle would panic because ensure_subtree
    // removed the container NodeId from taffy, then relink_to_parent
    // tried to access the stale node.
    let mut h = TestHarness::new(800.0, 600.0);
    let condition = Signal::new(true);
    h.mount(
        VStack::new().push(Conditional::new(
            condition.clone(),
            VStack::new()
                .push(Text::new("Nested A"))
                .push(Text::new("Deep A")),
            VStack::new()
                .push(Text::new("Nested B"))
                .push(Text::new("Deep B")),
        )),
    );
    h.run_frame();

    // Toggle multiple times — should never panic
    for i in 0..5 {
        let val = i % 2 == 0;
        h.set_signal(&condition, val);
        h.run_frame();

        // Verify the correct child is active
        let vstack_id = h.find(h.root_id()).unwrap().children[0];
        let cond_id = h.find(vstack_id).unwrap().children[0];
        let cond = h.find(cond_id).unwrap();
        assert!(
            !h.find(cond.children[0]).unwrap().slot_inactive.get() == val,
            "Toggle round {}: TRUE child active state mismatch",
            i
        );
        assert!(
            h.find(cond.children[1]).unwrap().slot_inactive.get() == val,
            "Toggle round {}: FALSE child active state mismatch",
            i
        );
    }
}

#[test]
fn regression_taffy_persistence_across_frames() {
    // Ensure the taffy tree is persistent: NodeId for root doesn't
    // change across frames without structural changes.
    let mut h = TestHarness::new(800.0, 600.0);
    let btn = Button::new("Test").primary();
    h.mount(VStack::new().push(btn));
    h.run_frame();

    let root_node_1 = h.taffy_node(h.root_id());

    // Run several REPAINT-only frames
    for _ in 0..3 {
        let vstack_id = h.find(h.root_id()).unwrap().children[0];
        h.find_mut(vstack_id).unwrap().mark_repaint();
        h.run_frame();
    }

    let root_node_2 = h.taffy_node(h.root_id());
    assert_eq!(
        root_node_1, root_node_2,
        "Root taffy NodeId should be persistent across REPAINT-only frames"
    );
}

// ═══════════════════════ E. Edge Cases ═══════════════════════

#[test]
fn empty_structural_drain_goes_to_measure_path() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sig = Signal::new(String::from("Start"));
    h.mount(VStack::new().push(Text::new(String::new()).bind(sig.clone())));
    h.run_frame();

    // Drain structural set (should be empty after first run_frame)
    let structural = dirty_registry::drain_structurally_changed();
    assert!(
        structural.is_empty(),
        "Structural set should be empty after clean frame"
    );

    // Change text signal (MEASURE, not structural)
    h.set_signal(&sig, String::from("Changed"));
    h.run_frame();

    // Should not panic — should go through MEASURE path
    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    assert!(h.find(vstack_id).unwrap().screen_bounds.width > 0.0);
}

#[test]
fn root_resize_keeps_taffy_node_ids() {
    let mut h = TestHarness::new(800.0, 600.0);
    let vstack_id = h.mount(VStack::new().push(Text::new("Content")));
    h.run_frame();

    let root_node_before = h.taffy_node(h.root_id());
    let vstack_node_before = h.taffy_node(vstack_id);

    // Resize window (REPOSITION only)
    h.resize(640.0, 480.0);
    h.run_frame();

    assert_eq!(
        h.taffy_node(h.root_id()),
        root_node_before,
        "Root NodeId unchanged after resize"
    );
    assert_eq!(
        h.taffy_node(vstack_id),
        vstack_node_before,
        "VStack NodeId unchanged after resize"
    );
}

#[test]
fn conditional_then_text_binding_in_subsequent_frames() {
    // Structural change first, then MEASURE change — ensure both paths work
    let mut h = TestHarness::new(800.0, 600.0);
    let condition = Signal::new(true);
    let text_sig = Signal::new(String::from("Old"));

    h.mount(VStack::new().push(Conditional::new(
        condition.clone(),
        Text::new(String::new()).bind(text_sig.clone()),
        Text::new("Static"),
    )));
    h.run_frame();

    // Step 1: Toggle conditional (structural)
    h.set_signal(&condition, false);
    h.run_frame();

    // Step 2: Change text (MEASURE) — should not panic
    h.set_signal(&text_sig, String::from("New text content"));
    h.run_frame();

    // Both should have valid bounds
    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let cond_id = h.find(vstack_id).unwrap().children[0];
    assert!(
        h.find(cond_id).unwrap().screen_bounds.width > 0.0,
        "Conditional bounds valid after toggle + text change"
    );
}

#[test]
fn deeply_nested_structural_change() {
    // Ensure structural change works for deeply nested Conditionals
    let mut h = TestHarness::new(800.0, 600.0);
    let inner = Signal::new(true);
    let outer = Signal::new(true);

    h.mount(VStack::new().push(Conditional::new(
        outer.clone(),
        Conditional::new(
            inner.clone(),
            Text::new("Inner-True"),
            Text::new("Inner-False"),
        ),
        Text::new("Outer-False"),
    )));
    h.run_frame();

    // Toggle inner only (should only rebuild inner subtree)
    h.set_signal(&inner, false);
    h.run_frame();

    // Toggle outer (should rebuild entire Conditional subtree including inner)
    h.set_signal(&outer, false);
    h.run_frame();

    // Both toggles should work without panic
    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    assert!(h.find(vstack_id).unwrap().screen_bounds.width > 0.0);
}

// ═══════════════════════ F. Widget Position & Size Stability ═══════════════

#[test]
fn textinput_mirrors_to_text_width_updates() {
    let mut h = TestHarness::new(800.0, 600.0);
    let shared = Signal::new(String::from("Hi"));

    h.mount(
        VStack::new()
            .push(TextInput::new(shared.clone()).placeholder("Input"))
            .push(Text::new(String::new()).bind(shared.clone())),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let text_eid = h.find(vstack_id).unwrap().children[1];
    let w_initial = bounds_of(&h, text_eid).width;

    h.set_signal(&shared, String::from("Much longer mirrored text!"));
    h.run_frame();
    h.run_frame();

    let w_after = bounds_of(&h, text_eid).width;
    assert!(
        w_after > w_initial + 10.0,
        "Mirrored Text should grow: {} -> {}",
        w_initial,
        w_after
    );
}

#[test]
fn textinput_and_text_do_not_overlap() {
    let mut h = TestHarness::new(800.0, 600.0);
    let shared = Signal::new(String::from("example"));
    h.mount(
        VStack::new()
            .push(TextInput::new(shared.clone()))
            .push(Text::new(String::new()).bind(shared.clone())),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let (_, y0, _, h0) = pos_size(&h, h.find(vstack_id).unwrap().children[0]);
    let (_, y1, _, _) = pos_size(&h, h.find(vstack_id).unwrap().children[1]);

    assert!(
        y0 + h0 <= y1 + 2.0,
        "Input bottom={} should be <= Text top={}",
        y0 + h0,
        y1
    );
}

#[test]
fn button_size_stable_across_content_changes() {
    let mut h = TestHarness::new(800.0, 600.0);
    let label = Signal::new(String::from("OK"));
    h.mount(Button::new("Dynamic").bind(label.clone()));
    h.run_frame();

    let btn_id = h.find(h.root_id()).unwrap().children[0];
    let (_, _, _w0, h0) = pos_size(&h, btn_id);

    h.set_signal(&label, String::from("Very Long Button Text Here"));
    h.run_frame();
    h.run_frame();

    let (_, _, w1, h1) = pos_size(&h, btn_id);
    assert!(
        (h1 - h0).abs() < 4.0,
        "Button height stable: {} -> {}",
        h0,
        h1
    );
    assert!(
        w1 >= _w0 - 2.0,
        "Button width not shrink: {} -> {}",
        _w0,
        w1
    );
}

#[test]
fn textinput_preserves_minimum_width() {
    let mut h = TestHarness::new(800.0, 600.0);
    let val = Signal::new(String::new());
    h.mount(TextInput::new(val.clone()).placeholder("Email"));
    h.run_frame();

    let input_id = h.find(h.root_id()).unwrap().children[0];
    let w = bounds_of(&h, input_id).width;
    assert!(w >= 200.0, "TextInput min width ~200px, got {}", w);
}

#[test]
fn textinput_width_does_not_grow_with_content() {
    // TextInput should keep its set width regardless of content
    let mut h = TestHarness::new(800.0, 600.0);
    let val = Signal::new(String::from("short"));
    h.mount(TextInput::new(val.clone()));
    h.run_frame();

    let input_id = h.find(h.root_id()).unwrap().children[0];
    let w_initial = bounds_of(&h, input_id).width;

    // Set very long content
    h.set_signal(
        &val,
        String::from("This is a very very very long text that should not make the input wider"),
    );
    h.run_frame();
    h.run_frame();
    let w_long = bounds_of(&h, input_id).width;

    assert!(
        (w_long - w_initial).abs() < 2.0,
        "TextInput width must stay fixed: {} -> {}",
        w_initial,
        w_long
    );
}

#[test]
fn conditional_toggle_10_times_with_input() {
    let mut h = TestHarness::new(800.0, 600.0);
    let cond = Signal::new(true);
    let val = Signal::new(String::from("data"));

    h.mount(
        VStack::new()
            .push(Text::new("Header"))
            .push(Conditional::new(
                cond.clone(),
                VStack::new()
                    .push(TextInput::new(val.clone()))
                    .push(Text::new("Footer inside")),
                Text::new("Collapsed"),
            )),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    for i in 0..10 {
        h.set_signal(&cond, i % 2 == 0);
        h.run_frame();

        let parent = h.find(vstack_id).unwrap();
        let mut prev_bottom = 0.0f32;
        for &cid in &parent.children {
            let child = h.find(cid).unwrap();
            if child.is_visible() && !child.slot_inactive.get() {
                let b = child.screen_bounds;
                assert!(
                    b.width > 0.0 && b.height > 0.0,
                    "Round {}: zero-size visible element {:?}",
                    i,
                    cid
                );
                assert!(
                    b.y >= prev_bottom - 1.0,
                    "Round {}: y={} prev_bottom={}",
                    i,
                    b.y,
                    prev_bottom
                );
                prev_bottom = b.y + b.height;
            }
        }
    }
}

#[test]
fn nested_conditional_mixed_widgets() {
    let mut h = TestHarness::new(800.0, 600.0);
    let outer = Signal::new(true);
    let inner = Signal::new(true);
    let text_val = Signal::new(String::from("initial"));

    h.mount(
        VStack::new()
            .push(Text::new("Top"))
            .push(Conditional::new(
                outer.clone(),
                Conditional::new(
                    inner.clone(),
                    VStack::new().push(Text::new("Inner Header")).push(
                        HStack::new()
                            .push(Button::new("A").primary())
                            .push(TextInput::new(text_val.clone()))
                            .push(Text::new(String::new()).bind(text_val.clone())),
                    ),
                    Text::new("Inner hidden"),
                ),
                Text::new("Outer hidden"),
            ))
            .push(Text::new("Bottom")),
    );
    h.run_frame();

    for &(outer_val, inner_val) in &[
        (true, true),
        (true, false),
        (false, true),
        (false, false),
        (true, true),
    ] {
        h.set_signal(&outer, outer_val);
        h.set_signal(&inner, inner_val);
        h.run_frame();

        let vstack_id = h.find(h.root_id()).unwrap().children[0];
        let parent = h.find(vstack_id).unwrap();
        let mut prev_bottom = 0.0f32;
        for &cid in &parent.children {
            let child = h.find(cid).unwrap();
            if child.is_visible() && !child.slot_inactive.get() {
                let b = child.screen_bounds;
                assert!(
                    b.width > 0.0 && b.height > 0.0,
                    "Outer={} inner={}: zero-size visible {:?}",
                    outer_val,
                    inner_val,
                    cid
                );
                assert!(
                    b.y >= prev_bottom - 1.0,
                    "Outer={} inner={}: y={} prev_bottom={}",
                    outer_val,
                    inner_val,
                    b.y,
                    prev_bottom
                );
                prev_bottom = b.y + b.height;
            }
        }
    }
}

#[test]
fn rapid_toggle_no_frame_skip() {
    let mut h = TestHarness::new(800.0, 600.0);
    let cond = Signal::new(true);
    let text_val = Signal::new(String::from("visible text"));

    h.mount(
        VStack::new()
            .push(Text::new("Static header"))
            .push(Conditional::new(
                cond.clone(),
                VStack::new()
                    .push(Text::new(String::new()).bind(text_val.clone()))
                    .push(Button::new("Action").primary()),
                Text::new("Nothing to show"),
            ))
            .push(Text::new("Static footer")),
    );
    h.run_frame();

    for (i, &state) in [true, false, true, false, true].iter().enumerate() {
        h.set_signal(&cond, state);
        h.run_frame();

        let vstack_id = h.find(h.root_id()).unwrap().children[0];
        let header = h.find(vstack_id).unwrap().children[0];
        let footer = h.find(vstack_id).unwrap().children[2];
        let hdr_bottom = bounds_of(&h, header).y + bounds_of(&h, header).height;
        let ftr_y = bounds_of(&h, footer).y;
        assert!(
            ftr_y >= hdr_bottom - 1.0,
            "Round {} state={}: footer y={} headers_bottom={}",
            i,
            state,
            ftr_y,
            hdr_bottom
        );
    }
}

#[test]
fn text_mirrored_from_input_stays_at_correct_y() {
    let mut h = TestHarness::new(800.0, 600.0);
    let shared = Signal::new(String::from("A"));
    h.mount(
        VStack::new()
            .push(TextInput::new(shared.clone()))
            .push(Text::new(String::new()).bind(shared.clone()))
            .push(Button::new("Submit").primary()),
    );
    h.run_frame();

    h.set_signal(&shared, String::from("Much much much longer text here now"));
    h.run_frame();
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let (_, ny0, _, nh0) = pos_size(&h, h.find(vstack_id).unwrap().children[0]);
    let (_, ny1, _, nh1) = pos_size(&h, h.find(vstack_id).unwrap().children[1]);
    let (_, ny2, _, _) = pos_size(&h, h.find(vstack_id).unwrap().children[2]);

    assert!(ny0 + nh0 <= ny1 + 2.0, "Input bottom <= Text top");
    assert!(ny1 + nh1 <= ny2 + 2.0, "Text bottom <= Button top");
}

#[test]
fn hstack_with_variable_width_content_no_overlap() {
    let mut h = TestHarness::new(800.0, 600.0);
    let label_text = Signal::new(String::from("Short"));
    h.mount(
        HStack::new()
            .push(Text::new(String::new()).bind(label_text.clone()))
            .push(TextInput::new(Signal::new(String::from("input")))),
    );
    h.run_frame();

    let hstack_id = h.find(h.root_id()).unwrap().children[0];
    let (x0, _, w0, _) = pos_size(&h, h.find(hstack_id).unwrap().children[0]);
    let (x1, _, _, _) = pos_size(&h, h.find(hstack_id).unwrap().children[1]);
    assert!(x0 + w0 <= x1 + 2.0, "Label right <= Input left");

    h.set_signal(&label_text, String::from("Very very very long label text"));
    h.run_frame();
    h.run_frame();

    let (nx0, _, nw0, _) = pos_size(&h, h.find(hstack_id).unwrap().children[0]);
    let (nx1, _, _, _) = pos_size(&h, h.find(hstack_id).unwrap().children[1]);
    assert!(
        nx0 + nw0 <= nx1 + 2.0,
        "After grow: Label right <= Input left"
    );
    assert!(nw0 > w0, "Label width should grow: {} -> {}", w0, nw0);
}

#[test]
fn textinput_in_hstack_maintains_x_position() {
    let mut h = TestHarness::new(800.0, 600.0);
    let input_val = Signal::new(String::from("edit me"));
    h.mount(
        HStack::new()
            .push(Text::new("Name:"))
            .push(TextInput::new(input_val.clone())),
    );
    h.run_frame();

    let hstack_id = h.find(h.root_id()).unwrap().children[0];
    let (x_input, _, _, _) = pos_size(&h, h.find(hstack_id).unwrap().children[1]);
    assert!(
        x_input > 10.0,
        "TextInput starts after label: x={}",
        x_input
    );

    for text in &["long", "very long content here now", "s", "medium text"] {
        h.set_signal(&input_val, String::from(*text));
        h.run_frame();
        h.run_frame();

        let (nx, _, _, _) = pos_size(&h, h.find(hstack_id).unwrap().children[1]);
        assert!(
            (nx - x_input).abs() < 5.0,
            "TextInput x stable for '{}': {} -> {}",
            text,
            x_input,
            nx
        );
    }
}

#[test]
fn vstack_gap_maintained_across_signal_changes() {
    let mut h = TestHarness::new(800.0, 600.0);
    let shared = Signal::new(String::from("First"));
    h.mount(
        VStack::new()
            .gap(8.0)
            .push(Text::new(String::new()).bind(shared.clone()))
            .push(Button::new("Second").primary()),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let (_, y0, _, h0) = pos_size(&h, h.find(vstack_id).unwrap().children[0]);
    let (_, y1, _, _) = pos_size(&h, h.find(vstack_id).unwrap().children[1]);
    let gap0 = y1 - (y0 + h0);

    h.set_signal(
        &shared,
        String::from("Much much much longer first text now"),
    );
    h.run_frame();
    h.run_frame();

    let (_, ny0, _, nh0) = pos_size(&h, h.find(vstack_id).unwrap().children[0]);
    let (_, ny1, _, _) = pos_size(&h, h.find(vstack_id).unwrap().children[1]);
    let gap1 = ny1 - (ny0 + nh0);

    assert!(
        (gap1 - gap0).abs() < 4.0,
        "VStack gap ~8px regardless of content: {} -> {}",
        gap0,
        gap1
    );
}

fn pos_size(h: &TestHarness, id: ElementId) -> (f32, f32, f32, f32) {
    let b = bounds_of(h, id);
    (b.x, b.y, b.width, b.height)
}

// ═══════════════════════ G. Leaf ↔ Container Transitions ═══════════════════
// When Conditional::when toggles, the container changes from flex (with 1
// child) to leaf (0 children) or vice versa. This exercises ensure_subtree's
// type-switch path: set_children with empty vec, style rebuilt as leaf.

#[test]
fn conditional_when_container_to_leaf_transition() {
    // cond=true: 1 child → container. cond=false: 0 children → leaf.
    let mut h = TestHarness::new(800.0, 600.0);
    let cond = Signal::new(true);

    h.mount(
        VStack::new()
            .push(Text::new("Header"))
            .push(Conditional::when(
                cond.clone(),
                Text::new("Shown when true"),
            ))
            .push(Text::new("Footer")),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let child_ids = h.find(vstack_id).unwrap().children.clone();

    // Toggle off: container → leaf
    h.set_signal(&cond, false);
    h.run_frame();

    let header = h.find(child_ids[0]).unwrap();
    let footer = h.find(child_ids[2]).unwrap();
    assert!(
        header.screen_bounds.height > 0.0,
        "Header should be visible"
    );
    assert!(
        footer.screen_bounds.height > 0.0,
        "Footer should be visible"
    );

    let cond_id = child_ids[1];
    let cond_el = h.find(cond_id).unwrap();
    // When Conditional has no active child, its bounds should collapse to ~0 height
    assert!(
        cond_el.screen_bounds.height < 50.0,
        "Conditional with no active child should collapse: h={}",
        cond_el.screen_bounds.height
    );
}

#[test]
fn conditional_when_leaf_to_container_transition() {
    let mut h = TestHarness::new(800.0, 600.0);
    let cond = Signal::new(false);

    h.mount(
        VStack::new()
            .push(Text::new("Header"))
            .push(Conditional::when(
                cond.clone(),
                VStack::new()
                    .push(Text::new("Appears"))
                    .push(Text::new("When visible")),
            ))
            .push(Text::new("Footer")),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let children = h.find(vstack_id).unwrap().children.clone();
    let cond_id = children[1];
    let header_bottom_before = {
        let hdr = h.find(children[0]).unwrap();
        hdr.screen_bounds.y + hdr.screen_bounds.height
    };

    // Toggle on: leaf → container
    h.set_signal(&cond, true);
    h.run_frame();

    let cond_el = h.find(cond_id).unwrap();
    assert!(
        cond_el.screen_bounds.height > 20.0,
        "Conditional with active child should have height, got {}",
        cond_el.screen_bounds.height
    );

    let footer_y = h.find(children[2]).unwrap().screen_bounds.y;
    assert!(
        footer_y > header_bottom_before + 10.0,
        "Footer should move down: footer_y={} header_bottom={}",
        footer_y,
        header_bottom_before
    );
}

#[test]
fn conditional_when_toggle_10_times_leaf_container_cycle() {
    // Stress-test: toggle 10 times between leaf (0 children) and container (1 child)
    let mut h = TestHarness::new(800.0, 600.0);
    let cond = Signal::new(true);

    h.mount(
        VStack::new()
            .push(Text::new("Top"))
            .push(Conditional::when(
                cond.clone(),
                VStack::new()
                    .push(Text::new("Line 1"))
                    .push(Text::new("Line 2")),
            ))
            .push(Text::new("Bottom")),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];

    for i in 0..10 {
        let state = i % 2 == 0;
        h.set_signal(&cond, state);
        h.run_frame();

        let parent = h.find(vstack_id).unwrap();
        let mut prev_bottom = 0.0f32;
        for &cid in &parent.children {
            let child = h.find(cid).unwrap();
            if child.is_visible() && !child.slot_inactive.get() {
                let b = child.screen_bounds;
                assert!(
                    b.width > 0.0 && b.height > 0.0,
                    "Round {} state={}: zero-size visible {:?}",
                    i,
                    state,
                    cid
                );
                assert!(
                    b.y >= prev_bottom - 1.0,
                    "Round {} state={}: overlap y={} prev_bottom={}",
                    i,
                    state,
                    b.y,
                    prev_bottom
                );
                prev_bottom = b.y + b.height;
            }
        }
    }
}

#[test]
fn nested_conditional_opposite_transitions() {
    let mut h = TestHarness::new(800.0, 600.0);
    let outer = Signal::new(true);
    let inner = Signal::new(false);

    h.mount(
        VStack::new()
            .push(Text::new("Top"))
            .push(Conditional::when(
                outer.clone(),
                VStack::new()
                    .push(Text::new("Outer visible"))
                    .push(Conditional::when(inner.clone(), Text::new("Inner visible"))),
            ))
            .push(Text::new("Bottom")),
    );
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let child_ids = h.find(vstack_id).unwrap().children.clone();

    // Toggle outer off AND inner on simultaneously
    h.set_signal(&outer, false);
    h.set_signal(&inner, true);
    h.run_frame();

    let bottom = h.find(child_ids[2]).unwrap();
    assert!(
        bottom.screen_bounds.y >= 0.0 && bottom.screen_bounds.height > 0.0,
        "Bottom should be visible after opposite transitions"
    );

    // Toggle everything back
    h.set_signal(&outer, true);
    h.set_signal(&inner, false);
    h.run_frame();

    for &cid in &child_ids {
        let child = h.find(cid).unwrap();
        if child.is_visible() && !child.slot_inactive.get() {
            assert!(
                child.screen_bounds.width > 0.0,
                "Visible {:?} has zero width",
                cid
            );
        }
    }
}

#[test]
fn conditional_new_both_branches_toggle_preserves_single_active() {
    // Conditional::new always has 2 children, exactly 1 active.
    // Verify that toggling doesn't make both active or both inactive.
    let mut h = TestHarness::new(800.0, 600.0);
    let cond = Signal::new(true);

    h.mount(VStack::new().push(Conditional::new(
        cond.clone(),
        Text::new("Branch A: very long text here"),
        Text::new("Branch B: also long text here"),
    )));
    h.run_frame();

    let vstack_id = h.find(h.root_id()).unwrap().children[0];
    let cond_id = h.find(vstack_id).unwrap().children[0];

    for i in 0..6 {
        h.set_signal(&cond, i % 2 == 0);
        h.run_frame();

        let cond_el = h.find(cond_id).unwrap();
        assert_eq!(cond_el.children.len(), 2, "Always 2 children");

        let slot_a = h.find(cond_el.children[0]).unwrap().slot_inactive.get();
        let slot_b = h.find(cond_el.children[1]).unwrap().slot_inactive.get();
        assert_ne!(
            slot_a, slot_b,
            "Round {}: Exactly one child should be active (slot_a={} slot_b={})",
            i, slot_a, slot_b
        );
    }
}

#[test]
fn leaf_container_transition_keeps_taffy_node_id() {
    // When Conditional.when goes leaf→container or back, NodeId should persist
    let mut h = TestHarness::new(800.0, 600.0);
    let cond = Signal::new(true);

    h.mount(Conditional::when(cond.clone(), Text::new("Content")));
    h.run_frame();

    let cond_id = h.find(h.root_id()).unwrap().children[0];
    let node_before = h.taffy_node(cond_id).unwrap();

    // Toggle off: container → leaf
    h.set_signal(&cond, false);
    h.run_frame();
    let node_mid = h.taffy_node(cond_id).unwrap();
    assert_eq!(node_before, node_mid, "NodeId unchanged container→leaf");

    // Toggle on: leaf → container
    h.set_signal(&cond, true);
    h.run_frame();
    let node_after = h.taffy_node(cond_id).unwrap();
    assert_eq!(node_before, node_after, "NodeId unchanged leaf→container");
}
