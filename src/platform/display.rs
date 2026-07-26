//! Display/monitor information.
//!
//! Provides [`MonitorHandle`] and [`VideoMode`] types wrapping winit's
//! monitor API. No extra dependencies beyond winit.
//!
//! ## Entry points
//!
//! - `WindowHandle::current_monitor` — the monitor the window is on
//! - `WindowHandle::available_monitors` — all connected monitors
//! - `WindowHandle::primary_monitor` — the primary monitor

use std::fmt;

/// Handle to a connected display monitor.
///
/// Obtained via `WindowHandle::current_monitor`,
/// `WindowHandle::available_monitors`, or
/// `WindowHandle::primary_monitor`.
///
/// Comparison is based on per-session identity (internal platform handle).
/// For cross-session persistence, use [`MonitorHandle::name`] + [`MonitorHandle::size`]
/// as a composite key.
#[derive(Clone)]
pub struct MonitorHandle(pub(crate) winit::monitor::MonitorHandle);

/// A supported video mode (resolution + refresh rate + bit depth).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoMode {
    size: (u32, u32),
    refresh_rate_millihertz: Option<u32>,
    bit_depth: Option<u16>,
}

// ── MonitorHandle ───────────────────────────────────────────────────────

impl MonitorHandle {
    /// Human-readable monitor name (e.g. `\\.\DISPLAY1` on Windows).
    /// Returns `None` if the name cannot be obtained.
    pub fn name(&self) -> Option<String> {
        self.0.name().map(|c| c.to_string())
    }

    /// Top-left corner of the monitor in desktop (virtual) coordinates.
    pub fn position(&self) -> Option<(i32, i32)> {
        self.0.position().map(|p| (p.x, p.y))
    }

    /// DPI scale factor of the monitor.
    pub fn scale_factor(&self) -> f64 {
        self.0.scale_factor()
    }

    /// Currently active video mode.
    pub fn current_video_mode(&self) -> Option<VideoMode> {
        self.0.current_video_mode().map(VideoMode::from_winit)
    }

    /// All supported fullscreen video modes.
    pub fn video_modes(&self) -> Vec<VideoMode> {
        self.0.video_modes().map(VideoMode::from_winit).collect()
    }

    /// Current refresh rate in millihertz (e.g. 60 000 = 60 Hz).
    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        self.current_video_mode()?.refresh_rate_millihertz
    }

    /// Current resolution as `(width, height)` in physical pixels.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.current_video_mode().map(|m| m.size)
    }

    /// Returns `true` if this monitor is the primary monitor.
    ///
    /// `primary` should be obtained from `WindowHandle::primary_monitor`.
    pub fn is_primary(&self, primary: &MonitorHandle) -> bool {
        self.0.eq(&primary.0)
    }

    pub(crate) fn inner(&self) -> &winit::monitor::MonitorHandle {
        &self.0
    }

    /// Platform-native identifier (used internally by advanced display features).
    #[cfg(feature = "display-advanced")]
    pub(crate) fn native_id(&self) -> u64 {
        self.0.native_id()
    }
}

impl PartialEq for MonitorHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl Eq for MonitorHandle {}

impl fmt::Debug for MonitorHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MonitorHandle")
            .field("name", &self.name())
            .field("position", &self.position())
            .field("scale_factor", &self.scale_factor())
            .field("size", &self.size())
            .finish_non_exhaustive()
    }
}

// ── VideoMode ───────────────────────────────────────────────────────────

impl VideoMode {
    pub(crate) fn from_winit(m: winit::monitor::VideoMode) -> Self {
        Self {
            size: (m.size().width, m.size().height),
            refresh_rate_millihertz: m.refresh_rate_millihertz().map(|r| r.get()),
            bit_depth: m.bit_depth().map(|b| b.get()),
        }
    }

    /// Resolution in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Refresh rate in millihertz (e.g. 60 000 = 60 Hz).
    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        self.refresh_rate_millihertz
    }

    /// Refresh rate in hertz (e.g. 60.0).
    pub fn refresh_rate_hz(&self) -> Option<f64> {
        self.refresh_rate_millihertz.map(|mhz| mhz as f64 / 1000.0)
    }

    /// Color bit depth (e.g. 24, 32). Returns `None` if unavailable.
    pub fn bit_depth(&self) -> Option<u16> {
        self.bit_depth
    }
}

impl fmt::Display for VideoMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.size.0, self.size.1)?;
        if let Some(mhz) = self.refresh_rate_millihertz {
            write!(f, " @ {:.1} Hz", mhz as f64 / 1000.0)?;
        }
        if let Some(bd) = self.bit_depth {
            write!(f, " ({} bpp)", bd)?;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Advanced display features (feature = "display-advanced")
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "display-advanced")]
pub use advanced::*;

#[cfg(feature = "display-advanced")]
mod advanced {
    use std::fmt;
    use thiserror::Error;

    // ── DisplayError ────────────────────────────────────────────────────

    /// Errors from advanced display operations.
    #[derive(Error, Debug, Clone, PartialEq, Eq)]
    #[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
    pub enum DisplayError {
        /// Feature is not available on this platform.
        #[error("display feature not available on this platform")]
        Unsupported,
        /// Underlying platform API call failed.
        #[error("display operation failed: {0}")]
        Platform(String),
    }

    impl From<String> for DisplayError {
        fn from(msg: String) -> Self {
            DisplayError::Platform(msg)
        }
    }

    // ── Rotation ─────────────────────────────────────────────────────────

    /// Physical orientation of a monitor.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Rotation {
        /// No rotation (landscape).
        Deg0,
        /// 90° clockwise (portrait).
        Deg90,
        /// 180° (upside-down).
        Deg180,
        /// 270° clockwise (portrait reversed).
        Deg270,
    }

    impl Rotation {
        fn from_degrees(d: f64) -> Option<Self> {
            match d as i32 {
                0 => Some(Self::Deg0),
                90 => Some(Self::Deg90),
                180 => Some(Self::Deg180),
                270 => Some(Self::Deg270),
                _ => None,
            }
        }
    }

    // ── ScreensaverInhibit ───────────────────────────────────────────────

    /// RAII guard that prevents the system screensaver from activating.
    ///
    /// The screensaver is re-enabled when the guard is dropped.
    ///
    /// ## Platform support
    ///
    /// | Platform | Mechanism |
    /// |----------|-----------|
    /// | Windows  | `SetThreadExecutionState(ES_DISPLAY_REQUIRED)` |
    /// | macOS    | `IOPMAssertionCreateWithName(NoDisplaySleepAssertion)` |
    /// | Linux    | D-Bus `org.freedesktop.ScreenSaver.Inhibit` |
    /// | Other    | Unsupported (`ScreensaverInhibit::new()` returns `Err`) |
    #[must_use]
    #[allow(dead_code)]
    pub struct ScreensaverInhibit(burin_platform::ScreensaverInhibit);

    impl ScreensaverInhibit {
        /// Create a screensaver inhibition guard.
        ///
        /// Delegates to the `burin-platform` crate for the actual
        /// platform-specific implementation.
        pub fn new() -> Result<Self, DisplayError> {
            burin_platform::ScreensaverInhibit::new()
                .map(Self)
                .map_err(DisplayError::from)
        }
    }

    impl fmt::Debug for ScreensaverInhibit {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ScreensaverInhibit").finish_non_exhaustive()
        }
    }

    // ── MonitorHandle advanced methods ───────────────────────────────────

    impl super::MonitorHandle {
        /// Physical size of the display panel in millimetres.
        ///
        /// ## Platform support
        ///
        /// | Platform | Status |
        /// |----------|--------|
        /// | macOS    | ✅ `CGDisplayScreenSize` |
        /// | Windows  | ❌ Requires EDID parsing — not yet implemented |
        /// | Linux    | ❌ Requires RandR / wl_output — not yet implemented |
        pub fn physical_size_mm(&self) -> Option<(u32, u32)> {
            burin_platform::display::physical_size_mm(self.native_id() as u64)
        }

        /// Physical orientation of the monitor.
        ///
        /// ## Platform support
        ///
        /// | Platform | Status |
        /// |----------|--------|
        /// | macOS    | ✅ `CGDisplayRotation` |
        /// | Windows  | ❌ Requires `QueryDisplayConfig` — not yet implemented |
        /// | Linux    | ❌ Requires RandR / wl_output — not yet implemented |
        pub fn rotation(&self) -> Option<Rotation> {
            burin_platform::display::rotation_degrees(self.native_id() as u64)
                .and_then(Rotation::from_degrees)
        }
    }
}
