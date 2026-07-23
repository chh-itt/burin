use auralis_signal::Signal;
use burin::widgets::input::RadioGroup;

fn get_sig(s: &str) -> Signal<String> {
    Signal::new(s.to_string())
}

#[test]
fn radio_mounts() {
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let sig = get_sig("a");
    let id = h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .option("Beta", "b".into()),
    );
    h.run_frame();
    h.assert_visible(id);
    h.assert_a11y_role(id, accesskit::Role::RadioGroup);
}

#[test]
fn radio_has_children() {
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let sig = get_sig("a");
    let id = h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .option("Beta", "b".into()),
    );
    h.run_frame();
    let el = h.find(id).unwrap();
    assert_eq!(el.children.len(), 2, "two radio buttons");
}

#[test]
fn radio_initial_selection() {
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let sig = get_sig("a");
    h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .option("Beta", "b".into()),
    );
    h.run_frame();
    assert_eq!(h.read_signal(&sig), "a".to_string());
}

#[test]
fn radio_click_changes_selection() {
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let sig = get_sig("a");
    let id = h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .option("Beta", "b".into()),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    let beta_row = el.children[1];
    h.click(beta_row).run_frame();

    assert_eq!(h.read_signal(&sig), "b".to_string());
}

#[test]
fn radio_disabled_option_not_clickable() {
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let sig = get_sig("a");
    let id = h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .disabled_option("Beta", "b".into()),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    let beta_row = el.children[1];
    h.click(beta_row).run_frame();

    assert_eq!(
        h.read_signal(&sig),
        "a".to_string(),
        "disabled option should not change selection"
    );
}

#[test]
fn radio_on_value_changed_fires() {
    use std::cell::Cell;
    use std::rc::Rc;

    let count = Rc::new(Cell::new(0u32));
    let sig = get_sig("a");
    let c2 = count.clone();
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let id = h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .option("Beta", "b".into())
            .on_value_changed(move |_: String| {
                c2.set(c2.get() + 1);
            }),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    h.click(el.children[1]).run_frame();

    assert_eq!(count.get(), 1);
}

#[test]
fn radio_a11y_label() {
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let sig = get_sig("a");
    let id = h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .option("Beta", "b".into()),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    let alpha_row = el.children[0];
    h.assert_a11y_role(alpha_row, accesskit::Role::RadioButton);
    h.assert_a11y_label(alpha_row, "Alpha");
}

#[test]
fn radio_disabled_group() {
    let mut h = burin::testing::TestHarness::new(400.0, 300.0);
    let sig = get_sig("a");
    let id = h.mount(
        RadioGroup::new(sig.clone())
            .option("Alpha", "a".into())
            .option("Beta", "b".into())
            .disabled(),
    );
    h.run_frame();

    let el = h.find(id).unwrap();
    h.click(el.children[1]).run_frame();
    assert_eq!(h.read_signal(&sig), "a".to_string());
}
