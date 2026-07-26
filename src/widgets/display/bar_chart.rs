use auralis_signal::Signal;

use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Rect};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};

#[derive(Clone)]
/// A group of bar values with a label.
pub struct BarGroup {
    pub label: String,
    pub values: Vec<f32>,
}

/// A vertical bar chart widget.
pub struct BarChart {
    data: Signal<Vec<BarGroup>>,
    colors: Vec<Color>,
    legend: Vec<String>,
    max_value: Option<f32>,
    min_max_value: f32,
    chart_width: f32,
    chart_height: f32,
    style: StyleRefinement,
}

impl BarChart {
    pub fn new(data: Signal<Vec<BarGroup>>) -> Self {
        Self {
            data,
            colors: vec![
                Color::rgba8(100, 143, 255, 255),
                Color::rgba8(120, 200, 120, 255),
                Color::rgba8(255, 180, 100, 255),
                Color::rgba8(255, 120, 120, 255),
                Color::rgba8(180, 130, 255, 255),
                Color::rgba8(100, 200, 200, 255),
                Color::rgba8(255, 200, 80, 255),
            ],
            legend: Vec::new(),
            max_value: None,
            min_max_value: 0.0,
            chart_width: 600.0,
            chart_height: 200.0,
            style: StyleRefinement::default(),
        }
    }

    pub fn colors(mut self, c: Vec<Color>) -> Self {
        self.colors = c;
        self
    }
    pub fn legend(mut self, l: Vec<String>) -> Self {
        self.legend = l;
        self
    }
    pub fn max_value(mut self, m: f32) -> Self {
        self.max_value = Some(m);
        self
    }
    /// Floor for auto-computed max value: actual max is never lower than this.
    /// Prevents tiny values from filling the chart.
    pub fn min_max_value(mut self, m: f32) -> Self {
        self.min_max_value = m;
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

impl Styled for BarChart {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for BarChart {
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

        // Write height_dim directly through arena's component tables
        // before `get_mut` so there's no borrow conflict.
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

            element.insert_user_data(BarChartData {
                data_signal: self.data.clone(),
                colors: std::mem::take(&mut self.colors),
                legend: std::mem::take(&mut self.legend),
                max_value: self.max_value,
                min_max_value: self.min_max_value,
                chart_width: self.chart_width,
                chart_height: self.chart_height,
            });
        }
        ctx.register_theme_component(id, &resolved, &role, &self.style);
        id
    }
}

/// Paint data for rendering a bar chart.
pub struct BarChartData {
    pub data_signal: Signal<Vec<BarGroup>>,
    pub colors: Vec<Color>,
    pub legend: Vec<String>,
    pub max_value: Option<f32>,
    pub min_max_value: f32,
    pub chart_width: f32,
    pub chart_height: f32,
}

fn grid_line_color() -> Color {
    Color::rgba8(60, 60, 80, 255)
}

pub(crate) fn paint_bar_chart(
    bd: &BarChartData,
    painter: &mut crate::render::Painter,
    x: f32,
    y: f32,
    _w: f32,
    _h: f32,
    clip: crate::render::ClipInfo,
    xform: glam::Affine2,
    z_index: i32,
) {
    let data = bd.data_signal.read();
    if data.is_empty() {
        return;
    }

    let left_pad = 50.0;
    let right_pad = 16.0;
    let top_pad = 8.0;
    let bottom_pad = 24.0;
    let bar_area_w = bd.chart_width - left_pad - right_pad;
    let bar_area_h = bd.chart_height - top_pad - bottom_pad;
    let bar_area_x = x + left_pad;
    let bar_area_y = y + top_pad;

    let max_val = bd.max_value.unwrap_or_else(|| {
        data.iter()
            .map(|g| g.values.iter().sum::<f32>())
            .fold(0.0f32, f32::max)
            .max(bd.min_max_value)
            .max(1.0)
    });

    let n = data.len();
    let bar_total_w = bar_area_w / n.max(1) as f32;
    let bar_w = (bar_total_w * 0.7).max(2.0);
    let gap = bar_total_w - bar_w;

    // grid lines
    let grid_lines = 4;
    for i in 0..=grid_lines {
        let gy = bar_area_y + bar_area_h - (bar_area_h * i as f32 / grid_lines as f32);
        let _val = max_val * i as f32 / grid_lines as f32;
        let grid_rect = Rect::new(x + left_pad - 4.0, gy, bar_area_w + 8.0, 1.0);
        painter.fill_rounded_rect(
            grid_rect,
            grid_line_color(),
            crate::style::CornerRadii::ZERO,
            clip,
            xform,
            z_index,
        );
    }

    // bars
    for (i, group) in data.iter().enumerate() {
        let bar_x = bar_area_x + i as f32 * bar_total_w + gap * 0.5;
        let mut accum_y = bar_area_y + bar_area_h;

        for (j, &val) in group.values.iter().enumerate() {
            let bar_h = (val / max_val) * bar_area_h;
            let color = bd
                .colors
                .get(j)
                .copied()
                .unwrap_or(Color::rgba8(150, 150, 150, 255));
            let bar_rect = Rect::new(bar_x, accum_y - bar_h, bar_w, bar_h);

            painter.fill_rounded_rect(
                bar_rect,
                color,
                crate::style::CornerRadii::ZERO,
                clip,
                xform,
                z_index,
            );

            accum_y -= bar_h;
        }
    }

    // X axis labels via child Text widget mechanism is not available here.
    // Labels will be rendered via child Text widgets positioned below the chart.
}

impl std::fmt::Debug for BarChart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BarChart").finish_non_exhaustive()
    }
}
