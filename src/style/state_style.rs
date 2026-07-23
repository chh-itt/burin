//! Declarative state→style mapping for interactive widgets.
//!
//! Widgets declare per-state visual overrides once during mount (via
//! `StateStyle`), and the framework automatically resolves the correct
//! style at paint time — no manual `set_state_dirty` + `with_ct_mut`
//! that can be forgotten.
//!
//! ## Resolution priority (highest first)
//!
//! ```text
//! DISABLED → PRESSED → CHECKED → HOVERED → FOCUSED
//!   → LOADING → INVALID → INDETERMINATE → DRAG_OVER
//!     → Base style (StyleComponent fields)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! el.with_state_style(|ss| {
//!     ss.hovered.background = Some(hover_bg);
//!     ss.pressed.background = Some(pressed_bg);
//!     ss.disabled.background = Some(disabled_bg);
//!     ss.disabled.foreground = Some(disabled_fg);
//!     ss.checked.background = Some(checked_bg);
//! });
//! ```

use crate::core::config::StateFlags;
use crate::style::styled::Shadow;
use crate::style::Color;

/// Per-state visual overrides for an element.
///
/// Each field contains the style values to use when the element
/// has the corresponding `StateFlags` bit set. `None` means
/// "no override for this property in this state" — the base
/// `StyleComponent` value is used instead.
///
/// `animated` has highest priority — used by the animation system
/// to interpolate between state transitions.
#[derive(Debug, Clone, Default)]
pub struct StateStyle {
    pub animated: StyleVariant,
    pub hovered: StyleVariant,
    pub pressed: StyleVariant,
    pub focused: StyleVariant,
    pub disabled: StyleVariant,
    pub checked: StyleVariant,
    pub loading: StyleVariant,
    pub invalid: StyleVariant,
    pub indeterminate: StyleVariant,
    pub drag_over: StyleVariant,
}

/// Visual overrides for a single interaction state.
#[derive(Debug, Clone, Default)]
pub struct StyleVariant {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub opacity: Option<f32>,
    pub shadow: Option<Shadow>,
    pub corner_radius: Option<crate::style::CornerRadii>,
}

impl StyleVariant {
    /// True when this variant overrides nothing — entering/leaving the
    /// corresponding state cannot change the element's resolved visuals.
    pub fn is_empty(&self) -> bool {
        self.background.is_none()
            && self.foreground.is_none()
            && self.border_color.is_none()
            && self.border_width.is_none()
            && self.opacity.is_none()
            && self.shadow.is_none()
            && self.corner_radius.is_none()
    }
}

/// The fully resolved visual style for the current frame.
///
/// Returned by `StyleComponent::resolve_style()` and consumed
/// by `paint_element_surface`.
#[derive(Debug, Clone)]
pub struct ResolvedStyle {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: f32,
    pub opacity: f32,
    pub shadow: Option<Shadow>,
    pub outline_color: Option<Color>,
    pub outline_width: f32,
    pub corner_radius: crate::style::CornerRadii,
    pub gradient: Option<crate::style::LinearGradient>,
    pub backdrop: bool,
    pub text_decoration: crate::style::styled::TextDecoration,
    pub text_overflow: crate::style::styled::TextOverflow,
    pub blend_mode: u8,
    pub backdrop_filter: Option<crate::style::styled::BackdropFilter>,
}

/// Resolve a property using state priority.
///
/// **ANIMATED has highest priority** — used by the animation system
/// for smooth state transitions.
///
/// **DISABLED is a hard gate**: when set, all other interaction states
/// (hover, press, focus, checked, etc.) are suppressed — just like
/// Flutter's `WidgetState.disabled` semantics.
///
/// Priority chain:
///   ANIMATED (highest) → DISABLED (hard gate) → PRESSED → CHECKED
///   → HOVERED → FOCUSED → LOADING → INVALID → INDETERMINATE
///   → DRAG_OVER → base
fn resolve_property<T: Copy>(
    state: StateFlags,
    ss: Option<&StateStyle>,
    accessor: impl Fn(&StyleVariant) -> Option<T>,
    base: Option<T>,
) -> Option<T> {
    let ss = match ss {
        Some(s) => s,
        None => return base,
    };

    // ANIMATED has highest priority — animation system writes intermediate values here.
    if let Some(v) = accessor(&ss.animated) {
        return Some(v);
    }

    // DISABLED suppresses all other interaction states.
    if state.contains(StateFlags::DISABLED) {
        if let Some(v) = accessor(&ss.disabled) {
            return Some(v);
        }
        return base;
    }

    // Normal priority chain
    if state.contains(StateFlags::PRESSED) {
        if let Some(v) = accessor(&ss.pressed) {
            return Some(v);
        }
    }
    if state.contains(StateFlags::CHECKED) {
        if let Some(v) = accessor(&ss.checked) {
            return Some(v);
        }
    }
    if state.contains(StateFlags::HOVERED) {
        if let Some(v) = accessor(&ss.hovered) {
            return Some(v);
        }
    }
    if state.contains(StateFlags::FOCUSED) {
        if let Some(v) = accessor(&ss.focused) {
            return Some(v);
        }
    }
    if state.contains(StateFlags::LOADING) {
        if let Some(v) = accessor(&ss.loading) {
            return Some(v);
        }
    }
    if state.contains(StateFlags::INVALID) {
        if let Some(v) = accessor(&ss.invalid) {
            return Some(v);
        }
    }
    if state.contains(StateFlags::INDETERMINATE) {
        if let Some(v) = accessor(&ss.indeterminate) {
            return Some(v);
        }
    }
    if state.contains(StateFlags::DRAG_OVER) {
        if let Some(v) = accessor(&ss.drag_over) {
            return Some(v);
        }
    }

    base
}

impl StateStyle {
    /// Builder: set a hover background.
    pub fn hovered_bg(mut self, c: Color) -> Self {
        self.hovered.background = Some(c);
        self
    }
    /// Builder: set a pressed background.
    pub fn pressed_bg(mut self, c: Color) -> Self {
        self.pressed.background = Some(c);
        self
    }
    /// Builder: set a focused background.
    pub fn focused_bg(mut self, c: Color) -> Self {
        self.focused.background = Some(c);
        self
    }
    /// Builder: set a disabled background and foreground.
    pub fn disabled_style(mut self, bg: Color, fg: Color) -> Self {
        self.disabled.background = Some(bg);
        self.disabled.foreground = Some(fg);
        self
    }
    /// Builder: set a checked background.
    pub fn checked_bg(mut self, c: Color) -> Self {
        self.checked.background = Some(c);
        self
    }
    /// Builder: set a disabled foreground.
    pub fn disabled_fg(mut self, c: Color) -> Self {
        self.disabled.foreground = Some(c);
        self
    }
    /// Builder: set a disabled border color.
    pub fn disabled_border(mut self, c: Color) -> Self {
        self.disabled.border_color = Some(c);
        self
    }
    /// Builder: set an invalid border color.
    pub fn invalid_border(mut self, c: Color) -> Self {
        self.invalid.border_color = Some(c);
        self
    }
}

/// Full style resolution, called in `paint_element_surface`.
pub fn resolve_style(
    state: StateFlags,
    style: &crate::ecs::components::StyleComponent,
) -> ResolvedStyle {
    let ss = style.state_style.as_ref();

    let background = resolve_property(state, ss, |v| v.background, style.background);
    let foreground = resolve_property(state, ss, |v| v.foreground, style.foreground);
    let border_color = resolve_property(state, ss, |v| v.border_color, style.border_color);
    let border_width = resolve_property(state, ss, |v| v.border_width, Some(style.border_width))
        .unwrap_or(style.border_width);
    let opacity =
        resolve_property(state, ss, |v| v.opacity, Some(style.opacity)).unwrap_or(style.opacity);
    let shadow = resolve_property(state, ss, |v| v.shadow, style.shadow);
    let corner_radius = resolve_property(state, ss, |v| v.corner_radius, Some(style.corners()))
        .unwrap_or(style.corners());

    ResolvedStyle {
        background,
        foreground,
        border_color,
        border_width,
        opacity,
        shadow,
        outline_color: style.outline_color,
        outline_width: style.outline_width,
        corner_radius,
        gradient: style.gradient,
        backdrop: style.backdrop,
        text_decoration: style.text_decoration,
        text_overflow: style.text_overflow,
        blend_mode: style.blend_mode,
        backdrop_filter: style.backdrop_filter,
    }
}
