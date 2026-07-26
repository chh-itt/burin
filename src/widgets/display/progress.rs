use auralis_signal::Signal;
use kurbo::BezPath;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Color;
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
use crate::theme::Intent;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProgressKind {
    Linear,
    Circular,
}

/// A linear or circular progress indicator.
///
/// Supports determinate (0..1 value) and indeterminate (spinning) modes.
pub struct Progress {
    value: Signal<f64>,
    kind: ProgressKind,
    indeterminate: bool,
    style: StyleRefinement,
}

impl Progress {
    pub fn new(value: Signal<f64>) -> Self {
        Self {
            value,
            kind: ProgressKind::Linear,
            indeterminate: false,
            style: StyleRefinement::default(),
        }
    }
    pub fn kind(mut self, k: ProgressKind) -> Self {
        self.kind = k;
        self
    }
    pub fn indeterminate(mut self) -> Self {
        self.indeterminate = true;
        self
    }
}

impl Styled for Progress {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Progress {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Interactive(InteractiveRole::Progress {
            intent: Intent::Primary,
        });
        let resolved = ctx.theme.resolve_component(&role);
        let style = match &resolved {
            ResolvedComponentStyle::Progress(s) => s,
            _ => unreachable!(),
        };
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            let circular = matches!(self.kind, ProgressKind::Circular);
            let elem_h = if circular {
                style.circular_size
            } else {
                style.height
            };
            let elem_w = if circular { style.circular_size } else { 200.0 };
            element.set_preferred_height(elem_h);
            element.set_preferred_width(Some(elem_w));
            self.style.height = Some(crate::style::Dimension::Pixels(elem_h));
            self.style.width = Some(crate::style::Dimension::Pixels(elem_w));
            if self.indeterminate {
                element
                    .state
                    .set(element.state.get() | crate::core::config::StateFlags::INDETERMINATE);
            }
            element.set_accessible_role(accesskit::Role::ProgressIndicator);
            element.set_accessible_value(self.value.read());
            element.set_accessible_min(0.0);
            element.set_accessible_max(100.0);

            let dirty = element.dirty.clone();
            crate::core::signal_bridge::subscribe_owned(id, &self.value, move || {
                dirty.set(dirty.get() | DirtyFlags::REPAINT);
            });

            if self.indeterminate {
                // Continuous sweep via the renewal model: the frame_tick
                // re-acquires the wake every frame it actually runs. The
                // tick pass skips hidden/inactive elements, so hiding the
                // spinner stops the renewal and the wake decays at the next
                // sweep — a hidden spinner costs nothing (Phase 3).
                let wake_key = crate::core::scheduler::keys::element_key(
                    crate::core::scheduler::keys::NS_SPINNER,
                    id,
                );
                crate::core::scheduler::acquire_element_continuous(wake_key);

                let tick_id = id;
                element.set_frame_tick(Box::new(move || {
                    if !crate::core::dirty_registry::is_visible_chain_fast(tick_id) {
                        return;
                    }
                    // Offscreen gate: a spinner scrolled out of view stops
                    // renewing its wake and producing dirty — scrolling back
                    // always produces frames, so this re-evaluates on entry.
                    let viewport = crate::core::frame_driver::CURRENT_VIEWPORT.with(|c| c.get());
                    if crate::core::dirty_registry::is_offscreen(tick_id, viewport) {
                        return;
                    }
                    crate::core::scheduler::acquire_element_continuous(wake_key);
                    // Full repaint semantics: the sweep angle is f(now), so
                    // the surface genuinely changes every frame — bump the
                    // generations so caches invalidate (and the over-render
                    // detector sees a real change).
                    crate::core::dirty_registry::mark_widget_repaint(tick_id);
                }));

                let unmount = Rc::new(RefCell::new(Some(Box::new(move || {
                    crate::core::scheduler::release_continuous(wake_key);
                }) as Box<dyn FnOnce()>)));
                crate::core::element::with_ct_mut(|ct| {
                    ct.lc.entry(id).or_default().on_unmount = Some(unmount);
                });
            }

            element.insert_user_data(ProgressData {
                value_signal: self.value.clone(),
                indeterminate: self.indeterminate,
                kind: self.kind,
                track_color: style.track_color,
                fill_color: style.fill_color,
            });

            if let Some(zi) = self.style.z_index {
                element.set_z_index(zi);
            }
            if let Some(o) = self.style.opacity {
                element.set_opacity(o);
            }
            if let Some(tx) = self.style.transform {
                element.set_transform(Some(tx));
            }
        }
        ctx.register_theme_component(id, &resolved, &role, &self.style);
        id
    }
}

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("kind", &self.kind)
            .field("value", &self.value.read())
            .field("indeterminate", &self.indeterminate)
            .finish_non_exhaustive()
    }
}

pub struct ProgressData {
    pub value_signal: Signal<f64>,
    pub indeterminate: bool,
    pub kind: ProgressKind,
    pub track_color: Color,
    pub fill_color: Color,
}

/// Phase (0.0..1.0) of the indeterminate linear sweep. Period: 500 ms.
/// Pure function of the animation timeline — see `clock::animation_millis`.
pub fn linear_sweep_phase(ms: u64) -> f32 {
    (ms % 500) as f32 / 500.0
}

/// End angle (degrees) of the indeterminate circular sweep, starting at
/// -90° (top). Period: 1000 ms. Pure function of the animation timeline.
pub fn circular_sweep_deg(ms: u64) -> f32 {
    -90.0 + 360.0 * ((ms % 1000) as f32 / 1000.0)
}

/// Custom paint for Progress (linear bar / circular ring).
/// Moved from `platform/window.rs` (audit round 3, ② phase 1) — widget
/// paint belongs next to the widget, not in the window host.
pub(crate) fn paint_progress(
    _element: &crate::core::element::Element,
    pd: &ProgressData,
    painter: &mut crate::render::Painter,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    clip: crate::render::ClipInfo,
    xform: glam::Affine2,
    z_index: i32,
) {
    use crate::style::Rect;
    let val = pd.value_signal.read().clamp(0.0, 100.0) as f32;
    let track = Rect::new(x, y, w, h);
    match pd.kind {
        ProgressKind::Linear => {
            painter.fill_rounded_rect(
                track,
                pd.track_color,
                crate::style::CornerRadii::all(h * 0.5),
                clip,
                xform,
                z_index,
            );
            if pd.indeterminate {
                let t = linear_sweep_phase(crate::core::clock::animation_millis());
                let bar_w = w * 0.3;
                let bar_x = x + (w - bar_w) * t;
                painter.fill_rounded_rect(
                    Rect::new(bar_x, y, bar_w, h),
                    pd.fill_color,
                    crate::style::CornerRadii::all(h * 0.5),
                    clip,
                    xform,
                    z_index,
                );
            } else {
                let fill_w = w * (val / 100.0);
                painter.fill_rounded_rect(
                    Rect::new(x, y, fill_w, h),
                    pd.fill_color,
                    crate::style::CornerRadii::all(h * 0.5),
                    clip,
                    xform,
                    z_index,
                );
            }
        }
        ProgressKind::Circular => {
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            let r = w.min(h) * 0.45;
            let sw = 5.0;

            let xform_local = xform * glam::Affine2::from_translation(glam::Vec2::new(cx, cy));
            let stroke = kurbo::Stroke {
                width: sw,
                ..Default::default()
            };

            // Track ring — full 360° arc centered at (cx, cy)
            let track_path = arc_path(r as f64, 0.0_f64, 360.0_f64);
            painter.stroke_path(
                &track_path,
                stroke.clone(),
                crate::style::Brush::Solid(pd.track_color),
                clip,
                xform_local,
                z_index,
            );

            // Fill arc — sweeps from -90° (top) clockwise
            let sweep_deg = if pd.indeterminate {
                circular_sweep_deg(crate::core::clock::animation_millis())
            } else {
                -90.0 + val / 100.0 * 360.0
            };
            if (sweep_deg + 90.0).abs() > 0.5 {
                let fill_path = arc_path(r as f64, -90.0_f64, sweep_deg as f64);
                painter.stroke_path(
                    &fill_path,
                    stroke,
                    crate::style::Brush::Solid(pd.fill_color),
                    clip,
                    xform_local,
                    z_index,
                );
            }
        }
    }
}

/// Build a circular arc path from `start_deg` to `end_deg` (degrees),
/// centered at (0, 0) with the given radius.
/// Decomposes into cubic Bézier segments (one per 90°).
pub fn arc_path(radius: f64, start_deg: f64, end_deg: f64) -> BezPath {
    let start = start_deg.to_radians();
    let end = end_deg.to_radians();
    let sweep = end - start;
    if sweep.abs() <= 0.001 {
        let mut path = BezPath::new();
        path.move_to(kurbo::Point::new(
            radius * start.cos(),
            radius * start.sin(),
        ));
        return path;
    }
    // Split into segments of at most 90°
    let segs = (sweep.abs() / (std::f64::consts::FRAC_PI_2)).ceil() as usize;
    let seg_sweep = sweep / segs as f64;
    let k = (4.0 / 3.0) * (seg_sweep * 0.25).tan(); // cubic approx constant

    let mut path = BezPath::new();
    let mut cx = start;
    let p0 = kurbo::Point::new(radius * cx.cos(), radius * cx.sin());
    path.move_to(p0);

    for _ in 0..segs {
        let nx = cx + seg_sweep;
        let c1 = kurbo::Point::new(
            radius * (cx.cos() - k * cx.sin()),
            radius * (cx.sin() + k * cx.cos()),
        );
        let c2 = kurbo::Point::new(
            radius * (nx.cos() + k * nx.sin()),
            radius * (nx.sin() - k * nx.cos()),
        );
        let p3 = kurbo::Point::new(radius * nx.cos(), radius * nx.sin());
        path.curve_to(c1, c2, p3);
        cx = nx;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_sweep_phase_is_time_pure_with_500ms_period() {
        assert_eq!(linear_sweep_phase(0), 0.0);
        assert!((linear_sweep_phase(250) - 0.5).abs() < 1e-6);
        assert_eq!(linear_sweep_phase(500), 0.0);
        assert!((linear_sweep_phase(1250) - 0.5).abs() < 1e-6);
        let huge = 1_700_000_000_000u64 + 125;
        assert!(
            (linear_sweep_phase(huge) - linear_sweep_phase(125)).abs() < 1e-6,
            "no f32 precision loss on large wall-clock timestamps"
        );
    }

    #[test]
    fn circular_sweep_deg_is_time_pure_with_1000ms_period() {
        assert_eq!(circular_sweep_deg(0), -90.0);
        assert!((circular_sweep_deg(250) - 0.0).abs() < 1e-3);
        assert!((circular_sweep_deg(500) - 90.0).abs() < 1e-3);
        assert_eq!(circular_sweep_deg(1000), -90.0);
        let huge = 1_700_000_000_000u64 + 250;
        assert!((circular_sweep_deg(huge) - circular_sweep_deg(250)).abs() < 1e-3);
    }
}
