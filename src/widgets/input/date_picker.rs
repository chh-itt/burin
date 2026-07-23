#[cfg(feature = "ext-jiff")]
use jiff::civil::Date;

#[cfg(not(feature = "ext-jiff"))]
use std::cell::RefCell;
use std::rc::Rc;
// Most of this widget is gated on ext-jiff; those imports must carry the
// same gate or `cargo fix` without the feature strips them and breaks the
// feature-enabled build (zero-warning sweep c5caef6 regression).
#[cfg(feature = "ext-jiff")]
use crate::core::config::EventHandler;
#[cfg(feature = "ext-jiff")]
use crate::core::element::DirtyFlags;
#[cfg(feature = "ext-jiff")]
use crate::core::LayoutDirection;
#[cfg(feature = "ext-jiff")]
use crate::ecs::components;
#[cfg(feature = "ext-jiff")]
use crate::event::action::{ActionKind, ActionOutcome};
#[cfg(feature = "ext-jiff")]
use crate::event::types::Key;
#[cfg(feature = "ext-jiff")]
use crate::style::{Color, Dimension, Padding, StyleRefinement, Styled};
#[cfg(feature = "ext-jiff")]
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
#[cfg(feature = "ext-jiff")]
use crate::theme::tokens;
#[cfg(feature = "ext-jiff")]
use crate::widgets::input::Button;
#[cfg(feature = "ext-jiff")]
use crate::widgets::input::TextInput;
#[cfg(feature = "ext-jiff")]
use crate::widgets::overlay::{FlipAxes, PopoverGeometry, PopoverPlacement};
#[cfg(feature = "ext-jiff")]
use auralis_signal::Signal;
#[cfg(feature = "ext-jiff")]
use std::cell::Cell;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
#[cfg(feature = "ext-jiff")]
use crate::widgets::display::Calendar;
#[cfg(feature = "ext-jiff")]
use crate::widgets::display::CalendarShared;

// Used only by the feature-off stub rendering path.
#[cfg(not(feature = "ext-jiff"))]
use crate::render::wgpu::glyphon_bridge::create_buffer;
#[cfg(not(feature = "ext-jiff"))]
use crate::style::TextAlign;

/// Parse a user-typed string into a Date.
#[cfg(feature = "ext-jiff")]
fn try_parse_date(input: &str, fmt: &str) -> Option<Date> {
    jiff::civil::Date::strptime(fmt, input).ok()
}

/// Format a Date for display.
#[cfg(feature = "ext-jiff")]
fn format_date(date: &Option<Date>, fmt: &str, placeholder: &str) -> String {
    date.as_ref()
        .map_or(placeholder.to_string(), |d| d.strftime(fmt).to_string())
}

/// A selected date range with start and end dates.
#[cfg(feature = "ext-jiff")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DateRange {
    pub start: Date,
    pub end: Date,
}

pub struct DatePicker {
    #[cfg(feature = "ext-jiff")]
    date: Signal<Option<Date>>,
    #[cfg(feature = "ext-jiff")]
    range: Option<Signal<Option<DateRange>>>,
    #[cfg(feature = "ext-jiff")]
    min_date: Option<Date>,
    #[cfg(feature = "ext-jiff")]
    max_date: Option<Date>,
    #[cfg(feature = "ext-jiff")]
    date_format: String,
    #[cfg(feature = "ext-jiff")]
    placeholder: String,
    #[cfg(feature = "ext-jiff")]
    clearable: bool,
    #[cfg(feature = "ext-jiff")]
    on_change: Option<Rc<dyn Fn(Option<Date>)>>,
    #[cfg(feature = "ext-jiff")]
    on_range_change: Option<Rc<dyn Fn(Option<DateRange>)>>,
    #[cfg(feature = "ext-jiff")]
    month_names: Option<Vec<String>>,
    #[cfg(feature = "ext-jiff")]
    weekday_names: Option<Vec<String>>,
    #[cfg(feature = "ext-jiff")]
    style: StyleRefinement,
}

impl DatePicker {
    #[cfg(feature = "ext-jiff")]
    pub fn new(date: Signal<Option<Date>>) -> Self {
        Self {
            date,
            range: None,
            min_date: None,
            max_date: None,
            date_format: "%Y-%m-%d".into(),
            placeholder: "Select date...".into(),
            clearable: false,
            on_change: None,
            on_range_change: None,
            month_names: None,
            weekday_names: None,
            style: StyleRefinement::default(),
        }
    }

    #[cfg(feature = "ext-jiff")]
    pub fn new_range(range: Signal<Option<DateRange>>) -> Self {
        Self {
            date: Signal::new(None),
            range: Some(range),
            min_date: None,
            max_date: None,
            date_format: "%Y-%m-%d".into(),
            placeholder: "Select range...".into(),
            clearable: false,
            on_change: None,
            on_range_change: None,
            month_names: None,
            weekday_names: None,
            style: StyleRefinement::default(),
        }
    }

    #[cfg(feature = "ext-jiff")]
    pub fn min_date(mut self, d: Date) -> Self {
        self.min_date = Some(d);
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn max_date(mut self, d: Date) -> Self {
        self.max_date = Some(d);
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn format(mut self, fmt: impl Into<String>) -> Self {
        self.date_format = fmt.into();
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn month_names(mut self, names: Vec<String>) -> Self {
        self.month_names = Some(names);
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn weekday_names(mut self, names: Vec<String>) -> Self {
        self.weekday_names = Some(names);
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn clearable(mut self, v: bool) -> Self {
        self.clearable = v;
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn on_change(mut self, f: impl Fn(Option<Date>) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn on_range_change(mut self, f: impl Fn(Option<DateRange>) + 'static) -> Self {
        self.on_range_change = Some(Rc::new(f));
        self
    }
}

#[cfg(feature = "ext-jiff")]
impl Styled for DatePicker {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[cfg(not(feature = "ext-jiff"))]
impl Widget for DatePicker {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_preferred_height(32.0);
            let buf = Rc::new(RefCell::new(create_buffer(
                "DatePicker (needs ext-jiff)",
                14.0,
                1.5,
                400,
                None,
                None,
                TextAlign::Start,
            )));
            el.set_text_buffer(buf);
        }
        id
    }
}

#[cfg(feature = "ext-jiff")]
impl Widget for DatePicker {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let role = ComponentRole::Interactive(InteractiveRole::Select {
            size: crate::theme::ControlSize::Medium,
        });
        let resolved = match theme.resolve_component(&role) {
            ResolvedComponentStyle::Select(s) => s,
            _ => unreachable!(),
        };

        // ── Helpers ──
        let placeholder = self.placeholder.clone();
        let fmt = self.date_format.clone();
        let min_d = self.min_date;
        let max_d = self.max_date;
        let date_sig = self.date.clone();
        let on_change = self.on_change.clone();
        let is_range = self.range.is_some();
        let range_sig = self.range.clone().unwrap_or_else(|| Signal::new(None));
        let on_range_change = self.on_range_change.clone();
        let rs_cell: Rc<Cell<Option<Date>>> = Rc::new(Cell::new(None));
        let re_cell: Rc<Cell<Option<Date>>> = Rc::new(Cell::new(None));

        // Initial display text
        let initial_text = if is_range {
            "Select range...".to_string()
        } else {
            format_date(&date_sig.read(), &fmt, &placeholder)
        };
        let input_text: Signal<String> = Signal::new(initial_text);
        let suppress_nav: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // Calendar: 7 cols × 36px + 16px padding = 268px
        let cal_width: f32 = 268.0;

        // ── Root container ──
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_layout_direction(LayoutDirection::Vertical);
            if let Some(w) = self.style.width {
                let pixel_fallback = match w {
                    Dimension::Pixels(px) => px,
                    _ => cal_width,
                };
                el.set_preferred_width(Some(pixel_fallback));
                if matches!(w, Dimension::Percent(_)) {
                    el.set_width_dim(Some(w));
                }
            } else {
                el.set_preferred_width(Some(cal_width));
            }
        }

        // ── TRIGGER: HStack(TextInput + calendar icon) ──
        let trigger_id = ctx.arena.allocate();
        ctx.preallocate(trigger_id, components::LAYOUT | components::LIFECYCLE);
        {
            let Some(el) = ctx.arena.get_mut(trigger_id) else {
                return id;
            };
            el.set_layout_direction(LayoutDirection::Horizontal);
            el.set_preferred_width(Some(cal_width));
            el.set_preferred_height(40.0);
            el.set_border_width(1.0);
            el.set_border_color(resolved.trigger_border);
            el.set_corner_radius(6.0);
            el.set_background(resolved.trigger_bg);
            el.set_padding(Padding {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            });
        }
        ctx.arena.add_child(id, trigger_id);
        ctx.register_theme_component(
            trigger_id,
            &ResolvedComponentStyle::Select(resolved.clone()),
            &role,
            &self.style,
        );
        // Override theme corner radius to match DatePicker design.
        if let Some(el) = ctx.arena.get_mut(trigger_id) {
            el.set_corner_radius(6.0);
        }

        // ── TextInput (borderless — border on parent HStack) ──
        let text_input = TextInput::new(input_text.clone())
            .placeholder(placeholder.clone())
            .suppress_nav_keys(suppress_nav.clone());
        let text_input_id = Box::new(text_input).mount_box(&mut ctx.child_with_events(trigger_id));
        {
            let Some(el) = ctx.arena.get_mut(text_input_id) else {
                return id;
            };
            el.set_flex_grow(1.0);
            el.set_flex_shrink(1.0);
            el.set_border_width(0.0);
            el.set_corner_radii(crate::style::CornerRadii {
                top_left: 6.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 6.0,
            });
            el.set_preferred_height(34.0);
            el.set_accessible_role(accesskit::Role::ComboBox);
            el.set_accessible_label("Date picker");
        }
        ctx.arena.add_child(trigger_id, text_input_id);

        // ── Calendar icon button ──
        let open = Signal::new(false);
        let cal_icon_btn = {
            let o = open.clone();
            Button::new(" \u{1F4C5} ").text_only().on_click(move || {
                let was = o.read();
                o.set(!was);
            })
        };
        let cal_icon_id = Box::new(cal_icon_btn).mount_box(&mut ctx.child_with_events(trigger_id));
        {
            let Some(el) = ctx.arena.get_mut(cal_icon_id) else {
                return id;
            };
            el.set_tab_index(None);
            el.set_focusable(true);
            el.set_flex_shrink(0.0);
            el.set_border_width(0.0);
            el.set_padding(Padding::ZERO);
            el.set_corner_radii(crate::style::CornerRadii {
                top_left: 0.0,
                top_right: 6.0,
                bottom_right: 6.0,
                bottom_left: 0.0,
            });
            el.set_preferred_height(0.0);
            el.set_preferred_width(Some(38.0));
            el.set_background(Color::TRANSPARENT);
            el.with_state_style(|ss| {
                ss.hovered.background = Some(Color::TRANSPARENT);
                ss.pressed.background = Some(Color::TRANSPARENT);
            });
        }
        ctx.arena.add_child(trigger_id, cal_icon_id);

        // ── Click on TextInput toggles dropdown ──
        if let Some(reg) = ctx.event_registry.as_deref_mut() {
            let oc = open.clone();
            EventHandler::new()
                .on_click_at(move |_pos| {
                    let was = oc.read();
                    oc.set(!was);
                })
                .register_all(reg, text_input_id);
        }

        // ── Signal: date_sig / range_sig → update input_text ──
        if is_range {
            let rs = range_sig.clone();
            let it = input_text.clone();
            let f = fmt.clone();
            crate::core::signal_bridge::subscribe_owned(id, &range_sig, move || {
                let text = rs.read().map_or("Select range...".to_string(), |dr| {
                    format!("{} ~ {}", dr.start.strftime(&f), dr.end.strftime(&f))
                });
                it.set(text);
            });
        } else {
            let ds = date_sig.clone();
            let it = input_text.clone();
            let f = fmt.clone();
            let plh = placeholder.clone();
            crate::core::signal_bridge::subscribe_owned(id, &date_sig, move || {
                let text = format_date(&ds.read(), &f, &plh);
                it.set(text);
            });
        }

        // ── DROPDOWN overlay container ──
        let dropdown_id = ctx.arena.allocate();
        ctx.preallocate(dropdown_id, components::LAYOUT | components::LIFECYCLE);

        let portal_h: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
        // Calendar intrinsic width: 268px grid + portal padding
        let portal_width_val = cal_width + tokens::S1 * 2.0;
        let rv: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let geo_cell: Rc<Cell<PopoverGeometry>> = Rc::new(Cell::new(PopoverGeometry {
            x: 0.0,
            y: 0.0,
            width: portal_width_val,
            height: 0.0,
            actual_position: crate::widgets::overlay::PopoverPosition::Bottom,
        }));

        let placement = PopoverPlacement {
            flip_axes: FlipAxes::VerticalOnly,
            viewport_margin: 8.0,
            min_width: Some(portal_width_val),
            max_width: Some(portal_width_val),
            ..Default::default()
        };

        {
            let Some(dropdown_el) = ctx.arena.get_mut(dropdown_id) else {
                return id;
            };
            dropdown_el.set_z_index(theme.z_index.dropdown);
            dropdown_el.z_index_floor = Some(theme.z_index.dropdown);
            dropdown_el.set_padding(Padding::all(tokens::S1));
            dropdown_el.set_flex_shrink(0.0);
            dropdown_el.set_background(resolved.dropdown_bg);
            dropdown_el.set_border_width(1.0);
            dropdown_el.set_border_color(resolved.dropdown_border);
            dropdown_el.set_corner_radius(8.0);
            dropdown_el.set_shadow(resolved.shadow);
            dropdown_el.set_layout_direction(LayoutDirection::Vertical);
            dropdown_el.set_reactive_visible(rv.clone());
            dropdown_el.insert_user_data(crate::platform::portal::PortalHeight(portal_h.clone()));
            dropdown_el.insert_user_data(geo_cell.clone());
            dropdown_el.insert_user_data(placement);
            dropdown_el.insert_user_data(trigger_id);
        }
        if let Some(lc) = ctx
            .arena
            .component_tables
            .borrow_mut()
            .lc
            .get_mut(&dropdown_id)
        {
            lc.component_role = Some(role.clone());
            lc.style_refinement = Some(self.style.clone());
        }
        crate::ecs::register_theme_element(dropdown_id);

        // ── Calendar grid inside dropdown ──
        let mut calendar_cfg = if is_range {
            Calendar::new(date_sig.clone()).with_range(rs_cell.clone(), re_cell.clone())
        } else {
            Calendar::new(date_sig.clone())
        };
        if let Some(lo) = min_d {
            calendar_cfg = calendar_cfg.min_date(lo);
        }
        if let Some(hi) = max_d {
            calendar_cfg = calendar_cfg.max_date(hi);
        }
        if let Some(ref names) = self.month_names {
            calendar_cfg = calendar_cfg.month_names(names.clone());
        }
        if let Some(ref names) = self.weekday_names {
            calendar_cfg = calendar_cfg.weekday_names(names.clone());
        }

        let on_select_cb: Rc<dyn Fn()> = {
            let o = open.clone();
            let oc = on_change.clone();
            let ocr = on_range_change.clone();
            let rs = rs_cell.clone();
            let re = re_cell.clone();
            let rg = range_sig.clone();
            let ds = date_sig.clone();
            let rm = is_range;
            Rc::new(move || {
                if rm {
                    // Range mode: don't close, update signal in real-time
                    if let (Some(start), Some(end)) = (rs.get(), re.get()) {
                        let dr = DateRange { start, end };
                        rg.set(Some(dr));
                        if let Some(ref cb) = ocr {
                            cb(Some(dr));
                        }
                    } else {
                        // Incomplete range (e.g. end toggled off) → signal None
                        rg.set(None);
                        if let Some(ref cb) = ocr {
                            cb(None);
                        }
                    }
                } else {
                    if o.read() {
                        o.set(false);
                    }
                    if let Some(ref cb) = oc {
                        cb(ds.read());
                    }
                }
            })
        };

        let on_select_for_trigger = on_select_cb.clone();

        let shared: CalendarShared = calendar_cfg.build_into(
            &mut ctx.child_with_events(dropdown_id),
            dropdown_id,
            Some(on_select_cb),
        );

        // ── Subscribe: typed text → sync calendar view ──
        {
            let it = input_text.clone();
            let f = fmt.clone();
            let cm = shared.current_month.clone();
            let fd = shared.focused_day.clone();
            let cont_id = shared.container_id;
            let range_mode = is_range;
            let rs_cell_sync = rs_cell.clone();
            let re_cell_sync = re_cell.clone();
            crate::core::signal_bridge::subscribe_owned(id, &input_text, move || {
                let text = it.read();
                if range_mode {
                    // Range format: "start ~ end"
                    if let Some((start_str, end_str)) = text.split_once(" ~ ") {
                        if let (Some(s), Some(e)) = (
                            try_parse_date(start_str.trim(), &f),
                            try_parse_date(end_str.trim(), &f),
                        ) {
                            cm.set((s.year(), s.month()));
                            fd.set(Some(s));
                            rs_cell_sync.set(Some(s));
                            re_cell_sync.set(Some(e));
                            crate::core::dirty_registry::register_dirty(
                                cont_id,
                                DirtyFlags::REPAINT,
                            );
                            crate::core::dirty_registry::bump_subtree_gen(cont_id);
                        }
                    }
                } else if let Some(d) = try_parse_date(&text, &f) {
                    cm.set((d.year(), d.month()));
                    fd.set(Some(d));
                    crate::core::dirty_registry::register_dirty(cont_id, DirtyFlags::REPAINT);
                    crate::core::dirty_registry::bump_subtree_gen(cont_id);
                }
            });
        }

        // Keyboard handler on trigger_id — in the main tree, so it always
        // receives actions regardless of overlay focus status.
        //
        // NOTE: do NOT autofocus the calendar container at mount — the dropdown
        // is closed, so focusing its (hidden) calendar would (a) make a closed
        // DatePicker the app's initial focus, breaking Tab/Shift+Tab order, and
        // (b) keep a cursor-blink schedule alive causing perpetual repaints.
        // Autofocus happens in the open-subscription below (`!was && is_open`).
        if let Some(reg) = ctx.event_registry.as_deref_mut() {
            let cm_t = shared.current_month.clone();
            let fd_t = shared.focused_day.clone();
            let Some(dirty_el) = ctx.arena.get_mut(shared.container_id) else {
                return id;
            };
            let dirty_t = dirty_el.dirty.clone();
            let cont_id = shared.container_id;
            let min_t = min_d;
            let max_t = max_d;
            let os_t = on_select_for_trigger.clone();
            let open_act = open.clone();
            let rs_act = if is_range {
                Some(rs_cell.clone())
            } else {
                None
            };
            let re_act = if is_range {
                Some(re_cell.clone())
            } else {
                None
            };

            EventHandler::new()
                .on_action(move |action| {
                    if !open_act.read() {
                        return ActionOutcome::Unhandled;
                    }
                    match action.kind {
                        ActionKind::MoveUp
                        | ActionKind::MoveDown
                        | ActionKind::MoveLeft
                        | ActionKind::MoveRight => {
                            let key = match action.kind {
                                ActionKind::MoveUp => Key::ArrowUp,
                                ActionKind::MoveDown => Key::ArrowDown,
                                ActionKind::MoveLeft => Key::ArrowLeft,
                                ActionKind::MoveRight => Key::ArrowRight,
                                _ => unreachable!(),
                            };
                            crate::widgets::display::handle_day_key(
                                &key,
                                &cm_t,
                                &fd_t,
                                &date_sig,
                                &dirty_t,
                                min_t,
                                max_t,
                                &Some(os_t.clone()),
                                rs_act.as_ref(),
                                re_act.as_ref(),
                            );
                            dirty_t.set(dirty_t.get() | DirtyFlags::REPAINT);
                            crate::core::dirty_registry::register_dirty(
                                cont_id,
                                DirtyFlags::REPAINT,
                            );
                            crate::core::dirty_registry::bump_subtree_gen(cont_id);
                            ActionOutcome::Consumed
                        }
                        ActionKind::Activate | ActionKind::NewLine => {
                            if let Some(d) = fd_t.get() {
                                if let (Some(rs), Some(re)) = (rs_act.as_ref(), re_act.as_ref()) {
                                    crate::widgets::display::apply_range_click(
                                        d, rs, re, &date_sig,
                                    );
                                    os_t();
                                } else {
                                    date_sig.set(Some(d));
                                    os_t();
                                }
                                dirty_t.set(dirty_t.get() | DirtyFlags::REPAINT);
                                crate::core::dirty_registry::register_dirty(
                                    cont_id,
                                    DirtyFlags::REPAINT,
                                );
                                crate::core::dirty_registry::bump_subtree_gen(cont_id);
                                #[allow(clippy::needless_return)]
                                return ActionOutcome::Consumed;
                            }
                            ActionOutcome::Unhandled
                        }
                        ActionKind::Cancel => {
                            open_act.set(false);
                            ActionOutcome::Consumed
                        }
                        _ => ActionOutcome::Unhandled,
                    }
                })
                .register_all(reg, trigger_id);
        }

        // ── Compute portal height for layout ──
        // dropdown padding (S1*2=16) + container padding (8*2=16) + header (36) + weekdays (24) + 6 rows × 36
        let cal_height = tokens::S1 * 2.0 + 16.0 + 36.0 + 24.0 + 6.0 * 36.0;

        // ── Register overlay (portal system) ──
        crate::widgets::shared::dropdown::register_dropdown_portal(id, dropdown_id, open.clone());

        // ── Subscribe: open → update visibility + suppress_nav + scope ──
        {
            let on_open: Rc<dyn Fn()> = {
                let sn = suppress_nav.clone();
                let it = input_text.clone();
                let plh = placeholder.clone();
                let cid = shared.container_id;
                Rc::new(move || {
                    sn.set(true);
                    if it.read() == plh {
                        it.set(String::new());
                    }
                    crate::core::dirty_registry::defer_action(move |_arena, _root, reg| {
                        reg.request_autofocus(cid);
                    });
                })
            };
            let on_close: Rc<dyn Fn()> = {
                let sn = suppress_nav.clone();
                Rc::new(move || {
                    sn.set(false);
                })
            };
            crate::widgets::shared::dropdown::register_overlay_lifecycle(
                open.clone(),
                dropdown_id,
                rv.clone(),
                portal_h.clone(),
                cal_height,
                Some(on_open),
                Some(on_close),
            );
        }

        // ── ESC closes overlay ──
        if let Some(reg) = ctx.event_registry.as_deref_mut() {
            let vis_cancel = open.clone();
            EventHandler::new()
                .on_action(move |action| {
                    if action.kind == crate::event::action::ActionKind::Cancel {
                        vis_cancel.set(false);
                        crate::event::action::ActionOutcome::Consumed
                    } else {
                        crate::event::action::ActionOutcome::Unhandled
                    }
                })
                .register_all(reg, dropdown_id);
        }

        // ── Unmount guard ──
        crate::widgets::shared::dropdown::register_dropdown_unmount(dropdown_id);

        if let Some(lc) = ctx.arena.component_tables.borrow_mut().lc.get_mut(&id) {
            lc.component_role = Some(role.clone());
            lc.style_refinement = Some(self.style.clone());
        }
        crate::ecs::register_theme_element(id);

        id
    }
}

impl std::fmt::Debug for DatePicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatePicker").finish_non_exhaustive()
    }
}
