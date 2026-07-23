//! A thread-local, controllable time source.
//!
//! Returns wall-clock time by default. In test mode (`install_virtual`), it
//! returns a pinned virtual instant that the test advances explicitly, so that
//! every time-driven feature (animation, tooltip, blink, debounce…) becomes
//! deterministic. The real app never installs a virtual clock, so `now()` is
//! wall-clock there.

use std::cell::Cell;
use web_time::{Duration, Instant};

thread_local! {
    /// `None` = use wall-clock; `Some(t)` = virtual clock pinned at `t`.
    static VIRTUAL: Cell<Option<Instant>> = const { Cell::new(None) };
    /// Epoch for [`animation_millis`] — lazily anchored on first call,
    /// re-anchored whenever the virtual clock is (un)installed so tests
    /// always observe a deterministic 0-based animation timeline.
    static ANIM_EPOCH: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The current time: wall-clock by default, or the pinned virtual instant.
#[inline]
pub fn now() -> Instant {
    VIRTUAL.with(|v| v.get()).unwrap_or_else(Instant::now)
}

/// Milliseconds elapsed on the animation timeline (monotonic, 0-based).
///
/// This is the single time axis for periodic visuals (indeterminate
/// progress sweep, skeleton shimmer, marquee…). Unlike
/// `SystemTime::now()` it follows the virtual clock in tests, and the
/// 0-based epoch keeps `f32` phase math exact (UNIX-epoch millis exceed
/// f32's 24-bit mantissa, turning `(ms as f32 * k).fract()` into noise).
pub fn animation_millis() -> u64 {
    let t = now();
    ANIM_EPOCH.with(|e| {
        let epoch = match e.get() {
            Some(ep) => ep,
            None => {
                e.set(Some(t));
                t
            }
        };
        t.saturating_duration_since(epoch).as_millis() as u64
    })
}

/// Install a virtual clock starting at the current wall-clock instant.
pub fn install_virtual() {
    VIRTUAL.with(|v| v.set(Some(Instant::now())));
    ANIM_EPOCH.with(|e| e.set(None));
}

/// Install a virtual clock pinned at `t`.
pub fn install_virtual_at(t: Instant) {
    VIRTUAL.with(|v| v.set(Some(t)));
    ANIM_EPOCH.with(|e| e.set(None));
}

/// Advance the virtual clock by `dur`. No-op when not in virtual mode.
pub fn advance(dur: Duration) {
    VIRTUAL.with(|v| {
        if let Some(t) = v.get() {
            v.set(Some(t + dur));
        }
    });
}

/// Reset to wall-clock (clears virtual mode).
pub fn reset_to_wall() {
    VIRTUAL.with(|v| v.set(None));
    ANIM_EPOCH.with(|e| e.set(None));
}

/// Whether a virtual clock is currently installed.
pub fn is_virtual() -> bool {
    VIRTUAL.with(|v| v.get().is_some())
}

/// A [`TimeSource`](auralis_task::TimeSource) for the async executor,
/// driven by this module's clock.
///
/// - **Production**: `clock::now()` is wall-clock, so `timer::sleep`
///   deadlines follow real time.
/// - **Tests**: with a virtual clock installed, the async timer axis
///   follows `clock::advance` — timers fire deterministically.
///
/// Installed by the production `App` (platform/window.rs) and by
/// `TestHarness::new`. Without a TimeSource the executor expires every
/// timer on the next flush, silently turning `sleep(n)` into `yield_now()`
/// (audit 2026-07-17 round 5, A1).
pub struct ClockTimeSource {
    start: Instant,
}

impl ClockTimeSource {
    pub fn new() -> Self {
        Self { start: now() }
    }
}

impl Default for ClockTimeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl auralis_task::TimeSource for ClockTimeSource {
    fn now_ms(&self) -> u64 {
        now().duration_since(self.start).as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_millis_follows_virtual_clock() {
        reset_to_wall();
        install_virtual();
        assert_eq!(animation_millis(), 0, "epoch re-anchors on install_virtual");
        advance(Duration::from_millis(300));
        assert_eq!(animation_millis(), 300);
        advance(Duration::from_millis(450));
        assert_eq!(animation_millis(), 750);
        reset_to_wall();
    }

    #[test]
    fn animation_millis_is_monotonic_in_wall_mode() {
        reset_to_wall();
        let a = animation_millis();
        let b = animation_millis();
        assert!(b >= a);
    }

    #[test]
    fn virtual_clock_is_pinned_and_advances() {
        reset_to_wall();
        assert!(!is_virtual());
        install_virtual();
        assert!(is_virtual());
        let t0 = now();
        // Pinned: repeated reads do not move on their own.
        assert_eq!(now(), t0);
        advance(Duration::from_millis(300));
        assert_eq!(now(), t0 + Duration::from_millis(300));
        reset_to_wall();
        assert!(!is_virtual());
    }

    #[test]
    fn advance_is_noop_in_wall_mode() {
        reset_to_wall();
        advance(Duration::from_secs(10)); // no panic, no effect
        assert!(!is_virtual());
    }
}
