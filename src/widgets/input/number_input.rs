use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::{ElementBuilder, EventHandler, InteractionConfig, LayoutConfig};
use crate::core::context::MountContext;
use crate::core::element::ElementId;
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::event::{Key, Modifiers};
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Padding;
use auralis_signal::Signal;

use super::button::Button;
use super::text_input::{TextInput, TextInputType};

/// A text input with numeric increment and decrement controls.
pub struct NumberInput {
    value: Signal<f64>,
    min: f64,
    max: f64,
    step: f64,
    large_step: f64,
    decimals: usize,
    disabled: bool,
    placeholder: String,
    style: StyleRefinement,
}

impl NumberInput {
    pub fn new(value: Signal<f64>) -> Self {
        Self {
            value,
            min: f64::MIN,
            max: f64::MAX,
            step: 1.0,
            large_step: 10.0,
            decimals: 2,
            disabled: false,
            placeholder: String::new(),
            style: StyleRefinement::default(),
        }
    }

    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }
    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }
    pub fn large_step(mut self, step: f64) -> Self {
        self.large_step = step;
        self
    }
    pub fn decimals(mut self, n: usize) -> Self {
        self.decimals = n;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
}

impl Styled for NumberInput {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn format_value(v: f64, decimals: usize) -> String {
    if decimals == 0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1$}", v, decimals)
    }
}

fn parse_value(s: &str) -> Option<f64> {
    let cleaned = s.trim().replace(',', ".");
    if cleaned.is_empty() || cleaned == "-" || cleaned == "." || cleaned == "-." {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

fn clamp_to_step(v: f64, min: f64, max: f64, step: f64) -> f64 {
    let clamped = v.clamp(min, max);
    // Only snap to step when bounds are within safe arithmetic range.
    // When min/max are at f64 extremes, (v - min) / step overflows and
    // floating-point cancellation snaps the result to 0 regardless of input.
    if step > 0.0 && min > f64::MIN / 2.0 && max < f64::MAX / 2.0 {
        let offset = (clamped - min) / step;
        let snapped = offset.round() * step + min;
        snapped.clamp(min, max)
    } else {
        clamped
    }
}

impl Widget for NumberInput {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let min = self.min;
        let max = self.max;
        let step = self.step;
        let large_step = self.large_step;
        let decimals = self.decimals;
        let disabled = self.disabled;
        let component_mask = self.component_mask();
        let value_signal = self.value;
        let placeholder = self.placeholder;

        // Guard against two-way binding loops: when external code sets the
        // signal (subscribe callback), suppress on_value_changed so it doesn't
        // fire back and trigger parse/clamp/set again.
        let external_sync_guard: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // ── Two-way binding: Signal<f64> ↔ Signal<String> ──
        let formatted = format_value(value_signal.read(), decimals);
        let text_signal = Signal::new(formatted);

        // ── Container (flex row) ──
        let mut events = EventHandler::new();
        if !disabled {
            let key_val = value_signal.clone();
            events = events.on_key_down(move |key: Key, mods: Modifiers| -> bool {
                let delta = if mods.shift { large_step } else { step };
                match key {
                    Key::ArrowUp => {
                        let v = clamp_to_step(key_val.read() + delta, min, max, step);
                        key_val.set(v);
                        true
                    }
                    Key::ArrowDown => {
                        let v = clamp_to_step(key_val.read() - delta, min, max, step);
                        key_val.set(v);
                        true
                    }
                    _ => false,
                }
            });
            let scroll_val = value_signal.clone();
            events = events.on_scroll(move |_dx: f32, dy: f32| -> bool {
                if dy.abs() < 0.5 {
                    return false;
                }
                let delta = if dy > 0.0 { step } else { -step };
                let v = clamp_to_step(scroll_val.read() + delta, min, max, step);
                scroll_val.set(v);
                true
            });
        }

        let container_layout = LayoutConfig {
            gap: 4.0,
            padding: Padding::all(0.0),
            ..LayoutConfig::default()
        };
        let container_id = ElementBuilder::new()
            .with_components(component_mask)
            .layout(container_layout)
            .interaction(InteractionConfig {
                events: Some(events),
                ..InteractionConfig::default()
            })
            .build(ctx);
        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_accessible_role(accesskit::Role::Group);
            el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
            if let Some(fs) = self.style.font_size {
                el.set_font_size(fs);
            }
        }

        // External → TextInput: when value signal changes, update displayed text
        {
            let ext_val_for_closure = value_signal.clone();
            let ext_text = text_signal.clone();
            let guard = external_sync_guard.clone();
            crate::core::signal_bridge::subscribe_owned(container_id, &value_signal, move || {
                guard.set(true);
                let v = ext_val_for_closure.read();
                let s = format_value(v, decimals);
                let current = ext_text.read();
                // Don't overwrite user edits that parse to the same value
                let same_val =
                    parse_value(&current).is_some_and(|pv| (pv - v).abs() < f64::EPSILON);
                if !same_val && s != current {
                    ext_text.set(s);
                }
                guard.set(false);
            });
        }

        // ── Decrement button ──
        let dec_val = value_signal.clone();
        let dec_btn: Box<dyn Widget> = if disabled {
            Box::new(Button::new("\u{2212}").disabled())
        } else {
            Box::new(Button::new("\u{2212}").on_click(move || {
                let v = clamp_to_step(dec_val.read() - step, min, max, step);
                dec_val.set(v);
            }))
        };
        let dec_id = dec_btn.mount_box(&mut ctx.child_with_events(container_id));
        ctx.arena.add_child(container_id, dec_id);

        // ── TextInput ──
        let on_change_val = value_signal.clone();
        let on_change_guard = external_sync_guard.clone();
        let on_change_text = text_signal.clone();
        let mut text_input = TextInput::new(text_signal.clone())
            .input_type(TextInputType::Text)
            .placeholder(placeholder)
            .on_value_changed(move |s: String| {
                if on_change_guard.get() {
                    return; // skip — this change came from external sync, not user input
                }
                if let Some(v) = parse_value(&s) {
                    // During editing, store the raw value without clamping —
                    // the user should be able to freely type e.g. "3.1" without
                    // it being snapped back to "3.10". Clamping occurs on submit
                    // and via Arrow keys / +/- buttons.
                    on_change_val.set(v);
                } else {
                    let filtered: String = s
                        .chars()
                        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                        .collect();
                    if filtered != s {
                        on_change_guard.set(true);
                        on_change_text.set(filtered);
                        on_change_guard.set(false);
                    } else if !filtered.is_empty()
                        && filtered != "-"
                        && filtered != "."
                        && filtered != "-."
                    {
                        on_change_guard.set(true);
                        on_change_text.set(format_value(on_change_val.read(), decimals));
                        on_change_guard.set(false);
                    }
                }
            })
            .on_submit({
                let tv = value_signal.clone();
                move |s: String| {
                    if let Some(v) = parse_value(&s) {
                        let clamped = clamp_to_step(v, min, max, step);
                        tv.set(clamped);
                    }
                }
            });
        if disabled {
            text_input = text_input.disabled();
        }

        let input_id = Box::new(text_input).mount_box(&mut ctx.child_with_events(container_id));
        ctx.arena.add_child(container_id, input_id);

        // ── Blur: validate and clamp on focus loss ──
        {
            let blur_val = value_signal.clone();
            let blur_text = text_signal.clone();
            let blur_guard = external_sync_guard.clone();
            let blur_events = EventHandler::new().on_focus_out(move |_reason| {
                blur_guard.set(true);
                let text = blur_text.read();
                if let Some(v) = parse_value(&text) {
                    let clamped = clamp_to_step(v, min, max, step);
                    blur_val.set(clamped);
                    blur_text.set(format_value(clamped, decimals));
                } else {
                    // Invalid or empty → reset to last valid value
                    blur_text.set(format_value(blur_val.read(), decimals));
                }
                blur_guard.set(false);
            });
            if let Some(reg) = ctx.event_registry.as_mut() {
                blur_events.register_all(reg, input_id);
            }
        }

        // ── Increment button ──
        let inc_val = value_signal.clone();
        let inc_btn: Box<dyn Widget> = if disabled {
            Box::new(Button::new("+").disabled())
        } else {
            Box::new(Button::new("+").on_click(move || {
                let v = clamp_to_step(inc_val.read() + step, min, max, step);
                inc_val.set(v);
            }))
        };
        let inc_id = inc_btn.mount_box(&mut ctx.child_with_events(container_id));
        ctx.arena.add_child(container_id, inc_id);

        // ── Accessibility ──
        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_accessible_label(format!(
                "Number input, value {}, minimum {}, maximum {}",
                format_value(value_signal.read(), decimals),
                min,
                max,
            ));
        }

        container_id
    }
}

impl std::fmt::Debug for NumberInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NumberInput")
            .field("min", &self.min)
            .field("max", &self.max)
            .field("step", &self.step)
            .field("decimals", &self.decimals)
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}
