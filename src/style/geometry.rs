//! Geometry primitives used throughout the framework.

use std::ops::{Add, Sub};

/// A 2D point in the coordinate system (origin at top-left).
#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance(&self, other: &Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

impl Add<Vec2> for Point {
    type Output = Self;
    fn add(self, rhs: Vec2) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub<Point> for Point {
    type Output = Vec2;
    fn sub(self, rhs: Point) -> Vec2 {
        Vec2 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Sub<Vec2> for Point {
    type Output = Self;
    fn sub(self, rhs: Vec2) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

/// A 2D vector (displacement).
#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A rectangle defined by its top-left corner and size.
///
/// The coordinate system has origin at top-left, x increasing rightward,
/// y increasing downward.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_min_max(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    pub fn from_points(top_left: Point, bottom_right: Point) -> Self {
        Self {
            x: top_left.x,
            y: top_left.y,
            width: bottom_right.x - top_left.x,
            height: bottom_right.y - top_left.y,
        }
    }

    pub fn min_x(&self) -> f32 {
        self.x
    }
    pub fn min_y(&self) -> f32 {
        self.y
    }
    pub fn max_x(&self) -> f32 {
        self.x + self.width
    }
    pub fn max_y(&self) -> f32 {
        self.y + self.height
    }

    pub fn top_left(&self) -> Point {
        Point::new(self.x, self.y)
    }
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    pub fn size(&self) -> Size {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x && point.x <= self.max_x() && point.y >= self.y && point.y <= self.max_y()
    }

    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            width: self.width,
            height: self.height,
        }
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.x < other.max_x()
            && self.max_x() > other.x
            && self.y < other.max_y()
            && self.max_y() > other.y
    }

    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let max_x = self.max_x().min(other.max_x());
        let max_y = self.max_y().min(other.max_y());
        if x < max_x && y < max_y {
            Some(Self {
                x,
                y,
                width: max_x - x,
                height: max_y - y,
            })
        } else {
            None
        }
    }

    /// Inflate the rect by `delta` on all sides.
    pub fn inflate(&self, delta: f32) -> Self {
        Self {
            x: self.x - delta,
            y: self.y - delta,
            width: self.width + 2.0 * delta,
            height: self.height + 2.0 * delta,
        }
    }

    pub fn expand(&self, delta: f32) -> Self {
        self.inflate(delta)
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    pub fn union(&self, other: &Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.max_x().max(other.max_x());
        let b = self.max_y().max(other.max_y());
        Self {
            x,
            y,
            width: r - x,
            height: b - y,
        }
    }

    pub fn contains_rect(&self, other: &Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.max_x() >= other.max_x()
            && self.max_y() >= other.max_y()
    }

    pub fn intersection_area(&self, other: &Self) -> f32 {
        if let Some(isect) = self.intersection(other) {
            isect.area()
        } else {
            0.0
        }
    }

    pub fn intersects_logical(&self, other: &Rect) -> bool {
        self.intersects(other)
    }
}

/// A 2D size.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Corner radii for rounded rectangles.
///
/// Each corner can have a different radius.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    /// All corners have the same radius.
    pub const fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub const ZERO: Self = Self::all(0.0);

    /// Capsule shape (fully rounded, 9999px).
    pub const FULL: Self = Self::all(9999.0);

    /// Top corners only.
    pub fn top(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    }

    pub fn is_uniform(&self) -> bool {
        self.top_left == self.top_right
            && self.top_right == self.bottom_right
            && self.bottom_right == self.bottom_left
    }

    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_right == 0.0
            && self.bottom_left == 0.0
    }
}

/// Outer padding applied to a widget.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Padding {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Padding {
    pub const ZERO: Self = Self {
        left: 0.0,
        right: 0.0,
        top: 0.0,
        bottom: 0.0,
    };

    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            right: horizontal,
            top: vertical,
            bottom: vertical,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// Outer margin applied to a widget in layout.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Margin {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Margin {
    pub const ZERO: Self = Self {
        left: 0.0,
        right: 0.0,
        top: 0.0,
        bottom: 0.0,
    };

    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }
}

/// Cross-axis alignment for flex/grid children.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

/// Text horizontal alignment.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
    Start,
    End,
}

/// Text direction for layout and alignment.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

/// Tooltip placement relative to the target element.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum TooltipPlacement {
    Top,
    Bottom,
    Left,
    Right,
    Auto,
}
