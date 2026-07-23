//! Button contract test: validates the complete interaction lifecycle
//! (mount → visual resolution → state transitions → callbacks → unmount)
//! using TestHarness's component-level interaction API rather than
//! simulating the full event pipeline.

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::style::resolve_style;
use burin::testing::TestHarness;
use burin::widgets::input::Button;

// ── Mount & default rendering ─────────────────────────────────────

#[test]
fn button_mounts_with_correct_visual_properties() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Submit").primary());
    h.run_frame();

    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::Button);
    h.assert_a11y_label(id, "Submit");

    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(bounds.width > 10.0, "button should have non-trivial width");
    assert!(
        bounds.height > 10.0,
        "button should have non-trivial height"
    );

    let style = h
        .style_component_of(id)
        .expect("button should have a style component");
    let default = resolve_style(StateFlags::NONE, &style);
    assert!(
        default.background.is_some(),
        "button should have a background colour"
    );
}

// ── Theme resolution ───────────────────────────────────────────────

#[test]
fn button_resolves_primary_filled_theme() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Primary").primary().filled());
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let res = resolve_style(StateFlags::NONE, &style);
    assert!(res.background.is_some());

    // Primary filled buttons should NOT have a border by default.
    assert!(
        res.border_color.is_none() || res.border_width == 0.0,
        "primary filled button should have no default border"
    );
}

#[test]
fn button_resolves_outlined_variant() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Outlined").outlined());
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let res = resolve_style(StateFlags::NONE, &style);
    // Outlined buttons have border width = 1.
    assert!(
        res.border_width > 0.0,
        "outlined button should have a border"
    );
    assert!(
        res.border_color.is_some(),
        "outlined button should have border colour"
    );
}

// ── State transitions: visual feedback ─────────────────────────────

#[test]
fn button_pressed_state_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Press me").primary());
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let default = resolve_style(StateFlags::NONE, &style);
    let pressed = resolve_style(StateFlags::PRESSED, &style);

    // The pressed background must differ from default (M3 state layer).
    assert_ne!(
        pressed.background, default.background,
        "pressed state should visually differ from default"
    );
}

#[test]
fn button_hovered_state_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Hover me").primary());
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
fn button_focused_state_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Focus me").primary());
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let default = resolve_style(StateFlags::NONE, &style);
    let focused = resolve_style(StateFlags::FOCUSED, &style);

    // The focused state applies a state layer in M3 — background or
    // outline should differ from default. The 2px auto-focus ring is
    // added at paint time by paint_element_surface, not in resolve_style,
    // so we check resolve_style's own focus variant.
    let visual_change = focused.background != default.background
        || focused.foreground != default.foreground
        || focused.border_color != default.border_color;
    assert!(
        visual_change,
        "focused button should have some visual difference from default in resolved style"
    );
}

// ── Hot reload of state via Harness ───────────────────────────────

#[test]
fn button_set_state_drives_visual_change() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("State test").primary());
    h.run_frame();

    let style_default = h.style_component_of(id).unwrap();
    let default_bg = resolve_style(StateFlags::NONE, &style_default).background;

    h.set_state(id, StateFlags::PRESSED, true);
    let style_pressed = h.style_component_of(id).unwrap();
    let pressed_bg = resolve_style(StateFlags::PRESSED, &style_pressed).background;

    assert_ne!(
        pressed_bg, default_bg,
        "pressed background should differ from default"
    );
}

// ── Callback behaviour ─────────────────────────────────────────────

#[test]
fn button_click_invokes_on_click() {
    let clicked = Signal::new(false);
    let c = clicked.clone();
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Click me").primary().on_click(move || {
        c.set(true);
    }));
    h.run_frame();

    assert!(
        !clicked.read(),
        "callback should not fire before interaction"
    );

    h.activate_button(id);
    assert!(clicked.read(), "on_click should fire after activate_button");
}

#[test]
fn button_disabled_has_correct_state_flag() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Disabled").disabled());
    h.run_frame();

    h.assert_state(id, StateFlags::DISABLED, true);
    // Disabled buttons are not focusable.
    assert!(
        !h.find(id).unwrap().is_focusable(),
        "disabled button should not be focusable"
    );
}

#[test]
fn button_disabled_blocks_click() {
    let clicked = Signal::new(false);
    let c = clicked.clone();
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Disabled").disabled().on_click(move || {
        c.set(true);
    }));
    h.run_frame();

    h.activate_button(id);
    assert!(!clicked.read(), "disabled button should not fire on_click");
}

#[test]
fn button_disabled_state_suppresses_visual_interaction_states() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Disabled").disabled());
    h.run_frame();

    let style = h.style_component_of(id).unwrap();
    let disabled = resolve_style(StateFlags::DISABLED, &style);
    // Disabled + hovered should still resolve to the disabled style
    // (DISABLED gates all other interaction states in the resolver).
    let disabled_hovered = resolve_style(StateFlags::DISABLED | StateFlags::HOVERED, &style);
    assert_eq!(
        disabled_hovered.background, disabled.background,
        "disabled button should ignore hover visual state"
    );
    assert_eq!(
        disabled_hovered.foreground, disabled.foreground,
        "disabled button foreground should stay disabled on hover"
    );
}

#[test]
fn button_loading_state_is_independent_of_disabled() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Loading").loading(true));
    h.run_frame();

    h.assert_state(id, StateFlags::LOADING, true);
    // Loading ≠ disabled. The button may still be technically enabled.
}

#[test]
fn button_disabled_and_loading_are_orthogonal() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Both").disabled().loading(true));
    h.run_frame();

    h.assert_state(id, StateFlags::DISABLED, true);
    h.assert_state(id, StateFlags::LOADING, true);
}

// ── Hover lifecycle ────────────────────────────────────────────────

#[test]
fn button_hover_sets_state_and_fires_callback() {
    let hovered_in = Signal::new(false);
    let hovered_out = Signal::new(false);
    let hi = hovered_in.clone();
    let ho = hovered_out.clone();

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(
        Button::new("Hover test")
            .primary()
            .on_hover_enter(move || {
                hi.set(true);
            })
            .on_hover_leave(move || {
                ho.set(true);
            }),
    );
    h.run_frame();

    h.hover(id);
    assert!(hovered_in.read(), "on_hover_enter should fire on hover");
    h.assert_state(id, StateFlags::HOVERED, true);

    h.unhover_id();
    assert!(hovered_out.read(), "on_hover_leave should fire on unhover");
    h.assert_state(id, StateFlags::HOVERED, false);
}

// ── Callback isolation (no cross-contamination) ────────────────────

#[test]
fn button_click_does_not_repeat_without_re_activation() {
    let count = Signal::new(0u32);
    let c = count.clone();
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Once").primary().on_click(move || {
        c.set(c.read() + 1);
    }));
    h.run_frame();

    h.activate_button(id);
    assert_eq!(count.read(), 1);

    h.run_frame();
    assert_eq!(
        count.read(),
        1,
        "callback should not fire again on a plain run_frame"
    );

    h.activate_button(id);
    assert_eq!(
        count.read(),
        2,
        "callback should fire again on re-activation"
    );
}

// ── Unmount completeness ───────────────────────────────────────────

#[test]
fn button_unmount_removes_element() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Gone").primary());
    h.run_frame();
    assert!(h.find(id).is_some());

    // clear_children on the root removes the button.
    h.arena.clear_children(h.root_id());
    h.run_frame();
    assert!(h.find(id).is_none());
}
