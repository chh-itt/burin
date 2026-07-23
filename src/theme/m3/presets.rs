//! M3 preset themes — curated seed colors + DesignParams + SchemeVariant combos.
//!
//! Each [`PresetTheme`] bundles a seed color, design personality, and scheme variant
//! into a single coherent visual identity. Apply via `M3Theme::preset(preset)`.
//!
//! **Community contributors**: add your own presets here and submit a PR.
//! Follow the existing naming convention: `<family>_<variant>()`.

use super::engine::{HctColor, SchemeVariant};
use super::DesignParams;
use crate::style::Color;

// ── PresetTheme ───────────────────────────────────────────────────

/// A complete preset theme — seed color + design personality + scheme variant.
///
/// One call to `M3Theme::preset(PresetTheme::neo_minimal_slate())` gives you
/// a fully resolved, production-ready visual identity.
#[derive(Clone, Debug)]
pub struct PresetTheme {
    pub name: &'static str,
    pub seed: Color,
    pub design: DesignParams,
    pub variant: SchemeVariant,
}

impl PresetTheme {
    // ── Neo-Minimal series (冷调极简主义) ──────────────────────

    /// Cool slate gray — the flagship neo-minimal preset.
    /// Apple-like neutral with a barely-there blue undertone.
    pub fn neo_minimal_slate() -> Self {
        Self {
            name: "Neo-Minimal Slate",
            seed: HctColor::new(240.0, 8.0, 40.0).to_color(),
            design: DesignParams::neo_minimal(),
            variant: SchemeVariant::Neutral,
        }
    }

    /// Morning mist — cool blue-gray, softer than slate.
    pub fn neo_minimal_mist() -> Self {
        Self {
            name: "Neo-Minimal Mist",
            seed: HctColor::new(220.0, 12.0, 42.0).to_color(),
            design: DesignParams::neo_minimal(),
            variant: SchemeVariant::Neutral,
        }
    }

    /// Sage gray — organic cool green-gray, natural but restrained.
    pub fn neo_minimal_sage() -> Self {
        Self {
            name: "Neo-Minimal Sage",
            seed: HctColor::new(150.0, 10.0, 40.0).to_color(),
            design: DesignParams::neo_minimal(),
            variant: SchemeVariant::Neutral,
        }
    }

    /// Twilight dusk — subtle lavender-gray, the warmest cool tone.
    pub fn neo_minimal_dusk() -> Self {
        Self {
            name: "Neo-Minimal Dusk",
            seed: HctColor::new(260.0, 10.0, 38.0).to_color(),
            design: DesignParams::neo_minimal(),
            variant: SchemeVariant::Neutral,
        }
    }

    /// Glacier ice — near-achromatic, the most extreme minimal statement.
    pub fn neo_minimal_ice() -> Self {
        Self {
            name: "Neo-Minimal Ice",
            seed: HctColor::new(200.0, 6.0, 45.0).to_color(),
            design: DesignParams::neo_minimal(),
            variant: SchemeVariant::Neutral,
        }
    }

    /// Warm ash — a warm exit ramp for those who want neo-minimal with warmth.
    pub fn neo_minimal_ash() -> Self {
        Self {
            name: "Neo-Minimal Ash",
            seed: HctColor::new(40.0, 3.0, 40.0).to_color(),
            design: DesignParams::neo_minimal(),
            variant: SchemeVariant::Neutral,
        }
    }

    // ── Classic series ─────────────────────────────────────────

    /// Refined — balanced, warm-neutral, restrained accent.
    /// The default personality of `M3Theme::from_seed()`.
    pub fn refined() -> Self {
        Self {
            name: "Refined",
            seed: Color::rgba8(103, 80, 164, 255),
            design: DesignParams::refined(),
            variant: SchemeVariant::TonalSpot,
        }
    }

    /// M3 Classic — the original Google Material 3 look (high chroma, strong depth).
    pub fn m3_classic() -> Self {
        Self {
            name: "M3 Classic",
            seed: Color::rgba8(103, 80, 164, 255),
            design: DesignParams::m3_classic(),
            variant: SchemeVariant::TonalSpot,
        }
    }
}

// ── Legacy seed color constants (backward-compatible) ─────────────

pub const SEED_PURPLE: Color = Color::rgba8(103, 80, 164, 255);
pub const SEED_BLUE: Color = Color::rgba8(25, 118, 210, 255);
pub const SEED_GREEN: Color = Color::rgba8(46, 125, 50, 255);
pub const SEED_ORANGE: Color = Color::rgba8(237, 108, 2, 255);
pub const SEED_RED: Color = Color::rgba8(179, 38, 30, 255);
pub const SEED_TEAL: Color = Color::rgba8(0, 121, 107, 255);
pub const SEED_CYAN: Color = Color::rgba8(0, 131, 143, 255);

// ── Neo-Minimal seed color constants ──────────────────────────────

pub const SEED_SLATE: Color = Color::rgba8(95, 98, 108, 255);
pub const SEED_MIST: Color = Color::rgba8(95, 103, 115, 255);
pub const SEED_SAGE: Color = Color::rgba8(91, 105, 99, 255);
pub const SEED_DUSK: Color = Color::rgba8(94, 92, 106, 255);
pub const SEED_ICE: Color = Color::rgba8(108, 112, 116, 255);
pub const SEED_ASH: Color = Color::rgba8(101, 99, 93, 255);
