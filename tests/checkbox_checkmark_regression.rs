//! Regression: unchecking a Checkbox must hide the checkmark.
//!
//! Pre-existing bug (present since before the AppContext refactor): the
//! checked/indeterminate signal callbacks dirtied the box and parent elements
//! but NOT the checkmark icon child. The icon's paint is gated by
//! `CheckboxIconState.visible`, but without invalidating the icon's own scene
//! cache, the stale checkmark stroke replayed from cache after unchecking — so
//! an unchecked box still showed a √. Fix: the callbacks now also dirty +
//! bump_subtree_gen the icon element.

use auralis_signal::Signal;
use burin::render::DrawCommand;
use burin::testing::TestHarness;
use burin::widgets::input::Checkbox;

fn count_stroke_paths(scene: &[DrawCommand]) -> usize {
    scene
        .iter()
        .filter(|c| matches!(c, DrawCommand::StrokePath { .. }))
        .count()
}

#[test]
fn checkbox_uncheck_hides_checkmark() {
    let checked = Signal::new(true);
    let mut h = TestHarness::new(200.0, 100.0);
    h.mount(Checkbox::new(checked.clone()));
    h.run_frame();

    let n_checked = count_stroke_paths(&h.drain_scene());
    assert!(
        n_checked >= 1,
        "checked checkbox must draw a checkmark stroke, got {n_checked}"
    );

    checked.set(false);
    h.run_frame();

    let n_unchecked = count_stroke_paths(&h.drain_scene());
    assert_eq!(
        n_unchecked, 0,
        "unchecked checkbox must NOT draw a checkmark stroke (stale icon cache?), got {n_unchecked}",
    );
}

#[test]
fn checkbox_recheck_shows_checkmark_again() {
    let checked = Signal::new(false);
    let mut h = TestHarness::new(200.0, 100.0);
    h.mount(Checkbox::new(checked.clone()));
    h.run_frame();
    assert_eq!(
        count_stroke_paths(&h.drain_scene()),
        0,
        "initially unchecked: no checkmark"
    );

    checked.set(true);
    h.run_frame();
    assert!(
        count_stroke_paths(&h.drain_scene()) >= 1,
        "after check: checkmark appears"
    );

    checked.set(false);
    h.run_frame();
    assert_eq!(
        count_stroke_paths(&h.drain_scene()),
        0,
        "after uncheck: checkmark gone"
    );
}
