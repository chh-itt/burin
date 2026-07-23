use crate::style::Color;

/// The kind of gradient interpolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GradientKind {
    /// Colors interpolate along the line from `start` to `end`.
    #[default]
    Linear,
    /// Colors interpolate radially outward from `start` (center); the radius is
    /// `|end - start|` (i.e. `end` is a point on the outer edge).
    Radial,
    /// Colors sweep by angle around `start` (center); `end` defines the 0°
    /// reference direction (angle of `end - start`).
    Conic,
}

/// A gradient paint with up to 4 color stops.
///
/// The `start`/`end` points are normalized `0..1` relative to the element rect.
/// Their meaning depends on [`kind`](GradientKind):
/// - **Linear**: `start` = line start, `end` = line end.
/// - **Radial**: `start` = center, `end` = a point on the outer radius.
/// - **Conic**:  `start` = center, `end` = 0° reference direction.
///
/// Kept named `LinearGradient` for backward compatibility (Linear is the default
/// kind); `Gradient` is a type alias. Use [`LinearGradient::radial`] /
/// [`LinearGradient::conic`] for the other kinds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    pub kind: GradientKind,
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub stops: [GradientStop; 4],
    pub stop_count: u32,
}

/// Preferred name going forward; `LinearGradient` is retained for compatibility.
pub type Gradient = LinearGradient;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub color: Color,
    pub offset: f32,
}

impl LinearGradient {
    fn build(
        kind: GradientKind,
        start: (f32, f32),
        end: (f32, f32),
        stops: &[(Color, f32)],
    ) -> Self {
        let mut s = Self {
            kind,
            start,
            end,
            stops: [GradientStop {
                color: Color::TRANSPARENT,
                offset: 0.0,
            }; 4],
            stop_count: stops.len().min(4) as u32,
        };
        for (i, &(color, offset)) in stops.iter().take(4).enumerate() {
            s.stops[i] = GradientStop { color, offset };
        }
        s
    }

    /// Linear gradient from `start` to `end` (both normalized `0..1`).
    pub fn new(start: (f32, f32), end: (f32, f32), stops: &[(Color, f32)]) -> Self {
        Self::build(GradientKind::Linear, start, end, stops)
    }

    /// Linear gradient (explicit alias of [`new`]).
    pub fn linear(start: (f32, f32), end: (f32, f32), stops: &[(Color, f32)]) -> Self {
        Self::build(GradientKind::Linear, start, end, stops)
    }

    /// Radial gradient centered at `center`, radius reaching `radius_point`
    /// (both normalized `0..1` relative to the rect). A common full-cover radial
    /// is `center=(0.5,0.5)`, `radius_point=(1.0,0.5)`.
    pub fn radial(center: (f32, f32), radius_point: (f32, f32), stops: &[(Color, f32)]) -> Self {
        Self::build(GradientKind::Radial, center, radius_point, stops)
    }

    /// Conic (angular) gradient centered at `center`, with `angle_ref` defining
    /// the 0° sweep direction (both normalized `0..1`). For a full color wheel
    /// use `center=(0.5,0.5)`, `angle_ref=(1.0,0.5)` (0° points right).
    pub fn conic(center: (f32, f32), angle_ref: (f32, f32), stops: &[(Color, f32)]) -> Self {
        Self::build(GradientKind::Conic, center, angle_ref, stops)
    }
}
