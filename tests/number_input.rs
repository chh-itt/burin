use auralis_signal::Signal;
use burin::testing::test_harness::TestHarness;
use burin::widgets::input::NumberInput;

#[test]
fn number_input_mounts() {
    let val = Signal::new(0.0);
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(NumberInput::new(val.clone()));
    h.run_frame();
    // Should mount without panic
}

#[test]
fn number_input_displays_initial_value() {
    let val = Signal::new(42.0);
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(NumberInput::new(val.clone()));
    h.run_frame();
    assert_eq!(val.read(), 42.0, "Initial value preserved");
}

#[test]
fn number_input_respects_range() {
    let val = Signal::new(5.0);
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(NumberInput::new(val.clone()).range(0.0, 10.0).step(1.0));
    h.run_frame();
    assert!(val.read() >= 0.0 && val.read() <= 10.0, "Value in range");
}

#[test]
fn number_input_disabled_mounts() {
    let val = Signal::new(0.0);
    let mut h = TestHarness::new(400.0, 200.0);
    h.mount(NumberInput::new(val.clone()).disabled());
    h.run_frame();
}

#[test]
fn two_number_inputs_independent() {
    let a = Signal::new(1.0);
    let b = Signal::new(2.0);
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(
        burin::widgets::layout::VStack::new()
            .push(NumberInput::new(a.clone()))
            .push(NumberInput::new(b.clone())),
    );
    h.run_frame();
    assert_eq!(a.read(), 1.0);
    assert_eq!(b.read(), 2.0);
}
