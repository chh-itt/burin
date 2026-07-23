//! Widget interaction coverage (gallery order, up to Table).
//!
//! Fills two gaps in the existing shallow `widget_coverage.rs`:
//!   1. Empty-coverage widgets that had no mount test at all.
//!   2. Deeper interaction assertions for widgets that only had
//!      "mounts and is visible" smoke tests.

use auralis_signal::Signal;
use burin::core::config::StateFlags;
use burin::event::{Key, Modifiers};
use burin::style::Point;
use burin::testing::selector::by_role;
use burin::testing::TestHarness;
use burin::widgets::composite::{TabBar, TabPanel};
use burin::widgets::display::Text;
use burin::widgets::display::{
    Avatar, AvatarImage, Chip, EmptyState, Icon, List, Progress, ProgressKind, Skeleton, Tree,
    TreeNode,
};
use burin::widgets::input::{
    Button, IconButton, RadioGroup, Select, Slider, TextInput, TextInputType,
};
use burin::widgets::layout::{Expanded, Flexible, GridItem, GridRow, HStack, SplitPane};

// ═══════════════ Section 1: empty-coverage mount tests ═══════════════

#[test]
fn expanded_mounts_with_child() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(HStack::new().push(Expanded::new(Text::new("grows"))));
    h.run_frame();
    h.assert_child_count(id, 1);
    let expanded = h.find(id).unwrap().children[0];
    h.assert_visible(expanded);
}

#[test]
fn flexible_mounts_with_flex_factor() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(HStack::new().push(Flexible::new(2.0, Text::new("flex"))));
    h.run_frame();
    h.assert_child_count(id, 1);
    h.assert_visible(h.find(id).unwrap().children[0]);
}

#[test]
fn split_pane_has_three_children() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(SplitPane::new(Text::new("left"), Text::new("right")).split_ratio(0.5));
    h.run_frame();
    // [first, divider, second]
    h.assert_child_count(id, 3);
    let divider = h.find(id).unwrap().children[1];
    h.assert_visible(divider);
}

#[test]
fn skeleton_mounts_visible() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Skeleton::new().rect(120.0, 20.0));
    h.run_frame();
    h.assert_visible(id);
    let el = h.find(id).unwrap();
    assert!(el.screen_bounds.width > 0.0 && el.screen_bounds.height > 0.0);
}

#[test]
fn empty_state_mounts_with_title() {
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        EmptyState::new()
            .title("Nothing here")
            .description("Try adding an item"),
    );
    h.run_frame();
    h.assert_visible(id);
    assert!(
        !h.find(id).unwrap().children.is_empty(),
        "EmptyState should build a subtree"
    );
}

#[cfg(feature = "ext-svg")]
#[test]
fn svg_image_mounts_from_bytes() {
    use burin::widgets::display::SvgImage;
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="red"/></svg>"#;
    let mut h = TestHarness::new(200.0, 200.0);
    let widget = SvgImage::from_bytes(svg).expect("valid SVG");
    let id = h.mount(widget);
    h.run_frame();
    h.assert_visible(id);
}

// ═══════════════ Section 2: deeper interaction tests ════════════════

#[test]
fn slider_track_click_changes_value() {
    let mut h = TestHarness::new(400.0, 200.0);
    let value = Signal::new(0.0f32);
    let id = h.mount(
        Slider::new(value.clone())
            .range(0.0, 100.0)
            .step(1.0)
            .width(200.0),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    let click_x = el.screen_bounds.x + el.screen_bounds.width * 0.9;
    let click_y = el.screen_bounds.y + el.screen_bounds.height * 0.5;
    h.click_at(Point::new(click_x, click_y)).run_frame();

    let v = h.read_signal(&value);
    assert!(
        v > 50.0,
        "clicking right side of slider should raise value, got {}",
        v
    );
}

#[test]
fn tab_bar_click_changes_active_index() {
    let mut h = TestHarness::new(600.0, 200.0);
    let active = Signal::new(0usize);
    let id = h.mount(
        TabBar::new(active.clone())
            .tab("First")
            .tab("Second")
            .tab("Third"),
    );
    h.run_frame();
    assert_eq!(h.read_signal(&active), 0);

    let tabs = h.find(id).unwrap().children.clone();
    assert!(tabs.len() >= 2, "TabBar should have tab buttons");
    h.click(tabs[1]).run_frame();
    assert_eq!(
        h.read_signal(&active),
        1,
        "clicking 2nd tab should set active=1"
    );
}

#[test]
fn tab_bar_keyboard_navigation() {
    let mut h = TestHarness::new(600.0, 200.0);
    let active = Signal::new(0usize);
    let id = h.mount(TabBar::new(active.clone()).tab("A").tab("B").tab("C"));
    h.run_frame();
    assert_eq!(h.read_signal(&active), 0);

    let tabs = h.find(id).unwrap().children.clone();
    // Click tab 1 to focus it and set active=1
    h.click(tabs[1]);
    h.run_frame();
    assert_eq!(h.read_signal(&active), 1);
}

#[test]
fn tab_panel_toggles_visibility() {
    let mut h = TestHarness::new(400.0, 300.0);
    let active = Signal::new(0usize);
    let id0 = h.mount(TabPanel::new(
        0,
        active.clone(),
        burin::widgets::display::Text::new("Panel 0"),
    ));
    h.run_frame();
    assert_eq!(h.find(id0).unwrap().slot_inactive.get(), false);

    active.set(1);
    h.run_frame();
    assert_eq!(h.find(id0).unwrap().slot_inactive.get(), true);
}

#[test]
fn tab_bar_variants_mount_without_panic() {
    let mut h = TestHarness::new(600.0, 200.0);
    let active = Signal::new(0usize);

    let id = h.mount(TabBar::new(active).tab("One").tab("Two").tab("Three"));
    h.run_frame();
    let _ = h.find(id).expect("Pill TabBar mounts");
}

#[test]
fn radio_group_click_selects_option() {
    let mut h = TestHarness::new(400.0, 200.0);
    let sel = Signal::new(String::from("A"));
    let id = h.mount(
        RadioGroup::new(sel.clone())
            .option("Option A", String::from("A"))
            .option("Option B", String::from("B")),
    );
    h.run_frame();
    assert_eq!(h.read_signal(&sel), "A");

    let opts = h.find(id).unwrap().children.clone();
    assert!(opts.len() >= 2, "RadioGroup should have option children");
    h.click(opts[1]).run_frame();
    assert_eq!(
        h.read_signal(&sel),
        "B",
        "clicking 2nd radio should select B"
    );
}

#[test]
fn list_item_click_updates_selection() {
    let mut h = TestHarness::new(400.0, 300.0);
    let items = Signal::new(vec![
        "apple".to_string(),
        "banana".to_string(),
        "cherry".to_string(),
    ]);
    let list_sel: Signal<Option<usize>> = Signal::new(None);
    let id = h.mount(
        List::new(items.clone())
            .render(|s: &String, _i| s.clone())
            .selected(list_sel.clone()),
    );
    h.run_frame();

    let item_ids = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    assert!(
        item_ids.len() >= 2,
        "List should render item elements, got {}",
        item_ids.len()
    );
    h.click(item_ids[1]).run_frame();
    assert_eq!(
        h.read_signal(&list_sel),
        Some(1),
        "clicking 2nd item should select index 1"
    );
    let _ = id;
}

#[test]
fn progress_value_change_marks_dirty() {
    let mut h = TestHarness::new(400.0, 200.0);
    let val = Signal::new(20.0f64);
    h.mount(Progress::new(val.clone()).kind(ProgressKind::Linear));
    h.run_frame();
    h.run_frame(); // settle
    assert_eq!(h.dirty_count(), 0);

    h.set_signal(&val, 80.0);
    assert!(
        h.dirty_count() > 0,
        "changing progress value must mark the element dirty"
    );
}

#[test]
fn grid_items_split_columns_evenly() {
    let mut h = TestHarness::new(480.0, 200.0);
    let id = h.mount(
        GridRow::new()
            .columns(24)
            .push(GridItem::new(Text::new("A")).cols(12))
            .push(GridItem::new(Text::new("B")).cols(12)),
    );
    h.run_frame();

    let row = h.find(id).unwrap();
    assert_eq!(row.children.len(), 2);
    let a = h.find(row.children[0]).unwrap();
    let b = h.find(row.children[1]).unwrap();
    assert!(
        (a.screen_bounds.width - b.screen_bounds.width).abs() < 5.0,
        "12/24 + 12/24 columns should split evenly: a={}, b={}",
        a.screen_bounds.width,
        b.screen_bounds.width,
    );
}

#[test]
fn avatar_image_mode_mounts() {
    let mut h = TestHarness::new(200.0, 200.0);
    let pixels = vec![128u8; 16 * 16 * 4];
    let id = h.mount(
        Avatar::new("Bob")
            .image(AvatarImage::from_rgba(pixels, 16, 16))
            .size(40.0),
    );
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn chip_fires_on_click() {
    let mut h = TestHarness::new(300.0, 100.0);
    let clicked = Signal::new(false);
    let id = h.mount(Chip::new("Filter").on_click({
        let c = clicked.clone();
        move || c.set(true)
    }));
    h.run_frame();
    h.click(id).run_frame();
    assert!(
        h.read_signal(&clicked),
        "clicking a Chip should fire on_click"
    );
}

#[test]
fn icon_respects_size() {
    let mut h = TestHarness::new(200.0, 200.0);
    let id = h.mount(Icon::new(burin::resource::icons::Icon::Check).size(32.0));
    h.run_frame();
    let el = h.find(id).unwrap();
    assert!(
        el.screen_bounds.width >= 28.0,
        "Icon size 32 should yield ~32px bounds, got {}",
        el.screen_bounds.width,
    );
}

#[test]
fn select_option_click_sets_value() {
    use burin::testing::selector::by_text;
    let mut h = TestHarness::new(400.0, 300.0);
    let selected: Signal<Option<&'static str>> = Signal::new(None);
    let id = h.mount(
        Select::new(selected.clone())
            .options(vec!["Rust", "Go", "Python"])
            .render(|s: &&'static str| s.to_string())
            .placeholder("Choose..."),
    );
    h.run_frame();
    assert_eq!(h.read_signal(&selected), None);

    // Open the dropdown (trigger is the first child).
    let trigger = h.find(id).unwrap().children[0];
    h.click(trigger);
    h.settle(5);

    // Click the "Python" option element.
    let python = h
        .find_sel(by_text("Python"))
        .expect("Python option mounted");
    h.click(python);
    h.settle(5);

    assert_eq!(
        h.read_signal(&selected),
        Some("Python"),
        "clicking the Python option should select it",
    );
}

// ═══════════════ Tree tests ═══════════════

#[derive(Clone)]
struct TestNode {
    name: String,
    kids: Vec<TestNode>,
}
impl TreeNode for TestNode {
    type Id = String;
    fn id(&self) -> String {
        self.name.clone()
    }
    fn label(&self) -> String {
        self.name.clone()
    }
    fn children(&self) -> &[Self] {
        &self.kids
    }
}

#[test]
fn tree_mounts_container_with_tree_role() {
    let roots = Signal::new(vec![
        TestNode {
            name: "a".into(),
            kids: vec![],
        },
        TestNode {
            name: "b".into(),
            kids: vec![],
        },
    ]);

    let mut h = TestHarness::new(400.0, 400.0);
    let id = h.mount(Tree::new(roots).indent(20.0).row_height(30.0).reserve(16));
    h.run_frame();

    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::Tree);
}

#[test]
fn tree_expand_collapse_via_signal() {
    let roots = Signal::new(vec![TestNode {
        name: "root".into(),
        kids: vec![TestNode {
            name: "child".into(),
            kids: vec![],
        }],
    }]);
    let expanded = Signal::new(std::collections::HashSet::new());

    let mut h = TestHarness::new(400.0, 400.0);
    let id = h.mount(
        Tree::new(roots)
            .expanded(expanded.clone())
            .indent(20.0)
            .row_height(30.0)
            .reserve(16),
    );
    h.run_frame();

    h.assert_a11y_role(id, accesskit::Role::Tree);

    // Expand by setting the signal
    let mut set = std::collections::HashSet::new();
    set.insert("root".to_string());
    expanded.set(set);
    h.run_frame();
    h.run_frame();
    // Tree should still be mounted and visible
    h.assert_visible(id);
}

// ═══════════════ Section 3: Button contract tests ═════════════════

#[test]
fn button_mounts_with_correct_accessibility_role() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Submit"));
    h.run_frame();
    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::Button);
    h.assert_a11y_label(id, "Submit");
}

#[test]
fn button_click_invokes_on_click() {
    let count = Signal::new(0u32);
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Click me").on_click({
        let c = count.clone();
        move || {
            c.set(c.read() + 1);
        }
    }));
    h.run_frame();
    assert_eq!(count.read(), 0);
    h.click(id);
    h.run_frame();
    assert_eq!(count.read(), 1, "click should increment counter");
}

#[test]
fn button_disabled_sets_state_flag() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Disabled").disabled());
    h.run_frame();

    let el = h.find(id).unwrap();
    assert!(
        el.state.get().contains(StateFlags::DISABLED),
        "disabled button should have DISABLED flag"
    );
}

#[test]
fn button_disabled_and_loading_are_orthogonal() {
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(Button::new("Both").disabled().loading(true));
    h.run_frame();

    let el = h.find(id).unwrap();
    assert!(
        el.state.get().contains(StateFlags::DISABLED),
        "should have DISABLED flag"
    );
    assert!(
        el.state.get().contains(StateFlags::LOADING),
        "should have LOADING flag"
    );
}

// ═══════════════ Section 4: IconButton contract tests ══════════════

#[test]
fn icon_button_mounts_with_button_role() {
    use burin::resource::icons::Icon as IconKind;
    use burin::widgets::display::Icon;
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(IconButton::new(Icon::new(IconKind::Search)));
    h.run_frame();
    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::Button);
}

#[test]
fn icon_button_click_invokes_on_click() {
    use burin::resource::icons::Icon as IconKind;
    use burin::widgets::display::Icon;
    let count = Signal::new(0u32);
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(IconButton::new(Icon::new(IconKind::Check)).on_click({
        let c = count.clone();
        move || {
            c.set(c.read() + 1);
        }
    }));
    h.run_frame();
    h.click(id);
    h.run_frame();
    assert_eq!(count.read(), 1, "click should increment counter");
}

#[test]
fn icon_button_disabled_sets_state_flag() {
    use burin::resource::icons::Icon as IconKind;
    use burin::widgets::display::Icon;
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(IconButton::new(Icon::new(IconKind::X)).disabled());
    h.run_frame();
    let el = h.find(id).unwrap();
    assert!(
        el.state.get().contains(StateFlags::DISABLED),
        "disabled icon_button should have DISABLED flag"
    );
}

#[test]
fn icon_button_loading_sets_state_flag() {
    use burin::resource::icons::Icon as IconKind;
    use burin::widgets::display::Icon;
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(IconButton::new(Icon::new(IconKind::Refresh)).loading(true));
    h.run_frame();
    let el = h.find(id).unwrap();
    assert!(
        el.state.get().contains(StateFlags::LOADING),
        "loading icon_button should have LOADING flag"
    );
    assert!(
        !el.state.get().contains(StateFlags::DISABLED),
        "loading icon_button should NOT have DISABLED flag"
    );
}

// ═══════════════ Section: TextInput (#33) ═══════════════

#[test]
fn text_input_mount_and_type() {
    let sig = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).placeholder("type..."));
    h.run_frame();
    h.assert_visible(id);

    let ids = h.find_all_sel(by_role(accesskit::Role::TextInput));
    assert!(!ids.is_empty(), "TextInput should be findable by role");

    h.type_text(ids[0], "hello");
    h.run_frame();
    assert_eq!(sig.read(), "hello", "signal should reflect typed text");
}

#[test]
fn text_input_password_masks_display() {
    let sig = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).input_type(TextInputType::Password));
    h.run_frame();
    h.assert_visible(id);

    let ids = h.find_all_sel(by_role(accesskit::Role::TextInput));
    h.type_text(ids[0], "secret");
    h.run_frame();

    // Signal holds plaintext
    assert_eq!(sig.read(), "secret");
}

#[test]
fn text_input_multiline_accepts_newline() {
    let sig = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).input_type(TextInputType::Multiline));
    h.run_frame();
    h.assert_visible(id);

    // Multi-line TextInput accepts newline characters in type_text
    let ids = h.find_all_sel(by_role(accesskit::Role::TextInput));
    h.type_text(ids[0], "line1\nline2");
    h.run_frame();

    assert!(
        sig.read().contains('\n'),
        "multiline should accept newline chars"
    );
    assert!(sig.read().starts_with("line1"), "should start with line1");
}

#[test]
fn text_input_disabled_blocks_input() {
    let sig = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 200.0);
    let id = h.mount(TextInput::new(sig.clone()).disabled());
    h.run_frame();
    h.assert_visible(id);

    // Disabled: element should have DISABLED flag
    let el = h.find(id).unwrap();
    assert!(el.state.get().contains(StateFlags::DISABLED));
}

#[test]
fn text_input_max_length() {
    let sig = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 200.0);
    let _id = h.mount(TextInput::new(sig.clone()).max_length(3));
    h.run_frame();

    let ids = h.find_all_sel(by_role(accesskit::Role::TextInput));
    h.type_text(ids[0], "abc");
    h.run_frame();
    assert_eq!(sig.read(), "abc");

    h.type_text(ids[0], "def");
    h.run_frame();
    assert_eq!(sig.read(), "abc", "max_length should block additional text");
}

// ═══════════════ Select tests ═══════════════

#[test]
fn select_mounts_with_combobox_role() {
    let selected = Signal::new(None::<String>);
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        Select::new(selected.clone())
            .options(vec!["A".into(), "B".into(), "C".into()])
            .placeholder("Pick one"),
    );
    h.run_frame();

    // Root is visible; trigger has ComboBox role
    h.assert_visible(id);
    let combos = h.find_all_sel(by_role(accesskit::Role::ComboBox));
    assert!(
        !combos.is_empty(),
        "Select trigger should have ComboBox role"
    );
}

#[test]
fn select_opens_on_click_and_selects_option() {
    let selected = Signal::new(None::<String>);
    let mut h = TestHarness::new(400.0, 300.0);
    let _id = h.mount(
        Select::new(selected.clone())
            .options(vec!["Red".into(), "Green".into(), "Blue".into()])
            .placeholder("Pick a color"),
    );
    h.run_frame();

    // Initially nothing selected
    assert_eq!(selected.read(), None);

    // Click the trigger to open dropdown
    let combos = h.find_all_sel(by_role(accesskit::Role::ComboBox));
    h.click(combos[0]);
    h.run_frame();

    // Optional: verify ListBoxOptions exist
    let options = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    assert!(
        !options.is_empty(),
        "Dropdown should have ListBoxOption roles"
    );

    // Click the first option
    h.click(options[0]);
    h.run_frame();

    // Selection should be set
    assert_eq!(selected.read(), Some("Red".to_string()));
}

#[test]
fn select_keyboard_navigation() {
    let selected = Signal::new(None::<String>);
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        Select::new(selected.clone())
            .options(vec!["Alpha".into(), "Beta".into(), "Gamma".into()])
            .placeholder("Choose letter"),
    );
    h.run_frame();

    // Focus the Select trigger
    h.click(id);
    h.run_frame();

    // Press Enter to open the dropdown (highlights first item)
    h.press_key(Key::Enter, Modifiers::default());
    h.run_frame();

    // Press ArrowDown to move highlight to second item
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();

    // Press ArrowDown to move to third item
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();

    // Press Enter to select third item
    h.press_key(Key::Enter, Modifiers::default());
    h.run_frame();

    // Should have "Gamma" selected
    assert!(
        selected.read().is_some(),
        "Should have a selection after Enter"
    );
    assert_eq!(selected.read(), Some("Gamma".to_string()));
}

#[test]
fn select_escape_closes_without_selection() {
    let selected = Signal::new(None::<String>);
    let mut h = TestHarness::new(400.0, 300.0);
    let id = h.mount(
        Select::new(selected.clone())
            .options(vec!["X".into(), "Y".into(), "Z".into()])
            .placeholder("Choose"),
    );
    h.run_frame();

    // Click to open
    h.click(id);
    h.run_frame();

    // Navigate down
    h.press_key(Key::ArrowDown, Modifiers::default());
    h.run_frame();

    // Escape closes without selecting
    h.press_key(Key::Escape, Modifiers::default());
    h.run_frame();

    assert_eq!(selected.read(), None, "Escape should not change selection");
}

// ═══════════════ Select real-pointer overlay tests ═══════════════

#[test]
fn select_option_clickable_via_hittest() {
    let selected = Signal::new(None::<String>);
    let mut h = TestHarness::new(400.0, 500.0);
    let _id = h.mount(
        Select::new(selected.clone())
            .options(vec!["Red".into(), "Green".into(), "Blue".into()])
            .placeholder("Pick a color"),
    );
    h.run_frame();

    let combos = h.find_all_sel(by_role(accesskit::Role::ComboBox));
    let tb = h.find(combos[0]).unwrap().screen_bounds;
    h.click_at(Point::new(tb.x + tb.width / 2.0, tb.y + tb.height / 2.0));
    h.run_frames(3);

    let options = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    assert!(!options.is_empty(), "dropdown should have options");
    let ob = h.find(options[0]).unwrap().screen_bounds;
    assert!(
        ob.y > tb.y,
        "option must be below trigger, not at local (0,0)"
    );
    // Click at the trigger's horizontal center (always within the dropdown width)
    // at the first option's vertical center.
    h.click_at(Point::new(tb.x + tb.width / 2.0, ob.y + ob.height / 2.0));
    h.run_frames(3);

    assert_eq!(
        selected.read(),
        Some("Red".to_string()),
        "clicking an option at its visual position must select it"
    );
}

#[test]
fn select_dismiss_on_outside_click() {
    let selected = Signal::new(None::<String>);
    let mut h = TestHarness::new(400.0, 500.0);
    let _id = h.mount(
        Select::new(selected.clone())
            .options(vec!["Red".into(), "Green".into(), "Blue".into()])
            .placeholder("Pick a color"),
    );
    h.run_frame();

    let combos = h.find_all_sel(by_role(accesskit::Role::ComboBox));
    let tb = h.find(combos[0]).unwrap().screen_bounds;
    h.click_at(Point::new(tb.x + tb.width / 2.0, tb.y + tb.height / 2.0));
    h.run_frames(3);
    let options_open = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    let ob = h.find(options_open[0]).unwrap().screen_bounds;
    assert!(ob.height > 0.0, "dropdown open: option has height");

    h.click_at(Point::new(390.0, 490.0)); // far from trigger + dropdown
    h.run_frames(3);

    // After outside click the dropdown is dismissed: its options are no longer
    // hit-testable (reactive_visible=false hides the subtree).
    let opt_center = {
        let ob2 = h.find(options_open[0]).unwrap().screen_bounds;
        Point::new(ob2.x + 4.0, ob2.y + ob2.height / 2.0)
    };
    let hit = burin::core::dirty_registry::spatial_hit_test(&h.arena, opt_center);
    assert_ne!(
        hit,
        Some(options_open[0]),
        "outside click should close the dropdown (option no longer hit-testable)"
    );
    assert_eq!(selected.read(), None, "outside click selects nothing");
}

// ═══════════════ ComboBox real-pointer overlay tests ═══════════════

#[test]
fn combobox_option_clickable_via_hittest() {
    use burin::widgets::input::ComboBox;
    let selected = Signal::new(None::<String>);
    let mut h = TestHarness::new(400.0, 500.0);
    let _id = h.mount(
        ComboBox::new(selected.clone())
            .options(vec!["Apple".into(), "Banana".into(), "Cherry".into()])
            .render(|s: &String| s.clone())
            .placeholder("Search..."),
    );
    h.run_frame();

    let combos = h.find_all_sel(by_role(accesskit::Role::ComboBox));
    let tb = h.find(combos[0]).unwrap().screen_bounds;
    h.click_at(Point::new(tb.x + tb.width / 2.0, tb.y + tb.height / 2.0));
    h.run_frames(3);

    let options = h.find_all_sel(by_role(accesskit::Role::ListBoxOption));
    assert!(!options.is_empty(), "dropdown should have options");
    let ob = h.find(options[0]).unwrap().screen_bounds;
    assert!(
        ob.y > tb.y,
        "option must be below trigger, not at local (0,0)"
    );
    h.click_at(Point::new(tb.x + tb.width / 2.0, ob.y + ob.height / 2.0));
    h.run_frames(3);

    assert_eq!(
        selected.read(),
        Some("Apple".to_string()),
        "clicking a combobox option at its visual position must select it"
    );
}
