//! SEAM-2 parity regression tests (audit 2026-07-16, round 4).
//!
//! `drive_frame_platform` is the SHARED between-layout-and-paint phase:
//! long-press wins, drag ghost, drag-z elevation, autofocus with full
//! production semantics, and the accessibility-action dispatch all run
//! through the same code path in `Window::on_frame` and
//! `TestHarness::run_frame`. These tests exercise behaviours that were
//! previously window-only (dead in tests): if any of them regresses, the
//! harness sees exactly what production would do.

use auralis_signal::Signal;
use burin::core::app_context::current_app;
use burin::platform::a11y_bridge::{push_a11y_action, A11yAction};
use burin::style::{Dimension, Styled};
use burin::testing::TestHarness;
use burin::widgets::input::{Button, Checkbox, TextInput};
use burin::widgets::layout::VStack;

/// The a11y action queue is process-global (`static Mutex`) — serialize
/// the tests that use it so parallel test threads don't drain each
/// other's actions.
static A11Y_QUEUE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn px(w: f32) -> Dimension {
    Dimension::Pixels(w)
}

/// A11y Focus actions run in the harness with full transfer semantics.
#[test]
fn a11y_focus_action_transfers_focus() {
    let _guard = A11Y_QUEUE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = TestHarness::new(400.0, 300.0);
    let c1 = Signal::new(false);
    let c2 = Signal::new(false);
    let page = h.mount(
        VStack::new()
            .width(px(300.0))
            .height(px(200.0))
            .push(Checkbox::new(c1.clone()))
            .push(Checkbox::new(c2.clone())),
    );
    h.run_frame();
    let boxes: Vec<_> = h.find(page).unwrap().children.clone();
    let (a, b) = (boxes[0], boxes[1]);

    push_a11y_action(A11yAction::Focus(a));
    h.run_frame();
    assert_eq!(h.focused(), Some(a), "a11y Focus reaches the harness");

    push_a11y_action(A11yAction::Focus(b));
    h.run_frame();
    assert_eq!(h.focused(), Some(b), "second a11y Focus transfers");

    push_a11y_action(A11yAction::Blur(b));
    h.run_frame();
    assert_eq!(h.focused(), None, "a11y Blur clears focus");
}

/// A11y Click actions fire the element's click handlers in the harness.
#[test]
fn a11y_click_action_fires_handler() {
    let _guard = A11Y_QUEUE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = TestHarness::new(400.0, 300.0);
    let clicked = Signal::new(0i32);
    let c = clicked.clone();
    let id = h.mount(Button::new("go").on_click(move || c.set(c.read() + 1)));
    h.run_frame();

    push_a11y_action(A11yAction::Click(id));
    h.run_frame();
    assert_eq!(
        h.read_signal(&clicked),
        1,
        "a11y Click dispatched through SEAM 2"
    );
}

/// Autofocus now runs the full production transfer: the previously
/// focused element loses focus (state flag cleared), not just a silent
/// `set_focused` swap.
#[test]
fn autofocus_fires_full_transfer_semantics() {
    use burin::core::config::StateFlags;

    let _guard = A11Y_QUEUE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = TestHarness::new(400.0, 300.0);
    let c1 = Signal::new(false);
    let page = h.mount(
        VStack::new()
            .width(px(300.0))
            .height(px(200.0))
            .push(Checkbox::new(c1.clone())),
    );
    h.run_frame();
    let first = h.find(page).unwrap().children[0];

    push_a11y_action(A11yAction::Focus(first));
    h.run_frame();
    assert_eq!(h.focused(), Some(first));

    // Mount a widget requesting autofocus — production semantics must
    // move focus AND clear the old element's FOCUSED state.
    let value = Signal::new(String::new());
    let input = h.mount(TextInput::new(value.clone()).autofocus());
    h.run_frame();

    assert_eq!(h.focused(), Some(input), "autofocus transferred focus");
    let old_focused_flag = h
        .find(first)
        .map(|el| el.state.get().contains(StateFlags::FOCUSED))
        .unwrap_or(false);
    assert!(
        !old_focused_flag,
        "previously focused element must receive the focus-out side of the transfer"
    );
}

/// Drag-z elevation requests are honoured by the shared SEAM 2:
/// elevate sets z_index_floor=1, de-elevate restores the saved floor.
#[test]
fn drag_z_elevation_round_trip() {
    let mut h = TestHarness::new(400.0, 300.0);
    let c1 = Signal::new(false);
    let page = h.mount(
        VStack::new()
            .width(px(300.0))
            .height(px(200.0))
            .push(Checkbox::new(c1.clone())),
    );
    h.run_frame();
    let row = h.find(page).unwrap().children[0];
    let original_floor = h.find(row).unwrap().z_index_floor;

    current_app().request_drag_z(row, true);
    h.run_frame();
    assert_eq!(
        h.find(row).unwrap().z_index_floor,
        Some(1),
        "elevated row gets z_index_floor=1"
    );

    current_app().request_drag_z(row, false);
    h.run_frame();
    assert_eq!(
        h.find(row).unwrap().z_index_floor,
        original_floor,
        "de-elevation restores the saved floor"
    );
}

/// A11y scroll actions move scroll offsets in the harness.
#[test]
fn a11y_scroll_actions_move_offset() {
    use burin::widgets::display::Text;
    use burin::widgets::layout::{ScrollView, SizedBox};

    let _guard = A11Y_QUEUE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut content = VStack::new();
    for i in 0..100 {
        content = content.push(Text::new(format!("line {i}")));
    }
    let mut h = TestHarness::new(400.0, 300.0);
    let mounted = h.mount(
        SizedBox::new()
            .width(300.0)
            .height(200.0)
            .child(ScrollView::new().child(content)),
    );
    h.run_frame();

    // Find the scrollable element.
    let target = {
        let mut found = None;
        let mut stack = vec![mounted];
        while let Some(id) = stack.pop() {
            if h.root().comp_scroll(id).is_some() {
                found = Some(id);
                break;
            }
            if let Some(el) = h.find(id) {
                stack.extend(el.children.iter().copied());
            }
        }
        found.expect("scrollable element")
    };

    // a11y scroll routes through the SAME path as wheel scrolling
    // (audit round 5): ScrollDown shows content below (offset grows,
    // clamped to content bounds), ScrollUp returns toward the top.
    push_a11y_action(A11yAction::ScrollDown(target));
    h.run_frame();
    let after_down = h.root().comp_scroll(target).unwrap().scroll_offset.get();
    assert_eq!(
        after_down.y, 40.0,
        "ScrollDown moves the viewport down by 40"
    );

    push_a11y_action(A11yAction::ScrollUp(target));
    h.run_frame();
    let after_up = h.root().comp_scroll(target).unwrap().scroll_offset.get();
    assert_eq!(after_up.y, 0.0, "ScrollUp returns toward the top");

    push_a11y_action(A11yAction::SetScrollOffset {
        target,
        x: 0.0,
        y: 120.0,
    });
    h.run_frame();
    let after_set = h.root().comp_scroll(target).unwrap().scroll_offset.get();
    assert_eq!(
        after_set.y, 120.0,
        "SetScrollOffset applied (clamped to content)"
    );

    push_a11y_action(A11yAction::SetScrollOffset {
        target,
        x: 0.0,
        y: 1.0e9,
    });
    h.run_frame();
    let clamped = h.root().comp_scroll(target).unwrap().scroll_offset.get();
    assert!(
        clamped.y < 1.0e9 && clamped.y > 0.0,
        "absurd offsets are clamped to the content bounds (got {})",
        clamped.y
    );
}
