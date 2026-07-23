//! W2 guards: touch-first scrolling — ScrollRecognizer in the gesture
//! arena (mobile-groundwork, audit 2026-07-19).
//!
//! - Touch-only: a single finger dragging a scroll container IS the
//!   scroll gesture on touch; mouse pointers never activate it (desktop
//!   scrolls via wheel/scrollbar — mouse-drag must not hijack).
//! - Direction verdict at TOUCH_SLOP (8px): the intent axis must be
//!   scrollable by the container, otherwise the recognizer bows out.
//! - Winning captures the pointer: subsequent moves apply scroll offset
//!   directly; release feeds tracked velocity into the existing fling.
//! - A scroll win SUPPRESSES click synthesis for that pointer sequence —
//!   scrolling over a button must not press it (the touch contract).

use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::{ScrollView, SizedBox, VStack};
use std::cell::Cell;
use std::rc::Rc;

fn scroll_offset_y(h: &TestHarness, scroll_id: burin::core::ElementId) -> f32 {
    h.arena
        .comp_scroll(scroll_id)
        .map_or(0.0, |sc| sc.scroll_offset.get().y)
}

fn tall_scroll(h: &mut TestHarness) -> burin::core::ElementId {
    let id = h.mount(
        ScrollView::new().child(
            VStack::new()
                .push(SizedBox::new().width(200.0).height(1200.0))
                .push(Text::new("bottom")),
        ),
    );
    h.run_frame();
    h.run_frame();
    id
}

/// A vertical touch drag scrolls the container (content follows the finger).
#[test]
fn touch_vertical_drag_scrolls_the_container() {
    let mut h = TestHarness::new(300.0, 200.0);
    let sid = tall_scroll(&mut h);

    h.touch_down_at(Point::new(150.0, 150.0));
    h.touch_move_at(Point::new(150.0, 140.0)); // 10px up — past TOUCH_SLOP(8)
    h.touch_move_at(Point::new(150.0, 100.0)); // keep dragging up
    h.run_frame();

    let off = scroll_offset_y(&h, sid);
    assert!(
        off > 10.0,
        "finger moved up 50px → content scrolls down (offset grows), got {off}"
    );
    h.touch_up_at(Point::new(150.0, 100.0));
}

/// Mouse drags must NOT scroll — desktop scrolls via wheel/scrollbar,
/// and mouse-dragging over content is selection/no-op, never a scroll.
#[test]
fn mouse_drag_does_not_scroll() {
    let mut h = TestHarness::new(300.0, 200.0);
    let sid = tall_scroll(&mut h);

    h.pointer_down_at(Point::new(150.0, 150.0));
    h.pointer_move_at(Point::new(150.0, 100.0));
    h.pointer_up_at(Point::new(150.0, 100.0));
    h.run_frame();

    assert_eq!(
        scroll_offset_y(&h, sid),
        0.0,
        "mouse pointers never activate the ScrollRecognizer"
    );
}

/// A horizontal swipe on a vertical-only container is rejected (the
/// recognizer bows out at the direction verdict).
#[test]
fn horizontal_swipe_on_vertical_container_is_rejected() {
    let mut h = TestHarness::new(300.0, 200.0);
    let sid = tall_scroll(&mut h);

    h.touch_down_at(Point::new(100.0, 150.0));
    h.touch_move_at(Point::new(160.0, 152.0)); // strongly horizontal
    h.touch_move_at(Point::new(220.0, 154.0));
    h.touch_up_at(Point::new(220.0, 154.0));
    h.run_frame();

    assert_eq!(
        scroll_offset_y(&h, sid),
        0.0,
        "horizontal intent must not vertically scroll"
    );
}

/// Scrolling over a button suppresses the click; a clean touch tap on
/// the same button still clicks. THE touch contract.
#[test]
fn touch_scroll_suppresses_click_but_tap_still_clicks() {
    let mut h = TestHarness::new(300.0, 200.0);
    let clicks: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let c = clicks.clone();
    h.mount(
        ScrollView::new().child(
            VStack::new()
                .push(Button::new("press me").on_click(move || c.set(c.get() + 1)))
                .push(SizedBox::new().width(200.0).height(1200.0)),
        ),
    );
    h.run_frame();
    h.run_frame();

    // Find the button center via its accessible role — simpler: the top strip.
    let btn_pos = Point::new(60.0, 18.0);

    // Sequence 1: drag starting ON the button → scroll, NOT a click.
    h.touch_down_at(btn_pos);
    h.touch_move_at(Point::new(60.0, btn_pos.y - 30.0));
    h.touch_up_at(Point::new(60.0, btn_pos.y - 30.0));
    h.run_frame();
    assert_eq!(
        clicks.get(),
        0,
        "a scroll that started on the button must not click it"
    );

    // Sequence 2: clean tap on the button (scroll back to top first).
    h.scroll(
        *h.find(h.root_id()).unwrap().children.first().unwrap(),
        0.0,
        10_000.0,
    );
    h.run_frame();
    h.touch_down_at(btn_pos);
    h.touch_up_at(btn_pos);
    h.run_frame();
    assert_eq!(clicks.get(), 1, "a clean touch tap still clicks");
}

/// Releasing a fast drag hands tracked velocity to the fling — the
/// offset keeps advancing after the finger lifts.
#[test]
fn touch_fling_continues_after_release() {
    let mut h = TestHarness::new(300.0, 200.0);
    let sid = tall_scroll(&mut h);

    h.touch_down_at(Point::new(150.0, 180.0));
    // Fast upward swipe: 40px per 16ms step.
    for i in 1..=4 {
        h.advance_time(16);
        h.touch_move_at(Point::new(150.0, 180.0 - 40.0 * i as f32));
    }
    h.advance_time(16);
    h.touch_up_at(Point::new(150.0, 20.0));
    h.run_frame();
    let at_release = scroll_offset_y(&h, sid);
    assert!(
        at_release > 100.0,
        "dragged ~160px before release, got {at_release}"
    );

    // Let the fling run on the virtual clock.
    for _ in 0..10 {
        h.advance_time(16);
        h.run_frame();
    }
    let after = scroll_offset_y(&h, sid);
    assert!(
        after > at_release + 20.0,
        "fling keeps scrolling after release: {at_release} → {after}"
    );
}
