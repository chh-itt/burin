use auralis_signal::Signal;

use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Rect};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};

pub struct LineChart {
    data: Signal<Vec<(f32, f32)>>,
    line_color: Color,
    line_width: f32,
    fill_below: bool,
    fill_color: Option<Color>,
    max_x: Option<f32>,
    max_y: Option<f32>,
    chart_width: f32,
    chart_height: f32,
    style: StyleRefinement,
}

impl LineChart {
    pub fn new(data: Signal<Vec<(f32, f32)>>) -> Self {
        Self {
            data,
            line_color: Color::rgba8(100, 143, 255, 255),
            line_width: 2.0,
            fill_below: false,
            fill_color: None,
            max_x: None,
            max_y: None,
            chart_width: 600.0,
            chart_height: 160.0,
            style: StyleRefinement::default(),
        }
    }

    pub fn line_color(mut self, c: Color) -> Self {
        self.line_color = c;
        self
    }
    pub fn line_width(mut self, w: f32) -> Self {
        self.line_width = w;
        self
    }
    pub fn fill_below(mut self, color: Color) -> Self {
        self.fill_below = true;
        self.fill_color = Some(color);
        self
    }
    pub fn max_x(mut self, x: f32) -> Self {
        self.max_x = Some(x);
        self
    }
    pub fn max_y(mut self, y: f32) -> Self {
        self.max_y = Some(y);
        self
    }
    pub fn chart_width(mut self, w: f32) -> Self {
        self.chart_width = w;
        self
    }
    pub fn chart_height(mut self, h: f32) -> Self {
        self.chart_height = h;
        self
    }
}

impl Styled for LineChart {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for LineChart {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Interactive(InteractiveRole::Progress {
            intent: crate::theme::Intent::Primary,
        });
        let resolved = ctx.theme.resolve_component(&role);
        let _style = match &resolved {
            ResolvedComponentStyle::Progress(s) => s,
            _ => {
                return ctx.arena.allocate();
            }
        };

        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());

        ctx.arena
            .component_tables
            .borrow_mut()
            .layout
            .entry(id)
            .or_default()
            .height_dim = crate::style::Dimension::Pixels(self.chart_height + 40.0);

        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            let total_h = self.chart_height + 40.0;
            element.set_preferred_height(total_h);
            element.set_preferred_width(Some(self.chart_width));
            element.set_affected_by_child_size(false);
            element.set_flex_grow(0.0);
            element.set_flex_shrink(0.0);
            element.set_accessible_role(accesskit::Role::Image);

            self.style.height = Some(crate::style::Dimension::Pixels(total_h));
            self.style.width = Some(crate::style::Dimension::Pixels(self.chart_width));

            if let Some(zi) = self.style.z_index {
                element.set_z_index(zi);
            }
            if let Some(o) = self.style.opacity {
                element.set_opacity(o);
            }

            let dirty = element.dirty.clone();
            let data_sig = self.data.clone();
            crate::core::signal_bridge::subscribe_owned(id, &data_sig, move || {
                dirty.set(dirty.get() | DirtyFlags::REPAINT);
            });

            element.insert_user_data(LineChartData {
                data_signal: self.data.clone(),
                line_color: self.line_color,
                line_width: self.line_width,
                fill_below: self.fill_below,
                fill_color: self.fill_color,
                max_x: self.max_x,
                max_y: self.max_y,
                chart_width: self.chart_width,
                chart_height: self.chart_height,
            });
        }
        ctx.register_theme_component(id, &resolved, &role, &self.style);
        id
    }
}

pub struct LineChartData {
    pub data_signal: Signal<Vec<(f32, f32)>>,
    pub line_color: Color,
    pub line_width: f32,
    pub fill_below: bool,
    pub fill_color: Option<Color>,
    pub max_x: Option<f32>,
    pub max_y: Option<f32>,
    pub chart_width: f32,
    pub chart_height: f32,
}

fn grid_line_color() -> Color {
    Color::rgba8(60, 60, 80, 255)
}

pub(crate) fn paint_line_chart(
    ld: &LineChartData,
    painter: &mut crate::render::Painter,
    x: f32,
    y: f32,
    _w: f32,
    _h: f32,
    clip: crate::render::ClipInfo,
    xform: glam::Affine2,
    z_index: i32,
) {
    let points = ld.data_signal.read();
    if points.len() < 2 {
        return;
    }

    let left_pad = 50.0;
    let right_pad = 16.0;
    let top_pad = 8.0;
    let bottom_pad = 24.0;
    let area_w = ld.chart_width - left_pad - right_pad;
    let area_h = ld.chart_height - top_pad - bottom_pad;
    let area_x = x + left_pad;
    let area_y = y + top_pad;

    let max_x = ld
        .max_x
        .unwrap_or_else(|| points.last().map(|p| p.0).unwrap_or(1.0).max(1.0));
    let max_y = ld
        .max_y
        .unwrap_or_else(|| points.iter().map(|p| p.1).fold(0.0f32, f32::max).max(1.0));

    // grid lines
    let grid_lines = 4;
    for i in 0..=grid_lines {
        let gy = area_y + area_h - (area_h * i as f32 / grid_lines as f32);
        painter.fill_rounded_rect(
            Rect::new(x + left_pad - 4.0, gy, area_w + 8.0, 1.0),
            grid_line_color(),
            crate::style::CornerRadii::ZERO,
            clip,
            xform,
            z_index,
        );
    }

    // line path
    let mut path = kurbo::BezPath::new();
    let first = &points[0];
    let sx = area_x + (first.0 / max_x) * area_w;
    let sy = area_y + area_h - (first.1 / max_y) * area_h;
    path.move_to(kurbo::Point::new(sx as f64, sy as f64));

    for pt in points.iter().skip(1) {
        let px = area_x + (pt.0 / max_x) * area_w;
        let py = area_y + area_h - (pt.1 / max_y) * area_h;
        path.line_to(kurbo::Point::new(px as f64, py as f64));
    }

    // fill below if requested
    if ld.fill_below {
        let last_x = area_x + (points.last().unwrap().0 / max_x) * area_w;
        let bottom_y = area_y + area_h;
        let first_x = area_x + (points[0].0 / max_x) * area_w;
        let fill_color = ld.fill_color.unwrap_or(ld.line_color.with_alpha(0.15));

        let mut fill_path = path.clone();
        fill_path.line_to(kurbo::Point::new(last_x as f64, bottom_y as f64));
        fill_path.line_to(kurbo::Point::new(first_x as f64, bottom_y as f64));
        fill_path.close_path();

        painter.fill_path(
            &fill_path,
            crate::style::Brush::Solid(fill_color),
            clip,
            xform,
            z_index,
        );
    }

    // stroke the line
    let stroke = kurbo::Stroke {
        width: ld.line_width as f64,
        ..Default::default()
    };
    painter.stroke_path(
        &path,
        stroke,
        crate::style::Brush::Solid(ld.line_color),
        clip,
        xform,
        z_index + 1,
    );
}

impl std::fmt::Debug for LineChart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LineChart").finish_non_exhaustive()
    }
}
