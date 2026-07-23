//! A per-window scheduler that aggregates "wake at time T" deadlines,
//! replacing the coarse `HAS_*` busy-pump flags in `window.rs::about_to_wait`.
//!
//! Two wake kinds:
//! - **Continuous** (animation, spinner, toast transition): a *keyed
//!   subscription set*. While any key holds a subscription,
//!   `next_deadline()` returns `clock::now()` so the loop pulses every
//!   frame. `acquire_continuous(key)` / `release_continuous(key)` are
//!   idempotent per key — no owner can evict another owner's wake
//!   (audit 2026-07-18 animation pass: the old single `Cell<bool>` was
//!   cleared wholesale each turn, so only the two hardcoded window
//!   features could reliably keep frames alive).
//! - **Discrete** (tooltip, blink, debounce): `next_deadline()` returns the
//!   earliest future instant. The loop sleeps via `ControlFlow::WaitUntil`.
//!
//! State lives on the current window's `AppContext` (extension domain,
//! audit 2026-07-18 multi-window pass): window A's animation no longer
//! forces frames on window B, and B's `cancel(key)` cannot evict A's
//! deadlines. `App::about_to_wait` folds every window's `next_deadline`
//! into one `ControlFlow::WaitUntil`.

use rustc_hash::FxHashSet;
use std::cell::RefCell;
use web_time::Instant;

use crate::core::clock;

/// Well-known scheduler keys. Feature-level singletons use fixed values;
/// per-element subscriptions derive a key via [`keys::element_key`].
pub mod keys {
    /// Text-input cursor blink (discrete, 500 ms boundaries).
    pub const CURSOR_BLINK: u64 = 4;
    /// Tooltip show/hide delay (discrete).
    pub const TOOLTIP: u64 = 10;
    /// AnimationDriver has active property animations (continuous).
    pub const ANIM_DRIVER: u64 = 0x414E_494D; // "ANIM"
    /// Exit animations pending (continuous).
    pub const EXIT_ANIMS: u64 = 0x4558_4954; // "EXIT"
    /// Toast enter/exit transition (continuous).
    pub const TOAST: u64 = 0x70A5_70A5;
    /// Namespace for indeterminate-progress spinners.
    pub const NS_SPINNER: u32 = 1;
    /// Namespace for Phase-2 driver-owned per-element wakes (reserved).
    pub const NS_DRIVER: u32 = 2;
    /// Namespace for accordion height transitions.
    pub const NS_ACCORDION: u32 = 3;
    /// Namespace for DevTools window frame-tick polling.
    pub const NS_DEVTOOLS: u32 = 4;

    /// Derive a per-element key: `namespace` in the high 32 bits, the
    /// element's raw id in the low 32. Collision-free against the fixed
    /// keys above (their high 32 bits stay well below u32 namespaces
    /// paired with real element ids only through this constructor).
    pub fn element_key(ns: u32, eid: crate::core::ElementId) -> u64 {
        ((ns as u64) << 32) | (eid.0 & 0xFFFF_FFFF)
    }

    /// Allocate a unique scheduler key for third-party use.
    /// Keys start at a high value to avoid collision with built-in constants.
    pub fn register_key() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0x1000_0000_0000);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }
}

struct DeadlineEntry {
    instant: Instant,
    key: u64,
}

/// Per-window scheduler state.
#[derive(Default)]
struct SchedulerDomain {
    /// Keyed continuous-wake subscriptions (n small: active animations).
    continuous: RefCell<FxHashSet<u64>>,
    /// Element-namespace keys renewed since the last sweep (renewal model).
    element_renewed: RefCell<FxHashSet<u64>>,
    /// Pending discrete deadlines, kept small (n <= 10).
    discrete: RefCell<Vec<DeadlineEntry>>,
}

fn domain() -> std::rc::Rc<SchedulerDomain> {
    crate::core::app_context::current_app().extension::<SchedulerDomain>()
}

/// Subscribe `key` to per-frame wakes. Idempotent — acquiring an already
/// held key is a no-op (NOT a counted nesting; one release clears it).
pub fn acquire_continuous(key: u64) {
    domain().continuous.borrow_mut().insert(key);
}

/// Drop `key`'s per-frame wake subscription. Releasing a key that was
/// never acquired is a no-op; other keys are unaffected.
pub fn release_continuous(key: u64) {
    domain().continuous.borrow_mut().remove(&key);
}

/// Renewal-model acquire for per-element wakes (spinners, marquees):
/// the subscription survives only until the next [`sweep_stale_element_wakes`]
/// unless renewed. Call this from the element's `frame_tick` every frame —
/// the tick pass already skips hidden/inactive elements, so hiding the
/// element stops the renewal and the wake decays automatically
/// (Makepad-NextFrame-style: nothing to un-register, nothing to leak).
pub fn acquire_element_continuous(key: u64) {
    let dom = domain();
    dom.continuous.borrow_mut().insert(key);
    dom.element_renewed.borrow_mut().insert(key);
}

/// Sweep element-namespace wakes (high 32 bits != 0) that were not renewed
/// since the last sweep. Fixed feature keys are never touched. Called once
/// per event-loop turn by the window / per frame by the harness.
pub fn sweep_stale_element_wakes() {
    let dom = domain();
    let renewed = std::mem::take(&mut *dom.element_renewed.borrow_mut());
    dom.continuous
        .borrow_mut()
        .retain(|k| (k >> 32) == 0 || renewed.contains(k));
}

/// Register a discrete wake deadline.
/// If a prior deadline for the same key exists, it is replaced.
pub fn schedule_at(instant: Instant, key: u64) {
    let dom = domain();
    let mut v = dom.discrete.borrow_mut();
    v.retain(|e| e.key != key);
    v.push(DeadlineEntry { instant, key });
}

/// Cancel all wakes associated with `key` — both the continuous
/// subscription and any discrete deadline.
pub fn cancel(key: u64) {
    let dom = domain();
    dom.discrete.borrow_mut().retain(|e| e.key != key);
    dom.continuous.borrow_mut().remove(&key);
}

/// The earliest pending deadline.
///
/// - If any continuous subscription is held -> `clock::now()` (frame due immediately).
/// - Else if discrete deadlines exist -> the minimum of them.
/// - Else -> `None` (nothing scheduled).
pub fn next_deadline() -> Option<Instant> {
    let dom = domain();
    if !dom.continuous.borrow().is_empty() {
        return Some(clock::now());
    }
    let v = dom.discrete.borrow();
    v.iter().map(|e| e.instant).min()
}

/// Whether any continuous wake subscription is held.
pub fn has_continuous() -> bool {
    !domain().continuous.borrow().is_empty()
}

/// Whether any discrete deadline has expired (i.e. <= now).
/// Returns `true` only when a deadline has actually been reached;
/// frames are NOT requested during the sleep period.
pub fn expired_discrete() -> bool {
    let now = clock::now();
    domain().discrete.borrow().iter().any(|e| e.instant <= now)
}

/// Remove all discrete deadlines that have expired (<= now), returning `true`
/// if any were removed. Unlike [`expired_discrete`], this consumes the expired
/// entries so a one-shot deadline fires exactly once instead of being reported
/// as expired on every subsequent frame.
pub fn drain_expired() -> bool {
    let now = clock::now();
    let dom = domain();
    let mut v = dom.discrete.borrow_mut();
    let before = v.len();
    v.retain(|e| e.instant > now);
    before != v.len()
}

/// Whether any wake is pending (continuous or discrete).
pub fn any_active() -> bool {
    let dom = domain();
    if !dom.continuous.borrow().is_empty() {
        return true;
    }
    let pending = !dom.discrete.borrow().is_empty();
    pending
}

/// Reset the scheduler — clear all registrations for the CURRENT app.
/// Called by `TestHarness::new` for test isolation.
pub fn reset() {
    let dom = domain();
    dom.continuous.borrow_mut().clear();
    dom.element_renewed.borrow_mut().clear();
    dom.discrete.borrow_mut().clear();
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::clock;
    use std::time::Duration;

    #[test]
    fn continuous_is_reference_counted_by_key() {
        reset();
        acquire_continuous(1);
        acquire_continuous(2);
        release_continuous(1);
        assert!(has_continuous(), "key 2 still holds the wake");
        release_continuous(2);
        assert!(!has_continuous(), "all keys released -> sleep");
    }

    #[test]
    fn acquire_is_idempotent_per_key() {
        reset();
        acquire_continuous(7);
        acquire_continuous(7);
        release_continuous(7);
        assert!(
            !has_continuous(),
            "one release clears an idempotent acquire"
        );
    }

    #[test]
    fn cancel_clears_both_discrete_and_continuous_for_key() {
        reset();
        clock::install_virtual();
        acquire_continuous(9);
        schedule_at(clock::now() + Duration::from_millis(100), 9);
        cancel(9);
        assert!(!any_active(), "cancel(key) evicts continuous AND discrete");
        clock::reset_to_wall();
    }

    #[test]
    fn release_of_foreign_key_does_not_affect_others() {
        reset();
        acquire_continuous(11);
        release_continuous(12); // never acquired
        assert!(has_continuous());
        release_continuous(11);
        assert!(!has_continuous());
    }

    #[test]
    fn empty_scheduler_has_no_deadline() {
        reset();
        assert!(!any_active());
        assert!(next_deadline().is_none());
    }

    #[test]
    fn discrete_deadline_returns_earliest() {
        reset();
        clock::install_virtual();
        let t0 = clock::now();
        schedule_at(t0 + Duration::from_millis(500), 1);
        schedule_at(t0 + Duration::from_millis(300), 2);
        let d = next_deadline().unwrap();
        assert_eq!(d, t0 + Duration::from_millis(300));
    }

    #[test]
    fn cancel_removes_discrete() {
        reset();
        clock::install_virtual();
        let t0 = clock::now();
        schedule_at(t0 + Duration::from_millis(500), 1);
        assert!(any_active());
        cancel(1);
        assert!(!any_active());
        assert!(next_deadline().is_none());
    }

    #[test]
    fn continuous_overrides_discrete() {
        reset();
        clock::install_virtual();
        let t0 = clock::now();
        schedule_at(t0 + Duration::from_millis(5000), 1);
        acquire_continuous(42);
        // Continuous returns now, not 5 seconds from now.
        let d = next_deadline().unwrap();
        assert!(d <= clock::now());
    }

    #[test]
    fn release_continuous_restores_discrete() {
        reset();
        clock::install_virtual();
        let t0 = clock::now();
        schedule_at(t0 + Duration::from_millis(500), 1);
        acquire_continuous(42);
        release_continuous(42);
        let d = next_deadline().unwrap();
        assert_eq!(d, t0 + Duration::from_millis(500));
    }

    #[test]
    fn schedule_same_key_replaces() {
        reset();
        clock::install_virtual();
        let t0 = clock::now();
        schedule_at(t0 + Duration::from_millis(500), 1);
        schedule_at(t0 + Duration::from_millis(200), 1); // replaces
        let d = next_deadline().unwrap();
        assert_eq!(d, t0 + Duration::from_millis(200));
    }
}
