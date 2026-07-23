//! Golden / snapshot workflow smoke test.
//!
//! Verifies the full round-trip: render → bless baseline → compare match, and
//! that a real visual change is detected.

use burin::testing::snapshot::{check_snapshot, SnapshotOptions};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::input::Button;
use burin::widgets::layout::VStack;
use std::sync::Mutex;

// `AURALIS_UPDATE_SNAPSHOTS` is process-global; serialize env-touching tests.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// End-to-end: a self-blessing round-trip using a temp dir as the crate root,
/// so the test is hermetic (doesn't depend on a committed PNG).
#[test]
fn snapshot_roundtrip_bless_then_match() {
    let _guard = ENV_LOCK.lock().unwrap();
    let tmp = std::env::temp_dir().join("auralis_snap_test_rt");
    let _ = std::fs::remove_dir_all(&tmp);
    let crate_dir = tmp.to_str().unwrap().to_string();

    let mut h = TestHarness::new(200.0, 120.0);
    h.mount(
        VStack::new()
            .push(Button::new("Save").primary())
            .push(Text::new("hello")),
    );
    h.settle(8);
    let buf = h.render_to_pixels();
    let opts = SnapshotOptions::default();

    // 1) No baseline + no bless → Err, writes .new.png.
    let r = check_snapshot(&buf, &crate_dir, "panel", &opts);
    assert!(r.is_err(), "missing baseline must fail");

    // 2) Bless creates the baseline.
    std::env::set_var("AURALIS_UPDATE_SNAPSHOTS", "1");
    let r = check_snapshot(&buf, &crate_dir, "panel", &opts);
    std::env::remove_var("AURALIS_UPDATE_SNAPSHOTS");
    assert!(r.is_ok(), "bless should create baseline: {r:?}");
    assert!(
        tmp.join("tests/snapshots/panel.png").exists(),
        "baseline written"
    );

    // 3) Same render now matches the baseline.
    let buf2 = h.render_to_pixels();
    let r = check_snapshot(&buf2, &crate_dir, "panel", &opts);
    assert!(r.is_ok(), "identical render must match: {r:?}");

    let _ = std::fs::remove_dir_all(&tmp);
}

/// A real visual change (different content) must be detected as a mismatch.
#[test]
fn snapshot_detects_visual_change() {
    let _guard = ENV_LOCK.lock().unwrap();
    let tmp = std::env::temp_dir().join("auralis_snap_test_change");
    let _ = std::fs::remove_dir_all(&tmp);
    let crate_dir = tmp.to_str().unwrap().to_string();
    let opts = SnapshotOptions::default();

    // Bless a red-ish panel.
    let mut h1 = TestHarness::new(160.0, 80.0);
    h1.mount(VStack::new().push(Button::new("A").primary()));
    h1.settle(8);
    std::env::set_var("AURALIS_UPDATE_SNAPSHOTS", "1");
    check_snapshot(&h1.render_to_pixels(), &crate_dir, "p", &opts).unwrap();
    std::env::remove_var("AURALIS_UPDATE_SNAPSHOTS");

    // A visibly different layout must mismatch.
    let mut h2 = TestHarness::new(160.0, 80.0);
    h2.mount(
        VStack::new()
            .push(Button::new("A").primary())
            .push(Button::new("B").primary())
            .push(Text::new("extra content changes pixels")),
    );
    h2.settle(8);
    let r = check_snapshot(&h2.render_to_pixels(), &crate_dir, "p", &opts);
    assert!(r.is_err(), "different content must be detected");

    let _ = std::fs::remove_dir_all(&tmp);
}
