//! Switch stress-tests — mount, toggle, disabled, multi, focus, a11y, signal.

use auralis_signal::Signal;
use burin::core::ElementId;
use burin::testing::TestHarness;
use burin::widgets::input::Switch;
use burin::widgets::layout::VStack;

fn mount_one(checked: bool, disabled: bool) -> (TestHarness, ElementId, Signal<bool>) {
    let mut h = TestHarness::new(400.0, 300.0);
    let sig = Signal::new(checked);
    let mut sw = Switch::new(sig.clone());
    if disabled {
        sw = sw.disabled();
    }
    let id = h.mount(sw);
    h.run_frame();
    (h, id, sig)
}

#[test]
fn switch_mounts_with_label_and_role() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(Switch::new(Signal::new(false)));
    h.run_frame();
    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::Switch);
    // Label is empty: the current Switch has no built-in label text.
    // Users compose a label externally.
    h.assert_a11y_label(id, "");
}

#[test]
fn switch_has_children() {
    let (h, id, _) = mount_one(false, false);
    let el = h.find(id).unwrap();
    // Switch mount_box returns the track element directly (not a row wrapper).
    // It contains a single child: the thumb knob.
    assert_eq!(el.children.len(), 1, "track → thumb");
    let thumb_id = el.children[0];
    let thumb = h.find(thumb_id).unwrap();
    assert_eq!(thumb.children.len(), 0, "thumb has no children");
    assert!(el.preferred_width().unwrap() > 0.0, "track has width");
}

#[test]
fn off_clicks_to_on() {
    let (mut h, id, sig) = mount_one(false, false);
    assert!(!h.read_signal(&sig));
    h.click(id).run_frame();
    assert!(h.read_signal(&sig));
}

#[test]
fn on_clicks_to_off() {
    let (mut h, id, sig) = mount_one(true, false);
    assert!(h.read_signal(&sig));
    h.click(id).run_frame();
    assert!(!h.read_signal(&sig));
}

#[test]
fn click_toggles_multiple_times() {
    let (mut h, id, sig) = mount_one(false, false);
    for expected in [true, false, true, false, true] {
        h.click(id).run_frame();
        assert_eq!(h.read_signal(&sig), expected);
    }
}

#[test]
fn disabled_does_not_toggle() {
    let sig = Signal::new(false);
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(sig.clone()).disabled());
    h.run_frame();
    h.click(id).run_frame();
    assert!(!h.read_signal(&sig));
}

#[test]
fn disabled_on_stays_on() {
    let sig = Signal::new(true);
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(sig.clone()).disabled());
    h.run_frame();
    h.click(id).run_frame();
    assert!(h.read_signal(&sig));
}

#[test]
fn two_switches_are_independent() {
    let mut h = TestHarness::new(600.0, 200.0);
    let a = Signal::new(false);
    let b = Signal::new(false);
    let id_a = h.mount(Switch::new(a.clone()));
    let id_b = h.mount(Switch::new(b.clone()));
    h.run_frame();

    h.click(id_a).run_frame();
    assert!(h.read_signal(&a));
    assert!(!h.read_signal(&b));

    h.click(id_b).run_frame();
    assert!(h.read_signal(&b));
}

#[test]
fn five_switches_are_independent() {
    let mut h = TestHarness::new(600.0, 400.0);
    let sigs: Vec<_> = (0..5).map(|_| Signal::new(false)).collect();
    let stack_id = h.mount(
        VStack::new()
            .push(Switch::new(sigs[0].clone()))
            .push(Switch::new(sigs[1].clone()))
            .push(Switch::new(sigs[2].clone()))
            .push(Switch::new(sigs[3].clone()))
            .push(Switch::new(sigs[4].clone())),
    );
    h.run_frame();
    let ids: Vec<ElementId> = h.find(stack_id).unwrap().children.clone();

    for &i in &[0usize, 2, 4] {
        h.click(ids[i]).run_frame();
    }
    for i in 0..5 {
        assert_eq!(h.read_signal(&sigs[i]), i % 2 == 0, "S{i}");
    }
}

#[test]
fn signal_set_to_true_updates() {
    let sig = Signal::new(false);
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(Switch::new(sig.clone()));
    h.run_frame();
    h.set_signal(&sig, true).run_frame();
    assert!(h.read_signal(&sig));
}

#[test]
fn signal_set_then_click() {
    let sig = Signal::new(false);
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(sig.clone()));
    h.run_frame();

    h.set_signal(&sig, true).run_frame();
    assert!(h.read_signal(&sig));

    h.click(id).run_frame();
    assert!(!h.read_signal(&sig));
}

#[test]
fn on_value_changed_fires() {
    use std::cell::Cell;
    use std::rc::Rc;

    let count = Rc::new(Cell::new(0u32));
    let sig = Signal::new(false);
    let c2 = count.clone();
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(sig.clone()).on_value_changed(move |_| {
        c2.set(c2.get() + 1);
    }));
    h.run_frame();
    h.click(id).run_frame();
    assert_eq!(count.get(), 1);
}

#[test]
fn on_value_changed_receives_correct_value() {
    use std::cell::Cell;
    use std::rc::Rc;

    let val = Rc::new(Cell::new(false));
    let sig = Signal::new(false);
    let v2 = val.clone();
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(sig.clone()).on_value_changed(move |v| {
        v2.set(v);
    }));
    h.run_frame();
    h.click(id).run_frame();
    assert!(val.get());
    h.click(id).run_frame();
    assert!(!val.get());
}

#[test]
fn focus_transfers_on_click() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(false)));
    h.run_frame();
    h.click(id).run_frame();
    h.assert_focused(id);
}

#[test]
fn disabled_a11y_state() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(false)).disabled());
    h.run_frame();
    h.assert_a11y_disabled(id);
}

#[test]
fn styled_switch_mounts() {
    use burin::style::styled::Styled;
    use burin::style::Color;
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(false)).background(Color::rgba8(200, 200, 200, 255)));
    h.run_frame();
    h.assert_visible(id);
}
