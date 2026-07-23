//! Styling system: colors, geometry, dimensions, and the `Styled` trait.
//!
//! The `Styled` trait provides a unified method-chaining API for all widgets.

pub mod brush;
pub mod color;
pub mod dimension;
pub mod geometry;
pub mod gradient;
pub mod state_style;
pub mod styled;

pub use brush::Brush;
pub use color::{Color, Hsla};
pub use dimension::{auto, pct, px, Dimension};
pub use geometry::{
    Alignment, CornerRadii, Margin, Padding, Point, Rect, Size, TextAlign, TextDirection,
    TooltipPlacement, Vec2,
};
pub use gradient::{Gradient, GradientKind, GradientStop, LinearGradient};
pub use state_style::{resolve_style, ResolvedStyle, StateStyle, StyleVariant};
pub use styled::{StyleRefinement, Styled};
