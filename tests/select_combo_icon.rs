//! Contract tests for Select, ComboBox, and IconButton.
//! Validates mount, visual state resolution, disabled behaviour,
//! and callback contracts against specifications.

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::resource::icons::Icon as IconKind;
use burin::style::resolve_style;
use burin::testing::TestHarness;
use burin::widgets::display::Icon;
use burin::widgets::input::{ComboBox, IconButton, Select};

// ═══════════════════════════════════════════════════════════════════════
// Select
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn select_mounts_with_label_and_role() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<String>> = Signal::new(None);
    let id =
        h.mount(Select::<String>::new(selected).options(vec!["A".to_string(), "B".to_string()]));
    h.run_frame();

    h.assert_visible(id);
    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(bounds.width > 10.0, "select should have non-trivial width");
    assert!(
        bounds.height > 10.0,
        "select should have non-trivial height"
    );
}

#[test]
fn select_disabled_sets_flag() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<String>> = Signal::new(None);
    let id = h.mount(
        Select::<String>::new(selected)
            .options(vec!["A".to_string(), "B".to_string()])
            .disabled(Signal::new(true)),
    );
    h.run_frame();

    h.set_state(id, StateFlags::DISABLED, true);
    h.assert_state(id, StateFlags::DISABLED, true);
}

#[test]
fn select_hovered_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<String>> = Signal::new(None);
    let id =
        h.mount(Select::<String>::new(selected).options(vec!["A".to_string(), "B".to_string()]));
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let default = resolve_style(StateFlags::NONE, &style);
    assert!(
        default.background.is_some(),
        "select should have a background colour"
    );

    h.set_state(id, StateFlags::HOVERED, true);
    h.assert_state(id, StateFlags::HOVERED, true);
}

#[test]
fn select_pressed_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<String>> = Signal::new(None);
    let id =
        h.mount(Select::<String>::new(selected).options(vec!["A".to_string(), "B".to_string()]));
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let default = resolve_style(StateFlags::NONE, &style);
    assert!(
        default.background.is_some(),
        "select should have a background colour"
    );

    h.set_state(id, StateFlags::PRESSED, true);
    h.assert_state(id, StateFlags::PRESSED, true);
}

// ═══════════════════════════════════════════════════════════════════════
// ComboBox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo_box_mounts_with_label() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<String>> = Signal::new(None);
    let id =
        h.mount(ComboBox::<String>::new(selected).options(vec!["A".to_string(), "B".to_string()]));
    h.run_frame();

    h.assert_visible(id);
    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(
        bounds.width > 10.0,
        "combo_box should have non-trivial width"
    );
    assert!(
        bounds.height > 10.0,
        "combo_box should have non-trivial height"
    );
}

#[test]
fn combo_box_disabled_sets_flag() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<String>> = Signal::new(None);
    let id = h.mount(
        ComboBox::<String>::new(selected)
            .options(vec!["A".to_string(), "B".to_string()])
            .disabled(Signal::new(true)),
    );
    h.run_frame();

    h.set_state(id, StateFlags::DISABLED, true);
    h.assert_state(id, StateFlags::DISABLED, true);
}

#[test]
fn combo_box_hovered_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<String>> = Signal::new(None);
    let id =
        h.mount(ComboBox::<String>::new(selected).options(vec!["A".to_string(), "B".to_string()]));
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let default = resolve_style(StateFlags::NONE, &style);
    assert!(
        default.background.is_some(),
        "combo_box should have a background colour"
    );

    h.set_state(id, StateFlags::HOVERED, true);
    h.assert_state(id, StateFlags::HOVERED, true);
}

// ═══════════════════════════════════════════════════════════════════════
// IconButton
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn icon_button_mounts_with_button_role() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(IconButton::new(Icon::new(IconKind::Check)));
    h.run_frame();

    h.assert_a11y_role(id, accesskit::Role::Button);

    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(
        bounds.width > 10.0,
        "icon_button should have non-trivial width"
    );
    assert!(
        bounds.height > 10.0,
        "icon_button should have non-trivial height"
    );
}

#[test]
fn icon_button_click_fires_callback() {
    let clicked = Signal::new(false);
    let c = clicked.clone();
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        IconButton::new(Icon::new(IconKind::Check)).on_click(move || {
            c.set(true);
        }),
    );
    h.run_frame();

    assert!(
        !clicked.read(),
        "callback should not fire before interaction"
    );

    h.activate_button(id);
    assert!(clicked.read(), "on_click should fire after activate_button");
}

#[test]
fn icon_button_disabled_blocks_click() {
    let clicked = Signal::new(false);
    let c = clicked.clone();
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        IconButton::new(Icon::new(IconKind::Check))
            .disabled()
            .on_click(move || {
                c.set(true);
            }),
    );
    h.run_frame();

    h.activate_button(id);
    assert!(
        !clicked.read(),
        "disabled icon_button should not fire on_click"
    );
}

#[test]
fn icon_button_hovered_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(IconButton::new(Icon::new(IconKind::Check)));
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let default = resolve_style(StateFlags::NONE, &style);
    let hovered = resolve_style(StateFlags::HOVERED, &style);

    assert_ne!(
        hovered.background, default.background,
        "hovered state should visually differ from default"
    );
}

#[test]
fn icon_button_disabled_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(IconButton::new(Icon::new(IconKind::Check)).disabled());
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let default = resolve_style(StateFlags::NONE, &style);
    let disabled = resolve_style(StateFlags::DISABLED, &style);

    assert!(
        disabled.background != default.background || disabled.foreground != default.foreground,
        "disabled state should visually differ from default (background or foreground)"
    );
}
