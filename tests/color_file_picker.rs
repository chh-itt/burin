use auralis_signal::Signal;
use burin::style::Color;
use burin::testing::selector::by_text;
use burin::testing::test_harness::TestHarness;

// ═══════════════════════════════════════════
// ColorPicker
// ═══════════════════════════════════════════

#[test]
fn color_picker_mounts() {
    let color = Signal::new(Color::rgba8(255, 0, 0, 255));
    let mut h = TestHarness::new(400.0, 400.0);
    h.mount(burin::widgets::input::ColorPicker::new(color.clone()));
    h.run_frame();
    assert_eq!(
        color.read(),
        Color::rgba8(255, 0, 0, 255),
        "Initial color preserved"
    );
}

#[test]
fn color_picker_with_presets() {
    let color = Signal::new(Color::rgba8(128, 128, 128, 255));
    let mut h = TestHarness::new(400.0, 400.0);
    h.mount(
        burin::widgets::input::ColorPicker::new(color.clone()).presets(vec![
            Color::rgba8(255, 0, 0, 255),
            Color::rgba8(0, 255, 0, 255),
            Color::rgba8(0, 0, 255, 255),
        ]),
    );
    h.run_frame();
}

#[test]
fn color_picker_on_changed_callback() {
    let color = Signal::new(Color::rgba8(0, 0, 0, 255));
    let called = std::rc::Rc::new(std::cell::RefCell::new(false));
    let c = called.clone();
    let mut h = TestHarness::new(400.0, 400.0);
    h.mount(
        burin::widgets::input::ColorPicker::new(color.clone()).on_changed(move |_| {
            *c.borrow_mut() = true;
        }),
    );
    h.run_frame();
    assert!(!*called.borrow(), "On_changed should not fire on mount");
}

// ═══════════════════════════════════════════
// FilePickerButton
// ═══════════════════════════════════════════

#[cfg(feature = "file-dialog")]
#[test]
fn file_picker_button_mounts() {
    use burin::widgets::input::FilePickerButton;
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(FilePickerButton::new("Open file"));
    h.run_frame();
    assert!(
        h.find_sel(by_text("Open file")).is_some(),
        "FilePicker label visible"
    );
}

#[cfg(feature = "file-dialog")]
#[test]
fn file_picker_button_mode_folder() {
    use burin::widgets::input::{FilePickerButton, FilePickerMode};
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(FilePickerButton::new("Choose folder").mode(FilePickerMode::Folder));
    h.run_frame();
    assert!(
        h.find_sel(by_text("Choose folder")).is_some(),
        "Folder picker label visible"
    );
}

#[cfg(feature = "file-dialog")]
#[test]
fn file_picker_with_callback() {
    use burin::widgets::input::FilePickerButton;
    let called = std::rc::Rc::new(std::cell::RefCell::new(false));
    let c = called.clone();
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(
        FilePickerButton::new("Pick").on_file_selected(move |_path| {
            *c.borrow_mut() = true;
        }),
    );
    h.run_frame();
}
