//! System theme auto-detection for M3.
//! Detects OS light/dark preference and generates appropriate M3 schemes
//! from a single seed color.

use crate::style::Color;
use crate::theme::m3::presets::SEED_PURPLE;
use crate::theme::M3Theme;
use auralis_signal::Signal;

/// Create a Signal<M3Theme> that auto-switches based on system light/dark preference.
/// Default seed = M3 Purple.
pub fn auto_theme() -> Signal<M3Theme> {
    auto_theme_with_seed(SEED_PURPLE)
}

/// Create auto-switching theme with a custom seed color.
pub fn auto_theme_with_seed(seed: Color) -> Signal<M3Theme> {
    #[cfg(feature = "system-theme")]
    {
        let is_dark = dark_light::detect()
            .map(|m| m == dark_light::Mode::Dark)
            .unwrap_or(false);
        Signal::new(M3Theme::from_seed(seed).with_is_dark(is_dark))
    }
    #[cfg(not(feature = "system-theme"))]
    {
        Signal::new(M3Theme::from_seed(seed))
    }
}
