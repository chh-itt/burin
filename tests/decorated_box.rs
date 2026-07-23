use burin::testing::selector::*;
use burin::testing::test_harness::TestHarness;
use burin::widgets::decoration::DecoratedBox;
use burin::widgets::display::Text;

#[test]
fn decorated_box_mounts_with_child() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(DecoratedBox::new().child(Text::new("Inside")));
    h.run_frame();
    assert!(
        h.find_sel(by_text("Inside")).is_some(),
        "Child text visible"
    );
}

#[test]
fn decorated_box_nested() {
    let mut h = TestHarness::new(400.0, 400.0);
    h.mount(DecoratedBox::new().child(DecoratedBox::new().child(Text::new("Nested"))));
    h.run_frame();
}

#[test]
fn multiple_decorated_boxes_no_panic() {
    let mut h = TestHarness::new(400.0, 400.0);
    h.mount(
        burin::widgets::layout::VStack::new()
            .push(DecoratedBox::new().child(Text::new("A")))
            .push(DecoratedBox::new().child(Text::new("B")))
            .push(DecoratedBox::new().child(Text::new("C"))),
    );
    h.run_frame();
}
