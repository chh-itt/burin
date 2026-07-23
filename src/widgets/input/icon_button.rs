//! IconButton — icon-only button, thin wrapper around Button with circle shape.
use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::theme::{Appearance, ControlSize, Intent};
use crate::widgets::display::Icon;
use crate::widgets::input::Button;

pub struct IconButton {
    icon: Icon,
    on_click: Option<Box<dyn Fn()>>,
    disabled: bool,
    loading: bool,
    intent: Intent,
    appearance: Appearance,
    size: ControlSize,
    style: StyleRefinement,
}

impl IconButton {
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            on_click: None,
            disabled: false,
            loading: false,
            intent: Intent::Default,
            appearance: Appearance::Filled,
            size: ControlSize::Medium,
            style: StyleRefinement::default(),
        }
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
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

    pub fn primary(mut self) -> Self {
        self.intent = Intent::Primary;
        self
    }
    pub fn secondary(mut self) -> Self {
        self.intent = Intent::Secondary;
        self
    }
    pub fn danger(mut self) -> Self {
        self.intent = Intent::Danger;
        self
    }
    pub fn warning(mut self) -> Self {
        self.intent = Intent::Warning;
        self
    }
    pub fn success(mut self) -> Self {
        self.intent = Intent::Success;
        self
    }
    pub fn info(mut self) -> Self {
        self.intent = Intent::Info;
        self
    }
    pub fn accent(mut self) -> Self {
        self.intent = Intent::Accent;
        self
    }

    pub fn filled(mut self) -> Self {
        self.appearance = Appearance::Filled;
        self
    }
    pub fn outlined(mut self) -> Self {
        self.appearance = Appearance::Outlined;
        self
    }
    pub fn elevated(mut self) -> Self {
        self.appearance = Appearance::Elevated;
        self
    }
    pub fn text_only(mut self) -> Self {
        self.appearance = Appearance::Text;
        self
    }

    pub fn small(mut self) -> Self {
        self.size = ControlSize::Small;
        self
    }
    pub fn medium(mut self) -> Self {
        self.size = ControlSize::Medium;
        self
    }
    pub fn large(mut self) -> Self {
        self.size = ControlSize::Large;
        self
    }
}

impl Styled for IconButton {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for IconButton {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::INTERACTION
            | components::TEXT
            | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let mut btn = Button::new(self.icon.glyph()).circle();
        btn = match self.intent {
            Intent::Primary => btn.primary(),
            Intent::Secondary => btn.secondary(),
            Intent::Danger => btn.danger(),
            Intent::Warning => btn.warning(),
            Intent::Success => btn.success(),
            Intent::Info => btn.info(),
            Intent::Accent => btn.accent(),
            Intent::Default => btn,
        };
        btn = match self.appearance {
            Appearance::Outlined => btn.outlined(),
            Appearance::Text => btn.text_only(),
            Appearance::Elevated => btn.elevated(),
            _ => btn,
        };
        btn = match self.size {
            ControlSize::Small => btn.small(),
            ControlSize::Large => btn.large(),
            _ => btn,
        };
        if self.disabled {
            btn = btn.disabled();
        }
        if self.loading {
            btn = btn.loading(true);
        }
        if let Some(f) = self.on_click {
            btn = btn.on_click(f);
        }
        Box::new(btn).mount_box(ctx)
    }
}

impl std::fmt::Debug for IconButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconButton")
            .field("disabled", &self.disabled)
            .field("loading", &self.loading)
            .field("intent", &self.intent)
            .field("appearance", &self.appearance)
            .finish_non_exhaustive()
    }
}
