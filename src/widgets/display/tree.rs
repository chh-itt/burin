use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::hash::Hash;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::config::{
    ElementBuilder, EventHandler, InteractionConfig, LayoutConfig, PaintConfig,
};
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::event::action::{Action, ActionKind, ActionOutcome};
use crate::event::FocusReason;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Dimension, Padding, TextAlign};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
use crate::widgets::bundle::ScrollBundle;
use crate::widgets::shared::{
    row_nav, set_item_highlight, sync_list_selection_focus, RowNavOutcome, SelectionBg, SlotPool,
    TextCellState,
};

// ── TreeNode trait ──

pub trait TreeNode: Sized {
    type Id: Clone + Eq + Hash + 'static;

    fn id(&self) -> Self::Id;
    fn label(&self) -> String;
    fn children(&self) -> &[Self];

    fn is_expandable(&self) -> bool {
        !self.children().is_empty()
    }
}

// ── FlatItem: visible row in the flattened output ──

struct FlatItem<T> {
    value: T,
    depth: usize,
    is_expanded: bool,
}

// ── flatten_visible ──

fn flatten_visible<T: TreeNode + Clone>(
    roots: &[T],
    expanded: &HashSet<T::Id>,
) -> Vec<FlatItem<T>> {
    let mut out = Vec::new();
    fn dfs<T: TreeNode + Clone>(
        nodes: &[T],
        depth: usize,
        expanded: &HashSet<T::Id>,
        out: &mut Vec<FlatItem<T>>,
    ) {
        for node in nodes {
            let expandable = !node.children().is_empty();
            let is_exp = expanded.contains(&node.id());
            out.push(FlatItem {
                value: node.clone(),
                depth,
                is_expanded: is_exp,
            });
            if expandable && is_exp {
                dfs(node.children(), depth + 1, expanded, out);
            }
        }
    }
    dfs(roots, 0, expanded, &mut out);
    out
}

/// Data stored in the container's user_data for keyboard navigation.
struct TreeData<T> {
    flat: RefCell<Vec<FlatItem<T>>>,
}

// ── Tree widget ──

pub struct Tree<T: TreeNode + Clone + 'static> {
    roots: Signal<Vec<T>>,
    selected: Option<Signal<Option<T::Id>>>,
    on_select: Option<Rc<dyn Fn(T::Id)>>,
    expanded: Option<Signal<HashSet<T::Id>>>,
    indent: f32,
    row_height: f32,
    style: StyleRefinement,
    disabled: bool,
    selection_follows_focus: bool,
    reserve: usize,
    virtual_threshold: Option<usize>,
    row_render: Option<Rc<dyn Fn(&T, usize, bool) -> String>>,
}

impl<T: TreeNode + Clone + 'static> Tree<T> {
    pub fn new(roots: Signal<Vec<T>>) -> Self {
        Self {
            roots,
            selected: None,
            on_select: None,
            expanded: None,
            indent: 20.0,
            row_height: 32.0,
            style: StyleRefinement::default(),
            disabled: false,
            selection_follows_focus: false,
            reserve: 32,
            virtual_threshold: None,
            row_render: None,
        }
    }

    pub fn selected(mut self, sig: Signal<Option<T::Id>>) -> Self {
        self.selected = Some(sig);
        self
    }
    pub fn on_select(mut self, f: impl Fn(T::Id) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    pub fn expanded(mut self, sig: Signal<HashSet<T::Id>>) -> Self {
        self.expanded = Some(sig);
        self
    }
    pub fn indent(mut self, px: f32) -> Self {
        self.indent = px;
        self
    }
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }
    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn selection_follows_focus(mut self, v: bool) -> Self {
        self.selection_follows_focus = v;
        self
    }
    pub fn reserve(mut self, n: usize) -> Self {
        self.reserve = n;
        self
    }

    /// Enable virtual scrolling when the VISIBLE row count (flattened)
    /// exceeds this threshold. Default: auto (20 rows). Set to 0 to always
    /// virtualize. Same model as `List::virtual_threshold`.
    pub fn virtual_threshold(mut self, n: usize) -> Self {
        self.virtual_threshold = Some(n);
        self
    }

    /// Override the row label for each visible item. The closure receives
    /// `(&T, depth, is_expanded)` and returns the display string (after the
    /// expand/collapse arrow). When set, `TreeNode::label()` is only used
    /// for the accessible label; the display text comes from this closure.
    pub fn row_render(mut self, f: impl Fn(&T, usize, bool) -> String + 'static) -> Self {
        self.row_render = Some(Rc::new(f));
        self
    }
}

impl<T: TreeNode + Clone + 'static> Styled for Tree<T> {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: TreeNode + Clone + 'static> Widget for Tree<T> {
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
        let row_h = self.row_height;
        let gap = self.style.gap.unwrap_or(0.0);
        let extra_spacing = self.style.gap.unwrap_or(8.0);
        let indent = self.indent;
        let roots_sig = self.roots.clone();
        let selected_sig = self.selected.clone();
        let on_select = self.on_select.clone();
        let selection_follows_focus = self.selection_follows_focus;
        let is_disabled = self.disabled;
        let reserve = self.reserve;
        let comp_mask = self.component_mask();
        let row_render = self.row_render.clone();
        let devtools_app: Option<std::rc::Weak<crate::core::app_context::AppContext>> =
            Some(ctx.app.clone());

        let expanded_sig = match self.expanded {
            Some(sig) => sig,
            None => Signal::new(HashSet::new()),
        };

        let role = ComponentRole::Interactive(InteractiveRole::TreeItem {
            selected: false,
            expanded: false,
        });
        let resolved = match ctx.theme.resolve_component(&role) {
            ResolvedComponentStyle::TreeItem(s) => s,
            _ => unreachable!(),
        };

        // ── Initial flatten ──
        let roots_data = roots_sig.read();
        let expanded_set = expanded_sig.read();
        let flat = flatten_visible(&roots_data, &expanded_set);
        let flat_len = flat.len();

        // ── Pool sizing ──
        // Virtual mode (audit 2026-07-17 round 5 follow-up): same model as
        // List — the pool covers the tallest realistic viewport, rows are
        // absolutely positioned at `virtual_index × row_h` via VirtualSlotY,
        // and a frame_tick ring-remaps slot contents while scrolling.
        let threshold = self.virtual_threshold.unwrap_or(20);
        let use_virtual = flat_len > threshold;
        const MAX_VIEWPORT_PX: f32 = 4320.0; // 8K portrait
        let pool_sz = if use_virtual {
            let viewport_cover = (MAX_VIEWPORT_PX / row_h.max(1.0)).ceil() as usize + 2;
            threshold.max(viewport_cover).min(flat_len)
        } else {
            reserve.max(flat_len)
        };

        // ── Per-slot cells ──
        let pool_mgr = SlotPool::new(pool_sz);
        let slot_to_virtual: Rc<Vec<Cell<usize>>> = Rc::new((0..pool_sz).map(Cell::new).collect());
        for i in flat_len..pool_sz {
            pool_mgr.set_inactive(i, true);
        }

        // ── Focus ──
        let focused_index: Rc<Cell<usize>> = Rc::new(Cell::new(0));

        // ── ScrollBundle ──
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

        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_accessible_role(accesskit::Role::Tree);
            el.set_accessible_label("Tree".to_owned());
            if !is_disabled {
                el.set_focusable(true);
            }
            el.set_outline_width(-1.0); // disable auto focus ring (0.0 triggers it)
            el.set_flex_grow(1.0);
        }

        // ── VStack inside clip ──
        let vstack_id = ctx.arena.allocate();
        {
            let Some(el) = ctx.arena.get_mut(vstack_id) else {
                return id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_affected_by_child_size(false);
            el.set_flex_grow(1.0);
            if self.style.gap.is_some() {
                el.set_gap(extra_spacing);
            }
        }
        ctx.arena.add_child(bundle.clip_id, vstack_id);

        // ── Mount pool rows ──
        let mut item_ids: Vec<ElementId> = Vec::with_capacity(pool_sz);
        let mut lazy_labels: Vec<Rc<Cell<String>>> = Vec::with_capacity(pool_sz);
        let mut lazy_gens: Vec<Rc<Cell<u64>>> = Vec::with_capacity(pool_sz);
        for i in 0..pool_sz {
            let text = if i < flat_len {
                let item = &flat[i];
                let ch = if item.value.is_expandable() {
                    if item.is_expanded {
                        "\u{25BE}"
                    } else {
                        "\u{25B8}"
                    }
                } else {
                    " "
                };
                let label = if let Some(ref render) = row_render {
                    render(&item.value, item.depth, item.is_expanded)
                } else {
                    item.value.label()
                };
                format!("{}  {}", ch, label)
            } else {
                String::new()
            };

            let paint = PaintConfig {
                background: Some(resolved.background),
                foreground: Some(resolved.foreground),
                font_size: resolved.font_size,
                font_weight: 400,
                corner_radius: crate::style::CornerRadii::all(4.0),
                text_align: TextAlign::Start,
                ..PaintConfig::default()
            };

            let left_pad = if i < flat_len {
                flat[i].depth as f32 * indent + 8.0
            } else {
                8.0
            };
            let layout = LayoutConfig {
                width: Dimension::Percent(1.0),
                height: Dimension::Pixels(row_h),
                padding: Padding {
                    left: left_pad,
                    right: 8.0,
                    top: 4.0,
                    bottom: 4.0,
                },
                flex_shrink: 0.0,
                ..LayoutConfig::default()
            };

            let mut builder = ElementBuilder::new()
                .layout(layout)
                .paint(paint)
                .accessibility(accesskit::Role::TreeItem, String::new());

            if !is_disabled {
                builder = builder.interaction(InteractionConfig {
                    input_pass_through: false,
                    ..InteractionConfig::default()
                });
            }

            builder = builder.with_components(comp_mask);
            let item_id = builder.build(ctx);

            {
                let Some(el) = ctx.arena.get_mut(item_id) else {
                    return id;
                };
                el.set_text_vertical_center(true);
                el.set_affected_by_child_size(false);
                el.set_accepts_mouse(true);
                el.with_state_style(|ss| {
                    ss.hovered.background = Some(resolved.hover_bg);
                    ss.focused.background = Some(resolved.hover_bg);
                    ss.checked.background = Some(resolved.selected_bg);
                });
                el.slot_inactive = pool_mgr.cell(i).clone();

                if i < flat_len {
                    el.set_accessible_label(flat[i].value.label());
                }

                let tcs = TextCellState::mount(
                    item_id,
                    el,
                    &text,
                    14.0,
                    1.5,
                    400,
                    None,
                    None,
                    TextAlign::Start,
                );
                lazy_labels.push(tcs.lazy_label);
                lazy_gens.push(tcs.text_gen);

                if use_virtual {
                    // Pool slot: absolutely position the row at its virtual
                    // content-space Y (initially slot index == virtual index).
                    el.insert_user_data(crate::widgets::display::list::VirtualSlotY(Rc::new(
                        Cell::new(i as f32 * (row_h + gap)),
                    )));
                }
            }

            ctx.arena.add_child(vstack_id, item_id);
            item_ids.push(item_id);
        }

        // ── SelectionBg ──
        let sel_state = Rc::new(RefCell::new(SelectionBg::new(item_ids.clone())));

        // ── Content bounds for scroll_to_row ──
        // Virtual mode uses the true row pitch (row_h — rows are border-box
        // sized); the legacy non-virtual path keeps its historical `+8`.
        let row_pitch = row_h + gap;
        if use_virtual {
            bundle.content_bounds.set(crate::style::Rect::new(
                0.0,
                0.0,
                0.0,
                flat_len as f32 * row_pitch,
            ));
        } else {
            bundle.content_bounds.set(crate::style::Rect::new(
                0.0,
                0.0,
                0.0,
                flat_len as f32 * (row_h + extra_spacing),
            ));
        }

        // ── TreeData for keyboard nav ──
        let tree_data = Rc::new(TreeData {
            flat: RefCell::new(flat),
        });
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.insert_user_data(tree_data.clone());
            if use_virtual {
                el.insert_user_data(crate::widgets::display::list::VirtualContentBounds(
                    Cell::new(crate::style::Rect::new(
                        0.0,
                        0.0,
                        0.0,
                        flat_len as f32 * row_pitch,
                    )),
                ));
            }
        }

        // ── Wire events ──
        if !is_disabled {
            // ── Container focus → selection is auto-managed by framework ──
            let container_focus_events = EventHandler::new().on_focus_in({
                let focused_fi = focused_index.clone();
                let td_fi = tree_data.clone();
                let sel_fi = selected_sig.clone();
                let sel_state_fi = sel_state.clone();
                let stv_fi = slot_to_virtual.clone();
                let uv_fi = use_virtual;
                move |reason| {
                    if reason == FocusReason::PointerClick {
                        return;
                    }
                    let flat = td_fi.flat.borrow();
                    if flat.is_empty() {
                        return;
                    }
                    let sel_id: Option<T::Id> = sel_fi.as_ref().and_then(|s| s.read().clone());
                    if let Some(ref id) = sel_id {
                        if let Some(pos) = flat.iter().position(|f| f.value.id() == *id) {
                            focused_fi.set(pos);
                        }
                    }
                    let focused = focused_fi.get();
                    let sel_pos = sel_id
                        .as_ref()
                        .and_then(|id| flat.iter().position(|f| f.value.id() == *id));
                    // sync_list_selection_focus is slot-domain — map data
                    // indices to their hosting slots in virtual mode.
                    let to_slot = |di: usize| -> usize {
                        if uv_fi {
                            stv_fi
                                .iter()
                                .position(|c| c.get() == di)
                                .unwrap_or(usize::MAX)
                        } else {
                            di
                        }
                    };
                    sync_list_selection_focus(
                        &sel_state_fi.borrow(),
                        sel_pos.map(to_slot),
                        to_slot(focused),
                    );
                }
            });

            // ── Mouse wheel scrolling on container ──
            let scroll_events = EventHandler::new().on_scroll({
                let bundle_wheel = bundle.clone();
                let con_wheel = id;
                move |_dx, dy| {
                    let vph = crate::core::dirty_registry::bounds_of(con_wheel)
                        .unwrap_or(crate::style::Rect::ZERO)
                        .height;
                    if vph <= 0.0 {
                        return false;
                    }
                    let cb_h = bundle_wheel.content_bounds.get().height;
                    let max_y = (cb_h - vph).max(0.0);
                    let mut o = bundle_wheel.scroll_offset.get();
                    o.y -= dy;
                    o.y = o.y.max(0.0).min(max_y);
                    bundle_wheel.scroll_offset.set(o);
                    crate::core::dirty_registry::spatial_update_scroll(
                        bundle_wheel.container_id,
                        o.x,
                        o.y,
                    );
                    crate::core::dirty_registry::bump_subtree_gen(bundle_wheel.container_id);
                    crate::core::dirty_registry::register_dirty(
                        bundle_wheel.container_id,
                        crate::core::element::DirtyFlags::REPAINT,
                    );
                    bundle_wheel
                        .generation
                        .set(bundle_wheel.generation.get() + 1);
                    true
                }
            });

            // ── Keyboard nav on container ──
            let action_events = EventHandler::new().on_action({
                let con_eid = id;
                let td_kb = tree_data.clone();
                let focused_kb = focused_index.clone();
                let sel_kb = selected_sig.clone();
                let on_sel_kb = on_select.clone();
                let sel_follows_kb = selection_follows_focus;
                let es_kb = expanded_sig.clone();
                let sel_state_kb = sel_state.clone();
                let bundle_nav = bundle.clone();
                let row_h_kb = row_h;
                let gap_kb = gap;
                let extra_kb = extra_spacing;
                let items_kb = Rc::new(item_ids.clone());
                let stv_nav = slot_to_virtual.clone();
                let uv_nav = use_virtual;

                move |action: &Action| {
                    let flat = td_kb.flat.borrow();
                    let cur_len = flat.len();
                    let foc = focused_kb.get();

                    // Virtual rows sit at `vi × row_h + gap` (border-box pitch);
                    // the legacy non-virtual path uses the tree spacing default.
                    let actual_row_h = row_h_kb + if uv_nav { gap_kb } else { extra_kb };

                    // SelectionBg / highlight are slot-domain: map a data
                    // index to whichever pool slot currently hosts it.
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
                    let select_slot = |di: usize| {
                        if uv_nav {
                            sel_state_kb
                                .borrow()
                                .sync_by(|p| stv_nav.get(p).is_some_and(|c| c.get() == di));
                        } else {
                            sel_state_kb.borrow().set_selected(di);
                        }
                    };

                    let scroll_to = |idx: usize| {
                        let vph = crate::core::dirty_registry::bounds_of(con_eid)
                            .unwrap_or(crate::style::Rect::ZERO)
                            .height
                            .max(1.0);
                        let target_y = idx as f32 * actual_row_h;
                        let mut o = bundle_nav.scroll_offset.get();
                        if target_y < o.y {
                            o.y = target_y;
                        } else if target_y + actual_row_h > o.y + vph {
                            o.y = (target_y + actual_row_h - vph).max(0.0);
                        }
                        o.y = o.y.max(0.0);
                        bundle_nav.content_bounds.set(crate::style::Rect::new(
                            0.0,
                            0.0,
                            0.0,
                            (cur_len as f32) * actual_row_h,
                        ));
                        bundle_nav.scroll_offset.set(o);
                        crate::core::dirty_registry::spatial_update_scroll(
                            bundle_nav.container_id,
                            o.x,
                            o.y,
                        );
                        crate::core::dirty_registry::bump_subtree_gen(bundle_nav.container_id);
                        bundle_nav.generation.set(bundle_nav.generation.get() + 1);
                    };

                    match action.kind {
                        ActionKind::MoveDown
                        | ActionKind::MoveUp
                        | ActionKind::MoveHome
                        | ActionKind::MoveEnd => {
                            match row_nav(action.kind, cur_len, foc, |_| false) {
                                RowNavOutcome::Navigate(new_idx) => {
                                    focused_kb.set(new_idx);
                                    if sel_follows_kb {
                                        if let Some(ref s) = sel_kb {
                                            let id = flat.get(new_idx).map(|f| f.value.id());
                                            if let Some(id) = id {
                                                s.set(Some(id));
                                            }
                                        }
                                        if let Some(ref f) = on_sel_kb {
                                            if let Some(item) = flat.get(new_idx) {
                                                f(item.value.id());
                                            }
                                        }
                                    }
                                    select_slot(new_idx);
                                    set_item_highlight(
                                        &items_kb,
                                        Some(nav_slot(foc)),
                                        nav_slot(new_idx),
                                    );
                                    scroll_to(new_idx);
                                    ActionOutcome::Consumed
                                }
                                RowNavOutcome::Activate => {
                                    if let Some(item) = flat.get(foc) {
                                        let id = item.value.id();
                                        select_slot(foc);
                                        if let Some(ref s) = sel_kb {
                                            s.set(Some(id.clone()));
                                        }
                                        if let Some(ref f) = on_sel_kb {
                                            f(id.clone());
                                        }
                                        if item.value.is_expandable() {
                                            es_kb.update(|set| {
                                                if set.contains(&id) {
                                                    set.remove(&id);
                                                } else {
                                                    set.insert(id);
                                                }
                                            });
                                        }
                                    }
                                    ActionOutcome::Consumed
                                }
                                RowNavOutcome::Unhandled => ActionOutcome::Unhandled,
                            }
                        }
                        ActionKind::Activate | ActionKind::NewLine => {
                            if let Some(item) = flat.get(foc) {
                                let id = item.value.id();
                                select_slot(foc);
                                if let Some(ref s) = sel_kb {
                                    s.set(Some(id.clone()));
                                }
                                if let Some(ref f) = on_sel_kb {
                                    f(id.clone());
                                }
                                if item.value.is_expandable() {
                                    es_kb.update(|set| {
                                        if set.contains(&id) {
                                            set.remove(&id);
                                        } else {
                                            set.insert(id);
                                        }
                                    });
                                }
                                ActionOutcome::Consumed
                            } else {
                                ActionOutcome::Unhandled
                            }
                        }
                        ActionKind::MoveRight => {
                            if let Some(item) = flat.get(foc) {
                                if item.value.is_expandable() && !item.is_expanded {
                                    es_kb.update(|set| {
                                        set.insert(item.value.id());
                                    });
                                    ActionOutcome::Consumed
                                } else if item.value.is_expandable() && item.is_expanded {
                                    if foc + 1 < cur_len {
                                        focused_kb.set(foc + 1);
                                        select_slot(foc + 1);
                                        set_item_highlight(
                                            &items_kb,
                                            Some(nav_slot(foc)),
                                            nav_slot(foc + 1),
                                        );
                                        scroll_to(foc + 1);
                                        ActionOutcome::Consumed
                                    } else {
                                        ActionOutcome::Unhandled
                                    }
                                } else {
                                    ActionOutcome::Unhandled
                                }
                            } else {
                                ActionOutcome::Unhandled
                            }
                        }
                        ActionKind::MoveLeft => {
                            if let Some(item) = flat.get(foc) {
                                if item.value.is_expandable() && item.is_expanded {
                                    es_kb.update(|set| {
                                        set.remove(&item.value.id());
                                    });
                                    ActionOutcome::Consumed
                                } else {
                                    let cur_depth = item.depth;
                                    for j in (0..foc).rev() {
                                        if let Some(parent) = flat.get(j) {
                                            if parent.depth == cur_depth.saturating_sub(1) {
                                                focused_kb.set(j);
                                                select_slot(j);
                                                set_item_highlight(
                                                    &items_kb,
                                                    Some(nav_slot(foc)),
                                                    nav_slot(j),
                                                );
                                                scroll_to(j);
                                                return ActionOutcome::Consumed;
                                            }
                                        }
                                    }
                                    ActionOutcome::Unhandled
                                }
                            } else {
                                ActionOutcome::Unhandled
                            }
                        }
                        _ => ActionOutcome::Unhandled,
                    }
                }
            });

            if let Some(reg) = ctx.event_registry.as_mut() {
                // ── Per-item click events ──
                for i in 0..pool_sz {
                    let pi = i;
                    let td = tree_data.clone();
                    let es_sig = expanded_sig.clone();
                    let sel = selected_sig.clone();
                    let on_sel = on_select.clone();
                    let focused = focused_index.clone();
                    let sel_state_row = sel_state.clone();
                    let stv_click = slot_to_virtual.clone();
                    let uv_click = use_virtual;
                    let click_events = EventHandler::new().on_click(move || {
                        // Virtual mode: translate pool index to data index
                        // (audit round 5: Tree virtualization — previously
                        // pi was used directly, matching the stale flat entry
                        // at that slot position).
                        let vi = if uv_click { stv_click[pi].get() } else { pi };
                        let flat = td.flat.borrow();
                        if vi >= flat.len() {
                            return;
                        }
                        let item = &flat[vi];
                        if item.value.is_expandable() {
                            es_sig.update(|set| {
                                let id = item.value.id();
                                if set.contains(&id) {
                                    set.remove(&id);
                                } else {
                                    set.insert(id);
                                }
                            });
                        }
                        focused.set(vi);
                        // SelectionBg is slot-domain: map the data index back
                        // to whichever slot currently hosts it.
                        if uv_click {
                            sel_state_row
                                .borrow()
                                .sync_by(|p| stv_click.get(p).is_some_and(|c| c.get() == vi));
                        } else {
                            sel_state_row.borrow().set_selected(vi);
                        }
                        if let Some(ref s) = sel {
                            s.set(Some(item.value.id()));
                        }
                        if let Some(ref f) = on_sel {
                            f(item.value.id());
                        }
                    });
                    click_events.register_all(reg, item_ids[i]);
                }

                container_focus_events.register_all(reg, id);
                scroll_events.register_all(reg, id);
                action_events.register_all(reg, id);
            }
        }

        // ── Signal: roots change → re-flatten & update pool ──
        // Shared with the virtual remap tick: the subscribes poke this to NaN
        // to force a full slot re-reconcile (same protocol as List).
        let last_so = Rc::new(Cell::new(crate::style::Vec2::ZERO));
        let row_render_tick = row_render.clone();
        {
            let roots_sub = roots_sig.clone();
            let es_sub = expanded_sig.clone();
            let roots_sub2 = roots_sub.clone();
            let es_sub2 = es_sub.clone();
            let vstack_cap = vstack_id;
            let items_rc = Rc::new(item_ids.clone());
            let pin_rc = Rc::new(pool_mgr.inactive_cells().to_vec());
            let stv_rc = slot_to_virtual.clone();
            let rh_cap = row_h + gap;
            let ind_cap = indent;
            let fg_cap = resolved.foreground;
            let td_rc = tree_data.clone();
            let content_bounds_cap = bundle.content_bounds.clone();
            let container_cap = id;

            let do_update =
                move |roots_sig: Signal<Vec<T>>,
                      expanded_sig: Signal<HashSet<T::Id>>,
                      items_rc: Rc<Vec<ElementId>>,
                      pin_rc: Rc<Vec<Rc<Cell<bool>>>>,
                      stv_rc: Rc<Vec<Cell<usize>>>,
                      td_rc: Rc<TreeData<T>>,
                      vstack_cap: ElementId,
                      rh_cap: f32,
                      ind_cap: f32,
                      fg_cap: Color,
                      cb_cell: Rc<Cell<crate::style::Rect>>,
                      slot_texts: Rc<Vec<RefCell<String>>>,
                      last_so_sub: Rc<Cell<crate::style::Vec2>>,
                      app: std::rc::Weak<crate::core::app_context::AppContext>| {
                    let roots = roots_sig.read();
                    let expanded = expanded_sig.read();
                    let flat = flatten_visible(&roots, &expanded);
                    let new_len = flat.len();
                    let pool_sz = items_rc.len();

                    // Use the captured AppContext (not current_app()) for cross-window safety
                    let app_rc = match app.upgrade() {
                        Some(a) => a,
                        None => return,
                    };

                    *td_rc.flat.borrow_mut() = flat;

                    if use_virtual {
                        // Virtual mode: the remap tick owns per-slot reconcile.
                        // Force a full re-reconcile on the next tick, sync the
                        // scrollbar range, and clamp the offset if the visible
                        // row count shrank below the current window (collapse).
                        last_so_sub.set(crate::style::Vec2::new(f32::NAN, f32::NAN));
                        let cid_v = container_cap;
                        let pitch = rh_cap;
                        cb_cell.set(crate::style::Rect::new(
                            0.0,
                            0.0,
                            0.0,
                            new_len as f32 * pitch,
                        ));
                        app_rc.defer_action(Box::new(move |arena, _root, _reg| {
                        if let Some(el) = arena.get_mut(cid_v) {
                            if let Some(vcb) = el.get_user_data::<crate::widgets::display::list::VirtualContentBounds>() {
                                vcb.0.set(crate::style::Rect::new(0.0, 0.0, 0.0, new_len as f32 * pitch));
                            }
                        }
                        if let Some(el) = arena.get(cid_v) {
                            if let Some(bref) = el.get_user_data::<crate::widgets::bundle::scroll::ScrollBundleRef>() {
                                let vp_h = el.screen_bounds.height;
                                let max_y = (new_len as f32 * pitch - vp_h).max(0.0);
                                let cur = bref.0.scroll_offset.get();
                                if cur.y > max_y {
                                    bref.0.apply_offset(crate::style::Vec2::new(cur.x, max_y));
                                }
                            }
                        }
                    }));
                        dirty_registry::register_dirty(cid_v, DirtyFlags::REPAINT);
                        return;
                    }

                    let items_d = Rc::clone(&items_rc);
                    let pin_d = Rc::clone(&pin_rc);
                    let stv_d = stv_rc.clone();
                    let td_d = Rc::clone(&td_rc);
                    let vstack_d = vstack_cap;
                    let rh_d = rh_cap;
                    let ind_d = ind_cap;
                    let fg_d = fg_cap;
                    let cb_d = cb_cell;
                    let texts_d = slot_texts;
                    let row_render_d = row_render.clone();

                    app_rc.defer_action(Box::new(move |arena, _root, _reg| {
                        let flat_now = td_d.flat.borrow();
                        for i in 0..pool_sz {
                            let active = i < new_len;
                            if pin_d.get(i).map(|c| c.get() == active).unwrap_or(false) {
                                if let Some(c) = pin_d.get(i) {
                                    c.set(!active);
                                }
                            }
                            if active {
                                stv_d[i].set(i);
                                if let Some(item) = flat_now.get(i) {
                                    let ch = if item.value.is_expandable() {
                                        if item.is_expanded {
                                            "\u{25BE}"
                                        } else {
                                            "\u{25B8}"
                                        }
                                    } else {
                                        " "
                                    };
                                    let label = if let Some(ref render) = row_render_d {
                                        render(&item.value, item.depth, item.is_expanded)
                                    } else {
                                        item.value.label()
                                    };
                                    let display = format!("{}  {}", ch, label);
                                    // Equal-text early-out (audit 2026-07-17 round 5,
                                    // B2): rows above an expand/collapse point keep
                                    // their content — skip the cosmic-text shaping.
                                    let unchanged =
                                        texts_d.get(i).is_some_and(|t| *t.borrow() == display);
                                    if unchanged {
                                        continue;
                                    }
                                    if let Some(t) = texts_d.get(i) {
                                        *t.borrow_mut() = display.clone();
                                    }
                                    if let Some(el) = arena.get_mut(items_d[i]) {
                                        el.set_padding(Padding {
                                            left: item.depth as f32 * ind_d + 8.0,
                                            right: 8.0,
                                            top: 4.0,
                                            bottom: 4.0,
                                        });
                                        el.set_accessible_label(item.value.label());
                                        el.set_foreground(fg_d);
                                        let buf = Rc::new(RefCell::new(create_buffer(
                                            &display,
                                            14.0,
                                            1.5,
                                            400,
                                            None,
                                            None,
                                            TextAlign::Start,
                                        )));
                                        el.set_text_buffer(buf);
                                        // Sync lazy_label so the paint-phase rebuild doesn't overwrite us
                                        el.set_lazy_label(Rc::new(Cell::new(display.clone())));
                                        let new_gen = el.text_generation().map(|tg| {
                                            let val = tg.get().wrapping_add(1);
                                            tg.set(val);
                                            val
                                        });
                                        if let Some(gen) = new_gen {
                                            if let Some(bg) = el.buffer_gen() {
                                                bg.set(gen);
                                            }
                                        }
                                    }
                                    dirty_registry::mark_dirty(items_d[i], DirtyFlags::REPAINT);
                                    dirty_registry::register_dirty(items_d[i], DirtyFlags::REPAINT);
                                    dirty_registry::bump_subtree_gen(items_d[i]);
                                }
                            }
                        }
                        cb_d.set(crate::style::Rect::new(
                            0.0,
                            0.0,
                            0.0,
                            new_len as f32 * (rh_d + 8.0),
                        ));
                    }));
                    // Mark structural change OUTSIDE defer_action so the MEASURE
                    // dirty is registered in the current frame, not a deferred one.
                    app_rc.mark_structurally_changed(vstack_d);
                };

            // Per-slot text cache for the equal-text early-out (B2) — shared
            // by both subscriptions.
            let slot_texts: Rc<Vec<RefCell<String>>> = Rc::new(
                (0..item_ids.len())
                    .map(|_| RefCell::new(String::new()))
                    .collect(),
            );
            // Same-frame dedup (B2): roots + expanded changing in one frame
            // previously ran the full O(pool) reconcile twice.
            let update_pending: Rc<Cell<bool>> = Rc::new(Cell::new(false));

            let items_rc1 = items_rc.clone();
            let pin_rc1 = pin_rc.clone();
            let stv_rc1 = stv_rc.clone();
            let td_rc1 = td_rc.clone();
            let cb_cap1 = content_bounds_cap.clone();
            let texts1 = slot_texts.clone();
            let pending1 = update_pending.clone();
            let last_so1 = last_so.clone();
            let do_update2 = do_update.clone();
            let dapp1 = devtools_app.clone();
            crate::core::signal_bridge::subscribe_owned(id, &roots_sub.clone(), move || {
                if pending1.get() {
                    return;
                }
                pending1.set(true);
                let pending_clear = pending1.clone();
                let app = dapp1.clone();
                if let Some(app) = app.and_then(|a| a.upgrade()) {
                    app.defer_action(Box::new(move |_arena, _root, _reg| {
                        pending_clear.set(false)
                    }));
                }
                do_update(
                    roots_sub.clone(),
                    es_sub.clone(),
                    items_rc1.clone(),
                    pin_rc1.clone(),
                    stv_rc1.clone(),
                    td_rc1.clone(),
                    vstack_cap,
                    rh_cap,
                    ind_cap,
                    fg_cap,
                    cb_cap1.clone(),
                    texts1.clone(),
                    last_so1.clone(),
                    dapp1.clone().unwrap(),
                );
            });

            let items_rc2 = items_rc.clone();
            let pin_rc2 = pin_rc.clone();
            let stv_rc2 = stv_rc.clone();
            let td_rc2 = td_rc.clone();
            let cb_cap2 = content_bounds_cap.clone();
            let texts2 = slot_texts.clone();
            let pending2 = update_pending.clone();
            let last_so2 = last_so.clone();
            let dapp2 = devtools_app.clone();
            crate::core::signal_bridge::subscribe_owned(id, &es_sub2.clone(), move || {
                if pending2.get() {
                    return;
                }
                pending2.set(true);
                let pending_clear = pending2.clone();
                let app = dapp2.clone();
                if let Some(app) = app.and_then(|a| a.upgrade()) {
                    app.defer_action(Box::new(move |_arena, _root, _reg| {
                        pending_clear.set(false)
                    }));
                }
                do_update2(
                    roots_sub2.clone(),
                    es_sub2.clone(),
                    items_rc2.clone(),
                    pin_rc2.clone(),
                    stv_rc2.clone(),
                    td_rc2.clone(),
                    vstack_cap,
                    rh_cap,
                    ind_cap,
                    fg_cap,
                    cb_cap2.clone(),
                    texts2.clone(),
                    last_so2.clone(),
                    dapp2.clone().unwrap(),
                );
            });
        }

        // ── Signal: selection → set CHECKED flag ──
        if let Some(ref sel_sig) = selected_sig {
            let sel_st = sel_state.clone();
            let td_sel = tree_data.clone();
            let sig_clone2 = sel_sig.clone();
            let stv_sel = slot_to_virtual.clone();
            let uv_sel = use_virtual;
            crate::core::signal_bridge::subscribe_owned(id, sel_sig, move || {
                let sel_guard = sig_clone2.read();
                let selected_val: Option<T::Id> = sel_guard.clone();
                drop(sel_guard);
                let flat = td_sel.flat.borrow();
                if let Some(ref selected_id) = selected_val {
                    if let Some(pos) = flat.iter().position(|f| f.value.id() == *selected_id) {
                        // CHECKED is slot-domain; `pos` is a data index — map
                        // it in virtual mode (same as List).
                        if uv_sel {
                            sel_st
                                .borrow()
                                .sync_by(|pi| stv_sel.get(pi).is_some_and(|c| c.get() == pos));
                        } else {
                            sel_st.borrow().set_selected(pos);
                        }
                    }
                }
            });
        }

        // ── Virtual scrolling: frame_tick reconcile (List model) ──
        // Ring reuse: slot pi hosts the unique vi ∈ [first, first+pool) with
        // vi ≡ pi (mod pool) — scrolling by k rows re-contents only k slots.
        // The data source is the CACHED flat (maintained by do_update), so
        // the tick never re-flattens the tree.
        if use_virtual {
            let so = bundle.scroll_offset.clone();
            let last_so_tick = last_so.clone();
            let td_tick = tree_data.clone();
            let container_id = id;
            let pitch = row_h + gap;
            let ind_tick = indent;
            let sel_tick = selected_sig.clone();
            let stv_tick = slot_to_virtual.clone();
            let pool_inactive: Vec<Rc<Cell<bool>>> = pool_mgr.inactive_cells().to_vec();
            let ids_tick = item_ids.clone();
            let labels_tick = lazy_labels.clone();
            let gens_tick = lazy_gens.clone();
            let row_render_ft = row_render_tick.clone();
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_frame_tick(Box::new(move || {
                let new_so = so.get();
                let old_so = last_so_tick.get();
                // NaN sentinel (poked by do_update): forces a full
                // re-reconcile — NaN != NaN passes the guard, and `forced`
                // disables the per-slot unchanged-vi skip below.
                let forced = old_so.y.is_nan();
                if new_so == old_so {
                    return;
                }
                last_so_tick.set(new_so);
                let pool_sz = stv_tick.len();
                if pool_sz == 0 {
                    return;
                }
                let flat = td_tick.flat.borrow();
                let cnt = flat.len();
                if cnt == 0 && !forced {
                    return;
                }
                let first = (new_so.y / pitch).max(0.0) as usize;
                let first = first.min(cnt.saturating_sub(pool_sz));
                // Collect per-slot changes: (pool idx, virtual idx, display
                // text, depth, plain label for a11y).
                let mut changes: Vec<(usize, usize, String, usize, String)> = Vec::new();
                for pi in 0..pool_sz {
                    let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                    if !forced && stv_tick[pi].get() == vi {
                        continue;
                    }
                    if vi < cnt {
                        let item = &flat[vi];
                        let ch = if item.value.is_expandable() {
                            if item.is_expanded {
                                "\u{25BE}"
                            } else {
                                "\u{25B8}"
                            }
                        } else {
                            " "
                        };
                        let label = if let Some(ref render) = row_render_ft {
                            render(&item.value, item.depth, item.is_expanded)
                        } else {
                            item.value.label()
                        };
                        changes.push((
                            pi,
                            vi,
                            format!("{}  {}", ch, label),
                            item.depth,
                            item.value.label(),
                        ));
                    } else {
                        changes.push((pi, vi, String::new(), 0, String::new()));
                    }
                }
                let any_flip = (0..pool_sz).any(|pi| {
                    let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                    pool_inactive[pi].get() == (vi < cnt)
                });
                drop(flat);
                if changes.is_empty() && !any_flip {
                    return;
                }
                let sel_now: Option<usize> = sel_tick.as_ref().and_then(|s| {
                    let sel_id = s.read().clone();
                    sel_id.and_then(|sid| {
                        td_tick
                            .flat
                            .borrow()
                            .iter()
                            .position(|f| f.value.id() == sid)
                    })
                });
                let ids2 = ids_tick.clone();
                let lab2 = labels_tick.clone();
                let gen2 = gens_tick.clone();
                let si2 = pool_inactive.clone();
                let stv2 = stv_tick.clone();
                let cid = container_id;
                let vcnt = cnt;
                let ind2 = ind_tick;
                crate::core::dirty_registry::defer_action(move |arena, _root, _reg| {
                    let pool_sz = ids2.len();
                    // Structural rebuild only when the active slot set flips
                    // (pool edge / collapse) — mid-scroll remaps are
                    // content-only (same contract as List/Table).
                    let mut active_set_changed = false;
                    for pi in 0..pool_sz {
                        let vi = first + (pi + pool_sz - first % pool_sz) % pool_sz;
                        let active = vi < vcnt;
                        if si2[pi].get() == active {
                            si2[pi].set(!active);
                            active_set_changed = true;
                        }
                    }
                    for (pi, vi, text, depth, label) in &changes {
                        let (pi, vi) = (*pi, *vi);
                        stv2[pi].set(vi);
                        if vi >= vcnt {
                            continue; // slot deactivated above
                        }
                        lab2[pi].set(text.clone());
                        let new_gen = gen2[pi].get().wrapping_add(1);
                        gen2[pi].set(new_gen);
                        if let Some(&eid) = ids2.get(pi) {
                            if let Some(el) = arena.get_mut(eid) {
                                if let Some(bg) = el.buffer_gen() {
                                    bg.set(new_gen);
                                }
                            }
                            // CHECKED follows the VIRTUAL index.
                            crate::core::dirty_registry::set_state(
                                eid,
                                crate::core::config::StateFlags::CHECKED,
                                sel_now == Some(vi),
                            );
                            if let Some(el) = arena.get_mut(eid) {
                                // Depth indent + a11y label follow the row.
                                el.set_padding(Padding {
                                    left: *depth as f32 * ind2 + 8.0,
                                    right: 8.0,
                                    top: 4.0,
                                    bottom: 4.0,
                                });
                                el.set_accessible_label(label.clone());
                                // Reposition the slot at its virtual
                                // content-space Y (see VirtualSlotY).
                                let new_y = vi as f32 * pitch;
                                if let Some(vsy) = el
                                    .get_user_data::<crate::widgets::display::list::VirtualSlotY>()
                                {
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
                            crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
                            crate::core::dirty_registry::bump_subtree_gen(eid);
                        }
                    }
                    if active_set_changed {
                        crate::core::dirty_registry::mark_structurally_changed(cid);
                    }
                    if let Some(el) = arena.get_mut(cid) {
                        if let Some(vcb_cell) = el
                            .get_user_data::<crate::widgets::display::list::VirtualContentBounds>(
                        ) {
                            vcb_cell.0.set(crate::style::Rect::new(
                                0.0,
                                0.0,
                                0.0,
                                vcnt as f32 * pitch,
                            ));
                        }
                    }
                });
            }));
        }

        ctx.register_theme_component(
            id,
            &ResolvedComponentStyle::TreeItem(resolved.clone()),
            &role,
            &self.style,
        );

        // ── Signal bridge ──
        {
            let app_weak = ctx.app.clone();
            let Some(container) = ctx.arena.get_mut(id) else {
                return id;
            };
            let roots_sb = roots_sig.clone();
            let expanded_sb = expanded_sig.clone();
            let _obs = crate::core::signal_bridge::observe_element(container, app_weak);
            crate::core::signal_bridge::set_implicit_dirty(DirtyFlags::MEASURE);
            let _r = roots_sb.read();
            let _e = expanded_sb.read();
            drop(_obs);
            crate::core::signal_bridge::apply_observed_subscriptions(container);
        }

        id
    }
}

impl<T: TreeNode + Clone + 'static> std::fmt::Debug for Tree<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tree")
            .field("indent", &self.indent)
            .field("row_height", &self.row_height)
            .field("disabled", &self.disabled)
            .field("reserve", &self.reserve)
            .finish_non_exhaustive()
    }
}
