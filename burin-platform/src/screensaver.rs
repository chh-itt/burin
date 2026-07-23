//! Screensaver inhibition via platform-specific APIs.
//!
//! ## Platform support
//!
//! | Platform | Mechanism |
//! |----------|-----------|
//! | Windows  | `SetThreadExecutionState(ES_DISPLAY_REQUIRED \| ES_CONTINUOUS)` |
//! | macOS    | `IOPMAssertionCreateWithName(NoDisplaySleepAssertion)` |
//! | Linux    | D-Bus `org.freedesktop.ScreenSaver.Inhibit` |
//! | Other    | Unsupported |

use std::fmt;

#[cfg(target_os = "macos")]
use core_foundation::base::TCFType;

// ── ScreensaverInhibit ────────────────────────────────────────────────

/// RAII guard that prevents the system screensaver from activating.
///
/// The screensaver is re-enabled when the guard is dropped.
///
/// # Errors
///
/// Returns `Err(error_description)` if the platform API call fails.
#[must_use]
pub struct ScreensaverInhibit {
    inner: ScreensaverImpl,
}

enum ScreensaverImpl {
    #[cfg(target_os = "windows")]
    Windows,
    #[cfg(target_os = "macos")]
    MacOS(core_foundation::base::CFIndex),
    #[cfg(target_os = "linux")]
    Linux {
        conn: zbus::blocking::Connection,
        cookie: u32,
    },
}

impl ScreensaverInhibit {
    /// Create a screensaver inhibition guard.
    pub fn new() -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            let prev = unsafe {
                windows_sys::Win32::System::Power::SetThreadExecutionState(
                    windows_sys::Win32::System::Power::ES_DISPLAY_REQUIRED
                        | windows_sys::Win32::System::Power::ES_CONTINUOUS,
                )
            };
            if prev == 0 {
                return Err("SetThreadExecutionState failed".into());
            }
            return Ok(Self {
                inner: ScreensaverImpl::Windows,
            });
        }

        #[cfg(target_os = "macos")]
        {
            let assertion_type = core_foundation::string::CFString::new("NoDisplaySleepAssertion");
            let reason = core_foundation::string::CFString::new("burin application");
            let mut assertion_id: core_foundation::base::CFIndex = 0;

            let result = unsafe {
                IOPMAssertionCreateWithName(
                    assertion_type.as_concrete_TypeRef(),
                    255, // kIOPMAssertionLevelOn
                    reason.as_concrete_TypeRef(),
                    &mut assertion_id,
                )
            };
            if result != 0 {
                return Err(format!("IOPMAssertionCreateWithName returned {result}"));
            }
            return Ok(Self {
                inner: ScreensaverImpl::MacOS(assertion_id),
            });
        }

        #[cfg(target_os = "linux")]
        {
            let conn = zbus::blocking::Connection::session()
                .map_err(|e| format!("D-Bus connection failed: {e}"))?;
            let reply = conn
                .call_method(
                    Some("org.freedesktop.ScreenSaver"),
                    "/org/freedesktop/ScreenSaver",
                    Some("org.freedesktop.ScreenSaver"),
                    "Inhibit",
                    &("burin", "Application requested screensaver inhibition"),
                )
                .map_err(|e| format!("D-Bus Inhibit failed: {e}"))?;
            let cookie: u32 = reply
                .body()
                .deserialize()
                .map_err(|e| format!("D-Bus Inhibit bad reply: {e}"))?;
            return Ok(Self {
                inner: ScreensaverImpl::Linux { conn, cookie },
            });
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err("screensaver inhibit not supported on this platform".into())
        }
    }
}

impl Drop for ScreensaverInhibit {
    fn drop(&mut self) {
        match &self.inner {
            #[cfg(target_os = "windows")]
            ScreensaverImpl::Windows => unsafe {
                windows_sys::Win32::System::Power::SetThreadExecutionState(
                    windows_sys::Win32::System::Power::ES_CONTINUOUS,
                );
            },
            #[cfg(target_os = "macos")]
            ScreensaverImpl::MacOS(id) => {
                let _ = unsafe { IOPMAssertionRelease(*id) };
            }
            #[cfg(target_os = "linux")]
            ScreensaverImpl::Linux { conn, cookie } => {
                if let Err(e) = conn.call_method(
                    Some("org.freedesktop.ScreenSaver"),
                    "/org/freedesktop/ScreenSaver",
                    Some("org.freedesktop.ScreenSaver"),
                    "UnInhibit",
                    &(*cookie,),
                ) {
                    // Silently ignored — Drop must not panic.
                    let _ = e;
                }
            }
        }
    }
}

impl fmt::Debug for ScreensaverInhibit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScreensaverInhibit").finish_non_exhaustive()
    }
}

// ── macOS FFI ─────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: core_foundation::string::CFStringRef,
        assertion_level: core_foundation::base::CFIndex,
        reason: core_foundation::string::CFStringRef,
        assertion_id: *mut core_foundation::base::CFIndex,
    ) -> i32;

    fn IOPMAssertionRelease(assertion_id: core_foundation::base::CFIndex) -> i32;
}
