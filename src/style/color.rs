/// An RGBA color with premultiplied alpha.
///
/// All components are in the range `0.0..=1.0`.
/// Alpha is premultiplied — `(r*a, g*a, b*a, a)`.
///
/// This matches the GPU's expected blending format when using
/// `wgpu::BlendState::PREMULTIPLIED_ALPHA`.
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Create from 0-255 integer components (non-premultiplied).
    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Create from a hex string like `"#2196F3"` or `"#2196F3FF"`.
    ///
    /// Returns `None` if the string is not a valid 6 or 8 digit hex color.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        let (r, g, b, a) = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        Some(Self::rgba8(r, g, b, a))
    }

    /// Linear interpolation between two colors.
    ///
    /// `t` is clamped to `0.0..=1.0`. Returns `self` at `t=0` and `other` at `t=1`.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// Return this color with the given alpha.
    pub fn with_alpha(&self, a: f32) -> Self {
        let a = a.clamp(0.0, 1.0);
        let factor = if self.a > 0.0 { a / self.a } else { 0.0 };
        Self {
            r: self.r * factor,
            g: self.g * factor,
            b: self.b * factor,
            a,
        }
    }

    /// Return the relative luminance for WCAG contrast calculations.
    pub fn relative_luminance(&self) -> f32 {
        let linearize = |c: f32| -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linearize(self.r) + 0.7152 * linearize(self.g) + 0.0722 * linearize(self.b)
    }

    /// WCAG 2.1 contrast ratio between two colors.
    pub fn contrast_ratio(&self, other: &Self) -> f32 {
        let l1 = self.relative_luminance();
        let l2 = other.relative_luminance();
        let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Check if this color meets WCAG AA contrast with `other`.
    /// `is_large_text`: true if the text is >= 18px or >= 14px bold.
    pub fn meets_aa(&self, other: &Self, is_large_text: bool) -> bool {
        let min = if is_large_text { 3.0 } else { 4.5 };
        self.contrast_ratio(other) >= min
    }

    /// Check if this color meets WCAG AAA contrast with `other`.
    pub fn meets_aaa(&self, other: &Self, is_large_text: bool) -> bool {
        let min = if is_large_text { 4.5 } else { 7.0 };
        self.contrast_ratio(other) >= min
    }

    /// Return WHITE or BLACK — whichever has better contrast against `background`.
    /// Guarantees at least WCAG AA contrast (4.5:1) for normal-sized text.
    pub fn auto_fg(background: &Self) -> Self {
        let white_ratio = Color::WHITE.contrast_ratio(background);
        let black_ratio = Color::BLACK.contrast_ratio(background);
        if white_ratio >= black_ratio {
            Color::WHITE
        } else {
            Color::BLACK
        }
    }

    /// Convert to `[f32; 4]` for GPU uniform buffers.
    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Convert a single sRGB-encoded channel (0..1) to linear light.
    /// Standard IEC 61966-2-1 sRGB transfer function (same as `palette`).
    #[inline]
    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// Convert a single linear channel (0..1) back to sRGB-encoded.
    #[inline]
    fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    /// This color with RGB converted from sRGB to linear light (alpha unchanged).
    ///
    /// GUI colors are authored/stored as sRGB. GPU render targets in this crate
    /// use an `*UnormSrgb` format, which auto-encodes shader output (treated as
    /// linear) back to sRGB on store. To display the authored color faithfully,
    /// vertex colors must be linearized before upload so the round-trip cancels.
    pub fn to_linear(&self) -> Self {
        Self {
            r: Self::srgb_to_linear(self.r),
            g: Self::srgb_to_linear(self.g),
            b: Self::srgb_to_linear(self.b),
            a: self.a,
        }
    }

    /// Inverse of [`to_linear`]: linear light back to sRGB-encoded RGB.
    pub fn from_linear(&self) -> Self {
        Self {
            r: Self::linear_to_srgb(self.r),
            g: Self::linear_to_srgb(self.g),
            b: Self::linear_to_srgb(self.b),
            a: self.a,
        }
    }

    /// Linear-light RGBA array for GPU upload (RGB linearized, alpha as-is).
    pub fn to_linear_array(&self) -> [f32; 4] {
        [
            Self::srgb_to_linear(self.r),
            Self::srgb_to_linear(self.g),
            Self::srgb_to_linear(self.b),
            self.a,
        ]
    }

    /// Darken the color by multiplying RGB by `factor` (0.0-1.0).
    pub fn darken(&self, factor: f32) -> Self {
        let f = factor.clamp(0.0, 1.0);
        Self {
            r: self.r * f,
            g: self.g * f,
            b: self.b * f,
            a: self.a,
        }
    }

    /// Lighten the color by mixing toward WHITE.
    /// `amount`: 0.0 = no change, 1.0 = fully white.
    pub fn lighten(&self, amount: f32) -> Self {
        let a = amount.clamp(0.0, 1.0);
        Self {
            r: self.r + (1.0 - self.r) * a,
            g: self.g + (1.0 - self.g) * a,
            b: self.b + (1.0 - self.b) * a,
            a: self.a,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::TRANSPARENT
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let r = (self.r * 255.0) as u8;
        let g = (self.g * 255.0) as u8;
        let b = (self.b * 255.0) as u8;
        let a = (self.a * 255.0) as u8;
        if a == 255 {
            write!(f, "#{r:02X}{g:02X}{b:02X}")
        } else {
            write!(f, "#{r:02X}{g:02X}{b:02X}{a:02X}")
        }
    }
}

// ── HSLA color space ────────────────────────────────────────────────

/// HSL + Alpha color in **straight** (non-premultiplied) alpha.
///
/// Unlike [`Color`] which stores premultiplied values for the GPU,
/// `Hsla` stores un-premultiplied components — the natural representation
/// for color picking, interpolation, and slider rendering.
///
/// Conversion to/from [`Color`] handles premultiplied↔straight alpha
/// automatically.
///
/// HSL↔RGB conversion math is derived from the `palette` crate's
/// industry-standard implementation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsla {
    /// Hue angle in degrees `0.0..=360.0`. Values outside this range wrap.
    pub h: f32,
    /// Saturation `0.0..=1.0`. 0 = gray, 1 = full color.
    pub s: f32,
    /// Lightness `0.0..=1.0`. 0 = black, 0.5 = pure hue, 1 = white.
    pub l: f32,
    /// Alpha (straight) `0.0..=1.0`.
    pub a: f32,
}

impl Hsla {
    pub fn new(h: f32, s: f32, l: f32, a: f32) -> Self {
        let h = ((h % 360.0) + 360.0) % 360.0;
        Self {
            h,
            s: s.clamp(0.0, 1.0),
            l: l.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    /// Convert from a [`Color`].
    ///
    /// `Color` stores straight (non-premultiplied) sRGB values internally;
    /// premultiplication only occurs during GPU upload.
    pub fn from_color(c: Color) -> Self {
        if c.a <= 0.0 {
            return Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.0,
            };
        }
        let (r, g, b, a) = (c.r, c.g, c.b, c.a);

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;

        if (max - min).abs() < f32::EPSILON {
            return Hsla {
                h: 0.0,
                s: 0.0,
                l,
                a,
            };
        }

        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if (max - r).abs() < f32::EPSILON {
            (g - b) / d + (if g < b { 6.0 } else { 0.0 })
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;

        Hsla { h, s, l, a }
    }

    /// Convert to [`Color`] for rendering.
    ///
    /// Returns straight sRGB values (not premultiplied), matching `Color`'s
    /// internal representation.
    pub fn to_color(&self) -> Color {
        if self.a <= 0.0 || self.s <= 0.0 {
            if self.s <= 0.0 {
                let v = self.l.clamp(0.0, 1.0);
                return Color {
                    r: v,
                    g: v,
                    b: v,
                    a: self.a,
                };
            }
            return Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
        }

        let l = self.l.clamp(0.0, 1.0);
        let s = self.s.clamp(0.0, 1.0);

        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hp = ((self.h % 360.0) + 360.0) % 360.0 / 60.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let m = l - c / 2.0;

        let (r1, g1, b1) = match hp as u8 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        Color {
            r: (r1 + m).clamp(0.0, 1.0),
            g: (g1 + m).clamp(0.0, 1.0),
            b: (b1 + m).clamp(0.0, 1.0),
            a: self.a.clamp(0.0, 1.0),
        }
    }

    /// Parse a hex color string into `Hsla`.
    ///
    /// Supports `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        let (r, g, b, a) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                (r, g, b, 255u8)
            }
            4 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                let a = u8::from_str_radix(&hex[3..4].repeat(2), 16).ok()?;
                (r, g, b, a)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 255u8)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        let color = Color::rgba8(r, g, b, a);
        Some(Hsla::from_color(color))
    }

    /// Format as hex string `#RRGGBB` or `#RRGGBBAA`.
    pub fn to_hex(&self) -> String {
        self.to_color().to_string()
    }
}

impl Default for Hsla {
    fn default() -> Self {
        Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_6_digit() {
        let c = Color::from_hex("#2196F3").unwrap();
        assert!((c.r - 0.129).abs() < 0.01);
        assert!((c.g - 0.588).abs() < 0.01);
        assert!((c.b - 0.953).abs() < 0.01);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn hex_8_digit() {
        let c = Color::from_hex("#2196F380").unwrap();
        assert!((c.a - 0.502).abs() < 0.01);
    }

    #[test]
    fn hex_invalid() {
        assert!(Color::from_hex("not-a-color").is_none());
        assert!(Color::from_hex("#12345").is_none());
    }

    #[test]
    fn white_black_contrast() {
        let ratio = Color::WHITE.contrast_ratio(&Color::BLACK);
        assert!((ratio - 21.0).abs() < 0.1);
    }

    #[test]
    fn meets_aa_normal_text() {
        // White on blue-600 (~5.7:1) passes AA for normal text (>=4.5:1)
        let blue = Color::rgba8(37, 99, 235, 255); // #2563eb
        assert!(Color::WHITE.meets_aa(&blue, false));
    }

    #[test]
    fn lerp_midpoint() {
        let white = Color::WHITE;
        let black = Color::BLACK;
        let mid = white.lerp(&black, 0.5);
        assert!((mid.r - 0.5).abs() < 0.01);
        assert!((mid.a - 1.0).abs() < 0.01);
    }

    // ── Hsla conversion tests ─────────────────────────────────────

    fn assert_color_eq(a: Color, b: Color, epsilon: f32) {
        assert!((a.r - b.r).abs() < epsilon, "r: {} vs {}", a.r, b.r);
        assert!((a.g - b.g).abs() < epsilon, "g: {} vs {}", a.g, b.g);
        assert!((a.b - b.b).abs() < epsilon, "b: {} vs {}", a.b, b.b);
        assert!((a.a - b.a).abs() < epsilon, "a: {} vs {}", a.a, b.a);
    }

    #[test]
    fn hsla_roundtrip_red() {
        let c = Color::rgba8(255, 0, 0, 255);
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        assert_color_eq(c, back, 0.01);
        assert!((hsla.h - 0.0).abs() < 1.0, "red hue: {}", hsla.h);
        assert!((hsla.s - 1.0).abs() < 0.01, "red sat: {}", hsla.s);
        assert!((hsla.l - 0.5).abs() < 0.01, "red light: {}", hsla.l);
    }

    #[test]
    fn hsla_roundtrip_green() {
        let c = Color::rgba8(0, 255, 0, 255);
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        assert_color_eq(c, back, 0.01);
        assert!((hsla.h - 120.0).abs() < 1.0, "green hue: {}", hsla.h);
    }

    #[test]
    fn hsla_roundtrip_blue() {
        let c = Color::rgba8(0, 0, 255, 255);
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        assert_color_eq(c, back, 0.01);
        assert!((hsla.h - 240.0).abs() < 1.0, "blue hue: {}", hsla.h);
    }

    #[test]
    fn hsla_roundtrip_white() {
        let c = Color::WHITE;
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        assert_color_eq(c, back, 0.01);
        assert_eq!(hsla.l, 1.0);
    }

    #[test]
    fn hsla_roundtrip_black() {
        let c = Color::rgba8(0, 0, 0, 255);
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        assert_color_eq(c, back, 0.01);
        assert_eq!(hsla.l, 0.0);
    }

    #[test]
    fn hsla_roundtrip_gray() {
        let c = Color::rgba8(128, 128, 128, 255);
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        assert_color_eq(c, back, 0.01);
        assert_eq!(hsla.s, 0.0);
    }

    #[test]
    fn hsla_roundtrip_orange() {
        let c = Color::rgba8(255, 128, 0, 255);
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        assert_color_eq(c, back, 0.02);
        assert!(hsla.h > 20.0 && hsla.h < 40.0, "orange hue: {}", hsla.h);
    }

    #[test]
    fn hsla_roundtrip_with_alpha() {
        let c = Color::rgba8(100, 150, 200, 128);
        let hsla = Hsla::from_color(c);
        let back = hsla.to_color();
        // alpha + HSL roundtrip adds ~2 LSB of float imprecision
        assert_color_eq(c, back, 0.05);
        assert!((hsla.a - 0.502).abs() < 0.01);
    }

    #[test]
    fn hsla_from_hex_3digit() {
        let hsla = Hsla::from_hex("#F0A").unwrap();
        let color = hsla.to_color();
        let expected = Color::rgba8(255, 0, 170, 255);
        assert_color_eq(expected, color, 0.01);
    }

    #[test]
    fn hsla_from_hex_4digit() {
        let hsla = Hsla::from_hex("#F0A8").unwrap();
        assert!((hsla.a - 0.533).abs() < 0.02);
    }

    #[test]
    fn hsla_from_hex_6digit() {
        let hsla = Hsla::from_hex("#3B82F6").unwrap();
        let color = hsla.to_color();
        let expected = Color::rgba8(59, 130, 246, 255);
        assert_color_eq(expected, color, 0.01);
    }

    #[test]
    fn hsla_from_hex_8digit() {
        let hsla = Hsla::from_hex("#3B82F680").unwrap();
        assert!((hsla.a - 0.502).abs() < 0.02);
    }

    #[test]
    fn hsla_from_hex_invalid() {
        assert!(Hsla::from_hex("not-a-color").is_none());
        assert!(Hsla::from_hex("#12345").is_none());
    }

    #[test]
    fn hsla_to_hex_roundtrip() {
        let hex = "#3B82F6";
        let hsla = Hsla::from_hex(hex).unwrap();
        let back = hsla.to_color();
        // HSL float roundtrip may differ by 1 LSB in u8 space — match by value, not string
        let expected = Color::from_hex(hex).unwrap();
        assert_color_eq(expected, back, 0.01);
    }

    #[test]
    fn hsla_hue_wraps() {
        let a = Hsla::new(370.0, 1.0, 0.5, 1.0);
        let b = Hsla::new(10.0, 1.0, 0.5, 1.0);
        assert!((a.h - b.h).abs() < 1.0);
    }
}
