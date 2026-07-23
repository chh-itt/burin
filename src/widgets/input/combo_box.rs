use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::event::DropdownKeyboard;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Dimension, Padding};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle};
use crate::theme::tokens;
use crate::widgets::bundle::ScrollBundle;
use crate::widgets::input::{Button, TextInput};
use crate::widgets::layout::ScrollDirection;
use crate::widgets::overlay::{FlipAxes, PopoverGeometry, PopoverPlacement};
use crate::widgets::shared::dropdown::{
    register_dropdown_portal, register_dropdown_unmount, register_overlay_lifecycle,
    scroll_to_selected_on_open, subscribe_dropdown_reopen,
};
use crate::widgets::shared::SelectionBg;

pub struct ComboBox<T: Clone + 'static> {
    selected: Signal<Option<T>>,
    options: Vec<T>,
    render: Option<Rc<dyn Fn(&T) -> String>>,
    placeholder: String,
    disabled: Signal<bool>,
    close_on_select: bool,
    max_visible: usize,
    item_height: f32,
    on_change: Option<Rc<dyn Fn(T)>>,
    on_open: Option<Rc<dyn Fn()>>,
    on_close: Option<Rc<dyn Fn()>>,
    style: StyleRefinement,
}

impl<T: Clone + 'static> ComboBox<T> {
    pub fn new(selected: Signal<Option<T>>) -> Self {
        Self {
            selected,
            options: Vec::new(),
            render: None,
            placeholder: "Search...".into(),
            disabled: Signal::new(false),
            close_on_select: true,
            max_visible: 6,
            item_height: 36.0,
            on_change: None,
            on_open: None,
            on_close: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn options(mut self, opts: Vec<T>) -> Self {
        self.options = opts;
        self
    }
    pub fn render(mut self, f: impl Fn(&T) -> String + 'static) -> Self {
        self.render = Some(Rc::new(f));
        self
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    pub fn disabled(mut self, d: Signal<bool>) -> Self {
        self.disabled = d;
        self
    }
    pub fn close_on_select(mut self, v: bool) -> Self {
        self.close_on_select = v;
        self
    }
    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n.max(1);
        self
    }
    pub fn item_height(mut self, h: f32) -> Self {
        self.item_height = h;
        self
    }
    pub fn on_change(mut self, f: impl Fn(T) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
    pub fn on_open(mut self, f: impl Fn() + 'static) -> Self {
        self.on_open = Some(Rc::new(f));
        self
    }
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

impl<T: Clone + 'static> Styled for ComboBox<T> {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn format_option<T: Clone + 'static>(opt: &T, render: &Option<Rc<dyn Fn(&T) -> String>>) -> String {
    if let Some(ref f) = render {
        f(opt)
    } else {
        format!("{:?}", std::any::type_name::<T>())
    }
}

impl<T: Clone + 'static> Widget for ComboBox<T> {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;

        let role = ComponentRole::Interactive(InteractiveRole::Select {
            size: crate::theme::ControlSize::Medium,
        });
        let resolved = theme.resolve_component(&role);
        let style = match &resolved {
            ResolvedComponentStyle::Select(s) => s,
            _ => unreachable!(),
        };

        let option_labels: Vec<String> = self
            .options
            .iter()
            .map(|o| format_option(o, &self.render))
            .collect();
        let option_pairs: Vec<(String, T)> = self
            .options
            .iter()
            .zip(option_labels.iter())
            .map(|(o, l)| (l.clone(), o.clone()))
            .collect();

        let placeholder = self.placeholder.clone();
        let item_height = self.item_height;
        let max_visible = self.max_visible;
        let on_change = self.on_change.clone();
        let on_open_final = self.on_open.clone();
        let on_close_final = self.on_close.clone();
        let close_on_select = self.close_on_select;
        let num_items = option_labels.len();

        let selected_val = self.selected.read();
        let selected_idx: Rc<Cell<Option<usize>>> =
            Rc::new(Cell::new(selected_val.as_ref().and_then(|v| {
                option_pairs.iter().position(|(_, val)| {
                    format_option(val, &self.render) == format_option(v, &self.render)
                })
            })));

        // ── Input text signal (drives TextInput display + filtering) ──
        let initial_text = selected_idx
            .get()
            .map_or(String::new(), |i| option_labels[i].clone());
        let input_text: Signal<String> = Signal::new(initial_text);

        // ── Suppress TextInput nav keys when dropdown is open ──
        let suppress_nav: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // ── Root container ──
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_layout_direction(crate::core::LayoutDirection::Vertical);
            element.set_affected_by_child_size(false);
            element.set_preferred_height(style.height);
            if let Some(w) = self.style.width {
                let pixel_fallback = match w {
                    Dimension::Pixels(px) => px,
                    _ => 200.0,
                };
                element.set_preferred_width(Some(pixel_fallback));
                if matches!(w, Dimension::Percent(_)) {
                    element.set_width_dim(Some(w));
                }
            }
        }

        // ── TRIGGER: HStack(TextInput + chevron button) ──
        // The HStack has the border/corner so children look unified.
        let trigger_id = ctx.arena.allocate();
        ctx.preallocate(trigger_id, components::LAYOUT | components::LIFECYCLE);
        {
            let Some(el) = ctx.arena.get_mut(trigger_id) else {
                return id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
            el.set_preferred_width(Some(200.0));
            el.set_preferred_height(36.0);
            el.set_border_width(1.0);
            el.set_border_color(style.trigger_border);
            el.set_background(style.trigger_bg);
            el.set_corner_radius(6.0);
            el.set_padding(Padding {
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            });
        }
        ctx.arena.add_child(id, trigger_id);
        ctx.register_theme_component(trigger_id, &resolved, &role, &self.style);
        if let Some(el) = ctx.arena.get_mut(trigger_id) {
            el.set_corner_radius(6.0);
        }
        let trigger_radius = 6.0f32;

        // TextInput (borderless — border is on the parent HStack)
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
                top_left: trigger_radius,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: trigger_radius,
            });
            el.set_preferred_height(36.0);
            el.set_accessible_role(accesskit::Role::ComboBox);
        }
        ctx.arena.add_child(trigger_id, text_input_id);

        // Chevron button (borderless — border is on the parent HStack)
        let open = Signal::new(false);
        let open_sig = open.clone();

        // ── Click: show options when clicking into the TextInput ──
        if let Some(reg) = ctx.event_registry.as_deref_mut() {
            let oc = open.clone();
            EventHandler::new()
                .on_click_at(move |_pos| {
                    if !oc.read() {
                        oc.set(true);
                    }
                })
                .register_all(reg, text_input_id);
        }

        let chevron_id = {
            let o = open_sig.clone();
            Box::new(Button::new(" ▾").text_only().on_click(move || {
                if !o.read() {
                    o.set(true);
                }
            }))
            .mount_box(&mut ctx.child_with_events(trigger_id))
        };
        {
            let Some(el) = ctx.arena.get_mut(chevron_id) else {
                return id;
            };
            el.set_tab_index(None);
            el.set_focusable(false);
            el.set_flex_shrink(0.0);
            el.set_border_width(0.0);
            el.set_corner_radii(crate::style::CornerRadii {
                top_left: 0.0,
                top_right: trigger_radius,
                bottom_right: trigger_radius,
                bottom_left: 0.0,
            });
            el.set_preferred_height(36.0);
            el.with_state_style(|ss| {
                ss.hovered.background = Some(Color::TRANSPARENT);
                ss.pressed.background = Some(Color::TRANSPARENT);
            });
        }
        ctx.arena.add_child(trigger_id, chevron_id);

        // ── DROPDOWN overlay ──
        let highlighted: Rc<Cell<usize>> = Rc::new(Cell::new(selected_idx.get().unwrap_or(0)));
        let dropdown_id = ctx.arena.allocate();
        ctx.preallocate(dropdown_id, components::LAYOUT | components::LIFECYCLE);

        let visible_count = max_visible.min(num_items.max(1));
        let visible_height = (visible_count as f32 * item_height) + tokens::S1 * 2.0;
        let portal_h: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
        let rv: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let trigger_width = ctx
            .arena
            .get(trigger_id)
            .and_then(|el| el.preferred_width());
        let geo_cell: Rc<Cell<PopoverGeometry>> = Rc::new(Cell::new(PopoverGeometry {
            x: 0.0,
            y: 0.0,
            width: trigger_width.unwrap_or(200.0),
            height: 0.0,
            actual_position: crate::widgets::overlay::PopoverPosition::Bottom,
        }));

        let placement = PopoverPlacement {
            flip_axes: FlipAxes::VerticalOnly,
            viewport_margin: 8.0,
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
            dropdown_el.set_background(style.dropdown_bg);
            dropdown_el.set_border_width(1.0);
            dropdown_el.set_border_color(style.dropdown_border);
            dropdown_el.set_corner_radius(8.0);
            dropdown_el.set_shadow(style.shadow);
            dropdown_el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            dropdown_el.set_reactive_visible(rv.clone());
            dropdown_el.insert_user_data(crate::platform::portal::PortalHeight(portal_h.clone()));
            dropdown_el.insert_user_data(geo_cell.clone());
            dropdown_el.insert_user_data(placement);
            dropdown_el.insert_user_data(trigger_id);
        }
        ctx.register_theme_component(dropdown_id, &resolved, &role, &self.style);

        // ── SCROLL container ──
        let scroll = ScrollBundle::new_rc(
            &mut ctx.child_with_events(dropdown_id),
            0,
            ScrollDirection::Vertical,
            6.0,
        );
        ctx.arena.add_child(dropdown_id, scroll.container_id);
        {
            let Some(scroll_el) = ctx.arena.get_mut(scroll.container_id) else {
                return id;
            };
            let view_h = (visible_count as f32 * item_height).max(item_height);
            scroll_el.set_preferred_height(view_h);
            scroll_el.set_flex_shrink(0.0);
        }
        scroll.content_bounds.set(crate::style::Rect::new(
            0.0,
            0.0,
            0.0,
            num_items as f32 * item_height,
        ));

        // ── Register overlay (portal system) ──
        register_dropdown_portal(id, dropdown_id, open.clone());

        // ── Subscribe: open (shared overlay lifecycle + OverlayStack; the
        //    ComboBox-specific input/scroll sync rides in on_open/on_close;
        //    suppress_nav toggles in both) ──
        {
            let scroll_sub = scroll.clone();
            let sid_sub = selected_idx.clone();
            let ih = item_height;
            let n_opt = num_items;
            let vis = visible_count;
            let sn_open = suppress_nav.clone();
            let sn_close = suppress_nav.clone();
            let it = input_text.clone();
            let placeholder_sub = placeholder.clone();
            let user_on_open = on_open_final.clone();
            let user_on_close = on_close_final.clone();
            let combined_on_open: Rc<dyn Fn()> = Rc::new(move || {
                sn_open.set(true);
                // When opening, clear the input if it shows placeholder
                if it.read() == placeholder_sub {
                    it.set(String::new());
                }
                crate::core::dirty_registry::register_dirty(
                    scroll_sub.container_id,
                    DirtyFlags::MEASURE,
                );
                crate::core::dirty_registry::bump_subtree_gen(scroll_sub.container_id);
                // Skip scroll_to_selected when a filter query is active:
                // the filter subscribe already reset scroll to top, and the
                // selected item's visual position differs from raw index.
                let filter_active = !it.read().is_empty() && it.read() != placeholder_sub;
                if !filter_active {
                    scroll_to_selected_on_open(sid_sub.clone(), ih, vis, n_opt, &scroll_sub);
                }
                if let Some(ref cb) = user_on_open {
                    cb();
                }
            });
            let combined_on_close: Rc<dyn Fn()> = Rc::new(move || {
                sn_close.set(false);
                if let Some(ref cb) = user_on_close {
                    cb();
                }
            });
            register_overlay_lifecycle(
                open.clone(),
                dropdown_id,
                rv.clone(),
                portal_h.clone(),
                visible_height,
                Some(combined_on_open),
                Some(combined_on_close),
            );
        }

        // ── Unmount guard ──
        register_dropdown_unmount(dropdown_id);

        let selected_bg = style.selected_bg;

        // ── Suppress filter-triggered open after selection ──
        // Both the option click handler and the keyboard on_select
        // set input_text to the selected label.  Without this counter,
        // the filter subscription would immediately re-open the dropdown.
        let filter_suppress: Rc<Cell<u32>> = Rc::new(Cell::new(0));

        // ── OPTION items (with reactive_visible for filtering) ──
        let mut option_ids: Vec<ElementId> = Vec::with_capacity(num_items);
        let mut option_visible: Vec<Rc<Cell<bool>>> = Vec::with_capacity(num_items);
        for i in 0..num_items {
            let label = option_labels[i].clone();
            let (_, val) = &option_pairs[i];
            let val_clone = val.clone();

            let sel_sig = self.selected.clone();
            let open_sig = open.clone();
            let oc = on_change.clone();
            let it_sel = input_text.clone();
            let lb_sel = label.clone();
            let fs_clk = filter_suppress.clone();
            let opt_id = Box::new(Button::new(label.clone()).text_only().on_click(move || {
                sel_sig.set(Some(val_clone.clone()));
                if close_on_select {
                    open_sig.set(false);
                }
                // Suppress the filter auto-open triggered by it_sel.set() below.
                fs_clk.set(fs_clk.get() + 1);
                it_sel.set(lb_sel.clone());
                if let Some(ref cb) = oc {
                    cb(val_clone.clone());
                }
            }))
            .mount_box(&mut ctx.child_with_events(scroll.clip_id));
            {
                let Some(el) = ctx.arena.get_mut(opt_id) else {
                    return id;
                };
                el.set_tab_index(None);
                crate::core::dirty_registry::invalidate_focus_order();
                el.set_preferred_width(None);
                el.set_flex_grow(1.0);
                el.set_flex_shrink(1.0);
                el.set_width_dim(Some(Dimension::Percent(1.0)));
                el.set_min_main(0.0);
                el.set_padding(Padding::ZERO);
                el.set_preferred_height(item_height);
                el.set_text_vertical_center(true);
                el.set_corner_radius(0.0);
                el.set_focusable(false);
                el.set_text_align(crate::style::TextAlign::Start);
                el.set_accessible_role(accesskit::Role::ListBoxOption);
                el.set_accessible_label(label.clone());
                el.with_state_style(|ss| {
                    ss.checked.background = Some(selected_bg);
                    ss.hovered.background = Some(style.option_hover_bg);
                    ss.hovered.foreground = Some(style.dropdown_fg);
                    ss.pressed.background = Some(style.option_hover_bg);
                    ss.pressed.foreground = Some(style.dropdown_fg);
                });

                let vis = Rc::new(Cell::new(true));
                el.set_reactive_visible(vis.clone());
                option_visible.push(vis);
            }
            ctx.arena.add_child(scroll.clip_id, opt_id);
            option_ids.push(opt_id);
        }

        // ── SelectionBg manager ──
        let sel_bg = Rc::new(SelectionBg::new(option_ids.clone()));

        // ── Subscribe: input_text → filter options + collapse invisible rows ──
        {
            let ol = option_labels.clone();
            let ov = option_visible.clone();
            let oids = option_ids.clone();
            let it_sub = input_text.clone();
            let it_inner = it_sub.clone();
            let open_filt = open.clone();
            let ih = item_height;
            let scroll_cb = scroll.content_bounds.clone();
            let scroll_cid = scroll.container_id;
            let scroll_off = scroll.scroll_offset.clone();
            let fs = filter_suppress.clone();
            crate::core::signal_bridge::subscribe_owned(id, &it_sub, move || {
                let query = it_inner.read().to_lowercase();
                let has_query = !query.is_empty();
                let mut any_visible = false;
                let mut vis_count = 0u32;
                for i in 0..ol.len() {
                    let matched = !has_query || ol[i].to_lowercase().contains(&query);
                    ov[i].set(matched);
                    if matched {
                        any_visible = true;
                        vis_count += 1;
                    }
                    // Collapse invisible rows so they don't leave blank gaps.
                    let h = if matched { ih } else { 0.0 };
                    crate::core::element::with_ct_mut(|ct| {
                        if let Some(el) = ct.layout.get_mut(&oids[i]) {
                            el.preferred_height = h;
                        }
                    });
                    dirty_registry::mark_dirty(oids[i], DirtyFlags::MEASURE);
                    dirty_registry::register_dirty(oids[i], DirtyFlags::MEASURE);
                }
                // Update scroll content bounds for the new visible count.
                scroll_cb.set(crate::style::Rect::new(
                    0.0,
                    0.0,
                    0.0,
                    vis_count as f32 * ih,
                ));
                // Reset scroll to top when filtered results don't fill the viewport,
                // otherwise the old scroll offset shows blank rows.
                scroll_off.set(crate::style::Vec2::ZERO);
                dirty_registry::bump_subtree_gen(scroll_cid);
                // Control open/close — suppressed after selection to avoid
                // re-opening when on_select sets input_text to the selected label.
                let sup = fs.get();
                if sup > 0 {
                    fs.set(sup - 1);
                } else if has_query {
                    if any_visible {
                        open_filt.set(true);
                    } else {
                        open_filt.set(false);
                    }
                }
            });
        }

        // ── Keyboard (DropdownKeyboard) ──
        if let Some(reg) = ctx.event_registry.as_deref_mut() {
            let Some(dd_el) = ctx.arena.get_mut(dropdown_id) else {
                return id;
            };
            let dropdown_dirty = dd_el.dirty.clone();
            let sel_key = self.selected.clone();
            let oc_key = on_change.clone();
            let ol_kb = option_labels.clone();

            DropdownKeyboard::new(id, open.clone())
                .with_highlighted(highlighted.clone())
                .with_item_count(num_items)
                .with_close_on_select(close_on_select)
                .with_open_on_down(true)
                .with_is_disabled({
                    let ov_dis = option_visible.clone();
                    move |idx| !ov_dis[idx].get()
                })
                .with_on_select({
                    let all_pairs_sel = option_pairs.clone();
                    let sid_set = selected_idx.clone();
                    let it_sel = input_text.clone();
                    let fs_sel = filter_suppress.clone();
                    move |idx| {
                        // Suppress the filter subscription's auto-open:
                        // setting input_text would otherwise re-open the dropdown.
                        fs_sel.set(fs_sel.get() + 1);
                        sid_set.set(Some(idx));
                        if let Some((label, v)) = all_pairs_sel.get(idx) {
                            it_sel.set(label.clone());
                            sel_key.set(Some(v.clone()));
                            if let Some(ref cb) = oc_key {
                                cb(v.clone());
                            }
                        }
                    }
                })
                .with_on_navigate({
                    let sel_bg_nav = sel_bg.clone();
                    let _sid_nav = selected_idx.clone();
                    let bundle_nav = scroll.clone();
                    let oids_nav = option_ids.clone();
                    let d_dirty = dropdown_dirty;
                    move |idx| {
                        sel_bg_nav.set_selected(idx);
                        sel_bg_nav.mark_all();
                        if let Some(&oid) = oids_nav.get(idx) {
                            bundle_nav.scroll_to_keep_visible(oid);
                        }
                        bundle_nav.content_bounds.set(crate::style::Rect::new(
                            0.0,
                            0.0,
                            0.0,
                            num_items as f32 * item_height,
                        ));
                        crate::core::dirty_registry::bump_subtree_gen(bundle_nav.container_id);
                        d_dirty.set(d_dirty.get() | DirtyFlags::MEASURE);
                    }
                })
                .with_typeahead_labels(Rc::new(ol_kb))
                .register(reg);
        }

        // ── Initial SelectionBg sync ──
        if let Some(idx) = selected_idx.get() {
            sel_bg.set_selected(idx);
        }

        // Re-sync selection visual when the dropdown opens (same as Select).
        subscribe_dropdown_reopen(
            id,
            open.clone(),
            selected_idx.clone(),
            sel_bg.clone(),
            option_ids.clone(),
        );

        // ── Register theme element ──
        ctx.register_theme_component(id, &resolved, &role, &self.style);

        id
    }
}

impl<T: Clone + 'static> std::fmt::Debug for ComboBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComboBox")
            .field("options", &self.options.len())
            .field("placeholder", &self.placeholder)
            .finish_non_exhaustive()
    }
}
