//! Contract tests for Checkbox, Switch, and RadioButton/RadioGroup.
//!
//! Each test verifies the widget's behaviour against its specification,
//! not against its current implementation.  Failing tests reveal real
//! gaps between the contract and the implementation.

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::style::resolve_style;
use burin::testing::TestHarness;
use burin::widgets::input::{Checkbox, RadioGroup, Switch};

// ═══════════════════════════════════════════════════════════════════════
//  Checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn checkbox_mounts_with_label_and_role() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Checkbox::new(Signal::new(false)));
    h.run_frame();

    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::CheckBox);

    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(bounds.width > 0.0, "checkbox should have non-zero width");
    assert!(bounds.height > 0.0, "checkbox should have non-zero height");
}

#[test]
fn checkbox_default_unchecked() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Checkbox::new(Signal::new(false)));
    h.run_frame();

    h.assert_state(id, StateFlags::CHECKED, false);
}

#[test]
fn checkbox_checked_sets_state() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Checkbox::new(Signal::new(true)));
    h.run_frame();

    h.assert_state(id, StateFlags::CHECKED, true);
}

#[test]
fn checkbox_click_toggles_checked() {
    let checked = Signal::new(false);
    let clicked = Signal::new(false);
    let c = clicked.clone();

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Checkbox::new(checked.clone()).on_value_changed(move |_| {
        c.set(true);
    }));
    h.run_frame();

    assert!(!checked.read(), "should start unchecked");
    assert!(
        !clicked.read(),
        "callback should not fire before interaction"
    );

    h.activate_button(id);

    assert!(
        checked.read(),
        "checked signal should toggle to true after click"
    );
    assert!(clicked.read(), "on_value_changed should fire after click");
    h.assert_state(id, StateFlags::CHECKED, true);
}

#[test]
fn checkbox_disabled_blocks_click() {
    let clicked = Signal::new(false);
    let c = clicked.clone();

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(
        Checkbox::new(Signal::new(false))
            .disabled()
            .on_value_changed(move |_| {
                c.set(true);
            }),
    );
    h.run_frame();

    h.activate_button(id);

    assert!(
        !clicked.read(),
        "disabled checkbox should not fire on_value_changed"
    );
}

#[test]
fn checkbox_disabled_sets_flag() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Checkbox::new(Signal::new(false)).disabled());
    h.run_frame();

    h.assert_state(id, StateFlags::DISABLED, true);
}

#[test]
fn checkbox_hovered_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Checkbox::new(Signal::new(false)));
    h.run_frame();

    let style = h
        .style_component_of(id)
        .expect("checkbox should have a style component");
    let default = resolve_style(StateFlags::NONE, &style);
    let hovered = resolve_style(StateFlags::HOVERED, &style);

    assert_ne!(
        hovered.background, default.background,
        "hovered state should visually differ from default"
    );
}

#[test]
fn checkbox_checked_has_different_visual() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Checkbox::new(Signal::new(false)));
    h.run_frame();

    let style = h
        .style_component_of(id)
        .expect("checkbox should have a style component");
    let default = resolve_style(StateFlags::NONE, &style);
    let checked = resolve_style(StateFlags::CHECKED, &style);

    assert_ne!(
        checked.background, default.background,
        "checked state should have different background from default"
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Switch
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn switch_mounts_with_correct_role() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(false)));
    h.run_frame();

    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::Switch);

    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(bounds.width > 0.0, "switch should have non-zero width");
    assert!(bounds.height > 0.0, "switch should have non-zero height");
}

#[test]
fn switch_default_unchecked() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(false)));
    h.run_frame();

    h.assert_state(id, StateFlags::CHECKED, false);
}

#[test]
fn switch_checked_sets_state() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(true)));
    h.run_frame();

    h.assert_state(id, StateFlags::CHECKED, true);
}

#[test]
fn switch_toggle_fires_on_click() {
    let toggled = Signal::new(false);
    let t = toggled.clone();

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(false)).on_value_changed(move |_| {
        t.set(true);
    }));
    h.run_frame();

    assert!(
        !toggled.read(),
        "callback should not fire before interaction"
    );

    h.activate_button(id);

    assert!(toggled.read(), "on_value_changed should fire after toggle");
}

#[test]
fn switch_disabled_blocks_interaction() {
    let toggled = Signal::new(false);
    let t = toggled.clone();

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(
        Switch::new(Signal::new(false))
            .disabled()
            .on_value_changed(move |_| {
                t.set(true);
            }),
    );
    h.run_frame();

    h.activate_button(id);

    assert!(
        !toggled.read(),
        "disabled switch should not fire on_value_changed"
    );
}

#[test]
fn switch_hovered_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Switch::new(Signal::new(false)));
    h.run_frame();

    let style = h
        .style_component_of(id)
        .expect("switch should have a style component");
    let default = resolve_style(StateFlags::NONE, &style);
    let hovered = resolve_style(StateFlags::HOVERED, &style);

    assert_ne!(
        hovered.background, default.background,
        "hovered state should visually differ from default"
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  RadioButton / RadioGroup
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn radio_group_mounts_multiple_options() {
    let selected = Signal::new("a".to_string());
    let mut h = TestHarness::new(400.0, 200.0);
    let _id = h.mount(
        RadioGroup::new(selected)
            .option("Option A", "a".to_string())
            .option("Option B", "b".to_string())
            .option("Option C", "c".to_string()),
    );
    h.run_frame();

    let radios = h.get_all_by_role(accesskit::Role::RadioButton);
    assert_eq!(radios.len(), 3, "RadioGroup should contain 3 radio buttons");
}

#[test]
fn radio_only_one_checked_at_a_time() {
    let selected = Signal::new("a".to_string());
    let mut h = TestHarness::new(400.0, 200.0);
    let _id = h.mount(
        RadioGroup::new(selected.clone())
            .option("Option A", "a".to_string())
            .option("Option B", "b".to_string())
            .option("Option C", "c".to_string()),
    );
    h.run_frame();

    assert_eq!(selected.read(), "a", "initial selection should be 'a'");

    let radios = h.get_all_by_role(accesskit::Role::RadioButton);
    assert_eq!(radios.len(), 3);

    // Select "B"
    h.activate_button(radios[1]);
    assert_eq!(
        selected.read(),
        "b",
        "after clicking option B, selection should be 'b'"
    );

    // Select "C"
    h.activate_button(radios[2]);
    assert_eq!(
        selected.read(),
        "c",
        "after clicking option C, selection should be 'c'"
    );

    // "A" should no longer be selected
    assert_ne!(
        selected.read(),
        "a",
        "after selecting 'c', 'a' should not be selected"
    );
}

#[test]
fn radio_disabled_button_not_selectable() {
    let selected = Signal::new("a".to_string());
    let mut h = TestHarness::new(400.0, 200.0);
    let _id = h.mount(
        RadioGroup::new(selected.clone())
            .option("Option A", "a".to_string())
            .option("Option B", "b".to_string())
            .disabled_option("Option C (disabled)", "c".to_string()),
    );
    h.run_frame();

    assert_eq!(selected.read(), "a", "initial selection should be 'a'");

    let radios = h.get_all_by_role(accesskit::Role::RadioButton);
    assert_eq!(radios.len(), 3);

    // Try to select the disabled option (third in the group)
    h.activate_button(radios[2]);

    // Selection should not have changed because the button is disabled
    assert_eq!(
        selected.read(),
        "a",
        "disabled radio button should not be selectable"
    );
}
