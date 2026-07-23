//! Integration tests: verify widgets mount correctly with proper tree structure.

use auralis_signal::Signal;
use burin::style::Dimension;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::{Button, Checkbox};
use burin::widgets::layout::{Conditional, HStack, SizedBox, VStack, ZStack};

fn mount(w: impl burin::core::widget::Widget) -> TestHarness {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(w);
    h.run_frame();
    h
}

fn first_child(h: &TestHarness) -> ElementId {
    let root = h.find(h.root_id()).unwrap();
    root.children[0]
}
use burin::core::ElementId;

#[test]
fn text_widget_has_accessible_label() {
    let h = mount(Text::new("Hello, world!"));
    let text = h.find(first_child(&h)).unwrap();
    assert_eq!(text.accessible_label().as_deref(), Some("Hello, world!"));
}

#[test]
fn vstack_contains_children() {
    let h = mount(
        VStack::new()
            .push(Text::new("A"))
            .push(Text::new("B"))
            .push(Text::new("C")),
    );
    let vstack = h.find(first_child(&h)).unwrap();
    assert_eq!(vstack.children.len(), 3);
}

#[test]
fn hstack_contains_children() {
    let h = mount(
        HStack::new()
            .push(Text::new("Left"))
            .push(Text::new("Right")),
    );
    let hstack = h.find(first_child(&h)).unwrap();
    assert_eq!(hstack.children.len(), 2);
}

#[test]
fn zstack_layers_children() {
    let h = mount(
        ZStack::new()
            .push(Text::new("back"))
            .push(Text::new("front")),
    );
    let zstack = h.find(first_child(&h)).unwrap();
    assert_eq!(zstack.children.len(), 2);
}

#[test]
fn button_has_accessible_role() {
    let h = mount(Button::new("Save").primary());
    let btn = h.find(first_child(&h)).unwrap();
    assert_eq!(btn.accessible_label(), Some("Save".to_string()));
}

#[test]
fn button_variants_still_mount() {
    for label in &["Save", "X", "OK"] {
        let h = mount(Button::new(*label));
        let btn = h.find(first_child(&h)).unwrap();
        assert!(
            btn.accessible_label().is_some(),
            "Button '{}' should have label",
            label
        );
    }
}

#[test]
fn checkbox_mounts_with_label() {
    let checked = Signal::new(false);
    let h = mount(Checkbox::new(checked.clone()));
    let cb = h.find(first_child(&h)).unwrap();
    assert!(
        cb.accessible_label().is_some(),
        "Checkbox should have an accessible label"
    );
}

#[test]
fn sized_box_has_content() {
    let h = mount(
        SizedBox::new()
            .width(Dimension::Pixels(200.0))
            .height(Dimension::Pixels(100.0))
            .child(Text::new("inside")),
    );
    let sb = h.find(first_child(&h)).unwrap();
    assert_eq!(sb.children.len(), 1);
}

#[test]
fn conditional_children() {
    let cond = Signal::new(true);
    let h = mount(Conditional::when(cond, Text::new("content")));
    let c = h.find(first_child(&h)).unwrap();
    assert_eq!(c.children.len(), 1);
}

#[test]
fn conditional_both_children() {
    let cond = Signal::new(true);
    let h = mount(Conditional::new(
        cond.clone(),
        Text::new("a"),
        Text::new("b"),
    ));
    let c = h.find(first_child(&h)).unwrap();
    assert_eq!(c.children.len(), 2);
    // Exactly one child is active
    let a = h.find(c.children[0]).unwrap();
    let b = h.find(c.children[1]).unwrap();
    // cond=true → child[0] active, child[1] inactive
    assert!(!a.slot_inactive.get());
    assert!(b.slot_inactive.get());
}

#[test]
fn nested_vstack_in_hstack() {
    let h = mount(
        VStack::new()
            .push(Text::new("top"))
            .push(HStack::new().push(Text::new("L")).push(Text::new("R"))),
    );
    let vstack = h.find(first_child(&h)).unwrap();
    assert_eq!(vstack.children.len(), 2);
    let hstack = h.find(vstack.children[1]).unwrap();
    assert_eq!(hstack.children.len(), 2);
}
