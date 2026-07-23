//! DevTools Data Probe — verifies the full data pipeline:
//! FrameSnapshot collection, signal changes, dirty events with triggers,
//! per-phase timing, element diffs, serialization, interactions, freeze.

#![cfg(feature = "devtools")]

use auralis_signal::Signal;
use burin::debug::devtools;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::*;

fn harness_with_devtools() -> TestHarness {
    // Install ring buffer BEFORE creating TestHarness (it reads the global in new())
    let buf = devtools::new_ring_buffer();
    devtools::install_ring_buffer(buf);
    burin::core::perf::perf_enable();
    burin::core::dirty_registry::set_dirty_trace_enabled(true);
    devtools::install_signal_observer();
    let _ = devtools::drain_test_snapshots(); // clear any prior state

    let h = TestHarness::new(400.0, 300.0);
    h
}

fn snapshots() -> Vec<devtools::FrameSnapshot> {
    let snaps = devtools::drain_test_snapshots();
    // Also try the ring buffer
    if snaps.is_empty() {
        if let Some(inspector) = devtools::DevtoolsInspector::attach() {
            use std::cell::RefCell;
            use std::rc::Rc;
            let buf = Rc::new(RefCell::new(Vec::new()));
            inspector.with_buffer(|map| {
                for snaps in map.values() {
                    for snap in snaps {
                        buf.borrow_mut().push(snap.clone());
                    }
                }
            });
            return Rc::try_unwrap(buf).unwrap().into_inner();
        }
    }
    snaps
}

#[test]
fn probe_smoke_frame_snapshot_collected() {
    let mut h = harness_with_devtools();
    let label = Signal::new("0".to_string());
    let l = label.clone();

    h.mount(VStack::new().push(Text::new(label.read()).font_size(14.0).bind(label.clone())));
    h.run_frame();

    for i in 0..5u64 {
        l.set(i.to_string());
        h.run_frame();
    }

    let snaps = snapshots();
    eprintln!("[PROBE] total snapshots: {}", snaps.len());
    assert!(!snaps.is_empty(), "should have frame snapshots");

    let snap = &snaps[snaps.len() / 2];
    eprintln!(
        "[PROBE] mid-frame: frame={} elements={} dirty={} elem_changes={} sig_changes={} dirty_events={} links={}",
        snap.frame_id, snap.element_count, snap.dirty_count,
        snap.element_changes.len(), snap.signal_changes.len(),
        snap.dirty_events.len(), snap.signal_element_links.len(),
    );
    assert!(snap.element_count > 0);
    assert!(snap.dirty_count > 0, "signal change should produce dirty");
}

#[test]
fn probe_per_phase_timing() {
    let mut h = harness_with_devtools();
    let label = Signal::new("0".to_string());
    let l = label.clone();

    h.mount(Text::new("0").font_size(14.0).bind(label.clone()));
    h.run_frame();

    for i in 0..10u64 {
        l.set(i.to_string());
        h.run_frame();
    }

    let snaps = snapshots();
    assert!(!snaps.is_empty(), "should have snapshots");

    let last = snaps.last().unwrap();
    let phases = &last.frame_timing.per_phase_us;
    eprintln!(
        "[PROBE:PHASES] pp={} dirty={} actions={} layout={} anim={} recheck={} paint={}",
        phases[1], phases[2], phases[3], phases[4], phases[5], phases[6], phases[7],
    );
    assert_eq!(phases.len(), 9);
    assert!(
        phases.iter().any(|&t| t > 0),
        "at least one phase should have data"
    );
}

#[test]
fn probe_dirty_event_triggers() {
    let mut h = harness_with_devtools();
    let label = Signal::new("hello".to_string());
    let l = label.clone();

    h.mount(Text::new(label.read()).font_size(14.0).bind(label.clone()));
    h.run_frame();

    l.set("world".to_string());
    h.run_frame();

    let snaps = snapshots();
    let last = snaps.last().unwrap();

    eprintln!(
        "[PROBE:DIRTY] dirty_events={} signal_element_links={}",
        last.dirty_events.len(),
        last.signal_element_links.len()
    );
    for de in &last.dirty_events {
        eprintln!(
            "[PROBE:DIRTY_EVENT] eid={:?} type={:?} trigger={:?}",
            de.element_id, de.element_type, de.trigger
        );
    }

    assert!(
        last.signal_element_links.len() > 0,
        "should have signal→element causal links"
    );
}

#[test]
fn probe_element_diff_works() {
    let mut h = harness_with_devtools();
    let label = Signal::new("0".to_string());
    let l = label.clone();

    h.mount(Text::new("0").font_size(14.0).bind(label.clone()));
    h.run_frame();
    h.run_frame();

    l.set("1".to_string());
    h.run_frame();

    let snaps = snapshots();
    assert!(
        snaps.len() >= 3,
        "should have at least 3 snapshots, got {}",
        snaps.len()
    );

    let diff = devtools::diff_snapshots(&snaps[snaps.len() - 2], &snaps[snaps.len() - 1]);
    eprintln!(
        "[PROBE:DIFF] {}→{} elem_changes={} sig_changes={}",
        diff.frame_id_prev,
        diff.frame_id_current,
        diff.element_changes.len(),
        diff.signal_changes.len()
    );

    for ec in &diff.element_changes {
        if let devtools::ElementChange::Modified {
            id,
            kind,
            what_changed,
        } = ec
        {
            eprintln!(
                "[PROBE:DIFF:MOD] {:?} ({}) changed: {:?}",
                id, kind, what_changed
            );
        }
    }
}

#[test]
fn probe_serialization_roundtrip() {
    let mut h = harness_with_devtools();
    let label = Signal::new("test".to_string());
    let l = label.clone();

    h.mount(Text::new("test").font_size(14.0).bind(label.clone()));
    h.run_frame();
    l.set("99".to_string());
    h.run_frame();

    let snaps = snapshots();
    let snap = snaps.last().unwrap();

    let json = serde_json::to_string_pretty(snap).expect("serialize");
    assert!(!json.is_empty());
    eprintln!(
        "[PROBE:SERDE] JSON size: {} bytes, elements={}",
        json.len(),
        snap.element_count
    );

    let deserialized: devtools::FrameSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.frame_id, snap.frame_id);
    assert_eq!(deserialized.element_count, snap.element_count);
}

#[test]
fn probe_interaction_recording() {
    let mut h = harness_with_devtools();
    let counter = Signal::new(0u64);
    let c = counter.clone();
    let label = Signal::new("0".to_string());
    let l = label.clone();

    let widget_id = h.mount(
        VStack::new()
            .push(Button::new("Btn").on_click(move || {
                c.set(c.read() + 1);
                l.set(format!("{}", c.read()));
            }))
            .push(Text::new(label.read()).font_size(14.0).bind(label.clone())),
    );
    h.run_frame();

    h.click(widget_id);
    h.run_frame();
    h.click(widget_id);
    h.run_frame();

    let count = devtools::peek_interaction_count();
    eprintln!("[PROBE:INTERACTIONS] peek count: {}", count);
    assert!(count > 0, "should have recorded click interactions");

    let interactions = devtools::drain_interactions();
    eprintln!("[PROBE:INTERACTIONS] drained: {}", interactions.len());
    for ix in &interactions {
        eprintln!(
            "[PROBE:INTERACTION] seq={} frame={} kind={:?}",
            ix.seq, ix.frame_id, ix.kind
        );
    }
}

#[test]
fn probe_freeze_unfreeze_queues_dirty() {
    let mut h = harness_with_devtools();
    let label = Signal::new("0".to_string());
    let l = label.clone();

    h.mount(Text::new("0").font_size(14.0).bind(label.clone()));
    h.run_frame();

    burin::core::dirty_registry::freeze_ui();
    l.set("1".to_string());
    l.set("2".to_string());
    h.run_frame();

    burin::core::dirty_registry::unfreeze_ui();
    h.run_frame();

    let snaps = snapshots();
    eprintln!(
        "[PROBE:FREEZE] total snapshots after cycle: {}",
        snaps.len()
    );
    assert!(
        snaps.len() >= 2,
        "should have snapshots before and after freeze"
    );
}

#[test]
fn probe_subtree_ids() {
    let mut h = harness_with_devtools();
    h.mount(
        VStack::new()
            .push(Text::new("a").font_size(14.0))
            .push(Text::new("b").font_size(14.0)),
    );
    h.run_frame();

    let root = h.root_id();
    let subtree = burin::core::dirty_registry::subtree_ids(&h.arena, root);
    eprintln!(
        "[PROBE:SUBTREE] root={:?} subtree_len={}",
        root,
        subtree.len()
    );
    assert!(
        subtree.len() >= 3,
        "VStack + 2 Text = at least 3 elements in subtree"
    );

    let children = burin::core::dirty_registry::children_of(&h.arena, root);
    eprintln!("[PROBE:CHILDREN] root children: {:?}", children);
    assert!(!children.is_empty(), "root should have children");
}
