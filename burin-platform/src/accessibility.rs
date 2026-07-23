//! Safe wrapper around `accesskit_macos::SubclassingAdapter` creation.
//!
//! The upstream `SubclassingAdapter::new` is `unsafe` because it requires:
//! - A valid NSView pointer
//! - Must be called from the main thread
//!
//! This wrapper is marked safe because the call site in `burin`
//! guarantees both preconditions (called from a winit event handler
//! on the main thread with a valid view handle from `raw-window-handle`).

#[cfg(all(target_os = "macos", feature = "accessibility"))]
use accesskit::ActionHandler;
#[cfg(all(target_os = "macos", feature = "accessibility"))]
use accesskit::ActivationHandler;
#[cfg(all(target_os = "macos", feature = "accessibility"))]
use accesskit_macos::SubclassingAdapter;

/// Create a macOS subclassing adapter for accessibility.
///
/// # Safety preconditions (guaranteed by caller)
///
/// - `view` must be a valid `NSView` pointer from `raw-window-handle`.
/// - Must be called from the main thread (winit event loop context).
/// - Must be called at most once per process (guard at call site).
#[cfg(all(target_os = "macos", feature = "accessibility"))]
pub fn create_macos_adapter<A: ActivationHandler + 'static, B: ActionHandler + 'static>(
    view: *mut std::ffi::c_void,
    activation: A,
    action: B,
) -> SubclassingAdapter {
    // SAFETY: Caller guarantees valid NSView pointer and main-thread execution.
    // The handler types are zero-sized and their trait implementations are
    // thread-safe (they only queue actions into a Mutex<Vec>).
    unsafe { accesskit_macos::SubclassingAdapter::new(view, activation, action) }
}
