use auralis_signal::Signal;
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::dirty_registry::{self, defer_action};
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::action::{Action, ActionKind, ActionOutcome};
use crate::event::EventRegistry;
use crate::event::Modifiers;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Padding, TextAlign};
use crate::theme::m3::roles::{ColorRole, ComponentRole, DisplayRole};
use crate::widgets::display::list::VirtualContentBounds;
use crate::widgets::shared::reorder::ReorderController;
use crate::widgets::shared::{
    set_item_disabled, set_item_highlight, sync_list_selection_focus, SelectionBg, SlotPool,
    TextCellState,
};

use crate::widgets::bundle::ScrollBundle;
use crate::widgets::display::table_row::{
    build_cell, CellPlacement, RowKind, RowOverrides, RowStyle,
};

// ═══════════════════════════════════════════════════════════════════
// Column Width Model
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ColumnWidth {
    Fixed(f32),
    Flex(f32),
}

// ═══════════════════════════════════════════════════════════════════
// Sort Direction
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortDirection {
    Ascending,
    Descending,
}

// ═══════════════════════════════════════════════════════════════════
// Column Definition
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct TableColumn<T: Clone + 'static> {
    pub header: String,
    pub width: ColumnWidth,
    pub min_width: f32,
    pub resizable: bool,
    pub text_align: TextAlign,
    pub font_size: f32,
    pub font_weight: u16,
    render_cell: Option<Rc<dyn Fn(&T, usize, usize) -> String>>,
    cell_color: Option<Rc<dyn Fn(&T, usize, usize) -> Option<Color>>>,
}

impl<T: Clone + 'static> TableColumn<T> {
    pub fn new(header: impl Into<String>, width: ColumnWidth) -> Self {
        Self {
            header: header.into(),
            width,
            min_width: 40.0,
            resizable: false,
            text_align: TextAlign::Start,
            font_size: 13.0,
            font_weight: 400,
            render_cell: None,
            cell_color: None,
        }
    }
    pub fn render(mut self, f: impl Fn(&T, usize, usize) -> String + 'static) -> Self {
        self.render_cell = Some(Rc::new(f));
        self
    }
    /// Override the text color for each cell. The closure receives
    /// `(&T, row_index, col_index)` and returns `Some(Color)` to override
    /// the default foreground, or `None` to use the default.
    pub fn cell_color(mut self, f: impl Fn(&T, usize, usize) -> Option<Color> + 'static) -> Self {
        self.cell_color = Some(Rc::new(f));
        self
    }
    pub fn resizable(mut self) -> Self {
        self.resizable = true;
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
    pub fn text_align(mut self, a: TextAlign) -> Self {
        self.text_align = a;
        self
    }
    pub fn font_size(mut self, s: f32) -> Self {
        self.font_size = s;
        self
    }
    pub fn font_weight(mut self, w: u16) -> Self {
        self.font_weight = w;
        self
    }
}

impl<T: Clone + 'static> std::fmt::Debug for TableColumn<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableColumn")
            .field("header", &self.header)
            .finish_non_exhaustive()
    }
}

// ═══════════════════════════════════════════════════════════════════
// Internal Column Runtime
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub(crate) struct ColRuntime {
    pub spec: ColumnWidth,
    pub current_width: Rc<Cell<f32>>,
    pub text_align: TextAlign,
    pub font_size: f32,
    pub font_weight: u16,
    pub min_width: f32,
}

impl ColRuntime {
    pub(crate) fn init_from(col: &TableColumn<impl Clone + 'static>) -> Self {
        let w = match col.width {
            ColumnWidth::Fixed(px) => px,
            ColumnWidth::Flex(_) => col.min_width.max(100.0),
        };
        Self {
            spec: col.width,
            current_width: Rc::new(Cell::new(w)),
            text_align: col.text_align,
            font_size: col.font_size,
            font_weight: col.font_weight,
            min_width: col.min_width,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Table Widget
// ═══════════════════════════════════════════════════════════════════

/// A virtualized data table with sortable columns and multi-select.
///
/// Define columns with [`TableColumn`] and supply data as a
/// `Signal<Vec<T>>`.  Renders a header row and a virtualized body.
pub struct Table<T: Clone + 'static> {
    rows: Signal<Vec<T>>,
    columns: Vec<TableColumn<T>>,
    sort_column: Option<usize>,
    sort_direction: SortDirection,
    on_sort: Option<Rc<dyn Fn(usize, SortDirection)>>,
    selection: Signal<Option<usize>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    row_height: f32,
    style: StyleRefinement,
    striped: bool,
    disabled_rows: Option<Signal<std::collections::HashSet<usize>>>,
    columns_reorderable: bool,
    on_reorder_column: Option<Rc<dyn Fn(usize, usize)>>,
    footer_texts: Option<Signal<Vec<String>>>,
    footer_exclude_disabled: bool,
    context_menu_items: Vec<crate::widgets::overlay::ContextMenuItem>,
    multi_selection: Option<Signal<std::collections::HashSet<usize>>>,
    virtual_threshold: Option<usize>,
}

impl<T: Clone + 'static> Table<T> {
    pub fn new(rows: Signal<Vec<T>>) -> Self {
        Self {
            rows,
            columns: Vec::new(),
            sort_column: None,
            sort_direction: SortDirection::Ascending,
            on_sort: None,
            selection: Signal::new(None),
            on_select: None,
            row_height: 36.0,
            style: StyleRefinement::default(),
            striped: false,
            disabled_rows: None,
            columns_reorderable: false,
            on_reorder_column: None,
            footer_texts: None,
            footer_exclude_disabled: true,
            context_menu_items: Vec::new(),
            multi_selection: None,
            virtual_threshold: None,
        }
    }
    pub fn columns(mut self, cols: Vec<TableColumn<T>>) -> Self {
        self.columns = cols;
        self
    }
    pub fn sort_column(mut self, col: usize) -> Self {
        self.sort_column = Some(col);
        self
    }
    pub fn on_sort(mut self, f: impl Fn(usize, SortDirection) + 'static) -> Self {
        self.on_sort = Some(Rc::new(f));
        self
    }
    pub fn selection_signal(mut self, sig: Signal<Option<usize>>) -> Self {
        self.selection = sig;
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    /// Enable a multi-select checkbox column (the signal holds checked indices).
    pub fn multi_select(mut self, sig: Signal<std::collections::HashSet<usize>>) -> Self {
        self.multi_selection = Some(sig);
        self
    }
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }
    pub fn striped(mut self, v: bool) -> Self {
        self.striped = v;
        self
    }
    pub fn disabled_rows(mut self, sig: Signal<std::collections::HashSet<usize>>) -> Self {
        self.disabled_rows = Some(sig);
        self
    }
    /// Enable column drag-to-reorder on the header cells.
    pub fn columns_reorderable(mut self, v: bool) -> Self {
        self.columns_reorderable = v;
        self
    }
    /// Called when a column reorder completes: `on_reorder_column(src, dst)`.
    pub fn on_reorder_column(mut self, f: impl Fn(usize, usize) + 'static) -> Self {
        self.on_reorder_column = Some(Rc::new(f));
        self
    }
    /// Attach a footer row whose text cells are driven by `texts` (one per column).
    /// The table shares column widths and styling with the footer automatically.
    /// By default disabled rows are excluded from footer computation; use
    /// `footer_include_disabled()` to include them.
    pub fn footer(mut self, texts: Signal<Vec<String>>) -> Self {
        self.footer_texts = Some(texts);
        self
    }
    /// When `true` (default), the footer counts disabled rows.  When `false`,
    /// disabled rows are still counted — the developer can use whichever semantics
    /// fit their use case.
    pub fn footer_include_disabled(mut self, v: bool) -> Self {
        self.footer_exclude_disabled = v;
        self
    }
    /// Attach a context menu to the table container (right-click anywhere on the table).
    /// For per-row context menus, wrap each row element with set_context_menu() during mount.
    pub fn context_menu(mut self, items: Vec<crate::widgets::overlay::ContextMenuItem>) -> Self {
        self.context_menu_items = items;
        self
    }
    /// Enable row virtualisation: when rows exceed `count`, reuse a pool of
    /// `min(count, rows)` slot elements instead of allocating one per row.
    pub fn virtual_threshold(mut self, count: usize) -> Self {
        self.virtual_threshold = Some(count);
        self
    }
    /// Disable virtual scrolling (always allocate one element per row).
    pub fn no_virtual(mut self) -> Self {
        self.virtual_threshold = None;
        self
    }
}

impl<T: Clone + 'static> Styled for Table<T> {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

// ═══════════════════════════════════════════════════════════════════
// Pool State (shared: signal callback + defer_action)
// ═══════════════════════════════════════════════════════════════════

struct RowState {
    eid: ElementId,
    cell_ids: Vec<ElementId>,
    cell_states: Vec<TextCellState>,
}

struct PoolState {
    rows: Vec<RowState>,
    pool_size: usize,
}

// ═══════════════════════════════════════════════════════════════════
// Focus outline helpers
// ═══════════════════════════════════════════════════════════════════

fn set_cell_outline(eid: ElementId, color: Color, width: f32) {
    let changed = crate::core::element::with_ct_mut(|ct| {
        if let Some(s) = ct.style.get_mut(&eid) {
            let changed =
                (s.outline_color != Some(color)) || (s.outline_width - width).abs() > 0.01;
            s.outline_color = Some(color);
            s.outline_width = width;
            changed
        } else {
            false
        }
    });
    if changed {
        dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
        dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
        dirty_registry::bump_subtree_gen(eid);
    }
}

fn clear_cell_outline(eid: ElementId) {
    let changed = crate::core::element::with_ct_mut(|ct| {
        if let Some(s) = ct.style.get_mut(&eid) {
            let changed = s.outline_color.is_some() || s.outline_width > 0.01;
            s.outline_color = None;
            s.outline_width = 0.0;
            changed
        } else {
            false
        }
    });
    if changed {
        dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
        dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
        dirty_registry::bump_subtree_gen(eid);
    }
}

fn mark_a11y_dirty() {
    dirty_registry::mark_a11y_dirty();
}

fn data_to_slot(rev_map: &RefCell<FxHashMap<usize, usize>>, data_idx: usize) -> Option<usize> {
    rev_map.borrow().get(&data_idx).copied()
}

// ═══════════════════════════════════════════════════════════════════
// Resize handle geometry
// ═══════════════════════════════════════════════════════════════════

/// Width of the grab zone (the absolutely-positioned handle element).
const RESIZE_HANDLE_W: f32 = 7.0;
/// Width of the visible line drawn inside the grab zone.
const RESIZE_BAR_W: f32 = 2.0;

/// `(column_index, handle_element_id, absolute_inset_cell)` for one resize handle.
/// The cell stores `(inset_x, inset_y, width)` consumed by the absolute-layout path.
type HandleSpec = (usize, ElementId, Rc<Cell<(f32, f32, f32)>>);

/// Recompute the absolute inset `(x, y=0, w)` for every resize handle from the
/// columns' live widths. A zero-sum resize of one boundary shifts the boundaries
/// to its right, so all handles are repositioned together.
fn reposition_handles(col_configs: &[ColRuntime], header_pad_left: f32, specs: &[HandleSpec]) {
    for (ci, _, pos) in specs {
        let x = header_pad_left
            + col_configs[..=*ci]
                .iter()
                .map(|c| c.current_width.get())
                .sum::<f32>();
        pos.set((x - RESIZE_HANDLE_W / 2.0, 0.0, RESIZE_HANDLE_W));
    }
}

/// Sync Flex column current_widths from the actual rendered element bounds.
fn sync_flex_widths(
    cfgs: &[ColRuntime],
    header_cell_ids: &[ElementId],
    arena: &crate::core::element::ElementArena,
) {
    for ci in 0..cfgs.len().min(header_cell_ids.len()) {
        if matches!(cfgs[ci].spec, ColumnWidth::Flex(_)) {
            if let Some(el) = arena.get(header_cell_ids[ci]) {
                let w = el.screen_bounds.width;
                if w > 1.0 {
                    let old = cfgs[ci].current_width.get();
                    if (old - w).abs() > 0.5 {
                        cfgs[ci].current_width.set(w);
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Widget impl
// ═══════════════════════════════════════════════════════════════════

impl<T: Clone + 'static> Widget for Table<T> {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::INTERACTION
            | components::TEXT
            | components::SCROLL
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let component_mask = self.component_mask();
        let sort_col_v = self.sort_column;
        let sort_dir_v = self.sort_direction;
        let row_h = self.row_height;
        let rows_sig = self.rows.clone();
        let sel_sig = self.selection.clone();
        let on_sel = self.on_select.clone();
        let on_sort = self.on_sort.clone();
        let is_striped = self.striped;
        // Grid border: slightly more visible than the default border color
        let grid_border = theme.scheme.outline_variant;
        let focus_color = theme.scheme.primary;
        // Hover: same as List — primary container
        let hover_bg = theme.scheme.primary_container;
        // Pressed: darker than container
        let pressed_bg = theme.scheme.primary;
        // Zebra stripe: container-level tint for ≥3:1 non-text contrast with surface
        // (was surface_secondary at 1.4:1 — too subtle)
        let even_bg = theme.scheme.secondary_container;
        let surf_bg = theme.scheme.surface;
        let fg_color = theme.scheme.on_surface;
        // Selected: primary container (same as hover, but persists after leaving)
        let sel_bg = theme.scheme.primary_container;
        // Focus within keyboard nav: primary container
        let focus_bg = theme.scheme.primary_container;
        let num_cols = self.columns.len();
        let has_cb = self.multi_selection.is_some();
        let cb_sig = self.multi_selection.clone();

        // Column runtime config
        let col_configs: Vec<ColRuntime> = self.columns.iter().map(ColRuntime::init_from).collect();
        let cols =
            crate::widgets::display::table_row::TableColumns::new(col_configs.clone(), has_cb);
        let render_fns: Vec<Option<Rc<dyn Fn(&T, usize, usize) -> String>>> =
            self.columns.iter().map(|c| c.render_cell.clone()).collect();
        let color_fns: Vec<Option<Rc<dyn Fn(&T, usize, usize) -> Option<Color>>>> =
            self.columns.iter().map(|c| c.cell_color.clone()).collect();

        let sb_w = 10.0;
        let sb_gutter = sb_w + 2.0;

        // ═══ Container ═══
        let container_id = ctx.arena.allocate();
        ctx.preallocate(container_id, component_mask);
        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_accessible_role(accesskit::Role::Grid);
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_flex_grow(1.0);
            el.set_focusable(true);
            el.set_outline_color(Color::TRANSPARENT);
            el.set_padding(Padding {
                left: 10.0,
                right: 12.0,
                top: 0.0,
                bottom: 0.0,
            });
            if !self.context_menu_items.is_empty() {
                el.set_context_menu(self.context_menu_items.clone());
            }
        }

        // ═══ Container component_role ═══
        {
            let role = ComponentRole::Display(DisplayRole::Text {
                foreground: ColorRole::OnSurface,
            });
            ctx.register_theme_component(
                container_id,
                &theme.resolve_component(&role),
                &role,
                &self.style,
            );
        }

        // ═══ Body (created early so header resize handles can reference it) ═══
        let body_id = ctx.arena.allocate();
        ctx.preallocate(body_id, component_mask);
        {
            let Some(el) = ctx.arena.get_mut(body_id) else {
                return container_id;
            };
            el.set_accessible_role(accesskit::Role::RowGroup);
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_flex_grow(1.0);
            el.set_flex_shrink(0.0);
        }

        let rows_state: Rc<RefCell<PoolState>> = Rc::new(RefCell::new(PoolState {
            rows: Vec::new(),
            pool_size: 0,
        }));

        // ═══ Header Row ═══
        let header_row_id = ctx.arena.allocate();
        {
            let Some(el) = ctx.arena.get_mut(header_row_id) else {
                return container_id;
            };
            el.set_accessible_role(accesskit::Role::Row);
            el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
            el.set_preferred_height(row_h);
            el.set_flex_shrink(0.0);
            el.set_padding(Padding {
                right: sb_gutter,
                ..Padding::ZERO
            });
            el.set_background(surf_bg);
        }
        ctx.arena.add_child(container_id, header_row_id);
        {
            let role = ComponentRole::Display(DisplayRole::Text {
                foreground: ColorRole::OnSurface,
            });
            ctx.register_theme_component(
                header_row_id,
                &theme.resolve_component(&role),
                &role,
                &self.style,
            );
        }

        // Checkbox column geometry offsets — applied to header/resize/reorder so
        // the checkbox track is accounted for. `cb_col_off` converts a data-column
        // index into a grid-column index; `cb_off` is the checkbox track width.
        let cb_col_off: usize = if has_cb { 1 } else { 0 };
        let cb_off: f32 = if has_cb { cols.cb_w } else { 0.0 };

        // header_cell_ids holds ONLY data-column header cells (data-indexed),
        // matching col_configs / render_fns. The checkbox header cell is added to
        // the row but deliberately NOT tracked here, so resize/reorder indexing
        // stays purely in the data-column space.
        let header_cell_ids: Rc<RefCell<Vec<ElementId>>> =
            Rc::new(RefCell::new(Vec::with_capacity(num_cols)));
        // Footer cell IDs — created before resize handles so the resize
        // defer_action can update footer cell widths alongside body rows.
        let footer_cell_ids_rc: Rc<RefCell<Vec<ElementId>>> = Rc::new(RefCell::new(Vec::new()));
        // ── Checkbox header cell = select-all (not part of header_cell_ids) ──
        let cb_header: Option<(ElementId, Rc<TextCellState>)> = if has_cb {
            let cb_hid = ctx.arena.allocate();
            let ts = {
                let Some(el) = ctx.arena.get_mut(cb_hid) else {
                    return container_id;
                };
                el.set_preferred_height(row_h);
                el.set_preferred_width(Some(cols.cb_w - 16.0));
                el.set_flex_shrink(0.0);
                el.set_affected_by_child_size(false);
                el.set_padding(Padding {
                    left: 8.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                });
                el.set_accepts_mouse(true);
                el.set_font_size(13.0);
                el.set_font_weight(600);
                el.set_text_align(TextAlign::Center);
                el.set_text_vertical_center(true);
                el.set_background(surf_bg);
                el.set_border_width(1.0);
                el.set_border_color(grid_border);
                TextCellState::mount(
                    cb_hid,
                    el,
                    "\u{2610}",
                    13.0,
                    1.0,
                    600,
                    None,
                    Some(16.0),
                    TextAlign::Center,
                )
            };
            ctx.arena.add_child(header_row_id, cb_hid);
            Some((cb_hid, Rc::new(ts)))
        } else {
            None
        };
        for (ci, col) in self.columns.iter().enumerate() {
            let sort_indicator = if sort_col_v == Some(ci) {
                match sort_dir_v {
                    SortDirection::Ascending => " \u{25B2}",
                    SortDirection::Descending => " \u{25BC}",
                }
            } else {
                ""
            };
            let display_text = format!("{}{}", col.header, sort_indicator);
            let hcell_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(hcell_id) else {
                    return container_id;
                };
                el.set_accessible_role(accesskit::Role::ColumnHeader);
                el.set_accessible_label(col.header.clone());
                el.set_preferred_height(row_h);
                el.set_flex_shrink(0.0);
                el.set_affected_by_child_size(false);
                el.set_font_size(13.0);
                el.set_font_weight(600);
                el.set_accepts_mouse(true);
                el.set_background(surf_bg);
                el.set_text_align(col.text_align);
                el.set_text_vertical_center(true);
                el.set_padding(Padding {
                    left: 8.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                });
                // Border matches body grid cells (1 px, border_hover) so column boundaries
                // are visually consistent and text starts at the same offset in both.
                el.set_border_width(1.0);
                el.set_border_color(theme.scheme.outline_variant);
                // Content = column px - padding(16) - border(2), outer = px = body grid track.
                match col.width {
                    ColumnWidth::Fixed(px) => {
                        el.set_preferred_width(Some((px - 16.0).max(4.0)));
                    }
                    ColumnWidth::Flex(f) => {
                        el.set_flex_grow(f);
                        el.set_preferred_width(Some(0.0));
                    }
                }
                let max_w = match col.width {
                    ColumnWidth::Fixed(px) => Some((px - 16.0).max(4.0)),
                    ColumnWidth::Flex(_) => None,
                };
                let buf = Rc::new(RefCell::new(create_buffer(
                    &display_text,
                    13.0,
                    1.0,
                    600,
                    None,
                    max_w,
                    col.text_align,
                )));
                el.set_text_buffer(buf);
            }
            {
                let os = on_sort.clone();
                let header_events = EventHandler::new().on_click(move || {
                    let new_dir = if sort_col_v == Some(ci) {
                        match sort_dir_v {
                            SortDirection::Ascending => SortDirection::Descending,
                            _ => SortDirection::Ascending,
                        }
                    } else {
                        SortDirection::Ascending
                    };
                    if let Some(ref f) = os {
                        f(ci, new_dir);
                    }
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    header_events.register_all(reg, hcell_id);
                }
            }
            ctx.arena.add_child(header_row_id, hcell_id);
            header_cell_ids.borrow_mut().push(hcell_id);
            {
                let role = ComponentRole::Display(DisplayRole::Text {
                    foreground: ColorRole::OnSurface,
                });
                ctx.register_theme_component(
                    hcell_id,
                    &theme.resolve_component(&role),
                    &role,
                    &self.style,
                );
            }
        }

        // ═══ Resize handles ═══
        // A thin handle per resizable boundary: an absolutely-positioned (z_index=1)
        // child of the header row, centred on the column boundary. Being out of flow
        // it steals no column width; having real bounds it is hit-tested exactly where
        // it is drawn. The header cell itself stays the sort-click zone.
        let handle_specs: Rc<RefCell<Vec<HandleSpec>>> = Rc::new(RefCell::new(Vec::new()));
        for (ci, col) in self.columns.iter().enumerate() {
            if !(col.resizable && ci + 1 < num_cols) {
                continue;
            }
            let bx = cb_off
                + col_configs[..=ci]
                    .iter()
                    .map(|c| c.current_width.get())
                    .sum::<f32>();
            let handle_pos: Rc<Cell<(f32, f32, f32)>> = Rc::new(Cell::new((
                bx - RESIZE_HANDLE_W / 2.0,
                0.0,
                RESIZE_HANDLE_W,
            )));

            let handle_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(handle_id) else {
                    return container_id;
                };
                el.set_accessible_role(accesskit::Role::Splitter);
                el.set_accessible_label("Resize column".to_owned());
                el.set_z_index(1); // → taffy Position::Absolute (out of flow)
                el.insert_user_data(handle_pos.clone()); // (inset_x, inset_y, width)
                el.set_accepts_mouse(true);
                el.set_cursor_icon(Some(crate::platform::CursorIcon::COL_RESIZE));
                el.set_background(Color::TRANSPARENT);
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_content_align(crate::style::Alignment::Center);
            }
            // Visible thin bar, centred in the grab zone. Also forces the container
            // (absolute) layout path, which requires ≥1 active child.
            let bar_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(bar_id) else {
                    return container_id;
                };
                el.set_preferred_width(Some(RESIZE_BAR_W));
                el.set_preferred_height(row_h);
                el.set_accepts_mouse(false);
                el.set_background(grid_border);
            }
            ctx.arena.add_child(handle_id, bar_id);
            ctx.arena.add_child(header_row_id, handle_id);
            handle_specs
                .borrow_mut()
                .push((ci, handle_id, handle_pos.clone()));

            {
                let wc_left = col_configs[ci].current_width.clone();
                let wc_right_ds = col_configs[ci + 1].current_width.clone();
                let wc_right_upd = col_configs[ci + 1].current_width.clone();
                let ml = col_configs[ci].min_width;
                let mr = col_configs[ci + 1].min_width;

                let start_x: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
                let init_left: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
                let init_right: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));

                let s = start_x.clone();
                let il = init_left.clone();
                let ir = init_right.clone();
                let wl0 = wc_left.clone();

                let wc_left_upd = wc_left.clone();
                let col_ci = ci;
                let col_next = ci + 1;
                let init_l = init_left.clone();
                let init_r = init_right.clone();
                let row_state = rows_state.clone();
                let hc_ids = header_cell_ids.clone();
                let hdr_row = header_row_id;
                let cfgs = col_configs.clone();
                let specs = handle_specs.clone();
                let fcell_ids = footer_cell_ids_rc.clone();

                let handle_events = EventHandler::new()
                    .on_drag_start(
                        move |_local: crate::style::Point, abs: crate::style::Point| {
                            let v_l = wl0.get();
                            let v_r = wc_right_ds.get();
                            s.set(v_l - abs.x);
                            il.set(v_l);
                            ir.set(v_r);
                        },
                    )
                    .on_drag_update({
                        let start_x = start_x.clone();
                        let init_l = init_l.clone();
                        let init_r = init_r.clone();
                        let wc_left_upd = wc_left_upd.clone();
                        let wc_right_upd = wc_right_upd.clone();
                        let col_ci = col_ci;
                        let col_next = col_next;
                        let row_state = row_state.clone();
                        let hc_ids = hc_ids.clone();
                        let hdr_row = hdr_row;
                        let cfgs = cfgs.clone();
                        let specs = specs.clone();
                        let fcell_ids = fcell_ids.clone();
                        move |_local: crate::style::Point, abs: crate::style::Point| {
                            if init_l.get() == 0.0 && init_r.get() == 0.0 {
                                return;
                            }
                            let raw_left = start_x.get() + abs.x;
                            let max_left = init_l.get() + init_r.get() - mr;
                            let new_left = raw_left.max(ml).min(max_left);
                            wc_left_upd.set(new_left);
                            let delta = new_left - init_l.get();
                            let new_right = (init_r.get() - delta).max(mr);
                            wc_right_upd.set(new_right);
                            let wl = new_left;
                            let wr = new_right;
                            let lci = col_ci;
                            let rci = col_next;
                            let st = row_state.clone();
                            let headers = hc_ids.clone();
                            let hdr_row = hdr_row;
                            let cfgs = cfgs.clone();
                            let specs = specs.clone();
                            let fcell_ids2 = fcell_ids.clone();
                            defer_action(move |_arena, _root, _reg| {
                                let state = st.borrow();
                                for row in &state.rows {
                                    crate::core::element::with_ct_mut(|ct| {
                                        if let Some(layout) = ct.layout.get_mut(&row.eid) {
                                            if let Some(slot) =
                                                layout.grid_column_widths.get_mut(lci + cb_col_off)
                                            {
                                                *slot = wl;
                                            }
                                            if let Some(slot) =
                                                layout.grid_column_widths.get_mut(rci + cb_col_off)
                                            {
                                                *slot = wr;
                                            }
                                        }
                                    });
                                }
                                {
                                    let fcids = fcell_ids2.borrow();
                                    let set_fw = |ci: usize, w: f32| {
                                        let cw = (w - 16.0).max(4.0);
                                        if let Some(&fid) = fcids.get(ci) {
                                            crate::core::element::with_ct_mut(|ct| {
                                                if let Some(l) = ct.layout.get_mut(&fid) {
                                                    l.preferred_width = Some(cw);
                                                }
                                            });
                                        }
                                    };
                                    set_fw(lci, wl);
                                    set_fw(rci, wr);
                                }
                                for row in &state.rows {
                                    for &ci in &[lci, rci] {
                                        if let Some(&cid) = row.cell_ids.get(ci + cb_col_off) {
                                            crate::core::element::with_ct_mut(|ct| {
                                                if let Some(t) = ct.text.get_mut(&cid) {
                                                    t.text_generation.set(
                                                        t.text_generation.get().wrapping_add(1),
                                                    );
                                                }
                                            });
                                        }
                                    }
                                }
                                let h = headers.borrow();
                                let set_cell = |ci: usize, w: f32| {
                                    let cw = (w - 16.0).max(4.0);
                                    crate::core::element::with_ct_mut(|ct| {
                                        if let Some(&hc) = h.get(ci) {
                                            if let Some(l) = ct.layout.get_mut(&hc) {
                                                l.preferred_width = Some(cw);
                                            }
                                        }
                                    });
                                };
                                set_cell(lci, wl);
                                set_cell(rci, wr);
                                drop(h);
                                reposition_handles(&cfgs, cb_off, &specs.borrow());
                                dirty_registry::mark_structurally_changed(hdr_row);
                                dirty_registry::mark_structurally_changed(body_id);

                                // ── Follow-up: after layout runs, sync Flex widths & reposition ──
                                let cfgs_fup = cfgs.clone();
                                let specs_fup = specs.clone();
                                let headers_fup = headers.clone();
                                let hdr_fup = hdr_row;
                                defer_action(move |arena, _root, _reg| {
                                    sync_flex_widths(&cfgs_fup, &headers_fup.borrow(), arena);
                                    reposition_handles(&cfgs_fup, cb_off, &specs_fup.borrow());
                                    for &(_, hid, _) in specs_fup.borrow().iter() {
                                        dirty_registry::mark_dirty(hid, DirtyFlags::MEASURE);
                                        dirty_registry::register_dirty(hid, DirtyFlags::MEASURE);
                                        dirty_registry::bump_surface_gen_remote(hid);
                                    }
                                    dirty_registry::mark_structurally_changed(hdr_fup);
                                });
                            });
                        }
                    });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    handle_events.register_all(reg, handle_id);
                }
            }
        }

        // Sync Flex column widths after first layout (handles already created)
        {
            let cfgs_init = col_configs.clone();
            let hids_init = header_cell_ids.clone();
            let hspec_init = handle_specs.clone();
            defer_action(move |arena, _root, _reg| {
                sync_flex_widths(&cfgs_init, &hids_init.borrow(), arena);
                reposition_handles(&cfgs_init, cb_off, &hspec_init.borrow());
                for &(_, hid, _) in hspec_init.borrow().iter() {
                    dirty_registry::mark_dirty(hid, DirtyFlags::MEASURE);
                    dirty_registry::register_dirty(hid, DirtyFlags::MEASURE);
                    dirty_registry::bump_surface_gen_remote(hid);
                }

                // ── Follow-up: after first layout, sync Flex widths & reposition ──
                let cfgs_i2 = cfgs_init.clone();
                let hids_i2 = hids_init.clone();
                let hspec_i2 = hspec_init.clone();
                let hdr_i2 = header_row_id;
                defer_action(move |arena, _root, _reg| {
                    sync_flex_widths(&cfgs_i2, &hids_i2.borrow(), arena);
                    reposition_handles(&cfgs_i2, cb_off, &hspec_i2.borrow());
                    for &(_, hid, _) in hspec_i2.borrow().iter() {
                        dirty_registry::mark_dirty(hid, DirtyFlags::MEASURE);
                        dirty_registry::register_dirty(hid, DirtyFlags::MEASURE);
                        dirty_registry::bump_surface_gen_remote(hid);
                    }
                    dirty_registry::mark_structurally_changed(hdr_i2);
                });
            });
        }

        // ═══ Column drag-to-reorder ═══
        if self.columns_reorderable {
            let col_w = match &col_configs[0].spec {
                ColumnWidth::Fixed(px) => *px,
                ColumnWidth::Flex(_) => 120.0,
            };
            let mut col_offsets: Vec<Rc<Cell<crate::style::Vec2>>> = Vec::with_capacity(num_cols);
            let hcell_ids = header_cell_ids.borrow();
            for ci in 0..num_cols {
                let off = Rc::new(Cell::new(crate::style::Vec2::ZERO));
                col_offsets.push(off.clone());
                if let Some(&hc) = hcell_ids.get(ci) {
                    if let Some(el) = ctx.arena.get_mut(hc) {
                        el.set_position_offset(off);
                    }
                }
            }

            let swap_bag: Rc<
                RefCell<(
                    Vec<ColRuntime>,
                    Vec<Option<Rc<dyn Fn(&T, usize, usize) -> String>>>,
                    Vec<ElementId>,
                    ElementId,
                    ElementId,
                    Rc<RefCell<PoolState>>,
                    Signal<Vec<T>>,
                    Vec<String>,
                    Rc<RefCell<Vec<HandleSpec>>>,
                )>,
            > = Rc::new(RefCell::new((
                col_configs.clone(),
                render_fns.clone(),
                hcell_ids.clone(),
                header_row_id,
                body_id,
                rows_state.clone(),
                self.rows.clone(),
                self.columns.iter().map(|c| c.header.clone()).collect(),
                handle_specs.clone(),
            )));

            let controller = ReorderController::new();
            controller.set_horizontal(col_w);
            controller.configure(hcell_ids.clone(), col_offsets);
            let ro_ctrl = Rc::new(controller);

            {
                let on_reorder_cb = self.on_reorder_column.clone();
                let hcell_ids_for_drag = hcell_ids.clone();
                let swap_b = swap_bag.clone();
                for &hcell_id in hcell_ids_for_drag.iter() {
                    let drag_events = EventHandler::new()
                        .on_drag_start({
                            let ctrl = ro_ctrl.clone();
                            move |_local: crate::style::Point, abs: crate::style::Point| {
                                ctrl.begin(hcell_id, abs);
                            }
                        })
                        .on_drag_update({
                            let ctrl = ro_ctrl.clone();
                            move |_local: crate::style::Point, abs: crate::style::Point| {
                                ctrl.update(abs);
                            }
                        })
                        .on_drag_end({
                            let ctrl = ro_ctrl.clone();
                            let bag2 = swap_b.clone();
                            let ocb2 = on_reorder_cb.clone();
                            move |_local: crate::style::Point, _abs: crate::style::Point| {
                                if let Some((src, dst)) = ctrl.end() {
                                    if src == dst {
                                        return;
                                    }
                                    let mut bag = bag2.borrow_mut();
                                    let (
                                        ref mut cfgs,
                                        ref mut rfns,
                                        ref hc_ids_s,
                                        ref hdr2,
                                        ref body2,
                                        ref rows_st,
                                        ref rows_sig,
                                        ref _col_hdrs,
                                        ref hspecs,
                                    ) = *bag;
                                    let (lo, hi) = if src < dst {
                                        let (l, r) = cfgs.split_at_mut(dst);
                                        (&mut l[src], &mut r[0])
                                    } else {
                                        let (l, r) = cfgs.split_at_mut(src);
                                        (&mut r[0], &mut l[dst])
                                    };
                                    std::mem::swap(&mut lo.spec, &mut hi.spec);
                                    std::mem::swap(&mut lo.text_align, &mut hi.text_align);
                                    std::mem::swap(&mut lo.font_size, &mut hi.font_size);
                                    std::mem::swap(&mut lo.font_weight, &mut hi.font_weight);
                                    std::mem::swap(&mut lo.min_width, &mut hi.min_width);
                                    let v_src = lo.current_width.get();
                                    let v_dst = hi.current_width.get();
                                    lo.current_width.set(v_dst);
                                    hi.current_width.set(v_src);
                                    rfns.swap(src, dst);

                                    crate::core::element::with_ct_mut(|ct| {
                                        // Swap text buffers
                                        let buf_src = ct
                                            .text
                                            .get(&hc_ids_s[src])
                                            .and_then(|t| t.text_buffer.clone());
                                        let buf_dst = ct
                                            .text
                                            .get(&hc_ids_s[dst])
                                            .and_then(|t| t.text_buffer.clone());
                                        if let Some(ref b) = buf_dst {
                                            if let Some(t) = ct.text.get_mut(&hc_ids_s[src]) {
                                                t.text_buffer = Some(b.clone());
                                            }
                                        }
                                        if let Some(ref b) = buf_src {
                                            if let Some(t) = ct.text.get_mut(&hc_ids_s[dst]) {
                                                t.text_buffer = Some(b.clone());
                                            }
                                        }
                                        // Swap preferred_width
                                        let pw_src = ct
                                            .layout
                                            .get(&hc_ids_s[src])
                                            .and_then(|l| l.preferred_width);
                                        let pw_dst = ct
                                            .layout
                                            .get(&hc_ids_s[dst])
                                            .and_then(|l| l.preferred_width);
                                        if let Some(p) = pw_dst {
                                            if let Some(l) = ct.layout.get_mut(&hc_ids_s[src]) {
                                                l.preferred_width = Some(p);
                                            }
                                        }
                                        if let Some(p) = pw_src {
                                            if let Some(l) = ct.layout.get_mut(&hc_ids_s[dst]) {
                                                l.preferred_width = Some(p);
                                            }
                                        }
                                        // Swap flex_grow (critical for Fixed↔Flex column reorder)
                                        let fg_src = ct
                                            .layout
                                            .get(&hc_ids_s[src])
                                            .map(|l| l.flex_grow)
                                            .unwrap_or(0.0);
                                        let fg_dst = ct
                                            .layout
                                            .get(&hc_ids_s[dst])
                                            .map(|l| l.flex_grow)
                                            .unwrap_or(0.0);
                                        if let Some(l) = ct.layout.get_mut(&hc_ids_s[src]) {
                                            l.flex_grow = fg_dst;
                                        }
                                        if let Some(l) = ct.layout.get_mut(&hc_ids_s[dst]) {
                                            l.flex_grow = fg_src;
                                        }
                                        // Swap text_align
                                        let ta_src =
                                            ct.text.get(&hc_ids_s[src]).map(|t| t.text_align);
                                        let ta_dst =
                                            ct.text.get(&hc_ids_s[dst]).map(|t| t.text_align);
                                        if let Some(v) = ta_dst {
                                            if let Some(t) = ct.text.get_mut(&hc_ids_s[src]) {
                                                t.text_align = v;
                                            }
                                        }
                                        if let Some(v) = ta_src {
                                            if let Some(t) = ct.text.get_mut(&hc_ids_s[dst]) {
                                                t.text_align = v;
                                            }
                                        }
                                        // Swap accessible_label
                                        let la_src = ct
                                            .a11y
                                            .get(&hc_ids_s[src])
                                            .and_then(|a| a.accessible_label.clone());
                                        let la_dst = ct
                                            .a11y
                                            .get(&hc_ids_s[dst])
                                            .and_then(|a| a.accessible_label.clone());
                                        if let Some(ref l) = la_dst {
                                            if let Some(a) = ct.a11y.get_mut(&hc_ids_s[src]) {
                                                a.accessible_label = Some(l.clone());
                                            }
                                        }
                                        if let Some(ref l) = la_src {
                                            if let Some(a) = ct.a11y.get_mut(&hc_ids_s[dst]) {
                                                a.accessible_label = Some(l.clone());
                                            }
                                        }
                                        // Bump generations on both header cells to invalidate scene cache
                                        for &hcid in &[hc_ids_s[src], hc_ids_s[dst]] {
                                            let gen = ct
                                                .text
                                                .entry(hcid)
                                                .or_default()
                                                .text_generation
                                                .clone();
                                            gen.set(gen.get().wrapping_add(1));
                                        }
                                    });
                                    let data = rows_sig.read();
                                    let state = rows_st.borrow();
                                    for (ri, row) in state.rows.iter().enumerate() {
                                        for &col in &[src, dst] {
                                            let text = data
                                                .get(ri)
                                                .and_then(|item| {
                                                    rfns.get(col)
                                                        .and_then(|f| f.as_ref())
                                                        .map(|f| f(item, ri, col))
                                                })
                                                .unwrap_or_default();
                                            row.cell_states[col + cb_col_off].set_text(&text);
                                        }
                                    }
                                    for row in &state.rows {
                                        crate::core::element::with_ct_mut(|ct| {
                                            if let Some(layout) = ct.layout.get_mut(&row.eid) {
                                                let len = layout.grid_column_widths.len();
                                                let (gs, gd) = (src + cb_col_off, dst + cb_col_off);
                                                if gs < len && gd < len {
                                                    layout.grid_column_widths.swap(gs, gd);
                                                }
                                            }
                                        });
                                    }
                                    // ═══ Column reorder commit ═══
                                    drop(state);
                                    {
                                        let cfgs_rep = cfgs.clone();
                                        let hspecs_rep = hspecs.clone();
                                        let hdr_rep = *hdr2;
                                        let body_rep = *body2;
                                        let cb2 = ocb2.clone();
                                        let hc_ids_for_defer = hc_ids_s.clone();
                                        defer_action(move |arena, _root, _reg| {
                                            sync_flex_widths(&cfgs_rep, &hc_ids_for_defer, arena);
                                            reposition_handles(
                                                &cfgs_rep,
                                                cb_off,
                                                &hspecs_rep.borrow(),
                                            );
                                            for &(_, hid, _) in hspecs_rep.borrow().iter() {
                                                dirty_registry::mark_dirty(
                                                    hid,
                                                    DirtyFlags::MEASURE,
                                                );
                                                dirty_registry::register_dirty(
                                                    hid,
                                                    DirtyFlags::MEASURE,
                                                );
                                                dirty_registry::bump_surface_gen_remote(hid);
                                            }
                                            dirty_registry::mark_structurally_changed(hdr_rep);
                                            dirty_registry::mark_structurally_changed(body_rep);
                                            if let Some(ref cb) = cb2 {
                                                cb(src, dst);
                                            }

                                            // ── Follow-up: after layout, resync Flex widths & reposition ──
                                            let cfgs_f2 = cfgs_rep.clone();
                                            let hspecs_f2 = hspecs_rep.clone();
                                            let hcids_f2 = hc_ids_for_defer.clone();
                                            let hdr_f2 = hdr_rep;
                                            defer_action(move |arena, _root, _reg| {
                                                sync_flex_widths(&cfgs_f2, &hcids_f2, arena);
                                                reposition_handles(
                                                    &cfgs_f2,
                                                    cb_off,
                                                    &hspecs_f2.borrow(),
                                                );
                                                for &(_, hid, _) in hspecs_f2.borrow().iter() {
                                                    dirty_registry::mark_dirty(
                                                        hid,
                                                        DirtyFlags::MEASURE,
                                                    );
                                                    dirty_registry::register_dirty(
                                                        hid,
                                                        DirtyFlags::MEASURE,
                                                    );
                                                    dirty_registry::bump_surface_gen_remote(hid);
                                                }
                                                dirty_registry::mark_structurally_changed(hdr_f2);
                                            });
                                        });
                                    }
                                }
                            }
                        });
                    if let Some(reg) = ctx.event_registry.as_mut() {
                        drag_events.register_all(reg, hcell_id);
                    }
                }
            }
        }

        // ═══ Pool pre-allocation (before scroll, for total_h) ═══
        let initial_len = rows_sig.read().len();
        let virtual_threshold = self.virtual_threshold.unwrap_or(20);
        let use_virtual = initial_len > virtual_threshold;
        let initial_pool = if use_virtual {
            virtual_threshold.min(initial_len)
        } else {
            initial_len.max(8)
        };

        // ═══ Scroll Body ═══
        let extra_mask = components::STYLE
            | components::LAYOUT
            | components::INTERACTION
            | components::TEXT
            | components::ACCESSIBLE
            | components::LIFECYCLE;
        let bundle = ScrollBundle::new(
            ctx,
            extra_mask,
            crate::widgets::layout::ScrollDirection::Vertical,
            sb_w,
        );
        let scroll_id = bundle.container_id;
        let scroll_offset = bundle.scroll_offset.clone();
        {
            let Some(el) = ctx.arena.get_mut(scroll_id) else {
                return container_id;
            };
            let total_h = initial_len as f32 * row_h
                + if self.footer_texts.is_some() {
                    row_h
                } else {
                    0.0
                };
            el.insert_user_data(VirtualContentBounds(std::cell::Cell::new(
                crate::style::Rect::new(0.0, 0.0, 0.0, total_h),
            )));
        }
        ctx.arena.add_child(container_id, scroll_id);
        ctx.arena.add_child(bundle.clip_id, body_id);

        // Resize pool_size now that we know it
        {
            let mut st = rows_state.borrow_mut();
            st.rows.reserve(initial_pool);
            st.pool_size = initial_pool;
        }

        // Pool bookkeeping arrays are reserved up to `pool_capacity` (cheap
        // Cells — a few KB) while row ELEMENTS are created lazily: the
        // viewport height is unknown at mount, so the viewport tracker below
        // grows the element pool on demand (audit follow-up #1: a pool
        // smaller than the viewport left the bottom rows empty).
        const MAX_VIEWPORT_PX: f32 = 4320.0; // 8K portrait
        let pool_capacity = if use_virtual {
            ((MAX_VIEWPORT_PX / row_h).ceil() as usize + 2).max(initial_pool)
        } else {
            0
        };
        let slot_to_virtual: Rc<Vec<Cell<usize>>> = if use_virtual {
            Rc::new((0..pool_capacity).map(Cell::new).collect())
        } else {
            Rc::new(Vec::new())
        };
        let virtual_to_slot: Rc<RefCell<FxHashMap<usize, usize>>> = if use_virtual {
            let m: FxHashMap<usize, usize> = (0..initial_pool).map(|i| (i, i)).collect();
            Rc::new(RefCell::new(m))
        } else {
            Rc::new(RefCell::new(FxHashMap::default()))
        };
        let pool_inactive: Rc<Vec<Rc<Cell<bool>>>> = if use_virtual {
            let pool_mgr = SlotPool::new(pool_capacity);
            // Slots beyond the initially-built rows are dormant until the
            // viewport tracker grows the element pool into them.
            for i in initial_pool..pool_capacity {
                pool_mgr.set_inactive(i, true);
            }
            Rc::new(pool_mgr.inactive_cells().to_vec())
        } else {
            Rc::new(Vec::new())
        };
        let _stv = slot_to_virtual.clone();
        let _pin = pool_inactive.clone();
        let current_len: Rc<Cell<usize>> = Rc::new(Cell::new(initial_len));
        // Per-slot disabled click-guard cache (visual domain — logic reads
        // the `disabled_rows` signal in data space; audit round 6).
        let disabled_cells: Rc<RefCell<Vec<Rc<Cell<bool>>>>> = Rc::new(RefCell::new(
            (0..initial_pool)
                .map(|_| Rc::new(Cell::new(false)))
                .collect(),
        ));

        // ═══ Virtual scroll reconcile (frame_tick) ═══
        // last_so is shared with the data subscribe: poking it to NaN forces
        // the next tick to re-reconcile every slot (data shrink/replace).
        let last_so: Rc<Cell<crate::style::Vec2>> = Rc::new(Cell::new(crate::style::Vec2::ZERO));
        if use_virtual {
            let so = scroll_offset.clone();
            let last_so = last_so.clone();
            let sig_read = rows_sig.clone();
            let sid_tick = scroll_id;
            let rh = row_h;
            let stv_clone = slot_to_virtual.clone();
            let vts_clone = virtual_to_slot.clone();
            let pin_clone = pool_inactive.clone();
            let rs = rows_state.clone();
            let cl = current_len.clone();
            let rfns = render_fns.clone();
            let nc = num_cols;
            let bid = body_id;
            let hf = self.footer_texts.is_some();
            let striped_tick = is_striped;
            let even_bg_tick = even_bg;
            let cb_off_tick = if has_cb { 1usize } else { 0 };
            let cb_sig_tick = cb_sig.clone();
            let sel_sig_tick = sel_sig.clone();
            let dis_sig_tick = self.disabled_rows.clone();
            let dis_cells_tick = Rc::clone(&disabled_cells);
            let Some(el) = ctx.arena.get_mut(scroll_id) else {
                return container_id;
            };
            el.set_frame_tick(Box::new(move || {
                let new_so = so.get();
                let old_so = last_so.get();
                // NaN sentinel (poked by the data subscribe): forces a full
                // re-reconcile — NaN != NaN so the guard passes, and `forced`
                // disables the per-slot unchanged-vi skip below.
                let forced = old_so.y.is_nan();
                if new_so == old_so { return; }
                last_so.set(new_so);
                // Ring modulus = the number of BUILT rows (the bookkeeping
                // arrays are reserved up to pool_capacity; elements grow on
                // demand via the viewport tracker).
                let pool_sz = rs.borrow().rows.len();
                if pool_sz == 0 { return; }
                // Ring reuse: slot pi hosts the unique vi ∈ [first, first+pool)
                // with vi ≡ pi (mod pool). Scrolling by k rows re-contents only
                // k slots (audit follow-up #3: remap was O(pool) per frame).
                // Borrow the rows in place — `read()` would clone the whole Vec.
                let (cnt, first, changes) = sig_read.with(|data| {
                    let cnt = data.len();
                    if cnt == 0 {
                        return (0, 0, Vec::new());
                    }
                    let first = (new_so.y / rh).max(0.0) as usize;
                    let first = first.min(cnt.saturating_sub(pool_sz));

                    // Collect only slots whose assigned virtual index changed.
                    let mut changes: Vec<(usize, usize, Vec<String>)> = Vec::new();
                    for pi in 0..pool_sz {
                        let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                        if !forced && stv_clone[pi].get() == vi {
                            continue;
                        }
                        let mut row_texts = Vec::with_capacity(nc);
                        if vi < cnt {
                            if let Some(item) = data.get(vi) {
                                for ci in 0..nc {
                                    let t = if let Some(ref f) = rfns[ci] {
                                        f(item, vi, ci)
                                    } else {
                                        String::new()
                                    };
                                    row_texts.push(t);
                                }
                            }
                        }
                        changes.push((pi, vi, row_texts));
                    }
                    (cnt, first, changes)
                });
                if cnt == 0 { return; }
                // Any active/inactive flip pending (data edge)?
                let any_flip = (0..pool_sz).any(|pi| {
                    let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                    pin_clone[pi].get() == (vi < cnt)
                });
                if changes.is_empty() && !any_flip { return; }

                let stv2 = stv_clone.clone();
                let vts2 = vts_clone.clone();
                let pin2 = pin_clone.clone();
                let rs2 = rs.clone();
                let cl2 = cl.clone();
                let rh2 = rh;
                let bid2 = bid;
                let sid2 = sid_tick;
                let hf2 = hf;
                let striped2 = striped_tick;
                let even_bg2 = even_bg_tick;
                let cbo2 = cb_off_tick;
                let cb_sig2 = cb_sig_tick.clone();
                let sel_sig2 = sel_sig_tick.clone();
                let dis_sig2 = dis_sig_tick.clone();
                let dc2 = Rc::clone(&dis_cells_tick);

                defer_action(move |arena: &mut crate::core::element::ElementArena, _root_id: ElementId, _reg: &mut EventRegistry| {
                    let state = rs2.borrow();
                    // Snapshot the multi-select set once for glyph refresh.
                    let cb_set = cb_sig2.as_ref().map(|s| s.read());
                    // Slot visual state is re-derived per new virtual index
                    // (audit round 6): CHECKED (single + multi selection) and
                    // DISABLED would otherwise stick to the pool slot and
                    // travel with it while scrolling.
                    let sel_now = sel_sig2.read();
                    let dis_set = dis_sig2.as_ref().map(|d| d.read());
                    // Structural rebuild is only needed when the active slot
                    // set changes (pool edge / data shrink); mid-scroll remaps
                    // are content-only.
                    let mut active_set_changed = false;
                    for pi in 0..pool_sz {
                        let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                        let active = vi < cl2.get();
                        if pin2[pi].get() == active {
                            pin2[pi].set(!active);
                            active_set_changed = true;
                        }
                    }
                    for (pi, vi, texts) in &changes {
                        let (pi, vi) = (*pi, *vi);
                        // Retire the old virtual→slot mapping before rebinding.
                        let old_vi = stv2[pi].get();
                        {
                            let mut m = vts2.borrow_mut();
                            if m.get(&old_vi) == Some(&pi) {
                                m.remove(&old_vi);
                            }
                        }
                        stv2[pi].set(vi);
                        if vi >= cl2.get() {
                            continue; // slot deactivated above
                        }
                        vts2.borrow_mut().insert(vi, pi);
                        if let Some(row) = state.rows.get(pi) {
                            // Data-column texts start after the checkbox cell
                            // (when present) — align via the grid offset, like
                            // the data subscribe does. Zipping cell_states
                            // directly would overwrite the checkbox glyph
                            // with the first data column.
                            for (ci, t) in texts.iter().enumerate() {
                                if let Some(cs) = row.cell_states.get(ci + cbo2) {
                                    cs.set_text(t);
                                }
                            }
                            // Checkbox glyph follows the VIRTUAL index — the
                            // slot now shows a different data row, whose
                            // checked state may differ (follow-up: selection
                            // rendered stale after scrolling).
                            if cbo2 == 1 {
                                if let (Some(cb), Some(set)) = (row.cell_states.first(), cb_set.as_ref()) {
                                    cb.set_text(if set.contains(&vi) { "\u{2611}" } else { "\u{2610}" });
                                }
                            }
                            // CHECKED background follows the VIRTUAL index
                            // (single selection + multi selection).
                            let checked = sel_now == Some(vi)
                                || cb_set.as_ref().is_some_and(|s| s.contains(&vi));
                            crate::core::dirty_registry::set_state(
                                row.eid, crate::core::config::StateFlags::CHECKED, checked,
                            );
                            // DISABLED follows the VIRTUAL index; keep the
                            // per-slot click-guard cache in sync.
                            if let Some(ref dis) = dis_set {
                                let is_dis = dis.contains(&vi);
                                if let Some(c) = dc2.borrow().get(pi) { c.set(is_dis); }
                                set_item_disabled(row.eid, is_dis);
                                for &cid in &row.cell_ids {
                                    set_item_disabled(cid, is_dis);
                                }
                            }
                            // Stripe follows the VIRTUAL index, not the slot
                            // (mount-time slot stripes inverted whenever
                            // `first` was odd).
                            if striped2 {
                                let bg = if vi % 2 == 1 { Some(even_bg2) } else { None };
                                let rid = row.eid;
                                crate::core::element::with_ct_mut(|ct| {
                                    ct.style.entry(rid).or_default().background = bg;
                                });
                            }
                            // Reposition the slot at its virtual content-space
                            // Y (VirtualSlotY → taffy absolute inset; contained
                            // at the scroll-container boundary, O(pool)).
                            let new_y = vi as f32 * rh2;
                            if let Some(el) = arena.get(row.eid) {
                                if let Some(vsy) = el.get_user_data::<crate::widgets::display::list::VirtualSlotY>() {
                                    if (vsy.0.get() - new_y).abs() > 0.01 {
                                        vsy.0.set(new_y);
                                        dirty_registry::mark_dirty(row.eid, DirtyFlags::REPOSITION);
                                        dirty_registry::register_dirty(row.eid, DirtyFlags::REPOSITION);
                                    }
                                }
                            }
                            for &cid in &row.cell_ids {
                                dirty_registry::mark_dirty(cid, DirtyFlags::REPAINT);
                                dirty_registry::register_dirty(cid, DirtyFlags::REPAINT);
                                dirty_registry::bump_subtree_gen(cid);
                            }
                            dirty_registry::mark_dirty(row.eid, DirtyFlags::REPAINT);
                            dirty_registry::register_dirty(row.eid, DirtyFlags::REPAINT);
                        }
                    }
                    drop(state);
                    if active_set_changed {
                        dirty_registry::mark_structurally_changed(bid2);
                    }
                    // Update VirtualContentBounds for scrollbar range
                    if let Some(el) = arena.get_mut(sid2) {
                        if let Some(vcb_cell) = el.get_user_data::<VirtualContentBounds>() {
                            let fh = if hf2 { rh2 } else { 0.0 };
                            vcb_cell.0.set(crate::style::Rect::new(0.0, 0.0, 0.0, cl2.get() as f32 * rh2 + fh));
                        }
                    }
                });
            }));
        }

        // ── Shared row styling bag for the build_data_row seam ──
        let row_style = RowStyle {
            grid_border,
            surface_bg: surf_bg,
            even_bg,
            foreground: fg_color,
            row_h,
        };

        // Pre-allocate rows
        {
            let mut state = rows_state.borrow_mut();
            let data = rows_sig.read();
            for ri in 0..initial_pool {
                let cell_texts: Vec<String> = (0..num_cols)
                    .map(|ci| {
                        render_fns
                            .get(ci)
                            .and_then(|f| f.as_ref())
                            .and_then(|f| data.get(ri).map(|item| f(item, ri, ci)))
                            .unwrap_or_default()
                    })
                    .collect();
                let cell_colors: Vec<Option<Color>> = (0..num_cols)
                    .map(|ci| {
                        color_fns
                            .get(ci)
                            .and_then(|f| f.as_ref())
                            .and_then(|f| data.get(ri).map(|item| f(item, ri, ci)))
                            .flatten()
                    })
                    .collect();
                let checked = cb_sig.as_ref().map(|s| s.read().contains(&ri));
                let overrides = RowOverrides {
                    hover_bg: Some(hover_bg),
                    pressed_bg: Some(pressed_bg),
                    checked_bg: Some(sel_bg),
                    focused_bg: Some(focus_bg),
                    striped_even: is_striped && ri % 2 == 1,
                };
                let parts = crate::widgets::display::table_row::build_data_row(
                    ctx.arena,
                    &cols,
                    RowKind::Body,
                    &row_style,
                    &cell_texts,
                    checked,
                    overrides,
                );
                // Apply per-cell color overrides
                for (ci, color_opt) in cell_colors.iter().enumerate() {
                    if let Some(color) = color_opt {
                        let cell_idx = if has_cb { ci + 1 } else { ci };
                        if let Some(cell_id) = parts.cell_ids.get(cell_idx) {
                            if let Some(el) = ctx.arena.get_mut(*cell_id) {
                                el.set_foreground(*color);
                            }
                        }
                    }
                }
                // Checkbox click wiring (cell_ids[0] when has_cb).
                if has_cb {
                    let cb_cid = parts.cell_ids[0];
                    let s = cb_sig.clone();
                    let pi = ri;
                    let stv_cb = slot_to_virtual.clone();
                    let uv_cb = use_virtual;
                    let dis_cb = disabled_cells.borrow().get(ri).cloned();
                    let cb_events = EventHandler::new().on_click(move || {
                        if let Some(ref c) = dis_cb {
                            if c.get() {
                                return;
                            }
                        }
                        let ir = if uv_cb { stv_cb[pi].get() } else { pi };
                        if let Some(ref s) = s {
                            let mut m = s.read().clone();
                            if m.contains(&ir) {
                                m.remove(&ir);
                            } else {
                                m.insert(ir);
                            }
                            s.set(m);
                        }
                    });
                    if let Some(reg) = ctx.event_registry.as_mut() {
                        cb_events.register_all(reg, cb_cid);
                    }
                }

                ctx.arena.add_child(body_id, parts.eid);
                if use_virtual {
                    let Some(el) = ctx.arena.get_mut(parts.eid) else {
                        return container_id;
                    };
                    // Pool slot: absolutely position the row at its virtual
                    // content-space Y (initially slot index == virtual index).
                    el.insert_user_data(crate::widgets::display::list::VirtualSlotY(Rc::new(
                        Cell::new(ri as f32 * row_h),
                    )));
                    // Share the pool's inactive cell so remap flips take
                    // effect on the element (taffy filtering + paint skip).
                    el.slot_inactive = pool_inactive[ri].clone();
                }
                state.rows.push(RowState {
                    eid: parts.eid,
                    cell_ids: parts.cell_ids,
                    cell_states: parts.cell_states,
                });
            }
        }

        // Hide excess rows (only for non-virtual mode)
        if !use_virtual {
            for ri in initial_len..initial_pool {
                let eid = rows_state.borrow().rows[ri].eid;
                if let Some(el) = ctx.arena.get_mut(eid) {
                    el.slot_inactive.set(true);
                }
            }
        }

        // ═══ Footer row ═══
        let footer_row_id: Rc<Cell<Option<ElementId>>> = Rc::new(Cell::new(None));
        // Virtual mode: footer's VirtualSlotY cell, updated when data length changes.
        let mut footer_slot_y: Option<Rc<Cell<f32>>> = None;
        if let Some(ref footer_sig) = self.footer_texts {
            let initial_texts: Vec<String> = {
                let texts0 = footer_sig.read();
                (0..num_cols)
                    .map(|ci| texts0.get(ci).cloned().unwrap_or_default())
                    .collect()
            };

            // Footer row — built as a flex row (like the header) so it never
            // depends on taffy grid-column placement, which is unreliable
            // during mark_structurally_changed rebuilds.
            let frow_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(frow_id) else {
                    return container_id;
                };
                el.set_accessible_role(accesskit::Role::Row);
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_preferred_height(row_h);
                el.set_flex_shrink(0.0);
                el.set_padding(Padding {
                    right: sb_gutter,
                    ..Padding::ZERO
                });
                el.set_background(surf_bg);
            }
            ctx.arena.add_child(body_id, frow_id);
            if use_virtual {
                // Footer sits at the end of the *virtual* content (after all
                // data rows), not after the pool slots.
                let vsy = Rc::new(Cell::new(initial_len as f32 * row_h));
                footer_slot_y = Some(vsy.clone());
                if let Some(el) = ctx.arena.get_mut(frow_id) {
                    el.insert_user_data(crate::widgets::display::list::VirtualSlotY(vsy));
                }
            }
            {
                let role = ComponentRole::Display(DisplayRole::Text {
                    foreground: ColorRole::OnSurface,
                });
                ctx.register_theme_component(
                    frow_id,
                    &theme.resolve_component(&role),
                    &role,
                    &self.style,
                );
            }

            // Checkbox column spacer — mirrors the body's checkbox track width
            if has_cb {
                let spacer_id = ctx.arena.allocate();
                {
                    let Some(el) = ctx.arena.get_mut(spacer_id) else {
                        return container_id;
                    };
                    el.set_preferred_width(Some(cols.cb_w));
                    el.set_preferred_height(row_h);
                    el.set_flex_shrink(0.0);
                    el.set_border_width(0.0);
                    el.set_accepts_mouse(false);
                }
                ctx.arena.add_child(frow_id, spacer_id);
            }

            let mut fcell_ids: Vec<ElementId> = Vec::with_capacity(num_cols);
            let mut fcell_states: Vec<TextCellState> = Vec::with_capacity(num_cols);

            for (dci, text) in initial_texts.iter().enumerate() {
                let cfg = &cols.cfgs[dci];
                let fw = cfg.font_weight + 200; // footer = bolder
                let (pref_w, flex_grow) = match cfg.spec {
                    ColumnWidth::Fixed(px) => (Some((px - 16.0).max(4.0)), 0.0),
                    ColumnWidth::Flex(f) => (Some(0.0), f),
                };
                let max_w = match cfg.spec {
                    ColumnWidth::Fixed(px) => Some((px - 16.0).max(4.0)),
                    ColumnWidth::Flex(_) => None,
                };
                let (cid, cs) = build_cell(
                    ctx.arena,
                    accesskit::Role::GridCell,
                    text,
                    cfg.font_size,
                    fw,
                    cfg.text_align,
                    fg_color,
                    grid_border,
                    row_h,
                    max_w,
                    false,
                    CellPlacement::Flex {
                        preferred_width: pref_w,
                        flex_grow,
                    },
                );
                ctx.arena.add_child(frow_id, cid);
                fcell_ids.push(cid);
                fcell_states.push(cs);
            }

            // Store for resize handler
            *footer_cell_ids_rc.borrow_mut() = fcell_ids;

            footer_row_id.set(Some(frow_id));

            // Footer text updates go through the cell TextCellStates.
            let footer_states: Rc<Vec<TextCellState>> = Rc::new(fcell_states);
            let fsig = footer_sig.clone();
            let fcells = footer_states.clone();
            crate::core::signal_bridge::subscribe_owned(container_id, footer_sig, move || {
                let texts = fsig.read();
                for (ci, cs) in fcells.iter().enumerate() {
                    cs.set_text(&texts.get(ci).cloned().unwrap_or_default());
                }
            });
        }

        // Focused cell tracking
        let focused_cell: Rc<Cell<(isize, usize)>> =
            Rc::new(Cell::new(if initial_len > 0 { (0, 0) } else { (-1, 0) }));
        // Anchor for Shift+Arrow range selection (-1 = no active range).
        let anchor_row: Rc<Cell<isize>> = Rc::new(Cell::new(-1));

        // ═══ Selection state ═══
        let initial_row_ids: Vec<ElementId> = {
            let state = rows_state.borrow();
            state.rows.iter().map(|r| r.eid).collect()
        };
        let sel_state: Rc<RefCell<SelectionBg>> =
            Rc::new(RefCell::new(SelectionBg::new(initial_row_ids)));

        // Wire initial row click events.
        // Hover is auto-managed by the framework (HOVERED flag + StateStyle).
        {
            let ms_click = self.multi_selection.clone();
            let anchor_click = anchor_row.clone();
            let dis_click_sig = self.disabled_rows.clone();
            for ri in 0..initial_pool {
                let pi = ri;
                let stv_c = slot_to_virtual.clone();
                let uv_c = use_virtual;
                let eid = rows_state.borrow().rows[ri].eid;
                let ss = sel_sig.clone();
                let os = on_sel.clone();
                let fc = focused_cell.clone();
                let st = rows_state.clone();
                let dis_cell = disabled_cells.borrow().get(ri).cloned();
                let ms = ms_click.clone();
                let ar = anchor_click.clone();
                let dn = dis_click_sig.clone();
                let cbo_click = if has_cb { 1 } else { 0 };
                let row_events = EventHandler::new().on_click_with_mods(move |mods: Modifiers| {
                    let ri = if uv_c { stv_c[pi].get() } else { pi };
                    if let Some(ref c) = dis_cell {
                        if c.get() {
                            return;
                        }
                    }
                    let (old_row, old_col) = fc.get();
                    if old_row >= 0 {
                        if let Some(r) = st.borrow().rows.get(old_row as usize) {
                            if let Some(&cid) = r.cell_ids.get(old_col + cbo_click) {
                                clear_cell_outline(cid);
                            }
                        }
                    }
                    fc.set((ri as isize, 0));

                    if let Some(ref ms_sig) = ms {
                        if mods.shift {
                            let a = if ar.get() < 0 { ri as isize } else { ar.get() };
                            ar.set(a);
                            let a = a.max(0) as usize;
                            let (lo, hi) = (a.min(ri), a.max(ri));
                            // Disabled test in DATA space (the slot cache is
                            // pool-sized; audit round 6 — indexing it with
                            // data indices panicked past the pool).
                            let dis = dn.as_ref().map(|d| d.read());
                            ms_sig.set(
                                (lo..=hi)
                                    .filter(|i| dis.as_ref().is_none_or(|d| !d.contains(i)))
                                    .collect(),
                            );
                            return;
                        }
                        if mods.ctrl {
                            let mut set = ms_sig.read().clone();
                            if set.contains(&ri) {
                                set.remove(&ri);
                            } else {
                                set.insert(ri);
                            }
                            ms_sig.set(set);
                            ar.set(ri as isize);
                            return;
                        }
                    }

                    ar.set(ri as isize);
                    ss.set(Some(ri));
                    if let Some(ref f) = os {
                        f(ri);
                    }
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    row_events.register_all(reg, eid);
                }
            }
        }

        // ═══ Data signal reactivity ═══
        {
            let rows_state = rows_state.clone();
            let cur_len = current_len.clone();
            let sel_state = sel_state.clone();
            let _col_configs = col_configs.clone();
            let render_fns = render_fns.clone();
            let row_h = row_h;
            let is_striped = is_striped;
            let _even_bg = even_bg;
            let hover_bg = hover_bg;
            let pressed_bg = pressed_bg;
            let _grid_border = grid_border;
            let body_id = body_id;
            let scroll_sub = scroll_id;
            let footer_sub = footer_row_id.clone();
            let num_cols = num_cols;
            let cols = cols.clone();
            let cb_sig = cb_sig.clone();
            let row_style = row_style;
            let uv = use_virtual;
            let stv_sub = slot_to_virtual.clone();
            let dis_sub = Rc::clone(&disabled_cells);
            let sel_sig_sub = sel_sig.clone();
            let on_sel_sub = on_sel.clone();
            let fc_sub = focused_cell.clone();
            let ar_sub = anchor_row.clone();
            let ms_sub = self.multi_selection.clone();
            let footer_vsy = footer_slot_y.clone();
            let last_so_sub = last_so.clone();

            crate::core::signal_bridge::subscribe_owned(
                container_id,
                &rows_sig.clone(),
                move || {
                    let data = rows_sig.read();
                    let new_len = data.len();
                    let cur_pool = {
                        let st = rows_state.borrow();
                        st.pool_size
                    };

                    // ── Phase 1: Deferred pool growth/shrink ──
                    if !uv {
                        if new_len > cur_pool {
                            let cols_g = cols.clone();
                            let cb_sig_g = cb_sig.clone();
                            let render_clones = render_fns.clone();
                            let rows_sig_clone = rows_sig.clone();
                            let body = body_id;
                            let pool_sz = cur_pool;
                            let striped = is_striped;
                            let h_bg = hover_bg;
                            let p_bg = pressed_bg;
                            let s_bg = sel_bg;
                            let f_bg = focus_bg;
                            let state_rc = rows_state.clone();
                            let sel_state = sel_state.clone();
                            let ncols = num_cols;
                            let scroll_g = scroll_sub;
                            let footer_g = footer_sub.clone();
                            let rh_g = row_h;
                            let dc_g = Rc::clone(&dis_sub);
                            let stv_g = stv_sub.clone();
                            let uv_g = uv;
                            let sel_sig_g = sel_sig_sub.clone();
                            let on_sel_g = on_sel_sub.clone();
                            let fc_g = fc_sub.clone();
                            let ar_g = ar_sub.clone();
                            let ms_g = ms_sub.clone();
                            let has_cb_g = cb_sig_g.is_some();
                            defer_action(move |arena, _root, reg| {
                                let mut st = state_rc.borrow_mut();

                                // Extend disabled cells
                                for _ in pool_sz..new_len {
                                    dc_g.borrow_mut().push(Rc::new(Cell::new(false)));
                                }

                                for ri in pool_sz..new_len {
                                    let cell_texts: Vec<String> = (0..ncols)
                                        .map(|ci| {
                                            render_clones
                                                .get(ci)
                                                .and_then(|f| f.as_ref())
                                                .and_then(|f| {
                                                    rows_sig_clone
                                                        .read()
                                                        .get(ri)
                                                        .map(|item| f(item, ri, ci))
                                                })
                                                .unwrap_or_default()
                                        })
                                        .collect();
                                    let checked = cb_sig_g.as_ref().map(|s| s.read().contains(&ri));
                                    let overrides = RowOverrides {
                                        hover_bg: Some(h_bg),
                                        pressed_bg: Some(p_bg),
                                        checked_bg: Some(s_bg),
                                        focused_bg: Some(f_bg),
                                        striped_even: striped && ri % 2 == 1,
                                    };
                                    let parts = crate::widgets::display::table_row::build_data_row(
                                        arena,
                                        &cols_g,
                                        RowKind::Body,
                                        &row_style,
                                        &cell_texts,
                                        checked,
                                        overrides,
                                    );
                                    {
                                        let Some(_el) = arena.get_mut(parts.eid) else {
                                            return;
                                        };
                                    }
                                    arena.add_child(body, parts.eid);
                                    st.rows.push(RowState {
                                        eid: parts.eid,
                                        cell_ids: parts.cell_ids,
                                        cell_states: parts.cell_states,
                                    });
                                }

                                // ── Register events for newly grown rows ──
                                // Hover is auto-managed by the framework.
                                let cbo = if has_cb_g { 1 } else { 0 };
                                for ri in pool_sz..new_len {
                                    let eid = st.rows[ri].eid;
                                    let pi = ri;

                                    // Checkbox click
                                    if has_cb_g {
                                        let cb_cid = st.rows[ri].cell_ids[0];
                                        let s = cb_sig_g.clone();
                                        let stv_cb = stv_g.clone();
                                        let uv_cb = uv_g;
                                        let dis_cb = dc_g.borrow()[ri].clone();
                                        let cb_events = EventHandler::new().on_click(move || {
                                            if dis_cb.get() {
                                                return;
                                            }
                                            let ir = if uv_cb { stv_cb[pi].get() } else { pi };
                                            if let Some(ref s) = s {
                                                let mut m = s.read().clone();
                                                if m.contains(&ir) {
                                                    m.remove(&ir);
                                                } else {
                                                    m.insert(ir);
                                                }
                                                s.set(m);
                                            }
                                        });
                                        cb_events.register_all(reg, cb_cid);
                                    }

                                    // Row click for selection
                                    {
                                        let stv_c = stv_g.clone();
                                        let uv_c = uv_g;
                                        let ss_c = sel_sig_g.clone();
                                        let os_c = on_sel_g.clone();
                                        let fc_c = fc_g.clone();
                                        let st_c = state_rc.clone();
                                        let dis_cell = dc_g.borrow()[ri].clone();
                                        let ms_c = ms_g.clone();
                                        let ar_c = ar_g.clone();
                                        let dn_c = dc_g.clone();
                                        let row_events = EventHandler::new().on_click_with_mods(
                                            move |mods: Modifiers| {
                                                let ri = if uv_c { stv_c[pi].get() } else { pi };
                                                if dis_cell.get() {
                                                    return;
                                                }
                                                let (old_row, old_col) = fc_c.get();
                                                if old_row >= 0 {
                                                    if let Some(r) =
                                                        st_c.borrow().rows.get(old_row as usize)
                                                    {
                                                        if let Some(&cid) =
                                                            r.cell_ids.get(old_col + cbo)
                                                        {
                                                            clear_cell_outline(cid);
                                                        }
                                                    }
                                                }
                                                fc_c.set((ri as isize, 0));

                                                if let Some(ref ms_sig) = ms_c {
                                                    if mods.shift {
                                                        let a = if ar_c.get() < 0 {
                                                            ri as isize
                                                        } else {
                                                            ar_c.get()
                                                        };
                                                        ar_c.set(a);
                                                        let a = a.max(0) as usize;
                                                        let (lo, hi) = (a.min(ri), a.max(ri));
                                                        ms_sig.set(
                                                            (lo..=hi)
                                                                .filter(|&i| {
                                                                    !dn_c.borrow()[i].get()
                                                                })
                                                                .collect(),
                                                        );
                                                        return;
                                                    }
                                                    if mods.ctrl {
                                                        let mut set = ms_sig.read().clone();
                                                        if set.contains(&ri) {
                                                            set.remove(&ri);
                                                        } else {
                                                            set.insert(ri);
                                                        }
                                                        ms_sig.set(set);
                                                        ar_c.set(ri as isize);
                                                        return;
                                                    }
                                                }

                                                ar_c.set(ri as isize);
                                                ss_c.set(Some(ri));
                                                if let Some(ref f) = os_c {
                                                    f(ri);
                                                }
                                            },
                                        );
                                        row_events.register_all(reg, eid);
                                    }
                                }

                                // Rebuild sel_state with enlarged pool
                                let all_ids: Vec<ElementId> =
                                    st.rows.iter().map(|r| r.eid).collect();
                                *sel_state.borrow_mut() = SelectionBg::new(all_ids);
                                st.pool_size = new_len;

                                // Move footer to end of children list (new rows appended after it)
                                if let Some(fid) = footer_g.get() {
                                    if let Some(body_el) = arena.get_mut(body) {
                                        if let Some(pos) =
                                            body_el.children.iter().position(|&id| id == fid)
                                        {
                                            let f = body_el.children.remove(pos);
                                            body_el.children.push(f);
                                            body_el.sorted_children.borrow_mut().take();
                                        }
                                    }
                                }
                                // Update VirtualContentBounds for scrollbar range
                                if let Some(el) = arena.get_mut(scroll_g) {
                                    if let Some(vcb) = el.get_user_data::<VirtualContentBounds>() {
                                        let fh = if footer_g.get().is_some() { rh_g } else { 0.0 };
                                        vcb.0.set(crate::style::Rect::new(
                                            0.0,
                                            0.0,
                                            0.0,
                                            new_len as f32 * rh_g + fh,
                                        ));
                                    }
                                }

                                dirty_registry::mark_structurally_changed(body);
                                mark_a11y_dirty();
                            });
                        }

                        if new_len < cur_pool {
                            let state_rc = rows_state.clone();
                            let body = body_id;
                            let scroll_sh = scroll_sub;
                            let footer_sh = footer_sub.clone();
                            let rh_sh = row_h;
                            defer_action(move |arena, _root, _reg| {
                                let st = state_rc.borrow();
                                for ri in new_len..cur_pool {
                                    if let Some(el) = arena.get_mut(st.rows[ri].eid) {
                                        el.slot_inactive.set(true);
                                    }
                                }
                                drop(st);
                                let mut st = state_rc.borrow_mut();
                                st.pool_size = new_len;
                                // Update VirtualContentBounds for scrollbar range
                                if let Some(el) = arena.get_mut(scroll_sh) {
                                    if let Some(vcb) = el.get_user_data::<VirtualContentBounds>() {
                                        let fh = if footer_sh.get().is_some() {
                                            rh_sh
                                        } else {
                                            0.0
                                        };
                                        vcb.0.set(crate::style::Rect::new(
                                            0.0,
                                            0.0,
                                            0.0,
                                            new_len as f32 * rh_sh + fh,
                                        ));
                                    }
                                }
                                dirty_registry::mark_structurally_changed(body);
                                mark_a11y_dirty();
                            });
                        }
                    }

                    // ── Phase 2: Update text in overlapping region ──
                    // Virtual mode needs no explicit text pass here: the NaN
                    // sentinel below forces the remap tick to re-reconcile every
                    // slot (with equal-text early-outs), covering both in-place
                    // replacement and window shifts.
                    if !uv {
                        // Non-virtual mode: existing code
                        let visible = new_len.min(cur_pool);
                        let st = rows_state.borrow();
                        for ri in 0..visible {
                            if let Some(item) = data.get(ri) {
                                if let Some(row_state) = st.rows.get(ri) {
                                    for ci in 0..num_cols {
                                        let text = render_fns
                                            .get(ci)
                                            .and_then(|f| f.as_ref().map(|f| f(item, ri, ci)))
                                            .unwrap_or_default();
                                        let grid_ci = cols.grid_off(ci) as usize;
                                        if let Some(cs) = row_state.cell_states.get(grid_ci) {
                                            cs.set_text(&text);
                                        }
                                        if let Some(&eid) = row_state.cell_ids.get(grid_ci) {
                                            crate::core::element::with_ct_mut(|ct| {
                                                if let Some(a11y) = ct.a11y.get_mut(&eid) {
                                                    a11y.accessible_label = Some(text);
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Virtual mode: keep scrollbar range + footer position in
                    // sync with the new data length here — the remap tick only
                    // runs when the scroll offset changes.
                    if uv {
                        // Force the next tick to re-reconcile every slot: data may
                        // have shrunk below the current window (stale slots must
                        // deactivate) or been replaced in place (same-vi texts).
                        last_so_sub.set(crate::style::Vec2::new(f32::NAN, f32::NAN));
                        let scroll_v = scroll_sub;
                        let footer_v = footer_sub.clone();
                        let fvsy = footer_vsy.clone();
                        let rh_v = row_h;
                        defer_action(move |arena, _root, _reg| {
                            if let Some(el) = arena.get_mut(scroll_v) {
                                if let Some(vcb) = el.get_user_data::<VirtualContentBounds>() {
                                    let fh = if footer_v.get().is_some() { rh_v } else { 0.0 };
                                    vcb.0.set(crate::style::Rect::new(
                                        0.0,
                                        0.0,
                                        0.0,
                                        new_len as f32 * rh_v + fh,
                                    ));
                                }
                            }
                            if let (Some(vsy), Some(fid)) = (fvsy.as_ref(), footer_v.get()) {
                                let ny = new_len as f32 * rh_v;
                                if (vsy.get() - ny).abs() > 0.01 {
                                    vsy.set(ny);
                                    dirty_registry::mark_dirty(fid, DirtyFlags::REPOSITION);
                                    dirty_registry::register_dirty(fid, DirtyFlags::REPOSITION);
                                }
                            }
                            // Clamp the scroll offset to the shrunken content so
                            // the viewport doesn't hang past the end of the data.
                            if let Some(el) = arena.get(scroll_v) {
                                if let Some(bref) = el.get_user_data::<crate::widgets::bundle::scroll::ScrollBundleRef>() {
                                let fh = if footer_v.get().is_some() { rh_v } else { 0.0 };
                                let vp_h = el.screen_bounds.height;
                                let max_y = (new_len as f32 * rh_v + fh - vp_h).max(0.0);
                                let cur = bref.0.scroll_offset.get();
                                if cur.y > max_y {
                                    bref.0.apply_offset(crate::style::Vec2::new(cur.x, max_y));
                                }
                            }
                            }
                        });
                    }

                    cur_len.set(new_len);
                },
            );
        }

        // ═══ Selection signal ═══
        {
            let sel_state = sel_state.clone();
            let fc = focused_cell.clone();
            let sr = sel_sig.clone();
            let rows_state = rows_state.clone();
            let _stv_sel = slot_to_virtual.clone();
            let vts_sel = virtual_to_slot.clone();
            let uv_sel = use_virtual;
            let cbo_ss = if has_cb { 1 } else { 0 };
            crate::core::signal_bridge::subscribe_owned(container_id, &sel_sig, move || {
                let sel = sr.read();
                let cur_pos = fc.get();
                if let Some(idx) = sel {
                    // CHECKED is slot-domain; `idx` is a data index — map it
                    // (audit round 6). Off-window rows simply show no
                    // highlight until the remap re-derives per slot.
                    if uv_sel {
                        let sel_slot = data_to_slot(&vts_sel, idx);
                        sel_state.borrow().sync_by(|pi| Some(pi) == sel_slot);
                    } else {
                        sel_state.borrow().set_selected(idx);
                    }
                }
                // aria-activedescendant
                let pos = if cur_pos.0 >= 0 {
                    cur_pos
                } else {
                    (sel.unwrap_or(0) as isize, 0)
                };
                let st = rows_state.borrow();
                let row_slot = if uv_sel {
                    data_to_slot(&vts_sel, pos.0.max(0) as usize).unwrap_or(usize::MAX)
                } else {
                    pos.0.max(0) as usize
                };
                let active = if pos.0 >= 0 {
                    st.rows
                        .get(row_slot)
                        .and_then(|r| r.cell_ids.get(pos.1 + cbo_ss))
                        .copied()
                } else {
                    None
                };
                drop(st);
                crate::core::element::with_ct_mut(|ct| {
                    if let Some(a11y) = ct.a11y.get_mut(&container_id) {
                        a11y.accessible_active_descendant = active;
                    }
                });
                crate::ecs::mark_a11y_changed(container_id);
                mark_a11y_dirty();
            });
        }
        // Multi-selection visual sync + header select-all
        if let Some(ref ms_sig) = self.multi_selection {
            // Header checkbox click = select-all / clear-all.
            //
            // Disabled-ness is judged in DATA space via the `disabled_rows`
            // signal (the SSOT). `disabled_cells` is a pool-slot-sized visual
            // cache — indexing it with data indices panicked as soon as the
            // data outgrew the pool (virtual table: len 20, index 20).
            if let Some((cb_hid, _)) = cb_header.as_ref() {
                let s = ms_sig.clone();
                let cl = current_len.clone();
                let dis_hdr = self.disabled_rows.clone();
                let header_cb_events = EventHandler::new().on_click(move || {
                    let n = cl.get();
                    if n == 0 {
                        return;
                    }
                    let disabled = dis_hdr.as_ref().map(|d| d.read()).unwrap_or_default();
                    let all = {
                        let cur = s.read();
                        (0..n).all(|i| disabled.contains(&i) || cur.contains(&i))
                    };
                    if all {
                        s.set(std::collections::HashSet::new());
                    } else {
                        s.set((0..n).filter(|i| !disabled.contains(i)).collect());
                    }
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    header_cb_events.register_all(reg, *cb_hid);
                }
            }

            let ms_state = sel_state.clone();
            let ms_read = ms_sig.clone();
            let ms_rows = rows_state.clone();
            let ms_len = current_len.clone();
            let ms_has_cb = has_cb;
            let ms_cb_header = cb_header.as_ref().map(|(_, ts)| ts.clone());
            let stv_ms = slot_to_virtual.clone();
            let uv_ms = use_virtual;
            crate::core::signal_bridge::subscribe_owned(container_id, ms_sig, move || {
                let ms = ms_read.read();
                // CHECKED is slot-domain; the selection set holds DATA
                // indices — translate per slot in virtual mode (audit
                // round 6: plain sync compared slot indices against the
                // data-index set, mis-highlighting after scroll).
                if uv_ms {
                    ms_state
                        .borrow()
                        .sync_by(|pi| stv_ms.get(pi).is_some_and(|c| ms.contains(&c.get())));
                } else {
                    ms_state.borrow().sync(&ms);
                }
                // Per-row checkbox glyph (cell_states[0] when has_cb).
                if ms_has_cb {
                    let st = ms_rows.borrow();
                    let n = ms_len.get().min(st.rows.len());
                    for (pi, row) in st.rows.iter().enumerate().take(n) {
                        if let Some(cb) = row.cell_states.first() {
                            let di = if uv_ms { stv_ms[pi].get() } else { pi };
                            cb.set_text(if ms.contains(&di) {
                                "\u{2611}"
                            } else {
                                "\u{2610}"
                            });
                        }
                    }
                }
                // Header checkbox tri-state: all -> ☑, none -> ☐, some -> ▣.
                if let Some(ref hdr) = ms_cb_header {
                    let n = ms_len.get();
                    let in_range = (0..n).filter(|i| ms.contains(i)).count();
                    let glyph = if n > 0 && in_range == n {
                        "\u{2611}"
                    } else if in_range == 0 {
                        "\u{2610}"
                    } else {
                        "\u{25A3}"
                    };
                    hdr.set_text(glyph);
                }
            });
        }

        // ═══ Disabled rows ═══
        if let Some(ref dis_sig) = self.disabled_rows {
            // M3: disabled foreground is auto-managed via StateStyle (DISABLED priority).
            // The widget should set ss.disabled.foreground on cells if needed.
            // Synchronous initial state
            let init_dis = dis_sig.read();
            for (i, row) in rows_state.borrow().rows.iter().enumerate() {
                if i >= current_len.get() {
                    break;
                }
                if init_dis.contains(&i) {
                    disabled_cells.borrow()[i].set(true);
                    set_item_disabled(row.eid, true);
                    for &cid in &row.cell_ids {
                        set_item_disabled(cid, true);
                    }
                }
            }
            // Subscribe to future changes
            let cells = Rc::clone(&disabled_cells);
            let rows_state = rows_state.clone();
            let dis_read = dis_sig.clone();
            let num_rows = current_len.clone();
            let stv_dis = slot_to_virtual.clone();
            let uv_dis = use_virtual;
            crate::core::signal_bridge::subscribe_owned(container_id, dis_sig, move || {
                let dis_set = dis_read.read();
                let state = rows_state.borrow();
                for (i, row) in state.rows.iter().enumerate() {
                    if i >= num_rows.get() {
                        break;
                    }
                    let di = if uv_dis { stv_dis[i].get() } else { i };
                    let is_dis = dis_set.contains(&di);
                    if cells.borrow()[i].get() != is_dis {
                        cells.borrow()[i].set(is_dis);
                        set_item_disabled(row.eid, is_dis);
                        for &cid in &row.cell_ids {
                            set_item_disabled(cid, is_dis);
                            dirty_registry::mark_dirty(cid, DirtyFlags::REPAINT);
                            dirty_registry::register_dirty(cid, DirtyFlags::REPAINT);
                        }
                    }
                }
            });
        }

        // ═══ Viewport height tracker + virtual pool growth ═══
        // The viewport is unknown at mount, so the element pool starts at
        // `virtual_threshold` rows and grows here on demand (bookkeeping
        // arrays are pre-reserved up to pool_capacity). Newly built rows get
        // empty texts + dormant slots; the NaN sentinel forces the remap tick
        // to assign content, position and activation on the next frame.
        let viewport_h: Rc<Cell<f32>> = Rc::new(Cell::new(row_h * 10.0));
        {
            let vh = viewport_h.clone();
            let sid = bundle.container_id;
            // Growth captures (virtual mode only).
            let grow_state = rows_state.clone();
            let grow_cl = current_len.clone();
            let grow_cols = cols.clone();
            let grow_style = row_style;
            let grow_body = body_id;
            let grow_pin = pool_inactive.clone();
            let grow_stv = slot_to_virtual.clone();
            let grow_dc = Rc::clone(&disabled_cells);
            let grow_sel_state = sel_state.clone();
            let grow_sel_sig = sel_sig.clone();
            let grow_on_sel = on_sel.clone();
            let grow_fc = focused_cell.clone();
            let grow_ar = anchor_row.clone();
            let grow_ms = self.multi_selection.clone();
            let grow_cb_sig = cb_sig.clone();
            let grow_has_cb = has_cb;
            let grow_footer = footer_row_id.clone();
            let grow_last_so = last_so.clone();
            let grow_rh = row_h;
            let grow_nc = num_cols;
            let grow_uv = use_virtual;
            let grow_hover = hover_bg;
            let grow_pressed = pressed_bg;
            let grow_sel_bg = sel_bg;
            let grow_focus = focus_bg;
            let growing: Rc<Cell<bool>> = Rc::new(Cell::new(false));
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_frame_tick(Box::new(move || {
                let Some(rect) = dirty_registry::bounds_of(sid) else {
                    return;
                };
                if rect.height <= 0.0 {
                    return;
                }
                vh.set(rect.height);
                if !grow_uv || growing.get() {
                    return;
                }
                let built = grow_state.borrow().rows.len();
                let needed = ((rect.height / grow_rh).ceil() as usize + 2)
                    .min(grow_cl.get())
                    .min(grow_pin.len());
                if needed <= built {
                    return;
                }
                growing.set(true);
                let state_rc = grow_state.clone();
                let cols_g = grow_cols.clone();
                let style_g = grow_style;
                let body_g = grow_body;
                let pin_g = grow_pin.clone();
                let stv_g = grow_stv.clone();
                let dc_g = Rc::clone(&grow_dc);
                let sel_state_g = grow_sel_state.clone();
                let ss_g = grow_sel_sig.clone();
                let os_g = grow_on_sel.clone();
                let fc_g = grow_fc.clone();
                let ar_g = grow_ar.clone();
                let ms_g = grow_ms.clone();
                let cb_sig_g = grow_cb_sig.clone();
                let has_cb_g = grow_has_cb;
                let footer_g = grow_footer.clone();
                let last_so_g = grow_last_so.clone();
                let rh_g = grow_rh;
                let nc_g = grow_nc;
                let growing_g = growing.clone();
                defer_action(move |arena, _root, reg| {
                    growing_g.set(false);
                    let mut st = state_rc.borrow_mut();
                    let start = st.rows.len();
                    if needed <= start {
                        return;
                    }
                    while dc_g.borrow().len() < needed {
                        dc_g.borrow_mut().push(Rc::new(Cell::new(false)));
                    }
                    let empty_texts: Vec<String> = vec![String::new(); nc_g];
                    let cbo = if has_cb_g { 1 } else { 0 };
                    for pi in start..needed {
                        let checked = if has_cb_g { Some(false) } else { None };
                        let overrides = RowOverrides {
                            hover_bg: Some(grow_hover),
                            pressed_bg: Some(grow_pressed),
                            checked_bg: Some(grow_sel_bg),
                            focused_bg: Some(grow_focus),
                            striped_even: false,
                        };
                        let parts = crate::widgets::display::table_row::build_data_row(
                            arena,
                            &cols_g,
                            RowKind::Body,
                            &style_g,
                            &empty_texts,
                            checked,
                            overrides,
                        );
                        {
                            let Some(el) = arena.get_mut(parts.eid) else {
                                return;
                            };
                            el.insert_user_data(crate::widgets::display::list::VirtualSlotY(
                                Rc::new(Cell::new(pi as f32 * rh_g)),
                            ));
                            el.slot_inactive = pin_g[pi].clone();
                        }
                        arena.add_child(body_g, parts.eid);

                        // ── Wire events (mirrors the initial-pool wiring) ──
                        if has_cb_g {
                            if let Some(&cb_cid) = parts.cell_ids.first() {
                                let s = cb_sig_g.clone();
                                let stv_cb = stv_g.clone();
                                let dis_cb = dc_g.borrow().get(pi).cloned();
                                let cb_events = EventHandler::new().on_click(move || {
                                    if let Some(ref c) = dis_cb {
                                        if c.get() {
                                            return;
                                        }
                                    }
                                    let ir = stv_cb[pi].get();
                                    if let Some(ref s) = s {
                                        let mut m = s.read().clone();
                                        if m.contains(&ir) {
                                            m.remove(&ir);
                                        } else {
                                            m.insert(ir);
                                        }
                                        s.set(m);
                                    }
                                });
                                cb_events.register_all(reg, cb_cid);
                            }
                        }
                        {
                            let stv_c = stv_g.clone();
                            let ss_c = ss_g.clone();
                            let os_c = os_g.clone();
                            let fc_c = fc_g.clone();
                            let st_c = state_rc.clone();
                            let dis_cell = dc_g.borrow().get(pi).cloned();
                            let ms_c = ms_g.clone();
                            let ar_c = ar_g.clone();
                            let dn_c = Rc::clone(&dc_g);
                            let row_events =
                                EventHandler::new().on_click_with_mods(move |mods: Modifiers| {
                                    let ri = stv_c[pi].get();
                                    if let Some(ref c) = dis_cell {
                                        if c.get() {
                                            return;
                                        }
                                    }
                                    let (old_row, old_col) = fc_c.get();
                                    if old_row >= 0 {
                                        if let Some(r) = st_c.borrow().rows.get(old_row as usize) {
                                            if let Some(&cid) = r.cell_ids.get(old_col + cbo) {
                                                clear_cell_outline(cid);
                                            }
                                        }
                                    }
                                    fc_c.set((ri as isize, 0));
                                    if let Some(ref ms_sig) = ms_c {
                                        if mods.shift {
                                            let a = if ar_c.get() < 0 {
                                                ri as isize
                                            } else {
                                                ar_c.get()
                                            };
                                            ar_c.set(a);
                                            let a = a.max(0) as usize;
                                            let (lo, hi) = (a.min(ri), a.max(ri));
                                            ms_sig.set(
                                                (lo..=hi)
                                                    .filter(|&i| {
                                                        dn_c.borrow()
                                                            .get(i)
                                                            .is_none_or(|c| !c.get())
                                                    })
                                                    .collect(),
                                            );
                                            return;
                                        }
                                        if mods.ctrl {
                                            let mut set = ms_sig.read().clone();
                                            if set.contains(&ri) {
                                                set.remove(&ri);
                                            } else {
                                                set.insert(ri);
                                            }
                                            ms_sig.set(set);
                                            ar_c.set(ri as isize);
                                            return;
                                        }
                                    }
                                    ar_c.set(ri as isize);
                                    ss_c.set(Some(ri));
                                    if let Some(ref f) = os_c {
                                        f(ri);
                                    }
                                });
                            row_events.register_all(reg, parts.eid);
                        }

                        st.rows.push(RowState {
                            eid: parts.eid,
                            cell_ids: parts.cell_ids,
                            cell_states: parts.cell_states,
                        });
                    }
                    st.pool_size = needed;
                    let all_ids: Vec<ElementId> = st.rows.iter().map(|r| r.eid).collect();
                    drop(st);
                    *sel_state_g.borrow_mut() = SelectionBg::new(all_ids);
                    // Keep the footer after the appended rows in child order.
                    if let Some(fid) = footer_g.get() {
                        if let Some(body_el) = arena.get_mut(body_g) {
                            if let Some(pos) = body_el.children.iter().position(|&id| id == fid) {
                                let f = body_el.children.remove(pos);
                                body_el.children.push(f);
                                body_el.sorted_children.borrow_mut().take();
                            }
                        }
                    }
                    // New rows enter the taffy tree + remap assigns content.
                    dirty_registry::mark_structurally_changed(body_g);
                    last_so_g.set(crate::style::Vec2::new(f32::NAN, f32::NAN));
                    mark_a11y_dirty();
                });
            }));
        }

        // ═══ Keyboard navigation ═══
        {
            let ss = sel_sig.clone();
            let os2 = on_sel.clone();
            let ms_toggle = self.multi_selection.clone();
            let dis_sel_all = self.disabled_rows.clone();
            let fc = focused_cell.clone();
            let cur_len = current_len.clone();
            let num_cols = num_cols;
            let bundle_nav = bundle.clone();
            let vph = viewport_h.clone();
            let row_h = row_h;
            let fb = focus_color;
            let rows_state = rows_state.clone();
            let dis_nav_sig = self.disabled_rows.clone();
            let anchor_row = anchor_row.clone();

            let fi_sel = sel_sig.clone();
            let fi_fc = focused_cell.clone();
            let fi_state = sel_state.clone();
            let fi_rows = rows_state.clone();
            let fi_fb = focus_color;
            let _stv_fi = slot_to_virtual.clone();
            let _uv_fi = use_virtual;
            let cbo_fi = if has_cb { 1 } else { 0 };

            let fo_fc = focused_cell.clone();
            let fo_rows = rows_state.clone();
            let cbo_fo = if has_cb { 1 } else { 0 };

            let sel_state_nav = sel_state.clone();
            let _stv_nav = slot_to_virtual.clone();
            let vts_nav = virtual_to_slot.clone();
            let uv_nav = use_virtual;

            let container_events = EventHandler::new()
                .on_focus_in(move |reason: crate::event::FocusReason| {
                    if reason == crate::event::FocusReason::PointerClick {
                        return;
                    }
                    let (row, col) = fi_fc.get();
                    if row >= 0 {
                        let st = fi_rows.borrow();
                        if let Some(r) = st.rows.get(row as usize) {
                            if let Some(&eid) = r.cell_ids.get(col + cbo_fi) {
                                set_cell_outline(eid, fi_fb, 2.0);
                            }
                        }
                    }
                    let selected_opt = fi_sel.read();
                    let focused_row = fi_fc.get().0.max(0) as usize;
                    sync_list_selection_focus(&fi_state.borrow(), selected_opt, focused_row);
                })
                .on_focus_out(move |_reason: crate::event::FocusReason| {
                    let (row, col) = fo_fc.get();
                    let foc_row = row.max(0) as usize;
                    fo_fc.set((-1, 0));
                    let st = fo_rows.borrow();
                    if let Some(r) = st.rows.get(foc_row) {
                        if let Some(&eid) = r.cell_ids.get(col + cbo_fo) {
                            clear_cell_outline(eid);
                        }
                    }
                })
                .on_action(move |action: &Action| -> ActionOutcome {
                    let c = cur_len.get();
                    if c == 0 {
                        return ActionOutcome::Unhandled;
                    }
                    let (mut row, col) = fc.get();
                    if row < 0 {
                        row = 0;
                    }
                    let cur_row = row as usize;
                    let cbo_kb = if has_cb { 1 } else { 0 };

                    let move_focus = |old_row: usize,
                                      old_col: usize,
                                      new_row: isize,
                                      new_col: usize,
                                      st: &PoolState| {
                        let old_slot = if uv_nav {
                            data_to_slot(&vts_nav, old_row).unwrap_or(usize::MAX)
                        } else {
                            old_row
                        };
                        if let Some(r) = st.rows.get(old_slot) {
                            if let Some(&eid) = r.cell_ids.get(old_col + cbo_kb) {
                                clear_cell_outline(eid);
                            }
                        }
                        fc.set((new_row, new_col));
                        let slot_idx = if uv_nav {
                            data_to_slot(&vts_nav, new_row.max(0) as usize).unwrap_or(usize::MAX)
                        } else {
                            new_row.max(0) as usize
                        };
                        if let Some(r) = st.rows.get(slot_idx) {
                            if let Some(&eid) = r.cell_ids.get(new_col + cbo_kb) {
                                set_cell_outline(eid, fb, 2.0);
                                crate::core::element::with_ct_mut(|ct| {
                                    if let Some(a11y) = ct.a11y.get_mut(&container_id) {
                                        a11y.accessible_active_descendant = Some(eid);
                                    }
                                });
                                crate::ecs::mark_a11y_changed(container_id);
                            }
                        }
                    };

                    // Data-space disabled test — the pool cache (`disabled_cells`)
                    // is slot-sized; indexing it with data indices panicked in
                    // virtual mode (audit round 6, same family as the header
                    // select-all fix).
                    let is_row_disabled = |di: usize| -> bool {
                        dis_nav_sig.as_ref().is_some_and(|d| d.read().contains(&di))
                    };
                    // Data→slot translation for slot-domain visuals.
                    let nav_slot = |di: usize| -> usize {
                        if uv_nav {
                            data_to_slot(&vts_nav, di).unwrap_or(usize::MAX)
                        } else {
                            di
                        }
                    };

                    let range_or_anchor = |is_shift: bool, from: usize, to: usize| {
                        if let Some(ref ms) = ms_toggle {
                            if is_shift {
                                let a = if anchor_row.get() < 0 {
                                    from as isize
                                } else {
                                    anchor_row.get()
                                };
                                anchor_row.set(a);
                                let a = a.max(0) as usize;
                                let (lo, hi) = (a.min(to), a.max(to));
                                ms.set(
                                    (lo..=hi)
                                        .filter(|&i| !is_row_disabled(i))
                                        .collect::<std::collections::HashSet<usize>>(),
                                );
                            } else {
                                anchor_row.set(to as isize);
                            }
                        }
                    };

                    match action.kind {
                        ActionKind::MoveDown | ActionKind::MoveUp => {
                            match crate::widgets::shared::keyboard::row_nav(
                                action.kind,
                                c,
                                cur_row,
                                is_row_disabled,
                            ) {
                                crate::widgets::shared::keyboard::RowNavOutcome::Navigate(
                                    new_row,
                                ) => {
                                    let st = rows_state.borrow();
                                    move_focus(cur_row, col, new_row as isize, col, &st);
                                    drop(st);
                                    set_item_highlight(
                                        &sel_state_nav.borrow().ids,
                                        Some(nav_slot(cur_row)),
                                        nav_slot(new_row),
                                    );
                                    bundle_nav.scroll_to_row(new_row, row_h, vph.get().max(row_h));
                                    range_or_anchor(action.selection, cur_row, new_row);
                                    ActionOutcome::Consumed
                                }
                                _ => ActionOutcome::Unhandled,
                            }
                        }
                        ActionKind::MoveLeft => {
                            if col > 0 {
                                let st = rows_state.borrow();
                                move_focus(cur_row, col, row, col - 1, &st);
                                ActionOutcome::Consumed
                            } else {
                                ActionOutcome::Unhandled
                            }
                        }
                        ActionKind::MoveRight => {
                            if col + 1 < num_cols {
                                let st = rows_state.borrow();
                                move_focus(cur_row, col, row, col + 1, &st);
                                ActionOutcome::Consumed
                            } else {
                                ActionOutcome::Unhandled
                            }
                        }
                        ActionKind::MoveHome => {
                            let st = rows_state.borrow();
                            move_focus(cur_row, col, row, 0, &st);
                            ActionOutcome::Consumed
                        }
                        ActionKind::MoveEnd => {
                            let st = rows_state.borrow();
                            move_focus(cur_row, col, row, num_cols.saturating_sub(1), &st);
                            ActionOutcome::Consumed
                        }
                        ActionKind::MovePageDown => {
                            let page_size = (vph.get() / row_h).max(1.0) as usize;
                            let next =
                                ((cur_row + page_size).min(c.saturating_sub(1))).max(cur_row);
                            if next != cur_row {
                                let st = rows_state.borrow();
                                move_focus(cur_row, col, next as isize, col, &st);
                                drop(st);
                                {
                                    let sel_ref = sel_state_nav.borrow();
                                    set_item_highlight(
                                        &sel_ref.ids,
                                        Some(nav_slot(cur_row)),
                                        nav_slot(next),
                                    );
                                }
                                bundle_nav.scroll_to_row(next, row_h, vph.get().max(row_h));
                                range_or_anchor(action.selection, cur_row, next);
                            }
                            ActionOutcome::Consumed
                        }
                        ActionKind::MovePageUp => {
                            let page_size = (vph.get() / row_h).max(1.0) as usize;
                            let prev = cur_row.saturating_sub(page_size);
                            if prev != cur_row {
                                let st = rows_state.borrow();
                                move_focus(cur_row, col, prev as isize, col, &st);
                                drop(st);
                                {
                                    let sel_ref = sel_state_nav.borrow();
                                    set_item_highlight(
                                        &sel_ref.ids,
                                        Some(nav_slot(cur_row)),
                                        nav_slot(prev),
                                    );
                                }
                                bundle_nav.scroll_to_row(prev, row_h, vph.get().max(row_h));
                                range_or_anchor(action.selection, cur_row, prev);
                            }
                            ActionOutcome::Consumed
                        }
                        ActionKind::Activate => {
                            ss.set(Some(cur_row));
                            if let Some(ref ms) = ms_toggle {
                                let mut set = ms.read().clone();
                                if set.contains(&cur_row) {
                                    set.remove(&cur_row);
                                } else {
                                    set.insert(cur_row);
                                }
                                ms.set(set);
                            }
                            if let Some(ref f) = os2 {
                                f(cur_row);
                            }
                            ActionOutcome::Consumed
                        }
                        ActionKind::SelectAll => {
                            if let Some(ref ms) = ms_toggle {
                                // Same data-space semantics as the header
                                // select-all: disabled rows are skipped.
                                let disabled =
                                    dis_sel_all.as_ref().map(|d| d.read()).unwrap_or_default();
                                let all = {
                                    let cur = ms.read();
                                    c > 0
                                        && (0..c).all(|i| disabled.contains(&i) || cur.contains(&i))
                                };
                                if all {
                                    ms.set(std::collections::HashSet::new());
                                } else {
                                    ms.set((0..c).filter(|i| !disabled.contains(i)).collect());
                                }
                                ActionOutcome::Consumed
                            } else {
                                ActionOutcome::Unhandled
                            }
                        }
                        ActionKind::Cancel => {
                            ss.set(None);
                            if let Some(ref ms) = ms_toggle {
                                ms.set(std::collections::HashSet::new());
                            }
                            anchor_row.set(-1);
                            ActionOutcome::Consumed
                        }
                        _ => ActionOutcome::Unhandled,
                    }
                });
            if let Some(reg) = ctx.event_registry.as_mut() {
                container_events.register_all(reg, container_id);
            }
        }

        container_id
    }
}

// ═══════════════════════════════════════════════════════════════════
// Static helpers — row/cell construction now lives in `table_row.rs`.
// ═══════════════════════════════════════════════════════════════════

impl<T: Clone + 'static> std::fmt::Debug for Table<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field("columns", &self.columns.len())
            .finish_non_exhaustive()
    }
}
