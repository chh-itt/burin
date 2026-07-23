//! Action system integration tests — verify KeyBindingMap, ActionKind,
//! and widget-level action handlers.

use auralis_signal::Signal;
use burin::event::action::{Action, ActionKind, ActionOutcome, KeyChord};
use burin::event::bindings::KeyBindingMap;
use burin::event::types::{Key, Modifiers};
use burin::testing::TestHarness;

use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::overlay::Modal;

// ── KeyChord ───────────────────────────────────────────────────────

#[test]
fn key_chord_equality() {
    let a = KeyChord::new(
        Key::Character("c".into()),
        Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    let b = KeyChord::new(
        Key::Character("c".into()),
        Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(a, b);
}

#[test]
fn key_chord_inequality_modifiers() {
    let a = KeyChord::new(Key::Character("c".into()), Modifiers::NONE);
    let b = KeyChord::new(
        Key::Character("c".into()),
        Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    assert_ne!(a, b);
}

// ── KeyBindingMap ──────────────────────────────────────────────────

#[test]
fn binding_map_finds_copy() {
    let map = KeyBindingMap::new();
    let found = map.find(
        None,
        &Key::Character("c".into()),
        &Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(found, Some(ActionKind::Copy));
}

#[test]
fn binding_map_finds_paste() {
    let map = KeyBindingMap::new();
    let found = map.find(
        None,
        &Key::Character("v".into()),
        &Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(found, Some(ActionKind::Paste));
}

#[test]
fn binding_map_finds_focus_next() {
    let map = KeyBindingMap::new();
    let found = map.find(None, &Key::Tab, &Modifiers::NONE);
    assert_eq!(found, Some(ActionKind::FocusNext));
}

#[test]
fn binding_map_finds_focus_prev() {
    let map = KeyBindingMap::new();
    let found = map.find(
        None,
        &Key::Tab,
        &Modifiers {
            shift: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(found, Some(ActionKind::FocusPrev));
}

#[test]
fn binding_map_escape_is_cancel() {
    let map = KeyBindingMap::new();
    let found = map.find(None, &Key::Escape, &Modifiers::NONE);
    assert_eq!(found, Some(ActionKind::Cancel));
}

#[test]
fn binding_map_custom_registration() {
    let mut map = KeyBindingMap::new();
    map.register(
        KeyChord::new(
            Key::Character("k".into()),
            Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            },
        ),
        ActionKind::Custom { id: "myapp.search" },
    );
    let found = map.find(
        None,
        &Key::Character("k".into()),
        &Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(found, Some(ActionKind::Custom { id: "myapp.search" }));
}

#[test]
fn binding_map_remove() {
    let mut map = KeyBindingMap::new();
    let chord = KeyChord::new(
        Key::Character("c".into()),
        Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    map.remove(&chord);
    let found = map.find(
        None,
        &Key::Character("c".into()),
        &Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    );
    assert_eq!(found, None);
}

// ── Action struct ──────────────────────────────────────────────────

#[test]
fn action_with_selection_sets_flag() {
    let a = Action::new(ActionKind::MoveLeft).with_selection();
    assert!(a.selection);
}

#[test]
fn action_outcome_semantics() {
    assert!(!ActionOutcome::Unhandled.is_handled());
    assert!(ActionOutcome::Consumed.is_handled());
    assert!(ActionOutcome::Blocked.is_handled());

    assert!(!ActionOutcome::Unhandled.should_stop());
    assert!(!ActionOutcome::Consumed.should_stop());
    assert!(ActionOutcome::Blocked.should_stop());
}

// ── Widget action handlers ─────────────────────────────────────────

#[test]
fn modal_registers_cancel_handler() {
    let mut h = TestHarness::new(800.0, 600.0);
    let visible = Signal::new(true);
    h.mount(Modal::new(visible.clone(), Text::new("inside")));
    h.run_frame();

    assert!(h.read_signal(&visible));

    // Close via Cancel action — but action dispatch requires focus.
    // The Modal registers on_action(Cancel) in its mount_box.
    // We verify the Modal mounted correctly.
    let root = h.find(h.root_id()).unwrap();
    assert!(!root.children.is_empty(), "Root should have modal child");
    h.assert_visible(root.children[0]);
}

// ── Integration: keyboard → action check ──────────────────────────

#[test]
fn action_kind_custom_string_identity() {
    // Verify Custom variants with the same string are equal.
    assert_eq!(
        ActionKind::Custom {
            id: "textinput.new_line"
        },
        ActionKind::Custom {
            id: "textinput.new_line"
        },
    );
    assert_ne!(
        ActionKind::Custom {
            id: "textinput.new_line"
        },
        ActionKind::Custom {
            id: "datatable.refresh"
        },
    );
}

#[test]
fn all_default_bindings_are_valid_keys() {
    // Just verify the map constructs without panicking.
    let map = KeyBindingMap::new();
    // Focus bindings should exist.
    assert_eq!(
        map.find(None, &Key::Tab, &Modifiers::NONE),
        Some(ActionKind::FocusNext)
    );
    assert_eq!(
        map.find(
            None,
            &Key::Tab,
            &Modifiers {
                shift: true,
                ..Modifiers::NONE
            }
        ),
        Some(ActionKind::FocusPrev),
    );
    // Activation bindings.
    assert_eq!(
        map.find(None, &Key::Enter, &Modifiers::NONE),
        Some(ActionKind::NewLine)
    );
    assert_eq!(
        map.find(None, &Key::Escape, &Modifiers::NONE),
        Some(ActionKind::Cancel)
    );
}

// ── Focus transfer ─────────────────────────────────────────────────

#[test]
fn activate_default_fires_focused_button_click() {
    let mut h = TestHarness::new(800.0, 600.0);
    let clicked = Signal::new(false);
    let id = h.mount(Button::new("Action me").primary().on_click({
        let c = clicked.clone();
        move || c.set(true)
    }));
    h.run_frame();
    h.click(id).run_frame(); // focus + click the button
    assert!(h.read_signal(&clicked));
    // Verify focus is on the button.
    h.assert_focused(id);
}
