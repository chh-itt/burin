/// A length value for layout constraints.
///
/// # Examples
///
/// ```
/// use burin::style::{Dimension, px, pct, auto};
///
/// let fixed = px(200.0);     // Dimension::Pixels(200.0)
/// let ratio = pct(0.5);     // Dimension::Percent(0.5) = 50%
/// let fluid = auto();        // Dimension::Auto
/// ```
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Dimension {
    /// A fixed size in logical pixels.
    Pixels(f32),
    /// A fraction of the parent's available space (0.0..1.0).
    Percent(f32),
    /// Size determined by the content.
    #[default]
    Auto,
}

impl Dimension {
    /// Returns `true` if this is `Dimension::Auto`.
    pub fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

/// Create a fixed-pixel dimension.
///
/// ```
/// use burin::style::px;
/// let dim = px(16.0);
/// ```
pub fn px(value: f32) -> Dimension {
    Dimension::Pixels(value)
}

/// Create a percentage dimension (0.0..1.0).
///
/// ```
/// use burin::style::pct;
/// let half = pct(0.5); // 50% of parent
/// ```
pub fn pct(value: f32) -> Dimension {
    Dimension::Percent(value.clamp(0.0, 1.0))
}

/// Create an auto dimension.
///
/// ```
/// use burin::style::auto;
/// let dim = auto();
/// ```
pub fn auto() -> Dimension {
    Dimension::Auto
}

impl From<f32> for Dimension {
    fn from(value: f32) -> Self {
        Dimension::Pixels(value)
    }
}

impl std::fmt::Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pixels(v) => write!(f, "{v}px"),
            Self::Percent(v) => write!(f, "{}%", v * 100.0),
            Self::Auto => write!(f, "auto"),
        }
    }
}
