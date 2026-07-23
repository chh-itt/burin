//! Comprehensive widget coverage via TestHarness.
//!
//! Every built-in widget gets at least one test covering:
//!   - mount → layout → assert structure
//!   - interaction (if applicable) → assert state change
//!   - signal reactivity (if applicable) → assert value propagation

use auralis_signal::Signal;
use burin::style::{Color, Point};
use burin::testing::TestHarness;
use burin::widgets::composite::*;
use burin::widgets::decoration::*;
use burin::widgets::display::*;
use burin::widgets::input::*;
use burin::widgets::layout::*;
use burin::widgets::overlay::{clear_queue, queue_len, show, show_duration, ToastKind};
use burin::widgets::overlay::{ContextMenu, ContextMenuItem, Modal, ToastContainer};

// ── Input Widgets: mount + signal → interaction → assert ───────────

#[test]
fn switch_toggles_on_click() {
    let mut h = TestHarness::new(800.0, 600.0);
    let checked = Signal::new(false);
    let id = h.mount(Switch::new(checked.clone()));
    h.run_frame();

    assert!(!h.read_signal(&checked));
    h.click(id).run_frame();
    assert!(h.read_signal(&checked));
    h.click(id).run_frame();
    assert!(!h.read_signal(&checked));
}

#[test]
fn slider_mounts_and_is_visible() {
    let mut h = TestHarness::new(800.0, 600.0);
    let val = Signal::new(0.5f32);
    let id = h.mount(Slider::new(val.clone()).range(0.0, 1.0).step(0.1));
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn radio_group_selects_option() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sel = Signal::new(String::from("A"));
    let id = h.mount(
        RadioGroup::new(sel.clone())
            .option("Option A", String::from("A"))
            .option("Option B", String::from("B")),
    );
    h.run_frame();
    h.assert_child_count(id, 2);
    assert_eq!(h.read_signal(&sel), String::from("A"));
}

#[test]
fn select_has_options() {
    let mut h = TestHarness::new(800.0, 600.0);
    let sel = Signal::<Option<&'static str>>::new(None);
    let id = h.mount(
        Select::new(sel.clone())
            .options(vec!["Rust", "Go", "Python"])
            .placeholder("Choose..."),
    );
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn tab_bar_has_tabs() {
    let mut h = TestHarness::new(800.0, 600.0);
    let active = Signal::new(0usize);
    let id = h.mount(
        TabBar::new(active.clone())
            .tab("First")
            .tab("Second")
            .tab("Third"),
    );
    h.run_frame();
    h.assert_child_count(id, 3);
}

// ── Display Widgets: mount → assert structure ─────────────────────

#[test]
fn progress_mounts_and_is_visible() {
    let mut h = TestHarness::new(800.0, 600.0);
    let val = Signal::new(0.42f64);
    let id = h.mount(Progress::new(val.clone()).kind(ProgressKind::Linear));
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn avatar_mounts() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Avatar::new("Alice"));
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn badge_has_label() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Badge::new("New").color(Color::rgba8(220, 38, 38, 255)));
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn chip_has_label() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Chip::new("Filter"));
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn icon_renders_glyph() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Icon::new(burin::resource::icons::Icon::Check).size(24.0));
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn image_data_exists() {
    // Image::from_bytes requires `ext-image` feature.
    // ImageData can be constructed directly for testing.
    use std::rc::Rc;
    let pixels: Rc<Vec<u8>> = Rc::new(vec![0u8; 32 * 32 * 4]);
    let _img = ImageData {
        hash: 42,
        width: 32,
        height: 32,
        pixels,
        fit: Default::default(),
    };
}

// ── Overlay Widgets: signal → visibility ───────────────────────────

#[test]
fn modal_toggles_visibility_by_signal() {
    let mut h = TestHarness::new(800.0, 600.0);
    let visible = Signal::new(true);

    h.mount(Modal::new(visible.clone(), Text::new("inside modal")));
    h.run_frame();
    // Modal registers as portal; backdrop is visible, find portal children
    let root = h.find(h.root_id()).unwrap();
    assert!(root.children.len() >= 1, "Root should have children");
    let portal_id = root.children[root.children.len() - 1];
    h.assert_visible(portal_id);

    h.set_signal(&visible, false).run_frame();
    h.assert_not_visible(portal_id);

    h.set_signal(&visible, true).run_frame();
    h.assert_visible(portal_id);
}

#[test]
fn toast_container_mounts() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(ToastContainer::new());
    h.run_frame();
    // ToastContainer starts hidden (no pending toasts)
    h.assert_not_visible(id);
}

#[test]
fn toast_show_and_dismiss_by_time() {
    let mut h = TestHarness::new(800.0, 600.0);
    let _id = h.mount(ToastContainer::new());
    h.run_frame();

    // Show a toast with short duration
    show_duration("Test message", ToastKind::Info, 100);
    h.run_frames(60); // ~1s at 60fps

    // Fast-forward past duration + exit
    h.advance_time(500);
    h.run_frames(20);
}

#[test]
fn toast_queue_basics() {
    let mut h = TestHarness::new(800.0, 600.0);
    let _id = h.mount(ToastContainer::new());
    h.run_frame();

    show("One", ToastKind::Info);
    show("Two", ToastKind::Success);
    assert_eq!(queue_len(), 2);

    clear_queue();
    assert_eq!(queue_len(), 0);
}

#[test]
fn context_menu_starts_hidden() {
    let mut h = TestHarness::new(800.0, 600.0);
    let visible = Signal::new(false);
    let pos = Signal::new(Point::ZERO);

    let id = h.mount(
        ContextMenu::new(visible.clone(), pos.clone())
            .item(ContextMenuItem::new("Copy").action(|| {}))
            .item(ContextMenuItem::new("Paste")),
    );
    h.run_frame();
    h.assert_not_visible(id);

    h.set_signal(&visible, true).run_frame();
    h.assert_visible(id);
}

// ── Layout Containers: mount → assert children ────────────────────

#[test]
fn zstack_layers_children() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        ZStack::new()
            .push(Text::new("back"))
            .push(Text::new("front")),
    );
    h.run_frame();
    h.assert_child_count(id, 2);
}

#[test]
fn center_contains_child() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Center::new(Text::new("centered")));
    h.run_frame();
    h.assert_visible(id);
    h.assert_child_count(id, 1);
}

#[test]
fn opacity_passes_through_child() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Opacity::new(0.5, Text::new("faded")));
    h.run_frame();
    assert!(h.find(id).unwrap().opacity() < 1.0);
    h.assert_child_count(id, 1);
}

#[test]
fn sized_box_has_child() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        SizedBox::new()
            .width(200.0)
            .height(100.0)
            .child(Text::new("inside")),
    );
    h.run_frame();
    h.assert_child_count(id, 1);
}

#[test]
fn padding_container_contains_child() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Padding::new(
        burin::style::Padding::all(16.0),
        Text::new("padded"),
    ));
    h.run_frame();
    let el = h.find(id).unwrap();
    let p = el.padding();
    assert!(p.top > 0.0 || p.left > 0.0);
}

#[test]
fn spacer_mounts_without_error() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Spacer::new());
    h.run_frame();
    h.assert_visible(id);
}

#[test]
fn mouse_region_fires_click() {
    let mut h = TestHarness::new(800.0, 600.0);
    let clicked = Signal::new(false);

    let id = h.mount(MouseRegion::new(Text::new("clickable")).on_click({
        let c = clicked.clone();
        move || c.set(true)
    }));
    h.run_frame();
    h.click(id).run_frame();
    assert!(h.read_signal(&clicked));
}

// ── Grid ────────────────────────────────────────────────────────────

#[test]
fn grid_row_has_items() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        GridRow::new()
            .push(GridItem::new(Text::new("A")).cols(12))
            .push(GridItem::new(Text::new("B")).cols(12)),
    );
    h.run_frame();
    h.assert_child_count(id, 2);
}

// ── List ───────────────────────────────────────────────────────────

#[test]
fn list_renders_items() {
    let mut h = TestHarness::new(800.0, 600.0);
    let items = Signal::new(vec!["apple".to_string(), "banana".to_string()]);
    let id = h.mount(List::new(items.clone()).render(|s: &String, _i| s.clone()));
    h.run_frame();
    assert!(h.find(id).unwrap().children.len() >= 1);
}

// ── Nested Layout ──────────────────────────────────────────────────

#[test]
fn deep_nesting_maintains_structure() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(
        VStack::new()
            .push(HStack::new().push(VStack::new().push(Text::new("deep"))))
            .push(Button::new("Btn").primary()),
    );
    h.run_frame();
    h.assert_child_count(id, 2);
    let hstack_id = h.find(id).unwrap().children[0];
    h.assert_child_count(hstack_id, 1);
}

#[test]
fn full_form_layout() {
    let mut h = TestHarness::new(800.0, 600.0);
    let name = Signal::new(String::new());
    let agreed = Signal::new(false);
    let volume = Signal::new(0.5f32);

    let id = h.mount(
        VStack::new()
            .push(Text::new("Form").font_size(16.0))
            .push(TextInput::new(name.clone()).placeholder("Your name"))
            .push(Switch::new(Signal::new(true)))
            .push(Slider::new(volume.clone()).range(0.0, 1.0))
            .push(Checkbox::new(agreed.clone()))
            .push(Button::new("Submit").primary()),
    );
    h.run_frame();
    h.assert_child_count(id, 6);
}

// ── Focus Navigation ───────────────────────────────────────────────

#[test]
fn click_transfers_focus_between_buttons() {
    let mut h = TestHarness::new(800.0, 600.0);
    let b1 = h.mount(Button::new("B1").primary());
    let b2 = h.mount(Button::new("B2").secondary());
    h.run_frame();

    h.click(b1).run_frame();
    h.assert_focused(b1);

    h.click(b2).run_frame();
    h.assert_focused(b2);
}

// ── Dirty Flag: Idle Frame ─────────────────────────────────────────

#[test]
fn multiple_widgets_idle_after_initial_frame() {
    let mut h = TestHarness::new(800.0, 600.0);

    h.mount(Text::new("a"));
    h.mount(Text::new("b"));
    h.mount(Button::new("c").primary());
    h.mount(Switch::new(Signal::new(false)));

    h.run_frame();
    h.run_frame();
    assert_eq!(h.dirty_count(), 0);
}

// ── Hover ───────────────────────────────────────────────────────────

#[test]
fn hover_at_changes_hover_state() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Button::new("Hover me").primary());
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
}

// ── Disabled ───────────────────────────────────────────────────────

#[test]
fn disabled_button_is_not_focusable() {
    let mut h = TestHarness::new(800.0, 600.0);
    let id = h.mount(Button::new("Disabled").primary().disabled());
    h.run_frame();
    assert!(!h.find(id).unwrap().is_focusable());
}

// ── ReactiveConditional preserves internal state ──────────────────

#[test]
fn reactive_conditional_preserves_child_state() {
    let mut h = TestHarness::new(800.0, 600.0);
    let show = Signal::new(true);
    let count = Signal::new(0u32);
    let count_clone = count.clone();

    let wrapper_id = h.mount(Conditional::new(
        show.clone(),
        {
            let c = count_clone.clone();
            VStack::new().push(
                Button::new("+1")
                    .primary()
                    .on_click(move || c.update(|n| *n += 1)),
            )
        },
        VStack::new().push(Text::new("hidden")),
    ));
    h.run_frame();

    let wrapper = h.find(wrapper_id).unwrap();
    assert!(
        wrapper.children.len() == 2,
        "wrapper has {} children, expected 2",
        wrapper.children.len()
    );
    let branch_a = wrapper.children[0];
    let btn_id = h.find(branch_a).unwrap().children[0];

    h.click(btn_id).run_frame();
    assert_eq!(h.read_signal(&count), 1);

    // Toggle to B and back — button should still work.
    h.set_signal(&show, false).run_frame();
    h.set_signal(&show, true).run_frame();

    // Button id is the same (it was preserved in the element tree).
    h.click(btn_id).run_frame();
    assert_eq!(h.read_signal(&count), 2);
}

// ── Color Utilities ────────────────────────────────────────────────

#[test]
fn white_black_contrast_ratio() {
    assert!(Color::WHITE.contrast_ratio(&Color::BLACK) > 10.0);
    assert!(Color::WHITE.meets_aa(&Color::BLACK, false));
    assert!(Color::WHITE.meets_aa(&Color::BLACK, true));
}

#[test]
fn auto_fg_returns_readable_contrast() {
    let bg = Color::rgba8(37, 99, 235, 255);
    let fg = Color::auto_fg(&bg);
    assert!(bg.contrast_ratio(&fg) > 4.5);
}
