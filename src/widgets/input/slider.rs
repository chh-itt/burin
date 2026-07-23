use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::{
    ElementBuilder, EventHandler, InteractionConfig, LayoutConfig, PaintConfig,
};
use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::event::action::{Action, ActionKind, ActionOutcome};
use crate::event::Key;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Dimension, Padding};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};

pub struct Slider {
    value: auralis_signal::Signal<f32>,
    min: f32,
    max: f32,
    step: f32,
    on_changed: Option<Rc<dyn Fn(f32)>>,
    on_change_start: Option<Rc<dyn Fn(f32)>>,
    on_change_end: Option<Rc<dyn Fn(f32)>>,
    disabled: bool,
    width: f32,
    orientation: SliderOrientation,
    style: StyleRefinement,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliderOrientation {
    Horizontal,
    Vertical,
}

impl Slider {
    pub fn new(value: auralis_signal::Signal<f32>) -> Self {
        Self {
            value,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            on_changed: None,
            on_change_start: None,
            on_change_end: None,
            disabled: false,
            width: 200.0,
            orientation: SliderOrientation::Horizontal,
            style: StyleRefinement::default(),
        }
    }

    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }
    pub fn step(mut self, s: f32) -> Self {
        self.step = s;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn orientation(mut self, o: SliderOrientation) -> Self {
        self.orientation = o;
        self
    }
    pub fn on_changed(mut self, f: impl Fn(f32) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
    pub fn on_change_start(mut self, f: impl Fn(f32) + 'static) -> Self {
        self.on_change_start = Some(Rc::new(f));
        self
    }
    pub fn on_change_end(mut self, f: impl Fn(f32) + 'static) -> Self {
        self.on_change_end = Some(Rc::new(f));
        self
    }
}

impl Styled for Slider {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[derive(Clone)]
pub struct SliderPaintData {
    pub value_signal: auralis_signal::Signal<f32>,
    pub min: f32,
    pub max: f32,
    pub disabled: bool,
    pub orientation: SliderOrientation,
    pub track_height: f32,
    pub thumb_radius: f32,
    pub track_color: Color,
    pub fill_color: Color,
    pub thumb_color: Color,
    pub thumb_hover_color: Color,
    pub thumb_press_color: Color,
    pub disabled_fill: Color,
    pub disabled_thumb: Color,
}

/// Custom paint for Slider (track / fill / thumb / focus ring).
/// Moved from `platform/window.rs` (audit round 3, ② phase 1).
pub(crate) fn paint_slider(
    sd: &SliderPaintData,
    element: &crate::core::element::Element,
    painter: &mut crate::render::Painter,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    clip: crate::render::ClipInfo,
    xform: glam::Affine2,
    z_index: i32,
) {
    use crate::core::config::StateFlags;
    let val = sd.value_signal.read();
    let ratio = ((val - sd.min) / (sd.max - sd.min)).clamp(0.0, 1.0);
    let is_vert = matches!(sd.orientation, SliderOrientation::Vertical);
    let track_h = sd.track_height;
    let thumb_r = sd.thumb_radius;

    let track_c = sd.track_color;
    let fill_c = if sd.disabled {
        sd.disabled_fill
    } else {
        sd.fill_color
    };
    let thumb_c = if sd.disabled {
        sd.disabled_thumb
    } else {
        sd.thumb_color
    };

    if is_vert {
        let track_x = x + (w - track_h) * 0.5;
        let track_rect = crate::style::Rect::new(track_x, y, track_h, h);
        painter.fill_rounded_rect(
            track_rect,
            track_c,
            crate::style::CornerRadii::all(track_h * 0.5),
            clip,
            xform,
            z_index,
        );
        let fill_h = h * ratio;
        painter.fill_rounded_rect(
            crate::style::Rect::new(track_x, y + h - fill_h, track_h, fill_h),
            fill_c,
            crate::style::CornerRadii::all(track_h * 0.5),
            clip,
            xform,
            z_index,
        );
        let thumb_size = thumb_r * 2.0;
        let thumb_y = (y + h - fill_h - thumb_r).clamp(y, y + h - thumb_size);
        painter.fill_rounded_rect(
            crate::style::Rect::new(
                track_x + track_h * 0.5 - thumb_r,
                thumb_y,
                thumb_size,
                thumb_size,
            ),
            thumb_c,
            crate::style::CornerRadii::all(thumb_r),
            clip,
            xform,
            z_index,
        );
    } else {
        let track_y = y + (h - track_h) * 0.5;
        let track_rect = crate::style::Rect::new(x, track_y, w, track_h);
        painter.fill_rounded_rect(
            track_rect,
            track_c,
            crate::style::CornerRadii::all(track_h * 0.5),
            clip,
            xform,
            z_index,
        );
        let fill_w = w * ratio;
        let thumb_size = thumb_r * 2.0;
        if ratio > 0.0 {
            painter.fill_rounded_rect(
                crate::style::Rect::new(x, track_y, fill_w.min(w), track_h),
                fill_c,
                crate::style::CornerRadii::all(track_h * 0.5),
                clip,
                xform,
                z_index,
            );
        }
        // Thumb shadow
        if !sd.disabled {
            painter.push_local_shadow(
                crate::style::styled::Shadow::new(
                    crate::style::Color::BLACK.with_alpha(0.15),
                    0.0,
                    2.0,
                    4.0,
                ),
                thumb_size,
                thumb_size,
                crate::style::CornerRadii::all(thumb_r),
            );
        }
        let thumb_x = (x + fill_w - thumb_r).clamp(x, x + w - thumb_size);
        let thumb_center_y = track_y + track_h * 0.5 - thumb_r;
        painter.fill_rounded_rect(
            crate::style::Rect::new(thumb_x, thumb_center_y, thumb_size, thumb_size),
            thumb_c,
            crate::style::CornerRadii::all(thumb_r),
            clip,
            xform,
            z_index,
        );
    }
    // Focus ring: draw after thumb so it's on top of slider content
    let focused = element.state.get().contains(StateFlags::FOCUSED);
    if focused {
        let ow = 2.0;
        let oc = Color::rgba8(59, 130, 246, 255);
        let radius = if is_vert { thumb_r } else { 4.0 };
        let gap = ow;
        // outer ring
        painter.stroke_rounded_rect(
            crate::style::Rect::new(x - gap, y - gap, w + gap * 2.0, h + gap * 2.0),
            oc,
            ow,
            crate::style::CornerRadii::all(radius + ow * 0.5),
            clip,
            xform,
            z_index + 1,
        );
    }
}

impl Widget for Slider {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::INTERACTION | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Interactive(InteractiveRole::Slider {
            size: crate::theme::ControlSize::Medium,
        });
        let sl_style = match ctx.theme.resolve_component(&role) {
            ResolvedComponentStyle::Slider(s) => s,
            _ => unreachable!(),
        };
        let val = self.value.read();
        let disabled = self.disabled;
        let is_vert = self.orientation == SliderOrientation::Vertical;

        let (layout_w, layout_h) = if is_vert {
            (
                Dimension::Pixels(sl_style.thumb_radius * 2.0 + 4.0),
                self.style.height.unwrap_or(Dimension::Pixels(self.width)),
            )
        } else {
            (
                self.style.width.unwrap_or(Dimension::Pixels(self.width)),
                Dimension::Pixels(sl_style.thumb_radius * 2.0 + 4.0),
            )
        };

        let bg = self.style.background.unwrap_or(ctx.theme.scheme.surface);

        let layout = LayoutConfig {
            width: layout_w,
            height: layout_h,
            padding: self.style.padding.unwrap_or(Padding::all(4.0)),
            ..LayoutConfig::default()
        };

        let track_dim: Rc<Cell<f32>> = Rc::new(Cell::new(self.width));

        let mut events = EventHandler::new();
        {
            let track_dim_resize = track_dim.clone();
            let is_vert_r = is_vert;
            events = events.on_resize(move |w, h| {
                track_dim_resize.set(if is_vert_r { h } else { w });
            });
        }
        if !disabled {
            let sig = self.value.clone();
            let min_v = self.min;
            let max_v = self.max;
            let step_v = self.step;
            let start_cb = self.on_change_start.clone();
            let end_cb = self.on_change_end.clone();
            let changed_cb = self.on_changed.clone();
            let isvert = is_vert;

            // Click
            {
                let sig2 = sig.clone();
                let td2 = track_dim.clone();
                let start_cb2 = start_cb.clone();
                let end_cb2 = end_cb.clone();
                let changed_cb2 = changed_cb.clone();
                let start_val = self.value.read();
                events = events.on_click_at(move |pos| {
                    let coord = if isvert { pos.y } else { pos.x };
                    let ratio = (coord / td2.get()).clamp(0.0, 1.0);
                    let r = if isvert { 1.0 - ratio } else { ratio };
                    let raw = min_v + r * (max_v - min_v);
                    let stepped = ((raw / step_v).round() * step_v).clamp(min_v, max_v);
                    if let Some(ref cb) = start_cb2 {
                        cb(start_val);
                    }
                    sig2.set(stepped);
                    if let Some(ref cb) = changed_cb2 {
                        cb(stepped);
                    }
                    if let Some(ref cb) = end_cb2 {
                        cb(sig2.read());
                    }
                });
            }

            // Drag
            {
                let sig3 = sig.clone();
                let td3 = track_dim.clone();
                let start_cb3 = start_cb.clone();
                let changed_cb3 = changed_cb.clone();
                let drag_started = Cell::new(false);
                events = events.on_drag_update(move |pos, _abs| {
                    if !drag_started.get() {
                        drag_started.set(true);
                        if let Some(ref cb) = start_cb3 {
                            cb(sig3.read());
                        }
                    }
                    let coord = if isvert { pos.y } else { pos.x };
                    let ratio = (coord / td3.get()).clamp(0.0, 1.0);
                    let r = if isvert { 1.0 - ratio } else { ratio };
                    let raw = min_v + r * (max_v - min_v);
                    let stepped = ((raw / step_v).round() * step_v).clamp(min_v, max_v);
                    sig3.set(stepped);
                    if let Some(ref cb) = changed_cb3 {
                        cb(stepped);
                    }
                });
                let sig4 = sig.clone();
                let end_cb3 = end_cb.clone();
                events = events.on_drag_end(move |_local, _abs| {
                    if let Some(ref cb) = end_cb3 {
                        cb(sig4.read());
                    }
                });
            }

            // Keyboard: Arrow keys + Home/End.
            let is_h_kb = !is_vert;
            events = events.on_action(move |action: &Action| -> ActionOutcome {
                match (is_h_kb, action.kind) {
                    (true, ActionKind::MoveLeft)
                    | (true, ActionKind::MoveRight)
                    | (true, ActionKind::MoveHome)
                    | (true, ActionKind::MoveEnd)
                    | (false, ActionKind::MoveUp)
                    | (false, ActionKind::MoveDown) => ActionOutcome::Consumed,
                    _ => ActionOutcome::Unhandled,
                }
            });
            let sig_kb = self.value.clone();
            let step_kb = self.step;
            let min_kb = self.min;
            let max_kb = self.max;
            let changed_kb = self.on_changed.clone();
            let is_h_kb2 = is_h_kb;
            events = events.on_key_down(move |key: Key, _mods| -> bool {
                let v = sig_kb.read();
                let new_v = match (is_h_kb2, key) {
                    (_, Key::Home) => min_kb,
                    (_, Key::End) => max_kb,
                    (true, Key::ArrowLeft) | (false, Key::ArrowDown) => (v - step_kb).max(min_kb),
                    (true, Key::ArrowRight) | (false, Key::ArrowUp) => (v + step_kb).min(max_kb),
                    _ => return false,
                };
                sig_kb.set(new_v);
                if let Some(ref cb) = changed_kb {
                    cb(new_v);
                }
                true
            });
        }

        let id = ElementBuilder::new()
            .with_components(self.component_mask())
            .layout(layout)
            .interaction(InteractionConfig {
                events: Some(events),
                enabled: !disabled,
                focusable: !disabled,
                cursor: crate::platform::CursorIcon::POINTER,
                ..InteractionConfig::default()
            })
            .paint(PaintConfig {
                background: Some(bg),
                ..PaintConfig::default()
            })
            .accessibility(accesskit::Role::Slider, format!("{:.0}", val))
            .build(ctx);

        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };

            el.insert_user_data(SliderPaintData {
                value_signal: self.value.clone(),
                min: self.min,
                max: self.max,
                disabled,
                orientation: self.orientation,
                track_height: sl_style.track_height,
                thumb_radius: sl_style.thumb_radius,
                track_color: sl_style.track_color,
                fill_color: sl_style.fill_color,
                thumb_color: sl_style.thumb_color,
                thumb_hover_color: sl_style.hover_thumb,
                thumb_press_color: sl_style.pressed_thumb,
                disabled_fill: sl_style.disabled_track,
                disabled_thumb: sl_style.disabled_thumb,
            });

            // Mark element dirty whenever the signal changes so the custom
            // paint_slider path re-reads the value and re-renders.
            let dirty_el = el.dirty.clone();
            let sig_sub = self.value.clone();
            crate::core::signal_bridge::subscribe_owned(id, &sig_sub, move || {
                crate::core::element::Element::mark_surface_dirty_remote(&dirty_el, id);
            });
        }

        ctx.register_theme_component(
            id,
            &ResolvedComponentStyle::Slider(sl_style.clone()),
            &role,
            &self.style,
        );

        id
    }
}

impl std::fmt::Debug for Slider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slider")
            .field("range", &(self.min, self.max))
            .field("step", &self.step)
            .field("orientation", &self.orientation)
            .finish_non_exhaustive()
    }
}
