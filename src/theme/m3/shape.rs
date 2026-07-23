//! M3 Shape system — 7 levels of corner radius.
//! Default: symmetric corners via CornerRadii::all(radius).
//! Asymmetric corners only for special components (e.g. Dialog top-only).

use crate::style::CornerRadii;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeLevel {
    None,       // 0
    ExtraSmall, // 4
    Small,      // 8
    Medium,     // 12
    Large,      // 16
    ExtraLarge, // 28
    Full,       // f32::MAX
}

impl ShapeLevel {
    pub fn radius(&self) -> f32 {
        match self {
            ShapeLevel::None => 0.0,
            ShapeLevel::ExtraSmall => 4.0,
            ShapeLevel::Small => 8.0,
            ShapeLevel::Medium => 12.0,
            ShapeLevel::Large => 16.0,
            ShapeLevel::ExtraLarge => 28.0,
            ShapeLevel::Full => f32::MAX,
        }
    }

    pub fn to_corner_radii(&self) -> CornerRadii {
        CornerRadii::all(self.radius())
    }

    /// Map a design_radius parameter (0.0=sharp, 1.0=pill) to a ShapeLevel.
    pub fn from_design_radius(v: f32) -> Self {
        let v = v.clamp(0.0, 1.0);
        if v < 0.1 {
            ShapeLevel::None
        } else if v < 0.25 {
            ShapeLevel::ExtraSmall
        } else if v < 0.4 {
            ShapeLevel::Small
        } else if v < 0.55 {
            ShapeLevel::Medium
        } else if v < 0.7 {
            ShapeLevel::Large
        } else if v < 0.85 {
            ShapeLevel::ExtraLarge
        } else {
            ShapeLevel::Full
        }
    }
}
