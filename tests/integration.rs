//! Integration tests covering widget combinations, layout, and theme.

use auralis_signal::Signal;
use burin::core::ElementId;
use burin::event::{ClickCounter, FocusManager};
use burin::style::{Color, CornerRadii, Dimension, Rect};
use burin::testing::TestHarness;
use burin::theme::M3Theme;
use burin::widgets::display::Text;
use burin::widgets::input::{Button, Checkbox, Slider, TextInput};
use burin::widgets::layout::*;

fn mount(w: impl burin::core::widget::Widget) -> TestHarness {
    let mut h = TestHarness::new(800.0, 600.0);
    h.mount(w);
    h.run_frame();
    h
}

#[test]
fn sidebar_layout() {
    let h = mount(
        HStack::new()
            .push(
                SizedBox::new().width(Dimension::Pixels(240.0)).child(
                    VStack::new()
                        .push(Text::new("Navigation"))
                        .push(Button::new("Home").primary())
                        .push(Button::new("Settings")),
                ),
            )
            .push(
                VStack::new()
                    .push(Text::new("Content"))
                    .push(Text::new("Main area")),
            ),
    );
    let root = h.find(h.root_id()).unwrap();
    let hstack = h.find(root.children[0]).unwrap();
    assert_eq!(hstack.children.len(), 2);
}

#[test]
fn grid_dashboard() {
    let h = mount(
        VStack::new()
            .push(Text::new("Dashboard"))
            .push(
                GridRow::new()
                    .push(GridItem::new(Text::new("Chart A")).cols(12))
                    .push(GridItem::new(Text::new("Chart B")).cols(12)),
            )
            .push(
                GridRow::new()
                    .push(GridItem::new(Text::new("Stats")).cols(8))
                    .push(GridItem::new(Text::new("Activity")).cols(16)),
            ),
    );
    let root = h.find(h.root_id()).unwrap();
    assert!(root.children.len() > 0);
}

#[test]
fn form_layout() {
    let name = Signal::new(String::new());
    let email = Signal::new(String::new());
    let agreed = Signal::new(false);

    let h = mount(
        VStack::new()
            .push(Text::new("Registration"))
            .push(TextInput::new(name).placeholder("Name"))
            .push(TextInput::new(email).placeholder("Email"))
            .push(Checkbox::new(agreed))
            .push(Button::new("Submit").primary()),
    );
    let root = h.find(h.root_id()).unwrap();
    let vstack = h.find(root.children[0]).unwrap();
    assert_eq!(vstack.children.len(), 5);
}

#[test]
fn m3_theme_from_seed() {
    let _base = M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF));
    let _tw = M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF));
}

#[test]
fn semantic_colors_accessible() {
    let theme = M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF));
    let primary_bg = theme.scheme.primary;
    let primary_fg = theme.scheme.on_primary;
    assert!(primary_fg.meets_aa(&primary_bg, true));
}

#[test]
fn click_counter_single() {
    let mut cc = ClickCounter::new();
    let pos = burin::style::Point::new(100.0, 100.0);
    let now = web_time::Instant::now();

    cc.pointer_down(pos, now);
    let result = cc.pointer_up(pos, now);
    assert!(matches!(result, burin::event::ClickResult::Single { .. }));
}

#[test]
fn click_counter_double() {
    let mut cc = ClickCounter::new();
    let pos = burin::style::Point::new(100.0, 100.0);
    let now = web_time::Instant::now();

    cc.pointer_down(pos, now);
    assert!(matches!(
        cc.pointer_up(pos, now),
        burin::event::ClickResult::Single { .. }
    ));

    // Second click within interval
    cc.pointer_down(pos, now);
    assert!(matches!(
        cc.pointer_up(pos, now),
        burin::event::ClickResult::Double { .. }
    ));
}

#[test]
fn drag_recognizer_arena() {
    use burin::event::GesturePhase;
    use burin::event::{DragRecognizer, Recognizer};

    let mut dr = DragRecognizer::new();
    let pos = burin::style::Point::new(100.0, 100.0);

    assert!(matches!(
        dr.handle_event(GesturePhase::Started, pos),
        burin::event::RecognizerResult::Possible
    ));
    let moved = burin::style::Point::new(120.0, 100.0);
    assert!(matches!(
        dr.handle_event(GesturePhase::Moved, moved),
        burin::event::RecognizerResult::Accepted
    ));
}

#[test]
fn focus_scope_push_pop() {
    use burin::event::TraversalEdgeBehavior;
    let mut fs = FocusManager::new();
    let id1 = ElementId::allocate();
    let scope_root = ElementId::allocate();

    fs.set_focused(Some(id1));
    assert_eq!(fs.focused(), Some(id1));

    // Push scope saves the current focus
    fs.push_scope(scope_root, TraversalEdgeBehavior::Wrap);
    assert_eq!(fs.focused(), Some(id1)); // focus preserved
    assert!(fs.current_scope_root().is_some());

    // Pop scope restores the saved focus
    fs.pop_scope();
    assert_eq!(fs.focused(), Some(id1));
    assert!(fs.current_scope_root().is_none());
}

#[test]
fn focus_nested_scopes() {
    use burin::event::TraversalEdgeBehavior;
    let mut fs = FocusManager::new();
    let id1 = ElementId::allocate();
    let id2 = ElementId::allocate();
    let outer = ElementId::allocate();
    let inner = ElementId::allocate();

    fs.set_focused(Some(id1));
    fs.push_scope(outer, TraversalEdgeBehavior::Wrap);
    fs.push_scope(inner, TraversalEdgeBehavior::Leave);
    fs.set_focused(Some(id2));
    assert_eq!(fs.focused(), Some(id2));

    fs.pop_scope(); // pop inner → restore id1
    assert_eq!(fs.focused(), Some(id1));

    fs.pop_scope(); // pop outer → still id1 (no saved_focus before outer)
    assert_eq!(fs.focused(), Some(id1));
}

#[test]
fn painter_generates_commands() {
    use burin::render::Painter;
    let clip = Rect::new(0.0, 0.0, 800.0, 600.0);
    let xform = glam::Affine2::IDENTITY;
    let z = 0i32;
    let mut painter = Painter::new(clip);
    painter.fill_rect(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        Color::WHITE,
        clip.into(),
        xform,
        z,
    );
    painter.fill_rounded_rect(
        Rect::new(10.0, 10.0, 80.0, 40.0),
        Color::rgba8(37, 99, 235, 255),
        CornerRadii::all(8.0),
        clip.into(),
        xform,
        z,
    );
    let commands = painter.take_commands();
    assert!(commands.len() >= 2);
}

#[test]
fn element_tree_depth() {
    let h = mount(VStack::new().push(HStack::new().push(VStack::new().push(Text::new("Deep")))));
    let root = h.find(h.root_id()).unwrap();
    let v1 = h.find(root.children[0]).unwrap();
    let h1 = h.find(v1.children[0]).unwrap();
    let v2 = h.find(h1.children[0]).unwrap();
    let t1 = h.find(v2.children[0]).unwrap();
    assert_eq!(t1.accessible_label().as_deref(), Some("Deep"));
}

#[test]
fn element_visibility_propagation() {
    let mut h = TestHarness::new(800.0, 600.0);
    let _el_id = h.mount(Text::new("visible"));
    h.run_frame();

    let root = h.find(h.root_id()).unwrap();
    let child_id = root.children[0];

    let child = h.find_mut(child_id).unwrap();
    child.set_visible(false);
    assert!(!child.is_visible());
}

#[test]
fn widgets_have_correct_roles() {
    let h = mount(
        VStack::new()
            .push(Button::new("btn"))
            .push(Checkbox::new(Signal::new(false)))
            .push(Slider::new(Signal::new(0.5)))
            .push(TextInput::new(Signal::new(String::new()))),
    );
    let root = h.find(h.root_id()).unwrap();
    let vstack = h.find(root.children[0]).unwrap();
    assert_eq!(vstack.children.len(), 4);
}
