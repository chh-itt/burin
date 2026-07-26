use std::cell::Cell;
use std::rc::Rc;

use auralis_signal::Signal;

use crate::animation::{AnimatedProperty, AnimatedValue, AnimationConfig, EasingCurve};
use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::action::{ActionKind, ActionOutcome};
use crate::event::TraversalEdgeBehavior;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Dimension};
use crate::theme::m3::roles::{ComponentRole, DisplayRole, ResolvedComponentStyle};

/// A modal overlay with backdrop, animations, and focus trapping.
pub struct Modal {
    child: Option<Box<dyn Widget>>,
    visible: Signal<bool>,
    backdrop_color: Option<Color>,
    close_on_backdrop: bool,
    animate: bool,
    style: StyleRefinement,
}

impl Modal {
    pub fn new(visible: Signal<bool>, widget: impl Widget + 'static) -> Self {
        Self {
            child: Some(Box::new(widget)),
            visible,
            backdrop_color: None,
            close_on_backdrop: true,
            animate: true,
            style: StyleRefinement::default(),
        }
    }

    pub fn backdrop_color(mut self, c: Color) -> Self {
        self.backdrop_color = Some(c);
        self
    }
    pub fn close_on_backdrop(mut self, v: bool) -> Self {
        self.close_on_backdrop = v;
        self
    }
    pub fn animate(mut self, v: bool) -> Self {
        self.animate = v;
        self
    }
}

impl Styled for Modal {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Modal {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::INTERACTION | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let modal_role = ComponentRole::Display(DisplayRole::Modal);
        let modal_style = match theme.resolve_component(&modal_role) {
            ResolvedComponentStyle::Modal(s) => s,
            _ => unreachable!(),
        };
        let bg_color = self.backdrop_color.unwrap_or(modal_style.backdrop_color);

        // ── Full-screen backdrop ──
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_z_index(theme.z_index.modal);
            el.z_index_floor = Some(theme.z_index.modal);
            el.set_background(bg_color);
            el.set_backdrop(true);
            el.set_focusable(true);
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_animation_config(Some(AnimationConfig {
                property: AnimatedProperty::Opacity,
                from: AnimatedValue::Float(0.0),
                to: AnimatedValue::Float(1.0),
                animation: crate::animation::Animation {
                    curve: EasingCurve::EaseOut,
                    duration_secs: 0.2,
                },
            }));

            let lc = crate::ecs::components::LayoutComponent {
                alignment: crate::style::Alignment::Center,
                content_align: crate::style::Alignment::Center,
                width_dim: Some(Dimension::Percent(100.0)),
                height_dim: Dimension::Percent(100.0),
                ..Default::default()
            };
            crate::core::element::with_ct_mut(|ct| {
                ct.layout.insert(id, lc);
            });
        }

        let close_on_bd = self.close_on_backdrop;
        let visible_sig = self.visible.clone();
        let rv: Rc<Cell<bool>> = Rc::new(Cell::new(self.visible.read()));
        {
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            el.set_reactive_visible(rv.clone());
        }

        let make_entry = {
            let scope_id = id;
            move || crate::event::overlay::OverlayEntry {
                element_id: scope_id,
                layer: crate::event::overlay::OverlayLayer::Modal,
                barrier_color: Some(bg_color),
                dismiss_on_click_outside: close_on_bd,
                dismiss_on_escape: true,
                trap_focus: true,
                autofocus_first: true,
                previous_focus: None,
                on_dismiss: None,
            }
        };

        {
            let open_sub = visible_sig.clone();
            let rv_sub = rv.clone();
            let scope_id = id;
            let Some(el) = ctx.arena.get_mut(id) else {
                return id;
            };
            let dirty = el.dirty.clone();
            let entry_fn = make_entry;
            crate::core::signal_bridge::subscribe_owned(id, &visible_sig, move || {
                let is_open = open_sub.read();
                let was = rv_sub.get();
                if was == is_open {
                    return;
                }
                rv_sub.set(is_open);
                if is_open {
                    // Undo process_exits' permanent ElInfo.visible=false
                    crate::core::dirty_registry::set_elinfo_visible(scope_id, true);
                    crate::event::push_modal_scope(scope_id, TraversalEdgeBehavior::Wrap);
                    dirty.set(dirty.get() | DirtyFlags::MEASURE | DirtyFlags::REPAINT);
                    crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::MEASURE);
                    crate::core::dirty_registry::register_dirty(scope_id, DirtyFlags::REPAINT);
                    crate::core::dirty_registry::bump_subtree_gen(scope_id);
                    crate::event::overlay::push(entry_fn());
                } else {
                    crate::event::pop_modal_scope();
                    crate::event::overlay::remove(scope_id);
                }
            });
        }

        if visible_sig.read() {
            crate::event::push_modal_scope(id, TraversalEdgeBehavior::Wrap);
            // The subscription above only reacts to CHANGES — an
            // initially-visible Modal must join the overlay stack at mount,
            // or Escape/click-outside stack semantics never see it
            // (audit 2026-07-18, popup dismiss contract).
            crate::event::overlay::push(make_entry());
        }

        // ── Unmount cleanup ──
        {
            let did = id;
            let on_unmount = Rc::new(std::cell::RefCell::new(Some(Box::new(move || {
                crate::platform::portal::remove_portal(did);
                crate::event::remove_modal_scopes_of(did);
                crate::event::overlay::remove(did);
            })
                as Box<dyn FnOnce()>)));
            crate::core::element::with_ct_mut(|ct| {
                ct.lc.entry(did).or_default().on_unmount = Some(on_unmount);
            });
        }

        // ── Event handlers ──
        {
            let events = EventHandler::new()
                .on_click({
                    let vis_close = visible_sig.clone();
                    let self_id = id;
                    move || {
                        // Two-step dismiss (audit 2026-07-18): only close on
                        // backdrop click while this Modal is the TOP overlay.
                        // An open child popup (Select dropdown) owns the
                        // first outside click; the Modal takes the next one.
                        if close_on_bd
                            && vis_close.read()
                            && crate::event::overlay::top() == Some(self_id)
                        {
                            vis_close.set(false);
                        }
                    }
                })
                .on_scroll(|_, _| true)
                .on_action({
                    let vis_cancel = visible_sig.clone();
                    let self_id = id;
                    move |action| {
                        if action.kind == ActionKind::Cancel {
                            // Same stack-top guard for Escape reaching the
                            // Modal through action bubbling.
                            if crate::event::overlay::top() == Some(self_id) {
                                vis_cancel.set(false);
                                ActionOutcome::Consumed
                            } else {
                                ActionOutcome::Unhandled
                            }
                        } else {
                            ActionOutcome::Unhandled
                        }
                    }
                });
            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, id);
            }
        }

        // ── Mount child content ──
        if let Some(widget) = self.child.take() {
            let child_id = widget.mount_box(&mut ctx.child_with_events(id));
            {
                let Some(child_el) = ctx.arena.get_mut(child_id) else {
                    return id;
                };
                child_el.set_background(Color::TRANSPARENT);
            }
            ctx.arena.add_child(id, child_id);
        }

        // ── Register theme + portal ──
        ctx.register_theme_component(
            id,
            &ResolvedComponentStyle::Modal(modal_style),
            &modal_role,
            &self.style,
        );
        crate::platform::portal::register_portal(id);

        // User's custom backdrop_color overrides the theme default
        if let Some(custom_bg) = self.backdrop_color {
            if let Some(el) = ctx.arena.get_mut(id) {
                el.set_background(custom_bg);
            }
        }

        id
    }
}

impl std::fmt::Debug for Modal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modal").finish_non_exhaustive()
    }
}

// ── Dialog ─────────────────────────────────────────────────────────

use crate::resource::icons::Icon as IconKind;
use crate::widgets::display::{Icon, Text};
use crate::widgets::input::{Button, IconButton};
use crate::widgets::layout::ScrollView;
use crate::widgets::layout::{Center, HStack, SizedBox, Spacer, VStack};

/// A single action button in a dialog.
pub struct DialogAction {
    label: String,
    intent: crate::theme::Intent,
    on_click: Rc<std::cell::RefCell<Option<Box<dyn FnOnce()>>>>,
    close_on_click: bool,
}

impl DialogAction {
    pub fn new(label: impl Into<String>, intent: crate::theme::Intent) -> Self {
        Self {
            label: label.into(),
            intent,
            on_click: Rc::new(std::cell::RefCell::new(None)),
            close_on_click: true,
        }
    }

    pub fn on_click(mut self, f: impl FnOnce() + 'static) -> Self {
        self.on_click = Rc::new(std::cell::RefCell::new(Some(Box::new(f))));
        self
    }

    pub fn persistent(mut self) -> Self {
        self.close_on_click = false;
        self
    }
}

/// A pre-built modal dialog with header, body, and action buttons.
pub struct Dialog {
    visible: Signal<bool>,
    title: String,
    content_widget: Option<Box<dyn Widget>>,
    content_text: Option<String>,
    actions: Vec<DialogAction>,
    on_close: Option<Box<dyn Fn()>>,
    close_on_backdrop: bool,
    show_close_button: bool,
    scrollable: bool,
    max_width: Option<f32>,
    style: StyleRefinement,
}

impl Dialog {
    pub fn new(visible: Signal<bool>) -> Self {
        Self {
            visible,
            title: String::new(),
            content_widget: None,
            content_text: None,
            actions: Vec::new(),
            on_close: None,
            close_on_backdrop: true,
            show_close_button: true,
            scrollable: false,
            max_width: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }
    pub fn content(mut self, widget: impl Widget + 'static) -> Self {
        self.content_widget = Some(Box::new(widget));
        self
    }
    pub fn content_text(mut self, text: impl Into<String>) -> Self {
        self.content_text = Some(text.into());
        self
    }
    pub fn actions(mut self, actions: Vec<DialogAction>) -> Self {
        self.actions = actions;
        self
    }
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }
    pub fn close_on_backdrop(mut self, v: bool) -> Self {
        self.close_on_backdrop = v;
        self
    }
    pub fn show_close_button(mut self, v: bool) -> Self {
        self.show_close_button = v;
        self
    }
    pub fn scrollable(mut self, v: bool) -> Self {
        self.scrollable = v;
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(w);
        self
    }
}

impl Styled for Dialog {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Dialog {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let vis = self.visible.clone();
        let theme = ctx.theme;
        let dialog_role = ComponentRole::Display(DisplayRole::Dialog);
        let dialog_style = match theme.resolve_component(&dialog_role) {
            ResolvedComponentStyle::Dialog(s) => s,
            _ => unreachable!(),
        };
        let max_w = self.max_width.unwrap_or(dialog_style.max_width);
        let _close_on_bd = self.close_on_backdrop;
        let show_x = self.show_close_button;

        let mut card = VStack::new().gap(0.0);
        {
            let sr = card.style_refinement();
            sr.background = Some(dialog_style.background);
            sr.corner_radius = Some(dialog_style.corner_radius);
            sr.shadow = dialog_style.shadow;
            sr.border_color = Some(dialog_style.border_color);
            sr.border_width = Some(1.0);
        }

        // ── Header ──
        if !self.title.is_empty() || show_x {
            let mut header = HStack::new();
            if !self.title.is_empty() {
                header = header.push(
                    Text::new(&self.title)
                        .font_size(dialog_style.title_font_size)
                        .font_weight(600),
                );
            }
            header = header.push(Spacer::new());
            if show_x {
                let vis_close = vis.clone();
                header = header.push(IconButton::new(Icon::new(IconKind::X)).on_click(move || {
                    vis_close.set(false);
                }));
            }
            card = card.push(header.padding(crate::style::Padding {
                top: 24.0,
                left: 24.0,
                right: 16.0,
                bottom: 0.0,
            }));
        }

        // ── Body (scrollable or not) ──
        let has_title_or_icon = !self.title.is_empty() || show_x;
        let has_body = self.content_text.is_some() || self.content_widget.is_some();
        let has_actions = !self.actions.is_empty();

        if has_body {
            let mut body = VStack::new().gap(8.0);
            if let Some(t) = self.content_text {
                body = body.push(
                    Text::new(t)
                        .font_size(dialog_style.body_font_size)
                        .color(theme.scheme.on_surface_variant),
                );
            }
            if let Some(w) = self.content_widget {
                body = body.push(BoxedWidget(w));
            }
            let top_pad = if has_title_or_icon { 0.0 } else { 24.0 };
            let bot_pad = if has_actions { dialog_style.gap } else { 24.0 };
            body = body.padding(crate::style::Padding {
                left: 24.0,
                right: 24.0,
                top: top_pad,
                bottom: bot_pad,
            });

            if self.scrollable && has_actions {
                card = card.push(ScrollView::new().child(body));
            } else {
                card = card.push(body);
            }
        }

        // ── Actions ──
        if has_actions {
            let mut action_row = HStack::new().gap(8.0).padding(crate::style::Padding {
                bottom: 24.0,
                left: 24.0,
                right: 24.0,
                top: 0.0,
            });
            action_row = action_row.push(Spacer::new());
            for action in self.actions {
                let v = vis.clone();
                let close = action.close_on_click;
                let on_click_cell = action.on_click.clone();
                let btn = match action.intent {
                    crate::theme::Intent::Primary => Button::new(&action.label).primary(),
                    crate::theme::Intent::Secondary => Button::new(&action.label).secondary(),
                    crate::theme::Intent::Danger => Button::new(&action.label).danger(),
                    crate::theme::Intent::Warning => Button::new(&action.label).warning(),
                    crate::theme::Intent::Success => Button::new(&action.label).success(),
                    crate::theme::Intent::Info => Button::new(&action.label).info(),
                    _ => Button::new(&action.label),
                };
                action_row = action_row.push(btn.on_click(move || {
                    if close {
                        v.set(false);
                    }
                    if let Some(f) = on_click_cell.borrow_mut().take() {
                        f();
                    }
                }));
            }
            card = card.push(action_row);
        }

        let wrapper = SizedBox::new()
            .width(Dimension::Pixels(max_w))
            .background(Color::TRANSPARENT);

        // ── Register dialog theme ──
        let modal_content = Center::new(wrapper.child(card));
        let id = Box::new(Modal::new(vis.clone(), modal_content)).mount_box(ctx);

        // ── on_close subscriber ──
        if let Some(on_close) = self.on_close {
            let vis_sub = vis.clone();
            crate::core::signal_bridge::subscribe_owned(id, &vis, move || {
                if !vis_sub.read() {
                    on_close();
                }
            });
        }

        id
    }
}

struct BoxedWidget(Box<dyn Widget>);

impl Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        self.0.mount_box(ctx)
    }
}

impl std::fmt::Debug for Dialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dialog")
            .field("title", &self.title)
            .field("actions", &self.actions.len())
            .finish_non_exhaustive()
    }
}
