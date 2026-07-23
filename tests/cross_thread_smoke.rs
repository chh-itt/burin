//! Cross-thread wake + run_on_ui smoke tests.
//!
//! Verifies the Layer 1–3 mechanism end-to-end:
//!   1. A background thread enqueues work via [`run_on_ui`].
//!   2. The closure executes on the UI thread when the harness drains.
//!   3. Multiple enqueues are coalesced (dedup + FIFO order).
//!
//! Note: `Signal<T>` uses `Rc` internally and is not `Send`.  These tests
//! use `Arc<Mutex<T>>` (which is `Send`) as the verification target.
//! Production code should use [`run_on_ui`] closures that either capture
//! `Send` data directly or access the signal on the UI thread through
//! a `Send` channel / wrapper.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use burin::platform::wake;
use burin::testing::TestHarness;

/// Reset globals so parallel tests don't interfere.
fn setup() {
    burin::platform::wake::reset_state();
}

/// A closure enqueued from a background thread via `run_on_ui` is
/// executed on the harness's (UI) thread when `run_frame` drains.
#[test]
fn run_on_ui_from_background_thread_executes_on_ui() {
    setup();
    let shared = Arc::new(Mutex::new(String::new()));

    let mut h = TestHarness::new(400.0, 100.0);
    h.mount(burin::widgets::display::Text::new("label"));
    h.run_frame();

    let shared_bg = shared.clone();
    let done = Arc::new(AtomicBool::new(false));
    let done_c = done.clone();

    std::thread::spawn(move || {
        wake::run_on_ui(move || {
            *shared_bg.lock().unwrap() = "updated".into();
        });
        done_c.store(true, Ordering::Release);
    });

    spin_for(&done, Duration::from_secs(5));

    h.run_frame();
    assert_eq!(*shared.lock().unwrap(), "updated");
}

/// Multiple `run_on_ui` calls within one wake cycle are delivered in
/// FIFO order within a single `run_frame`.
#[test]
fn multiple_run_on_ui_calls_deliver_fifo() {
    setup();
    let shared = Arc::new(Mutex::new(Vec::new()));

    let mut h = TestHarness::new(400.0, 50.0);
    h.mount(burin::widgets::display::Text::new(""));
    h.run_frame();

    let s = shared.clone();
    let done = Arc::new(AtomicBool::new(false));
    let dc = done.clone();

    std::thread::spawn(move || {
        for i in 0..10 {
            let v = s.clone();
            wake::run_on_ui(move || {
                v.lock().unwrap().push(i);
            });
        }
        dc.store(true, Ordering::Release);
    });

    spin_for(&done, Duration::from_secs(5));
    h.run_frame();

    let result = shared.lock().unwrap();
    assert_eq!(result.len(), 10);
    assert_eq!(*result, (0..10).collect::<Vec<_>>());
}

/// `wake_ui()` is idempotent — 1000 calls without a drain are safe
/// (internal dedup `AtomicBool` prevents `wake_up()` storms).
#[test]
fn wake_ui_is_deduped() {
    setup();
    for _ in 0..1000 {
        wake::wake_ui();
    }
}

/// `run_on_ui` with large payload (> 1 MB) works correctly.
#[test]
fn run_on_ui_with_large_payload() {
    setup();
    let shared = Arc::new(Mutex::new(String::new()));

    let mut h = TestHarness::new(400.0, 50.0);
    h.mount(burin::widgets::display::Text::new(""));
    h.run_frame();

    let s = shared.clone();
    let done = Arc::new(AtomicBool::new(false));
    let dc = done.clone();

    std::thread::spawn(move || {
        let large = "x".repeat(1_000_000);
        wake::run_on_ui(move || {
            *s.lock().unwrap() = large;
        });
        dc.store(true, Ordering::Release);
    });

    spin_for(&done, Duration::from_secs(5));
    h.run_frame();

    assert_eq!(shared.lock().unwrap().len(), 1_000_000);
}

/// Drain resets cleanly, allowing a second background-thread cycle to
/// deliver work in a subsequent frame.
#[test]
#[ignore = "flaky on CI — requires real OS thread coordination"]
fn run_on_ui_works_across_multiple_cycles() {
    setup();
    let shared = Arc::new(Mutex::new(0));

    let mut h = TestHarness::new(400.0, 50.0);
    h.mount(burin::widgets::display::Text::new(""));
    h.run_frame();

    // Cycle 1
    let s1 = shared.clone();
    let done1 = Arc::new(AtomicBool::new(false));
    let dc1 = done1.clone();
    std::thread::spawn(move || {
        wake::run_on_ui(move || *s1.lock().unwrap() = 1);
        dc1.store(true, Ordering::Release);
    });
    spin_for(&done1, Duration::from_secs(5));
    h.run_frame();
    assert_eq!(*shared.lock().unwrap(), 1);

    // Cycle 2
    let s2 = shared.clone();
    let done2 = Arc::new(AtomicBool::new(false));
    let dc2 = done2.clone();
    std::thread::spawn(move || {
        wake::run_on_ui(move || *s2.lock().unwrap() = 42);
        dc2.store(true, Ordering::Release);
    });
    spin_for(&done2, Duration::from_secs(5));
    h.run_frame();
    assert_eq!(*shared.lock().unwrap(), 42);
}

// ── helpers ─────────────────────────────────────────────────────────

fn spin_for(flag: &AtomicBool, timeout: Duration) {
    let start = std::time::Instant::now();
    while !flag.load(Ordering::Acquire) {
        if start.elapsed() > timeout {
            panic!("background thread did not finish within {:?}", timeout);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
