use auralis_signal::Signal;
use burin::testing::selector::*;
use burin::testing::test_harness::TestHarness;
use burin::widgets::input::{Field, Form, TextInput};

#[test]
fn field_mounts_with_label() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(
        Field::new()
            .label("Username")
            .child(TextInput::new(Signal::new(String::new()))),
    );
    h.run_frame();
    assert!(
        h.find_sel(by_text("Username")).is_some(),
        "Field should show label text"
    );
}

#[test]
fn form_mounts_multiple_fields() {
    let mut h = TestHarness::new(400.0, 600.0);
    h.mount(
        Form::new()
            .child(
                Field::new()
                    .label("A")
                    .child(TextInput::new(Signal::new(String::new()))),
            )
            .child(
                Field::new()
                    .label("B")
                    .child(TextInput::new(Signal::new(String::new()))),
            ),
    );
    h.run_frame();
    assert!(h.find_sel(by_text("A")).is_some(), "Field A visible");
    assert!(h.find_sel(by_text("B")).is_some(), "Field B visible");
}

#[test]
fn field_with_validator_mounts() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(
        Field::new()
            .label("Email")
            .validator(|v| {
                if v.contains('@') {
                    None
                } else {
                    Some("Invalid email".into())
                }
            })
            .child(TextInput::new(Signal::new(String::from("bad")))),
    );
    h.run_frame();
    assert!(
        h.find_sel(by_text("Email")).is_some(),
        "Field should mount with validator"
    );
}

#[test]
fn form_on_submit_callback() {
    let called = std::rc::Rc::new(std::cell::RefCell::new(false));
    let c = called.clone();
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(
        Form::new()
            .child(
                Field::new()
                    .label("Name")
                    .child(TextInput::new(Signal::new(String::from("A")))),
            )
            .on_submit(move || {
                *c.borrow_mut() = true;
            }),
    );
    h.run_frame();
    assert!(!*called.borrow(), "On_submit should not fire on mount");
}

#[test]
fn required_field_with_empty_value_validation() {
    let text = Signal::new(String::new());
    let mut h = TestHarness::new(400.0, 300.0);
    let _form_id = h.mount(
        Field::new()
            .label("Required")
            .required(true)
            .child(TextInput::new(text.clone())),
    );
    h.run_frame();
    // Form and field should mount without panicking
    assert!(
        h.find_sel(by_label("Required")).is_some(),
        "Required field should mount"
    );
}
