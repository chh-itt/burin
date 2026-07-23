use crate::core::config::{FlexWrap, Overflow, ScrollbarPolicy};
use crate::event::{DragData, DropType};
use crate::style::{
    Alignment, Color, CornerRadii, Dimension, LinearGradient, Margin, Padding, TextAlign,
    TextDirection,
};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    Strikethrough,
    Overline,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct Shadow {
    pub color: Color,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
}

impl Shadow {
    pub fn new(color: Color, offset_x: f32, offset_y: f32, blur: f32) -> Self {
        Self {
            color,
            offset_x,
            offset_y,
            blur,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum BlendMode {
    Multiply,
    Screen,
    Overlay,
}

impl BlendMode {
    pub fn to_u8(self) -> u8 {
        match self {
            BlendMode::Multiply => 1,
            BlendMode::Screen => 2,
            BlendMode::Overlay => 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct BackdropFilter {
    pub blur_radius: f32,
    pub tint: Option<Color>,
}

/// The unified styling trait for all widgets.
///
/// `Styled` provides a method-chaining API for setting visual properties.
/// Every widget and container automatically gets these methods via the
/// blanket impl on [`Widget`](crate::core::Widget).
///
/// # Style resolution priority (lowest → highest)
///
/// 1. Component presets from [`M3Theme`](crate::theme::M3Theme)
/// 2. `.intent()` / `.appearance()` / `.size()` / `.shape()`
/// 3. Direct style methods (`.background()`, `.text_color()`, etc.) — **highest**
pub trait Styled: Sized {
    fn style_refinement(&mut self) -> &mut StyleRefinement;

    // ── Background & foreground ──

    fn background(mut self, color: impl Into<Color>) -> Self {
        self.style_refinement().background = Some(color.into());
        self
    }

    fn text_color(mut self, color: impl Into<Color>) -> Self {
        self.style_refinement().text_color = Some(color.into());
        self
    }

    // ── Opacity & visibility ──

    fn opacity(mut self, opacity: f32) -> Self {
        self.style_refinement().opacity = Some(opacity.clamp(0.0, 1.0));
        self
    }

    fn visible(mut self, visible: bool) -> Self {
        self.style_refinement().visible = Some(visible);
        self
    }

    // ── Size ──

    fn width(mut self, w: impl Into<Dimension>) -> Self {
        self.style_refinement().width = Some(w.into());
        self
    }

    fn height(mut self, h: impl Into<Dimension>) -> Self {
        self.style_refinement().height = Some(h.into());
        self
    }

    fn min_width(mut self, w: impl Into<Dimension>) -> Self {
        self.style_refinement().min_width = Some(w.into());
        self
    }

    fn max_width(mut self, w: impl Into<Dimension>) -> Self {
        self.style_refinement().max_width = Some(w.into());
        self
    }

    // ── Spacing ──

    fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.style_refinement().padding = Some(p.into());
        self
    }

    fn margin(mut self, m: impl Into<Margin>) -> Self {
        self.style_refinement().margin = Some(m.into());
        self
    }

    fn gap(mut self, gap: f32) -> Self {
        self.style_refinement().gap = Some(gap);
        self
    }

    // ── Typography ──

    fn text_align(mut self, align: TextAlign) -> Self {
        self.style_refinement().text_align = Some(align);
        self
    }

    fn font_family(mut self, family: impl Into<String>) -> Self {
        self.style_refinement().font_family = Some(family.into());
        self
    }

    fn font_size(mut self, size: f32) -> Self {
        self.style_refinement().font_size = Some(size);
        self
    }

    fn font_weight(mut self, weight: u16) -> Self {
        self.style_refinement().font_weight = Some(weight);
        self
    }

    fn line_height(mut self, lh: f32) -> Self {
        self.style_refinement().line_height = Some(lh);
        self
    }

    fn text_decoration(mut self, dec: TextDecoration) -> Self {
        self.style_refinement().text_decoration = Some(dec);
        self
    }

    fn text_overflow(mut self, ov: TextOverflow) -> Self {
        self.style_refinement().text_overflow = Some(ov);
        self
    }

    fn shadow(mut self, color: impl Into<Color>, ox: f32, oy: f32, blur: f32) -> Self {
        self.style_refinement().shadow = Some(Shadow::new(color.into(), ox, oy, blur));
        self
    }

    fn blend_mode(mut self, mode: BlendMode) -> Self {
        self.style_refinement().blend_mode = Some(mode);
        self
    }

    fn backdrop_filter(mut self, blur_radius: f32) -> Self {
        self.style_refinement().backdrop_filter = Some(BackdropFilter {
            blur_radius,
            tint: None,
        });
        self
    }

    fn gradient(mut self, grad: LinearGradient) -> Self {
        self.style_refinement().gradient = Some(grad);
        self
    }

    fn z_index(mut self, zi: i32) -> Self {
        self.style_refinement().z_index = Some(zi);
        self
    }

    fn placeholder_color(mut self, color: impl Into<Color>) -> Self {
        self.style_refinement().placeholder_color = Some(color.into());
        self
    }

    // ── Border ──

    fn border_width(mut self, width: f32) -> Self {
        self.style_refinement().border_width = Some(width);
        self
    }

    fn border_color(mut self, color: impl Into<Color>) -> Self {
        self.style_refinement().border_color = Some(color.into());
        self
    }

    fn corner_radius(mut self, radius: impl Into<CornerRadii>) -> Self {
        self.style_refinement().corner_radius = Some(radius.into());
        self
    }

    // ── Outline (focus ring) ──

    fn outline_width(mut self, width: f32) -> Self {
        self.style_refinement().outline_width = Some(width);
        self
    }

    fn outline_color(mut self, color: impl Into<Color>) -> Self {
        self.style_refinement().outline_color = Some(color.into());
        self
    }

    fn text_direction(mut self, dir: TextDirection) -> Self {
        self.style_refinement().text_direction = Some(dir);
        self
    }

    fn transform(mut self, affine: glam::Affine2) -> Self {
        let existing = self
            .style_refinement()
            .transform
            .map(|t| glam::Affine2::from_cols_array(&t))
            .unwrap_or(glam::Affine2::IDENTITY);
        self.style_refinement().transform = Some((existing * affine).to_cols_array());
        self
    }

    // ── Drag & Drop ──

    fn draggable(mut self) -> Self {
        self.style_refinement().draggable = Some(true);
        self
    }

    fn drag_data(mut self, data: DragData) -> Self {
        self.style_refinement().drag_data = Some(data);
        self
    }

    fn drop_target(mut self) -> Self {
        self.style_refinement().drop_target = Some(true);
        self
    }

    fn accept_drop_types(mut self, types: &[DropType]) -> Self {
        self.style_refinement().accept_drop_types = Some(types.to_vec());
        self
    }

    fn rotate(mut self, radians: f32) -> Self {
        let a = glam::Affine2::from_angle(radians);
        let existing = self
            .style_refinement()
            .transform
            .map(|t| glam::Affine2::from_cols_array(&t))
            .unwrap_or(glam::Affine2::IDENTITY);
        self.style_refinement().transform = Some((existing * a).to_cols_array());
        self
    }

    fn scale(mut self, s: f32) -> Self {
        let a = glam::Affine2::from_scale(glam::Vec2::new(s, s));
        let existing = self
            .style_refinement()
            .transform
            .map(|t| glam::Affine2::from_cols_array(&t))
            .unwrap_or(glam::Affine2::IDENTITY);
        self.style_refinement().transform = Some((existing * a).to_cols_array());
        self
    }

    fn translate(mut self, x: f32, y: f32) -> Self {
        let a = glam::Affine2::from_translation(glam::Vec2::new(x, y));
        let existing = self
            .style_refinement()
            .transform
            .map(|t| glam::Affine2::from_cols_array(&t))
            .unwrap_or(glam::Affine2::IDENTITY);
        self.style_refinement().transform = Some((existing * a).to_cols_array());
        self
    }

    // ── Conditional styling ──

    fn when(self, condition: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if condition {
            f(self)
        } else {
            self
        }
    }

    fn when_some<T>(self, option: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        match option {
            Some(val) => f(self, val),
            None => self,
        }
    }
}

/// Accumulated style overrides for a widget.
///
/// Each field is `None` until explicitly set.  During style resolution,
/// `None` fields fall through to the next priority layer.
#[derive(Clone, Default, Debug)]
pub struct StyleRefinement {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub text_color: Option<Color>,
    pub opacity: Option<f32>,
    pub visible: Option<bool>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub max_width: Option<Dimension>,
    pub padding: Option<Padding>,
    pub margin: Option<Margin>,
    pub gap: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<Color>,
    pub corner_radius: Option<CornerRadii>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub line_height: Option<f32>,
    pub text_decoration: Option<TextDecoration>,
    pub text_overflow: Option<TextOverflow>,
    pub shadow: Option<Shadow>,
    pub blend_mode: Option<BlendMode>,
    pub backdrop_filter: Option<BackdropFilter>,
    pub gradient: Option<LinearGradient>,
    pub z_index: Option<i32>,
    pub text_align: Option<TextAlign>,
    pub font_family: Option<String>,
    pub placeholder_color: Option<Color>,
    pub outline_width: Option<f32>,
    pub outline_color: Option<Color>,
    pub text_direction: Option<TextDirection>,
    pub transform: Option<[f32; 6]>,
    pub draggable: Option<bool>,
    pub drag_data: Option<DragData>,
    pub drop_target: Option<bool>,
    pub accept_drop_types: Option<Vec<DropType>>,
    /// Declarative state→style overrides.
    pub state_style: Option<crate::style::StateStyle>,
    // ── Layout fields ──
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Dimension>,
    pub flex_wrap: Option<FlexWrap>,
    pub overflow: Option<Overflow>,
    pub aspect_ratio: Option<Option<f32>>,
    pub order: Option<i32>,
    pub scrollbar_policy: Option<ScrollbarPolicy>,
    pub content_align: Option<Alignment>,
}

impl StyleRefinement {
    pub fn merge(&mut self, other: &Self) {
        if let Some(v) = other.background {
            self.background = Some(v);
        }
        if let Some(v) = other.foreground {
            self.foreground = Some(v);
        }
        if let Some(v) = other.text_color {
            self.text_color = Some(v);
        }
        if let Some(v) = other.opacity {
            self.opacity = Some(v);
        }
        if let Some(v) = other.visible {
            self.visible = Some(v);
        }
        if let Some(v) = other.width {
            self.width = Some(v);
        }
        if let Some(v) = other.height {
            self.height = Some(v);
        }
        if let Some(v) = other.min_width {
            self.min_width = Some(v);
        }
        if let Some(v) = other.max_width {
            self.max_width = Some(v);
        }
        if let Some(v) = other.padding {
            self.padding = Some(v);
        }
        if let Some(v) = other.margin {
            self.margin = Some(v);
        }
        if let Some(v) = other.gap {
            self.gap = Some(v);
        }
        if let Some(v) = other.border_width {
            self.border_width = Some(v);
        }
        if let Some(v) = other.border_color {
            self.border_color = Some(v);
        }
        if let Some(v) = other.corner_radius {
            self.corner_radius = Some(v);
        }
        if let Some(v) = other.font_size {
            self.font_size = Some(v);
        }
        if let Some(v) = other.font_weight {
            self.font_weight = Some(v);
        }
        if let Some(v) = other.line_height {
            self.line_height = Some(v);
        }
        if let Some(v) = other.text_align {
            self.text_align = Some(v);
        }
        if let Some(v) = other.font_family.as_ref() {
            self.font_family = Some(v.clone());
        }
        if let Some(v) = other.line_height {
            self.line_height = Some(v);
        }
        if let Some(v) = other.text_decoration {
            self.text_decoration = Some(v);
        }
        if let Some(v) = other.text_overflow {
            self.text_overflow = Some(v);
        }
        if let Some(v) = other.shadow {
            self.shadow = Some(v);
        }
        if let Some(v) = other.blend_mode {
            self.blend_mode = Some(v);
        }
        if let Some(v) = other.backdrop_filter {
            self.backdrop_filter = Some(v);
        }
        if let Some(v) = other.gradient {
            self.gradient = Some(v);
        }
        if let Some(v) = other.z_index {
            self.z_index = Some(v);
        }
        if let Some(v) = other.placeholder_color {
            self.placeholder_color = Some(v);
        }
        if let Some(v) = other.outline_width {
            self.outline_width = Some(v);
        }
        if let Some(v) = other.outline_color {
            self.outline_color = Some(v);
        }
        if let Some(v) = other.draggable {
            self.draggable = Some(v);
        }
        if let Some(v) = other.drag_data.as_ref() {
            self.drag_data = Some(v.clone());
        }
        if let Some(v) = other.drop_target {
            self.drop_target = Some(v);
        }
        if let Some(v) = other.accept_drop_types.as_ref() {
            self.accept_drop_types = Some(v.clone());
        }
        if let Some(v) = other.flex_grow {
            self.flex_grow = Some(v);
        }
        if let Some(v) = other.flex_shrink {
            self.flex_shrink = Some(v);
        }
        if let Some(v) = other.flex_basis {
            self.flex_basis = Some(v);
        }
        if let Some(v) = other.flex_wrap {
            self.flex_wrap = Some(v);
        }
        if let Some(v) = other.overflow {
            self.overflow = Some(v);
        }
        if let Some(v) = other.aspect_ratio {
            self.aspect_ratio = Some(v);
        }
        if let Some(v) = other.order {
            self.order = Some(v);
        }
        if let Some(v) = other.scrollbar_policy {
            self.scrollbar_policy = Some(v);
        }
        if let Some(v) = other.content_align {
            self.content_align = Some(v);
        }
    }
}
