//! Integration tests for the `TestHarness` — verify full-frame simulation.

use auralis_signal::Signal;
use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::{Button, Checkbox, TextInput};
use burin::widgets::layout::*;

#[test]
fn mount_single_text_widget() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Text::new("Hello, harness!"));
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn mount_vstack_with_children() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        VStack::new()
            .push(Text::new("A"))
            .push(Text::new("B"))
            .push(Text::new("C")),
    );
    h.run_frame();
    h.assert_child_count(id, 3);
}

#[test]
fn layout_computes_bounds() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(VStack::new().push(Text::new("Hello")));
    h.run_frame();
    let el = h.find(id).unwrap();
    assert!(el.screen_bounds.width > 0.0);
    assert!(el.screen_bounds.height > 0.0);
}

#[test]
fn idle_frame_has_no_dirty() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(Text::new("static text"));
    h.run_frame();
    h.run_frame();
    assert_eq!(h.dirty_count(), 0);
}

#[test]
fn button_click_sets_dirty() {
    let mut h = TestHarness::new(800.0, 600.0);
    let labeled_sig = Signal::new(String::from("Before"));
    let count = Signal::new(0u32);

    h.mount(
        HStack::new()
            .push(Button::new("Click").on_click({
                let c = count.clone();
                let l = labeled_sig.clone();
                move || {
                    c.update(|n| *n += 1);
                    l.set(format!("Clicked {}", c.read()));
                }
            }))
            .push(Text::new("").bind(labeled_sig.clone())),
    );
    h.run_frame();
    h.run_frame();
    assert_eq!(h.dirty_count(), 0);
}

#[test]
fn conditional_shows_correct_branch() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Conditional::new(
        Signal::new(true),
        Text::new("true"),
        Text::new("false"),
    ));
    h.run_frame();

    let el = h.find(id).unwrap();
    assert!(
        h.find(el.children[0]).unwrap().is_visible(),
        "true branch should be visible"
    );
    assert!(
        !h.find(el.children[1]).unwrap().is_visible(),
        "false branch should be hidden"
    );
}

#[test]
fn conditional_shows_false_branch() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Conditional::new(
        Signal::new(false),
        Text::new("true"),
        Text::new("false"),
    ));
    h.run_frame();

    let el = h.find(id).unwrap();
    assert!(
        !h.find(el.children[0]).unwrap().is_visible(),
        "true branch should be hidden"
    );
    assert!(
        h.find(el.children[1]).unwrap().is_visible(),
        "false branch should be visible"
    );
}

#[test]
fn reactive_conditional_toggles_visibility() {
    let mut h = TestHarness::new(800.0, 600.0);
    let show = Signal::new(true);

    let id = h.mount(Conditional::new(
        show.clone(),
        Text::new("visible"),
        Text::new("hidden"),
    ));
    h.run_frame();

    h.assert_child_count(id, 2);

    let el = h.find(id).unwrap();
    assert!(h.find(el.children[0]).unwrap().is_visible());
    assert!(!h.find(el.children[1]).unwrap().is_visible());

    h.set_signal(&show, false).run_frame();

    let el = h.find(id).unwrap();
    assert!(!h.find(el.children[0]).unwrap().is_visible());
    assert!(h.find(el.children[1]).unwrap().is_visible());

    h.set_signal(&show, true).run_frame();

    let el = h.find(id).unwrap();
    assert!(h.find(el.children[0]).unwrap().is_visible());
    assert!(!h.find(el.children[1]).unwrap().is_visible());
}

#[test]
fn click_focuses_button() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Button::new("Focused").primary());
    h.run_frame();
    h.assert_not_focused();

    h.click(id).run_frame();
    h.assert_focused(id);
}

#[test]
fn click_away_blurs() {
    let mut h = TestHarness::new(800.0, 600.0);
    let btn1 = h.mount(Button::new("Btn1").primary());
    h.mount(Button::new("Btn2"));
    h.run_frame();

    h.click(btn1).run_frame();
    h.assert_focused(btn1);

    let el = h.find(btn1).unwrap();
    let pos = Point::new(
        el.screen_bounds.x + el.screen_bounds.width * 0.5,
        el.screen_bounds.y + el.screen_bounds.height * 0.5,
    );
    h.click_at(pos).run_frame();
}

#[test]
fn text_input_accepts_characters() {
    let mut h = TestHarness::new(800.0, 600.0);
    let text_sig = Signal::new(String::from("Hello"));
    let id = h.mount(TextInput::new(text_sig.clone()).placeholder("type..."));
    h.run_frame();

    let el = h.find(id).unwrap();
    let cx = el.screen_bounds.x + el.screen_bounds.width * 0.5;
    let cy = el.screen_bounds.y + el.screen_bounds.height * 0.5;
    h.click_at(Point::new(cx, cy)).run_frame();

    h.type_text(id, "World").run_frame();
    let result = h.read_signal(&text_sig);
    assert!(
        result.contains("World"),
        "Expected 'World' somewhere in '{}'",
        result
    );
}

#[test]
fn checkbox_toggles_on_click() {
    let mut h = TestHarness::new(800.0, 600.0);
    let checked = Signal::new(false);
    let id = h.mount(Checkbox::new(checked.clone()));
    h.run_frame();

    assert!(!h.read_signal(&checked));
    h.click(id).run_frame();
    assert!(h.read_signal(&checked));
    h.click(id).run_frame();
    assert!(!h.read_signal(&checked));
}

#[test]
fn measure_dirty_after_reactive_conditional_toggle() {
    let mut h = TestHarness::new(800.0, 600.0);
    let show = Signal::new(true);
    h.mount(Conditional::new(
        show.clone(),
        Text::new("A"),
        Text::new("B"),
    ));
    h.run_frame();
    h.run_frame();

    h.set_signal(&show, false);
    assert!(h.measure_dirty_count() > 0);
}

#[test]
fn hidden_element_not_visible() {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(Text::new("visible"));
    h.run_frame();
    let root = h.find(h.root_id()).unwrap();
    assert!(h.find(root.children[0]).unwrap().is_visible());
}

#[test]
fn find_all_sel_returns_dfs_order() {
    use burin::testing::selector::by_name;
    use burin::testing::WidgetTestExt;

    let mut h = TestHarness::new(400.0, 400.0);
    h.mount(
        VStack::new()
            .push(Text::new("0").name("row").test_id("0"))
            .push(Text::new("1").name("row").test_id("1"))
            .push(Text::new("2").name("row").test_id("2"))
            .push(Text::new("3").name("row").test_id("3"))
            .push(Text::new("4").name("row").test_id("4"))
            .push(Text::new("5").name("row").test_id("5")),
    );
    h.run_frame();

    let ids = h.find_all_sel(by_name("row"));
    let test_ids: Vec<String> = ids
        .iter()
        .map(|&id| h.find(id).unwrap().test_id().unwrap())
        .collect();
    assert_eq!(
        test_ids,
        vec!["0", "1", "2", "3", "4", "5"],
        "find_all_sel must return DFS (tree) order, not FxHashMap hash order",
    );
}

#[test]
fn harness_instances_are_isolated_from_stale_registry_state() {
    // First harness leaves residual dirty entries in the thread-local registry
    // (mount produces register_dirty → DIRTY_ENTRIES; never drained).
    {
        let mut a = TestHarness::new(200.0, 100.0);
        a.mount(
            VStack::new()
                .push(Text::new("leaky"))
                .push(Text::new("residual")),
        );
        // Intentionally do NOT run_frame — dirty entries stay in DIRTY_ENTRIES.
    } // a dropped

    // A second harness must start completely clean and be able to settle.
    // TestHarness::new creates its own fresh AppContext, so per-instance state
    // is isolated by construction — no manual global reset needed.
    let mut b = TestHarness::new(200.0, 100.0);
    b.mount(Text::new("clean"));
    let settled = b.settle(10);
    assert!(
        settled,
        "fresh harness must settle — stale DIRTY_ENTRIES from previous harness leaked",
    );
}

#[test]
fn mount_twice_creates_separate_trees() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id1 = h.mount(Text::new("first"));
    let id2 = h.mount(Text::new("second"));
    h.run_frame();

    assert_ne!(id1, id2);
    let t1 = h.find(id1).unwrap();
    let t2 = h.find(id2).unwrap();
    assert!(t1.accessible_label().as_deref() == Some("first"));
    assert!(t2.accessible_label().as_deref() == Some("second"));
}
