use auralis_signal::Signal;
use burin::testing::TestHarness;
use burin::widgets::input::Checkbox;

#[test]
fn indeterminate_click_goes_directly_to_unchecked() {
    // Standard behavior: clicking an indeterminate checkbox resolves it to
    // UNCHECKED in a single click (not indeterminate -> checked -> unchecked).
    let checked = Signal::new(true);
    let indet = Signal::new(true);
    let mut h = TestHarness::new(200.0, 100.0);
    let id = h.mount(Checkbox::new(checked.clone()).indeterminate(indet.clone()));
    h.run_frame();

    // One click.
    h.click(id);
    h.run_frame();

    assert!(!indet.read(), "click must clear indeterminate");
    assert!(
        !checked.read(),
        "single click on indeterminate must go to UNCHECKED (got checked={})",
        checked.read()
    );
}

#[test]
fn indeterminate_then_click_cycles_normally() {
    // After resolving from indeterminate to unchecked, a further click checks it.
    let checked = Signal::new(true);
    let indet = Signal::new(true);
    let mut h = TestHarness::new(200.0, 100.0);
    let id = h.mount(Checkbox::new(checked.clone()).indeterminate(indet.clone()));
    h.run_frame();

    h.click(id); // indeterminate -> unchecked
    h.run_frame();
    h.click(id); // unchecked -> checked
    h.run_frame();

    assert!(!indet.read());
    assert!(checked.read(), "second click after resolving must check it");
}
