//! Declarative widget configuration and automatic property distribution.
//!
//! Widgets declare four configs.  `ElementBuilder` translates them into
//! concrete subsystem calls (layout, events, accessibility, paint).

use crate::core::context::MountContext;
use crate::core::element::{with_ct_mut, ElementId};
use crate::event::action::{Action, ActionOutcome};
use crate::event::types::{GesturePhase, Key, Modifiers};
use crate::event::FocusReason;
use crate::event::{DragAxis, DragData, DropType};
use crate::platform::CursorIcon;
use crate::style::{
    Alignment, Color, Dimension, LinearGradient, Margin, Padding, Point, TextAlign,
};
use std::rc::Rc;

/// Whether flex children wrap to the next line when they overflow.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

/// How content that exceeds the element's bounds is handled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub enum Overflow {
    Visible,
    Clip,
    Scroll,
}

/// How screen-reader announcements are handled for dynamic content.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AriaLive {
    #[default]
    Off,
    Polite,
    Assertive,
}

/// When scrollbars are visible on a scrollable container.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScrollbarPolicy {
    Always,
    Auto,
    Never,
}

/// Interaction state bitmask for widgets.
///
/// Tracks hover, press, focus, disabled, loading, and other states.
/// Used by the theme system to resolve dynamic styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct StateFlags(pub(crate) u16);

impl StateFlags {
    pub const NONE: Self = Self(0);
    pub const HOVERED: Self = Self(1 << 0);
    pub const PRESSED: Self = Self(1 << 1);
    pub const FOCUSED: Self = Self(1 << 2);
    pub const DISABLED: Self = Self(1 << 3);
    pub const LOADING: Self = Self(1 << 4);
    pub const INVALID: Self = Self(1 << 5);
    pub const INDETERMINATE: Self = Self(1 << 6);
    pub const CHECKED: Self = Self(1 << 7);
    pub const DRAG_OVER: Self = Self(1 << 8);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn set(&mut self, flag: Self, on: bool) {
        if on {
            self.0 |= flag.0;
        } else {
            self.0 &= !flag.0;
        }
    }
}

impl std::ops::BitOr for StateFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for StateFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Taffy layout properties for an element.
///
/// Controls size, padding, margin, flex behaviour, overflow, alignment,
/// grid columns, and scrollbar visibility.  Passed to [`ElementBuilder`].
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub min_height: Dimension,
    pub max_width: Dimension,
    pub max_height: Dimension,
    pub padding: Padding,
    pub margin: Margin,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub flex_wrap: FlexWrap,
    pub gap: f32,
    pub alignment: Alignment,
    pub overflow: Overflow,
    pub aspect_ratio: Option<f32>,
    pub tab_index: Option<usize>,
    pub order: i32,
    pub scrollbar_policy: ScrollbarPolicy,
    pub accepts_mouse: bool,
    pub content_align: Alignment,
    pub grid_columns: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: Padding::ZERO,
            margin: Margin::ZERO,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            flex_wrap: FlexWrap::NoWrap,
            gap: 0.0,
            alignment: Alignment::Start,
            overflow: Overflow::Visible,
            aspect_ratio: None,
            tab_index: None,
            order: 0,
            scrollbar_policy: ScrollbarPolicy::Auto,
            accepts_mouse: true,
            content_align: Alignment::Start,
            grid_columns: 0,
        }
    }
}

/// Declarative event handler collection.
///
/// Replaces ad-hoc `reg.on_click()` / `reg.register_drag_update()` / `Sense`
/// A collection of event callbacks for a widget.
///
/// Each field is an optional closure.  When the widget receives an event
/// of the matching kind, the closure fires.  All fields default to `None`.
///
/// Build with [`EventHandler::new`] and the builder methods, then pass
/// through [`ElementBuilder::interaction`].
#[derive(Default)]
pub struct EventHandler {
    pub on_click: Option<Box<dyn Fn()>>,
    pub on_click_at: Option<Box<dyn FnMut(Point)>>,
    pub on_click_with_mods: Option<Box<dyn FnMut(Modifiers)>>,
    pub on_click_at_with_mods: Option<Box<dyn FnMut(Point, Modifiers)>>,
    pub on_hover_enter: Option<Box<dyn Fn()>>,
    pub on_hover_leave: Option<Box<dyn Fn()>>,
    pub on_focus_in: Option<Box<dyn Fn(FocusReason)>>,
    pub on_focus_out: Option<Box<dyn Fn(FocusReason)>>,
    pub on_double_click: Option<Box<dyn Fn()>>,
    pub on_triple_click: Option<Box<dyn Fn()>>,
    pub on_long_press: Option<Box<dyn Fn()>>,
    pub on_drag_start: Option<Box<dyn FnMut(Point, Point)>>,
    pub on_drag_update: Option<Box<dyn FnMut(Point, Point)>>,
    pub on_drag_end: Option<Box<dyn FnMut(Point, Point)>>,
    pub on_key_down: Option<Box<dyn FnMut(Key, Modifiers) -> bool>>,
    pub on_key_up: Option<Box<dyn FnMut(Key, Modifiers) -> bool>>,
    pub on_scroll: Option<Box<dyn FnMut(f32, f32) -> bool>>,
    pub on_pinch: Option<Box<dyn FnMut(f64, GesturePhase) -> bool>>,
    pub on_rotate: Option<Box<dyn FnMut(f32, GesturePhase) -> bool>>,
    pub on_resize: Option<Box<dyn FnMut(f32, f32)>>,
    pub on_text_input: Option<Box<dyn FnMut(char)>>,
    pub on_preedit: Option<Box<dyn FnMut(String, Option<(usize, usize)>)>>,
    pub on_ime_delete_surrounding: Option<Box<dyn FnMut(usize, usize)>>,
    pub on_ime_commit: Option<Box<dyn FnMut(String)>>,
    pub on_action: Option<Box<dyn FnMut(&Action) -> ActionOutcome>>,
    pub on_clipboard_copy: Option<Box<dyn Fn() -> String>>,
    pub on_clipboard_paste: Option<Box<dyn FnMut(String)>>,
    /// How drag handlers arbitrate against taps (None = Eager default).
    pub drag_arbitration: Option<crate::event::DragArbitration>,
}

impl EventHandler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
    pub fn on_click_at(mut self, f: impl FnMut(Point) + 'static) -> Self {
        self.on_click_at = Some(Box::new(f));
        self
    }
    pub fn on_click_with_mods(mut self, f: impl FnMut(Modifiers) + 'static) -> Self {
        self.on_click_with_mods = Some(Box::new(f));
        self
    }
    pub fn on_click_at_with_mods(mut self, f: impl FnMut(Point, Modifiers) + 'static) -> Self {
        self.on_click_at_with_mods = Some(Box::new(f));
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
    pub fn on_focus_in(mut self, f: impl Fn(FocusReason) + 'static) -> Self {
        self.on_focus_in = Some(Box::new(f));
        self
    }
    pub fn on_focus_out(mut self, f: impl Fn(FocusReason) + 'static) -> Self {
        self.on_focus_out = Some(Box::new(f));
        self
    }
    pub fn on_double_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_double_click = Some(Box::new(f));
        self
    }
    pub fn on_triple_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_triple_click = Some(Box::new(f));
        self
    }
    pub fn on_long_press(mut self, f: impl Fn() + 'static) -> Self {
        self.on_long_press = Some(Box::new(f));
        self
    }
    pub fn on_drag_start(mut self, f: impl FnMut(Point, Point) + 'static) -> Self {
        self.on_drag_start = Some(Box::new(f));
        self
    }
    pub fn on_drag_update(mut self, f: impl FnMut(Point, Point) + 'static) -> Self {
        self.on_drag_update = Some(Box::new(f));
        self
    }
    pub fn on_drag_end(mut self, f: impl FnMut(Point, Point) + 'static) -> Self {
        self.on_drag_end = Some(Box::new(f));
        self
    }
    /// Opt into threshold-gated drag (tap-vs-drag disambiguation) or back
    /// to the eager zero-threshold default.
    pub fn drag_arbitration(mut self, mode: crate::event::DragArbitration) -> Self {
        self.drag_arbitration = Some(mode);
        self
    }

    /// Group all three drag callbacks (start / update / end) in a single call.
    ///
    /// Eliminates the repetitive `.on_drag_start().on_drag_update().on_drag_end()` chain.
    pub fn on_drag(
        mut self,
        start: impl FnMut(Point, Point) + 'static,
        update: impl FnMut(Point, Point) + 'static,
        end: impl FnMut(Point, Point) + 'static,
    ) -> Self {
        self.on_drag_start = Some(Box::new(start));
        self.on_drag_update = Some(Box::new(update));
        self.on_drag_end = Some(Box::new(end));
        self
    }
    pub fn on_key_down(mut self, f: impl FnMut(Key, Modifiers) -> bool + 'static) -> Self {
        self.on_key_down = Some(Box::new(f));
        self
    }
    pub fn on_key_up(mut self, f: impl FnMut(Key, Modifiers) -> bool + 'static) -> Self {
        self.on_key_up = Some(Box::new(f));
        self
    }
    pub fn on_scroll(mut self, f: impl FnMut(f32, f32) -> bool + 'static) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }
    pub fn on_pinch(mut self, f: impl FnMut(f64, GesturePhase) -> bool + 'static) -> Self {
        self.on_pinch = Some(Box::new(f));
        self
    }
    pub fn on_rotate(mut self, f: impl FnMut(f32, GesturePhase) -> bool + 'static) -> Self {
        self.on_rotate = Some(Box::new(f));
        self
    }
    pub fn on_resize(mut self, f: impl FnMut(f32, f32) + 'static) -> Self {
        self.on_resize = Some(Box::new(f));
        self
    }
    pub fn on_text_input(mut self, f: impl FnMut(char) + 'static) -> Self {
        self.on_text_input = Some(Box::new(f));
        self
    }
    pub fn on_preedit(mut self, f: impl FnMut(String, Option<(usize, usize)>) + 'static) -> Self {
        self.on_preedit = Some(Box::new(f));
        self
    }
    pub fn on_ime_delete_surrounding(mut self, f: impl FnMut(usize, usize) + 'static) -> Self {
        self.on_ime_delete_surrounding = Some(Box::new(f));
        self
    }
    pub fn on_ime_commit(mut self, f: impl FnMut(String) + 'static) -> Self {
        self.on_ime_commit = Some(Box::new(f));
        self
    }
    pub fn on_action(mut self, f: impl FnMut(&Action) -> ActionOutcome + 'static) -> Self {
        self.on_action = Some(Box::new(f));
        self
    }
    pub fn on_clipboard_copy(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.on_clipboard_copy = Some(Box::new(f));
        self
    }
    pub fn on_clipboard_paste(mut self, f: impl FnMut(String) + 'static) -> Self {
        self.on_clipboard_paste = Some(Box::new(f));
        self
    }

    /// Register all handlers with the given EventRegistry.
    pub(crate) fn register_all(self, reg: &mut crate::event::EventRegistry, id: ElementId) {
        // Arbitration mode must land before the drag registrations so
        // ensure_drag_recognizer picks the right recognizer (a later
        // set_drag_arbitration would re-register anyway — this just
        // avoids the churn).
        if let Some(mode) = self.drag_arbitration {
            reg.set_drag_arbitration(id, mode);
        }
        if let Some(f) = self.on_click {
            reg.on_click(id, f);
        }
        if let Some(f) = self.on_click_at {
            reg.on_click_at(id, f);
        }
        if let Some(f) = self.on_click_with_mods {
            reg.on_click_with_mods(id, f);
        }
        if let Some(f) = self.on_click_at_with_mods {
            reg.on_click_at_with_mods(id, f);
        }
        if let Some(f) = self.on_hover_enter {
            reg.on_hover_enter(id, f);
        }
        if let Some(f) = self.on_hover_leave {
            reg.on_hover_leave(id, f);
        }
        if let Some(f) = self.on_focus_in {
            reg.on_focus_in(id, f);
        }
        if let Some(f) = self.on_focus_out {
            reg.on_focus_out(id, f);
        }
        if let Some(f) = self.on_double_click {
            reg.on_double_click(id, f);
        }
        if let Some(f) = self.on_triple_click {
            reg.on_triple_click(id, f);
        }
        if let Some(f) = self.on_long_press {
            reg.on_long_press(id, f);
        }
        if let Some(f) = self.on_drag_start {
            reg.register_drag_start(id, f);
        }
        if let Some(f) = self.on_drag_update {
            reg.register_drag_update(id, f);
        }
        if let Some(f) = self.on_drag_end {
            reg.register_drag_end(id, f);
        }
        if let Some(f) = self.on_key_down {
            reg.on_key_down(id, f);
        }
        if let Some(f) = self.on_key_up {
            reg.on_key_up(id, f);
        }
        if let Some(f) = self.on_scroll {
            reg.on_scroll(id, f);
        }
        if let Some(f) = self.on_pinch {
            reg.on_pinch(id, f);
        }
        if let Some(f) = self.on_rotate {
            reg.on_rotate(id, f);
        }
        if let Some(f) = self.on_resize {
            reg.on_resize(id, f);
        }
        if let Some(f) = self.on_text_input {
            reg.register_text_input(id, f);
        }
        if let Some(f) = self.on_preedit {
            reg.register_preedit(
                id,
                f,
                std::rc::Rc::new(std::cell::RefCell::new(String::new())),
            );
        }
        if let Some(f) = self.on_ime_delete_surrounding {
            reg.register_ime_delete(id, f);
        }
        if let Some(f) = self.on_ime_commit {
            reg.register_ime_commit(id, f);
        }
        if let Some(f) = self.on_action {
            reg.on_action(id, f);
        }
        if let Some(f) = self.on_clipboard_copy {
            reg.register_clipboard_copy(id, f);
        }
        if let Some(f) = self.on_clipboard_paste {
            reg.register_clipboard_paste(id, f);
        }

        // Arena registration is SUNK into the EventRegistry registration
        // paths themselves (register_drag_* / on_long_press) so imperative
        // widget registrations get recognizers too — audit 2026-07-19:
        // the explicit block here only covered the declarative path, and
        // the old single-slot registry made drag + long-press overwrite
        // each other.
    }
}

/// Interaction and focus configuration for a widget.
///
/// Wraps an optional [`EventHandler`] plus properties for focusability,
/// autofocus, cursor, drag-and-drop, input validation, and event blocking.
pub struct InteractionConfig {
    /// Declarative event handlers — automatically registered during `ElementBuilder::build()`.
    pub events: Option<EventHandler>,
    pub enabled: bool,
    /// Whether this element can receive keyboard focus (was FocusPolicy::Strong / Click).
    pub focusable: bool,
    /// Request autofocus when the element is mounted (and focusable).
    /// Eliminates manual `reg.request_autofocus(id)` in each widget.
    pub autofocus: bool,
    pub cursor: CursorIcon,
    pub block_events: bool,
    pub input_pass_through: bool,
    pub draggable: bool,
    pub drag_data: Option<crate::event::DragData>,
    pub drop_target: bool,
    pub accept_drop_types: Vec<DropType>,
    pub on_drop: Option<Box<dyn Fn(DragData)>>,
    pub drag_axis: DragAxis,
    pub on_drag_start: Option<Rc<dyn Fn() -> DragData>>,
    pub max_length: Option<usize>,
    pub validation: Option<Rc<dyn Fn(&str) -> bool>>,
}

impl std::fmt::Debug for InteractionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteractionConfig")
            .field("draggable", &self.draggable)
            .field("drop_target", &self.drop_target)
            .field("on_drop", &self.on_drop.as_ref().map(|_| "Some(fn)"))
            .finish()
    }
}

impl InteractionConfig {
    /// Convenience: disable interaction and focusability in one call.
    /// Sets `enabled = false` and `focusable = false`.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self.focusable = false;
        self
    }
}

impl Default for InteractionConfig {
    fn default() -> Self {
        Self {
            events: None,
            enabled: true,
            focusable: false,
            autofocus: false,
            cursor: CursorIcon::DEFAULT,
            block_events: false,
            input_pass_through: false,
            draggable: false,
            drag_data: None,
            drop_target: false,
            accept_drop_types: Vec::new(),
            on_drop: None,
            drag_axis: DragAxis::Free,
            on_drag_start: None,
            max_length: None,
            validation: None,
        }
    }
}

/// Accessibility properties for a widget.
///
/// Carries an AccessKit role and an optional accessible label.  These
/// are pushed to the platform accessibility tree every frame.
pub struct AccessibilityConfig {
    pub role: accesskit::Role,
    pub label: Option<String>,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            role: accesskit::Role::Unknown,
            label: None,
        }
    }
}

/// Visual paint properties for a widget.
///
/// Covers background, borders, outline, corner radius, opacity,
/// blend mode, shadow, gradient, and text styling.
pub struct PaintConfig {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub border_width: f32,
    pub border_color: Option<Color>,
    pub outline_width: f32,
    pub outline_color: Option<Color>,
    pub corner_radius: crate::style::CornerRadii,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub text_decoration: crate::style::styled::TextDecoration,
    pub text_overflow: crate::style::styled::TextOverflow,
    pub shadow: Option<crate::style::styled::Shadow>,
    pub gradient: Option<LinearGradient>,
    pub z_index: i32,
    pub z_index_floor: Option<i32>,
    pub text_align: TextAlign,
    pub font_family: Option<String>,
    pub placeholder_color: Option<Color>,
    pub text_direction: crate::style::TextDirection,
    /// Declarative state→style overrides for interactive widgets.
    pub state_style: Option<crate::style::StateStyle>,
}

impl Default for PaintConfig {
    fn default() -> Self {
        Self {
            background: None,
            foreground: None,
            border_width: 0.0,
            border_color: None,
            outline_width: 0.0,
            outline_color: None,
            corner_radius: crate::style::CornerRadii::all(4.0),
            font_size: 18.0,
            font_weight: 400,
            line_height: 1.5,
            text_decoration: crate::style::styled::TextDecoration::None,
            text_overflow: crate::style::styled::TextOverflow::Clip,
            shadow: None,
            z_index: 0,
            z_index_floor: None,
            gradient: None,
            text_align: TextAlign::Start,
            font_family: None,
            placeholder_color: None,
            text_direction: crate::style::TextDirection::Ltr,
            state_style: None,
        }
    }
}

/// Declarative element construction helper.
///
/// Collects layout, paint, interaction, and accessibility configuration
/// and applies them all to a freshly allocated element in one call to
/// [`build`](Self::build).  Most widgets create an `ElementBuilder` in
/// their `mount_box` implementation.
pub struct ElementBuilder {
    layout: LayoutConfig,
    interaction: InteractionConfig,
    paint: PaintConfig,
    accessible_role: Option<accesskit::Role>,
    accessible_label: Option<String>,
    /// Bitmask of ECS components this widget declares. Set via
    /// [`with_components`] before calling [`build`].  Pre-allocates
    /// entries in `ComponentTables` so the element is visible to O(k)
    /// component-filtered queries even before any setter has run.
    /// Defaults to 0 (no pre-allocation).
    component_mask: u64,
    test_id: Option<String>,
    name: Option<String>,
}

impl ElementBuilder {
    pub fn new() -> Self {
        Self {
            layout: LayoutConfig::default(),
            interaction: InteractionConfig::default(),
            paint: PaintConfig::default(),
            accessible_role: None,
            accessible_label: None,
            component_mask: 0,
            test_id: None,
            name: None,
        }
    }

    /// Declare which ECS components this element uses.
    /// Components declared here are pre-allocated during [`build`],
    /// ensuring the element is visible to O(k) component-filtered
    /// queries from the moment of creation.
    ///
    /// Each widget that knows its component set at construction time
    /// should call this — see `Widget::component_mask()`.
    pub fn with_components(mut self, mask: u64) -> Self {
        self.component_mask = mask;
        self
    }

    fn pixels_from_dim(&self, d: Dimension) -> f32 {
        match d {
            Dimension::Pixels(px) => px,
            Dimension::Percent(_) | Dimension::Auto => 0.0,
        }
    }

    pub fn layout(mut self, config: LayoutConfig) -> Self {
        self.layout = config;
        self
    }
    pub fn interaction(mut self, config: InteractionConfig) -> Self {
        self.interaction = config;
        self
    }
    pub fn paint(mut self, config: PaintConfig) -> Self {
        self.paint = config;
        self
    }
    /// Request autofocus when this element is mounted.
    /// Shorthand for `.interaction(InteractionConfig { autofocus: true, ..})`.
    pub fn autofocus(mut self) -> Self {
        self.interaction.autofocus = true;
        self
    }

    pub fn accessibility(mut self, role: accesskit::Role, label: impl Into<String>) -> Self {
        self.accessible_role = Some(role);
        self.accessible_label = Some(label.into());
        self
    }

    pub fn test_id(mut self, id: impl Into<String>) -> Self {
        self.test_id = Some(id.into());
        self
    }
    pub fn name(mut self, n: impl Into<String>) -> Self {
        self.name = Some(n.into());
        self
    }

    pub fn build(self, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        // Pre-allocate component table entries so the element is visible
        // to O(k) component-filtered queries from the moment of creation.
        ctx.preallocate(id, self.component_mask);
        let el = ctx.arena.get_mut(id).unwrap();

        // Invalidate focus order cache on new element creation.
        crate::core::dirty_registry::invalidate_focus_order();
        crate::core::dirty_registry::mark_a11y_dirty();

        if let Some(bg) = self.paint.background {
            el.set_background(bg);
        }
        if let Some(ref ss) = self.paint.state_style {
            with_ct_mut(|ct| {
                ct.style.entry(id).or_default().state_style = Some(ss.clone());
            });
        }
        if let Some(fg) = self.paint.foreground {
            el.set_foreground(fg);
        }
        if let Some(bc) = self.paint.border_color {
            el.set_border_color(bc);
        }
        if let Some(oc) = self.paint.outline_color {
            el.set_outline_color(oc);
        }
        if let Some(role) = self.accessible_role {
            el.set_accessible_role(role);
        }
        if let Some(label) = self.accessible_label.clone() {
            el.set_accessible_label(label);
        }
        if let Some(ref id) = self.test_id {
            el.set_test_id(id.clone());
        }
        if let Some(ref n) = self.name {
            el.set_name(n.clone());
        }

        el.set_border_width(self.paint.border_width);
        el.set_outline_width(self.paint.outline_width);
        el.set_focusable(self.interaction.focusable);
        // Autofocus: queue element for focus on next frame.
        if self.interaction.autofocus && self.interaction.focusable {
            if let Some(reg) = ctx.event_registry.as_mut() {
                reg.request_autofocus(id);
            }
        }
        el.set_input_pass_through(self.interaction.input_pass_through);
        el.set_corner_radii(self.paint.corner_radius);
        el.set_font_size(self.paint.font_size);
        el.set_font_weight(self.paint.font_weight);
        el.set_line_height(self.paint.line_height);
        el.set_text_align(self.paint.text_align);
        el.set_font_family(self.paint.font_family.clone());
        el.set_text_decoration(self.paint.text_decoration);
        el.set_text_overflow(self.paint.text_overflow);
        el.set_shadow(self.paint.shadow);
        el.set_gradient(self.paint.gradient);
        el.set_z_index(self.paint.z_index);
        el.z_index_floor = self.paint.z_index_floor;
        if let Some(c) = self.paint.placeholder_color {
            el.set_placeholder_color(c);
        }
        el.set_text_direction(self.paint.text_direction);
        el.set_preferred_height(match self.layout.height {
            Dimension::Pixels(px) => px,
            _ => 36.0,
        });
        // Store original Dimension for percent-aware taffy bridge
        if self.layout.width != Dimension::Auto {
            el.set_width_dim(Some(self.layout.width));
        }
        el.set_height_dim(self.layout.height);
        if let Dimension::Pixels(px) = self.layout.width {
            el.set_preferred_width(Some(px));
        }
        el.set_gap(self.layout.gap);
        el.set_flex_wrap(self.layout.flex_wrap);
        el.set_flex_grow(self.layout.flex_grow);
        el.set_flex_shrink(self.layout.flex_shrink);
        el.set_flex_basis(self.pixels_from_dim(self.layout.flex_basis));
        el.set_flex_basis_dim(self.layout.flex_basis);
        el.set_overflow(self.layout.overflow);
        el.set_aspect_ratio(self.layout.aspect_ratio);
        el.set_tab_index(self.layout.tab_index);
        el.set_order(self.layout.order);
        el.set_scrollbar_policy(self.layout.scrollbar_policy);
        el.set_accepts_mouse(self.layout.accepts_mouse);
        el.set_content_align(self.layout.content_align);
        el.set_margin(self.layout.margin);
        el.set_padding(self.layout.padding);
        el.set_alignment(self.layout.alignment);
        if self.layout.grid_columns > 0 {
            el.set_grid_columns(self.layout.grid_columns);
            el.set_affected_by_child_size(false);
        }

        let InteractionConfig {
            events,
            draggable,
            drag_data,
            drop_target,
            on_drop,
            cursor,
            drag_axis,
            on_drag_start,
            max_length,
            validation,
            ..
        } = self.interaction;
        el.set_draggable(draggable);
        el.set_drag_data(drag_data);
        el.set_drop_target(drop_target);
        if let Some(handler) = on_drop {
            el.set_on_drop(handler);
        }
        el.set_cursor_icon(Some(cursor));
        el.set_drag_axis(drag_axis);
        if let Some(f) = on_drag_start {
            el.set_on_drag_start(f);
        }
        if let Some(v) = max_length {
            el.set_max_length(Some(v));
        }
        if let Some(f) = validation {
            el.set_validation(Some(f));
        }

        // Auto-register all declarative event handlers.
        if let Some(events) = events {
            if let Some(reg) = ctx.event_registry.as_mut() {
                events.register_all(reg, id);
            }
        }

        id
    }
}

impl Default for ElementBuilder {
    fn default() -> Self {
        Self::new()
    }
}
