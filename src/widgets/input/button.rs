use crate::core::config::{
    ElementBuilder, EventHandler, InteractionConfig, LayoutConfig, PaintConfig,
};
use crate::core::context::MountContext;
use crate::core::element::LazyFontParams;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Dimension, Margin};
use crate::theme::m3::roles::{ComponentRole, InteractiveRole};
use crate::theme::{Appearance, ControlShape, ControlSize, Intent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// ── Button ──

/// A pressable button with a text label.
///
/// Fires an `.on_click()` callback when pressed.  Follows the active
/// theme's button style for colours, border radius, and typography.
/// Supports disabled, loading, and intent variants.
pub struct Button {
    label: String,
    label_signal: Option<auralis_signal::Signal<String>>,
    on_click: Option<Box<dyn Fn()>>,
    on_hover_enter: Option<Box<dyn Fn()>>,
    on_hover_leave: Option<Box<dyn Fn()>>,
    on_focus_in: Option<Box<dyn Fn(crate::event::FocusReason)>>,
    on_focus_out: Option<Box<dyn Fn(crate::event::FocusReason)>>,
    on_key_down: Option<Box<dyn Fn(crate::event::Key, crate::event::Modifiers) -> bool>>,
    on_key_up: Option<Box<dyn Fn(crate::event::Key, crate::event::Modifiers) -> bool>>,
    disabled: bool,
    loading: bool,
    intent: Intent,
    appearance: Appearance,
    size: ControlSize,
    shape: ControlShape,
    style: StyleRefinement,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            label_signal: None,
            on_click: None,
            on_hover_enter: None,
            on_hover_leave: None,
            on_focus_in: None,
            on_focus_out: None,
            on_key_down: None,
            on_key_up: None,
            disabled: false,
            loading: false,
            intent: Intent::Default,
            appearance: Appearance::Filled,
            size: ControlSize::Medium,
            shape: ControlShape::Rounded,
            style: StyleRefinement::default(),
        }
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
    pub fn on_hover_enter(mut self, f: impl Fn() + 'static) -> Self {
        self.on_hover_enter = Some(Box::new(f));
        self
    }
    pub fn on_hover_leave(mut self, f: impl Fn() + 'static) -> Self {
        self.on_hover_leave = Some(Box::new(f));
        self
    }
    pub fn on_focus_in(mut self, f: impl Fn(crate::event::FocusReason) + 'static) -> Self {
        self.on_focus_in = Some(Box::new(f));
        self
    }
    pub fn on_focus_out(mut self, f: impl Fn(crate::event::FocusReason) + 'static) -> Self {
        self.on_focus_out = Some(Box::new(f));
        self
    }
    pub fn on_key_down(
        mut self,
        f: impl Fn(crate::event::Key, crate::event::Modifiers) -> bool + 'static,
    ) -> Self {
        self.on_key_down = Some(Box::new(f));
        self
    }
    pub fn on_key_up(
        mut self,
        f: impl Fn(crate::event::Key, crate::event::Modifiers) -> bool + 'static,
    ) -> Self {
        self.on_key_up = Some(Box::new(f));
        self
    }
    /// Bind a `Signal<String>` for reactive label updates.
    /// The button text updates every frame from the signal.
    pub fn bind(mut self, signal: auralis_signal::Signal<String>) -> Self {
        self.label = signal.read();
        self.label_signal = Some(signal);
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
    pub fn intent(mut self, i: Intent) -> Self {
        self.intent = i;
        self
    }
    pub fn appearance(mut self, a: Appearance) -> Self {
        self.appearance = a;
        self
    }
    pub fn size(mut self, s: ControlSize) -> Self {
        self.size = s;
        self
    }
    pub fn shape(mut self, s: ControlShape) -> Self {
        self.shape = s;
        self
    }
    pub fn primary(self) -> Self {
        self.intent(Intent::Primary)
    }
    pub fn secondary(self) -> Self {
        self.intent(Intent::Secondary)
    }
    pub fn danger(self) -> Self {
        self.intent(Intent::Danger)
    }
    pub fn warning(self) -> Self {
        self.intent(Intent::Warning)
    }
    pub fn success(self) -> Self {
        self.intent(Intent::Success)
    }
    pub fn info(self) -> Self {
        self.intent(Intent::Info)
    }
    pub fn accent(self) -> Self {
        self.intent(Intent::Accent)
    }
    pub fn filled(self) -> Self {
        self.appearance(Appearance::Filled)
    }
    pub fn outlined(self) -> Self {
        self.appearance(Appearance::Outlined)
    }
    pub fn text_only(self) -> Self {
        self.appearance(Appearance::Text)
    }
    pub fn elevated(self) -> Self {
        self.appearance(Appearance::Elevated)
    }
    pub fn small(self) -> Self {
        self.size(ControlSize::Small)
    }
    pub fn medium(self) -> Self {
        self.size(ControlSize::Medium)
    }
    pub fn large(self) -> Self {
        self.size(ControlSize::Large)
    }
    pub fn rounded(self) -> Self {
        self.shape(ControlShape::Rounded)
    }
    pub fn pill(self) -> Self {
        self.shape(ControlShape::Pill)
    }
    pub fn square(self) -> Self {
        self.shape(ControlShape::Square)
    }
    pub fn circle(self) -> Self {
        self.shape(ControlShape::Circle)
    }
}

impl Styled for Button {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Button {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::INTERACTION
            | components::TEXT
            | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Interactive(InteractiveRole::Button {
            intent: self.intent,
            appearance: self.appearance,
            size: self.size,
            shape: self.shape,
        });

        let resolved = ctx.theme.resolve_component(&role);
        let style = match &resolved {
            crate::theme::m3::roles::ResolvedComponentStyle::Button(s) => s,
            _ => unreachable!(),
        };

        let layout = LayoutConfig {
            width: self.style.width.unwrap_or(Dimension::Auto),
            height: self.style.height.unwrap_or(Dimension::Auto),
            min_width: self.style.min_width.unwrap_or(Dimension::Auto),
            max_width: self.style.max_width.unwrap_or(Dimension::Auto),
            padding: self.style.padding.unwrap_or(style.padding),
            margin: self.style.margin.unwrap_or(Margin::ZERO),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            gap: 0.0,
            ..LayoutConfig::default()
        };

        let cursor = if self.disabled {
            crate::platform::CursorIcon::DEFAULT
        } else if self.loading {
            crate::platform::CursorIcon::WAIT
        } else {
            crate::platform::CursorIcon::POINTER
        };

        let mut events = EventHandler::new();
        if !self.disabled {
            let can_click = !self.loading;
            if let Some(handler) = self.on_click.take() {
                if can_click {
                    events = events.on_click(handler);
                }
            }
            if let Some(handler) = self.on_hover_enter.take() {
                events = events.on_hover_enter(handler);
            }
            if let Some(handler) = self.on_hover_leave.take() {
                events = events.on_hover_leave(handler);
            }
            if let Some(handler) = self.on_focus_in.take() {
                events = events.on_focus_in(handler);
            }
            if let Some(handler) = self.on_focus_out.take() {
                events = events.on_focus_out(handler);
            }
            if let Some(handler) = self.on_key_down.take() {
                events = events.on_key_down(handler);
            }
            if let Some(handler) = self.on_key_up.take() {
                events = events.on_key_up(handler);
            }
        }

        let interaction = InteractionConfig {
            events: Some(events),
            enabled: !self.disabled,
            focusable: !self.disabled,
            cursor,
            block_events: false,
            input_pass_through: false,
            ..InteractionConfig::default()
        };

        let paint = PaintConfig {
            background: Some(style.background),
            foreground: Some(style.foreground),
            state_style: self.style.state_style.clone(),
            border_width: if style.border.is_some() { 1.0 } else { 0.0 },
            border_color: style.border,
            outline_width: self.style.outline_width.unwrap_or(0.0),
            outline_color: self.style.outline_color,
            corner_radius: style.corner_radius,
            font_size: style.font_size,
            text_align: self
                .style
                .text_align
                .unwrap_or(crate::style::TextAlign::Center),
            font_family: self.style.font_family.clone(),
            text_decoration: self.style.text_decoration.unwrap_or_default(),
            text_overflow: self.style.text_overflow.unwrap_or_default(),
            shadow: self.style.shadow,
            z_index: self.style.z_index.unwrap_or(0),
            text_direction: self
                .style
                .text_direction
                .unwrap_or(crate::style::TextDirection::Ltr),
            ..PaintConfig::default()
        };

        let id = ElementBuilder::new()
            .with_components(self.component_mask())
            .layout(layout)
            .interaction(interaction)
            .paint(paint)
            .accessibility(accesskit::Role::Button, self.label.clone())
            .build(ctx);

        ctx.register_theme_component(id, &resolved, &role, &self.style);

        let label_signal = self.label_signal;
        let label = self.label;
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };

            let pref_w = crate::render::text::measure_text_width(
                &label,
                style.font_size,
                element.font_weight(),
                element.font_family().map(|s| s.to_string()),
            )
            .max(style.font_size * 2.0);
            let buffer = create_buffer(
                &label,
                style.font_size,
                1.5,
                element.font_weight(),
                element.font_family().as_deref(),
                Some(pref_w),
                element.text_align(),
            );
            element.set_preferred_width(Some(pref_w + style.font_size * 0.15));
            let buf_rc = Rc::new(RefCell::new(buffer));
            element.set_text_buffer(buf_rc.clone());
            element.set_text_generation(Rc::new(Cell::new(1u64)));

            if self.disabled {
                element
                    .state
                    .set(element.state.get() | crate::core::config::StateFlags::DISABLED);
            }
            if self.loading {
                element
                    .state
                    .set(element.state.get() | crate::core::config::StateFlags::LOADING);
            }

            if let Some(ref sig) = label_signal {
                let lazy_label = Rc::new(Cell::new(label.clone()));
                let text_gen = element.text_generation().unwrap();
                let measured_width = Rc::new(Cell::new(0.0));
                let lazy_fp = Rc::new(LazyFontParams {
                    font_size: style.font_size,
                    line_height: 1.5,
                    font_weight: element.font_weight(),
                    font_family: element.font_family().map(|s| s.to_string()),
                    max_width: None,
                    text_align: element.text_align(),
                });
                element.set_lazy_label(lazy_label.clone());
                element.set_buffer_gen(Rc::new(Cell::new(1u64)));
                element.set_measured_text_width(measured_width.clone());
                element.set_lazy_font_params(lazy_fp.clone());
                crate::core::signal_bridge::bind_label_lazy(
                    lazy_label,
                    text_gen,
                    element.dirty.clone(),
                    id,
                    sig,
                    ctx.app.clone(),
                    Some(lazy_fp),
                    Some(measured_width),
                );
            }

            if let Some(tx) = self.style.transform {
                element.set_transform(Some(tx));
            }
        }

        id
    }
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("label", &self.label)
            .field("disabled", &self.disabled)
            .field("intent", &self.intent)
            .field("appearance", &self.appearance)
            .finish_non_exhaustive()
    }
}
