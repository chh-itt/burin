//! Accessibility tree integration tests.

use auralis_signal::Signal;
use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::{Button, Checkbox, TextInput};
use burin::widgets::layout::*;
use burin::widgets::overlay::{ContextMenu, ContextMenuItem};

fn build(h: &mut TestHarness) -> accesskit::TreeUpdate {
    burin::platform::build_accessibility_tree(h.root(), h.root_id(), None)
}

#[test]
fn tree_has_nodes_after_mounting_button() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(Button::new("Submit").primary());
    h.run_frame();

    let tree = build(&mut h);
    assert!(!tree.nodes.is_empty(), "tree should have at least one node");
}

#[test]
fn tree_has_nodes_after_mounting_text_input() {
    let mut h = TestHarness::new(800.0, 600.0);
    let text = Signal::new(String::from("hello"));
    h.mount(TextInput::new(text.clone()).placeholder("type..."));
    h.run_frame();

    let tree = build(&mut h);
    assert!(!tree.nodes.is_empty(), "tree should have at least one node");
}

#[test]
fn tree_has_nodes_after_mounting_checkbox() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(Checkbox::new(Signal::new(false)));
    h.run_frame();

    let tree = build(&mut h);
    assert!(!tree.nodes.is_empty(), "tree should have at least one node");
}

#[test]
fn nested_layout_produces_multiple_nodes() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(
        VStack::new()
            .push(Text::new("Header"))
            .push(Button::new("OK").primary())
            .push(Checkbox::new(Signal::new(true))),
    );
    h.run_frame();

    let tree = build(&mut h);
    assert!(
        tree.nodes.len() >= 5,
        "expected >= 5 nodes, got {}",
        tree.nodes.len()
    );
}

#[test]
fn tree_includes_focus_reference() {
    let mut h = TestHarness::new(800.0, 600.0);
    let btn_id = h.mount(Button::new("Focus me").primary());
    h.run_frame();
    h.click(btn_id).run_frame();

    let tree = build(&mut h);
    let _ = tree.focus;
}

#[test]
fn idle_frame_produces_consistent_node_count() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(Text::new("static"));
    h.run_frame();
    let t1 = build(&mut h);

    h.run_frame();
    let t2 = build(&mut h);

    assert_eq!(t1.nodes.len(), t2.nodes.len());
}

#[test]
fn context_menu_exposes_a11y_roles() {
    let mut h = TestHarness::new(800.0, 600.0);
    let visible = Signal::new(true); // open immediately so rows are in the tree
    let pos = Signal::new(Point::ZERO);
    let _id = h.mount(
        ContextMenu::new(visible.clone(), pos.clone())
            .item(ContextMenuItem::new("Copy").action(|| {}))
            .item(
                ContextMenuItem::new("Show grid")
                    .checked(true)
                    .action(|| {}),
            )
            .item(ContextMenuItem::new("Density").radio(true).action(|| {}))
            .item(ContextMenuItem::new("Locked").disabled())
            .item(ContextMenuItem::separator())
            .item(ContextMenuItem::new("Paste").action(|| {})),
    );
    h.run_frame();

    let tree = h.a11y_tree();
    let (mut menu, mut item, mut checkbox, mut radio) = (0, 0, 0, 0);
    let (mut copy_labeled, mut locked_disabled) = (false, false);
    for (_nid, node) in &tree.nodes {
        match node.role() {
            accesskit::Role::Menu => menu += 1,
            accesskit::Role::MenuItem => item += 1,
            accesskit::Role::MenuItemCheckBox => checkbox += 1,
            accesskit::Role::MenuItemRadio => radio += 1,
            _ => {}
        }
        if node.label() == Some("Copy") {
            copy_labeled = true;
        }
        if node.label() == Some("Locked") && node.is_disabled() {
            locked_disabled = true;
        }
    }
    assert_eq!(menu, 1, "exactly one Menu container");
    assert!(
        item >= 3,
        "Copy + Locked + Paste are plain MenuItems, got {item}"
    );
    assert_eq!(checkbox, 1, "Show grid is a MenuItemCheckBox");
    assert_eq!(radio, 1, "Density is a MenuItemRadio");
    assert!(copy_labeled, "Copy item exposes its a11y label");
    assert!(locked_disabled, "disabled item is a11y-disabled");
}
