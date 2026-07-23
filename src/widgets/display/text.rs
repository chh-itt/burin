use crate::core::config::{ElementBuilder, LayoutConfig, PaintConfig};
use crate::core::context::MountContext;
use crate::core::element::LazyFontParams;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Color;
use crate::style::Dimension;
use crate::theme::m3::roles::{ColorRole, ComponentRole, DisplayRole};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A simple text label.
pub use auralis_signal::Signal;

pub struct Text {
    content: String,
    style: StyleRefinement,
    dynamic_signal: Option<Signal<String>>,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            style: StyleRefinement::default(),
            dynamic_signal: None,
        }
    }
    /// Convenience — delegates to [`Styled::text_color`].
    pub fn color(mut self, c: Color) -> Self {
        self.style.text_color = Some(c);
        self
    }
    /// Convenience — delegates to [`Styled::font_size`].
    pub fn font_size(mut self, s: f32) -> Self {
        self.style.font_size = Some(s);
        self
    }
    pub fn content(&self) -> &str {
        &self.content
    }
    pub fn bind(mut self, signal: Signal<String>) -> Self {
        self.content = signal.read();
        self.dynamic_signal = Some(signal);
        self
    }
}

impl Styled for Text {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Text {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TEXT
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Display(DisplayRole::Text {
            foreground: ColorRole::OnSurface,
        });
        let t_resolved = match ctx.theme.resolve_component(&role) {
            crate::theme::m3::roles::ResolvedComponentStyle::Text(s) => s,
            _ => unreachable!(),
        };

        let fs = t_resolved.font_size;
        let fw = t_resolved.font_weight;
        let lh = self.style.line_height.unwrap_or(1.4);
        let ff = self.style.font_family.clone();

        let layout = LayoutConfig {
            width: self.style.width.unwrap_or(Dimension::Auto),
            height: self.style.height.unwrap_or(Dimension::Auto),
            min_width: self.style.min_width.unwrap_or(Dimension::Auto),
            max_width: self.style.max_width.unwrap_or(Dimension::Auto),
            padding: self.style.padding.unwrap_or(crate::style::Padding::ZERO),
            margin: self.style.margin.unwrap_or(crate::style::Margin::ZERO),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            gap: 0.0,
            ..LayoutConfig::default()
        };

        let paint = PaintConfig {
            foreground: Some(t_resolved.foreground),
            background: self.style.background,
            font_size: fs,
            font_weight: fw,
            line_height: lh,
            font_family: ff.clone(),
            text_align: self
                .style
                .text_align
                .unwrap_or(crate::style::TextAlign::Start),
            text_decoration: self.style.text_decoration.unwrap_or_default(),
            text_overflow: self.style.text_overflow.unwrap_or_default(),
            shadow: self.style.shadow,
            gradient: self.style.gradient,
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
            .paint(paint)
            .accessibility(accesskit::Role::Label, self.content.clone())
            .build(ctx);

        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };

            element.set_text_vertical_center(false);

            let explicit_width = match self.style.width {
                Some(Dimension::Pixels(px)) => Some(px),
                _ => None,
            };
            let buffer_max_width = explicit_width;

            // Shape ONCE at mount (audit 2026-07-17 round 2): create the
            // buffer first, then read the intrinsic width straight from the
            // shaped runs when single-line Start/Left — the old order ran
            // measure_text_width (a full extra shaping pass) before
            // create_buffer for every static Text.
            let ta = element.text_align();
            let buffer = create_buffer(
                &self.content,
                fs,
                lh,
                fw,
                ff.as_deref(),
                buffer_max_width,
                ta,
            );
            let single_line_w = if self.content.is_empty() {
                Some(0.0)
            } else if matches!(
                ta,
                crate::style::TextAlign::Start | crate::style::TextAlign::Left
            ) {
                crate::render::wgpu::glyphon_bridge::intrinsic_width_from_buffer(&buffer, fs)
            } else {
                None
            };
            let preferred_width = single_line_w
                .unwrap_or_else(|| {
                    crate::render::text::measure_text_width(&self.content, fs, fw, ff.clone())
                })
                .max(fs * 2.0);
            if element.preferred_width().is_none() {
                element.set_preferred_width(Some(preferred_width));
            }
            element.set_preferred_height(fs * lh);

            let buf_rc = Rc::new(RefCell::new(buffer));
            element.set_text_buffer(buf_rc.clone());
            element.set_text_generation(Rc::new(Cell::new(1u64)));

            if let Some(ref sig) = self.dynamic_signal {
                let lazy_label = Rc::new(Cell::new(self.content.clone()));
                let text_gen = element.text_generation().unwrap();
                let measured_width = Rc::new(Cell::new(preferred_width));
                let lazy_fp = Rc::new(LazyFontParams {
                    font_size: fs,
                    line_height: lh,
                    font_weight: fw,
                    font_family: ff.clone(),
                    max_width: buffer_max_width,
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

            if let Some(o) = self.style.opacity {
                element.set_opacity(o);
            }
            if let Some(vis) = self.style.visible {
                element.set_visible(vis);
            }
            if let Some(tx) = self.style.transform {
                element.set_transform(Some(tx));
            }
        }

        ctx.register_theme_component(
            id,
            &crate::theme::m3::roles::ResolvedComponentStyle::Text(t_resolved.clone()),
            &role,
            &self.style,
        );

        id
    }
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Text")
            .field("content", &self.content)
            .finish_non_exhaustive()
    }
}
