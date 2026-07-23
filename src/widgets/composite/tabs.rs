use crate::core::config::{EventHandler, StateFlags};
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::element::ElementId;
use crate::core::widget::Widget;
use crate::event::types::Key;
use crate::resource::icons::Icon as IconKind;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Alignment, Color, Padding};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole, ResolvedComponentStyle, TabStyle};
use crate::widgets::display::{Icon, Text};
use auralis_signal::Signal;
use std::cell::Cell;
use std::rc::Rc;

// ── Tab data ─────────────────────────────────────────────────────

pub struct Tab {
    pub label: String,
    pub icon: Option<Icon>,
    pub closable: bool,
    pub disabled: bool,
    pub tooltip: Option<String>,
}

impl Tab {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            closable: false,
            disabled: false,
            tooltip: None,
        }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn closable(mut self) -> Self {
        self.closable = true;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn tooltip(mut self, t: impl Into<String>) -> Self {
        self.tooltip = Some(t.into());
        self
    }
}

// ── Per-tab element tracking ─────────────────────────────────────

#[derive(Clone)]
struct TabElements {
    container: ElementId,
    indicator: ElementId,
    text: ElementId,
}

// ── TabBar ───────────────────────────────────────────────────────

pub struct TabBar {
    tabs: Vec<Tab>,
    active: Signal<usize>,
    on_close: Option<Rc<dyn Fn(usize)>>,
    on_reorder: Option<Rc<dyn Fn(usize, usize)>>,
    on_change: Option<Rc<dyn Fn(usize)>>,
    style: StyleRefinement,
}

impl TabBar {
    pub fn new(active: Signal<usize>) -> Self {
        Self {
            tabs: Vec::new(),
            active,
            on_close: None,
            on_reorder: None,
            on_change: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn tab(mut self, label: impl Into<String>) -> Self {
        self.tabs.push(Tab::new(label));
        self
    }

    pub fn tab_full(mut self, tab: Tab) -> Self {
        self.tabs.push(tab);
        self
    }

    pub fn tabs(mut self, tabs: Vec<Tab>) -> Self {
        self.tabs = tabs;
        self
    }

    pub fn on_close(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    pub fn on_reorder(mut self, f: impl Fn(usize, usize) + 'static) -> Self {
        self.on_reorder = Some(Rc::new(f));
        self
    }

    pub fn on_change(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl Styled for TabBar {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for TabBar {
    fn component_mask(&self) -> u64 {
        use crate::ecs::components;
        components::LAYOUT | components::STYLE | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let tab_count = self.tabs.len();
        let active_sig = self.active;
        let on_close = self.on_close;
        let on_change = self.on_change;

        let tab_style = {
            let tab_role = ComponentRole::Interactive(InteractiveRole::Tab { selected: false });
            let resolved = ctx.theme.resolve_component(&tab_role);
            match &resolved {
                ResolvedComponentStyle::Tab(s) => s.clone(),
                _ => TabStyle {
                    background: Color::TRANSPARENT,
                    foreground: Color::rgba8(100, 100, 100, 255),
                    selected_bg: Color::rgba8(200, 200, 200, 255),
                    selected_fg: Color::BLACK,
                    indicator_color: Color::BLACK,
                    indicator_height: 3.0,
                    hover_bg: Color::rgba8(0, 0, 0, 20),
                    hover_fg: Color::BLACK,
                    pressed_bg: Color::rgba8(0, 0, 0, 40),
                    pressed_fg: Color::BLACK,
                    focused_bg: Color::rgba8(0, 0, 0, 10),
                    focused_fg: Color::BLACK,
                    disabled: crate::theme::m3::states::DisabledColors {
                        background: Color::rgba8(200, 200, 200, 255),
                        foreground: Color::rgba8(150, 150, 150, 255),
                        border: Color::rgba8(200, 200, 200, 255),
                    },
                    font_size: 14.0,
                    height: 48.0,
                    tab_gap: 4.0,
                    pill_radius: crate::style::CornerRadii::all(6.0),
                },
            }
        };

        // ── Container element ──────────────────────────────────────
        let id = ctx.arena.allocate();
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
            el.set_gap(self.style.gap.unwrap_or(tab_style.tab_gap));
            el.set_alignment(Alignment::Start);
        }

        // ── Mount each tab ────────────────────────────────────────
        let mut tab_refs: Vec<TabElements> = Vec::with_capacity(tab_count);

        for (i, tab) in self.tabs.into_iter().enumerate() {
            let is_active = active_sig.read() == i;
            let is_disabled = tab.disabled;

            // Tab container (VStack — interactive, holds button row + indicator bar)
            let tab_container_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(tab_container_id) else {
                    return id;
                };
                el.set_layout_direction(crate::core::LayoutDirection::Vertical);
                el.set_gap(0.0);
                el.set_alignment(Alignment::Stretch);

                if is_disabled {
                    crate::core::dirty_registry::set_state(
                        tab_container_id,
                        StateFlags::DISABLED,
                        true,
                    );
                } else {
                    el.set_focusable(true);
                    crate::core::dirty_registry::set_state(
                        tab_container_id,
                        StateFlags::CHECKED,
                        is_active,
                    );
                }
            }

            // Tab button (HStack — the visual pill, no events)
            let tab_button_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(tab_button_id) else {
                    return id;
                };
                el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
                el.set_alignment(Alignment::Center);
                el.set_content_align(Alignment::Center);
                el.set_gap(6.0);
                el.set_preferred_height(tab_style.height);
                el.set_padding(Padding {
                    left: 12.0,
                    right: 12.0,
                    top: 0.0,
                    bottom: 0.0,
                });
                el.set_corner_radii(tab_style.pill_radius);
                el.set_background(if is_active {
                    tab_style.selected_bg
                } else {
                    Color::TRANSPARENT
                });

                el.with_state_style(|ss| {
                    ss.checked.background = Some(tab_style.selected_bg);
                    ss.checked.foreground = Some(tab_style.selected_fg);
                    ss.hovered.background = Some(tab_style.hover_bg);
                    ss.hovered.foreground = Some(tab_style.hover_fg);
                    ss.pressed.background = Some(tab_style.pressed_bg);
                    ss.pressed.foreground = Some(tab_style.pressed_fg);
                    ss.focused.background = Some(tab_style.focused_bg);
                    ss.focused.foreground = Some(tab_style.focused_fg);
                    ss.disabled.foreground = Some(tab_style.disabled.foreground);
                });
            }

            // Icon
            if let Some(icon) = tab.icon {
                let icon_id = Box::new(icon).mount_box(&mut ctx.child_with_events(tab_button_id));
                ctx.arena.add_child(tab_button_id, icon_id);
            }

            // Label
            let text_fg = if is_active {
                tab_style.selected_fg
            } else {
                tab_style.foreground
            };
            let text_widget = Box::new(Text::new(&tab.label).font_size(tab_style.font_size).color(
                if is_disabled {
                    tab_style.disabled.foreground
                } else {
                    text_fg
                },
            ));
            let text_id = text_widget.mount_box(&mut ctx.child_with_events(tab_button_id));
            ctx.arena.add_child(tab_button_id, text_id);

            // Close button
            if tab.closable {
                if let Some(ref _oc) = on_close {
                    let close_icon = Box::new(
                        Icon::new(IconKind::X)
                            .size(tab_style.font_size * 0.75)
                            .color(if is_active {
                                tab_style.selected_fg
                            } else {
                                tab_style.foreground
                            }),
                    );
                    let close_id = close_icon.mount_box(&mut ctx.child_with_events(tab_button_id));
                    if !is_disabled {
                        let oc = on_close.clone();
                        let ix = i;
                        let mut close_events = EventHandler::new();
                        close_events = close_events.on_click(move || {
                            if let Some(ref cb) = oc {
                                cb(ix);
                            }
                        });
                        if let Some(reg) = ctx.event_registry.as_mut() {
                            close_events.register_all(reg, close_id);
                        }
                    }
                    ctx.arena.add_child(tab_button_id, close_id);
                }
            }

            // ── Events on tab container ───────────────────────────
            let mut events = EventHandler::new();
            if !is_disabled {
                let a = active_sig.clone();
                let ix = i;
                let oc = on_change.clone();
                events = events.on_click(move || {
                    a.set(ix);
                    if let Some(ref cb) = oc {
                        cb(ix);
                    }
                });

                let a_sig = active_sig.clone();
                let len = tab_count;
                let oc2 = on_change.clone();
                events = events.on_key_down(move |key, _mods| {
                    let cur = a_sig.read();
                    let set_and_notify = |idx: usize| {
                        a_sig.set(idx);
                        if let Some(ref cb) = oc2 {
                            cb(idx);
                        }
                    };
                    match key {
                        Key::ArrowLeft => {
                            if cur > 0 {
                                set_and_notify(cur - 1);
                            }
                            true
                        }
                        Key::ArrowRight => {
                            if cur + 1 < len {
                                set_and_notify(cur + 1);
                            }
                            true
                        }
                        Key::Home => {
                            set_and_notify(0);
                            true
                        }
                        Key::End => {
                            set_and_notify(len - 1);
                            true
                        }
                        _ => false,
                    }
                });
            }

            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, tab_container_id);
            }

            // Theme registration
            let sel_role = ComponentRole::Interactive(InteractiveRole::Tab {
                selected: is_active,
            });
            let sel_resolved = ctx.theme.resolve_component(&sel_role);
            ctx.register_theme_component(tab_button_id, &sel_resolved, &sel_role, &self.style);
            ctx.register_theme_component(tab_container_id, &sel_resolved, &sel_role, &self.style);

            // Indicator element (colored bar beneath the tab button)
            let indicator_id = ctx.arena.allocate();
            {
                let Some(el) = ctx.arena.get_mut(indicator_id) else {
                    return id;
                };
                el.set_preferred_height(tab_style.indicator_height);
                el.set_background(if is_active {
                    tab_style.indicator_color
                } else {
                    Color::TRANSPARENT
                });
                el.set_affected_by_child_size(false);
            }

            // Connect: button first (top), indicator second (bottom) in vertical layout
            ctx.arena.add_child(tab_container_id, tab_button_id);
            ctx.arena.add_child(tab_container_id, indicator_id);
            ctx.arena.add_child(id, tab_container_id);

            tab_refs.push(TabElements {
                container: tab_container_id,
                indicator: indicator_id,
                text: text_id,
            });
        }

        // ── Subscribe to active signal ─────────────────────────
        let tabs_info = tab_refs.clone();
        let container_id = id;
        let ts = tab_style.clone();
        let prev_active = Rc::new(Cell::new(active_sig.read()));
        let sub_sig = active_sig.clone();
        if let Some(el) = ctx.arena.get(id) {
            let dirty = el.dirty.clone();
            crate::core::signal_bridge::subscribe_owned(id, &active_sig, move || {
                let new_active = sub_sig.read();
                let old_active = prev_active.get();
                if new_active == old_active {
                    return;
                }
                prev_active.set(new_active);

                // Toggle CHECKED state on containers
                if let Some(info) = tabs_info.get(old_active) {
                    dirty_registry::set_state(info.container, StateFlags::CHECKED, false);
                    dirty_registry::mark_dirty(info.container, DirtyFlags::REPAINT);
                    dirty_registry::register_dirty(info.container, DirtyFlags::REPAINT);
                    dirty_registry::bump_subtree_gen(info.container);
                    dirty_registry::mark_dirty(info.indicator, DirtyFlags::REPAINT);
                    dirty_registry::register_dirty(info.indicator, DirtyFlags::REPAINT);
                    dirty_registry::bump_subtree_gen(info.indicator);
                    dirty_registry::mark_dirty(info.text, DirtyFlags::REPAINT);
                    dirty_registry::register_dirty(info.text, DirtyFlags::REPAINT);
                    dirty_registry::bump_subtree_gen(info.text);
                }
                if let Some(info) = tabs_info.get(new_active) {
                    dirty_registry::set_state(info.container, StateFlags::CHECKED, true);
                    dirty_registry::mark_dirty(info.container, DirtyFlags::REPAINT);
                    dirty_registry::register_dirty(info.container, DirtyFlags::REPAINT);
                    dirty_registry::bump_subtree_gen(info.container);
                    dirty_registry::mark_dirty(info.indicator, DirtyFlags::REPAINT);
                    dirty_registry::register_dirty(info.indicator, DirtyFlags::REPAINT);
                    dirty_registry::bump_subtree_gen(info.indicator);
                    dirty_registry::mark_dirty(info.text, DirtyFlags::REPAINT);
                    dirty_registry::register_dirty(info.text, DirtyFlags::REPAINT);
                    dirty_registry::bump_subtree_gen(info.text);
                }

                dirty.set(dirty.get() | DirtyFlags::REPAINT);
                dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);

                // Defer indicator + text color updates
                let infos = tabs_info.clone();
                let ts = ts.clone();
                dirty_registry::defer_action(move |arena, _, _| {
                    let mut ct = arena.component_tables.borrow_mut();
                    if let Some(info) = infos.get(old_active) {
                        ct.style.entry(info.indicator).or_default().background =
                            Some(Color::TRANSPARENT);
                        ct.style.entry(info.text).or_default().foreground = Some(ts.foreground);
                    }
                    if let Some(info) = infos.get(new_active) {
                        ct.style.entry(info.indicator).or_default().background =
                            Some(ts.indicator_color);
                        ct.style.entry(info.text).or_default().foreground = Some(ts.selected_fg);
                    }
                });
            });
        }

        crate::ecs::register_theme_element(id);
        id
    }
}

impl std::fmt::Debug for TabBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabBar")
            .field("tabs", &self.tabs.len())
            .finish_non_exhaustive()
    }
}

// ── TabPanel ─────────────────────────────────────────────────────

pub struct TabPanel {
    index: usize,
    active: Signal<usize>,
    child: Option<Box<dyn Widget>>,
    style: StyleRefinement,
}

impl TabPanel {
    pub fn new(index: usize, active: Signal<usize>, widget: impl Widget + 'static) -> Self {
        Self {
            index,
            active,
            child: Some(Box::new(widget)),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for TabPanel {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for TabPanel {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        let eid = id;

        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_layout_direction(crate::core::LayoutDirection::Vertical);

            if let Some(bg) = self.style.background {
                element.set_background(bg);
            }
            if let Some(grow) = self.style.flex_grow {
                element.set_flex_grow(grow);
            }

            let is_active = self.active.read() == self.index;
            element.slot_inactive.set(!is_active);

            let idx = self.index;
            let active_sig = self.active.clone();
            let dirty = element.dirty.clone();
            let slot = element.slot_inactive.clone();
            crate::core::signal_bridge::subscribe_owned(id, &self.active, move || {
                let visible = active_sig.read() == idx;
                if slot.get() == visible {
                    slot.set(!visible);
                    dirty.set(dirty.get() | DirtyFlags::MEASURE);
                    dirty_registry::register_dirty(eid, DirtyFlags::MEASURE);
                    dirty_registry::bump_subtree_gen(eid);
                    if let Some(pid) = dirty_registry::parent_of(eid) {
                        dirty_registry::mark_structurally_changed(pid);
                    }
                }
            });
        }

        if let Some(child) = self.child {
            let child_id = child.mount_box(&mut ctx.child_with_events(id));
            ctx.arena.add_child(id, child_id);
        }

        id
    }
}

impl std::fmt::Debug for TabPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabPanel")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}
