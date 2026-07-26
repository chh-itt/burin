//! HCT color engine — thin wrapper around material-colors.
//!
//! Exposes the subset of the material-colors API that the theme
//! system needs: sRGB ↔ HCT conversion and DynamicScheme generation.

use material_colors::color::Argb;

use crate::style::Color;

fn color_to_argb(c: Color) -> Argb {
    let a = (c.a * 255.0).round() as u8;
    if a == 0 {
        return Argb::new(0, 0, 0, 0);
    }
    let r = ((c.r / c.a).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = ((c.g / c.a).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = ((c.b / c.a).clamp(0.0, 1.0) * 255.0).round() as u8;
    Argb::new(a, r, g, b)
}

fn argb_to_color(argb: Argb) -> Color {
    let a = argb.alpha as f32 / 255.0;
    let r = argb.red as f32 / 255.0;
    let g = argb.green as f32 / 255.0;
    let b = argb.blue as f32 / 255.0;
    Color {
        r: r * a,
        g: g * a,
        b: b * a,
        a,
    }
}

/// Wrapper around material_colors::hct::Hct
#[derive(Clone, Copy, Debug)]
pub struct HctColor {
    pub hue: f64,
    pub chroma: f64,
    pub tone: f64,
}

impl HctColor {
    /// Convert sRGB Color to HCT.
    pub fn from_color(c: Color) -> Self {
        let argb = color_to_argb(c);
        let hct = material_colors::hct::Hct::new(argb);
        Self {
            hue: hct.get_hue(),
            chroma: hct.get_chroma(),
            tone: hct.get_tone(),
        }
    }

    /// Create HCT from hue, chroma, tone.
    pub fn new(hue: f64, chroma: f64, tone: f64) -> Self {
        Self { hue, chroma, tone }
    }

    /// Convert HCT back to sRGB Color.
    pub fn to_color(self) -> Color {
        let hct = material_colors::hct::Hct::from(self.hue, self.chroma, self.tone);
        argb_to_color(hct.into())
    }
}

/// Which Material 3 scheme variant to generate.
///
/// `TonalSpot` = classic M3 (medium chroma, warm-biased).
/// `Neutral`    = near-desaturated palette (chroma ~2-16), ideal for minimal/soft designs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemeVariant {
    TonalSpot,
    Neutral,
}

/// Generate a full Material 3 dynamic color scheme from a seed color.
pub struct SchemeGen;

impl SchemeGen {
    /// Generate light scheme from seed (backward-compatible, defaults to TonalSpot).
    pub fn light(seed: Color) -> material_colors::dynamic_color::DynamicScheme {
        Self::generate(seed, false, SchemeVariant::TonalSpot)
    }

    /// Generate dark scheme from seed (backward-compatible, defaults to TonalSpot).
    pub fn dark(seed: Color) -> material_colors::dynamic_color::DynamicScheme {
        Self::generate(seed, true, SchemeVariant::TonalSpot)
    }

    /// Generate scheme with explicit variant selection.
    pub fn generate(
        seed: Color,
        is_dark: bool,
        variant: SchemeVariant,
    ) -> material_colors::dynamic_color::DynamicScheme {
        let hct = material_colors::hct::Hct::new(color_to_argb(seed));
        match variant {
            SchemeVariant::TonalSpot => {
                material_colors::scheme::variant::SchemeTonalSpot::new(hct, is_dark, None).scheme
            }
            SchemeVariant::Neutral => {
                material_colors::scheme::variant::SchemeNeutral::new(hct, is_dark, None).scheme
            }
        }
    }
}

/// Convert a material-colors Argb to our premultiplied Color.
pub(crate) fn scheme_color_to_rgba(argb: Argb) -> Color {
    argb_to_color(argb)
}
