use auralis_signal::Signal;
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::core::config::{ElementBuilder, EventHandler, LayoutConfig, PaintConfig};
use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::event::action::{Action, ActionOutcome};
use crate::event::EventRegistry;
use crate::event::FocusReason;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Color;
use crate::style::Dimension;
use crate::style::Point;
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
use crate::widgets::bundle::ScrollBundle;
use crate::widgets::shared::reorder::ReorderController;
use crate::widgets::shared::{
    row_nav, set_item_disabled, set_item_highlight, sync_list_selection_focus, RowNavOutcome,
    SelectionBg, SlotPool, TextCellState,
};

/// Stored in Element user_data so the paint code can compute accurate
/// content_bounds when the clip has `affected_by_child_size(false)`.
pub struct ListItemIds(pub Vec<ElementId>);

/// Stored in scroll container user_data for virtualized lists.
/// Informs the paint code of the true content size so scrollbars
/// render correctly even though only a subset of items exist in the arena.
pub struct VirtualContentBounds(pub std::cell::Cell<crate::style::Rect>);

/// Virtual-slot content-space Y for a pooled row/item element.
///
/// A virtualized pool recycles a fixed set of elements while the data window
/// scrolls. The element's *real* position in content space is
/// `virtual_index × row_height`, not its pool-slot order. `element_taffy_style`
/// reads this cell and absolutely positions the element at that Y, so layout
/// bounds, paint translation and spatial hit-testing all agree on a single
/// coordinate space (audit 2026-07-16, Layer 3: rows previously kept slot-space
/// bounds and became hit-test-dead after scrolling past one pool height).
pub struct VirtualSlotY(pub std::rc::Rc<std::cell::Cell<f32>>);

/// Controls how items inside the List receive keyboard focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemFocusMode {
    /// **Roving tabindex** (default — ARIA / Qt model).
    /// The List container is a single Tab stop. Arrow keys navigate items
    /// internally. Tab enters the container, Tab again exits it.
    RovingTabindex,
    /// **Tab-navigable** (Flutter model).
    /// Each item is independently focusable. Tab traverses through every
    /// item. Arrow keys also move focus via global directional navigation.
    TabNavigable,
}

pub struct List<T: Clone + 'static> {
    items: Signal<Vec<T>>,
    render_fn: Option<Rc<dyn Fn(&T, usize) -> String>>,
    item_height: f32,
    selected: Option<Signal<Option<usize>>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    selection_follows_focus: bool,
    item_focus_mode: ItemFocusMode,
    style: StyleRefinement,
    reserve: usize,
    disabled: bool,
    disabled_items: Option<Signal<HashSet<usize>>>,
    virtual_threshold: Option<usize>,
    reorderable: bool,
    on_reorder: Option<Rc<dyn Fn(usize, usize)>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Clone + 'static> List<T> {
    pub fn new(items: Signal<Vec<T>>) -> Self {
        Self {
            items,
            render_fn: None,
            item_height: 36.0,
            selected: None,
            on_select: None,
            selection_follows_focus: true,
            item_focus_mode: ItemFocusMode::RovingTabindex,
            style: StyleRefinement::default(),
            reserve: 0,
            disabled: false,
            disabled_items: None,
            virtual_threshold: None,
            reorderable: false,
            on_reorder: None,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn render(mut self, f: impl Fn(&T, usize) -> String + 'static) -> Self {
        self.render_fn = Some(Rc::new(f));
        self
    }

    pub fn item_height(mut self, h: f32) -> Self {
        self.item_height = h;
        self
    }

    pub fn selected(mut self, sig: Signal<Option<usize>>) -> Self {
        self.selected = Some(sig);
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    pub fn selection_follows_focus(mut self, v: bool) -> Self {
        self.selection_follows_focus = v;
        self
    }

    pub fn item_focus_mode(mut self, mode: ItemFocusMode) -> Self {
        self.item_focus_mode = mode;
        self
    }

    pub fn reserve(mut self, n: usize) -> Self {
        self.reserve = n;
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn disabled_items(mut self, sig: Signal<HashSet<usize>>) -> Self {
        self.disabled_items = Some(sig);
        self
    }

    /// Enable virtual scrolling when data exceeds this threshold.
    /// Default: auto (20 items). Set to 0 to always virtualize.
    pub fn virtual_threshold(mut self, n: usize) -> Self {
        self.virtual_threshold = Some(n);
        self
    }

    /// Enable drag-to-reorder for the list items.
    pub fn reorderable(mut self, reorder: bool) -> Self {
        self.reorderable = reorder;
        self
    }

    /// Callback when a drag-to-reorder completes: `on_reorder(src_index, dst_index)`.
    pub fn on_reorder(mut self, f: impl Fn(usize, usize) + 'static) -> Self {
        self.on_reorder = Some(Rc::new(f));
        self
    }
}

impl<T: Clone + 'static> Styled for List<T> {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: Clone + 'static> Widget for List<T> {
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
        let _component_mask = self.component_mask();
        let theme = ctx.theme;
        let render_fn = self.render_fn;
        let item_h = self.item_height;
        let gap = self.style.gap.unwrap_or(0.0);
        let items_sig = self.items.clone();
        let selected_sig = self.selected.clone().unwrap_or_else(|| Signal::new(None));
        let on_select = self.on_select;
        let selection_follows_focus = self.selection_follows_focus;
        let is_tab_navigable = self.item_focus_mode == ItemFocusMode::TabNavigable;
        let is_disabled = self.disabled;
        let disabled_items_sig = self.disabled_items.clone();

        let role = ComponentRole::Interactive(InteractiveRole::ListItem { selected: false });
        let resolved = match theme.resolve_component(&role) {
            ResolvedComponentStyle::ListItem(s) => s,
            _ => unreachable!(),
        };
        let item_bg = resolved.background;

        let items_snapshot: Vec<String> = {
            let data = items_sig.read();
            (0..data.len())
                .map(|i| {
                    if let Some(ref f) = render_fn {
                        f(&data[i], i)
                    } else {
                        format!("[{i}]")
                    }
                })
                .collect()
        };
        let num_items = items_snapshot.len();
        let total_slots = self.reserve.max(num_items);
        let threshold = self.virtual_threshold.unwrap_or(20);
        let use_virtual = num_items > threshold;
        // Virtual pool: size it to cover the tallest realistic viewport up
        // front (audit follow-up #1: a threshold-sized pool left the bottom
        // of taller viewports empty). Items are cheap single-text elements,
        // so eager build keeps List simple — Table grows lazily instead.
        const MAX_VIEWPORT_PX: f32 = 4320.0; // 8K portrait
        let pool = if use_virtual {
            let viewport_cover = (MAX_VIEWPORT_PX / item_h.max(1.0)).ceil() as usize + 2;
            threshold.max(viewport_cover).min(num_items)
        } else {
            total_slots
        };

        let focused_index: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let pool_mgr = SlotPool::new(pool);
        for i in 0..pool {
            pool_mgr.set_inactive(i, i >= num_items.min(pool));
        }
        let disabled_cells: Vec<Rc<Cell<bool>>> =
            (0..pool).map(|_| Rc::new(Cell::new(false))).collect();
        let position_offsets: Vec<Rc<Cell<crate::style::Vec2>>> = (0..pool)
            .map(|_| Rc::new(Cell::new(crate::style::Vec2::ZERO)))
            .collect();
        let slot_to_virtual: Rc<Vec<std::cell::Cell<usize>>> =
            Rc::new((0..pool).map(std::cell::Cell::new).collect());

        let extra_mask = components::STYLE
            | components::INTERACTION
            | components::TEXT
            | components::ACCESSIBLE
            | components::LIFECYCLE;
        let bundle = ScrollBundle::new(
            ctx,
            extra_mask,
            crate::widgets::layout::ScrollDirection::Vertical,
            12.0,
        );
        let id = bundle.container_id;
        let scroll_offset = bundle.scroll_offset.clone();
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_accessible_role(accesskit::Role::ListBox);
            if is_tab_navigable {
                // TabNavigable: items are individually focusable;
                // container is not a Tab stop.
                el.set_focusable(false);
            } else {
                // RovingTabindex: container is the single Tab stop.
                // The item's bg_override provides visual feedback, so the
                // container ring is hidden to avoid dual-ring confusion.
                el.set_focusable(true);
                el.set_outline_color(Color::TRANSPARENT);
            }
            if let Some(bg) = self.style.background {
                el.set_background(bg);
            }
            if let Some(w) = self.style.width {
                if let Dimension::Pixels(px) = w {
                    el.set_preferred_width(Some(px));
                }
            }
            if let Some(h) = self.style.height {
                if let Dimension::Pixels(px) = h {
                    el.set_preferred_height(px);
                }
            }
        }

        let vstack_id = ctx.arena.allocate();
        {
            let Some(vs) = ctx.arena.get_mut(vstack_id) else {
                return id;
            };
            vs.set_layout_direction(crate::core::LayoutDirection::Vertical);
            vs.set_affected_by_child_size(false);
            vs.set_flex_grow(1.0);
            if self.style.gap.is_some() {
                vs.set_gap(gap);
            }
        }
        ctx.arena.add_child(bundle.clip_id, vstack_id);

        let mut item_ids: Vec<ElementId> = Vec::with_capacity(pool);
        let mut lazy_labels: Vec<Rc<Cell<String>>> = Vec::with_capacity(pool);
        let mut lazy_gens: Vec<Rc<Cell<u64>>> = Vec::with_capacity(pool);

        for i in 0..pool {
            let text = items_snapshot.get(i).cloned().unwrap_or_default();
            let paint = PaintConfig {
                background: Some(item_bg),
                foreground: Some(resolved.foreground),
                font_size: resolved.font_size,
                font_weight: 400,
                corner_radius: crate::style::CornerRadii::all(4.0),
                text_align: crate::style::TextAlign::Start,
                ..PaintConfig::default()
            };
            let layout = LayoutConfig {
                width: Dimension::Percent(1.0),
                height: Dimension::Pixels(item_h),
                padding: crate::style::Padding::symmetric(8.0, 0.0),
                flex_shrink: 0.0,
                ..LayoutConfig::default()
            };
            let mut builder = ElementBuilder::new()
                .layout(layout)
                .paint(paint)
                .accessibility(accesskit::Role::ListBoxOption, String::new());
            if is_tab_navigable {
                builder = builder.interaction(crate::core::config::InteractionConfig {
                    focusable: true,
                    ..crate::core::config::InteractionConfig::default()
                });
            }
            let item_id = builder.build(ctx);

            let Some(el) = ctx.arena.get_mut(item_id) else {
                return id;
            };
            el.set_text_vertical_center(true);
            el.set_affected_by_child_size(false);
            el.with_state_style(|ss| {
                ss.hovered.background = Some(resolved.hover_bg);
                ss.pressed.background = Some(resolved.hover_bg);
                ss.checked.background = Some(resolved.selected_bg);
                ss.focused.background = Some(resolved.hover_bg);
                ss.disabled.foreground = Some(Color::rgba8(150, 150, 165, 200));
            });
            el.slot_inactive = pool_mgr.cell(i).clone();
            el.set_position_offset(position_offsets[i].clone());

            if i >= num_items {
                pool_mgr.set_inactive(i, true);
            }

            if is_tab_navigable {
                let fi = focused_index.clone();
                let idx = i;
                let focus_events = EventHandler::new().on_focus_in(move |_reason: FocusReason| {
                    fi.set(idx);
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    focus_events.register_all(reg, item_id);
                }
            }

            let fs = el.font_size();
            let lh = el.line_height();
            let fw = el.font_weight();
            let ff = el.font_family().map(|s| s.to_string());
            let ta = el.text_align();

            let buffer_max_width = None;

            let tcs = TextCellState::mount(
                item_id,
                el,
                &text,
                fs,
                lh,
                fw,
                ff.clone(),
                buffer_max_width,
                ta,
            );
            // Read the intrinsic width from the buffer TextCellState just
            // shaped (unconstrained, so single-line == intrinsic) instead of
            // shaping the same text a second time (audit 2026-07-17 round 2).
            let single_line_w = if text.is_empty() {
                Some(0.0)
            } else if matches!(
                ta,
                crate::style::TextAlign::Start | crate::style::TextAlign::Left
            ) {
                el.text_buffer().and_then(|b| {
                    crate::render::wgpu::glyphon_bridge::intrinsic_width_from_buffer(
                        &b.borrow(),
                        fs,
                    )
                })
            } else {
                None
            };
            let measured_w = single_line_w
                .unwrap_or_else(|| crate::render::text::measure_text_width(&text, fs, fw, ff))
                .max(fs * 2.0);
            el.set_measured_text_width(Rc::new(Cell::new(measured_w)));

            lazy_labels.push(tcs.lazy_label);
            lazy_gens.push(tcs.text_gen);
            item_ids.push(item_id);

            if use_virtual {
                // Pool slot: absolutely position the item at its virtual
                // content-space Y (initially slot index == virtual index).
                el.insert_user_data(VirtualSlotY(Rc::new(Cell::new(i as f32 * (item_h + gap)))));
            }

            ctx.arena.add_child(vstack_id, item_id);
        }
        let rdr_item_ids = item_ids.clone();
        let rdr_offsets = position_offsets.clone();
        let sel_state = Rc::new(SelectionBg::new(item_ids.clone()));

        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.insert_user_data(ListItemIds(item_ids.clone()));
            if use_virtual {
                el.insert_user_data(VirtualContentBounds(std::cell::Cell::new(
                    crate::style::Rect::new(0.0, 0.0, 0.0, num_items as f32 * (item_h + gap)),
                )));
            }
        }

        // Apply initial per-item disabled state
        if let Some(ref dis_sig) = disabled_items_sig {
            let init_dis = dis_sig.read();
            for (i, &eid) in item_ids.iter().enumerate() {
                let disabled = init_dis.contains(&i);
                disabled_cells[i].set(disabled);
                set_item_disabled(eid, disabled);
            }
        }

        // Save for virtual-scroll frame_tick which runs after pool_mgr is moved
        let pool_inactive: Vec<Rc<Cell<bool>>> = pool_mgr.inactive_cells().to_vec();
        let current_len: Rc<Cell<usize>> = Rc::new(Cell::new(num_items));
        // Shared with the virtual-scroll tick below: the data subscribe pokes
        // this to NaN to force a full slot re-reconcile (shrink/replace).
        let last_so = Rc::new(std::cell::Cell::new(crate::style::Vec2::ZERO));
        {
            let render = render_fn.clone();
            let sig_read = items_sig.clone();
            let labels = lazy_labels.clone();
            let gens = lazy_gens.clone();
            let ids = item_ids.clone();
            let pool_mgr = pool_mgr;
            let vstack = vstack_id;
            let cur_len = current_len.clone();
            let last_so_sub = last_so.clone();
            let pitch_sub = item_h + gap;
            let cid_sub = id;
            crate::core::signal_bridge::subscribe_owned(id, &items_sig, move || {
                if !use_virtual {
                    let data = sig_read.read();
                    let new_len = data.len();
                    let visible = new_len.min(pool_mgr.len());
                    let prev = cur_len.get();
                    if visible != prev {
                        pool_mgr.sync_visible(visible);
                        cur_len.set(visible);
                        crate::core::dirty_registry::mark_structurally_changed(vstack);
                        crate::core::dirty_registry::mark_a11y_dirty();
                    }
                    for i in 0..visible {
                        let text = if let Some(item_val) = data.get(i) {
                            if let Some(ref f) = render {
                                f(item_val, i)
                            } else {
                                format!("[{i}]")
                            }
                        } else {
                            String::new()
                        };
                        let a11y_label = text.clone();
                        labels[i].set(text);
                        gens[i].set(gens[i].get().wrapping_add(1));
                        if let Some(&eid) = ids.get(i) {
                            crate::core::dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
                            crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
                            crate::core::dirty_registry::bump_subtree_gen(eid);
                        }
                        let eid = ids[i];
                        crate::core::element::with_ct_mut(|ct| {
                            if let Some(a11y) = ct.a11y.get_mut(&eid) {
                                a11y.accessible_label = Some(a11y_label);
                            }
                        });
                    }
                } else {
                    // Virtual mode: data changed — force the next tick to
                    // re-reconcile every slot (texts, active flips, positions),
                    // sync the scrollbar range, and clamp the offset if the
                    // data shrank below the current window.
                    let new_len = sig_read.with(|d| d.len());
                    last_so_sub.set(crate::style::Vec2::new(f32::NAN, f32::NAN));
                    let pitch_v = pitch_sub;
                    let cid_v = cid_sub;
                    crate::core::dirty_registry::defer_action(move |arena, _root, _reg| {
                        if let Some(el) = arena.get_mut(cid_v) {
                            if let Some(vcb) = el.get_user_data::<VirtualContentBounds>() {
                                vcb.0.set(crate::style::Rect::new(
                                    0.0,
                                    0.0,
                                    0.0,
                                    new_len as f32 * pitch_v,
                                ));
                            }
                        }
                        if let Some(el) = arena.get(cid_v) {
                            if let Some(bref) = el
                                .get_user_data::<crate::widgets::bundle::scroll::ScrollBundleRef>()
                            {
                                let vp_h = el.screen_bounds.height;
                                let max_y = (new_len as f32 * pitch_v - vp_h).max(0.0);
                                let cur = bref.0.scroll_offset.get();
                                if cur.y > max_y {
                                    bref.0.apply_offset(crate::style::Vec2::new(cur.x, max_y));
                                }
                            }
                        }
                    });
                }
            });
        }

        {
            let sel_state = sel_state.clone();
            let sel_read = selected_sig.clone();
            let container_id = id;
            let stv_sel = slot_to_virtual.clone();
            let uv_sel = use_virtual;
            crate::core::signal_bridge::subscribe_owned(id, &selected_sig, move || {
                let sel = sel_read.read();
                if let Some(idx) = sel {
                    // CHECKED is slot-domain; `idx` is a data index — map it
                    // in virtual mode (audit round 6). Off-window rows show
                    // no highlight until the remap re-derives per slot.
                    if uv_sel {
                        sel_state.sync_by(|pi| stv_sel.get(pi).is_some_and(|c| c.get() == idx));
                    } else {
                        sel_state.set_selected(idx);
                    }
                }
                crate::core::dirty_registry::mark_a11y_dirty();
                crate::core::dirty_registry::mark_dirty(container_id, DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);
            });
        }

        // Per-item disabled signal subscription
        if let Some(ref dis_sig) = disabled_items_sig {
            let ids = item_ids.clone();
            let cells = disabled_cells.clone();
            let dis_read = dis_sig.clone();
            let stv_dis = slot_to_virtual.clone();
            let uv_dis = use_virtual;
            crate::core::signal_bridge::subscribe_owned(id, dis_sig, move || {
                let dis_set = dis_read.read();
                for (i, &eid) in ids.iter().enumerate() {
                    // The disabled set holds DATA indices; slot i hosts
                    // virtual index stv[i] (audit round 6 — the pool index
                    // was compared against the data set directly).
                    let di = if uv_dis {
                        stv_dis.get(i).map_or(i, |c| c.get())
                    } else {
                        i
                    };
                    let is_dis = dis_set.contains(&di);
                    if cells[i].get() != is_dis {
                        cells[i].set(is_dis);
                        set_item_disabled(eid, is_dis);
                        crate::core::dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
                        crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
                    }
                }
            });
        }

        // ── Per-item click events (unless disabled) ──
        if !is_disabled {
            for i in 0..pool {
                let sel = selected_sig.clone();
                let fi = focused_index.clone();
                let sel_state = sel_state.clone();
                let on_sel = on_select.clone();
                let dis_cell = disabled_cells[i].clone();
                let stv_local = slot_to_virtual.clone();
                let pool_idx = i;
                let item_events = EventHandler::new().on_click(move || {
                    if dis_cell.get() {
                        return;
                    }
                    let vi = if use_virtual {
                        stv_local[pool_idx].get()
                    } else {
                        pool_idx
                    };
                    fi.set(vi);
                    sel.set(Some(vi));
                    if let Some(ref cb) = on_sel {
                        cb(vi);
                    }
                    sel_state.set_selected(vi);
                });
                if let Some(reg) = ctx.event_registry.as_mut() {
                    item_events.register_all(reg, item_ids[i]);
                }
            }

            // ── Drag-to-reorder ──
            if self.reorderable {
                let controller = ReorderController::new();
                controller.set_item_height(item_h);
                controller.configure(rdr_item_ids, rdr_offsets);
                if let Some(cb) = self.on_reorder {
                    let cb2 = cb.clone();
                    controller.on_reorder(move |s: usize, d: usize| cb2(s, d));
                }
                let ro_ctrl = Rc::new(controller);

                for &eid in item_ids.iter() {
                    let drag_events = EventHandler::new()
                        .on_drag_start({
                            let ctrl = ro_ctrl.clone();
                            move |_local: Point, abs: Point| {
                                ctrl.begin(eid, abs);
                            }
                        })
                        .on_drag_update({
                            let ctrl = ro_ctrl.clone();
                            move |_local: Point, abs: Point| {
                                ctrl.update(abs);
                            }
                        })
                        .on_drag_end({
                            let ctrl = ro_ctrl.clone();
                            move |_local: Point, _abs: Point| {
                                ctrl.end();
                            }
                        });
                    if let Some(reg) = ctx.event_registry.as_mut() {
                        drag_events.register_all(reg, eid);
                    }
                }
            }

            // ── Container events ──
            {
                let mut container_events = EventHandler::new();

                if !is_tab_navigable {
                    container_events = container_events
                        .on_focus_in({
                            let fi_in = focused_index.clone();
                            let sel_read = selected_sig.clone();
                            let st = sel_state.clone();
                            move |reason: FocusReason| {
                                if reason == FocusReason::PointerClick {
                                    return;
                                }
                                let sel = sel_read.read();
                                sync_list_selection_focus(&st, sel, fi_in.get());
                            }
                        })
                        .on_action({
                            let sel = selected_sig.clone();
                            let fi = focused_index.clone();
                            let sel_state = sel_state.clone();
                            let on_sel = on_select.clone();
                            let sff = selection_follows_focus;
                            let cur_len = current_len.clone();
                            let container_dirty = ctx.arena.get(id).map(|e| e.dirty.clone());
                            let bundle_nav = bundle.clone();
                            let cid = id;
                            let pitch = item_h + gap;
                            let dis_sig_nav = disabled_items_sig.clone();
                            let stv_nav = slot_to_virtual.clone();
                            let uv_nav = use_virtual;
                            move |action: &Action| -> ActionOutcome {
                                let cnt = cur_len.get();
                                if cnt == 0 {
                                    return ActionOutcome::Unhandled;
                                }
                                let old = fi.get();
                                // Disabled test in DATA space (the slot cache
                                // is pool-sized; indexing it with data indices
                                // panicked in virtual mode — audit round 6).
                                let dis = dis_sig_nav.as_ref().map(|d| d.read());
                                let outcome = row_nav(action.kind, cnt, old, |i| {
                                    dis.as_ref().is_some_and(|d| d.contains(&i))
                                });
                                // Data→slot translation for slot-domain visuals.
                                let nav_slot = |di: usize| -> usize {
                                    if uv_nav {
                                        stv_nav
                                            .iter()
                                            .position(|c| c.get() == di)
                                            .unwrap_or(usize::MAX)
                                    } else {
                                        di
                                    }
                                };
                                match outcome {
                                    RowNavOutcome::Navigate(new_idx) => {
                                        fi.set(new_idx);
                                        sel_state.mark(nav_slot(old));
                                        set_item_highlight(
                                            &sel_state.ids,
                                            Some(nav_slot(old)),
                                            nav_slot(new_idx),
                                        );
                                        if sff {
                                            sel.set(Some(new_idx));
                                            if uv_nav {
                                                sel_state.sync_by(|pi| {
                                                    stv_nav
                                                        .get(pi)
                                                        .is_some_and(|c| c.get() == new_idx)
                                                });
                                            } else {
                                                sel_state.set_selected(new_idx);
                                            }
                                            if let Some(ref cb) = on_sel {
                                                cb(new_idx);
                                            }
                                        }
                                        let vph = crate::core::dirty_registry::bounds_of(cid)
                                            .unwrap_or(crate::style::Rect::ZERO)
                                            .height;
                                        bundle_nav.scroll_to_row(new_idx, pitch, vph);
                                        if let Some(ref d) = container_dirty {
                                            d.set(d.get() | DirtyFlags::REPAINT);
                                        }
                                        crate::core::dirty_registry::register_dirty(
                                            cid,
                                            DirtyFlags::REPAINT,
                                        );
                                        ActionOutcome::Consumed
                                    }
                                    RowNavOutcome::Activate => {
                                        sel.set(Some(old));
                                        if let Some(ref cb) = on_sel {
                                            cb(old);
                                        }
                                        if uv_nav {
                                            sel_state.sync_by(|pi| {
                                                stv_nav.get(pi).is_some_and(|c| c.get() == old)
                                            });
                                        } else {
                                            sel_state.set_selected(old);
                                        }
                                        if let Some(ref d) = container_dirty {
                                            d.set(d.get() | DirtyFlags::REPAINT);
                                        }
                                        crate::core::dirty_registry::register_dirty(
                                            cid,
                                            DirtyFlags::REPAINT,
                                        );
                                        ActionOutcome::Consumed
                                    }
                                    RowNavOutcome::Unhandled => ActionOutcome::Unhandled,
                                }
                            }
                        });
                }

                // ── Type-ahead: character accumulation for ARIA listbox pattern ──
                let ta_buf: Rc<Cell<String>> = Rc::new(Cell::new(String::new()));
                let ta_time: Rc<Cell<u64>> = Rc::new(Cell::new(0));
                {
                    let ta_b = ta_buf.clone();
                    let ta_t = ta_time.clone();
                    let fi = focused_index.clone();
                    let sel = selected_sig.clone();
                    let render = render_fn.clone();
                    let sig_read = items_sig.clone();
                    let sel_state = sel_state.clone();
                    let sff_c = selection_follows_focus;
                    let bundle_ta = bundle.clone();
                    let cid = id;
                    let pitch = item_h + gap;
                    let cdirty = ctx.arena.get(id).map(|e| e.dirty.clone());
                    let dis_sig_ta = disabled_items_sig.clone();
                    let stv_ta = slot_to_virtual.clone();
                    let uv_ta = use_virtual;
                    container_events = container_events.on_key_down(move |key, _mods| -> bool {
                        let ch = match &key {
                            crate::event::Key::Character(c) if c.len() == 1 => c.clone(),
                            _ => return false,
                        };
                        let now = crate::core::clock::animation_millis();
                        if now - ta_t.get() > 800 {
                            ta_b.set(String::new());
                        }
                        ta_t.set(now);
                        let mut buf = ta_b.take();
                        buf.push_str(&ch);
                        let query = buf.to_lowercase();
                        let data = sig_read.read();
                        let cnt = data.len();
                        if cnt == 0 {
                            ta_b.set(buf);
                            return false;
                        }
                        let start = (fi.get() + 1) % cnt;
                        let mut found = None;
                        // Disabled test in DATA space (audit round 6 — the
                        // slot cache is pool-sized, `idx` is a data index).
                        let dis = dis_sig_ta.as_ref().map(|d| d.read());
                        for off in 0..cnt {
                            let idx = (start + off) % cnt;
                            if dis.as_ref().is_some_and(|d| d.contains(&idx)) {
                                continue;
                            }
                            if let Some(item_val) = data.get(idx) {
                                let text = if let Some(ref f) = render {
                                    f(item_val, idx).to_lowercase()
                                } else {
                                    format!("[{idx}]").to_lowercase()
                                };
                                if text.starts_with(&query) {
                                    found = Some(idx);
                                    break;
                                }
                            }
                        }
                        drop(data);
                        if let Some(new_idx) = found {
                            let old = fi.get();
                            fi.set(new_idx);
                            // Slot-domain visuals: map data→slot in virtual mode.
                            let ta_slot = |di: usize| -> usize {
                                if uv_ta {
                                    stv_ta
                                        .iter()
                                        .position(|c| c.get() == di)
                                        .unwrap_or(usize::MAX)
                                } else {
                                    di
                                }
                            };
                            sel_state.mark(ta_slot(old));
                            set_item_highlight(
                                &sel_state.ids,
                                Some(ta_slot(old)),
                                ta_slot(new_idx),
                            );
                            if sff_c {
                                sel.set(Some(new_idx));
                                if uv_ta {
                                    sel_state.sync_by(|pi| {
                                        stv_ta.get(pi).is_some_and(|c| c.get() == new_idx)
                                    });
                                } else {
                                    sel_state.set_selected(new_idx);
                                }
                            }
                            let vp_h = crate::core::dirty_registry::bounds_of(cid)
                                .unwrap_or(crate::style::Rect::ZERO)
                                .height;
                            bundle_ta.scroll_to_row(new_idx, pitch, vp_h);
                            if let Some(ref d) = cdirty {
                                d.set(d.get() | DirtyFlags::REPAINT);
                            }
                            crate::core::dirty_registry::register_dirty(cid, DirtyFlags::REPAINT);
                            ta_b.set(buf);
                            true
                        } else {
                            ta_b.set(buf);
                            false
                        }
                    });
                }

                if let Some(reg) = ctx.event_registry.as_mut() {
                    container_events.register_all(reg, id);
                }
            }
        }

        // ── Virtual scrolling: frame_tick reconcile ──
        // last_so is shared with the data subscribe: poking it to NaN forces
        // the next tick to re-reconcile every slot (data shrink/replace).
        let last_so = Rc::new(std::cell::Cell::new(crate::style::Vec2::ZERO));
        if use_virtual {
            let so = scroll_offset.clone();
            let last_so = last_so.clone();
            let sig_read = items_sig.clone();
            let container_id = id;
            let pitch = item_h + gap;
            let sel_tick = selected_sig.clone();
            let dis_tick = disabled_items_sig.clone();
            let dc_tick = disabled_cells.clone();
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_frame_tick(Box::new(move || {
                let new_so = so.get();
                let old_so = last_so.get();
                // NaN sentinel (poked by the data subscribe): forces a full
                // re-reconcile — NaN != NaN so the guard passes, and `forced`
                // disables the per-slot unchanged-vi skip below.
                let forced = old_so.y.is_nan();
                if new_so == old_so {
                    return;
                }
                last_so.set(new_so);
                let pool_sz = slot_to_virtual.len();
                if pool_sz == 0 {
                    return;
                }
                // Ring reuse: slot pi hosts the unique vi ∈ [first, first+pool)
                // with vi ≡ pi (mod pool) — scrolling by k rows re-contents
                // only k slots. Borrow items in place (`read()` clones).
                let (cnt, first, changes) = sig_read.with(|data| {
                    let cnt = data.len();
                    if cnt == 0 {
                        return (0, 0, Vec::new());
                    }
                    let first = (new_so.y / pitch).max(0.0) as usize;
                    let first = first.min(cnt.saturating_sub(pool_sz));
                    let mut changes: Vec<(usize, usize, String)> = Vec::new();
                    for pi in 0..pool_sz {
                        let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                        if !forced && slot_to_virtual[pi].get() == vi {
                            continue;
                        }
                        let t = if vi < cnt {
                            if let Some(item_val) = data.get(vi) {
                                if let Some(ref f) = render_fn {
                                    f(item_val, vi)
                                } else {
                                    format!("[{vi}]")
                                }
                            } else {
                                String::new()
                            }
                        } else {
                            String::new()
                        };
                        changes.push((pi, vi, t));
                    }
                    (cnt, first, changes)
                });
                if cnt == 0 {
                    return;
                }
                let any_flip = (0..pool_sz).any(|pi| {
                    let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                    pool_inactive[pi].get() == (vi < cnt)
                });
                if changes.is_empty() && !any_flip {
                    return;
                }
                let ids2 = item_ids.clone();
                let lab2 = lazy_labels.clone();
                let gen2 = lazy_gens.clone();
                let si2 = pool_inactive.clone();
                let stv2 = slot_to_virtual.clone();
                let cid = container_id;
                let vcnt = cnt;
                let sel2 = sel_tick.clone();
                let dis2 = dis_tick.clone();
                let dc2 = dc_tick.clone();
                crate::core::dirty_registry::defer_action(
                    move |arena: &mut crate::core::element::ElementArena,
                          _: crate::core::id::ElementId,
                          _reg: &mut EventRegistry| {
                        let pool_sz = ids2.len();
                        // Slot visual state is re-derived per new virtual index
                        // (audit round 6): CHECKED and DISABLED would otherwise
                        // stick to the pool slot and travel while scrolling.
                        let sel_now = sel2.read();
                        let dis_set = dis2.as_ref().map(|d| d.read());
                        // Structural rebuild only when the active slot set flips
                        // (pool edge / data shrink) — mid-scroll remaps are
                        // content-only (audit 2026-07-15, C3; see table.rs remap).
                        let mut active_set_changed = false;
                        for pi in 0..pool_sz {
                            let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                            let active = vi < vcnt;
                            if si2[pi].get() == active {
                                si2[pi].set(!active);
                                active_set_changed = true;
                            }
                        }
                        for (pi, vi, t) in &changes {
                            let (pi, vi) = (*pi, *vi);
                            stv2[pi].set(vi);
                            if vi >= vcnt {
                                continue; // slot deactivated above
                            }
                            lab2[pi].set(t.clone());
                            gen2[pi].set(gen2[pi].get().wrapping_add(1));
                            if let Some(&eid) = ids2.get(pi) {
                                // CHECKED follows the VIRTUAL index.
                                crate::core::dirty_registry::set_state(
                                    eid,
                                    crate::core::config::StateFlags::CHECKED,
                                    sel_now == Some(vi),
                                );
                                // DISABLED follows the VIRTUAL index; keep the
                                // per-slot click-guard cache in sync.
                                if let Some(ref dis) = dis_set {
                                    let is_dis = dis.contains(&vi);
                                    if let Some(c) = dc2.get(pi) {
                                        c.set(is_dis);
                                    }
                                    set_item_disabled(eid, is_dis);
                                }
                                // Reposition the slot at its virtual
                                // content-space Y (see VirtualSlotY).
                                let new_y = vi as f32 * pitch;
                                if let Some(el) = arena.get(eid) {
                                    if let Some(vsy) = el.get_user_data::<VirtualSlotY>() {
                                        if (vsy.0.get() - new_y).abs() > 0.01 {
                                            vsy.0.set(new_y);
                                            crate::core::dirty_registry::mark_dirty(
                                                eid,
                                                DirtyFlags::REPOSITION,
                                            );
                                            crate::core::dirty_registry::register_dirty(
                                                eid,
                                                DirtyFlags::REPOSITION,
                                            );
                                        }
                                    }
                                }
                                crate::core::dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
                                crate::core::dirty_registry::register_dirty(
                                    eid,
                                    DirtyFlags::REPAINT,
                                );
                                crate::core::dirty_registry::bump_subtree_gen(eid);
                            }
                        }
                        if active_set_changed {
                            crate::core::dirty_registry::mark_structurally_changed(cid);
                        }
                        if let Some(el) = arena.get_mut(cid) {
                            if let Some(vcb_cell) = el.get_user_data::<VirtualContentBounds>() {
                                vcb_cell.0.set(crate::style::Rect::new(
                                    0.0,
                                    0.0,
                                    0.0,
                                    vcnt as f32 * pitch,
                                ));
                            }
                        }
                    },
                );
            }));
        }

        ctx.register_theme_component(
            id,
            &ResolvedComponentStyle::ListItem(resolved.clone()),
            &role,
            &self.style,
        );

        id
    }
}

impl<T: Clone + 'static> std::fmt::Debug for List<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("List").finish_non_exhaustive()
    }
}
