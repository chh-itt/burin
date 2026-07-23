// ═══════════════════════ Scroll physics ═══════════════════════

pub(crate) const VELOCITY_HISTORY_MAX_MS: u128 = 120;
/// Pixels per mouse wheel notch (LineDelta). One notch ≈ 3 lines at ~13px/line on Windows.
pub(crate) const WHEEL_PIXELS_PER_LINE: f32 = 13.0;
pub(crate) const WHEEL_LINES_PER_NOTCH: f32 = 3.0;

pub(crate) use crate::core::frame_driver::ScrollKinetic;
