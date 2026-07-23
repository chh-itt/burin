//! Monitor physical properties (size, rotation).
//!
//! ## Platform support
//!
//! | Method | macOS | Windows | Linux |
//! |--------|-------|---------|-------|
//! | `physical_size_mm` | ✅ `CGDisplayScreenSize` | ❌ (EDID) | ❌ (RandR/wl_output) |
//! | `rotation_degrees` | ✅ `CGDisplayRotation` | ❌ (`QueryDisplayConfig`) | ❌ (RandR) |
//!
//! ## Unimplemented items
//!
//! - **Windows `rotation`**: Use `QueryDisplayConfig` → `DISPLAYCONFIG_PATH_TARGET_INFO.rotation`.
//!   Requires matching HMONITOR (from `native_id`) to QDC paths via `GetMonitorInfoW` name matching.
//!   Add `Win32_Devices_Display` feature to `windows-sys` dep.
//!
//! - **Windows `physical_size_mm`**: EDID parsing (bytes 21–22). Read from registry
//!   `HKLM\...\Enum\DISPLAY\...\Device Parameters\EDID` or via `SetupAPI`.
//!   Significantly more complex than rotation.
//!
//! - **Linux `physical_size_mm` + `rotation`**: Use `x11rb` RandR extension
//!   (`RRGetOutputInfo` / `RRGetCrtcInfo`). Add x11rb with `randr` feature as an
//!   optional dep under `[target.'cfg(target_os = "linux")'.dependencies]`.
//!
//! - **Wayland `physical_size_mm`**: `wl_output` protocol event carries physical
//!   dimensions. Rotation is not exposed at the wl_output level on Wayland.
//!   Would need `wayland-client` + `wayland-protocols` deps.

/// Physical size of the display in millimetres.
///
/// Returns `None` on platforms where the information is not available
/// or not yet implemented.
#[allow(unused_variables)]
pub fn physical_size_mm(native_id: u64) -> Option<(u32, u32)> {
    #[cfg(target_os = "macos")]
    {
        let size = unsafe { CGDisplayScreenSize(native_id as u32) };
        if size.width > 0.0 && size.height > 0.0 {
            return Some((size.width as u32, size.height as u32));
        }
    }
    None
}

/// Clockwise rotation of the display in degrees.
///
/// Returns `None` on platforms where the information is not available
/// or not yet implemented.
#[allow(unused_variables)]
pub fn rotation_degrees(native_id: u64) -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        let degrees = unsafe { CGDisplayRotation(native_id as u32) };
        return Some(degrees);
    }
    #[allow(unreachable_code)]
    None
}

// ── macOS FFI ─────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGDisplayScreenSize(display: u32) -> CGSize;
    fn CGDisplayRotation(display: u32) -> f64;
}
