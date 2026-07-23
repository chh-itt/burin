//! Component type definitions and bitmask constants.
//!
//! ComponentTables stores each component type in its own HashMap for O(1)
//! per-type access and O(k) type-filtered iteration.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use crate::animation::{AnimatedProperty, AnimatedValue, AnimationConfig, TransitionConfig};
use crate::core::config::{AriaLive, FlexWrap, Overflow, ScrollbarPolicy};
use crate::core::element::{ElementArena, LazyFontParams};
use crate::core::ElementId;
use crate::core::LayoutDirection;
use crate::event::{DragAxis, DragData, DropType};
use crate::style::styled::{Shadow, TextDecoration, TextOverflow};
use crate::style::LinearGradient;
use crate::style::StateStyle;
use crate::style::{
    Alignment, Color, CornerRadii, Margin, Padding, Rect, TextAlign, TextDirection,
    TooltipPlacement, Vec2,
};

// ── Bitmask constants ──

pub const STYLE: u64 = 1 << 0;
pub const LAYOUT: u64 = 1 << 1;
pub const INTERACTION: u64 = 1 << 2;
pub const TEXT: u64 = 1 << 3;
pub const SCROLL: u64 = 1 << 4;
pub const CURSOR: u64 = 1 << 5;
pub const TOOLTIP: u64 = 1 << 6;
pub const DRAG_DROP: u64 = 1 << 7;
pub const ANIMATION: u64 = 1 << 8;
pub const TRANSFORM: u64 = 1 << 9;
pub const ACCESSIBLE: u64 = 1 << 10;
pub const LIFECYCLE: u64 = 1 << 11;

/// Component trait for type-safe table dispatch.
pub trait Component: Clone + 'static {
    const BIT: u64;
}

macro_rules! component_impl {
    ($name:ident, $bit:ident) => {
        impl Component for $name {
            const BIT: u64 = $bit;
        }
    };
}

// ── 1. StyleComponent [bit 0] ──

#[derive(Clone)]
pub struct StyleComponent {
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub border_width: f32,
    pub border_color: Option<Color>,
    pub outline_width: f32,
    pub outline_color: Option<Color>,
    pub corner_radius: f32,
    /// Per-corner override. When set, takes priority over `corner_radius`
    /// for background/border painting. Outline and shadow still use the
    /// uniform `corner_radius` value.
    pub corner_radii: Option<CornerRadii>,
    pub shadow: Option<Shadow>,
    pub gradient: Option<LinearGradient>,
    pub text_decoration: TextDecoration,
    pub text_overflow: TextOverflow,
    pub opacity: f32,
    pub backdrop: bool,
    pub blend_mode: u8,
    pub backdrop_filter: Option<crate::style::styled::BackdropFilter>,
    pub state_style: Option<StateStyle>,
}

impl Default for StyleComponent {
    fn default() -> Self {
        Self {
            background: None,
            foreground: None,
            border_width: 0.0,
            border_color: None,
            outline_width: 0.0,
            outline_color: None,
            corner_radius: 4.0,
            shadow: None,
            gradient: None,
            corner_radii: None,
            text_decoration: TextDecoration::None,
            text_overflow: TextOverflow::Clip,
            opacity: 1.0,
            backdrop: false,
            blend_mode: 0,
            backdrop_filter: None,
            state_style: None,
        }
    }
}

impl StyleComponent {
    /// Returns the effective `CornerRadii` — per-corner override if set,
    /// otherwise a uniform radius derived from `corner_radius`.
    pub fn corners(&self) -> CornerRadii {
        self.corner_radii
            .unwrap_or(CornerRadii::all(self.corner_radius))
    }
}

component_impl!(StyleComponent, STYLE);

/// Per-axis boolean: whether an element's outer size on each axis is
/// independent of its children's content. Drives relayout-boundary detection
/// for incremental layout. `x` = width axis, `y` = height axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisPair {
    pub x: bool,
    pub y: bool,
}

impl AxisPair {
    /// Both axes depend on children (conservative default).
    pub const BOTH_DEP: Self = AxisPair { x: false, y: false };
    /// Both axes set to the same value.
    pub fn both(v: bool) -> Self {
        AxisPair { x: v, y: v }
    }
}

// ── 2. LayoutComponent [bit 1] ──

#[derive(Clone)]
pub struct LayoutComponent {
    pub layout_direction: LayoutDirection,
    pub gap: f32,
    pub margin: Margin,
    pub padding: Padding,
    pub alignment: Alignment,
    pub content_align: Alignment,
    pub preferred_width: Option<f32>,
    pub preferred_height: f32,
    /// Original Dimension for width, preserved for taffy percent resolution.
    /// None = not explicitly set (falls back to preferred_width).
    pub width_dim: Option<crate::style::Dimension>,
    /// Original Dimension for height, preserved for taffy percent resolution.
    pub height_dim: crate::style::Dimension,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: f32,
    /// Original Dimension for flex_basis, preserved for taffy percent resolution.
    pub flex_basis_dim: crate::style::Dimension,
    pub flex_wrap: FlexWrap,
    pub aspect_ratio: Option<f32>,
    pub overflow: Overflow,
    pub order: i32,
    pub scrollbar_policy: ScrollbarPolicy,
    pub scrollbar_width: f32,
    pub affected_by_child_size: bool,
    /// Per-axis: is this element's outer size independent of its children's
    /// content on that axis? Cached each layout frame; drives relayout-boundary
    /// detection. `x`/`y` true => that axis can be frozen for subtree isolation.
    pub size_independent: AxisPair,
    /// Min size on the flex main axis. -1.0 = auto (content-based, default).
    /// >= 0 = fixed min size in pixels (e.g. 0 = allow shrink below content).
    pub min_main: f32,
    /// 0 = not a grid container. >0 = CSS Grid with this many equal 1fr columns.
    pub grid_columns: u32,
    /// Per-column track widths for Grid. Empty = use `grid_columns` × 1fr.
    /// Positive values = fixed px, values <= 0 = flex fraction (auto).
    pub grid_column_widths: Vec<f32>,
    /// Per-child: span this many grid columns (0 = auto).
    pub grid_column_span: u32,
    /// Per-child: skip this many grid columns before starting (0 = none).
    pub grid_column_offset: u32,
}

impl Default for LayoutComponent {
    fn default() -> Self {
        Self {
            layout_direction: LayoutDirection::Vertical,
            gap: 0.0,
            margin: Margin::ZERO,
            padding: Padding::ZERO,
            alignment: Alignment::Start,
            content_align: Alignment::Start,
            preferred_width: None,
            preferred_height: 36.0,
            width_dim: None,
            height_dim: crate::style::Dimension::Auto,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: 0.0,
            flex_basis_dim: crate::style::Dimension::Auto,
            flex_wrap: FlexWrap::NoWrap,
            aspect_ratio: None,
            overflow: Overflow::Visible,
            order: 0,
            scrollbar_policy: ScrollbarPolicy::Auto,
            scrollbar_width: 10.0,
            affected_by_child_size: true,
            size_independent: AxisPair::BOTH_DEP,
            min_main: -1.0,
            grid_columns: 0,
            grid_column_widths: Vec::new(),
            grid_column_span: 0,
            grid_column_offset: 0,
        }
    }
}

component_impl!(LayoutComponent, LAYOUT);

// ── 3. InteractionComponent [bit 2] ──

#[derive(Clone)]
pub struct InteractionComponent {
    pub focusable: bool,
    pub tab_index: Option<usize>,
    pub accepts_mouse: bool,
    pub input_pass_through: bool,
    pub read_only: bool,
    pub selected: bool,
}

impl Default for InteractionComponent {
    fn default() -> Self {
        Self {
            focusable: false,
            tab_index: None,
            accepts_mouse: true,
            input_pass_through: false,
            read_only: false,
            selected: false,
        }
    }
}

component_impl!(InteractionComponent, INTERACTION);

// ── 4. TextComponent [bit 3] ──

#[derive(Clone)]
pub struct TextComponent {
    pub text_buffer: Option<Rc<RefCell<cosmic_text::Buffer>>>,
    pub text_generation: Rc<Cell<u64>>,
    pub font_size: f32,
    pub font_weight: u16,
    pub font_family: Option<String>,
    pub text_align: TextAlign,
    pub text_direction: TextDirection,
    pub text_vertical_center: bool,
    pub line_height: f32,
    pub measured_text_width: Rc<Cell<f32>>,
    pub lazy_label: Option<Rc<Cell<String>>>,
    pub buffer_gen: Rc<Cell<u64>>,
    pub lazy_font_params: Option<Rc<LazyFontParams>>,
    pub is_placeholder: Rc<Cell<bool>>,
    pub placeholder_color: Option<Color>,
    pub selection_color: Option<Color>,
    pub caret_color: Option<Color>,
}

impl Default for TextComponent {
    fn default() -> Self {
        Self {
            text_buffer: None,
            text_generation: Rc::new(Cell::new(0)),
            font_size: 18.0,
            font_weight: 400,
            font_family: None,
            text_align: TextAlign::Start,
            text_direction: TextDirection::Ltr,
            text_vertical_center: true,
            line_height: 1.5,
            measured_text_width: Rc::new(Cell::new(0.0)),
            lazy_label: None,
            buffer_gen: Rc::new(Cell::new(0)),
            lazy_font_params: None,
            is_placeholder: Rc::new(Cell::new(false)),
            placeholder_color: None,
            selection_color: None,
            caret_color: None,
        }
    }
}

component_impl!(TextComponent, TEXT);

// ── 5. ScrollComponent [bit 4] ──

#[derive(Clone)]
pub struct ScrollComponent {
    pub scroll_offset: Rc<Cell<Vec2>>,
    pub content_bounds: Rc<Cell<Rect>>,
    pub text_scroll_x: Rc<Cell<f32>>,
    pub text_scroll_y: Rc<Cell<f32>>,
    pub max_scroll_y: Rc<Cell<f32>>,
    pub pending_scroll_to: Rc<Cell<Option<ElementId>>>,
}

impl Default for ScrollComponent {
    fn default() -> Self {
        Self {
            scroll_offset: Rc::new(Cell::new(Vec2::ZERO)),
            content_bounds: Rc::new(Cell::new(Rect::ZERO)),
            text_scroll_x: Rc::new(Cell::new(0.0)),
            text_scroll_y: Rc::new(Cell::new(0.0)),
            max_scroll_y: Rc::new(Cell::new(0.0)),
            pending_scroll_to: Rc::new(Cell::new(None)),
        }
    }
}

component_impl!(ScrollComponent, SCROLL);

// ── 6. CursorComponent [bit 5] ──

#[derive(Clone)]
pub struct CursorComponent {
    pub cursor_x: Rc<Cell<f32>>,
    pub cursor_visible: Rc<Cell<bool>>,
    pub cursor_line: Rc<Cell<usize>>,
    pub cursor_blink_last_input: Rc<Cell<Instant>>,
    pub cursor_focused: Rc<Cell<bool>>,
    pub selection_rect: Rc<Cell<Vec<Rect>>>,
    pub ime_cursor_rect: Rc<Cell<Option<Rect>>>,
    pub composition_underline_rect: Rc<Cell<Option<Rect>>>,
    pub cursor_icon: Option<crate::platform::CursorIcon>,
}

impl Default for CursorComponent {
    fn default() -> Self {
        Self {
            cursor_x: Rc::new(Cell::new(0.0)),
            cursor_visible: Rc::new(Cell::new(false)),
            cursor_line: Rc::new(Cell::new(0)),
            cursor_blink_last_input: Rc::new(Cell::new(crate::core::clock::now())),
            cursor_focused: Rc::new(Cell::new(false)),
            selection_rect: Rc::new(Cell::new(Vec::new())),
            ime_cursor_rect: Rc::new(Cell::new(None)),
            composition_underline_rect: Rc::new(Cell::new(None)),
            cursor_icon: None,
        }
    }
}

component_impl!(CursorComponent, CURSOR);

// ── 7. TooltipComponent [bit 6] ──

#[derive(Clone)]
pub struct TooltipComponent {
    pub tooltip_text: Rc<String>,
    pub tooltip_visible: Rc<Cell<bool>>,
    pub tooltip_placement: TooltipPlacement,
    pub tooltip_delay_start: Rc<Cell<Option<Instant>>>,
    pub tooltip_alpha: Rc<Cell<f32>>,
    pub tooltip_delay_ms: u64,
}

impl Default for TooltipComponent {
    fn default() -> Self {
        Self {
            tooltip_text: Rc::new(String::new()),
            tooltip_visible: Rc::new(Cell::new(false)),
            tooltip_placement: TooltipPlacement::Bottom,
            tooltip_delay_start: Rc::new(Cell::new(None)),
            tooltip_alpha: Rc::new(Cell::new(0.0)),
            tooltip_delay_ms: 300,
        }
    }
}

component_impl!(TooltipComponent, TOOLTIP);

// ── 8. DragDropComponent [bit 7] ──

#[derive(Clone)]
pub struct DragDropComponent {
    pub draggable: bool,
    pub drag_data: Option<DragData>,
    pub drag_axis: DragAxis,
    pub drop_target: bool,
    pub accept_drop_types: Vec<DropType>,
    pub max_length: Option<usize>,
    pub validation: Option<Rc<dyn Fn(&str) -> bool>>,
    pub on_drop: Option<Rc<dyn Fn(DragData)>>,
    pub on_drag_start: Option<Rc<dyn Fn() -> DragData>>,
}

impl Default for DragDropComponent {
    fn default() -> Self {
        Self {
            draggable: false,
            drag_data: None,
            drag_axis: DragAxis::Free,
            drop_target: false,
            accept_drop_types: Vec::new(),
            max_length: None,
            validation: None,
            on_drop: None,
            on_drag_start: None,
        }
    }
}

component_impl!(DragDropComponent, DRAG_DROP);

// ── 9. AnimationComponent [bit 8] ──

#[derive(Clone, Default)]
pub struct AnimationComponent {
    pub animation_config: Option<AnimationConfig>,
    pub exit_pending: Option<(AnimatedProperty, AnimatedValue, Rc<Cell<bool>>)>,
    pub transition_config: Option<Rc<TransitionConfig>>,
}

component_impl!(AnimationComponent, ANIMATION);

// ── 10. TransformComponent [bit 9] ──

#[derive(Clone)]
pub struct TransformComponent {
    pub transform: Option<[f32; 6]>,
    pub transform_origin_x: f32,
    pub transform_origin_y: f32,
    pub position_offset: Rc<Cell<Vec2>>,
    pub size_scale: Rc<Cell<Vec2>>,
}

impl Default for TransformComponent {
    fn default() -> Self {
        Self {
            transform: None,
            transform_origin_x: 0.5,
            transform_origin_y: 0.5,
            position_offset: Rc::new(Cell::new(Vec2::ZERO)),
            size_scale: Rc::new(Cell::new(Vec2::new(1.0, 1.0))),
        }
    }
}

component_impl!(TransformComponent, TRANSFORM);

// ── 11. AccessibleComponent [bit 10] ──

#[derive(Clone)]
pub struct AccessibleComponent {
    pub accessible_role: Option<accesskit::Role>,
    pub accessible_label: Option<String>,
    pub accessible_description: Option<String>,
    pub accessible_level: Option<u32>,
    pub accessible_live: AriaLive,
    pub accessible_hidden: bool,
    pub accessible_checked: Option<bool>,
    pub accessible_required: bool,
    pub accessible_value: Option<f64>,
    pub accessible_min: f64,
    pub accessible_max: f64,
    pub accessible_active_descendant: Option<crate::core::ElementId>,
}

impl Default for AccessibleComponent {
    fn default() -> Self {
        Self {
            accessible_role: None,
            accessible_label: None,
            accessible_description: None,
            accessible_level: None,
            accessible_live: AriaLive::Off,
            accessible_hidden: false,
            accessible_checked: None,
            accessible_required: false,
            accessible_value: None,
            accessible_min: 0.0,
            accessible_max: 100.0,
            accessible_active_descendant: None,
        }
    }
}

component_impl!(AccessibleComponent, ACCESSIBLE);

// ── 12. LifecycleComponent [bit 11] ──

#[derive(Default)]
pub struct LifecycleComponent {
    pub on_mount: Option<Rc<RefCell<Option<Box<dyn FnOnce()>>>>>,
    pub on_appear: Option<Rc<RefCell<Option<Box<dyn Fn()>>>>>,
    pub on_disappear: Option<Rc<RefCell<Option<Box<dyn Fn()>>>>>,
    pub on_unmount: Option<Rc<RefCell<Option<Box<dyn FnOnce()>>>>>,
    pub frame_tick: Option<Rc<RefCell<Option<Box<dyn Fn()>>>>>,
    pub reactive_visible: Option<Rc<Cell<bool>>>,
    pub name: Option<String>,
    pub debug_label: Option<String>,
    pub test_id: Option<String>,
    pub apply_drag_layout: Option<Rc<dyn Fn(&mut ElementArena, ElementId)>>,
    pub component_role: Option<crate::theme::m3::roles::ComponentRole>,
    pub invalid_hint: Option<Rc<Cell<bool>>>,
    pub error_text: Option<Rc<RefCell<Option<String>>>>,
    pub style_refinement: Option<crate::style::styled::StyleRefinement>,
    pub subscriptions: Vec<auralis_signal::subscription::SubscriptionHandle>,
}

impl Clone for LifecycleComponent {
    fn clone(&self) -> Self {
        Self {
            on_mount: self.on_mount.clone(),
            on_appear: self.on_appear.clone(),
            on_disappear: self.on_disappear.clone(),
            on_unmount: self.on_unmount.clone(),
            frame_tick: self.frame_tick.clone(),
            reactive_visible: self.reactive_visible.clone(),
            name: self.name.clone(),
            debug_label: self.debug_label.clone(),
            test_id: self.test_id.clone(),
            apply_drag_layout: self.apply_drag_layout.clone(),
            component_role: self.component_role.clone(),
            invalid_hint: self.invalid_hint.clone(),
            error_text: self.error_text.clone(),
            style_refinement: self.style_refinement.clone(),
            subscriptions: Vec::new(),
        }
    }
}

component_impl!(LifecycleComponent, LIFECYCLE);
