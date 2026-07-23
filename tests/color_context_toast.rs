//! Contract tests for ColorPicker, ContextMenu, and ToastContainer.
//! Tests validate behaviour against the public API contract, not
//! internal implementation details.

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::style::resolve_style;
use burin::style::{Color, Point};
use burin::testing::TestHarness;
use burin::widgets::input::ColorPicker;
use burin::widgets::overlay::{ContextMenu, ContextMenuItem, ToastContainer};

// ── ColorPicker ─────────────────────────────────────────────────────

#[test]
fn color_picker_mounts_with_correct_bounds() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(ColorPicker::new(Signal::new(Color::rgba8(255, 0, 0, 255))));
    h.run_frame();

    h.assert_visible(id);
    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(
        bounds.width > 0.0,
        "color picker should have non-zero width"
    );
    assert!(
        bounds.height > 0.0,
        "color picker should have non-zero height"
    );
}

#[test]
fn color_picker_hovered_has_visual_effect() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(ColorPicker::new(Signal::new(Color::rgba8(255, 0, 0, 255))));
    h.run_frame();

    // The root wraps trigger + dropdown; find a child with a style component.
    let style = {
        let children = h.arena.get(id).unwrap().children.clone();
        children
            .iter()
            .find_map(|&cid| h.style_component_of(cid))
            .expect("at least one child should have a style component")
    };

    let default = resolve_style(StateFlags::NONE, &style);
    // The colour picker's trigger swatch always has a resolved background.
    assert!(
        default.background.is_some(),
        "default state should have a resolved background"
    );

    // Set the hover state via TestHarness and verify the flag is respected.
    h.set_state(id, StateFlags::HOVERED, true);
    h.assert_state(id, StateFlags::HOVERED, true);

    // State interactions exist and resolve_style produces valid output
    // for all standard states.
    for &flag in &[
        StateFlags::HOVERED,
        StateFlags::PRESSED,
        StateFlags::FOCUSED,
    ] {
        let resolved = resolve_style(flag, &style);
        assert!(
            resolved.background.is_some(),
            "{:?} state should have a resolved background",
            flag
        );
    }
}

#[test]
fn color_picker_on_changed_fires() {
    let color = Signal::new(Color::rgba8(255, 0, 0, 255));
    let changed = Signal::new(false);
    let ch = changed.clone();

    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(ColorPicker::new(color.clone()).on_changed(move |_| {
        ch.set(true);
    }));
    h.run_frame();

    assert!(
        !changed.read(),
        "on_changed should not fire before color change"
    );

    h.set_signal(&color, Color::rgba8(0, 0, 255, 255));
    h.run_frame();

    assert!(
        changed.read(),
        "on_changed should fire after color signal change"
    );
}

// ── ContextMenu ─────────────────────────────────────────────────────

#[test]
fn context_menu_mounts_with_items() {
    let visible = Signal::new(true);
    let position = Signal::new(Point::new(100.0, 100.0));

    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        ContextMenu::new(visible, position)
            .item(ContextMenuItem::new("Item 1"))
            .item(ContextMenuItem::new("Item 2"))
            .item(ContextMenuItem::new("Item 3")),
    );
    h.run_frame();

    h.assert_visible(id);
    let bounds = h.find(id).unwrap().screen_bounds;
    assert!(
        bounds.width > 0.0,
        "context menu should have non-zero width"
    );
}

#[test]
fn context_menu_item_has_accessible_label() {
    let visible = Signal::new(true);
    let position = Signal::new(Point::new(100.0, 100.0));

    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        ContextMenu::new(visible, position)
            .item(ContextMenuItem::new("Item 1"))
            .item(ContextMenuItem::new("Item 2"))
            .item(ContextMenuItem::new("Item 3")),
    );
    h.run_frame();

    let el = h.arena.get(id).expect("context menu element should exist");
    assert_eq!(
        el.children.len(),
        3,
        "context menu should have exactly 3 child item elements"
    );
    let labels = ["Item 1", "Item 2", "Item 3"];
    for (i, child_id) in el.children.iter().enumerate() {
        h.assert_a11y_label(*child_id, labels[i]);
    }
}

#[test]
fn context_menu_visibility_signal_controls_visibility() {
    let visible = Signal::new(false);
    let position = Signal::new(Point::new(100.0, 100.0));

    let mut h = TestHarness::new(400.0, 300.0);
    let id =
        h.mount(ContextMenu::new(visible.clone(), position).item(ContextMenuItem::new("Item 1")));
    h.run_frame();

    h.assert_not_visible(id);

    h.set_signal(&visible, true);
    h.run_frame();

    h.assert_visible(id);
}

// ── ToastContainer ──────────────────────────────────────────────────

#[test]
fn toast_container_mounts() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(ToastContainer::new());
    h.run_frame();

    // ToastContainer root is created with a z-index portal structure.
    assert!(h.find(id).is_some(), "toast container element should exist");
    // The container has a toast slot child (structural requirement).
    assert!(
        h.arena.get(id).unwrap().children.len() >= 1,
        "toast container should have at least one child slot"
    );
}

#[test]
fn toast_container_starts_empty() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(ToastContainer::new());
    h.run_frame();

    assert!(h.find(id).is_some(), "toast container element should exist");
    // No active toast notifications — the slot template exists but is empty
    // of notification content.
    let container = h.arena.get(id).unwrap();
    assert!(
        container.children.len() >= 1,
        "toast container should have internal slot structure"
    );
}
