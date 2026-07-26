//! Cross-thread UI wake and callback dispatch.
//!
//! Layer 1 — EventLoopProxy globalisation:
//!   On startup, `App::run()` stores a winit `EventLoopProxy` so that any
//!   thread can call [`wake_ui()`] to wake the event loop.
//!
//! Layer 2 — `run_on_ui` / `spawn_background` (see [`crate::task`]):
//!   [`run_on_ui`] pushes a `Send + 'static` closure into a global queue
//!   and wakes the UI.  The closure is drained and executed on the UI
//!   thread during the next event-loop cycle.  This is how background
//!   threads safely update signals without touching `thread_local!` state.
//!
//! Layer 3 — automatic wake dedup:
//!   An [`AtomicBool`] keeps track of whether a `wake_up()` call is already
//!   in-flight.  Repeated calls to [`wake_ui()`] are no-ops until the UI
//!   thread drains the pending work, avoiding `wake_up()` storms.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use winit::event_loop::EventLoopProxy;

/// The singleton winit proxy.  Set once by `App::run()` before the event
/// loop starts and never cleared.
static UI_PROXY: OnceLock<EventLoopProxy> = OnceLock::new();

/// Set by [`wake_ui`] to `true` when a wake-up has been requested but not
/// yet consumed.  Switched back to `false` by [`drain_ui_queue`].
static WAKE_SENT: AtomicBool = AtomicBool::new(false);

/// Pending closures enqueued via [`run_on_ui`] from any thread.
static UI_QUEUE: Mutex<Vec<Box<dyn FnOnce() + Send>>> = Mutex::new(Vec::new());

// ── Initialisation ─────────────────────────────────────────────────

/// Called once by `App::run()`.  Safe to call multiple times; subsequent
/// calls are ignored.
pub(crate) fn set_ui_proxy(proxy: EventLoopProxy) {
    let _ = UI_PROXY.set(proxy);
}

// ── Public API ─────────────────────────────────────────────────────

/// Wake the event loop from any thread.
///
/// Idempotent — if a wake-up is already pending the call is a no-op.
/// This is the primitive that makes cross-thread signal updates
/// responsive; call it after pushing work via [`run_on_ui`].
///
/// See also [`run_on_ui`] which calls `wake_ui()` automatically.
pub fn wake_ui() {
    if WAKE_SENT.swap(true, Ordering::AcqRel) {
        return; // already pending
    }
    if let Some(proxy) = UI_PROXY.get() {
        proxy.wake_up();
    }
}

/// Push a closure onto the global UI queue and wake the event loop.
///
/// The closure will be executed on the UI thread during the next
/// event-loop drain cycle (see `drain_ui_queue`).  All of the
/// framework's `thread_local!` state is available to the closure.
///
/// # Example
///
/// ```ignore
/// std::thread::spawn(move || {
///     let data = compute();
///     burin::platform::wake::run_on_ui(move || {
///         my_signal.set(data); // safe — we are on the UI thread
///     });
/// });
/// ```
pub fn run_on_ui(f: impl FnOnce() + Send + 'static) {
    UI_QUEUE.lock().unwrap().push(Box::new(f));
    wake_ui();
}

// ── Internal: drain (called from the event loop) ───────────────────

/// Drain all pending `run_on_ui` closures on the UI thread.
///
/// Called once per event-loop cycle from `App::about_to_wait`, before
/// `flush_scheduler.drain()`.  Clears the `WAKE_SENT` flag so that
/// subsequent `wake_ui()` calls can trigger a new wake-up cycle.
pub(crate) fn drain_ui_queue() {
    WAKE_SENT.store(false, Ordering::Release);
    let fns: Vec<Box<dyn FnOnce() + Send>> = {
        let mut q = UI_QUEUE.lock().unwrap();
        if q.is_empty() {
            return;
        }
        std::mem::take(&mut *q)
    };
    for f in fns {
        f();
    }
}

/// True if the global UI queue holds pending work.
#[allow(dead_code)]
pub(crate) fn has_pending_ui_work() -> bool {
    !UI_QUEUE.lock().unwrap().is_empty()
}

/// Reset global state (for parallel test isolation).
#[doc(hidden)]
pub fn reset_state() {
    WAKE_SENT.store(false, Ordering::Release);
    UI_QUEUE.lock().unwrap().clear();
}
