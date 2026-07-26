use std::cell::{Cell, RefCell};
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::config::{
    ElementBuilder, EventHandler, FlexWrap, InteractionConfig, LayoutConfig,
};
use crate::core::context::MountContext;
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::TraversalEdgeBehavior;
use crate::platform::portal::PortalHeight;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Hsla};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
use crate::widgets::input::TextInput;
use crate::widgets::overlay::{PopoverGeometry, PopoverPlacement};

const PLANE_SIZE: f32 = 150.0;
const BAR_HEIGHT: f32 = 14.0;
const BAR_WIDTH: f32 = PLANE_SIZE;
const HANDLE_RADIUS: f32 = 6.0;
const PRESET_SIZE: f32 = 20.0;
const GAP: f32 = 8.0;
const PANEL_PADDING: f32 = 12.0;
const PANEL_WIDTH: f32 = PLANE_SIZE + PANEL_PADDING * 2.0;
const HEX_FONT_SIZE: f32 = 13.0;

/// A color picker with hue, saturation, and alpha controls.
pub struct ColorPicker {
    color: Signal<Color>,
    presets: Vec<Color>,
    show_hex_input: bool,
    show_alpha: bool,
    on_changed: Option<Rc<dyn Fn(Color)>>,
    style: StyleRefinement,
}

impl ColorPicker {
    pub fn new(color: Signal<Color>) -> Self {
        Self {
            color,
            presets: default_presets(),
            show_hex_input: true,
            show_alpha: true,
            on_changed: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn presets(mut self, p: Vec<Color>) -> Self {
        self.presets = p;
        self
    }
    pub fn show_hex_input(mut self, v: bool) -> Self {
        self.show_hex_input = v;
        self
    }
    pub fn show_alpha(mut self, v: bool) -> Self {
        self.show_alpha = v;
        self
    }
    pub fn on_changed(mut self, f: impl Fn(Color) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

fn default_presets() -> Vec<Color> {
    (0..12)
        .map(|i| {
            let h = i as f32 * 30.0;
            Hsla::new(h, 0.75, 0.55, 1.0).to_color()
        })
        .collect()
}

impl Styled for ColorPicker {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

// ── Paint data types (read by window.rs custom paint functions) ───

#[derive(Clone)]
pub struct ColorPlanePaintData {
    pub hsla_state: Rc<Cell<Hsla>>,
    pub plane_size: f32,
    pub handle_radius: f32,
    pub handle_color: Color,
    pub handle_border: Color,
}

#[derive(Clone)]
pub struct HueBarPaintData {
    pub hsla_state: Rc<Cell<Hsla>>,
    pub bar_width: f32,
    pub bar_height: f32,
    pub handle_radius: f32,
    pub handle_color: Color,
}

#[derive(Clone)]
pub struct AlphaBarPaintData {
    pub hsla_state: Rc<Cell<Hsla>>,
    pub bar_width: f32,
    pub bar_height: f32,
    pub handle_radius: f32,
    pub handle_color: Color,
    pub checker_light: Color,
    pub checker_dark: Color,
}

// ── Custom paint (moved from platform/window.rs — audit round 3, ② phase 1) ──

/// 2D saturation-lightness plane with a draggable handle.
pub(crate) fn paint_color_plane(
    pd: &ColorPlanePaintData,
    _element: &crate::core::element::Element,
    painter: &mut crate::render::Painter,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    _clip: crate::render::ClipInfo,
    xform: glam::Affine2,
    z_index: i32,
) {
    use crate::render::ClipInfo;
    use crate::style::{CornerRadii, Rect};
    let hsla = pd.hsla_state.get();
    let hue = hsla.h;
    // Use element's own bounds as clip (portal clip is zero)
    let own_clip = ClipInfo::new(Rect::new(x, y, w, h));
    let cell_count = 15;
    let cell_w = w / cell_count as f32;
    let cell_h = h / cell_count as f32;

    // Render as grid of colored cells
    for row in 0..cell_count {
        for col in 0..cell_count {
            let s = col as f32 / (cell_count - 1) as f32;
            let l = 1.0 - row as f32 / (cell_count - 1) as f32;
            let cell_hsla = Hsla::new(hue, s, l, 1.0);
            let cell_color = cell_hsla.to_color();
            let cx = x + col as f32 * cell_w;
            let cy = y + row as f32 * cell_h;
            painter.fill_rounded_rect(
                Rect::new(cx, cy, cell_w, cell_h),
                cell_color,
                CornerRadii::ZERO,
                own_clip,
                xform,
                z_index,
            );
        }
    }

    // Draw handle at current S,L position
    let handle_s = hsla.s.clamp(0.0, 1.0);
    let handle_l = hsla.l.clamp(0.0, 1.0);
    let handle_cx = x + handle_s * w;
    let handle_cy = y + (1.0 - handle_l) * h;
    let hr = pd.handle_radius;
    let handle_rect = Rect::new(handle_cx - hr, handle_cy - hr, hr * 2.0, hr * 2.0);

    // Handle border (white ring)
    painter.stroke_rounded_rect(
        Rect::new(
            handle_cx - hr - 1.0,
            handle_cy - hr - 1.0,
            hr * 2.0 + 2.0,
            hr * 2.0 + 2.0,
        ),
        pd.handle_border,
        2.0,
        CornerRadii::all(hr + 1.0),
        own_clip,
        xform,
        z_index + 1,
    );
    // Handle fill (current color)
    painter.fill_rounded_rect(
        handle_rect,
        pd.handle_color,
        CornerRadii::all(hr),
        own_clip,
        xform,
        z_index + 2,
    );
    // Handle inner dot (current HSLA color at full opacity)
    let inner_hsla = Hsla::new(hue, handle_s, handle_l, 1.0);
    let inner_color = inner_hsla.to_color();
    let ir = hr * 0.55;
    painter.fill_rounded_rect(
        Rect::new(handle_cx - ir, handle_cy - ir, ir * 2.0, ir * 2.0),
        inner_color,
        CornerRadii::all(ir),
        own_clip,
        xform,
        z_index + 3,
    );
}

/// Horizontal hue rainbow bar with a draggable handle.
pub(crate) fn paint_hue_bar(
    pd: &HueBarPaintData,
    _element: &crate::core::element::Element,
    painter: &mut crate::render::Painter,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    _clip: crate::render::ClipInfo,
    xform: glam::Affine2,
    z_index: i32,
) {
    use crate::render::ClipInfo;
    use crate::style::{CornerRadii, Rect};
    let hsla = pd.hsla_state.get();
    let own_clip = ClipInfo::new(Rect::new(x, y, w, h));
    let strip_count = 30;
    let strip_w = w / strip_count as f32;

    // Hue rainbow strips
    for i in 0..strip_count {
        let hue = i as f32 * 360.0 / (strip_count - 1) as f32;
        let strip_hsla = Hsla::new(hue, 1.0, 0.5, 1.0);
        let strip_color = strip_hsla.to_color();
        let sx = x + i as f32 * strip_w;
        painter.fill_rounded_rect(
            Rect::new(sx, y, strip_w, h),
            strip_color,
            CornerRadii::ZERO,
            own_clip,
            xform,
            z_index,
        );
    }

    // Handle
    let handle_ratio = (hsla.h % 360.0) / 360.0;
    let handle_cx = x + handle_ratio * w;
    let handle_cy = y + h / 2.0;
    let hr = pd.handle_radius;
    let handle_rect = Rect::new(handle_cx - hr, handle_cy - hr, hr * 2.0, hr * 2.0);

    painter.stroke_rounded_rect(
        Rect::new(
            handle_cx - hr - 1.0,
            handle_cy - hr - 1.0,
            hr * 2.0 + 2.0,
            hr * 2.0 + 2.0,
        ),
        Color::WHITE,
        2.0,
        CornerRadii::all(hr + 1.0),
        own_clip,
        xform,
        z_index + 1,
    );
    let handle_hsla = Hsla::new(hsla.h, 1.0, 0.5, 1.0);
    painter.fill_rounded_rect(
        handle_rect,
        handle_hsla.to_color(),
        CornerRadii::all(hr),
        own_clip,
        xform,
        z_index + 2,
    );
}

/// Alpha gradient bar over a checkerboard, with a draggable handle.
pub(crate) fn paint_alpha_bar(
    pd: &AlphaBarPaintData,
    _element: &crate::core::element::Element,
    painter: &mut crate::render::Painter,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    _clip: crate::render::ClipInfo,
    xform: glam::Affine2,
    z_index: i32,
) {
    use crate::render::ClipInfo;
    use crate::style::{CornerRadii, Rect};
    let hsla = pd.hsla_state.get();
    let own_clip = ClipInfo::new(Rect::new(x, y, w, h));
    let checker_size = h / 2.0;
    let cols = (w / checker_size).ceil() as i32;
    let rows = (h / checker_size).ceil() as i32;

    // Checkerboard background
    for row in 0..rows {
        for col in 0..cols {
            let is_light = (row + col) % 2 == 0;
            let checker_color = if is_light {
                pd.checker_light
            } else {
                pd.checker_dark
            };
            let cx = x + col as f32 * checker_size;
            let cy = y + row as f32 * checker_size;
            painter.fill_rounded_rect(
                Rect::new(cx, cy, checker_size, checker_size),
                checker_color,
                CornerRadii::ZERO,
                own_clip,
                xform,
                z_index,
            );
        }
    }

    // Alpha gradient overlay (transparent → current color at full alpha)
    let strip_count = 20;
    let strip_w = w / strip_count as f32;
    for i in 0..strip_count {
        let alpha = i as f32 / (strip_count - 1) as f32;
        let strip_hsla = Hsla::new(hsla.h, hsla.s, hsla.l, alpha);
        let strip_color = strip_hsla.to_color();
        let sx = x + i as f32 * strip_w;
        painter.fill_rounded_rect(
            Rect::new(sx, y, strip_w, h),
            strip_color,
            CornerRadii::ZERO,
            own_clip,
            xform,
            z_index + 1,
        );
    }

    // Handle
    let handle_ratio = hsla.a.clamp(0.0, 1.0);
    let handle_cx = x + handle_ratio * w;
    let handle_cy = y + h / 2.0;
    let hr = pd.handle_radius;
    let handle_rect = Rect::new(handle_cx - hr, handle_cy - hr, hr * 2.0, hr * 2.0);

    painter.stroke_rounded_rect(
        Rect::new(
            handle_cx - hr - 1.0,
            handle_cy - hr - 1.0,
            hr * 2.0 + 2.0,
            hr * 2.0 + 2.0,
        ),
        Color::WHITE,
        2.0,
        CornerRadii::all(hr + 1.0),
        own_clip,
        xform,
        z_index + 2,
    );
    painter.fill_rounded_rect(
        handle_rect,
        pd.handle_color,
        CornerRadii::all(hr),
        own_clip,
        xform,
        z_index + 3,
    );
}

impl Widget for ColorPicker {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Interactive(InteractiveRole::ColorPicker {
            size: crate::theme::ControlSize::Medium,
        });
        let cp_style = match ctx.theme.resolve_component(&role) {
            ResolvedComponentStyle::ColorPicker(s) => s,
            _ => unreachable!(),
        };

        let on_changed_cb = self.on_changed.clone();
        let initial_color = self.color.read();
        let hsla_state: Rc<Cell<Hsla>> = Rc::new(Cell::new(Hsla::from_color(initial_color)));

        // Hex signal + sync helper (created early so interaction handlers can capture it)
        let hex_sig = Signal::new(initial_color.to_string());
        let sync_hex: Rc<dyn Fn(&Color)> = {
            let hs = hex_sig.clone();
            Rc::new(move |c: &Color| {
                let h = c.to_string();
                if h != hs.read() {
                    hs.set(h);
                }
            })
        };

        // ── Build popover content panel ──
        let panel_id = ctx.arena.allocate();
        ctx.preallocate(panel_id, components::LAYOUT | components::LIFECYCLE);
        {
            let Some(panel) = ctx.arena.get_mut(panel_id) else {
                return panel_id;
            };
            panel.set_layout_direction(crate::core::LayoutDirection::Vertical);
            panel.set_gap(GAP);
            panel.set_padding(crate::style::Padding::all(PANEL_PADDING));
            panel.set_preferred_width(Some(PANEL_WIDTH));
            panel.set_background(cp_style.panel_bg);
            panel.set_corner_radius(cp_style.corner_radius.top_left);
        }

        // ── Subscribe: propagate color changes to on_changed callback ──
        {
            let color_read = self.color.clone();
            crate::core::signal_bridge::subscribe_owned(panel_id, &self.color, move || {
                if let Some(ref cb) = on_changed_cb {
                    cb(color_read.read());
                }
            });
        }

        // Collectors for per-element dirty flags (for manual repaint triggers)
        let dirty_cells: Rc<RefCell<Vec<Rc<Cell<DirtyFlags>>>>> = Rc::new(RefCell::new(Vec::new()));

        // ── Helper: mark all tracked elements dirty ──
        let mark_all = {
            let dc = dirty_cells.clone();
            Rc::new(move || {
                for cell in dc.borrow().iter() {
                    cell.set(cell.get() | DirtyFlags::REPAINT);
                }
            })
        };

        // ── 2D SV Plane ──
        {
            let hsla = hsla_state.clone();
            let cs = self.color.clone();
            let plane_size = PLANE_SIZE;
            let mark = mark_all.clone();
            let sh = sync_hex.clone();

            let mut events = EventHandler::new();

            {
                let hsla2 = hsla.clone();
                let cs2 = cs.clone();
                let mark2 = mark.clone();
                let sh2 = sh.clone();
                events = events.on_click_at(move |pos| {
                    let s = (pos.x / plane_size).clamp(0.0, 1.0);
                    let l = 1.0 - (pos.y / plane_size).clamp(0.0, 1.0);
                    let mut cur = hsla2.get();
                    cur.s = s;
                    cur.l = l;
                    let c = cur.to_color();
                    hsla2.set(cur);
                    cs2.set(c);
                    sh2(&c);
                    mark2();
                });
            }
            {
                let hsla2 = hsla.clone();
                let cs2 = cs.clone();
                let mark2 = mark.clone();
                let sh2 = sh.clone();
                events = events.on_drag_update(move |pos, _abs| {
                    let s = (pos.x / plane_size).clamp(0.0, 1.0);
                    let l = 1.0 - (pos.y / plane_size).clamp(0.0, 1.0);
                    let mut cur = hsla2.get();
                    cur.s = s;
                    cur.l = l;
                    let c = cur.to_color();
                    hsla2.set(cur);
                    cs2.set(c);
                    sh2(&c);
                    mark2();
                });
                let hsla3 = hsla.clone();
                events = events.on_drag_end(move |_local, _abs| {
                    let _ = hsla3.get().to_color();
                });
            }

            let eid = ElementBuilder::new()
                .with_components(
                    components::STYLE
                        | components::LAYOUT
                        | components::INTERACTION
                        | components::LIFECYCLE,
                )
                .layout(LayoutConfig {
                    width: crate::style::Dimension::Pixels(PLANE_SIZE),
                    height: crate::style::Dimension::Pixels(PLANE_SIZE),
                    ..LayoutConfig::default()
                })
                .interaction(InteractionConfig {
                    events: Some(events),
                    enabled: true,
                    focusable: true,
                    cursor: crate::platform::CursorIcon::CROSSHAIR,
                    ..InteractionConfig::default()
                })
                .build(ctx);
            {
                let Some(el) = ctx.arena.get_mut(eid) else {
                    return eid;
                };
                el.insert_user_data(ColorPlanePaintData {
                    hsla_state: hsla_state.clone(),
                    plane_size: PLANE_SIZE,
                    handle_radius: HANDLE_RADIUS,
                    handle_color: cp_style.plane_handle_color,
                    handle_border: cp_style.plane_handle_border,
                });
                dirty_cells.borrow_mut().push(el.dirty.clone());
            }
            ctx.arena.add_child(panel_id, eid);
        }

        // ── Hue Bar ──
        {
            let hsla = hsla_state.clone();
            let cs = self.color.clone();
            let bar_w = BAR_WIDTH;
            let mark = mark_all.clone();
            let sh = sync_hex.clone();

            let mut events = EventHandler::new();
            {
                let hsla2 = hsla.clone();
                let cs2 = cs.clone();
                let mark2 = mark.clone();
                let sh2 = sh.clone();
                events = events.on_click_at(move |pos| {
                    let h = (pos.x / bar_w).clamp(0.0, 1.0) * 360.0;
                    let mut cur = hsla2.get();
                    cur.h = h;
                    let c = cur.to_color();
                    hsla2.set(cur);
                    cs2.set(c);
                    sh2(&c);
                    mark2();
                });
            }
            {
                let hsla2 = hsla.clone();
                let cs2 = cs.clone();
                let mark2 = mark.clone();
                let sh2 = sh.clone();
                events = events.on_drag_update(move |pos, _abs| {
                    let h = (pos.x / bar_w).clamp(0.0, 1.0) * 360.0;
                    let mut cur = hsla2.get();
                    cur.h = h;
                    let c = cur.to_color();
                    hsla2.set(cur);
                    cs2.set(c);
                    sh2(&c);
                    mark2();
                });
            }

            let eid = ElementBuilder::new()
                .with_components(
                    components::STYLE
                        | components::LAYOUT
                        | components::INTERACTION
                        | components::LIFECYCLE,
                )
                .layout(LayoutConfig {
                    width: crate::style::Dimension::Pixels(BAR_WIDTH),
                    height: crate::style::Dimension::Pixels(BAR_HEIGHT),
                    ..LayoutConfig::default()
                })
                .interaction(InteractionConfig {
                    events: Some(events),
                    enabled: true,
                    focusable: true,
                    cursor: crate::platform::CursorIcon::POINTER,
                    ..InteractionConfig::default()
                })
                .build(ctx);
            {
                let Some(el) = ctx.arena.get_mut(eid) else {
                    return eid;
                };
                el.insert_user_data(HueBarPaintData {
                    hsla_state: hsla_state.clone(),
                    bar_width: BAR_WIDTH,
                    bar_height: BAR_HEIGHT,
                    handle_radius: HANDLE_RADIUS,
                    handle_color: cp_style.slider_handle_color,
                });
                dirty_cells.borrow_mut().push(el.dirty.clone());
            }
            ctx.arena.add_child(panel_id, eid);
        }

        // ── Alpha Bar ──
        if self.show_alpha {
            let hsla = hsla_state.clone();
            let cs = self.color.clone();
            let bar_w = BAR_WIDTH;
            let mark = mark_all.clone();
            let sh = sync_hex.clone();

            let mut events = EventHandler::new();
            {
                let hsla2 = hsla.clone();
                let cs2 = cs.clone();
                let mark2 = mark.clone();
                let sh2 = sh.clone();
                events = events.on_click_at(move |pos| {
                    let a = (pos.x / bar_w).clamp(0.0, 1.0);
                    let mut cur = hsla2.get();
                    cur.a = a;
                    let c = cur.to_color();
                    hsla2.set(cur);
                    cs2.set(c);
                    sh2(&c);
                    mark2();
                });
            }
            {
                let hsla2 = hsla.clone();
                let cs2 = cs.clone();
                let mark2 = mark.clone();
                let sh2 = sh.clone();
                events = events.on_drag_update(move |pos, _abs| {
                    let a = (pos.x / bar_w).clamp(0.0, 1.0);
                    let mut cur = hsla2.get();
                    cur.a = a;
                    let c = cur.to_color();
                    hsla2.set(cur);
                    cs2.set(c);
                    sh2(&c);
                    mark2();
                });
            }

            let eid = ElementBuilder::new()
                .with_components(
                    components::STYLE
                        | components::LAYOUT
                        | components::INTERACTION
                        | components::LIFECYCLE,
                )
                .layout(LayoutConfig {
                    width: crate::style::Dimension::Pixels(BAR_WIDTH),
                    height: crate::style::Dimension::Pixels(BAR_HEIGHT),
                    ..LayoutConfig::default()
                })
                .interaction(InteractionConfig {
                    events: Some(events),
                    enabled: true,
                    focusable: true,
                    cursor: crate::platform::CursorIcon::POINTER,
                    ..InteractionConfig::default()
                })
                .build(ctx);
            {
                let Some(el) = ctx.arena.get_mut(eid) else {
                    return eid;
                };
                el.insert_user_data(AlphaBarPaintData {
                    hsla_state: hsla_state.clone(),
                    bar_width: BAR_WIDTH,
                    bar_height: BAR_HEIGHT,
                    handle_radius: HANDLE_RADIUS,
                    handle_color: cp_style.slider_handle_color,
                    checker_light: Color::rgba8(200, 200, 200, 255),
                    checker_dark: Color::rgba8(140, 140, 140, 255),
                });
                dirty_cells.borrow_mut().push(el.dirty.clone());
            }
            ctx.arena.add_child(panel_id, eid);
        }

        // ── Preview + Hex row ──
        if self.show_hex_input {
            let row_id = ctx.arena.allocate();
            {
                let Some(row) = ctx.arena.get_mut(row_id) else {
                    return row_id;
                };
                row.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                row.set_gap(6.0);
                row.set_preferred_height(28.0);
                row.set_preferred_width(Some(BAR_WIDTH));
            }

            // Preview swatch
            let preview_id = ctx.arena.allocate();
            {
                let Some(preview) = ctx.arena.get_mut(preview_id) else {
                    return preview_id;
                };
                preview.set_preferred_width(Some(28.0));
                preview.set_preferred_height(28.0);
                preview.set_corner_radius(4.0);
                preview.set_border_width(1.0);
                preview.set_border_color(cp_style.preview_border);
                preview.set_background(initial_color);
            }
            {
                let pv_id = preview_id;
                let pv_dirty = {
                    let Some(el) = ctx.arena.get_mut(preview_id) else {
                        return preview_id;
                    };
                    el.dirty.clone()
                };
                let color_read = self.color.clone();
                crate::core::signal_bridge::subscribe_owned(panel_id, &self.color, move || {
                    let c = color_read.read();
                    crate::core::element::with_ct_mut(|ct| {
                        ct.style.entry(pv_id).or_default().background = Some(c);
                    });
                    pv_dirty.set(pv_dirty.get() | DirtyFlags::REPAINT);
                });
            }
            ctx.arena.add_child(row_id, preview_id);

            // Hex input
            let hi = TextInput::new(hex_sig.clone());
            let hi_id = Box::new(hi).mount_box(ctx);
            {
                let Some(hi_el) = ctx.arena.get_mut(hi_id) else {
                    return hi_id;
                };
                hi_el.set_preferred_height(28.0);
                hi_el.set_preferred_width(Some(BAR_WIDTH - 34.0));
                hi_el.set_font_size(HEX_FONT_SIZE);
            }
            {
                let hsla = hsla_state.clone();
                let cs = self.color.clone();
                let mark = mark_all.clone();
                let hex_sig2 = hex_sig.clone();
                crate::core::signal_bridge::subscribe_owned(panel_id, &hex_sig, move || {
                    let h = hex_sig2.read().trim().to_uppercase();
                    if let Some(new_hsla) = Hsla::from_hex(&h) {
                        let new_color = new_hsla.to_color();
                        let current = cs.read();
                        if (new_color.r - current.r).abs() < 0.005
                            && (new_color.g - current.g).abs() < 0.005
                            && (new_color.b - current.b).abs() < 0.005
                            && (new_color.a - current.a).abs() < 0.005
                        {
                            return;
                        }
                        cs.set(new_color);
                        hsla.set(new_hsla);
                        mark();
                    }
                });
            }
            ctx.arena.add_child(row_id, hi_id);
            ctx.arena.add_child(panel_id, row_id);
        }

        // ── Preset swatches ──
        if !self.presets.is_empty() {
            let preset_grid_id = ctx.arena.allocate();
            {
                let Some(grid) = ctx.arena.get_mut(preset_grid_id) else {
                    return preset_grid_id;
                };
                grid.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                grid.set_gap(4.0);
                grid.set_flex_wrap(FlexWrap::Wrap);
                grid.set_preferred_width(Some(BAR_WIDTH));
            }

            for &preset_color in &self.presets {
                let hsla = hsla_state.clone();
                let cs = self.color.clone();
                let _mark = mark_all.clone();
                let cell_eid = ElementBuilder::new()
                    .with_components(
                        components::STYLE
                            | components::LAYOUT
                            | components::INTERACTION
                            | components::LIFECYCLE,
                    )
                    .layout(LayoutConfig {
                        width: crate::style::Dimension::Pixels(PRESET_SIZE),
                        height: crate::style::Dimension::Pixels(PRESET_SIZE),
                        ..LayoutConfig::default()
                    })
                    .interaction(InteractionConfig {
                        events: Some(EventHandler::new().on_click({
                            let sh2 = sync_hex.clone();
                            let mark2 = mark_all.clone();
                            move || {
                                let new_hsla = Hsla::from_color(preset_color);
                                hsla.set(new_hsla);
                                cs.set(preset_color);
                                sh2(&preset_color);
                                mark2();
                            }
                        })),
                        enabled: true,
                        cursor: crate::platform::CursorIcon::POINTER,
                        ..InteractionConfig::default()
                    })
                    .build(ctx);
                {
                    let Some(cell) = ctx.arena.get_mut(cell_eid) else {
                        return cell_eid;
                    };
                    cell.set_background(preset_color);
                    cell.set_corner_radius(3.0);
                    cell.set_border_width(1.0);
                    cell.set_border_color(cp_style.preset_border);
                }
                ctx.arena.add_child(preset_grid_id, cell_eid);
            }
            ctx.arena.add_child(panel_id, preset_grid_id);
        }

        // ── Subscribe to external color changes ──
        {
            let hsla = hsla_state.clone();
            let mark = mark_all.clone();
            let sh = sync_hex.clone();
            let color_read = self.color.clone();
            crate::core::signal_bridge::subscribe_owned(panel_id, &self.color, move || {
                let c = color_read.read();
                let new_hsla = Hsla::from_color(c);
                let current = hsla.get();
                if (new_hsla.h - current.h).abs() < 0.1
                    && (new_hsla.s - current.s).abs() < 0.005
                    && (new_hsla.l - current.l).abs() < 0.005
                    && (new_hsla.a - current.a).abs() < 0.005
                {
                    return;
                }
                hsla.set(new_hsla);
                sh(&c);
                mark();
            });
        }

        ctx.register_theme_component(
            panel_id,
            &ResolvedComponentStyle::ColorPicker(cp_style.clone()),
            &role,
            &self.style,
        );

        // ── Open state ──
        let open = Signal::new(false);

        // ── Trigger swatch ──
        let swatch_id = {
            let o = open.clone();
            ElementBuilder::new()
                .with_components(
                    components::STYLE
                        | components::LAYOUT
                        | components::INTERACTION
                        | components::LIFECYCLE,
                )
                .layout(LayoutConfig {
                    width: crate::style::Dimension::Pixels(28.0),
                    height: crate::style::Dimension::Pixels(28.0),
                    ..LayoutConfig::default()
                })
                .interaction(InteractionConfig {
                    events: Some(EventHandler::new().on_click(move || {
                        o.set(!o.read());
                    })),
                    enabled: true,
                    cursor: crate::platform::CursorIcon::POINTER,
                    ..InteractionConfig::default()
                })
                .build(ctx)
        };
        {
            let Some(swatch) = ctx.arena.get_mut(swatch_id) else {
                return swatch_id;
            };
            swatch.set_background(initial_color);
            swatch.set_corner_radius(4.0);
            swatch.set_border_width(2.0);
            swatch.set_border_color(cp_style.trigger_border);
        }
        {
            let swatch_dirty = {
                let sw = ctx.arena.get(swatch_id).unwrap();
                sw.dirty.clone()
            };
            let swatch_eid = swatch_id;
            let color_read = self.color.clone();
            crate::core::signal_bridge::subscribe_owned(swatch_id, &self.color, move || {
                let c = color_read.read();
                crate::core::element::with_ct_mut(|ct| {
                    ct.style.entry(swatch_eid).or_default().background = Some(c);
                });
                swatch_dirty.set(swatch_dirty.get() | DirtyFlags::REPAINT);
            });
        }
        if let Some(lc) = ctx
            .arena
            .component_tables
            .borrow_mut()
            .lc
            .get_mut(&swatch_id)
        {
            lc.component_role = Some(role.clone());
            lc.style_refinement = Some(self.style.clone());
        }
        crate::ecs::register_theme_element(swatch_id);

        // ── Dropdown container (portal) — following Select's pattern exactly ──
        let dropdown_id = ctx.arena.allocate();
        ctx.preallocate(dropdown_id, components::LAYOUT | components::LIFECYCLE);

        let content_height: f32 = PANEL_PADDING * 2.0 + PLANE_SIZE + GAP + BAR_HEIGHT + GAP;
        let content_height = if self.show_alpha {
            content_height + BAR_HEIGHT + GAP
        } else {
            content_height
        };
        let content_height = if self.show_hex_input {
            content_height + 28.0 + GAP
        } else {
            content_height
        };
        let content_height = if !self.presets.is_empty() {
            content_height + PRESET_SIZE + GAP
        } else {
            content_height
        };

        let portal_h: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
        let rv: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let geo_cell: Rc<Cell<PopoverGeometry>> = Rc::new(Cell::new(PopoverGeometry {
            x: 0.0,
            y: 0.0,
            width: PANEL_WIDTH,
            height: 0.0,
            actual_position: crate::widgets::overlay::PopoverPosition::Bottom,
        }));
        let placement = PopoverPlacement {
            min_width: Some(PANEL_WIDTH),
            viewport_margin: 8.0,
            ..Default::default()
        };

        {
            let Some(dd) = ctx.arena.get_mut(dropdown_id) else {
                return dropdown_id;
            };
            dd.set_z_index(ctx.theme.z_index.dropdown);
            dd.z_index_floor = Some(ctx.theme.z_index.dropdown);
            dd.set_padding(crate::style::Padding::ZERO);
            dd.set_flex_shrink(0.0);
            dd.set_background(cp_style.panel_bg);
            dd.set_border_width(1.0);
            dd.set_border_color(cp_style.panel_border);
            dd.set_corner_radius(cp_style.corner_radius.top_left);
            dd.set_shadow(cp_style.shadow);
            dd.set_reactive_visible(rv.clone());
            dd.insert_user_data(PortalHeight(portal_h.clone()));
            dd.insert_user_data(geo_cell.clone());
            dd.insert_user_data(placement);
            dd.insert_user_data(swatch_id);
        }
        ctx.arena.add_child(dropdown_id, panel_id);

        // Register portal + dismiss
        crate::widgets::shared::dropdown::register_dropdown_portal(
            swatch_id,
            dropdown_id,
            open.clone(),
        );

        // Subscribe: open → toggle visibility (matching Select's exact pattern)
        {
            let Some(dirty_sub_el) = ctx.arena.get_mut(dropdown_id) else {
                return dropdown_id;
            };
            let dirty_sub = dirty_sub_el.dirty.clone();
            let dd = dropdown_id;
            let o = open.clone();
            let rv_s = rv.clone();
            let ph = portal_h.clone();
            let vh = content_height;
            crate::core::signal_bridge::subscribe_owned(dropdown_id, &open, move || {
                let is_open = o.read();
                let was = rv_s.get();
                rv_s.set(is_open);
                ph.set(if is_open { vh } else { 0.0 });
                if was && !is_open {
                    crate::event::pop_modal_scope();
                    dirty_sub.set(dirty_sub.get() | DirtyFlags::MEASURE);
                    crate::core::dirty_registry::register_dirty(dd, DirtyFlags::MEASURE);
                    crate::core::dirty_registry::bump_subtree_gen(dd);
                } else if !was && is_open {
                    crate::event::push_modal_scope(dd, TraversalEdgeBehavior::Wrap);
                    dirty_sub.set(dirty_sub.get() | DirtyFlags::MEASURE | DirtyFlags::REPAINT);
                    crate::core::dirty_registry::register_dirty(dd, DirtyFlags::MEASURE);
                    crate::core::dirty_registry::register_dirty(dd, DirtyFlags::REPAINT);
                    crate::core::dirty_registry::bump_subtree_gen(dd);
                }
            });
        }

        // Unmount guard
        crate::widgets::shared::dropdown::register_dropdown_unmount(dropdown_id);

        // Keyboard dismiss (Escape)
        {
            let o = open.clone();
            let events = EventHandler::new().on_action(move |action| {
                if action.kind == crate::event::action::ActionKind::Cancel {
                    o.set(false);
                    crate::event::action::ActionOutcome::Consumed
                } else {
                    crate::event::action::ActionOutcome::Unhandled
                }
            });
            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, dropdown_id);
            }
        }

        // ── Root: holds trigger + dropdown ──
        let root_id = ctx.arena.allocate();
        ctx.preallocate(root_id, components::LAYOUT | components::LIFECYCLE);
        ctx.arena.add_child(root_id, swatch_id);
        ctx.arena.add_child(root_id, dropdown_id);
        {
            let mut ct = ctx.arena.component_tables.borrow_mut();
            let lc = ct.lc.entry(root_id).or_default();
            lc.component_role = Some(ComponentRole::Interactive(InteractiveRole::ColorPicker {
                size: crate::theme::ControlSize::Medium,
            }));
            lc.style_refinement = Some(self.style.clone());
        }
        crate::ecs::register_theme_element(root_id);

        root_id
    }
}

impl std::fmt::Debug for ColorPicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorPicker")
            .field("presets", &self.presets.len())
            .field("show_hex_input", &self.show_hex_input)
            .field("show_alpha", &self.show_alpha)
            .finish_non_exhaustive()
    }
}
