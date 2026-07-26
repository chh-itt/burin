use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::core::context::MountContext;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::event::DropdownKeyboard;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Dimension, Padding};
use crate::theme::m3::roles::{
    ComponentRole, DisplayRole, InteractiveRole, ResolvedComponentStyle,
};
use crate::theme::tokens;
use crate::theme::ControlSize;
use crate::widgets::bundle::ScrollBundle;
use crate::widgets::input::Button;
use crate::widgets::layout::ScrollDirection;
use crate::widgets::overlay::{FlipAxes, PopoverGeometry, PopoverPlacement};
use crate::widgets::shared::dropdown::{
    register_dropdown_portal, register_dropdown_unmount, register_overlay_lifecycle,
    scroll_to_selected_on_open, subscribe_dropdown_reopen,
};
use crate::widgets::shared::SelectionBg;

// ═══════════════════════════════════════════════════════════════════════
// OptionGroup
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct OptionGroup<T: Clone> {
    pub label: String,
    pub options: Vec<T>,
}

impl<T: Clone> OptionGroup<T> {
    pub fn new(label: impl Into<String>, options: Vec<T>) -> Self {
        Self {
            label: label.into(),
            options,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Select
// ═══════════════════════════════════════════════════════════════════════

/// A dropdown selector backed by a `Signal<T>`.
///
/// Clicking opens a portal with a scrollable list of options.  Supports
/// keyboard navigation, type-ahead filtering, and disabled state.
pub struct Select<T: Clone + 'static> {
    selected: Signal<Option<T>>,
    options: Vec<T>,
    render: Option<Rc<dyn Fn(&T) -> String>>,
    placeholder: String,
    disabled: Signal<bool>,
    close_on_select: bool,
    max_visible: usize,
    item_height: f32,
    disabled_options: Option<Signal<HashSet<usize>>>,
    groups: Vec<OptionGroup<T>>,
    on_change: Option<Rc<dyn Fn(T)>>,
    on_open: Option<Rc<dyn Fn()>>,
    on_close: Option<Rc<dyn Fn()>>,
    size: ControlSize,
    style: StyleRefinement,
}

impl<T: Clone + 'static> Select<T> {
    pub fn new(selected: Signal<Option<T>>) -> Self {
        Self {
            selected,
            options: Vec::new(),
            render: None,
            placeholder: "Select...".into(),
            disabled: Signal::new(false),
            close_on_select: true,
            max_visible: 6,
            item_height: 36.0,
            disabled_options: None,
            groups: Vec::new(),
            on_change: None,
            on_open: None,
            on_close: None,
            size: ControlSize::Medium,
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
    pub fn disabled_options(mut self, sig: Signal<HashSet<usize>>) -> Self {
        self.disabled_options = Some(sig);
        self
    }
    pub fn groups(mut self, g: Vec<OptionGroup<T>>) -> Self {
        self.groups = g;
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
    pub fn size(mut self, s: ControlSize) -> Self {
        self.size = s;
        self
    }
}

impl<T: Clone + 'static> Styled for Select<T> {
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

/// Flatten groups into (labels, values, header_indices).
fn flatten<T: Clone + 'static>(
    opts: &[T],
    groups: &[OptionGroup<T>],
    render: &Option<Rc<dyn Fn(&T) -> String>>,
) -> (Vec<String>, Vec<(String, T)>, Vec<usize>, usize) {
    let mut labels: Vec<String> = Vec::new();
    let mut pairs: Vec<(String, T)> = Vec::new();
    let mut headers: Vec<usize> = Vec::new();
    let mut global_idx = 0usize;

    if !groups.is_empty() {
        for group in groups {
            headers.push(global_idx);
            labels.push(group.label.clone());
            // Dummy value for header — use group's first option
            let Some(dummy) = group.options.first().cloned() else {
                continue;
            };
            pairs.push((group.label.clone(), dummy));
            global_idx += 1;
            for opt in &group.options {
                let label = format_option(opt, render);
                labels.push(label.clone());
                pairs.push((label, opt.clone()));
                global_idx += 1;
            }
        }
    } else {
        for opt in opts {
            let label = format_option(opt, render);
            labels.push(label.clone());
            pairs.push((label, opt.clone()));
            global_idx += 1;
        }
    }

    (labels, pairs, headers, global_idx)
}

impl<T: Clone + 'static> Widget for Select<T> {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;

        let role = ComponentRole::Interactive(InteractiveRole::Select { size: self.size });
        let resolved = theme.resolve_component(&role);
        let style = match &resolved {
            ResolvedComponentStyle::Select(s) => s,
            _ => unreachable!(),
        };

        let (option_labels, option_pairs, header_indices, num_items) =
            flatten(&self.options, &self.groups, &self.render);

        let placeholder = self.placeholder.clone();
        let item_height = self.item_height;
        let max_visible = self.max_visible;
        let on_change = self.on_change.clone();
        let on_open_final = self.on_open.clone();
        let on_close_final = self.on_close.clone();
        let close_on_select = self.close_on_select;

        // ── Compute initial selected index ──
        let selected_val = self.selected.read();
        let selected_idx: Rc<Cell<Option<usize>>> =
            Rc::new(Cell::new(selected_val.as_ref().and_then(|v| {
                option_pairs.iter().position(|(_, val)| {
                    format_option(val, &self.render) == format_option(v, &self.render)
                })
            })));

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

        // ── TRIGGER button ──
        let chevron = " ▾";
        let trigger_label = format!(
            "{}{}",
            selected_idx
                .get()
                .map_or(placeholder.clone(), |i| option_labels[i].clone()),
            chevron
        );
        let trigger_label_for_lazy = trigger_label.clone();
        let open = Signal::new(false);
        let open_sig = open.clone();
        let trigger_once = Rc::new(Cell::new(false));

        let trigger_btn = {
            let o = open_sig.clone();
            let t = trigger_once.clone();
            Button::new(trigger_label).on_click(move || {
                if t.get() {
                    return;
                }
                t.set(true);
                if !o.read() {
                    o.set(true);
                }
            })
        };
        let trigger_id = Box::new(trigger_btn).mount_box(&mut ctx.child_with_events(id));
        {
            let Some(trigger_el) = ctx.arena.get_mut(trigger_id) else {
                return id;
            };
            trigger_el.set_accessible_role(accesskit::Role::ComboBox);
            trigger_el.set_accessible_label(placeholder.clone());
            trigger_el.set_border_width(1.0);
            trigger_el.set_border_color(style.trigger_border);
            trigger_el.set_corner_radius(6.0);
            trigger_el.with_state_style(|ss| {
                ss.hovered.background = Some(style.trigger_hover_bg);
                ss.hovered.foreground = Some(style.trigger_fg);
                ss.pressed.background = Some(style.trigger_hover_bg);
                ss.pressed.foreground = Some(style.trigger_fg);
            });
        }

        // ── Trigger label lazy update ──
        let trigger_lazy_label = Rc::new(Cell::new(trigger_label_for_lazy));
        let trigger_text_gen = ctx
            .arena
            .get(trigger_id)
            .unwrap()
            .text_generation()
            .unwrap();
        let trigger_once_reset = trigger_once.clone();
        {
            let Some(trigger_el) = ctx.arena.get_mut(trigger_id) else {
                return id;
            };
            trigger_el.set_lazy_label(trigger_lazy_label.clone());
            trigger_el.set_buffer_gen(Rc::new(Cell::new(1u64)));
            trigger_el.set_lazy_font_params(Rc::new(crate::core::element::LazyFontParams {
                font_size: trigger_el.font_size(),
                line_height: trigger_el.line_height(),
                font_weight: trigger_el.font_weight(),
                font_family: trigger_el.font_family().map(|s| s.to_string()),
                max_width: None,
                text_align: trigger_el.text_align(),
            }));
            trigger_el.set_frame_tick(Box::new(move || trigger_once_reset.set(false)));
        }
        let trigger_dirty = ctx.arena.get(trigger_id).unwrap().dirty.clone();
        ctx.arena.add_child(id, trigger_id);

        // ── Subscribe: selected signal → update trigger label + clear button ──
        {
            let sel_read = self.selected.clone();
            let ph = placeholder.clone();
            let ll = trigger_lazy_label.clone();
            let lg = trigger_text_gen.clone();
            let st = selected_idx.clone();
            let all_labels = option_labels.clone();
            let all_pairs = option_pairs.clone();
            let render_fn = self.render.clone();
            let td = trigger_dirty.clone();
            let tid = trigger_id;
            crate::core::signal_bridge::subscribe_owned(tid, &self.selected, move || {
                let val = sel_read.read();
                let idx = val.as_ref().and_then(|v| {
                    all_pairs.iter().position(|(_, pv)| {
                        format_option(pv, &render_fn) == format_option(v, &render_fn)
                    })
                });
                st.set(idx);
                let label = format!("{} ▾", idx.map_or(ph.clone(), |i| all_labels[i].clone()));
                ll.set(label);
                lg.set(lg.get().wrapping_add(1));
                td.set(td.get() | DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(tid, DirtyFlags::REPAINT);
                crate::core::dirty_registry::bump_subtree_gen(tid);
            });
        }

        // ── DROPDOWN overlay ──
        let highlighted: Rc<Cell<usize>> = Rc::new(Cell::new(selected_idx.get().unwrap_or(0)));
        let dropdown_id = ctx.arena.allocate();
        ctx.preallocate(dropdown_id, components::LAYOUT | components::LIFECYCLE);

        let visible_count = max_visible.min(num_items.max(1));
        let visible_height = (visible_count as f32 * item_height) + tokens::S1 * 2.0;
        let portal_h: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
        let rv: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        let trigger_width = ctx.arena.get(trigger_id).unwrap().preferred_width();
        let geo_cell: Rc<Cell<PopoverGeometry>> = Rc::new(Cell::new(PopoverGeometry {
            x: 0.0,
            y: 0.0,
            width: trigger_width.unwrap_or(150.0),
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
        //    Select-specific scroll sync rides in on_open) ──
        {
            let scroll_sub = scroll.clone();
            let sid_sub = selected_idx.clone();
            let ih = item_height;
            let n_opt = num_items;
            let vis = visible_count;
            let scope_id = dropdown_id;
            let on_open_cb: Rc<dyn Fn()> = Rc::new(move || {
                // Dirty the scroll container so its layout (and the
                // scrollbar) is recomputed on the first open frame.
                crate::core::dirty_registry::register_dirty(
                    scroll_sub.container_id,
                    DirtyFlags::MEASURE,
                );
                crate::core::dirty_registry::bump_subtree_gen(scroll_sub.container_id);
                scroll_to_selected_on_open(sid_sub.clone(), ih, vis, n_opt, &scroll_sub);
                let _ = scope_id;
            });
            let combined_on_open: Rc<dyn Fn()> = match on_open_final.clone() {
                Some(user) => {
                    let base = on_open_cb.clone();
                    Rc::new(move || {
                        base();
                        user();
                    })
                }
                None => on_open_cb,
            };
            register_overlay_lifecycle(
                open.clone(),
                dropdown_id,
                rv.clone(),
                portal_h.clone(),
                visible_height,
                Some(combined_on_open),
                on_close_final.clone().map(|cb| cb as Rc<dyn Fn()>),
            );
        }

        // ── Unmount guard ──
        register_dropdown_unmount(dropdown_id);

        // ── OPTION items ──
        let mut option_ids: Vec<ElementId> = Vec::with_capacity(num_items);
        for i in 0..num_items {
            let is_header = header_indices.contains(&i);
            let label = option_labels[i].clone();
            let (_, val) = &option_pairs[i];
            let val_clone = val.clone();

            if is_header {
                let hdr_id = Box::new(Button::new(label.clone()).text_only().disabled())
                    .mount_box(&mut ctx.child_with_events(scroll.clip_id));
                {
                    let Some(el) = ctx.arena.get_mut(hdr_id) else {
                        return id;
                    };
                    el.set_tab_index(None);
                    el.set_preferred_width(None);
                    el.set_flex_grow(1.0);
                    el.set_flex_shrink(0.0);
                    el.set_width_dim(Some(Dimension::Percent(1.0)));
                    el.set_padding(Padding {
                        left: tokens::S2,
                        right: tokens::S1,
                        top: tokens::S1,
                        bottom: tokens::S1,
                    });
                    el.set_preferred_height(28.0);
                    el.set_corner_radius(0.0);
                    el.set_focusable(false);
                    el.set_text_align(crate::style::TextAlign::Start);
                    el.set_font_size(11.0);
                    el.set_font_weight(600);
                    el.set_foreground(theme.scheme.on_surface.with_alpha(0.5));
                }
                ctx.arena.add_child(scroll.clip_id, hdr_id);
                option_ids.push(hdr_id);
            } else {
                let is_item_disabled = self
                    .disabled_options
                    .as_ref()
                    .is_some_and(|dos| dos.read().contains(&i));
                if is_item_disabled {
                    let opt_id = Box::new(Button::new(label.clone()).text_only().disabled())
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
                        el.set_foreground(style.option_disabled_fg);
                        el.set_accessible_role(accesskit::Role::ListBoxOption);
                    }
                    ctx.arena.add_child(scroll.clip_id, opt_id);
                    option_ids.push(opt_id);
                } else {
                    let sel_sig = self.selected.clone();
                    let open_sig = open.clone();
                    let oc = on_change.clone();
                    let opt_id =
                        Box::new(Button::new(label.clone()).text_only().on_click(move || {
                            sel_sig.set(Some(val_clone.clone()));
                            if close_on_select {
                                open_sig.set(false);
                            }
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
                        el.set_min_main(0.0); // allow shrink below content width when scrollbar appears
                        el.set_padding(Padding::ZERO);
                        el.set_preferred_height(item_height);
                        el.set_text_vertical_center(true);
                        el.set_corner_radius(0.0);
                        el.set_focusable(false);
                        el.set_text_align(crate::style::TextAlign::Start);
                        el.set_accessible_role(accesskit::Role::ListBoxOption);
                        el.set_accessible_label(label.clone());
                        el.with_state_style(|ss| {
                            ss.checked.background = Some(style.selected_bg);
                            ss.hovered.background = Some(style.option_hover_bg);
                            ss.hovered.foreground = Some(style.dropdown_fg);
                            ss.pressed.background = Some(style.option_hover_bg);
                            ss.pressed.foreground = Some(style.dropdown_fg);
                        });
                    }
                    ctx.arena.add_child(scroll.clip_id, opt_id);
                    option_ids.push(opt_id);
                }
            }
        }

        // ── SelectionBg manager ──
        let sel_bg = Rc::new(SelectionBg::new(option_ids.clone()));

        // ── Keyboard (DropdownKeyboard) ──
        if let Some(reg) = ctx.event_registry.as_deref_mut() {
            let Some(dd_el) = ctx.arena.get_mut(dropdown_id) else {
                return id;
            };
            let dropdown_dirty = dd_el.dirty.clone();
            let sel_key = self.selected.clone();
            let oc_key = on_change.clone();
            let hdrs_key = header_indices.clone();
            let dos_key = self.disabled_options.clone();

            DropdownKeyboard::new(id, open.clone())
                .with_highlighted(highlighted.clone())
                .with_item_count(num_items)
                .with_close_on_select(close_on_select)
                .with_is_disabled(move |idx| {
                    if hdrs_key.contains(&idx) {
                        return true;
                    }
                    if let Some(ref dis) = dos_key {
                        dis.read().contains(&idx)
                    } else {
                        false
                    }
                })
                .with_on_select({
                    let all_pairs_sel = option_pairs.clone();
                    let sid_set = selected_idx.clone();
                    move |idx| {
                        // Update selected_idx synchronously — the signal subscription
                        // fires asynchronously, so we can't rely on it being
                        // updated before the subsequent on_navigate call.
                        sid_set.set(Some(idx));
                        if let Some((_, v)) = all_pairs_sel.get(idx) {
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
                .with_typeahead_labels(Rc::new(option_labels))
                .register(reg);
        }

        // ── Initial SelectionBg sync ──
        if let Some(idx) = selected_idx.get() {
            sel_bg.set_selected(idx);
        }

        // Re-sync selection visual when the dropdown opens, because
        // set_selected at mount time may have had its repaint cleared
        // while the portal was invisible.
        subscribe_dropdown_reopen(
            id,
            open.clone(),
            selected_idx.clone(),
            sel_bg.clone(),
            option_ids.clone(),
        );

        // ── Register theme for main container and dropdown ──
        ctx.register_theme_component(id, &resolved, &role, &self.style);
        ctx.register_theme_component(
            dropdown_id,
            &resolved,
            &ComponentRole::Display(DisplayRole::Popover),
            &self.style,
        );

        id
    }
}

impl<T: Clone + 'static> std::fmt::Debug for Select<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Select")
            .field("options", &self.options.len())
            .field("placeholder", &self.placeholder)
            .finish_non_exhaustive()
    }
}
