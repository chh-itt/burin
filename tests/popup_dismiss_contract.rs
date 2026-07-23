//! Popup dismiss contract matrix (audit 2026-07-18, AnchoredPopup pass).
//!
//! Locks the interaction contract for anchored popups (Select dropdown)
//! and their nesting inside OverlayStack overlays (Modal):
//!
//! - Escape closes popups in strict LIFO order: the innermost open popup
//!   closes first; the outer Modal only closes on the NEXT Escape.
//! - An outside click closes the innermost anchored popup only; the Modal
//!   requires a second click (standard two-step dismiss UX).
//! - Single-popup behaviours (Escape / outside click / option select)
//!   keep their historical semantics.

use auralis_signal::Signal;
use burin::event::{Key, Modifiers};
use burin::style::Point;
use burin::testing::selector::by_role;
use burin::testing::TestHarness;
use burin::widgets::input::Select;
use burin::widgets::layout::VStack;
use burin::widgets::overlay::Modal;

fn mount_select(h: &mut TestHarness) -> burin::core::ElementId {
    let selected = Signal::new(None::<String>);
    h.mount(
        Select::new(selected)
            .options(vec!["Red".into(), "Green".into(), "Blue".into()])
            .placeholder("Pick"),
    )
}

fn open_select(h: &mut TestHarness) -> burin::core::ElementId {
    let combos = h.find_all_sel(by_role(accesskit::Role::ComboBox));
    assert!(!combos.is_empty(), "select trigger present");
    h.click(combos[0]);
    h.run_frame();
    let options = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    assert!(!options.is_empty(), "options mounted");
    let popup = h.popup_root_of(options[0]).expect("popup root");
    assert_eq!(
        h.reactive_visible_of(popup),
        Some(true),
        "dropdown visible after trigger click"
    );
    popup
}

#[test]
fn escape_closes_select_dropdown() {
    let mut h = TestHarness::new(500.0, 400.0);
    mount_select(&mut h);
    h.run_frame();
    let popup = open_select(&mut h);

    h.press_key(Key::Escape, Modifiers::default());
    h.run_frame();
    assert_eq!(
        h.reactive_visible_of(popup),
        Some(false),
        "Escape must close the dropdown"
    );
}

#[test]
fn outside_click_closes_select_dropdown() {
    let mut h = TestHarness::new(500.0, 400.0);
    mount_select(&mut h);
    h.run_frame();
    let popup = open_select(&mut h);

    // Bottom-right corner: outside both the trigger and the dropdown.
    h.click_at(Point::new(480.0, 380.0));
    h.run_frame();
    assert_eq!(
        h.reactive_visible_of(popup),
        Some(false),
        "outside click must close the dropdown"
    );
}

#[test]
fn option_select_closes_dropdown() {
    let mut h = TestHarness::new(500.0, 400.0);
    mount_select(&mut h);
    h.run_frame();
    let popup = open_select(&mut h);

    let options = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    h.click(options[0]);
    h.run_frame();
    assert_eq!(
        h.reactive_visible_of(popup),
        Some(false),
        "picking an option must close the dropdown"
    );
}

fn mount_modal_with_select(h: &mut TestHarness) -> Signal<bool> {
    let visible = Signal::new(true);
    let selected = Signal::new(None::<String>);
    h.mount(Modal::new(
        visible.clone(),
        VStack::new().push(
            Select::new(selected)
                .options(vec!["Red".into(), "Green".into(), "Blue".into()])
                .placeholder("Pick"),
        ),
    ));
    h.run_frame();
    visible
}

#[test]
fn escape_closes_nested_select_before_modal() {
    let mut h = TestHarness::new(500.0, 400.0);
    let modal_visible = mount_modal_with_select(&mut h);
    assert!(modal_visible.read(), "modal open");

    let popup = open_select(&mut h);

    // First Escape: the INNERMOST popup (Select dropdown) closes; the
    // Modal stays open. LIFO overlay semantics.
    h.press_key(Key::Escape, Modifiers::default());
    h.run_frame();
    assert_eq!(
        h.reactive_visible_of(popup),
        Some(false),
        "first Escape closes the dropdown, not the modal"
    );
    assert!(
        modal_visible.read(),
        "modal must survive the first Escape while its child popup was open"
    );

    // Second Escape: now the Modal closes.
    h.press_key(Key::Escape, Modifiers::default());
    h.run_frame();
    assert!(!modal_visible.read(), "second Escape closes the modal");
}

#[test]
fn outside_click_two_step_dismiss_in_modal() {
    let mut h = TestHarness::new(500.0, 400.0);
    let modal_visible = mount_modal_with_select(&mut h);
    let popup = open_select(&mut h);

    // First outside click: closes the dropdown only.
    h.click_at(Point::new(490.0, 390.0));
    h.run_frame();
    assert_eq!(
        h.reactive_visible_of(popup),
        Some(false),
        "first outside click closes the dropdown"
    );
    assert!(
        modal_visible.read(),
        "modal survives the click that closed its child popup"
    );

    // Second outside click: closes the modal.
    h.click_at(Point::new(490.0, 390.0));
    h.run_frame();
    assert!(
        !modal_visible.read(),
        "second outside click closes the modal"
    );
}
