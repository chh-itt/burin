//! Tests for ScrollView and Tooltip.

use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::*;
use burin::widgets::overlay::Tooltip;

// ── ScrollView ────────────────────────────────────────────────────

#[test]
fn scroll_view_wraps_child_in_clip() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        ScrollView::new()
            .child(Text::new("content"))
            .scroll_direction(ScrollDirection::Vertical),
    );
    h.run_frame();
    // Clip wrapper + scrollbar element = 2 children.
    h.assert_child_count(id, 2);
}

#[test]
fn scroll_view_horizontal_creates_two_children() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        ScrollView::new()
            .child(Text::new("wide content"))
            .scroll_direction(ScrollDirection::Horizontal),
    );
    h.run_frame();
    // Clip wrapper + hbar = 2 children.
    h.assert_child_count(id, 2);
}

// ── ScrollView fixed-size ─────────────────────────────────────────

#[test]
fn scroll_view_has_scroll_offset() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        ScrollView::new()
            .child(
                VStack::new()
                    .push(Text::new("line 1"))
                    .push(Text::new("line 2"))
                    .push(Text::new("line 3")),
            )
            .width(200.0)
            .height(100.0),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    assert!(
        el.scroll_offset().is_some(),
        "ScrollView missing scroll_offset"
    );
    assert!(!el.children.is_empty(), "ScrollView has no children");
}

#[test]
fn scroll_view_scroll_does_not_panic() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        ScrollView::new()
            .child(
                VStack::new()
                    .push(Text::new("a"))
                    .push(Text::new("b"))
                    .push(Text::new("c")),
            )
            .width(200.0)
            .height(100.0),
    );
    h.run_frame();

    // content_bounds is set during the first paint pass.
    let el = h.find(id).unwrap();
    let cb = el.content_bounds().as_ref().unwrap().get();
    assert!(cb.width > 0.0, "content_bounds width should be > 0");
    assert!(cb.height > 0.0, "content_bounds height should be > 0");

    // Scroll via TestHarness: should clamp correctly and not panic.
    h.scroll(id, 0.0, 25.0).run_frame();
    let y = h
        .find(id)
        .unwrap()
        .scroll_offset()
        .as_ref()
        .unwrap()
        .get()
        .y;
    assert!(y >= 0.0, "scroll y should be >= 0");
}

#[test]
fn scroll_consumed_by_handler_skips_offset_fallback() {
    use std::cell::Cell;
    use std::rc::Rc;

    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(
        ScrollView::new()
            .child(
                VStack::new()
                    .push(Text::new("a"))
                    .push(Text::new("b"))
                    .push(Text::new("c")),
            )
            .width(200.0)
            .height(60.0),
    );
    h.run_frame();

    let before = h.find(id).unwrap().scroll_offset().as_ref().unwrap().get();

    // A handler that consumes the scroll (returns true) without mutating offset.
    let fired = Rc::new(Cell::new(false));
    let f = fired.clone();
    h.events_mut().register_events(
        id,
        burin::core::config::EventHandler::new().on_scroll(move |_dx, _dy| {
            f.set(true);
            true
        }),
    );

    h.scroll(id, 0.0, 50.0);

    assert!(fired.get(), "scroll handler must fire");
    let after = h.find(id).unwrap().scroll_offset().as_ref().unwrap().get();
    assert_eq!(
        after, before,
        "consumed scroll must NOT fall through to direct offset mutation (no double-apply)",
    );
}

#[test]
fn scroll_offset_matches_window_do_scroll_sign() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(
        ScrollView::new()
            .child(
                VStack::new()
                    .push(Text::new("a"))
                    .push(Text::new("b"))
                    .push(Text::new("c"))
                    .push(Text::new("d"))
                    .push(Text::new("e")),
            )
            .width(200.0)
            .height(60.0),
    );
    h.run_frame();

    // From a fresh ScrollView (offset 0, no handler), window do_scroll does
    // `o.y -= dy`, so a positive dy clamps to 0 (already at top). The old
    // harness did `o.y += dy` → a positive value, which is the wrong sign.
    h.scroll(id, 0.0, 50.0);
    let y = h
        .find(id)
        .unwrap()
        .scroll_offset()
        .as_ref()
        .unwrap()
        .get()
        .y;
    assert_eq!(
        y, 0.0,
        "positive dy from top must clamp to 0 (matches window do_scroll)"
    );
}

// ── Tooltip ───────────────────────────────────────────────────────

#[test]
fn tooltip_mounts_with_anchor_child() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Tooltip::new(
        Button::new("Hover me").primary(),
        Text::new("This is a tooltip").font_size(12.0),
    ));
    h.run_frame();

    let el = h.find(id).unwrap();
    // Tooltip wraps child in a Popover; the anchor element is a child.
    assert!(
        !el.children.is_empty(),
        "Tooltip wrapper should have child element"
    );
}

#[test]
fn tooltip_anchor_is_visible() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Tooltip::new(
        Text::new("host"),
        Text::new("tip").font_size(12.0),
    ));
    h.run_frame();

    let el = h.find(id).unwrap();
    assert!(el.is_visible(), "Tooltip wrapper should be visible");
    assert!(!el.children.is_empty(), "Tooltip should have anchor child");
}

#[test]
fn tooltip_content_is_text_widget() {
    // Verify the Tooltip API accepts Widget content (not just String).
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        Tooltip::new(
            Button::new("Hover me").primary(),
            Text::new("help text").font_size(12.0),
        )
        .delay(0),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    assert!(
        !el.children.is_empty(),
        "Tooltip should mount successfully with widget content"
    );
}

// ── unhover smoke test ─────────────────────────────────────────────

#[test]
fn unhover_clears_hover_state() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Button::new("Hoverable").primary());
    h.run_frame();

    let el = h.find(id).unwrap();
    let cx = el.screen_bounds.x + el.screen_bounds.width * 0.5;
    let cy = el.screen_bounds.y + el.screen_bounds.height * 0.5;

    h.hover_at(Point::new(cx, cy)).run_frame();
    let el = h.find(id).unwrap();
    assert!(el
        .state
        .get()
        .contains(burin::core::config::StateFlags::HOVERED));

    h.unhover().run_frame();
    let el = h.find(id).unwrap();
    assert!(!el
        .state
        .get()
        .contains(burin::core::config::StateFlags::HOVERED));
}
