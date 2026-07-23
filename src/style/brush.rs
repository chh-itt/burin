use crate::style::{Color, LinearGradient};

#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    Solid(Color),
    Gradient(LinearGradient),
}

impl Brush {
    pub fn solid(color: Color) -> Self {
        Self::Solid(color)
    }
    pub fn gradient(gradient: LinearGradient) -> Self {
        Self::Gradient(gradient)
    }
    pub fn alpha(&self) -> f32 {
        match self {
            Brush::Solid(c) => c.a,
            Brush::Gradient(g) => g.stops.first().map_or(1.0, |s| s.color.a),
        }
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}
