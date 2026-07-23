//! Direct-path coverage for `spatial_hit_test`.
//!
//! Most tests hit through `hit_test_with_fallback`, which falls back to
//! `hit_test_leaf` (a direct arena scan) when the spatial grid returns nothing.
//! That fallback MASKS spatial-grid / visibility-chain bugs — a broken spatial
//! path still "works" in those tests. These tests call `spatial_hit_test`
//! DIRECTLY so spatial-grid regressions surface instead of hiding behind the
//! leaf fallback.
//!
//! Context: a real regression (root element registered into the thread_local
//! fallback instead of the active `AppContext`, because `set_current_app` ran
//! after `arena.allocate()` in `window::mount_root`) made every spatial hit
//! miss in the real window. It was invisible to the harness (harness mount uses
//! the correct ordering) and to fallback-based tests. The ordering itself is now
//! guarded by a `debug_assert!(app.has_element(root_id))` in `mount_root`; these
//! tests add ongoing direct-path coverage for the spatial grid.

use burin::core::dirty_registry::spatial_hit_test;
use burin::style::Point;
use burin::testing::TestHarness;
use burin::widgets::input::Button;
use burin::widgets::layout::{ScrollView, VStack};

#[test]
fn spatial_hit_test_finds_top_level_button() {
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(VStack::new().push(Button::new("click me")));
    h.run_frame();
    h.run_frame();

    // The button occupies the top-left region. The DIRECT spatial path must
    // find *some* element there (not None). A None here means the visibility
    // chain or grid is broken — the fallback would have masked it.
    let hit = spatial_hit_test(&h.arena, Point::new(30.0, 20.0));
    assert!(
        hit.is_some(),
        "spatial_hit_test must find an element at the button position; \
         None indicates a broken visibility chain / grid (root not registered \
         in AppContext, etc.)",
    );
}

#[test]
fn spatial_hit_test_works_inside_scrollview() {
    // Mirrors the gallery structure (ScrollView > VStack > Button) which is
    // where the original regression manifested.
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(ScrollView::new().child(VStack::new().push(Button::new("btn"))));
    h.run_frame();
    h.run_frame();

    let hit = spatial_hit_test(&h.arena, Point::new(30.0, 20.0));
    assert!(
        hit.is_some(),
        "spatial_hit_test must find an element inside a ScrollView; None means \
         the visibility chain broke walking up through the scroll container to \
         the root",
    );
}

#[test]
fn spatial_hit_test_visibility_chain_reaches_root() {
    // Directly assert the invariant that broke: every element returned as a
    // spatial hit must have a valid visibility chain all the way to the root.
    // If the root isn't registered in the active AppContext, this returns None.
    let mut h = TestHarness::new(400.0, 300.0);
    h.mount(VStack::new().push(Button::new("x")));
    h.run_frame();
    h.run_frame();

    // Several points across the top region; at least the ones over content
    // must hit. We assert the button-area point specifically.
    let hit = spatial_hit_test(&h.arena, Point::new(20.0, 15.0));
    assert!(
        hit.is_some(),
        "hit at content point must succeed — proves the visibility chain walks \
         to a registered root",
    );
}
