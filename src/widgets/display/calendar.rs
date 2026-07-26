#[cfg(feature = "ext-jiff")]
use jiff::{civil::Date, Zoned};

// The whole widget body is gated on ext-jiff; the imports must be too,
// or `cargo fix` without the feature strips them and breaks the
// feature-enabled build (this happened in the zero-warning sweep c5caef6).
#[cfg(feature = "ext-jiff")]
use crate::core::config::EventHandler;
#[cfg(feature = "ext-jiff")]
use crate::core::context::MountContext;
#[cfg(feature = "ext-jiff")]
use crate::core::dirty_registry;
#[cfg(feature = "ext-jiff")]
use crate::core::element::{DirtyFlags, ElementId};
#[cfg(feature = "ext-jiff")]
use crate::core::LayoutDirection;
#[cfg(feature = "ext-jiff")]
use crate::event::action::{ActionKind, ActionOutcome};
#[cfg(feature = "ext-jiff")]
use crate::event::types::Key;
#[cfg(feature = "ext-jiff")]
use crate::render::wgpu::glyphon_bridge::create_buffer;
#[cfg(feature = "ext-jiff")]
use crate::style::{Color, TextAlign};
#[cfg(feature = "ext-jiff")]
use auralis_signal::Signal;
#[cfg(feature = "ext-jiff")]
use std::cell::{Cell, RefCell};
#[cfg(feature = "ext-jiff")]
use std::rc::Rc;
/// English month abbreviations (default; override via `Calendar::month_names()`).
#[cfg(feature = "ext-jiff")]
pub const EN_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// English weekday abbreviations (default; override via `Calendar::weekday_names()`).
#[cfg(feature = "ext-jiff")]
pub const EN_WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

#[cfg(feature = "ext-jiff")]
#[derive(Clone, Copy, PartialEq, Eq)]
/// The current view mode of a calendar.
pub(crate) enum CalendarMode {
    Day,
    Month,
    Year,
}

#[cfg(feature = "ext-jiff")]
/// Shared range selection state machine.
/// Applies the closest-edge rule: inside [a,b] replaces the nearer edge (tie → start),
/// outside extends that side, clicking end toggles it off.
pub(crate) fn apply_range_click(
    d: Date,
    rs: &Rc<Cell<Option<Date>>>,
    re: &Rc<Cell<Option<Date>>>,
    sel: &Signal<Option<Date>>,
) {
    let s = rs.get();
    let e = re.get();
    if s.is_none() {
        rs.set(Some(d));
        re.set(None);
    } else if e.is_none() {
        if d >= s.unwrap() {
            re.set(Some(d));
            sel.set(Some(d));
        } else {
            rs.set(Some(d));
            re.set(None);
        }
    } else {
        let a = s.unwrap();
        let b = e.unwrap();
        if d == b {
            re.set(None);
            sel.set(None);
        } else if d > b {
            re.set(Some(d));
            sel.set(Some(d));
        } else if d < a {
            rs.set(Some(d));
        } else {
            if (d - a).get_days() <= (b - d).get_days() {
                rs.set(Some(d));
            } else {
                re.set(Some(d));
                sel.set(Some(d));
            }
        }
    }
}

#[cfg(feature = "ext-jiff")]
/// Single entry-point for marking a calendar cell as needing repaint.
/// Replaces the previous 7-call redundant stamp (cell_dirty.set + register_dirty
/// + mark_dirty + bump_surface_gen_remote + bump_subtree_gen + cell_gen.set).
/// Only the 3 essential ops are needed:
///   1) register_dirty → intent queue for process_dirty_phase
///   2) bump_surface_gen_remote → surface cache invalidation
///   3) bump_subtree_gen → subtree cache invalidation (also bumps subtree_generation)
#[inline]
fn mark_cell_repaint(cell_id: ElementId) {
    crate::core::dirty_registry::register_dirty(cell_id, DirtyFlags::REPAINT);
    crate::core::dirty_registry::bump_surface_gen_remote(cell_id);
    crate::core::dirty_registry::bump_subtree_gen(cell_id);
}

#[cfg(feature = "ext-jiff")]
/// A date grid with month and year selection modes.
pub struct Calendar {
    #[cfg(feature = "ext-jiff")]
    selected: Signal<Option<Date>>,
    #[cfg(feature = "ext-jiff")]
    min_date: Option<Date>,
    #[cfg(feature = "ext-jiff")]
    max_date: Option<Date>,
    #[cfg(feature = "ext-jiff")]
    first_day_of_week: i8,
    #[cfg(feature = "ext-jiff")]
    show_today_highlight: bool,
    #[cfg(feature = "ext-jiff")]
    range_mode: bool,
    #[cfg(feature = "ext-jiff")]
    range_start_cell: Option<Rc<Cell<Option<Date>>>>,
    #[cfg(feature = "ext-jiff")]
    range_end_cell: Option<Rc<Cell<Option<Date>>>>,
    #[cfg(feature = "ext-jiff")]
    month_names: Option<Vec<String>>,
    #[cfg(feature = "ext-jiff")]
    weekday_names: Option<Vec<String>>,
}

#[cfg(feature = "ext-jiff")]
impl Calendar {
    #[cfg(feature = "ext-jiff")]
    pub fn new(selected: Signal<Option<Date>>) -> Self {
        Self {
            selected,
            min_date: None,
            max_date: None,
            first_day_of_week: 0,
            show_today_highlight: true,
            range_mode: false,
            range_start_cell: None,
            range_end_cell: None,
            month_names: None,
            weekday_names: None,
        }
    }

    #[cfg(feature = "ext-jiff")]
    pub fn with_range(mut self, rs: Rc<Cell<Option<Date>>>, re: Rc<Cell<Option<Date>>>) -> Self {
        self.range_mode = true;
        self.range_start_cell = Some(rs);
        self.range_end_cell = Some(re);
        self
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
    pub fn first_day_of_week(mut self, dow: i8) -> Self {
        self.first_day_of_week = dow;
        self
    }
    #[cfg(feature = "ext-jiff")]
    pub fn show_today_highlight(mut self, v: bool) -> Self {
        self.show_today_highlight = v;
        self
    }
    /// Override month abbreviations (12 entries, e.g. from i18n).
    pub fn month_names(mut self, names: Vec<String>) -> Self {
        self.month_names = Some(names);
        self
    }
    /// Override weekday abbreviations (7 entries, starting from Monday).
    pub fn weekday_names(mut self, names: Vec<String>) -> Self {
        self.weekday_names = Some(names);
        self
    }

    #[cfg(feature = "ext-jiff")]
    pub(crate) fn build_into(
        &self,
        ctx: &mut MountContext<'_>,
        parent_id: ElementId,
        on_select: Option<Rc<dyn Fn()>>,
    ) -> CalendarShared {
        let theme = ctx.theme;
        let cell_w: f32 = 36.0;
        let cell_h: f32 = 36.0;
        let cols: usize = 7;

        let today = Zoned::now().date();
        let initial_date = self.selected.read().unwrap_or(today);
        let mode: Rc<Cell<CalendarMode>> = Rc::new(Cell::new(CalendarMode::Day));
        let current_month: Rc<Cell<(i16, i8)>> =
            Rc::new(Cell::new((initial_date.year(), initial_date.month())));
        let focused_day: Rc<Cell<Option<Date>>> = Rc::new(Cell::new(Some(initial_date)));
        let range_start: Rc<Cell<Option<Date>>> = self
            .range_start_cell
            .clone()
            .unwrap_or_else(|| Rc::new(Cell::new(None)));
        let range_end: Rc<Cell<Option<Date>>> = self
            .range_end_cell
            .clone()
            .unwrap_or_else(|| Rc::new(Cell::new(None)));
        let is_range_mode = self.range_mode;
        // Suppresses Enter/Space selection after title switches views via keyboard
        let suppress_select: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        let container_id = ctx.arena.allocate();
        if let Some(root) = ctx.arena.get_mut(container_id) {
            root.set_layout_direction(LayoutDirection::Vertical);
            root.set_preferred_width(Some(cell_w * cols as f32 + 16.0));
            root.set_padding(crate::style::Padding::all(8.0));
            root.set_focusable(true);
            root.set_accessible_label("Calendar");
        }
        ctx.arena.add_child(parent_id, container_id);

        let (header_id, title_id) = build_header(
            ctx,
            container_id,
            &current_month,
            &mode,
            &focused_day,
            suppress_select.clone(),
            theme,
            cell_w,
        );
        ctx.arena.add_child(container_id, header_id);

        let Some(el) = ctx.arena.get_mut(title_id) else {
            unreachable!()
        };
        let title_dirty = el.dirty.clone();
        let title_surf_gen = ctx.arena.get(title_id).unwrap().surface_gen.clone();

        let weekday_id = build_weekday_headers(
            ctx,
            container_id,
            self.first_day_of_week,
            theme,
            cols,
            cell_w,
            self.weekday_names.as_deref(),
        );
        ctx.arena.add_child(container_id, weekday_id);

        let day_grid_id = ctx.arena.allocate();
        if let Some(dg) = ctx.arena.get_mut(day_grid_id) {
            dg.set_layout_direction(LayoutDirection::Vertical);
            dg.set_preferred_width(Some(cell_w * cols as f32));
        }

        let day_cells: Vec<Rc<Cell<Option<i8>>>> =
            (0..42).map(|_| Rc::new(Cell::new(None))).collect();

        let selected = self.selected.clone();
        for week in 0..6 {
            let row_id = ctx.arena.allocate();
            if let Some(row) = ctx.arena.get_mut(row_id) {
                row.set_layout_direction(LayoutDirection::Horizontal);
                row.set_preferred_height(cell_h);
                row.set_preferred_width(Some(cell_w * cols as f32));
            }
            for wday in 0..cols {
                let idx = week * cols + wday;
                let cell_id = ctx.arena.allocate();
                let dc = day_cells[idx].clone();
                let dc2 = dc.clone();
                let cm = current_month.clone();
                let sel = selected.clone();
                let fd = focused_day.clone();
                let rs = range_start.clone();
                let re = range_end.clone();
                let td = today;
                let min_d = self.min_date;
                let max_d = self.max_date;
                let default_fg = theme.scheme.on_surface;

                if let Some(cel) = ctx.arena.get_mut(cell_id) {
                    cel.set_preferred_width(Some(cell_w));
                    cel.set_preferred_height(cell_h);
                    cel.set_corner_radius(4.0);
                    cel.set_font_size(14.0);

                    let prev_bg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));
                    let prev_fg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));
                    let prev_ym: Rc<Cell<(i16, i8)>> = Rc::new(Cell::new((0, 0)));
                    let _cell_dirty = cel.dirty.clone();
                    let _cell_gen = cel.subtree_generation.clone();
                    let buf = Rc::new(RefCell::new(create_buffer(
                        "",
                        14.0,
                        1.4,
                        400,
                        None,
                        Some(cell_w),
                        TextAlign::Center,
                    )));
                    cel.set_text_buffer(buf.clone());
                    let tg: Rc<Cell<u64>> = Rc::new(Cell::new(1));
                    cel.set_text_generation(tg.clone());

                    let today_bg = theme.scheme.primary;
                    let today_fg = theme.scheme.on_primary;
                    let focus_bg = theme.scheme.primary_container;
                    let focus_fg = theme.scheme.on_primary_container;
                    let range_bg = theme.scheme.primary_container.with_alpha(0.35);
                    let dis_fg = theme.scheme.disabled.foreground;

                    cel.set_frame_tick(Box::new(move || {
                        let (y, m) = cm.get();
                        // Change guard (audit 2026-07-15, C1a): text shaping
                        // depends only on (year, month) — skip the per-frame
                        // create_buffer when the month is unchanged. Colour
                        // state below stays per-frame but is diffed (prev_bg).
                        let ym_changed = prev_ym.get() != (y, m);
                        if ym_changed {
                            prev_ym.set((y, m));
                        }
                        let start_of_month = Date::new(y, m, 1).unwrap();
                        let start_wd = start_of_month.weekday().to_monday_zero_offset();
                        let cell_offset = idx as i32 - start_wd as i32;
                        let days_in_month_val = days_in_month(y, m) as i32;

                        if cell_offset >= 0 && cell_offset < days_in_month_val {
                            let day = (cell_offset + 1) as i8;
                            if let Ok(date) = Date::new(y, m, day) {
                                let in_range = min_d.map_or(true, |lo| date >= lo)
                                    && max_d.map_or(true, |hi| date <= hi);
                                if ym_changed {
                                    dc2.set(Some(day));
                                    let text = day.to_string();
                                    *buf.borrow_mut() = create_buffer(
                                        &text,
                                        14.0,
                                        1.4,
                                        400,
                                        None,
                                        Some(cell_w),
                                        TextAlign::Center,
                                    );
                                    mark_cell_repaint(cell_id);
                                }

                                let _is_today = date == td;
                                let is_selected = sel.read_untracked() == Some(date);
                                let is_focused = fd.get() == Some(date);
                                let rs_val = rs.get();
                                let re_val = re.get();
                                let is_range_start = rs_val == Some(date);
                                let is_range_end = re_val == Some(date);
                                let is_in_range = match (rs_val, re_val) {
                                    (Some(s), Some(e)) => date > s && date < e,
                                    _ => false,
                                };

                                let new_bg: Option<Color>;
                                let new_fg: Option<Color>;
                                if is_range_start || is_range_end || is_selected {
                                    new_bg = Some(today_bg);
                                    new_fg = Some(today_fg);
                                } else if is_focused {
                                    new_bg = Some(focus_bg);
                                    new_fg = Some(focus_fg);
                                } else if is_in_range {
                                    new_bg = Some(range_bg);
                                    new_fg = Some(default_fg);
                                } else if !in_range {
                                    new_bg = None;
                                    new_fg = Some(dis_fg);
                                } else {
                                    new_bg = None;
                                    new_fg = Some(default_fg);
                                }
                                if new_bg != prev_bg.get() || new_fg != prev_fg.get() {
                                    prev_bg.set(new_bg);
                                    prev_fg.set(new_fg);
                                    crate::core::dirty_registry::defer_action({
                                        let cid = cell_id;
                                        let bg = new_bg;
                                        let fg = new_fg;
                                        move |arena, _, _| {
                                            let mut ct = arena.component_tables.borrow_mut();
                                            ct.style.entry(cid).or_default().background = bg;
                                            ct.style.entry(cid).or_default().foreground = fg;
                                        }
                                    });
                                    tg.set(tg.get().wrapping_add(1));
                                    mark_cell_repaint(cell_id);
                                }
                            }
                        } else {
                            if ym_changed {
                                dc2.set(None);
                                *buf.borrow_mut() = create_buffer(
                                    "",
                                    14.0,
                                    1.4,
                                    400,
                                    None,
                                    Some(cell_w),
                                    TextAlign::Center,
                                );
                                mark_cell_repaint(cell_id);
                            }
                            if prev_bg.get().is_some() || prev_fg.get().is_some() {
                                prev_bg.set(None);
                                prev_fg.set(None);
                                crate::core::dirty_registry::defer_action({
                                    let cid = cell_id;
                                    move |arena, _, _| {
                                        let mut ct = arena.component_tables.borrow_mut();
                                        ct.style.entry(cid).or_default().background = None;
                                        ct.style.entry(cid).or_default().foreground = None;
                                    }
                                });
                                tg.set(tg.get().wrapping_add(1));
                                mark_cell_repaint(cell_id);
                            }
                        }
                    }));
                }

                {
                    let sel_c = selected.clone();
                    let cm_c = current_month.clone();
                    let os = on_select.clone();
                    let rs_click = range_start.clone();
                    let re_click = range_end.clone();
                    let rm = is_range_mode;
                    let cid = container_id;
                    let fd_click = focused_day.clone();
                    let Some(el) = ctx.arena.get_mut(container_id) else {
                        unreachable!()
                    };
                    let cont_dirty = el.dirty.clone();
                    let cell_events = EventHandler::new().on_click(move || {
                        if let Some(day) = dc.get() {
                            if day > 0 {
                                let (y, m) = cm_c.get();
                                if let Ok(d) = Date::new(y, m, day) {
                                    fd_click.set(Some(d));
                                    if rm {
                                        apply_range_click(d, &rs_click, &re_click, &sel_c);
                                        cont_dirty.set(cont_dirty.get() | DirtyFlags::REPAINT);
                                        dirty_registry::register_dirty(cid, DirtyFlags::REPAINT);
                                        dirty_registry::bump_subtree_gen(cid);
                                        if let Some(ref cb) = os {
                                            cb();
                                        }
                                    } else {
                                        sel_c.set(Some(d));
                                        if let Some(ref cb) = os {
                                            cb();
                                        }
                                    }
                                }
                            }
                        }
                    });
                    if let Some(reg) = ctx.event_registry.as_mut() {
                        cell_events.register_all(reg, cell_id);
                    }
                }

                ctx.arena.add_child(row_id, cell_id);
            }
            ctx.arena.add_child(day_grid_id, row_id);
        }
        ctx.arena.add_child(container_id, day_grid_id);

        let month_grid_id = build_month_grid(
            ctx,
            container_id,
            &current_month,
            &mode,
            &selected,
            self.min_date,
            self.max_date,
            theme,
            cell_w,
            cell_h,
            self.month_names.as_deref(),
        );
        ctx.arena.add_child(container_id, month_grid_id);

        let year_grid_id = build_year_grid(
            ctx,
            container_id,
            &current_month,
            &mode,
            &selected,
            self.min_date,
            self.max_date,
            theme,
            cell_w,
            cell_h,
        );
        ctx.arena.add_child(container_id, year_grid_id);

        {
            let m = mode.clone();
            let Some(el) = ctx.arena.get_mut(day_grid_id) else {
                unreachable!()
            };
            let day_el = el.slot_inactive.clone();
            let Some(el) = ctx.arena.get_mut(month_grid_id) else {
                unreachable!()
            };
            let mon_el = el.slot_inactive.clone();
            let Some(el) = ctx.arena.get_mut(year_grid_id) else {
                unreachable!()
            };
            let yr_el = el.slot_inactive.clone();
            let Some(el) = ctx.arena.get_mut(weekday_id) else {
                unreachable!()
            };
            let wk_el = el.slot_inactive.clone();
            let Some(el) = ctx.arena.get_mut(day_grid_id) else {
                unreachable!()
            };
            el.slot_inactive.set(false);
            let Some(el) = ctx.arena.get_mut(month_grid_id) else {
                unreachable!()
            };
            el.slot_inactive.set(true);
            let Some(el) = ctx.arena.get_mut(year_grid_id) else {
                unreachable!()
            };
            el.slot_inactive.set(true);
            let Some(el) = ctx.arena.get_mut(weekday_id) else {
                unreachable!()
            };
            el.slot_inactive.set(false);
            let Some(el) = ctx.arena.get_mut(container_id) else {
                unreachable!()
            };
            let container_dirty = el.dirty.clone();
            let cont_id = container_id;
            let fd_reset = focused_day.clone();
            let cm_fd = current_month.clone();
            let min_fd = self.min_date;
            let max_fd = self.max_date;
            let Some(el) = ctx.arena.get_mut(container_id) else {
                unreachable!()
            };
            el.set_frame_tick(Box::new(move || {
                let cur = m.get();
                let new_day = cur == CalendarMode::Day;
                let new_mon = cur == CalendarMode::Month;
                let new_yr = cur == CalendarMode::Year;
                let mut structural = false;
                if wk_el.get() != !new_day {
                    wk_el.set(!new_day);
                    structural = true;
                }
                if day_el.get() != !new_day {
                    day_el.set(!new_day);
                    if new_day {
                        let (y, m) = cm_fd.get();
                        let mut d = Date::new(y, m, 1).unwrap_or(today);
                        if let Some(lo) = min_fd {
                            if d < lo {
                                d = lo;
                            }
                        }
                        if let Some(hi) = max_fd {
                            if d > hi {
                                d = hi;
                            }
                        }
                        fd_reset.set(Some(d));
                    }
                    structural = true;
                }
                if mon_el.get() != !new_mon {
                    mon_el.set(!new_mon);
                    structural = true;
                }
                if yr_el.get() != !new_yr {
                    yr_el.set(!new_yr);
                    structural = true;
                }
                if structural {
                    dirty_registry::mark_structurally_changed(cont_id);
                    container_dirty.set(container_dirty.get() | DirtyFlags::REPAINT);
                    dirty_registry::register_dirty(cont_id, DirtyFlags::REPAINT);
                    dirty_registry::bump_subtree_gen(cont_id);
                }
            }));
        }

        {
            let cm_key = current_month.clone();
            let fd_key = focused_day.clone();
            let mode_key = mode.clone();
            let sel_key = selected.clone();
            let min_d = self.min_date;
            let max_d = self.max_date;
            let Some(el) = ctx.arena.get_mut(container_id) else {
                unreachable!()
            };
            let root_dirty = el.dirty.clone();
            let on_select_key = on_select.clone();
            let rs_key = if is_range_mode {
                Some(range_start.clone())
            } else {
                None
            };
            let re_key = if is_range_mode {
                Some(range_end.clone())
            } else {
                None
            };

            let _cm_act = current_month.clone();
            let mode_act = mode.clone();
            let Some(el) = ctx.arena.get_mut(container_id) else {
                unreachable!()
            };
            let dirty_act = el.dirty.clone();

            let update_header_style = {
                let fd_style = focused_day.clone();
                let td = title_dirty.clone();
                let tsg = title_surf_gen.clone();
                move || {
                    let focused = fd_style.get().is_none();
                    crate::core::element::with_ct_mut(|ct| {
                        let s = ct.style.entry(title_id).or_default();
                        if focused {
                            s.background = Some(crate::style::Color::rgba8(59, 130, 246, 40));
                            s.border_width = 1.0;
                            s.border_color = Some(crate::style::Color::rgba8(59, 130, 246, 255));
                        } else {
                            s.background = None;
                            s.border_width = 0.0;
                            s.border_color = None;
                        }
                    });
                    td.set(td.get() | crate::core::element::DirtyFlags::REPAINT);
                    tsg.set(tsg.get().wrapping_add(1));
                    crate::core::dirty_registry::bump_subtree_gen(title_id);
                }
            };

            let container_events = EventHandler::new()
                .on_key_down(move |key, _mods| -> bool {
                    let m = mode_key.get();
                    if key == Key::Enter || key == Key::Space {
                        if suppress_select.get() {
                            suppress_select.set(false);
                            return false;
                        }
                    } else {
                        suppress_select.set(false);
                    }
                    match m {
                        CalendarMode::Day => {
                            // ── Header-focus navigation ──
                            if fd_key.get().is_none() {
                                let handled = match key {
                                    Key::ArrowUp | Key::ArrowLeft => {
                                        let (y, mo) = cm_key.get();
                                        cm_key.set(if mo == 1 { (y - 1, 12) } else { (y, mo - 1) });
                                        true
                                    }
                                    Key::ArrowDown => {
                                        let (y, mo) = cm_key.get();
                                        fd_key.set(Some(jiff::civil::Date::new(y, mo, 1).unwrap()));
                                        true
                                    }
                                    Key::ArrowRight => {
                                        let (y, mo) = cm_key.get();
                                        cm_key.set(if mo == 12 { (y + 1, 1) } else { (y, mo + 1) });
                                        true
                                    }
                                    Key::Enter | Key::Space => {
                                        let cur = mode_key.get();
                                        let next = match cur {
                                            CalendarMode::Day => CalendarMode::Month,
                                            CalendarMode::Month => CalendarMode::Year,
                                            CalendarMode::Year => CalendarMode::Day,
                                        };
                                        mode_key.set(next);
                                        suppress_select.set(true);
                                        true
                                    }
                                    Key::Escape => {
                                        fd_key.set(Some(
                                            jiff::civil::Date::new(
                                                cm_key.get().0,
                                                cm_key.get().1,
                                                1,
                                            )
                                            .unwrap(),
                                        ));
                                        true
                                    }
                                    _ => false,
                                };
                                if handled {
                                    root_dirty.set(root_dirty.get() | DirtyFlags::REPAINT);
                                    dirty_registry::register_dirty(
                                        container_id,
                                        DirtyFlags::REPAINT,
                                    );
                                    dirty_registry::bump_subtree_gen(container_id);
                                }
                                update_header_style();
                                return handled;
                            }
                            // ── Up from first row → move focus to header ──
                            if key == Key::ArrowUp {
                                if let Some(current) = fd_key.get() {
                                    if let Ok(up) = current.checked_add(jiff::Span::new().days(-7))
                                    {
                                        if up.month() != current.month()
                                            || up.year() != current.year()
                                        {
                                            fd_key.set(None);
                                            root_dirty.set(root_dirty.get() | DirtyFlags::REPAINT);
                                            dirty_registry::register_dirty(
                                                container_id,
                                                DirtyFlags::REPAINT,
                                            );
                                            dirty_registry::bump_subtree_gen(container_id);
                                            update_header_style();
                                            return true;
                                        }
                                    }
                                }
                            }
                            let handled = handle_day_key(
                                &key,
                                &cm_key,
                                &fd_key,
                                &sel_key,
                                &root_dirty,
                                min_d,
                                max_d,
                                &on_select_key,
                                rs_key.as_ref(),
                                re_key.as_ref(),
                            );
                            if handled {
                                root_dirty.set(root_dirty.get() | DirtyFlags::REPAINT);
                                dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);
                                dirty_registry::bump_subtree_gen(container_id);
                            }
                            update_header_style();
                            handled
                        }
                        CalendarMode::Month => {
                            if key == Key::Enter || key == Key::Space {
                                mode_key.set(CalendarMode::Year);
                                return true;
                            }
                            let handled = handle_month_key(
                                &key,
                                &cm_key,
                                &mode_key,
                                &root_dirty,
                                min_d,
                                max_d,
                            );
                            if handled {
                                root_dirty.set(root_dirty.get() | DirtyFlags::REPAINT);
                                dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);
                                dirty_registry::bump_subtree_gen(container_id);
                            }
                            handled
                        }
                        CalendarMode::Year => {
                            if key == Key::Enter || key == Key::Space {
                                mode_key.set(CalendarMode::Day);
                                return true;
                            }
                            let handled = handle_year_key(
                                &key,
                                &cm_key,
                                &mode_key,
                                &root_dirty,
                                min_d,
                                max_d,
                            );
                            if handled {
                                root_dirty.set(root_dirty.get() | DirtyFlags::REPAINT);
                                dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);
                                dirty_registry::bump_subtree_gen(container_id);
                            }
                            handled
                        }
                    }
                })
                .on_action(move |action| match action.kind {
                    ActionKind::Cancel => {
                        if mode_act.get() != CalendarMode::Day {
                            mode_act.set(CalendarMode::Day);
                            dirty_act.set(dirty_act.get() | DirtyFlags::REPAINT);
                            ActionOutcome::Consumed
                        } else {
                            ActionOutcome::Unhandled
                        }
                    }
                    _ => ActionOutcome::Unhandled,
                });
            if let Some(reg) = ctx.event_registry.as_mut() {
                container_events.register_all(reg, container_id);
            }
        }

        crate::ecs::register_theme_element(container_id);

        CalendarShared {
            container_id,
            current_month: current_month.clone(),
            mode: mode.clone(),
            focused_day: focused_day.clone(),
            day_grid_id: day_grid_id,
            month_grid_id: month_grid_id,
            year_grid_id: year_grid_id,
            range_start: range_start.clone(),
            range_end: range_end.clone(),
        }
    }
}

#[cfg(feature = "ext-jiff")]
pub(crate) struct CalendarShared {
    #[cfg(feature = "ext-jiff")]
    pub(crate) container_id: ElementId,
    #[cfg(feature = "ext-jiff")]
    pub(crate) current_month: Rc<Cell<(i16, i8)>>,
    // The remaining handles are written at mount and kept for the DatePicker
    // keyboard-navigation follow-up (grid focus + range editing); not read yet.
    #[cfg(feature = "ext-jiff")]
    #[allow(dead_code)]
    pub(crate) mode: Rc<Cell<CalendarMode>>,
    #[cfg(feature = "ext-jiff")]
    pub(crate) focused_day: Rc<Cell<Option<jiff::civil::Date>>>,
    #[cfg(feature = "ext-jiff")]
    #[allow(dead_code)]
    pub(crate) day_grid_id: ElementId,
    #[cfg(feature = "ext-jiff")]
    #[allow(dead_code)]
    pub(crate) month_grid_id: ElementId,
    #[cfg(feature = "ext-jiff")]
    #[allow(dead_code)]
    pub(crate) year_grid_id: ElementId,
    #[cfg(feature = "ext-jiff")]
    #[allow(dead_code)]
    pub(crate) range_start: Rc<Cell<Option<jiff::civil::Date>>>,
    #[cfg(feature = "ext-jiff")]
    #[allow(dead_code)]
    pub(crate) range_end: Rc<Cell<Option<jiff::civil::Date>>>,
}

#[cfg(feature = "ext-jiff")]
fn build_header(
    ctx: &mut MountContext<'_>,
    parent_id: ElementId,
    current_month: &Rc<Cell<(i16, i8)>>,
    mode: &Rc<Cell<CalendarMode>>,
    _focused_day: &Rc<Cell<Option<Date>>>,
    _suppress_select: Rc<Cell<bool>>,
    _theme: &crate::theme::M3Theme,
    cell_w: f32,
) -> (ElementId, ElementId) {
    let header_id = ctx.arena.allocate();
    if let Some(h) = ctx.arena.get_mut(header_id) {
        h.set_layout_direction(LayoutDirection::Horizontal);
        h.set_preferred_height(36.0);
        h.set_preferred_width(Some(cell_w * 7.0));
    }

    let prev_id = ctx.arena.allocate();
    if let Some(p) = ctx.arena.get_mut(prev_id) {
        p.set_preferred_width(Some(32.0));
        p.set_preferred_height(32.0);
        p.set_font_size(16.0);
        p.set_accessible_label("Previous");
        let buf = Rc::new(RefCell::new(create_buffer(
            "\u{25C0}",
            16.0,
            1.5,
            400,
            None,
            None,
            TextAlign::Center,
        )));
        p.set_text_buffer(buf);
    }
    {
        let cm = current_month.clone();
        let mid = header_id;
        let prev_events = EventHandler::new().on_click(move || {
            let (y, m) = cm.get();
            if m == 1 {
                cm.set((y - 1, 12));
            } else {
                cm.set((y, m - 1));
            }
            crate::core::dirty_registry::register_dirty(mid, DirtyFlags::REPAINT);
        });
        if let Some(reg) = ctx.event_registry.as_mut() {
            prev_events.register_all(reg, prev_id);
        }
    }
    ctx.arena.add_child(header_id, prev_id);

    let title_id = ctx.arena.allocate();
    if let Some(t) = ctx.arena.get_mut(title_id) {
        t.set_preferred_width(Some(cell_w * 7.0 - 64.0));
        t.set_font_size(14.0);
        t.set_font_weight(600);
        t.set_accessible_label("Current month and year — press Enter to change view");
        let month_names_full: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        let cm_t = current_month.clone();
        let buf = Rc::new(RefCell::new({
            let (y, m) = cm_t.get();
            create_buffer(
                &format!("{} {}", month_names_full[m as usize - 1], y),
                14.0,
                1.4,
                600,
                None,
                None,
                TextAlign::Center,
            )
        }));
        t.set_text_buffer(buf.clone());

        t.set_frame_tick(Box::new({
            let cm_t2 = current_month.clone();
            let b = buf.clone();
            let tid = title_id;
            // Change guard: only re-shape the title when (year, month) actually
            // changes — the tick runs every frame (audit 2026-07-15, C1a).
            let prev_ym: Cell<(i16, i8)> = Cell::new((0, 0));
            move || {
                let (y, m) = cm_t2.get();
                if prev_ym.get() == (y, m) {
                    return;
                }
                prev_ym.set((y, m));
                let text = format!("{} {}", month_names_full[m as usize - 1], y);
                *b.borrow_mut() =
                    create_buffer(&text, 14.0, 1.4, 600, None, None, TextAlign::Center);
                mark_cell_repaint(tid);
            }
        }));
    }
    {
        let m = mode.clone();
        let pid = parent_id;
        let title_events = EventHandler::new().on_click({
            let m = m.clone();
            let pid = pid;
            move || {
                let cur = m.get();
                let next = match cur {
                    CalendarMode::Day => CalendarMode::Month,
                    CalendarMode::Month => CalendarMode::Year,
                    CalendarMode::Year => CalendarMode::Day,
                };
                m.set(next);
                crate::core::dirty_registry::register_dirty(pid, DirtyFlags::REPAINT);
            }
        });
        if let Some(reg) = ctx.event_registry.as_mut() {
            title_events.register_all(reg, title_id);
        }
    }
    ctx.arena.add_child(header_id, title_id);

    let next_id = ctx.arena.allocate();
    if let Some(n) = ctx.arena.get_mut(next_id) {
        n.set_preferred_width(Some(32.0));
        n.set_preferred_height(32.0);
        n.set_font_size(16.0);
        n.set_accessible_label("Next");
        let buf = Rc::new(RefCell::new(create_buffer(
            "\u{25B6}",
            16.0,
            1.5,
            400,
            None,
            None,
            TextAlign::Center,
        )));
        n.set_text_buffer(buf);
    }
    {
        let cm = current_month.clone();
        let mid = header_id;
        let next_events = EventHandler::new().on_click(move || {
            let (y, m) = cm.get();
            if m == 12 {
                cm.set((y + 1, 1));
            } else {
                cm.set((y, m + 1));
            }
            crate::core::dirty_registry::register_dirty(mid, DirtyFlags::REPAINT);
        });
        if let Some(reg) = ctx.event_registry.as_mut() {
            next_events.register_all(reg, next_id);
        }
    }
    ctx.arena.add_child(header_id, next_id);

    (header_id, title_id)
}

#[cfg(feature = "ext-jiff")]
fn build_weekday_headers(
    ctx: &mut MountContext<'_>,
    _parent_id: ElementId,
    first_dow: i8,
    theme: &crate::theme::M3Theme,
    cols: usize,
    cell_w: f32,
    custom_days: Option<&[String]>,
) -> ElementId {
    let row_id = ctx.arena.allocate();
    if let Some(r) = ctx.arena.get_mut(row_id) {
        r.set_layout_direction(LayoutDirection::Horizontal);
        r.set_preferred_height(24.0);
        r.set_preferred_width(Some(cell_w * cols as f32));
    }

    let labels: Vec<&str> = custom_days
        .map(|d| d.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| EN_WEEKDAYS.to_vec());
    for i in 0..cols {
        let label = labels[(i as i8 + first_dow) as usize % 7];
        let cell_id = ctx.arena.allocate();
        if let Some(c) = ctx.arena.get_mut(cell_id) {
            c.set_preferred_width(Some(cell_w));
            c.set_font_size(12.0);
            c.set_font_weight(600);
            c.set_foreground(theme.scheme.on_surface_variant);
            let buf = Rc::new(RefCell::new(create_buffer(
                label,
                12.0,
                1.4,
                600,
                None,
                Some(cell_w),
                TextAlign::Center,
            )));
            c.set_text_buffer(buf);
        }
        ctx.arena.add_child(row_id, cell_id);
    }
    row_id
}

#[cfg(feature = "ext-jiff")]
fn build_month_grid(
    ctx: &mut MountContext<'_>,
    parent_id: ElementId,
    current_month: &Rc<Cell<(i16, i8)>>,
    mode: &Rc<Cell<CalendarMode>>,
    _selected: &Signal<Option<Date>>,
    min_date: Option<Date>,
    max_date: Option<Date>,
    theme: &crate::theme::M3Theme,
    cell_w: f32,
    _cell_h: f32,
    custom_months: Option<&[String]>,
) -> ElementId {
    let grid_id = ctx.arena.allocate();
    if let Some(g) = ctx.arena.get_mut(grid_id) {
        g.set_layout_direction(LayoutDirection::Vertical);
        g.set_preferred_width(Some(cell_w * 7.0));
    }

    let month_labels: Vec<&str> = custom_months
        .map(|m| m.iter().map(|s| s.as_str()).collect())
        .unwrap_or_else(|| EN_MONTHS.to_vec());
    let focus_bg = theme.scheme.primary_container;
    let focus_fg = theme.scheme.on_primary_container;
    let _prev_bg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));
    let _prev_fg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));

    for row in 0..4 {
        let row_id = ctx.arena.allocate();
        if let Some(r) = ctx.arena.get_mut(row_id) {
            r.set_layout_direction(LayoutDirection::Horizontal);
            r.set_preferred_height(44.0);
        }
        for col in 0..3 {
            let month_idx = row * 3 + col;
            let cell_id = ctx.arena.allocate();
            let cm2 = current_month.clone();
            let cm = current_month.clone();
            let m_cell = mode.clone();
            let pid = parent_id;

            if let Some(c) = ctx.arena.get_mut(cell_id) {
                c.set_preferred_width(Some(cell_w * 7.0 / 3.0));
                c.set_preferred_height(44.0);
                c.set_corner_radius(4.0);
                c.set_font_size(14.0);

                let prev_bg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));
                let prev_fg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));
                let _cell_dirty = c.dirty.clone();
                let buf = Rc::new(RefCell::new(create_buffer(
                    month_labels[month_idx],
                    14.0,
                    1.4,
                    400,
                    None,
                    Some(cell_w * 7.0 / 3.0),
                    TextAlign::Center,
                )));
                c.set_text_buffer(buf);
                let tg_m: Rc<Cell<u64>> = Rc::new(Cell::new(1));
                c.set_text_generation(tg_m.clone());

                c.set_frame_tick(Box::new(move || {
                    let (y, _m) = cm2.get();
                    let mnum = (month_idx + 1) as i8;
                    if Date::new(y, mnum, 1).is_ok() {
                        let is_focused = _m == mnum;
                        let new_bg: Option<Color>;
                        let new_fg: Option<Color>;
                        if is_focused {
                            new_bg = Some(focus_bg);
                            new_fg = Some(focus_fg);
                        } else {
                            new_bg = None;
                            new_fg = None;
                        }
                        if new_bg != prev_bg.get() || new_fg != prev_fg.get() {
                            prev_bg.set(new_bg);
                            prev_fg.set(new_fg);
                            crate::core::dirty_registry::defer_action({
                                let cid = cell_id;
                                let bg = new_bg;
                                let fg = new_fg;
                                move |arena, _, _| {
                                    let mut ct = arena.component_tables.borrow_mut();
                                    ct.style.entry(cid).or_default().background = bg;
                                    ct.style.entry(cid).or_default().foreground = fg;
                                }
                            });
                            tg_m.set(tg_m.get().wrapping_add(1));
                            mark_cell_repaint(cell_id);
                        }
                    }
                }));
            }

            {
                let min_click = min_date;
                let max_click = max_date;
                let month_events = EventHandler::new().on_click(move || {
                    let (y, _old_m) = cm.get();
                    let mnum = (month_idx + 1) as i8;
                    let in_range = min_click.map_or(true, |lo| {
                        y >= lo.year() || (y == lo.year() && mnum >= lo.month())
                    }) && max_click.map_or(true, |hi| {
                        y <= hi.year() || (y == hi.year() && mnum <= hi.month())
                    });
                    if in_range {
                        cm.set((y, mnum));
                        m_cell.set(CalendarMode::Day);
                        crate::core::dirty_registry::register_dirty(pid, DirtyFlags::REPAINT);
                    }
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    month_events.register_all(reg, cell_id);
                }
            }

            ctx.arena.add_child(row_id, cell_id);
        }
        ctx.arena.add_child(grid_id, row_id);
    }

    grid_id
}

#[cfg(feature = "ext-jiff")]
fn build_year_grid(
    ctx: &mut MountContext<'_>,
    parent_id: ElementId,
    current_month: &Rc<Cell<(i16, i8)>>,
    mode: &Rc<Cell<CalendarMode>>,
    _selected: &Signal<Option<Date>>,
    min_date: Option<Date>,
    max_date: Option<Date>,
    theme: &crate::theme::M3Theme,
    cell_w: f32,
    _cell_h: f32,
) -> ElementId {
    let grid_id = ctx.arena.allocate();
    let cols: usize = 3;
    let rows: usize = 7;
    let year_count = cols * rows;
    if let Some(g) = ctx.arena.get_mut(grid_id) {
        g.set_layout_direction(LayoutDirection::Vertical);
        g.set_preferred_width(Some(cell_w * 7.0));
    }

    let focus_bg = theme.scheme.primary_container;
    let focus_fg = theme.scheme.on_primary_container;
    let dis_fg = theme.scheme.disabled.foreground;

    for row in 0..rows {
        let row_id = ctx.arena.allocate();
        if let Some(r) = ctx.arena.get_mut(row_id) {
            r.set_layout_direction(LayoutDirection::Horizontal);
            r.set_preferred_height(36.0);
        }
        for col in 0..cols {
            let idx = row * cols + col;
            let cell_id = ctx.arena.allocate();
            let cm2 = current_month.clone();
            let cm = current_month.clone();
            let m_cell = mode.clone();
            let pid = parent_id;

            if let Some(c) = ctx.arena.get_mut(cell_id) {
                c.set_preferred_width(Some(cell_w * 7.0 / cols as f32));
                c.set_preferred_height(36.0);
                c.set_corner_radius(4.0);
                c.set_font_size(14.0);

                let prev_bg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));
                let prev_fg: Rc<Cell<Option<Color>>> = Rc::new(Cell::new(None));
                let prev_year: Rc<Cell<i16>> = Rc::new(Cell::new(0));
                let _cell_dirty = c.dirty.clone();
                let _cell_gen = c.subtree_generation.clone();
                let tg = Rc::new(Cell::new(1u64));
                c.set_text_generation(tg.clone());
                let buf = Rc::new(RefCell::new(create_buffer(
                    "",
                    14.0,
                    1.4,
                    400,
                    None,
                    Some(cell_w * 7.0 / cols as f32),
                    TextAlign::Center,
                )));
                c.set_text_buffer(buf.clone());

                c.set_frame_tick(Box::new(move || {
                    let (y, _m) = cm2.get();
                    let base = y - (y % year_count as i16);
                    let year = base + idx as i16;
                    let in_range = min_date.map_or(true, |lo| year >= lo.year())
                        && max_date.map_or(true, |hi| year <= hi.year());

                    // Change guard (audit 2026-07-15, C1a): the label depends
                    // only on the computed `year` — skip re-shaping otherwise.
                    if prev_year.get() != year {
                        prev_year.set(year);
                        *buf.borrow_mut() = create_buffer(
                            &year.to_string(),
                            14.0,
                            1.4,
                            400,
                            None,
                            Some(cell_w * 7.0 / cols as f32),
                            TextAlign::Center,
                        );
                        mark_cell_repaint(cell_id);
                    }

                    let new_bg: Option<Color>;
                    let new_fg: Option<Color>;
                    if year == y {
                        new_bg = Some(focus_bg);
                        new_fg = Some(focus_fg);
                    } else if !in_range {
                        new_bg = None;
                        new_fg = Some(dis_fg);
                    } else {
                        new_bg = None;
                        new_fg = None;
                    }
                    if new_bg != prev_bg.get() || new_fg != prev_fg.get() {
                        prev_bg.set(new_bg);
                        prev_fg.set(new_fg);
                        crate::core::dirty_registry::defer_action({
                            let cid = cell_id;
                            let bg = new_bg;
                            let fg = new_fg;
                            move |arena, _, _| {
                                let mut ct = arena.component_tables.borrow_mut();
                                ct.style.entry(cid).or_default().background = bg;
                                ct.style.entry(cid).or_default().foreground = fg;
                            }
                        });
                        tg.set(tg.get().wrapping_add(1));
                        mark_cell_repaint(cell_id);
                    }
                }));
            }

            {
                let year_events = EventHandler::new().on_click(move || {
                    let (old_y, old_m) = cm.get();
                    let base = old_y - (old_y % year_count as i16);
                    let year = base + idx as i16;
                    let in_range = min_date.map_or(true, |lo| year >= lo.year())
                        && max_date.map_or(true, |hi| year <= hi.year());
                    if in_range {
                        cm.set((year, old_m));
                        m_cell.set(CalendarMode::Day);
                        crate::core::dirty_registry::register_dirty(pid, DirtyFlags::REPAINT);
                    }
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    year_events.register_all(reg, cell_id);
                }
            }

            ctx.arena.add_child(row_id, cell_id);
        }
        ctx.arena.add_child(grid_id, row_id);
    }

    grid_id
}

#[cfg(feature = "ext-jiff")]
pub(crate) fn handle_day_key(
    key: &Key,
    cm: &Rc<Cell<(i16, i8)>>,
    fd: &Rc<Cell<Option<Date>>>,
    sel: &Signal<Option<Date>>,
    _dirty: &Rc<Cell<DirtyFlags>>,
    min_date: Option<Date>,
    max_date: Option<Date>,
    on_select: &Option<Rc<dyn Fn()>>,
    range_start: Option<&Rc<Cell<Option<Date>>>>,
    range_end: Option<&Rc<Cell<Option<Date>>>>,
) -> bool {
    let (y, m) = cm.get();
    let current_focus = fd.get().unwrap_or_else(|| Date::new(y, m, 1).unwrap());

    let new_focus = match key {
        Key::ArrowLeft => current_focus.checked_add(jiff::Span::new().days(-1)).ok(),
        Key::ArrowRight => current_focus.checked_add(jiff::Span::new().days(1)).ok(),
        Key::ArrowUp => current_focus.checked_add(jiff::Span::new().days(-7)).ok(),
        Key::ArrowDown => current_focus.checked_add(jiff::Span::new().days(7)).ok(),
        Key::Home => Date::new(y, m, 1).ok(),
        Key::End => {
            let next_m = if m == 12 {
                Date::new(y + 1, 1, 1).ok()
            } else {
                Date::new(y, m + 1, 1).ok()
            };
            next_m.and_then(|nm| nm.checked_add(jiff::Span::new().days(-1)).ok())
        }
        Key::PageUp => {
            let (ny, nm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
            cm.set((ny, nm));
            Date::new(ny, nm, current_focus.day().min(days_in_month(ny, nm))).ok()
        }
        Key::PageDown => {
            let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            cm.set((ny, nm));
            Date::new(ny, nm, current_focus.day().min(days_in_month(ny, nm))).ok()
        }
        Key::Enter | Key::Space => {
            if let (Some(rs), Some(re)) = (range_start, range_end) {
                apply_range_click(current_focus, rs, re, sel);
                if let Some(ref cb) = on_select {
                    cb();
                }
            } else {
                sel.set(Some(current_focus));
                if let Some(ref cb) = on_select {
                    cb();
                }
            }
            return true;
        }
        _ => return false,
    };

    if let Some(d) = new_focus {
        let in_range = min_date.map_or(true, |lo| d >= lo) && max_date.map_or(true, |hi| d <= hi);
        if in_range {
            let (ny, nm) = (d.year(), d.month());
            cm.set((ny, nm));
            fd.set(Some(d));
            return true;
        }
    }

    false
}

#[cfg(feature = "ext-jiff")]
fn handle_month_key(
    key: &Key,
    cm: &Rc<Cell<(i16, i8)>>,
    mode: &Rc<Cell<CalendarMode>>,
    _dirty: &Rc<Cell<DirtyFlags>>,
    min_date: Option<Date>,
    max_date: Option<Date>,
) -> bool {
    let (y, m) = cm.get();
    let maybe_navigate = |ny: i16, nm: i8| -> bool {
        if let Ok(d) = Date::new(ny, nm, 1) {
            let in_range =
                min_date.map_or(true, |lo| d >= lo) && max_date.map_or(true, |hi| d <= hi);
            if in_range {
                cm.set((ny, nm));
                return true;
            }
        }
        false
    };
    match key {
        // 3×4 month grid: Left/Right move 1 month, Up/Down move 3 month_names_full (1 row)
        Key::ArrowLeft => {
            if m == 1 {
                false
            } else {
                maybe_navigate(y, m - 1)
            }
        }
        Key::ArrowRight => {
            if m == 12 {
                false
            } else {
                maybe_navigate(y, m + 1)
            }
        }
        Key::ArrowUp => {
            if m <= 3 {
                false
            } else {
                maybe_navigate(y, m - 3)
            }
        }
        Key::ArrowDown => {
            if m > 9 {
                false
            } else {
                maybe_navigate(y, m + 3)
            }
        }
        Key::Enter | Key::Space => {
            mode.set(CalendarMode::Day);
            true
        }
        Key::Escape => {
            mode.set(CalendarMode::Day);
            true
        }
        Key::Home => {
            cm.set((y, 1));
            true
        }
        Key::End => {
            cm.set((y, 12));
            true
        }
        _ => false,
    }
}

#[cfg(feature = "ext-jiff")]
fn handle_year_key(
    key: &Key,
    cm: &Rc<Cell<(i16, i8)>>,
    mode: &Rc<Cell<CalendarMode>>,
    _dirty: &Rc<Cell<DirtyFlags>>,
    min_date: Option<Date>,
    max_date: Option<Date>,
) -> bool {
    let (y, m) = cm.get();
    let step: i16 = 3;
    let maybe_navigate_year = |ny: i16| -> bool {
        if let Ok(d) = Date::new(ny, m, 1) {
            let in_range =
                min_date.map_or(true, |lo| d >= lo) && max_date.map_or(true, |hi| d <= hi);
            if in_range {
                cm.set((ny, m));
                return true;
            }
        }
        false
    };
    match key {
        // 3×7 year grid: Left/Right move 1 year, Up/Down move 3 years (1 row)
        Key::ArrowLeft => maybe_navigate_year(y - 1),
        Key::ArrowRight => maybe_navigate_year(y + 1),
        Key::ArrowUp => maybe_navigate_year(y - 3),
        Key::ArrowDown => maybe_navigate_year(y + 3),
        Key::PageUp => maybe_navigate_year(y - step),
        Key::PageDown => maybe_navigate_year(y + step),
        Key::Enter | Key::Space => {
            mode.set(CalendarMode::Day);
            true
        }
        Key::Escape => {
            mode.set(CalendarMode::Day);
            true
        }
        Key::Home => {
            cm.set((y - 20, m));
            true
        }
        Key::End => {
            cm.set((y + 20, m));
            true
        }
        _ => false,
    }
}

#[cfg(feature = "ext-jiff")]
fn days_in_month(year: i16, month: i8) -> i8 {
    let first_of_this = Date::new(year, month, 1);
    let first_of_next = if month == 12 {
        Date::new(year + 1, 1, 1)
    } else {
        Date::new(year, month + 1, 1)
    };
    match (first_of_next, first_of_this) {
        (Ok(next), Ok(this)) => (next - this).get_days() as i8,
        _ => 31,
    }
}

#[cfg(feature = "ext-jiff")]
impl std::fmt::Debug for Calendar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Calendar").finish_non_exhaustive()
    }
}
