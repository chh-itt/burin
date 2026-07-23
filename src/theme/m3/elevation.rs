//! M3 Elevation system — 6 levels (0-5).
//! Elevation changes surface color AND shadow, but NEVER corner_radius.

use crate::style::styled::Shadow;
use crate::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElevationLevel {
    Level0,
    Level1,
    Level2,
    Level3,
    Level4,
    Level5,
}

impl ElevationLevel {
    /// Shadow parameters per M3 spec, scaled by depth_scale (0.0 = flat, 1.0 = full M3).
    pub fn shadow(&self, is_dark: bool, depth_scale: f32) -> Shadow {
        let s = depth_scale.clamp(0.0, 1.0);
        let (offset_y, blur, intensity) = match self {
            ElevationLevel::Level0 => (0.0_f32, 0.0_f32, 0.0_f32),
            ElevationLevel::Level1 => (1.0_f32 * s, 3.0_f32 * s, 0.05_f32 * s),
            ElevationLevel::Level2 => (2.0_f32 * s, 6.0_f32 * s, 0.08_f32 * s),
            ElevationLevel::Level3 => (4.0_f32 * s, 12.0_f32 * s, 0.10_f32 * s),
            ElevationLevel::Level4 => (6.0_f32 * s, 24.0_f32 * s, 0.12_f32 * s),
            ElevationLevel::Level5 => (8.0_f32 * s, 40.0_f32 * s, 0.15_f32 * s),
        };
        let alpha = if is_dark {
            (intensity * 1.5_f32).min(1.0_f32)
        } else {
            intensity
        };
        Shadow {
            color: Color::BLACK.with_alpha(alpha),
            offset_x: 0.0,
            offset_y,
            blur,
        }
    }
}
