//! Regression guard: production-wiring semantics of `auralis_task::timer::sleep`.
//!
//! Audit 2026-07-17 round 5, A1: the production `App` installed a
//! `DeferredScheduler` but never a `TimeSource`, so `SleepFuture::poll`
//! saw `now == 0` and expired EVERY timer on the next flush —
//! `sleep(320ms)` silently behaved like `yield_now()` (measured: 1.1 ms).
//! Fixed by installing `core::clock::ClockTimeSource` in `App::new` /
//! `AppBuilder::build` and bridging `next_timer_delay_ms()` into winit's
//! `ControlFlow::WaitUntil`.
//!
//! These tests reproduce both wirings without a window:
//! - broken wiring (no TimeSource) is documented by the `--ignored` probe
//!   below (kept for archaeology);
//! - fixed wiring must make `sleep(220ms)` take >= 200ms wall time.
//!
//! Run guard: cargo test --test sleep_semantics_probe
//! Run probe: cargo test --test sleep_semantics_probe -- --ignored --nocapture

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Simulate the winit loop: drain the flush scheduler, fire expired
/// executor timers (the about_to_wait bridge), sleep until the next
/// deadline. Returns total loop turns.
fn run_event_loop_until(
    sched: &Rc<auralis_task::DeferredScheduler>,
    done: &Rc<Cell<Option<Duration>>>,
    max_wall: Duration,
) -> u32 {
    let t0 = Instant::now();
    let mut turns = 0;
    while t0.elapsed() < max_wall {
        turns += 1;
        sched.drain();
        auralis_task::drain_deferred_signal_callbacks();
        // about_to_wait bridge: fire expired timers.
        match auralis_task::next_timer_delay_ms() {
            Some(0) => {
                auralis_task::flush_all();
                sched.drain();
            }
            Some(delay) => {
                // WaitUntil equivalent (capped so the test stays responsive).
                std::thread::sleep(Duration::from_millis(delay.min(20)));
            }
            None => {
                if done.get().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if done.get().is_some() {
            break;
        }
    }
    turns
}

#[test]
fn sleep_with_clock_time_source_waits_real_time() {
    let sched = auralis_task::DeferredScheduler::new();
    auralis_task::init_flush_scheduler(sched.clone() as Rc<dyn auralis_task::ScheduleFlush>);
    // The A1 fix: production wiring installs a ClockTimeSource.
    auralis_task::init_time_source(Rc::new(burin::core::clock::ClockTimeSource::new()));

    let done: Rc<Cell<Option<Duration>>> = Rc::new(Cell::new(None));
    let done2 = done.clone();
    let t0 = Instant::now();

    auralis_task::spawn_global(async move {
        auralis_task::timer::sleep(Duration::from_millis(220)).await;
        done2.set(Some(t0.elapsed()));
    });

    run_event_loop_until(&sched, &done, Duration::from_secs(5));

    let elapsed = done.get().expect("sleep task must complete within 5s");
    assert!(
        elapsed >= Duration::from_millis(200),
        "sleep(220ms) completed in {elapsed:?} — timer semantics broken again (A1 regression)"
    );
    assert!(
        elapsed < Duration::from_millis(2000),
        "sleep(220ms) took {elapsed:?} — wake-up bridge not firing (lost wakeup)"
    );
}

/// Archaeology probe: the pre-fix behaviour (no TimeSource → sleep expires
/// on the next flush). Kept `--ignored` to document the failure mode.
#[test]
#[ignore]
fn sleep_320ms_without_time_source_completes_immediately() {
    let sched = auralis_task::DeferredScheduler::new();
    auralis_task::init_flush_scheduler(sched.clone() as Rc<dyn auralis_task::ScheduleFlush>);
    // Deliberately NO init_time_source — the pre-A1 production wiring.

    let done: Rc<Cell<Option<Duration>>> = Rc::new(Cell::new(None));
    let done2 = done.clone();
    let t0 = Instant::now();

    auralis_task::spawn_global(async move {
        auralis_task::timer::sleep(Duration::from_millis(320)).await;
        done2.set(Some(t0.elapsed()));
    });

    let mut turns = 0;
    for i in 0..600 {
        sched.drain();
        if done.get().is_some() {
            turns = i + 1;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    eprintln!(
        "sleep(320ms) without TimeSource completed after {turns} turns, wall time: {:?}",
        done.get()
    );
}
