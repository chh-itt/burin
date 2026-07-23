use crate::style::{Color, CornerRadii, LinearGradient, Rect};
use glam::Affine2;

/// z_index added to outline ring draw commands so they render above
/// adjacent sibling elements instead of being occluded by them.
pub const OUTLINE_Z_OFFSET: i32 = 10;

/// A draw item in local element coordinates (no transform, clip, or z_index).
/// Local items are converted to [`DrawCommand`] at replay time with the
/// element's current geometry, enabling cache hits when transforms change
/// (e.g. scrolling).
#[derive(Clone, Debug)]
pub enum LocalDrawItem {
    FillRect {
        local_rect: Rect,
        color: Color,
        radius: CornerRadii,
        blend_mode: u8,
    },
    StrokeRect {
        local_rect: Rect,
        color: Color,
        width: f32,
        radius: CornerRadii,
    },
    BorderFill {
        local_rect: Rect,
        color: Color,
        radius: CornerRadii,
    },
    OutlineGap {
        outline_width: f32,
        gap: f32,
        outline_color: Color,
        gap_color: Color,
        elem_w: f32,
        elem_h: f32,
        radius: CornerRadii,
    },
    Shadow {
        color: Color,
        offset_x: f32,
        offset_y: f32,
        blur: f32,
        elem_w: f32,
        elem_h: f32,
        radius: CornerRadii,
    },
    LinearGradient {
        local_rect: Rect,
        gradient: LinearGradient,
        radius: CornerRadii,
        stroke_width: f32,
    },
    FillPath {
        path: std::rc::Rc<kurbo::BezPath>,
        brush: crate::style::Brush,
    },
    StrokePath {
        path: std::rc::Rc<kurbo::BezPath>,
        stroke: kurbo::Stroke,
        brush: crate::style::Brush,
    },
}

impl LocalDrawItem {
    pub fn to_world(
        &self,
        x: f32,
        y: f32,
        element_clip: ClipInfo,
        element_xform: glam::Affine2,
        ez: i32,
        opacity: f32,
    ) -> DrawCommand {
        let fade = |c: Color| -> Color {
            if (opacity - 1.0).abs() < 0.001 {
                c
            } else {
                c.with_alpha(c.a * opacity)
            }
        };
        match *self {
            LocalDrawItem::FillRect {
                local_rect,
                color,
                radius,
                blend_mode,
            } => {
                let rect = Rect::new(
                    x + local_rect.x,
                    y + local_rect.y,
                    local_rect.width,
                    local_rect.height,
                );
                DrawCommand::FillRect {
                    rect,
                    color: fade(color),
                    radius,
                    clip: element_clip,
                    transform: element_xform,
                    z_index: ez,
                    blend_mode,
                }
            }
            LocalDrawItem::StrokeRect {
                local_rect,
                color,
                width,
                radius,
            } => {
                let rect = Rect::new(
                    x + local_rect.x,
                    y + local_rect.y,
                    local_rect.width,
                    local_rect.height,
                );
                DrawCommand::StrokeRect {
                    rect,
                    color: fade(color),
                    width,
                    radius,
                    clip: element_clip,
                    transform: element_xform,
                    z_index: ez,
                    blend_mode: 0,
                }
            }
            LocalDrawItem::OutlineGap { .. } | LocalDrawItem::Shadow { .. } => {
                DrawCommand::FillRect {
                    rect: Rect::ZERO,
                    color: Color::TRANSPARENT,
                    radius: CornerRadii::ZERO,
                    clip: element_clip,
                    transform: element_xform,
                    z_index: ez,
                    blend_mode: 0,
                }
            }
            LocalDrawItem::LinearGradient {
                local_rect,
                gradient,
                radius,
                stroke_width,
            } => {
                let rect = Rect::new(
                    x + local_rect.x,
                    y + local_rect.y,
                    local_rect.width,
                    local_rect.height,
                );
                DrawCommand::FillLinearGradient {
                    rect,
                    gradient,
                    radius,
                    stroke_width,
                    clip: element_clip,
                    transform: element_xform,
                    z_index: ez,
                }
            }
            LocalDrawItem::FillPath {
                ref path,
                ref brush,
            } => DrawCommand::FillPath {
                path: std::sync::Arc::new((**path).clone()),
                brush: brush.clone(),
                clip: element_clip,
                transform: element_xform,
                z_index: ez,
            },
            LocalDrawItem::StrokePath {
                ref path,
                ref stroke,
                ref brush,
            } => DrawCommand::StrokePath {
                path: std::sync::Arc::new((**path).clone()),
                stroke: stroke.clone(),
                brush: brush.clone(),
                clip: element_clip,
                transform: element_xform,
                z_index: ez,
            },
            LocalDrawItem::BorderFill { .. } => DrawCommand::FillRect {
                rect: Rect::ZERO,
                color: Color::TRANSPARENT,
                radius: CornerRadii::ZERO,
                clip: element_clip,
                transform: element_xform,
                z_index: ez,
                blend_mode: 0,
            },
        }
    }

    pub fn to_world_accum(
        &self,
        x: f32,
        y: f32,
        accum_clip: ClipInfo,
        element_xform: glam::Affine2,
        ez: i32,
        opacity: f32,
    ) -> Vec<DrawCommand> {
        let fade = |c: Color| -> Color {
            if (opacity - 1.0).abs() < 0.001 {
                c
            } else {
                c.with_alpha(c.a * opacity)
            }
        };
        match *self {
            LocalDrawItem::OutlineGap {
                outline_width,
                gap,
                outline_color,
                gap_color,
                elem_w,
                elem_h,
                radius,
            } => {
                let ow = outline_width;
                let or = Rect::new(
                    x - ow - gap,
                    y - ow - gap,
                    elem_w + 2.0 * (ow + gap),
                    elem_h + 2.0 * (ow + gap),
                );
                let gr = Rect::new(x - gap, y - gap, elem_w + 2.0 * gap, elem_h + 2.0 * gap);
                let expand_outer = ow + gap;
                vec![
                    DrawCommand::FillRect {
                        rect: or,
                        color: fade(outline_color),
                        radius: crate::style::CornerRadii {
                            top_left: radius.top_left + expand_outer,
                            top_right: radius.top_right + expand_outer,
                            bottom_right: radius.bottom_right + expand_outer,
                            bottom_left: radius.bottom_left + expand_outer,
                        },
                        clip: accum_clip,
                        transform: element_xform,
                        z_index: ez,
                        blend_mode: 0,
                    },
                    DrawCommand::FillRect {
                        rect: gr,
                        color: fade(gap_color),
                        radius: crate::style::CornerRadii {
                            top_left: radius.top_left + gap,
                            top_right: radius.top_right + gap,
                            bottom_right: radius.bottom_right + gap,
                            bottom_left: radius.bottom_left + gap,
                        },
                        clip: accum_clip,
                        transform: element_xform,
                        z_index: ez,
                        blend_mode: 0,
                    },
                ]
            }
            LocalDrawItem::Shadow {
                color,
                offset_x,
                offset_y,
                blur,
                elem_w,
                elem_h,
                radius,
            } => {
                let sr = Rect::new(
                    x + offset_x - blur,
                    y + offset_y - blur,
                    elem_w + blur * 2.0,
                    elem_h + blur * 2.0,
                );
                let shadow = crate::style::styled::Shadow {
                    color: fade(color),
                    offset_x,
                    offset_y,
                    blur,
                };
                let elem_size = (elem_w, elem_h);
                vec![DrawCommand::FillShadow {
                    rect: sr,
                    color: fade(color),
                    radius,
                    shadow,
                    elem_size,
                    clip: accum_clip,
                    transform: element_xform,
                    z_index: ez,
                }]
            }
            LocalDrawItem::LinearGradient {
                local_rect,
                gradient,
                radius,
                stroke_width,
            } => {
                let rect = Rect::new(
                    x + local_rect.x,
                    y + local_rect.y,
                    local_rect.width,
                    local_rect.height,
                );
                vec![DrawCommand::FillLinearGradient {
                    rect,
                    gradient,
                    radius,
                    stroke_width,
                    clip: accum_clip,
                    transform: element_xform,
                    z_index: ez,
                }]
            }
            LocalDrawItem::BorderFill {
                local_rect,
                color,
                radius,
            } => {
                let rect = Rect::new(
                    x + local_rect.x,
                    y + local_rect.y,
                    local_rect.width,
                    local_rect.height,
                );
                vec![DrawCommand::FillRect {
                    rect,
                    color: fade(color),
                    radius,
                    clip: accum_clip,
                    transform: element_xform,
                    z_index: ez,
                    blend_mode: 0,
                }]
            }
            _ => vec![],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClipInfo {
    pub rect: Rect,
    pub radius: CornerRadii,
    pub scroll_offset: [f32; 2],
}

impl ClipInfo {
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            radius: CornerRadii::ZERO,
            scroll_offset: [0.0, 0.0],
        }
    }

    pub fn with_radius(rect: Rect, radius: CornerRadii) -> Self {
        Self {
            rect,
            radius,
            scroll_offset: [0.0, 0.0],
        }
    }

    pub fn with_scroll(rect: Rect, radius: CornerRadii, scroll_offset: [f32; 2]) -> Self {
        Self {
            rect,
            radius,
            scroll_offset,
        }
    }

    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Self {
            rect: self.rect.offset(dx, dy),
            radius: self.radius,
            scroll_offset: self.scroll_offset,
        }
    }

    /// Re-express this clip in a path-local coordinate frame.
    ///
    /// Path commands are drawn through `element_xform * path_xform` where
    /// `path_xform = translate(tx, ty) * scale(sx, sy)` maps path-local
    /// (e.g. 24×24 icon) coordinates to document space. The CPU backend
    /// rasterises the clip through the **same combined transform** as the
    /// content, so a document-space clip would be displaced by the path
    /// transform (audit 2026-07-16: icon clips inside scroll containers
    /// landed at the wrong position). Mapping the clip into the path-local
    /// frame first makes `combined(local_clip) == element_xform(doc_clip)`.
    pub fn to_path_local(&self, tx: f32, ty: f32, sx: f32, sy: f32) -> Self {
        let sx = if sx.abs() < 1e-6 { 1e-6 } else { sx };
        let sy = if sy.abs() < 1e-6 { 1e-6 } else { sy };
        Self {
            rect: Rect::new(
                (self.rect.x - tx) / sx,
                (self.rect.y - ty) / sy,
                self.rect.width / sx,
                self.rect.height / sy,
            ),
            radius: CornerRadii {
                top_left: self.radius.top_left / sx,
                top_right: self.radius.top_right / sx,
                bottom_right: self.radius.bottom_right / sx,
                bottom_left: self.radius.bottom_left / sx,
            },
            scroll_offset: [self.scroll_offset[0] / sx, self.scroll_offset[1] / sy],
        }
    }
}

impl From<Rect> for ClipInfo {
    fn from(rect: Rect) -> Self {
        ClipInfo::new(rect)
    }
}

#[derive(Clone, Debug)]
pub enum DrawCommand {
    FillRect {
        rect: Rect,
        color: Color,
        radius: CornerRadii,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
        blend_mode: u8,
    },
    StrokeRect {
        rect: Rect,
        color: Color,
        width: f32,
        radius: CornerRadii,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
        blend_mode: u8,
    },
    FillShadow {
        rect: Rect,
        color: Color,
        radius: CornerRadii,
        shadow: crate::style::styled::Shadow,
        elem_size: (f32, f32),
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    },
    FillLinearGradient {
        rect: Rect,
        gradient: LinearGradient,
        radius: CornerRadii,
        stroke_width: f32,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    },
    DrawImage {
        hash: u64,
        rect: Rect,
        opacity: f32,
        content_fit: crate::widgets::display::ContentFit,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    },
    FillPath {
        path: std::sync::Arc<kurbo::BezPath>,
        brush: crate::style::Brush,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    },
    StrokePath {
        path: std::sync::Arc<kurbo::BezPath>,
        stroke: kurbo::Stroke,
        brush: crate::style::Brush,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    },
}

impl DrawCommand {
    pub fn z_index(&self) -> i32 {
        match self {
            DrawCommand::FillRect { z_index, .. } => *z_index,
            DrawCommand::StrokeRect { z_index, .. } => *z_index,
            DrawCommand::FillShadow { z_index, .. } => *z_index,
            DrawCommand::FillLinearGradient { z_index, .. } => *z_index,
            DrawCommand::DrawImage { z_index, .. } => *z_index,
            DrawCommand::FillPath { z_index, .. } => *z_index,
            DrawCommand::StrokePath { z_index, .. } => *z_index,
        }
    }
}

impl DrawCommand {
    pub fn clip(&self) -> ClipInfo {
        match self {
            DrawCommand::FillRect { clip, .. } => *clip,
            DrawCommand::StrokeRect { clip, .. } => *clip,
            DrawCommand::FillShadow { clip, .. } => *clip,
            DrawCommand::FillLinearGradient { clip, .. } => *clip,
            DrawCommand::DrawImage { clip, .. } => *clip,
            DrawCommand::FillPath { clip, .. } => *clip,
            DrawCommand::StrokePath { clip, .. } => *clip,
        }
    }

    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        match *self {
            DrawCommand::FillRect {
                rect,
                color,
                radius,
                clip,
                transform,
                z_index,
                blend_mode,
            } => DrawCommand::FillRect {
                rect: rect.offset(dx, dy),
                color,
                radius,
                clip: clip.offset(dx, dy),
                transform,
                z_index,
                blend_mode,
            },
            DrawCommand::StrokeRect {
                rect,
                color,
                width,
                radius,
                clip,
                transform,
                z_index,
                blend_mode,
            } => DrawCommand::StrokeRect {
                rect: rect.offset(dx, dy),
                color,
                width,
                radius,
                clip: clip.offset(dx, dy),
                transform,
                z_index,
                blend_mode,
            },
            DrawCommand::FillShadow {
                rect,
                color,
                radius,
                shadow,
                elem_size,
                clip,
                transform,
                z_index,
            } => DrawCommand::FillShadow {
                rect: rect.offset(dx, dy),
                color,
                radius,
                shadow,
                elem_size,
                clip: clip.offset(dx, dy),
                transform,
                z_index,
            },
            DrawCommand::FillLinearGradient {
                rect,
                gradient,
                radius,
                stroke_width,
                clip,
                transform,
                z_index,
            } => DrawCommand::FillLinearGradient {
                rect: rect.offset(dx, dy),
                gradient,
                radius,
                stroke_width,
                clip: clip.offset(dx, dy),
                transform,
                z_index,
            },
            DrawCommand::DrawImage {
                hash,
                rect,
                opacity,
                content_fit,
                clip,
                transform,
                z_index,
            } => DrawCommand::DrawImage {
                hash,
                rect: rect.offset(dx, dy),
                opacity,
                content_fit,
                clip: clip.offset(dx, dy),
                transform,
                z_index,
            },
            DrawCommand::FillPath {
                ref path,
                ref brush,
                clip,
                transform,
                z_index,
            } => DrawCommand::FillPath {
                path: path.clone(),
                brush: brush.clone(),
                clip: clip.offset(dx, dy),
                transform,
                z_index,
            },
            DrawCommand::StrokePath {
                ref path,
                ref stroke,
                ref brush,
                clip,
                transform,
                z_index,
            } => DrawCommand::StrokePath {
                path: path.clone(),
                stroke: stroke.clone(),
                brush: brush.clone(),
                clip: clip.offset(dx, dy),
                transform,
                z_index,
            },
        }
    }

    /// Post-multiply a translation to the command's transform.
    /// Used during subtree cache replay to update the scroll-related
    /// portion of the transform without changing rect positions.
    pub fn adjust_transform(&mut self, xform: glam::Affine2) {
        match self {
            DrawCommand::FillRect { transform, .. }
            | DrawCommand::StrokeRect { transform, .. }
            | DrawCommand::FillShadow { transform, .. }
            | DrawCommand::FillLinearGradient { transform, .. }
            | DrawCommand::DrawImage { transform, .. }
            | DrawCommand::FillPath { transform, .. }
            | DrawCommand::StrokePath { transform, .. } => {
                *transform *= xform;
            }
        }
    }

    /// Offset the clip rect to account for scroll changes, and update
    /// `clip.scroll_offset` to the new absolute scroll position so the
    /// GPU backend correctly reconstructs document-root clip coordinates.
    /// The content `rect` is left unchanged — only the clip moves.
    pub fn offset_clip(
        &self,
        clip_dx: f32,
        clip_dy: f32,
        new_scroll_ox: f32,
        new_scroll_oy: f32,
    ) -> Self {
        let mut c = self.clone();
        match c {
            DrawCommand::FillRect { ref mut clip, .. }
            | DrawCommand::StrokeRect { ref mut clip, .. }
            | DrawCommand::FillShadow { ref mut clip, .. }
            | DrawCommand::FillLinearGradient { ref mut clip, .. }
            | DrawCommand::DrawImage { ref mut clip, .. }
            | DrawCommand::FillPath { ref mut clip, .. }
            | DrawCommand::StrokePath { ref mut clip, .. } => {
                clip.rect = clip.rect.offset(clip_dx, clip_dy);
                clip.scroll_offset = [new_scroll_ox, new_scroll_oy];
            }
        }
        c
    }

    pub fn clip_mut(&mut self) -> &mut ClipInfo {
        match self {
            DrawCommand::FillRect { ref mut clip, .. }
            | DrawCommand::StrokeRect { ref mut clip, .. }
            | DrawCommand::FillShadow { ref mut clip, .. }
            | DrawCommand::FillLinearGradient { ref mut clip, .. }
            | DrawCommand::DrawImage { ref mut clip, .. }
            | DrawCommand::FillPath { ref mut clip, .. }
            | DrawCommand::StrokePath { ref mut clip, .. } => clip,
        }
    }
}

pub struct Painter {
    pub commands: Vec<DrawCommand>,
    pub viewport: Rect,
    pub local_items: Vec<LocalDrawItem>,
    pub backdrop_regions: Vec<BackdropRegion>,
}

#[derive(Clone, Debug)]
pub struct BackdropRegion {
    /// Element rect in document (layout) coordinates.
    pub rect: Rect,
    /// Element transform (carries accumulated scroll), applied by the renderer.
    pub transform: Affine2,
    /// Corner radius (for the rounded mask).
    pub corner_radius: crate::style::CornerRadii,
    /// Gaussian blur radius in logical pixels.
    pub blur_radius: f32,
    /// Optional tint applied over the blurred backdrop.
    pub tint: Option<Color>,
    /// Paint z-index (so the effect composites at the right depth).
    pub z_index: i32,
}

impl Painter {
    pub fn new(viewport: Rect) -> Self {
        Self {
            commands: Vec::new(),
            viewport,
            local_items: Vec::new(),
            backdrop_regions: Vec::new(),
        }
    }

    pub fn fill_rect(
        &mut self,
        rect: Rect,
        color: Color,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    ) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            color,
            radius: CornerRadii::ZERO,
            clip,
            transform,
            z_index,
            blend_mode: 0,
        });
    }

    pub fn fill_rect_blend(
        &mut self,
        rect: Rect,
        color: Color,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
        blend_mode: u8,
    ) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            color,
            radius: CornerRadii::ZERO,
            clip,
            transform,
            z_index,
            blend_mode,
        });
    }

    pub fn fill_rounded_rect(
        &mut self,
        rect: Rect,
        color: Color,
        radius: CornerRadii,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    ) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            color,
            radius,
            clip,
            transform,
            z_index,
            blend_mode: 0,
        });
    }

    pub fn fill_rounded_rect_blend(
        &mut self,
        rect: Rect,
        color: Color,
        radius: CornerRadii,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
        blend_mode: u8,
    ) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            color,
            radius,
            clip,
            transform,
            z_index,
            blend_mode,
        });
    }

    pub fn stroke_rect(
        &mut self,
        rect: Rect,
        color: Color,
        width: f32,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    ) {
        self.commands.push(DrawCommand::StrokeRect {
            rect,
            color,
            width,
            radius: CornerRadii::ZERO,
            clip,
            transform,
            z_index,
            blend_mode: 0,
        });
    }

    pub fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        color: Color,
        width: f32,
        radius: CornerRadii,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    ) {
        self.commands.push(DrawCommand::StrokeRect {
            rect,
            color,
            width,
            radius,
            clip,
            transform,
            z_index,
            blend_mode: 0,
        });
    }

    pub fn push_image(
        &mut self,
        hash: u64,
        rect: Rect,
        opacity: f32,
        content_fit: crate::widgets::display::ContentFit,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    ) {
        self.commands.push(DrawCommand::DrawImage {
            hash,
            rect,
            opacity,
            content_fit,
            clip,
            transform,
            z_index,
        });
    }

    pub fn take_commands(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.commands)
    }

    pub fn snapshot_len(&self) -> usize {
        self.commands.len()
    }

    pub fn local_items_len(&self) -> usize {
        self.local_items.len()
    }

    pub fn drain_from(&mut self, start: usize) -> Vec<DrawCommand> {
        self.commands.drain(start..).collect()
    }

    pub fn drain_local_items_from(&mut self, start: usize) -> Vec<LocalDrawItem> {
        self.local_items.drain(start..).collect()
    }

    pub fn commands_since(&self, start: usize) -> &[DrawCommand] {
        &self.commands[start..]
    }

    pub fn replay(&mut self, cached: &[DrawCommand]) {
        self.commands.extend_from_slice(cached);
    }

    pub fn push_local_fill_rect(&mut self, rect: Rect, color: Color, radius: CornerRadii) {
        self.local_items.push(LocalDrawItem::FillRect {
            local_rect: rect,
            color,
            radius,
            blend_mode: 0,
        });
    }

    pub fn push_local_fill_rect_blend(
        &mut self,
        rect: Rect,
        color: Color,
        radius: CornerRadii,
        blend_mode: u8,
    ) {
        self.local_items.push(LocalDrawItem::FillRect {
            local_rect: rect,
            color,
            radius,
            blend_mode,
        });
    }

    pub fn push_local_border_fill(&mut self, rect: Rect, color: Color, radius: CornerRadii) {
        self.local_items.push(LocalDrawItem::BorderFill {
            local_rect: rect,
            color,
            radius,
        });
    }

    pub fn push_local_stroke_rect(
        &mut self,
        rect: Rect,
        color: Color,
        width: f32,
        radius: CornerRadii,
    ) {
        self.local_items.push(LocalDrawItem::StrokeRect {
            local_rect: rect,
            color,
            width,
            radius,
        });
    }

    pub fn push_local_outline(
        &mut self,
        ow: f32,
        gap: f32,
        oc: Color,
        gap_c: Color,
        w: f32,
        h: f32,
        radius: CornerRadii,
    ) {
        self.local_items.push(LocalDrawItem::OutlineGap {
            outline_width: ow,
            gap,
            outline_color: oc,
            gap_color: gap_c,
            elem_w: w,
            elem_h: h,
            radius,
        });
    }

    pub fn push_local_shadow(
        &mut self,
        shadow: crate::style::styled::Shadow,
        elem_w: f32,
        elem_h: f32,
        radius: CornerRadii,
    ) {
        self.local_items.push(LocalDrawItem::Shadow {
            color: shadow.color,
            offset_x: shadow.offset_x,
            offset_y: shadow.offset_y,
            blur: shadow.blur,
            elem_w,
            elem_h,
            radius,
        });
    }

    pub fn push_local_linear_gradient(
        &mut self,
        local_rect: Rect,
        gradient: LinearGradient,
        radius: CornerRadii,
    ) {
        self.push_local_linear_gradient_with_stroke(local_rect, gradient, radius, 0.0);
    }

    pub fn push_local_linear_gradient_with_stroke(
        &mut self,
        local_rect: Rect,
        gradient: LinearGradient,
        radius: CornerRadii,
        stroke_width: f32,
    ) {
        self.local_items.push(LocalDrawItem::LinearGradient {
            local_rect,
            gradient,
            radius,
            stroke_width,
        });
    }

    pub fn fill_path(
        &mut self,
        path: &kurbo::BezPath,
        brush: crate::style::Brush,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    ) {
        self.commands.push(DrawCommand::FillPath {
            path: std::sync::Arc::new(path.clone()),
            brush,
            clip,
            transform,
            z_index,
        });
    }

    pub fn stroke_path(
        &mut self,
        path: &kurbo::BezPath,
        stroke: kurbo::Stroke,
        brush: crate::style::Brush,
        clip: ClipInfo,
        transform: Affine2,
        z_index: i32,
    ) {
        self.commands.push(DrawCommand::StrokePath {
            path: std::sync::Arc::new(path.clone()),
            stroke,
            brush,
            clip,
            transform,
            z_index,
        });
    }

    pub fn push_local_fill_path(
        &mut self,
        path: std::rc::Rc<kurbo::BezPath>,
        brush: crate::style::Brush,
    ) {
        self.local_items
            .push(LocalDrawItem::FillPath { path, brush });
    }

    pub fn push_local_stroke_path(
        &mut self,
        path: std::rc::Rc<kurbo::BezPath>,
        stroke: kurbo::Stroke,
        brush: crate::style::Brush,
    ) {
        self.local_items.push(LocalDrawItem::StrokePath {
            path,
            stroke,
            brush,
        });
    }

    pub fn drain_local_items(&mut self) -> Vec<LocalDrawItem> {
        std::mem::take(&mut self.local_items)
    }

    pub fn replay_local(
        &mut self,
        items: &[LocalDrawItem],
        x: f32,
        y: f32,
        clip: ClipInfo,
        xform: Affine2,
        z: i32,
        opacity: f32,
    ) {
        for item in items {
            match item {
                LocalDrawItem::OutlineGap { .. } | LocalDrawItem::Shadow { .. } => {}
                _ => {
                    self.commands
                        .push(item.to_world(x, y, clip, xform, z, opacity));
                }
            }
        }
    }

    pub fn replay_local_accum(
        &mut self,
        items: &[LocalDrawItem],
        x: f32,
        y: f32,
        accum_clip: ClipInfo,
        element_clip: ClipInfo,
        xform: Affine2,
        z: i32,
        opacity: f32,
    ) {
        for item in items {
            match item {
                LocalDrawItem::OutlineGap { .. }
                | LocalDrawItem::Shadow { .. }
                | LocalDrawItem::BorderFill { .. } => {
                    for cmd in item.to_world_accum(x, y, accum_clip, xform, z, opacity) {
                        self.commands.push(cmd);
                    }
                }
                LocalDrawItem::LinearGradient { .. } => {
                    for cmd in item.to_world_accum(x, y, accum_clip, xform, z, opacity) {
                        self.commands.push(cmd);
                    }
                }
                _ => {
                    self.commands
                        .push(item.to_world(x, y, element_clip, xform, z, opacity));
                }
            }
        }
    }
}
