//! Accessibility semantic finders — locate widgets by role / label, the way
//! egui-kittest, Slint and Flutter (bySemanticsLabel) do.

use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::VStack;

#[test]
fn get_by_role_and_label() {
    let mut h = TestHarness::new(300.0, 240.0);
    h.mount(
        VStack::new()
            .push(Button::new("Save").primary())
            .push(Button::new("Cancel"))
            .push(Text::new("Status: ready")),
    );
    h.run_frame();

    // Role-based: at least the two buttons are found.
    let buttons = h.get_all_by_role(accesskit::Role::Button);
    assert!(
        buttons.len() >= 2,
        "expected >=2 buttons, got {}",
        buttons.len()
    );

    // get_by_role returns the first Button (panics if none).
    let _first = h.get_by_role(accesskit::Role::Button);

    // Label substring vs exact.
    let _save = h.get_by_label_contains("Save");
    assert!(
        h.query_by_label("Status: ready").is_some(),
        "exact label lookup"
    );
    assert!(
        h.query_by_label("Status").is_none(),
        "exact label must not substring-match"
    );
}

#[test]
#[should_panic(expected = "get_by_label")]
fn get_by_label_panics_when_absent() {
    let mut h = TestHarness::new(120.0, 80.0);
    h.mount(VStack::new().push(Text::new("hello")));
    h.run_frame();
    h.get_by_label("nonexistent-label");
}
