use crate::animation::{AnimatedProperty, AnimatedValue, AnimationConfig};
use crate::core::config::StateFlags;
use crate::core::config::{AriaLive, FlexWrap, Overflow, ScrollbarPolicy};
use crate::core::error::{panic_to_string, push_error, UiError};
pub use crate::core::id::ElementId;
use crate::event::{DragAxis, DragData, DropType};
use crate::style::styled::{Shadow, TextDecoration, TextOverflow};
use crate::style::{
    Alignment, Color, LinearGradient, Margin, Padding, Rect, TextAlign, TextDirection,
};
use crate::style::{StateStyle, TooltipPlacement, Vec2};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Set the shared ComponentTables reference for getter/setter access.
/// Routes through the active `AppContext` (which `current_app()` guarantees).
pub(crate) fn set_component_tables(
    ct: std::rc::Rc<std::cell::RefCell<crate::ecs::tables::ComponentTables>>,
) {
    crate::core::app_context::current_app().set_component_tables(ct);
}

pub(crate) fn with_ct<F, R>(f: F) -> R
where
    F: FnOnce(&crate::ecs::tables::ComponentTables) -> R,
{
    // Get the shared handle from the active AppContext (lazily created if the
    // caller hasn't installed one yet), drop the AppContext borrow, then borrow
    // the tables. Cloning the Rc first avoids nested-borrow panics.
    let ct = crate::core::app_context::current_app().ensure_component_tables();
    let tables = ct.borrow();
    f(&tables)
}

pub(crate) fn with_ct_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut crate::ecs::tables::ComponentTables) -> R,
{
    let ct = crate::core::app_context::current_app().ensure_component_tables();
    let mut tables = ct.borrow_mut();
    f(&mut tables)
}

static NEXT_TREE_ORDER: AtomicU64 = AtomicU64::new(1);

// ═══════════════════════ DirtyFlags ═══════════════════════

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct DirtyFlags(pub(crate) u32);

impl DirtyFlags {
    /// Bits 0-7: framework. Bits 8-31: third-party.
    pub const FRAMEWORK_MASK: u32 = 0xFF;

    pub const CLEAN: Self = Self(0);
    pub const REPAINT: Self = Self(1);
    pub const REPOSITION: Self = Self(3);
    pub const MEASURE: Self = Self(7);
    pub const MEASURE_BIT: Self = Self(4);
    pub const REPOSITION_BIT: Self = Self(2);

    pub fn contains(self, o: Self) -> bool {
        self.0 & o.0 != 0
    }
    pub fn is_clean(self) -> bool {
        self.0 == 0
    }
    pub fn has_measure(self) -> bool {
        self.0 & 4 != 0
    }
    pub fn has_reposition(self) -> bool {
        self.0 & 2 != 0
    }
    pub fn has_repaint(self) -> bool {
        self.0 & 1 != 0
    }
    pub fn downgrade_measure(self) -> Self {
        Self(self.0 & 3)
    }

    /// Allocate one or more bits for a third-party dirty category.
    /// Returns a `DirtyFlags` value with the requested bits set.
    /// Panics if `name` is already registered or no bits remain.
    pub fn register_custom(name: &'static str, bits: u32) -> Self {
        use std::collections::HashMap;
        use std::sync::OnceLock;
        static REGISTRY: OnceLock<std::sync::Mutex<HashMap<&'static str, u32>>> = OnceLock::new();
        let mut map = REGISTRY
            .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
            .lock()
            .unwrap();
        if bits & Self::FRAMEWORK_MASK != 0 {
            panic!("DirtyFlags::register_custom: custom bits must be >= 0x100 (bits 0-7 are reserved for framework use)");
        }
        if map.contains_key(name) {
            panic!("DirtyFlags::register_custom: '{name}' is already registered");
        }
        for existing in map.values() {
            if bits & existing != 0 {
                panic!("DirtyFlags::register_custom: bits 0x{bits:08X} overlap with already-registered bits 0x{existing:08X}");
            }
        }
        map.insert(name, bits);
        Self(bits)
    }
}

impl std::ops::BitOr for DirtyFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for DirtyFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0
    }
}

impl std::ops::Not for DirtyFlags {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0 & DirtyFlags::FRAMEWORK_MASK)
    }
}

// ═══════════════════════ Misc types ═══════════════════════

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutDirection {
    Vertical,
    Horizontal,
}

#[derive(Clone)]
pub struct LazyFontParams {
    pub font_size: f32,
    pub line_height: f32,
    pub font_weight: u16,
    pub font_family: Option<String>,
    pub max_width: Option<f32>,
    pub text_align: crate::style::TextAlign,
}

// ═══════════════════════ Slim Element ═══════════════════════

pub struct Element {
    // Identity & tree
    id: ElementId,
    /// Generation code that must match the ElementId's generation field
    /// for the arena to consider this slot entry valid (ABA guard).
    pub(crate) generation: u32,
    pub parent: Option<ElementId>,
    pub depth: u32,
    pub element_type: Option<&'static str>,
    pub children: Vec<ElementId>,
    pub sorted_children: RefCell<Option<Rc<Vec<ElementId>>>>,
    // Geometry
    pub screen_bounds: Rect,
    pub z_index: i32,
    /// Monotonic allocation order (never renumbered). Because children are
    /// only ever appended and every mount path allocates parents before
    /// children, this equals DFS preorder for all synchronous mounts —
    /// consumers (hit-test z-tie-break, `find_scrollable_at` innermost-first,
    /// focus traversal) rely on that. Deferred/late mounts sort after their
    /// earlier siblings, which also matches their paint order (appended last).
    pub tree_order: u64,
    pub z_index_floor: Option<i32>,
    pub slot_inactive: Rc<Cell<bool>>,
    // Rendering state
    pub dirty: Rc<Cell<DirtyFlags>>,
    pub state: Rc<Cell<StateFlags>>,
    pub surface_gen: Rc<Cell<u64>>,
    pub decor_gen: Rc<Cell<u64>>,
    pub subtree_generation: Rc<Cell<u64>>,
    pub layout_generation: Rc<Cell<u64>>,
    /// Memoized subtree visual AABB for paint-descent culling, keyed by
    /// `subtree_generation` (audit 2026-07-17 L2). Layout-space rect covering
    /// this element's own inflated visual rect (shadow/outline/position_offset)
    /// unioned with all descendant AABBs, passed through the element's own
    /// transform. Any change inside the subtree bumps `subtree_generation`
    /// along the ancestor chain (process_dirty_set), invalidating the memo.
    pub subtree_aabb: Cell<Option<(u64, Rect)>>,
    // Extension
    pub user_data: Option<Box<HashMap<std::any::TypeId, Box<dyn std::any::Any>>>>,
    /// Optional custom paint callback invoked by `paint_element_tree`
    /// after standard decor painting.
    pub paint_fn: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&mut crate::render::Painter, crate::style::Rect)>>,
    >,
    /// Optional callbacks for element lifecycle events.
    pub on_mount: Option<std::rc::Rc<dyn Fn(ElementId)>>,
    pub on_unmount: Option<std::rc::Rc<dyn Fn(ElementId)>>,
    /// The reason for the most recent focus change to this element.
    pub last_focus_reason: Cell<Option<crate::event::FocusReason>>,
    /// Per-element ComponentTables reference — eliminates dependency on
    /// `current_app()` for property accessors, preventing cross-window
    /// pollution when Signal callbacks fire under a stale AppContext.
    pub(crate) component_tables: Rc<RefCell<crate::ecs::tables::ComponentTables>>,
}

// ═══════════════════════ Construction ═══════════════════════

impl Element {
    pub fn new(component_tables: Rc<RefCell<crate::ecs::tables::ComponentTables>>) -> Self {
        Self {
            id: ElementId::SENTINEL,
            generation: 0,
            parent: None,
            depth: 0,
            element_type: None,
            children: Vec::new(),
            sorted_children: RefCell::new(None),
            screen_bounds: Rect::ZERO,
            z_index: 0,
            tree_order: NEXT_TREE_ORDER.fetch_add(1, Ordering::Relaxed),
            z_index_floor: None,
            slot_inactive: Rc::new(Cell::new(false)),
            dirty: Rc::new(Cell::new(DirtyFlags::REPAINT)),
            state: Rc::new(Cell::new(StateFlags::default())),
            user_data: None,
            paint_fn: None,
            on_mount: None,
            on_unmount: None,
            surface_gen: Rc::new(Cell::new(0)),
            decor_gen: Rc::new(Cell::new(0)),
            subtree_generation: Rc::new(Cell::new(0)),
            layout_generation: Rc::new(Cell::new(0)),
            subtree_aabb: Cell::new(None),
            last_focus_reason: Cell::new(None),
            component_tables,
        }
    }
}

impl Default for Element {
    fn default() -> Self {
        Self::new(Rc::new(RefCell::new(
            crate::ecs::tables::ComponentTables::default(),
        )))
    }
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Element")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

// ═══════════════════════ Metadata + delegation methods ═══════════════════════

impl Element {
    pub fn id(&self) -> ElementId {
        self.id
    }

    /// Set the element's identity after arena slot assignment.
    /// Called only by `ElementArena::allocate` and `ElementArena::insert`.
    pub(crate) fn assign_id(&mut self, index: u32, gen: u32) {
        self.id = ElementId::from_parts(index, gen);
        self.generation = gen;
    }

    pub fn global_rect(&self) -> Rect {
        self.screen_bounds
    }

    /// Access the ComponentTables via this element's own reference (not
    /// `current_app()`), preventing cross-window pollution when Signal
    /// callbacks fire under a stale AppContext.
    fn ct<R>(&self, f: impl FnOnce(&crate::ecs::tables::ComponentTables) -> R) -> R {
        f(&self.component_tables.borrow())
    }
    fn ct_mut<R>(&self, f: impl FnOnce(&mut crate::ecs::tables::ComponentTables) -> R) -> R {
        f(&mut self.component_tables.borrow_mut())
    }

    // ── Style delegates (via per-element ComponentTables reference) ──

    pub fn background(&self) -> Option<Color> {
        self.ct(|ct| ct.style.get(&self.id).and_then(|s| s.background))
    }
    pub fn foreground(&self) -> Option<Color> {
        self.ct(|ct| ct.style.get(&self.id).and_then(|s| s.foreground))
    }
    pub fn border_color(&self) -> Option<Color> {
        self.ct(|ct| ct.style.get(&self.id).and_then(|s| s.border_color))
    }
    pub fn border_width(&self) -> f32 {
        self.ct(|ct| ct.style.get(&self.id).map(|s| s.border_width))
            .unwrap_or(0.0)
    }
    pub fn outline_color(&self) -> Option<Color> {
        self.ct(|ct| ct.style.get(&self.id).and_then(|s| s.outline_color))
    }
    pub fn outline_width(&self) -> f32 {
        self.ct(|ct| ct.style.get(&self.id).map(|s| s.outline_width))
            .unwrap_or(0.0)
    }
    pub fn corner_radius(&self) -> f32 {
        self.ct(|ct| ct.style.get(&self.id).map(|s| s.corner_radius))
            .unwrap_or(4.0)
    }
    pub fn shadow(&self) -> Option<Shadow> {
        self.ct(|ct| ct.style.get(&self.id).and_then(|s| s.shadow))
    }
    pub fn gradient(&self) -> Option<LinearGradient> {
        self.ct(|ct| ct.style.get(&self.id).and_then(|s| s.gradient))
    }
    pub fn opacity(&self) -> f32 {
        self.ct(|ct| ct.style.get(&self.id).map(|s| s.opacity))
            .unwrap_or(1.0)
    }
    /// Opacity after full state/animation resolution — the value actually
    /// painted this frame. Unlike [`opacity`](Self::opacity) (which returns the
    /// authored base value), this applies the `resolve_style` priority chain,
    /// so animation overrides written to `state_style.animated` are reflected.
    pub fn resolved_opacity(&self) -> f32 {
        let state = self.state.get();
        self.ct(|ct| {
            ct.style
                .get(&self.id)
                .map(|s| crate::style::state_style::resolve_style(state, s).opacity)
        })
        .unwrap_or(1.0)
    }
    pub fn text_decoration(&self) -> TextDecoration {
        self.ct(|ct| ct.style.get(&self.id).map(|s| s.text_decoration))
            .unwrap_or(TextDecoration::None)
    }
    pub fn text_overflow(&self) -> TextOverflow {
        self.ct(|ct| ct.style.get(&self.id).map(|s| s.text_overflow))
            .unwrap_or(TextOverflow::Clip)
    }
    pub fn backdrop(&self) -> bool {
        self.ct(|ct| ct.style.get(&self.id).map(|s| s.backdrop))
            .unwrap_or(false)
    }

    /// Set state-dependent style overrides for this element.
    /// The framework resolves the correct visual properties at paint time
    /// based on the element's current `StateFlags`.
    pub fn with_state_style(&mut self, f: impl FnOnce(&mut StateStyle)) {
        self.ct_mut(|ct| {
            let s = ct.style.entry(self.id).or_default();
            let ss = s.state_style.get_or_insert_with(StateStyle::default);
            f(ss);
            let entry = ct.lc.entry(self.id).or_default();
            entry
                .style_refinement
                .get_or_insert_with(Default::default)
                .state_style = Some(ss.clone());
        });
        self.mark_surface_dirty();
    }

    /// Read the entire StyleComponent in one with_ct() call.
    #[inline]
    pub fn read_style(&self) -> Option<crate::ecs::components::StyleComponent> {
        self.ct(|ct| ct.style.get(&self.id).cloned())
    }

    pub fn set_background(&mut self, c: Color) {
        self.ct_mut(|ct| {
            ct.style.entry(self.id).or_default().background = Some(c);
            ct.lc
                .entry(self.id)
                .or_default()
                .style_refinement
                .get_or_insert_with(Default::default)
                .background = Some(c);
        });
        self.mark_surface_dirty();
    }
    pub fn set_foreground(&mut self, c: Color) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().foreground = Some(c));
        self.mark_surface_dirty();
        self.mark_text_dirty();
    }
    pub fn set_border_color(&mut self, c: Color) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().border_color = Some(c));
        self.mark_surface_dirty();
    }
    pub fn set_border_width(&mut self, v: f32) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().border_width = v);
        self.mark_surface_dirty();
    }
    pub fn set_outline_color(&mut self, c: Color) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().outline_color = Some(c));
        self.mark_surface_dirty();
    }
    pub fn set_outline_width(&mut self, v: f32) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().outline_width = v);
        self.mark_surface_dirty();
    }
    pub fn set_corner_radius(&mut self, r: f32) {
        self.ct_mut(|ct| {
            let s = ct.style.entry(self.id).or_default();
            s.corner_radius = r;
            s.corner_radii = None;
        });
        self.mark_surface_dirty();
    }
    /// Set per-corner radii. Also updates the uniform `corner_radius` to
    /// the maximum corner value so outline and shadow match.  Persists into
    /// the element's `StyleRefinement` so it survives theme reapplication.
    pub fn set_corner_radii(&mut self, radii: crate::style::CornerRadii) {
        let max_r = radii
            .top_left
            .max(radii.top_right)
            .max(radii.bottom_right)
            .max(radii.bottom_left);
        self.ct_mut(|ct| {
            let s = ct.style.entry(self.id).or_default();
            s.corner_radius = max_r;
            s.corner_radii = Some(radii);
            ct.lc
                .entry(self.id)
                .or_default()
                .style_refinement
                .get_or_insert_with(Default::default)
                .corner_radius = Some(radii);
        });
        self.mark_surface_dirty();
    }
    pub fn set_shadow(&mut self, v: Option<Shadow>) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().shadow = v);
        self.mark_surface_dirty();
    }
    pub fn set_blend_mode(&mut self, v: u8) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().blend_mode = v);
        self.mark_surface_dirty();
    }
    pub fn set_backdrop_filter(&mut self, v: Option<crate::style::styled::BackdropFilter>) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().backdrop_filter = v);
        self.mark_surface_dirty();
    }
    pub fn set_gradient(&mut self, v: Option<LinearGradient>) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().gradient = v);
        self.mark_surface_dirty();
    }
    pub fn set_opacity(&mut self, o: f32) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().opacity = o.clamp(0.0, 1.0));
    }
    pub fn set_text_decoration(&mut self, v: TextDecoration) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().text_decoration = v);
        self.mark_surface_dirty();
    }
    pub fn set_text_overflow(&mut self, v: TextOverflow) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().text_overflow = v);
        self.mark_surface_dirty();
    }
    pub fn set_backdrop(&mut self, v: bool) {
        self.ct_mut(|ct| ct.style.entry(self.id).or_default().backdrop = v);
        self.mark_surface_dirty();
    }

    // ── Layout delegates ──

    pub fn bounds(&self) -> Rect {
        self.screen_bounds
    }
    pub fn set_bounds(&mut self, r: Rect) {
        self.screen_bounds = r;
    }
    pub fn layout_direction(&self) -> LayoutDirection {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.layout_direction))
            .unwrap_or(LayoutDirection::Vertical)
    }
    pub fn gap(&self) -> f32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.gap))
            .unwrap_or(0.0)
    }
    pub fn margin(&self) -> Margin {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.margin))
            .unwrap_or(Margin::ZERO)
    }
    pub fn padding(&self) -> Padding {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.padding))
            .unwrap_or(Padding::ZERO)
    }
    pub fn alignment(&self) -> Alignment {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.alignment))
            .unwrap_or(Alignment::Start)
    }
    pub fn content_align(&self) -> Alignment {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.content_align))
            .unwrap_or(Alignment::Center)
    }
    pub fn preferred_width(&self) -> Option<f32> {
        self.ct(|ct| ct.layout.get(&self.id).and_then(|l| l.preferred_width))
    }
    pub fn preferred_height(&self) -> f32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.preferred_height))
            .unwrap_or(36.0)
    }
    pub fn flex_grow(&self) -> f32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.flex_grow))
            .unwrap_or(0.0)
    }
    pub fn flex_shrink(&self) -> f32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.flex_shrink))
            .unwrap_or(1.0)
    }
    pub fn flex_basis(&self) -> f32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.flex_basis))
            .unwrap_or(0.0)
    }
    pub fn flex_wrap(&self) -> FlexWrap {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.flex_wrap))
            .unwrap_or(FlexWrap::NoWrap)
    }
    pub fn aspect_ratio(&self) -> Option<f32> {
        self.ct(|ct| ct.layout.get(&self.id).and_then(|l| l.aspect_ratio))
    }
    pub fn overflow(&self) -> Overflow {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.overflow))
            .unwrap_or(Overflow::Visible)
    }
    pub fn is_scrollable(&self) -> bool {
        let ov = self
            .ct(|ct| ct.layout.get(&self.id).map(|l| l.overflow))
            .unwrap_or(Overflow::Visible);
        ov == Overflow::Scroll || ov == Overflow::Clip
    }
    pub fn order(&self) -> i32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.order))
            .unwrap_or(0)
    }
    pub fn scrollbar_policy(&self) -> ScrollbarPolicy {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.scrollbar_policy))
            .unwrap_or(ScrollbarPolicy::Auto)
    }
    pub fn scrollbar_width(&self) -> f32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.scrollbar_width))
            .unwrap_or(10.0)
    }
    pub fn affected_by_child_size(&self) -> bool {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.affected_by_child_size))
            .unwrap_or(true)
    }

    pub fn grid_columns(&self) -> u32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.grid_columns))
            .unwrap_or(0)
    }
    pub fn grid_column_span(&self) -> u32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.grid_column_span))
            .unwrap_or(0)
    }
    pub fn grid_column_offset(&self) -> u32 {
        self.ct(|ct| ct.layout.get(&self.id).map(|l| l.grid_column_offset))
            .unwrap_or(0)
    }

    pub fn set_grid_columns(&mut self, v: u32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().grid_columns = v);
        self.mark_reposition();
    }
    pub fn set_grid_column_widths(&mut self, v: Vec<f32>) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().grid_column_widths = v);
        self.mark_reposition();
    }
    pub fn set_grid_column_span(&mut self, v: u32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().grid_column_span = v);
        self.mark_reposition();
    }
    pub fn set_grid_column_offset(&mut self, v: u32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().grid_column_offset = v);
        self.mark_reposition();
    }

    pub fn set_layout_direction(&mut self, v: LayoutDirection) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().layout_direction = v);
        self.mark_reposition();
    }
    pub fn set_gap(&mut self, v: f32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().gap = v);
        self.mark_reposition();
    }
    pub fn set_margin(&mut self, v: Margin) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().margin = v);
        self.mark_reposition();
    }
    pub fn set_padding(&mut self, v: Padding) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().padding = v);
        self.mark_reposition();
    }
    pub fn set_alignment(&mut self, v: Alignment) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().alignment = v);
        self.mark_reposition();
    }
    pub fn set_content_align(&mut self, v: Alignment) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().content_align = v);
        self.mark_reposition();
    }
    pub fn set_preferred_width(&mut self, v: Option<f32>) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().preferred_width = v);
        self.mark_reposition();
    }
    pub fn set_preferred_height(&mut self, v: f32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().preferred_height = v);
        self.mark_reposition();
    }
    /// Set the original width Dimension (for percent-aware taffy resolution).
    pub fn set_width_dim(&mut self, v: Option<crate::style::Dimension>) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().width_dim = v);
        self.mark_reposition();
    }
    /// Set the original height Dimension (for percent-aware taffy resolution).
    pub fn set_height_dim(&mut self, v: crate::style::Dimension) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().height_dim = v);
        self.mark_reposition();
    }
    pub fn set_flex_grow(&mut self, v: f32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().flex_grow = v);
        self.mark_reposition();
    }
    pub fn set_flex_shrink(&mut self, v: f32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().flex_shrink = v);
        self.mark_reposition();
    }
    pub fn set_flex_basis(&mut self, v: f32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().flex_basis = v);
        self.mark_reposition();
    }
    /// Set the original flex_basis Dimension (for percent-aware taffy resolution).
    pub fn set_flex_basis_dim(&mut self, v: crate::style::Dimension) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().flex_basis_dim = v);
        self.mark_reposition();
    }
    pub fn set_min_main(&mut self, v: f32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().min_main = v);
        self.mark_reposition();
    }
    pub fn set_flex_wrap(&mut self, v: FlexWrap) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().flex_wrap = v);
        self.mark_reposition();
    }
    pub fn set_aspect_ratio(&mut self, v: Option<f32>) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().aspect_ratio = v);
        self.mark_reposition();
    }
    pub fn set_overflow(&mut self, v: Overflow) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().overflow = v);
        match v {
            Overflow::Scroll | Overflow::Clip => crate::ecs::register_scrollable(self.id),
            Overflow::Visible => crate::ecs::unregister_scrollable(self.id),
        }
        self.mark_reposition();
    }
    pub fn set_order(&mut self, v: i32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().order = v);
        self.mark_reposition();
    }
    pub fn set_scrollbar_policy(&mut self, v: ScrollbarPolicy) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().scrollbar_policy = v);
        self.mark_reposition();
    }
    pub fn set_scrollbar_width(&mut self, v: f32) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().scrollbar_width = v);
        self.mark_reposition();
    }
    pub fn set_affected_by_child_size(&mut self, v: bool) {
        self.ct_mut(|ct| ct.layout.entry(self.id).or_default().affected_by_child_size = v);
        crate::core::dirty_registry::set_affected_by_child_size(self.id, v);
        self.mark_measure();
    }

    // ── Interaction delegates ──

    pub fn is_focusable(&self) -> bool {
        self.ct(|ct| ct.interact.get(&self.id).map(|i| i.focusable))
            .unwrap_or(false)
    }
    pub fn tab_index(&self) -> Option<usize> {
        self.ct(|ct| ct.interact.get(&self.id).and_then(|i| i.tab_index))
    }
    pub fn accepts_mouse(&self) -> bool {
        self.ct(|ct| ct.interact.get(&self.id).map(|i| i.accepts_mouse))
            .unwrap_or(true)
    }
    pub fn input_pass_through(&self) -> bool {
        self.ct(|ct| ct.interact.get(&self.id).map(|i| i.input_pass_through))
            .unwrap_or(false)
    }
    pub fn read_only(&self) -> bool {
        self.ct(|ct| ct.interact.get(&self.id).map(|i| i.read_only))
            .unwrap_or(false)
    }
    pub fn selected(&self) -> bool {
        self.ct(|ct| ct.interact.get(&self.id).map(|i| i.selected))
            .unwrap_or(false)
    }

    pub fn set_focusable(&mut self, f: bool) {
        let was = self
            .ct(|ct| ct.interact.get(&self.id).map(|i| i.focusable))
            .unwrap_or(false);
        self.ct_mut(|ct| ct.interact.entry(self.id).or_default().focusable = f);
        if f && !was {
            let ti = self.ct(|ct| ct.interact.get(&self.id).and_then(|i| i.tab_index));
            crate::core::dirty_registry::register_focusable(self.id, ti, self.tree_order);
        } else if !f && was {
            let ti = self.ct(|ct| ct.interact.get(&self.id).and_then(|i| i.tab_index));
            crate::core::dirty_registry::unregister_focusable(self.id, ti, self.tree_order);
        }
    }
    pub fn set_tab_index(&mut self, v: Option<usize>) {
        self.ct_mut(|ct| ct.interact.entry(self.id).or_default().tab_index = v);
        crate::core::dirty_registry::set_elinfo_tab_index(self.id, v);
        self.mark_surface_dirty();
    }
    pub fn set_accepts_mouse(&mut self, v: bool) {
        self.ct_mut(|ct| ct.interact.entry(self.id).or_default().accepts_mouse = v);
        crate::core::dirty_registry::set_elinfo_accepts_mouse(self.id, v);
    }
    pub fn set_input_pass_through(&mut self, v: bool) {
        self.ct_mut(|ct| ct.interact.entry(self.id).or_default().input_pass_through = v);
    }
    pub fn set_read_only(&mut self, v: bool) {
        self.ct_mut(|ct| ct.interact.entry(self.id).or_default().read_only = v);
    }
    pub fn set_selected(&mut self, v: bool) {
        self.ct_mut(|ct| ct.interact.entry(self.id).or_default().selected = v);
    }
    pub fn set_cursor_icon(&mut self, v: Option<crate::platform::CursorIcon>) {
        self.ct_mut(|ct| ct.cursor.entry(self.id).or_default().cursor_icon = v);
    }

    // ── Text delegates ──

    pub fn font_size(&self) -> f32 {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.font_size))
            .unwrap_or(18.0)
    }
    pub fn font_weight(&self) -> u16 {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.font_weight))
            .unwrap_or(400)
    }
    pub fn font_family(&self) -> Option<String> {
        self.ct(|ct| ct.text.get(&self.id).and_then(|t| t.font_family.clone()))
    }
    pub fn text_align(&self) -> TextAlign {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.text_align))
            .unwrap_or(TextAlign::Start)
    }
    pub fn text_direction(&self) -> TextDirection {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.text_direction))
            .unwrap_or(TextDirection::Ltr)
    }
    pub fn text_vertical_center(&self) -> bool {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.text_vertical_center))
            .unwrap_or(true)
    }
    pub fn line_height(&self) -> f32 {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.line_height))
            .unwrap_or(1.5)
    }
    pub fn text_buffer(&self) -> Option<Rc<RefCell<cosmic_text::Buffer>>> {
        self.ct(|ct| ct.text.get(&self.id).and_then(|t| t.text_buffer.clone()))
    }
    pub fn text_generation(&self) -> Option<Rc<Cell<u64>>> {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.text_generation.clone()))
    }
    pub fn measured_text_width(&self) -> Option<Rc<Cell<f32>>> {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.measured_text_width.clone()))
    }
    pub fn lazy_label(&self) -> Option<Rc<Cell<String>>> {
        self.ct(|ct| ct.text.get(&self.id).and_then(|t| t.lazy_label.clone()))
    }
    pub fn buffer_gen(&self) -> Option<Rc<Cell<u64>>> {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.buffer_gen.clone()))
    }
    pub fn lazy_font_params(&self) -> Option<Rc<LazyFontParams>> {
        self.ct(|ct| {
            ct.text
                .get(&self.id)
                .and_then(|t| t.lazy_font_params.clone())
        })
    }
    pub fn is_placeholder(&self) -> Option<Rc<Cell<bool>>> {
        self.ct(|ct| ct.text.get(&self.id).map(|t| t.is_placeholder.clone()))
    }
    pub fn placeholder_color(&self) -> Option<Color> {
        self.ct(|ct| ct.text.get(&self.id).and_then(|t| t.placeholder_color))
    }
    pub fn selection_color(&self) -> Option<Color> {
        self.ct(|ct| ct.text.get(&self.id).and_then(|t| t.selection_color))
    }
    pub fn caret_color(&self) -> Option<Color> {
        self.ct(|ct| ct.text.get(&self.id).and_then(|t| t.caret_color))
    }
    pub fn bump_generation(&self) {}

    pub fn cursor_icon(&self) -> Option<crate::platform::CursorIcon> {
        self.ct(|ct| ct.cursor.get(&self.id).and_then(|c| c.cursor_icon))
    }

    pub fn set_text_generation(&mut self, v: Rc<Cell<u64>>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().text_generation = v);
    }
    pub fn mark_text_dirty(&mut self) {
        // Phase guard: Layout must NOT directly bump generation counters.
        // Width-driven text invalidation should use defer_action (see
        // frame_pipeline::layout_phase). Prepass frame_ticks should also
        // route through defer_action rather than calling this directly.
        crate::core::frame_pipeline::debug_assert_phase(&[
            crate::core::frame_pipeline::FramePhase::Prepass,
            crate::core::frame_pipeline::FramePhase::Paint,
            crate::core::frame_pipeline::FramePhase::None,
        ]);
        self.ct_mut(|ct| {
            if let Some(t) = ct.text.get_mut(&self.id) {
                t.text_generation.set(t.text_generation.get() + 1);
            }
        });
    }
    pub fn set_measured_text_width(&mut self, v: Rc<Cell<f32>>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().measured_text_width = v);
    }
    pub fn set_lazy_label(&mut self, v: Rc<Cell<String>>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().lazy_label = Some(v));
    }
    pub fn set_buffer_gen(&mut self, v: Rc<Cell<u64>>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().buffer_gen = v);
    }
    pub fn set_lazy_font_params(&mut self, v: Rc<LazyFontParams>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().lazy_font_params = Some(v));
    }
    pub fn set_is_placeholder(&mut self, v: Rc<Cell<bool>>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().is_placeholder = v);
    }

    pub fn set_font_size(&mut self, v: f32) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().font_size = v);
        self.mark_surface_dirty();
    }
    pub fn set_font_weight(&mut self, v: u16) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().font_weight = v);
        self.mark_surface_dirty();
    }
    pub fn set_font_family(&mut self, v: Option<String>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().font_family = v);
    }
    pub fn set_text_align(&mut self, v: TextAlign) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().text_align = v);
        self.mark_surface_dirty();
    }
    pub fn set_text_direction(&mut self, v: TextDirection) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().text_direction = v);
        self.mark_surface_dirty();
    }
    pub fn set_text_vertical_center(&mut self, v: bool) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().text_vertical_center = v);
        self.mark_surface_dirty();
    }
    pub fn set_line_height(&mut self, v: f32) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().line_height = v);
        self.mark_surface_dirty();
    }
    pub fn set_text_buffer(&mut self, v: Rc<RefCell<cosmic_text::Buffer>>) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().text_buffer = Some(v));
    }
    pub fn set_placeholder_color(&mut self, v: Color) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().placeholder_color = Some(v));
    }
    pub fn set_selection_color(&mut self, v: Color) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().selection_color = Some(v));
    }
    pub fn set_caret_color(&mut self, v: Color) {
        self.ct_mut(|ct| ct.text.entry(self.id).or_default().caret_color = Some(v));
    }

    // ── Scroll delegates ──

    pub fn scroll_offset(&self) -> Option<Rc<Cell<Vec2>>> {
        self.ct(|ct| ct.scroll.get(&self.id).map(|s| s.scroll_offset.clone()))
    }
    pub fn content_bounds(&self) -> Option<Rc<Cell<Rect>>> {
        self.ct(|ct| ct.scroll.get(&self.id).map(|s| s.content_bounds.clone()))
    }
    pub fn text_scroll_x(&self) -> Option<Rc<Cell<f32>>> {
        self.ct(|ct| ct.scroll.get(&self.id).map(|s| s.text_scroll_x.clone()))
    }
    pub fn text_scroll_y(&self) -> Option<Rc<Cell<f32>>> {
        self.ct(|ct| ct.scroll.get(&self.id).map(|s| s.text_scroll_y.clone()))
    }
    pub fn max_scroll_y(&self) -> Option<Rc<Cell<f32>>> {
        self.ct(|ct| ct.scroll.get(&self.id).map(|s| s.max_scroll_y.clone()))
    }
    pub fn pending_scroll_to(&self) -> Option<Rc<Cell<Option<ElementId>>>> {
        self.ct(|ct| ct.scroll.get(&self.id).map(|s| s.pending_scroll_to.clone()))
    }

    pub fn set_scroll_offset(&mut self, v: Rc<Cell<Vec2>>) {
        self.ct_mut(|ct| ct.scroll.entry(self.id).or_default().scroll_offset = v);
    }
    pub fn set_content_bounds(&mut self, v: Rc<Cell<Rect>>) {
        self.ct_mut(|ct| ct.scroll.entry(self.id).or_default().content_bounds = v);
    }
    pub fn set_pending_scroll_to(&mut self, v: Rc<Cell<Option<ElementId>>>) {
        self.ct_mut(|ct| ct.scroll.entry(self.id).or_default().pending_scroll_to = v);
        crate::ecs::register_pending_scroll(self.id);
    }
    pub fn set_text_scroll_x(&mut self, v: Rc<Cell<f32>>) {
        self.ct_mut(|ct| ct.scroll.entry(self.id).or_default().text_scroll_x = v);
    }
    pub fn set_text_scroll_y(&mut self, v: Rc<Cell<f32>>) {
        self.ct_mut(|ct| ct.scroll.entry(self.id).or_default().text_scroll_y = v);
    }
    pub fn set_max_scroll_y(&mut self, v: Rc<Cell<f32>>) {
        self.ct_mut(|ct| ct.scroll.entry(self.id).or_default().max_scroll_y = v);
    }

    // ── Cursor delegates ──

    pub fn cursor_x(&self) -> Option<Rc<Cell<f32>>> {
        self.ct(|ct| ct.cursor.get(&self.id).map(|c| c.cursor_x.clone()))
    }
    pub fn cursor_visible(&self) -> Option<Rc<Cell<bool>>> {
        self.ct(|ct| ct.cursor.get(&self.id).map(|c| c.cursor_visible.clone()))
    }
    pub fn cursor_line(&self) -> Option<Rc<Cell<usize>>> {
        self.ct(|ct| ct.cursor.get(&self.id).map(|c| c.cursor_line.clone()))
    }
    pub fn cursor_blink_last_input(&self) -> Option<Rc<Cell<Instant>>> {
        self.ct(|ct| {
            ct.cursor
                .get(&self.id)
                .map(|c| c.cursor_blink_last_input.clone())
        })
    }
    pub fn cursor_focused(&self) -> Option<Rc<Cell<bool>>> {
        self.ct(|ct| ct.cursor.get(&self.id).map(|c| c.cursor_focused.clone()))
    }
    pub fn selection_rect(&self) -> Option<Rc<Cell<Vec<Rect>>>> {
        self.ct(|ct| ct.cursor.get(&self.id).map(|c| c.selection_rect.clone()))
    }

    pub fn set_cursor_x(&mut self, v: Rc<Cell<f32>>) {
        self.ct_mut(|ct| ct.cursor.entry(self.id).or_default().cursor_x = v);
    }
    pub fn set_cursor_visible(&mut self, v: Rc<Cell<bool>>) {
        self.ct_mut(|ct| ct.cursor.entry(self.id).or_default().cursor_visible = v);
    }
    pub fn set_cursor_line(&mut self, v: Rc<Cell<usize>>) {
        self.ct_mut(|ct| ct.cursor.entry(self.id).or_default().cursor_line = v);
    }
    pub fn set_cursor_blink_last_input(&mut self, v: Rc<Cell<Instant>>) {
        self.ct_mut(|ct| {
            ct.cursor
                .entry(self.id)
                .or_default()
                .cursor_blink_last_input = v
        });
    }
    pub fn set_cursor_focused(&mut self, v: Rc<Cell<bool>>) {
        self.ct_mut(|ct| ct.cursor.entry(self.id).or_default().cursor_focused = v);
    }
    pub fn set_selection_rect(&mut self, v: Rc<Cell<Vec<Rect>>>) {
        self.ct_mut(|ct| ct.cursor.entry(self.id).or_default().selection_rect = v);
    }

    pub fn set_ime_cursor_rect(&mut self, v: Rc<Cell<Option<Rect>>>) {
        self.ct_mut(|ct| ct.cursor.entry(self.id).or_default().ime_cursor_rect = v);
    }

    pub fn ime_cursor_rect(&self) -> Option<Rc<Cell<Option<Rect>>>> {
        self.ct(|ct| ct.cursor.get(&self.id).map(|c| c.ime_cursor_rect.clone()))
    }

    pub fn set_composition_underline_rect(&mut self, v: Rc<Cell<Option<Rect>>>) {
        self.ct_mut(|ct| {
            ct.cursor
                .entry(self.id)
                .or_default()
                .composition_underline_rect = v
        });
    }

    pub fn composition_underline_rect(&self) -> Option<Rc<Cell<Option<Rect>>>> {
        self.ct(|ct| {
            ct.cursor
                .get(&self.id)
                .map(|c| c.composition_underline_rect.clone())
        })
    }

    // ── Tooltip delegates ──

    pub fn tooltip_text(&self) -> Option<Rc<String>> {
        self.ct(|ct| ct.tooltip.get(&self.id).map(|t| t.tooltip_text.clone()))
    }
    pub fn tooltip_visible(&self) -> Option<Rc<Cell<bool>>> {
        self.ct(|ct| ct.tooltip.get(&self.id).map(|t| t.tooltip_visible.clone()))
    }
    pub fn tooltip_placement(&self) -> Option<TooltipPlacement> {
        self.ct(|ct| ct.tooltip.get(&self.id).map(|t| t.tooltip_placement))
    }
    pub fn tooltip_delay_start(&self) -> Option<Rc<Cell<Option<Instant>>>> {
        self.ct(|ct| {
            ct.tooltip
                .get(&self.id)
                .map(|t| t.tooltip_delay_start.clone())
        })
    }
    pub fn tooltip_alpha(&self) -> Option<Rc<Cell<f32>>> {
        self.ct(|ct| ct.tooltip.get(&self.id).map(|t| t.tooltip_alpha.clone()))
    }
    pub fn tooltip_delay_ms(&self) -> u64 {
        self.ct(|ct| ct.tooltip.get(&self.id).map(|t| t.tooltip_delay_ms))
            .unwrap_or(300)
    }

    // ── Drag delegates ──

    pub fn draggable(&self) -> bool {
        self.ct(|ct| ct.dragdrop.get(&self.id).map(|d| d.draggable))
            .unwrap_or(false)
    }
    pub fn drag_data(&self) -> Option<DragData> {
        self.ct(|ct| ct.dragdrop.get(&self.id).and_then(|d| d.drag_data.clone()))
    }
    pub fn drag_axis(&self) -> DragAxis {
        self.ct(|ct| ct.dragdrop.get(&self.id).map(|d| d.drag_axis))
            .unwrap_or(DragAxis::Free)
    }
    pub fn drop_target(&self) -> bool {
        self.ct(|ct| ct.dragdrop.get(&self.id).map(|d| d.drop_target))
            .unwrap_or(false)
    }
    pub fn accept_drop_types(&self) -> Vec<DropType> {
        self.ct(|ct| {
            ct.dragdrop
                .get(&self.id)
                .map(|d| d.accept_drop_types.clone())
        })
        .unwrap_or_default()
    }
    pub fn max_length(&self) -> Option<usize> {
        self.ct(|ct| ct.dragdrop.get(&self.id).and_then(|d| d.max_length))
    }
    pub fn validation(&self) -> Option<Rc<dyn Fn(&str) -> bool>> {
        self.ct(|ct| {
            ct.dragdrop
                .get(&self.id)
                .and_then(|dd| dd.validation.clone())
        })
    }
    pub fn on_drop(&self) -> Option<Rc<dyn Fn(DragData)>> {
        self.ct(|ct| ct.dragdrop.get(&self.id).and_then(|dd| dd.on_drop.clone()))
    }
    pub fn on_drag_start(&self) -> Option<Rc<dyn Fn() -> DragData>> {
        self.ct(|ct| {
            ct.dragdrop
                .get(&self.id)
                .and_then(|dd| dd.on_drag_start.clone())
        })
    }
    pub fn set_draggable(&mut self, v: bool) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().draggable = v);
    }
    pub fn set_drag_data(&mut self, v: Option<DragData>) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().drag_data = v);
    }
    pub fn set_drag_axis(&mut self, v: DragAxis) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().drag_axis = v);
    }
    pub fn set_drop_target(&mut self, v: bool) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().drop_target = v);
    }
    pub fn set_max_length(&mut self, v: Option<usize>) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().max_length = v);
    }
    pub fn set_accept_drop_types(&mut self, v: Vec<DropType>) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().accept_drop_types = v);
    }
    pub fn set_validation(&mut self, v: Option<Rc<dyn Fn(&str) -> bool>>) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().validation = v);
    }
    pub fn set_on_drop(&mut self, f: Box<dyn Fn(DragData)>) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().on_drop = Some(f.into()));
    }
    pub fn set_on_drag_start(&mut self, f: Rc<dyn Fn() -> DragData>) {
        self.ct_mut(|ct| ct.dragdrop.entry(self.id).or_default().on_drag_start = Some(f));
    }

    // ── Animation delegates ──

    pub fn animation_config(&self) -> Option<AnimationConfig> {
        self.ct(|ct| {
            ct.anim
                .get(&self.id)
                .and_then(|a| a.animation_config.clone())
        })
    }
    pub fn exit_pending(&self) -> Option<(AnimatedProperty, AnimatedValue, Rc<Cell<bool>>)> {
        self.ct(|ct| ct.anim.get(&self.id).and_then(|a| a.exit_pending.clone()))
    }
    pub fn transition_config(&self) -> Option<Rc<crate::animation::TransitionConfig>> {
        self.ct(|ct| {
            ct.anim
                .get(&self.id)
                .and_then(|a| a.transition_config.clone())
        })
    }

    pub fn animate_exit(
        &mut self,
        p: AnimatedProperty,
        to: AnimatedValue,
        anim: crate::animation::Animation,
    ) {
        let from = match p {
            AnimatedProperty::Opacity => AnimatedValue::Float(self.opacity()),
            AnimatedProperty::Background => {
                AnimatedValue::Color(self.background().unwrap_or(Color::TRANSPARENT))
            }
            _ => AnimatedValue::Float(self.opacity()),
        };
        let done = Rc::new(Cell::new(false));
        self.ct_mut(|ct| {
            ct.anim.entry(self.id).or_default().exit_pending = Some((p, to.clone(), done.clone()))
        });
        crate::core::dirty_registry::register_exit(self.id());
        crate::animation::request_exit_anim(self.id(), p, from, to, anim, done);
    }

    // ── Transform delegates ──

    pub fn transform(&self) -> Option<[f32; 6]> {
        self.ct(|ct| ct.xform.get(&self.id).and_then(|x| x.transform))
    }
    pub fn transform_origin_x(&self) -> f32 {
        self.ct(|ct| ct.xform.get(&self.id).map(|x| x.transform_origin_x))
            .unwrap_or(0.5)
    }
    pub fn transform_origin_y(&self) -> f32 {
        self.ct(|ct| ct.xform.get(&self.id).map(|x| x.transform_origin_y))
            .unwrap_or(0.5)
    }
    pub fn position_offset(&self) -> Option<Rc<Cell<Vec2>>> {
        self.ct(|ct| ct.xform.get(&self.id).map(|x| x.position_offset.clone()))
    }
    pub fn size_scale(&self) -> Option<Rc<Cell<Vec2>>> {
        self.ct(|ct| ct.xform.get(&self.id).map(|x| x.size_scale.clone()))
    }

    // ── A11y delegates ──

    pub fn accessible_role(&self) -> Option<accesskit::Role> {
        self.ct(|ct| ct.a11y.get(&self.id).and_then(|a| a.accessible_role))
    }
    pub fn accessible_label(&self) -> Option<String> {
        self.ct(|ct| {
            ct.a11y
                .get(&self.id)
                .and_then(|a| a.accessible_label.clone())
        })
    }
    pub fn accessible_description(&self) -> Option<String> {
        self.ct(|ct| {
            ct.a11y
                .get(&self.id)
                .and_then(|a| a.accessible_description.clone())
        })
    }
    pub fn accessible_level(&self) -> Option<u32> {
        self.ct(|ct| ct.a11y.get(&self.id).and_then(|a| a.accessible_level))
    }
    pub fn accessible_live(&self) -> AriaLive {
        self.ct(|ct| ct.a11y.get(&self.id).map(|a| a.accessible_live))
            .unwrap_or(AriaLive::Off)
    }
    pub fn accessible_hidden(&self) -> bool {
        self.ct(|ct| ct.a11y.get(&self.id).map(|a| a.accessible_hidden))
            .unwrap_or(false)
    }
    pub fn accessible_checked(&self) -> Option<bool> {
        self.ct(|ct| ct.a11y.get(&self.id).and_then(|a| a.accessible_checked))
    }
    pub fn accessible_required(&self) -> bool {
        self.ct(|ct| ct.a11y.get(&self.id).map(|a| a.accessible_required))
            .unwrap_or(false)
    }
    pub fn accessible_value(&self) -> Option<f64> {
        self.ct(|ct| ct.a11y.get(&self.id).and_then(|a| a.accessible_value))
    }
    pub fn accessible_min(&self) -> f64 {
        self.ct(|ct| ct.a11y.get(&self.id).map(|a| a.accessible_min))
            .unwrap_or(0.0)
    }
    pub fn accessible_max(&self) -> f64 {
        self.ct(|ct| ct.a11y.get(&self.id).map(|a| a.accessible_max))
            .unwrap_or(100.0)
    }

    pub fn set_accessible_role(&mut self, r: accesskit::Role) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_role = Some(r));
    }
    pub fn set_accessible_label(&mut self, l: impl Into<String>) {
        let s = l.into();
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_label = Some(s));
    }
    pub fn set_accessible_description(&mut self, v: String) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_description = Some(v));
    }
    pub fn set_accessible_level(&mut self, v: u32) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_level = Some(v));
    }
    pub fn set_accessible_live(&mut self, v: AriaLive) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_live = v);
    }
    pub fn set_accessible_hidden(&mut self, v: bool) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_hidden = v);
    }
    pub fn set_accessible_checked(&mut self, v: bool) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_checked = Some(v));
    }
    pub fn set_accessible_required(&mut self, v: bool) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_required = v);
    }
    pub fn set_accessible_value(&mut self, v: f64) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_value = Some(v));
    }
    pub fn set_accessible_min(&mut self, v: f64) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_min = v);
    }
    pub fn set_accessible_max(&mut self, v: f64) {
        self.ct_mut(|ct| ct.a11y.entry(self.id).or_default().accessible_max = v);
    }
    pub fn set_active_descendant(&mut self, eid: Option<ElementId>) {
        self.ct_mut(|ct| {
            ct.a11y
                .entry(self.id)
                .or_default()
                .accessible_active_descendant = eid
        });
    }
    pub fn active_descendant(&self) -> Option<ElementId> {
        self.ct(|ct| {
            ct.a11y
                .get(&self.id)
                .and_then(|a| a.accessible_active_descendant)
        })
    }

    // ── Lifecycle delegates ──

    pub fn is_visible(&self) -> bool {
        !self.slot_inactive.get()
            && crate::core::dirty_registry::is_elinfo_visible(self.id)
            && self.ct(|ct| {
                ct.lc
                    .get(&self.id)
                    .and_then(|lc| lc.reactive_visible.as_ref())
                    .is_none_or(|c| c.get())
            })
    }
    pub fn visible(&self) -> bool {
        self.ct(|ct| {
            ct.lc
                .get(&self.id)
                .and_then(|lc| lc.reactive_visible.as_ref())
                .is_none_or(|c| c.get())
        })
    }
    pub fn name(&self) -> Option<String> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.name.clone()))
    }
    pub fn debug_label(&self) -> Option<String> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.debug_label.clone()))
    }
    pub fn test_id(&self) -> Option<String> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.test_id.clone()))
    }
    pub fn reactive_visible(&self) -> Option<Rc<Cell<bool>>> {
        self.ct(|ct| {
            ct.lc
                .get(&self.id)
                .and_then(|lc| lc.reactive_visible.clone())
        })
    }
    pub fn invalid_hint(&self) -> Option<Rc<Cell<bool>>> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.invalid_hint.clone()))
    }
    pub fn error_text(&self) -> Option<Rc<RefCell<Option<String>>>> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.error_text.clone()))
    }
    pub fn frame_tick(&self) -> Option<Rc<RefCell<Option<Box<dyn Fn()>>>>> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.frame_tick.clone()))
    }
    pub fn on_mount(&self) -> Option<Rc<RefCell<Option<Box<dyn FnOnce()>>>>> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.on_mount.clone()))
    }
    pub fn on_appear(&self) -> Option<Rc<RefCell<Option<Box<dyn Fn()>>>>> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.on_appear.clone()))
    }
    pub fn on_disappear(&self) -> Option<Rc<RefCell<Option<Box<dyn Fn()>>>>> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.on_disappear.clone()))
    }
    pub fn on_unmount(&self) -> Option<Rc<RefCell<Option<Box<dyn FnOnce()>>>>> {
        self.ct(|ct| ct.lc.get(&self.id).and_then(|lc| lc.on_unmount.clone()))
    }

    pub fn set_visible(&mut self, v: bool) {
        crate::core::dirty_registry::set_elinfo_visible(self.id, v);
        self.mark_surface_dirty();
    }
    pub fn set_state_dirty(&mut self, flag: StateFlags, on: bool) {
        let old = self.state.get();
        if old.contains(flag) == on {
            return;
        }
        let mut s = old;
        s.set(flag, on);
        self.state.set(s);

        // HOVERED flips at pointer-move rate, and most elements the cursor
        // crosses (plain containers, texts) have no hover-dependent visuals.
        // The flag is still recorded above (user code / a11y may read it),
        // but repaint + subtree-gen invalidation is skipped when the resolved
        // style cannot change — mouse traffic over static content no longer
        // drives frames (audit 2026-07-17 round 3, Finding C). All other
        // flags flip at click/keyboard rate and keep the unconditional
        // repaint (FOCUSED additionally paints focus rings outside
        // StateStyle, e.g. window focus outline and Slider).
        if flag == StateFlags::HOVERED && !self.hover_affects_visuals() {
            return;
        }

        self.mark_surface_dirty();

        // Auto-animate state transitions
        if crate::animation::animations_enabled() {
            if let Some(ref tc) = self.transition_config() {
                for t in &tc.transitions {
                    let from_val = Self::resolve_anim_property(self.id, old, t.property);
                    let to_val = Self::resolve_anim_property(self.id, s, t.property);
                    if from_val != to_val {
                        crate::animation::apply_transition(self, t.property, from_val, to_val);
                    }
                }
            }
        }
    }

    /// True when flipping HOVERED can change this element's rendered output:
    /// its `StateStyle.hovered` variant overrides at least one property.
    /// Hover *callbacks* (MouseRegion etc.) are unaffected — they fire
    /// regardless and register their own dirty if they mutate visuals (SSOT).
    fn hover_affects_visuals(&self) -> bool {
        self.ct(|ct| {
            ct.style
                .get(&self.id)
                .and_then(|sc| sc.state_style.as_ref())
                .is_some_and(|ss| !ss.hovered.is_empty())
        })
    }
    /// Resolve the value of an `AnimatedProperty` for a given state.
    pub fn resolve_anim_property(
        eid: ElementId,
        state: StateFlags,
        property: crate::animation::AnimatedProperty,
    ) -> crate::animation::AnimatedValue {
        use crate::animation::{AnimatedProperty as P, AnimatedValue};
        use crate::ecs::components::StyleComponent;
        use crate::style::Color;
        with_ct(|ct| {
            match property {
                P::Background
                | P::Opacity
                | P::Foreground
                | P::BorderColor
                | P::BorderWidth
                | P::Shadow
                | P::CornerRadius => {
                    let default = StyleComponent::default();
                    let sc = ct.style.get(&eid).unwrap_or(&default);
                    let resolved = crate::style::state_style::resolve_style(state, sc);
                    match property {
                        P::Background => {
                            AnimatedValue::Color(resolved.background.unwrap_or(Color::TRANSPARENT))
                        }
                        P::Foreground => {
                            AnimatedValue::Color(resolved.foreground.unwrap_or(Color::TRANSPARENT))
                        }
                        P::BorderColor => AnimatedValue::Color(
                            resolved.border_color.unwrap_or(Color::TRANSPARENT),
                        ),
                        P::BorderWidth => AnimatedValue::Float(resolved.border_width),
                        P::Shadow => AnimatedValue::Shadow(resolved.shadow.unwrap_or(
                            crate::style::styled::Shadow::new(Color::TRANSPARENT, 0.0, 0.0, 0.0),
                        )),
                        P::CornerRadius => AnimatedValue::CornerRadii(resolved.corner_radius),
                        _ => AnimatedValue::Float(resolved.opacity),
                    }
                }
                P::Position => AnimatedValue::Float(
                    ct.xform
                        .get(&eid)
                        .map(|x| x.position_offset.get().x)
                        .unwrap_or(0.0),
                ),
                P::Size => AnimatedValue::Float(
                    ct.xform
                        .get(&eid)
                        .map(|x| x.size_scale.get().x)
                        .unwrap_or(1.0),
                ),
                // Degrees, recovered from the affine's first column.
                P::Rotation => AnimatedValue::Float(
                    ct.xform
                        .get(&eid)
                        .and_then(|x| x.transform)
                        .map(|m| m[1].atan2(m[0]).to_degrees())
                        .unwrap_or(0.0),
                ),
                P::Custom(_) => AnimatedValue::Float(0.0),
            }
        })
    }
    pub fn set_z_index(&mut self, v: i32) {
        self.z_index = v;
        self.mark_surface_dirty();
        crate::core::dirty_registry::set_elinfo_z_index(self.id, v);
        if self.screen_bounds != Rect::ZERO {
            crate::core::dirty_registry::spatial_register(
                self.id,
                self.screen_bounds,
                self.tree_order,
            );
        }
        let my_id = self.id;
        crate::core::dirty_registry::defer_action(move |arena, _root, _reg| {
            arena.invalidate_sorted_children(my_id);
        });
    }
    pub fn set_slot_inactive(&mut self, v: bool) {
        self.slot_inactive.set(v);
    }

    pub fn set_reactive_visible(&mut self, v: Rc<Cell<bool>>) {
        self.ct_mut(|ct| ct.lc.entry(self.id).or_default().reactive_visible = Some(v.clone()));
        crate::core::dirty_registry::set_elinfo_reactive_visible(self.id, v);
    }
    pub fn set_invalid_hint(&mut self, v: Rc<Cell<bool>>) {
        self.ct_mut(|ct| ct.lc.entry(self.id).or_default().invalid_hint = Some(v));
    }
    pub fn set_error_text(&mut self, v: Rc<RefCell<Option<String>>>) {
        self.ct_mut(|ct| ct.lc.entry(self.id).or_default().error_text = Some(v));
    }
    pub fn set_apply_drag_layout(&mut self, f: Box<dyn Fn(&mut ElementArena, ElementId)>) {
        self.ct_mut(|ct| ct.lc.entry(self.id).or_default().apply_drag_layout = Some(f.into()));
        crate::ecs::register_drag_element(self.id);
    }
    pub fn set_frame_tick(&mut self, f: Box<dyn Fn()>) {
        self.ct_mut(|ct| {
            ct.lc.entry(self.id).or_default().frame_tick = Some(Rc::new(RefCell::new(Some(f))))
        });
        crate::ecs::active::register_active(self.id, crate::ecs::active::ActiveTag::FrameTick);
    }
    pub fn set_test_id(&mut self, v: impl Into<String>) {
        self.ct_mut(|ct| ct.lc.entry(self.id).or_default().test_id = Some(v.into()));
    }
    pub fn set_name(&mut self, v: impl Into<String>) {
        self.ct_mut(|ct| ct.lc.entry(self.id).or_default().name = Some(v.into()));
    }
    pub fn set_animation_config(&mut self, v: Option<AnimationConfig>) {
        self.ct_mut(|ct| ct.anim.entry(self.id).or_default().animation_config = v);
    }
    pub fn set_transform(&mut self, v: Option<[f32; 6]>) {
        self.ct_mut(|ct| ct.xform.entry(self.id).or_default().transform = v);
    }
    pub fn set_transform_origin_x(&mut self, v: f32) {
        self.ct_mut(|ct| ct.xform.entry(self.id).or_default().transform_origin_x = v);
    }
    pub fn set_transform_origin_y(&mut self, v: f32) {
        self.ct_mut(|ct| ct.xform.entry(self.id).or_default().transform_origin_y = v);
    }
    pub fn set_position_offset(&mut self, v: Rc<Cell<Vec2>>) {
        self.ct_mut(|ct| ct.xform.entry(self.id).or_default().position_offset = v);
        crate::core::dirty_registry::spatial_register_position_offset(self.id);
    }
    pub fn set_size_scale(&mut self, v: Rc<Cell<Vec2>>) {
        self.ct_mut(|ct| ct.xform.entry(self.id).or_default().size_scale = v);
    }
}

// ═══════════════════════ Dirty-flag management ═══════════════════════

impl Element {
    #[inline]
    pub fn needs_repaint(&self) -> bool {
        self.dirty.get().has_repaint()
    }
    #[inline]
    pub fn needs_reposition(&self) -> bool {
        self.dirty.get().has_reposition()
    }
    #[inline]
    pub fn needs_measure(&self) -> bool {
        self.dirty.get().has_measure()
    }
    #[inline]
    pub fn dirty_flags(&self) -> DirtyFlags {
        self.dirty.get()
    }

    #[inline]
    pub fn mark_repaint(&self) {
        self.dirty.set(self.dirty.get() | DirtyFlags::REPAINT);
        crate::core::dirty_registry::register_dirty(self.id, DirtyFlags::REPAINT);
        self.surface_gen.set(self.surface_gen.get().wrapping_add(1));
        self.decor_gen.set(self.decor_gen.get().wrapping_add(1));
        crate::core::dirty_registry::bump_subtree_gen(self.id);
    }

    #[inline]
    pub fn mark_surface_dirty(&self) {
        self.dirty.set(self.dirty.get() | DirtyFlags::REPAINT);
        crate::core::dirty_registry::register_dirty(self.id, DirtyFlags::REPAINT);
        self.surface_gen.set(self.surface_gen.get().wrapping_add(1));
        crate::core::dirty_registry::bump_subtree_gen(self.id);
    }

    #[inline]
    pub fn mark_decor_dirty(&self) {
        self.dirty.set(self.dirty.get() | DirtyFlags::REPAINT);
        crate::core::dirty_registry::register_dirty(self.id, DirtyFlags::REPAINT);
        self.decor_gen.set(self.decor_gen.get().wrapping_add(1));
        crate::core::dirty_registry::bump_subtree_gen(self.id);
    }

    #[inline]
    pub fn mark_reposition(&self) {
        self.dirty.set(self.dirty.get() | DirtyFlags::REPOSITION);
        crate::core::dirty_registry::register_dirty(self.id, DirtyFlags::REPOSITION);
        crate::core::dirty_registry::bump_subtree_gen(self.id);
    }

    #[inline]
    pub fn mark_measure(&self) {
        self.dirty.set(self.dirty.get() | DirtyFlags::MEASURE);
        crate::core::dirty_registry::register_dirty(self.id, DirtyFlags::MEASURE);
        crate::core::dirty_registry::bump_subtree_gen(self.id);
    }

    #[inline]
    pub fn clear_repaint(&self) {
        self.dirty.set(DirtyFlags(self.dirty.get().0 & 0b110));
    }

    #[inline]
    pub fn clear_reposition(&self) {
        self.dirty.set(DirtyFlags(self.dirty.get().0 & 0b101));
    }

    #[inline]
    pub fn clear_measure(&self) {
        self.dirty.set(DirtyFlags(self.dirty.get().0 & 0b011));
    }

    pub fn mark_surface_dirty_remote(dirty: &Rc<Cell<DirtyFlags>>, eid: ElementId) {
        dirty.set(dirty.get() | DirtyFlags::REPAINT);
        crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
        crate::core::dirty_registry::bump_surface_gen_remote(eid);
        crate::core::dirty_registry::bump_subtree_gen(eid);
    }

    pub fn mark_repaint_remote(dirty: &Rc<Cell<DirtyFlags>>, eid: ElementId) {
        dirty.set(dirty.get() | DirtyFlags::REPAINT);
        crate::core::dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
        crate::core::dirty_registry::bump_subtree_gen(eid);
    }

    pub fn mark_measure_remote(dirty: &Rc<Cell<DirtyFlags>>, eid: ElementId) {
        dirty.set(dirty.get() | DirtyFlags::MEASURE);
        crate::core::dirty_registry::register_dirty(eid, DirtyFlags::MEASURE);
        crate::core::dirty_registry::bump_subtree_gen(eid);
    }
}

// ═══════════════════════ User data ═══════════════════════

impl Element {
    pub fn insert_user_data<T: 'static>(&mut self, v: T) {
        self.user_data
            .get_or_insert_with(|| Box::new(HashMap::new()))
            .insert(
                std::any::TypeId::of::<T>(),
                Box::new(v) as Box<dyn std::any::Any>,
            );
    }

    /// Attach a context menu that opens on right-click.
    /// The framework handles hit-testing, positioning, and dismiss
    /// automatically — the widget just calls this during mount.
    pub fn set_context_menu(&mut self, items: Vec<crate::widgets::overlay::ContextMenuItem>) {
        self.insert_user_data(crate::widgets::overlay::ContextMenuItems(items));
    }

    pub fn get_user_data<T: 'static>(&self) -> Option<&T> {
        self.user_data
            .as_ref()?
            .get(&std::any::TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    pub fn get_user_data_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.user_data
            .as_mut()?
            .get_mut(&std::any::TypeId::of::<T>())
            .and_then(|b| b.downcast_mut::<T>())
    }
}

// ═══════════════════════ ElementArena ═══════════════════════

/// A single slot in the arena's contiguous storage.
struct Slot {
    element: Option<Element>,
    /// Incremented each time this slot is reused.  Must match the
    /// `element.generation` field (and therefore the `ElementId`'s
    /// generation bits) for the slot to be considered occupied.
    generation: u32,
}

pub struct ElementArena {
    slots: Vec<Slot>,
    /// Indices of freed slots available for immediate reuse.
    free_list: Vec<u32>,
    pub root_id: Option<ElementId>,
    pub component_tables: std::rc::Rc<std::cell::RefCell<crate::ecs::tables::ComponentTables>>,
}

impl ElementArena {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_list: Vec::new(),
            root_id: None,
            component_tables: std::rc::Rc::new(std::cell::RefCell::new(
                crate::ecs::tables::ComponentTables::new(),
            )),
        }
    }

    /// Reserve the next free slot index, bump its generation, and return
    /// `(index, generation)`.  The caller must store an `Element` in the
    /// returned slot before returning.
    fn next_slot(&mut self) -> (u32, u32) {
        if let Some(idx) = self.free_list.pop() {
            let slot = &mut self.slots[idx as usize];
            slot.generation = slot.generation.wrapping_add(1);
            (idx, slot.generation)
        } else {
            let idx = self.slots.len() as u32;
            self.slots.push(Slot {
                element: None,
                generation: 1,
            });
            (idx, 1)
        }
    }

    /// Validate an `ElementId` against the arena's slot table.
    #[inline]
    fn validate(&self, id: ElementId) -> Option<usize> {
        let idx = id.index() as usize;
        let slot = self.slots.get(idx)?;
        if slot.generation != id.generation() {
            return None;
        }
        slot.element.as_ref()?;
        Some(idx)
    }

    pub fn allocate(&mut self) -> ElementId {
        let (index, gen) = self.next_slot();
        let mut el = Element::new(self.component_tables.clone());
        el.assign_id(index, gen);
        let id = el.id();
        crate::core::dirty_registry::register_element_full(
            id,
            el.dirty.clone(),
            el.state.clone(),
            true,
            false,
            el.subtree_generation.clone(),
            el.layout_generation.clone(),
            Some(el.surface_gen.clone()),
            Some(el.decor_gen.clone()),
            el.z_index,
            true,
            false,
            true,
            Some(el.slot_inactive.clone()),
            None,
            el.tree_order,
        );
        crate::core::dirty_registry::register_bounds(id, el.screen_bounds);
        self.slots[index as usize].element = Some(el);
        id
    }

    pub fn insert(&mut self, mut el: Element) -> ElementId {
        let (index, gen) = self.next_slot();
        // Ensure the element carries our ComponentTables reference.
        el.component_tables = self.component_tables.clone();
        el.assign_id(index, gen);
        let id = el.id();
        let si = Some(el.slot_inactive.clone());
        crate::core::dirty_registry::register_element_full(
            id,
            el.dirty.clone(),
            el.state.clone(),
            true,
            false,
            el.subtree_generation.clone(),
            el.layout_generation.clone(),
            Some(el.surface_gen.clone()),
            Some(el.decor_gen.clone()),
            el.z_index,
            true,
            false,
            true,
            si,
            None,
            el.tree_order,
        );
        crate::core::dirty_registry::register_bounds(id, el.screen_bounds);
        self.slots[index as usize].element = Some(el);
        id
    }

    pub fn set_root(&mut self, id: ElementId) {
        self.root_id = Some(id);
    }

    pub fn add_child(&mut self, p: ElementId, c: ElementId) {
        // Idempotent: skip if c is already a direct child of p (portals and
        // other overlay elements may be registered both through the widget
        // tree path and the drain_portals path).
        if self
            .get(p)
            .is_some_and(|parent| parent.children.contains(&c))
        {
            return;
        }
        // If reparenting (portal re-rooting), detach from the old parent so the
        // element is not painted twice — once from root and once from its
        // original mount point (e.g. inside a ScrollView). Without this, every
        // backdrop fill executes in duplicate.
        let old_parent = self.get(c).and_then(|child| child.parent);
        if let Some(op) = old_parent {
            if op != p {
                let didx = op.index() as usize;
                if let Some(ref mut op_el) = self.slots[didx].element {
                    op_el.children.retain(|&cc| cc != c);
                    *op_el.sorted_children.borrow_mut() = None;
                }
                crate::core::dirty_registry::mark_structurally_changed(op);
            }
        }
        let dp = self.get(p).map_or(0, |x| x.depth);
        {
            let cidx = c.index() as usize;
            if let Some(ref mut x) = self.slots[cidx].element {
                x.parent = Some(p);
                x.depth = dp + 1;
            }
        }
        // `c` may already carry a pre-built subtree whose descendants were given
        // depths relative to c's OLD depth (e.g. cells added to a row before the
        // row itself was attached). Re-propagate so `child.depth == parent.depth
        // + 1` holds throughout the attached subtree — hit_test_leaf's depth
        // tie-break (and any depth-ordered logic) relies on this invariant.
        self.propagate_depth(c);
        {
            let pidx = p.index() as usize;
            if let Some(ref mut x) = self.slots[pidx].element {
                x.children.push(c);
                *x.sorted_children.borrow_mut() = None;
            }
        }
        crate::core::dirty_registry::register_parent(c, p);
        crate::core::dirty_registry::invalidate_focus_order();
        crate::core::dirty_registry::mark_a11y_dirty();
        crate::core::dirty_registry::mark_structurally_changed(p);
        crate::ecs::mark_a11y_changed(p);
    }

    /// Re-propagate `depth` through `root`'s subtree so every node satisfies
    /// `child.depth == parent.depth + 1`. `root`'s own depth is taken as-is.
    fn propagate_depth(&mut self, root: ElementId) {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let d = match self.get(id) {
                Some(x) => x.depth,
                None => continue,
            };
            let kids: Vec<ElementId> = self.get(id).map(|x| x.children.clone()).unwrap_or_default();
            for k in kids {
                {
                    let kidx = k.index() as usize;
                    if let Some(ref mut x) = self.slots[kidx].element {
                        x.depth = d + 1;
                    }
                }
                stack.push(k);
            }
        }
    }

    pub fn remove_child(&mut self, p: ElementId, c: ElementId) -> Option<Element> {
        let pidx = p.index() as usize;
        let x = self.slots[pidx].element.as_mut()?;
        let pos = x.children.iter().position(|&z| z == c)?;
        x.children.remove(pos);
        *x.sorted_children.borrow_mut() = None;
        let removed = self.teardown_subtree(c);
        crate::core::dirty_registry::invalidate_focus_order();
        crate::core::dirty_registry::mark_a11y_dirty();
        crate::core::dirty_registry::mark_structurally_changed(p);
        crate::ecs::mark_a11y_changed(p);
        removed
    }

    pub fn remove(&mut self, id: ElementId) -> Option<Element> {
        let parent = self.get(id)?.parent;
        if let Some(pid) = parent {
            let pidx = pid.index() as usize;
            if let Some(ref mut p) = self.slots[pidx].element {
                p.children.retain(|&c| c != id);
                *p.sorted_children.borrow_mut() = None;
                crate::core::dirty_registry::mark_structurally_changed(pid);
            }
        }
        let removed = self.teardown_subtree(id);
        crate::core::dirty_registry::invalidate_focus_order();
        crate::core::dirty_registry::mark_a11y_dirty();
        removed
    }

    pub fn clear_children(&mut self, p: ElementId) {
        let pidx = p.index() as usize;
        let ids: Vec<ElementId> = if let Some(ref mut x) = self.slots[pidx].element {
            let ids = std::mem::take(&mut x.children);
            *x.sorted_children.borrow_mut() = None;
            ids
        } else {
            return;
        };
        for c in ids {
            self.teardown_subtree(c);
        }
        crate::core::dirty_registry::invalidate_focus_order();
        crate::core::dirty_registry::mark_a11y_dirty();
        crate::core::dirty_registry::mark_structurally_changed(p);
        crate::ecs::mark_a11y_changed(p);
    }

    /// Collect `root` and all its descendants (parents before children).
    fn collect_subtree_ids(&self, root: ElementId) -> Vec<ElementId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(cur) = stack.pop() {
            out.push(cur);
            if let Some(el) = self.get(cur) {
                stack.extend(el.children.iter().copied());
            }
        }
        out
    }

    /// Full recursive teardown of the subtree rooted at `root`
    /// (audit 2026-07-16, F1). Children are torn down before parents.
    ///
    /// Per node:
    /// 1. fire `on_unmount` (panic-isolated) — this makes the overlay
    ///    widgets' portal/modal-scope cleanup actually run,
    /// 2. drop component-table entries — dropping `LifecycleComponent`
    ///    drops its `subscriptions` handles, unsubscribing every signal
    ///    binding created during mount,
    /// 3. unregister from dirty registry, spatial grid, bounds, focus,
    /// 4. queue paint-cache eviction and EventRegistry handler removal
    ///    (drained by the frame driver, which owns the registry),
    /// 5. unregister gesture recognizers and ECS tracking sets,
    /// 6. remove the `Element` itself.
    ///
    /// Returns the root's `Element` (API compatibility with the old
    /// one-level `remove`). Before this existed, removing a subtree root
    /// leaked every grandchild: elements, components, subscriptions and
    /// handlers all stayed alive (measured: 51/52 elements leaked,
    /// `tests/lifecycle_leak_audit.rs`).
    fn teardown_subtree(&mut self, root: ElementId) -> Option<Element> {
        let ids = self.collect_subtree_ids(root);
        // Take the root element out first so we can return it to the caller.
        let root_el = {
            let ridx = root.index() as usize;
            self.slots[ridx].element.take()
        };
        // Reverse pre-order ≈ children before parents.
        for &id in ids.iter().rev() {
            if id == root {
                continue;
            } // already taken above
            self.fire_on_unmount(id);
            for portal in crate::platform::portal::take_portals_of(id) {
                crate::platform::portal::remove_portal(portal);
            }
            self.component_tables.borrow_mut().remove_element(id);
            crate::core::dirty_registry::unregister_parent(id);
            crate::core::dirty_registry::unregister_element(id);
            crate::core::dirty_registry::unregister_bounds(id);
            crate::core::dirty_registry::run_teardown_hooks(id);
            crate::core::app_context::with_current_app(|app| {
                app.queue_cache_eviction(id);
                app.queue_handler_removal(id);
            });
            crate::event::unregister_recognizer(id);
            crate::ecs::unregister_element(id);
            self.slots[id.index() as usize].element = None;
            self.free_list.push(id.index());
        }
        // Finalize the root (it was taken out before the loop).
        if let Some(ref _root) = root_el {
            self.fire_on_unmount(root);
            for portal in crate::platform::portal::take_portals_of(root) {
                crate::platform::portal::remove_portal(portal);
            }
            self.component_tables.borrow_mut().remove_element(root);
            crate::core::dirty_registry::unregister_parent(root);
            crate::core::dirty_registry::unregister_element(root);
            crate::core::dirty_registry::unregister_bounds(root);
            crate::core::dirty_registry::run_teardown_hooks(root);
            crate::core::app_context::with_current_app(|app| {
                app.queue_cache_eviction(root);
                app.queue_handler_removal(root);
            });
            crate::event::unregister_recognizer(root);
            crate::ecs::unregister_element(root);
            self.free_list.push(root.index());
        }
        root_el
    }

    /// Fire the element's `on_unmount` callback with panic isolation.
    /// The callback is taken out of the component table before invocation
    /// so re-entrant table access from inside the callback is safe.
    fn fire_on_unmount(&self, id: ElementId) {
        let cb = self
            .component_tables
            .borrow_mut()
            .lc
            .get_mut(&id)
            .and_then(|lc| lc.on_unmount.clone())
            .and_then(|rc| rc.borrow_mut().take());
        if let Some(f) = cb {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            if let Err(panic) = result {
                push_error(UiError::CallbackPanic {
                    context: "fire_on_unmount".into(),
                    window_id: None,
                    element_id: Some(id),
                    message: panic_to_string(&panic),
                });
            }
        }
        // Also fire the Element-level on_unmount hook (third-party extension).
        if let Some(el) = self.get(id) {
            if let Some(hook) = el.on_unmount.clone() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    hook(id);
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_on_unmount (element hook)".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    pub fn invalidate_sorted_children(&self, c: ElementId) {
        if let Some(p) = crate::core::dirty_registry::parent_of(c) {
            if let Some(x) = self.get(p) {
                *x.sorted_children.borrow_mut() = None;
            }
        }
    }

    #[inline]
    pub fn get(&self, id: ElementId) -> Option<&Element> {
        let idx = self.validate(id)?;
        self.slots[idx].element.as_ref()
    }

    #[inline]
    pub fn get_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        let idx = self.validate(id)?;
        self.slots[idx].element.as_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ElementId, &Element)> {
        self.slots.iter().filter_map(|s| {
            let el = s.element.as_ref()?;
            Some((el.id(), el))
        })
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.element.is_some()).count()
    }

    /// ── Component table accessors (read) ──

    pub fn comp_style(&self, eid: ElementId) -> Option<crate::ecs::components::StyleComponent> {
        self.component_tables.borrow().style.get(&eid).cloned()
    }
    pub fn comp_layout(&self, eid: ElementId) -> Option<crate::ecs::components::LayoutComponent> {
        self.component_tables.borrow().layout.get(&eid).cloned()
    }
    pub fn comp_interact(
        &self,
        eid: ElementId,
    ) -> Option<crate::ecs::components::InteractionComponent> {
        self.component_tables.borrow().interact.get(&eid).cloned()
    }
    pub fn comp_text(&self, eid: ElementId) -> Option<crate::ecs::components::TextComponent> {
        self.component_tables.borrow().text.get(&eid).cloned()
    }
    pub fn comp_scroll(&self, eid: ElementId) -> Option<crate::ecs::components::ScrollComponent> {
        self.component_tables.borrow().scroll.get(&eid).cloned()
    }
    pub fn comp_cursor(&self, eid: ElementId) -> Option<crate::ecs::components::CursorComponent> {
        self.component_tables.borrow().cursor.get(&eid).cloned()
    }
    pub fn comp_tooltip(&self, eid: ElementId) -> Option<crate::ecs::components::TooltipComponent> {
        self.component_tables.borrow().tooltip.get(&eid).cloned()
    }
    pub fn comp_dragdrop(
        &self,
        eid: ElementId,
    ) -> Option<crate::ecs::components::DragDropComponent> {
        self.component_tables.borrow().dragdrop.get(&eid).cloned()
    }
    pub fn comp_anim(&self, eid: ElementId) -> Option<crate::ecs::components::AnimationComponent> {
        self.component_tables.borrow().anim.get(&eid).cloned()
    }
    pub fn comp_xform(&self, eid: ElementId) -> Option<crate::ecs::components::TransformComponent> {
        self.component_tables.borrow().xform.get(&eid).cloned()
    }
    pub fn comp_a11y(&self, eid: ElementId) -> Option<crate::ecs::components::AccessibleComponent> {
        self.component_tables.borrow().a11y.get(&eid).cloned()
    }
    pub fn comp_lc(&self, eid: ElementId) -> Option<crate::ecs::components::LifecycleComponent> {
        self.component_tables.borrow().lc.get(&eid).cloned()
    }

    /// ── Setter helpers (delegate to Element methods for dirty-marking) ──

    pub fn set_background(&mut self, eid: ElementId, v: Color) {
        if let Some(el) = self.get_mut(eid) {
            el.set_background(v);
        }
    }
    pub fn set_foreground(&mut self, eid: ElementId, v: Color) {
        if let Some(el) = self.get_mut(eid) {
            el.set_foreground(v);
        }
    }
    pub fn set_border_color(&mut self, eid: ElementId, v: Color) {
        if let Some(el) = self.get_mut(eid) {
            el.set_border_color(v);
        }
    }
    pub fn set_border_width(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_border_width(v);
        }
    }
    pub fn set_outline_color(&mut self, eid: ElementId, v: Color) {
        if let Some(el) = self.get_mut(eid) {
            el.set_outline_color(v);
        }
    }
    pub fn set_outline_width(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_outline_width(v);
        }
    }
    pub fn set_shadow(&mut self, eid: ElementId, v: Option<Shadow>) {
        if let Some(el) = self.get_mut(eid) {
            el.set_shadow(v);
        }
    }
    pub fn set_gradient(&mut self, eid: ElementId, v: Option<LinearGradient>) {
        if let Some(el) = self.get_mut(eid) {
            el.set_gradient(v);
        }
    }
    pub fn set_text_decoration(&mut self, eid: ElementId, v: TextDecoration) {
        if let Some(el) = self.get_mut(eid) {
            el.set_text_decoration(v);
        }
    }
    pub fn set_text_overflow(&mut self, eid: ElementId, v: TextOverflow) {
        if let Some(el) = self.get_mut(eid) {
            el.set_text_overflow(v);
        }
    }
    pub fn set_backdrop(&mut self, eid: ElementId, v: bool) {
        if let Some(el) = self.get_mut(eid) {
            el.set_backdrop(v);
        }
    }
    pub fn set_corner_radius(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_corner_radius(v);
        }
    }
    pub fn set_corner_radii(&mut self, eid: ElementId, v: crate::style::CornerRadii) {
        if let Some(el) = self.get_mut(eid) {
            el.set_corner_radii(v);
        }
    }
    pub fn set_opacity(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_opacity(v);
        }
    }

    pub fn set_layout_direction(&mut self, eid: ElementId, v: LayoutDirection) {
        if let Some(el) = self.get_mut(eid) {
            el.set_layout_direction(v);
        }
    }
    pub fn set_gap(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_gap(v);
        }
    }
    pub fn set_margin(&mut self, eid: ElementId, v: Margin) {
        if let Some(el) = self.get_mut(eid) {
            el.set_margin(v);
        }
    }
    pub fn set_padding(&mut self, eid: ElementId, v: Padding) {
        if let Some(el) = self.get_mut(eid) {
            el.set_padding(v);
        }
    }
    pub fn set_alignment(&mut self, eid: ElementId, v: Alignment) {
        if let Some(el) = self.get_mut(eid) {
            el.set_alignment(v);
        }
    }
    pub fn set_content_align(&mut self, eid: ElementId, v: Alignment) {
        if let Some(el) = self.get_mut(eid) {
            el.set_content_align(v);
        }
    }
    pub fn set_preferred_width(&mut self, eid: ElementId, v: Option<f32>) {
        if let Some(el) = self.get_mut(eid) {
            el.set_preferred_width(v);
        }
    }
    pub fn set_preferred_height(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_preferred_height(v);
        }
    }
    pub fn set_flex_grow(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_flex_grow(v);
        }
    }
    pub fn set_flex_shrink(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_flex_shrink(v);
        }
    }
    pub fn set_flex_basis(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_flex_basis(v);
        }
    }
    pub fn set_min_main(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_min_main(v);
        }
    }
    pub fn set_flex_wrap(&mut self, eid: ElementId, v: FlexWrap) {
        if let Some(el) = self.get_mut(eid) {
            el.set_flex_wrap(v);
        }
    }
    pub fn set_aspect_ratio(&mut self, eid: ElementId, v: Option<f32>) {
        if let Some(el) = self.get_mut(eid) {
            el.set_aspect_ratio(v);
        }
    }
    pub fn set_overflow(&mut self, eid: ElementId, v: Overflow) {
        if let Some(el) = self.get_mut(eid) {
            el.set_overflow(v);
        }
    }
    pub fn set_order(&mut self, eid: ElementId, v: i32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_order(v);
        }
    }
    pub fn set_scrollbar_policy(&mut self, eid: ElementId, v: ScrollbarPolicy) {
        if let Some(el) = self.get_mut(eid) {
            el.set_scrollbar_policy(v);
        }
    }
    pub fn set_scrollbar_width(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_scrollbar_width(v);
        }
    }
    pub fn set_affected_by_child_size(&mut self, eid: ElementId, v: bool) {
        if let Some(el) = self.get_mut(eid) {
            el.set_affected_by_child_size(v);
        }
    }

    pub fn set_grid_columns(&mut self, eid: ElementId, v: u32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_grid_columns(v);
        }
    }
    pub fn set_grid_column_span(&mut self, eid: ElementId, v: u32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_grid_column_span(v);
        }
    }
    pub fn set_grid_column_offset(&mut self, eid: ElementId, v: u32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_grid_column_offset(v);
        }
    }

    pub fn set_font_size(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_font_size(v);
        }
    }
    pub fn set_font_weight(&mut self, eid: ElementId, v: u16) {
        if let Some(el) = self.get_mut(eid) {
            el.set_font_weight(v);
        }
    }
    pub fn set_font_family(&mut self, eid: ElementId, v: Option<String>) {
        if let Some(el) = self.get_mut(eid) {
            el.set_font_family(v);
        }
    }
    pub fn set_text_align(&mut self, eid: ElementId, v: TextAlign) {
        if let Some(el) = self.get_mut(eid) {
            el.set_text_align(v);
        }
    }
    pub fn set_text_direction(&mut self, eid: ElementId, v: TextDirection) {
        if let Some(el) = self.get_mut(eid) {
            el.set_text_direction(v);
        }
    }
    pub fn set_text_vertical_center(&mut self, eid: ElementId, v: bool) {
        if let Some(el) = self.get_mut(eid) {
            el.set_text_vertical_center(v);
        }
    }
    pub fn set_line_height(&mut self, eid: ElementId, v: f32) {
        if let Some(el) = self.get_mut(eid) {
            el.set_line_height(v);
        }
    }
    pub fn set_text_buffer(&mut self, eid: ElementId, v: Rc<RefCell<cosmic_text::Buffer>>) {
        self.component_tables
            .borrow_mut()
            .text
            .entry(eid)
            .or_default()
            .text_buffer = Some(v);
    }

    pub fn set_tab_index(&mut self, eid: ElementId, v: Option<usize>) {
        self.component_tables
            .borrow_mut()
            .interact
            .entry(eid)
            .or_default()
            .tab_index = v;
    }

    pub fn set_accessible_role(&mut self, eid: ElementId, v: accesskit::Role) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_role = Some(v);
    }
    pub fn set_accessible_label(&mut self, eid: ElementId, v: String) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_label = Some(v);
    }
    pub fn set_accessible_description(&mut self, eid: ElementId, v: String) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_description = Some(v);
    }
    pub fn set_accessible_value(&mut self, eid: ElementId, v: f64) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_value = Some(v);
    }
    pub fn set_accessible_hidden(&mut self, eid: ElementId, v: bool) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_hidden = v;
    }
    pub fn set_accessible_required(&mut self, eid: ElementId, v: bool) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_required = v;
    }
    pub fn set_accessible_level(&mut self, eid: ElementId, v: u32) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_level = Some(v);
    }
    pub fn set_accessible_live(&mut self, eid: ElementId, v: AriaLive) {
        self.component_tables
            .borrow_mut()
            .a11y
            .entry(eid)
            .or_default()
            .accessible_live = v;
    }

    pub fn set_scroll_offset(&mut self, eid: ElementId, v: Rc<Cell<Vec2>>) {
        self.component_tables
            .borrow_mut()
            .scroll
            .entry(eid)
            .or_default()
            .scroll_offset = v;
    }
    pub fn set_content_bounds(&mut self, eid: ElementId, v: Rc<Cell<Rect>>) {
        self.component_tables
            .borrow_mut()
            .scroll
            .entry(eid)
            .or_default()
            .content_bounds = v;
    }

    pub fn set_cursor_icon(&mut self, eid: ElementId, v: Option<crate::platform::CursorIcon>) {
        self.component_tables
            .borrow_mut()
            .cursor
            .entry(eid)
            .or_default()
            .cursor_icon = v;
    }

    pub fn set_state_dirty(&mut self, eid: ElementId, flag: StateFlags, on: bool) {
        if let Some(el) = self.get_mut(eid) {
            let mut s = el.state.get();
            if s.contains(flag) != on {
                s.set(flag, on);
                el.state.set(s);
                el.mark_surface_dirty();
            }
        }
    }

    pub fn set_visible(&mut self, eid: ElementId, v: bool) {
        if let Some(el) = self.get_mut(eid) {
            crate::core::dirty_registry::set_elinfo_visible(eid, v);
            el.mark_surface_dirty();
        }
    }

    pub fn set_invalid_hint(&mut self, eid: ElementId, v: Rc<Cell<bool>>) {
        self.component_tables
            .borrow_mut()
            .lc
            .entry(eid)
            .or_default()
            .invalid_hint = Some(v);
    }
    pub fn set_error_text(&mut self, eid: ElementId, v: Rc<RefCell<Option<String>>>) {
        self.component_tables
            .borrow_mut()
            .lc
            .entry(eid)
            .or_default()
            .error_text = Some(v);
    }

    pub fn set_reactive_visible(&mut self, eid: ElementId, v: Rc<Cell<bool>>) {
        self.component_tables
            .borrow_mut()
            .lc
            .entry(eid)
            .or_default()
            .reactive_visible = Some(v.clone());
        crate::core::dirty_registry::set_elinfo_reactive_visible(eid, v);
    }
    pub fn set_on_mount(&mut self, eid: ElementId, v: Box<dyn FnOnce()>) {
        self.component_tables
            .borrow_mut()
            .lc
            .entry(eid)
            .or_default()
            .on_mount = Some(Rc::new(RefCell::new(Some(v))));
        crate::ecs::register_on_mount(eid);
    }
    pub fn set_apply_drag_layout(
        &mut self,
        eid: ElementId,
        f: Box<dyn Fn(&mut ElementArena, ElementId)>,
    ) {
        self.component_tables
            .borrow_mut()
            .lc
            .entry(eid)
            .or_default()
            .apply_drag_layout = Some(f.into());
        crate::ecs::register_drag_element(eid);
    }
    pub fn set_frame_tick(&mut self, eid: ElementId, v: Rc<RefCell<Option<Box<dyn Fn()>>>>) {
        self.component_tables
            .borrow_mut()
            .lc
            .entry(eid)
            .or_default()
            .frame_tick = Some(v);
        crate::ecs::active::register_active(eid, crate::ecs::active::ActiveTag::FrameTick);
    }

    pub fn set_position_offset(&mut self, eid: ElementId, v: Rc<Cell<Vec2>>) {
        self.component_tables
            .borrow_mut()
            .xform
            .entry(eid)
            .or_default()
            .position_offset = v;
        crate::core::dirty_registry::spatial_register_position_offset(eid);
    }
    pub fn set_size_scale(&mut self, eid: ElementId, v: Rc<Cell<Vec2>>) {
        self.component_tables
            .borrow_mut()
            .xform
            .entry(eid)
            .or_default()
            .size_scale = v;
    }
    pub fn set_focusable(&mut self, eid: ElementId, v: bool) {
        let was = self
            .component_tables
            .borrow()
            .interact
            .get(&eid)
            .map(|i| i.focusable)
            .unwrap_or(false);
        self.component_tables
            .borrow_mut()
            .interact
            .entry(eid)
            .or_default()
            .focusable = v;
        if v != was {
            if let Some(el) = self.get(eid) {
                let ti = self
                    .component_tables
                    .borrow()
                    .interact
                    .get(&eid)
                    .and_then(|i| i.tab_index);
                if v {
                    crate::core::dirty_registry::register_focusable(eid, ti, el.tree_order);
                } else {
                    crate::core::dirty_registry::unregister_focusable(eid, ti, el.tree_order);
                }
            }
        }
    }
    pub fn set_bounds(&mut self, eid: ElementId, v: Rect) {
        if let Some(el) = self.get_mut(eid) {
            el.screen_bounds = v;
        }
    }

    /// Walk the ancestor chain from `eid` to root, summing scroll offsets.
    pub fn accumulated_scroll(&self, eid: ElementId) -> (f32, f32) {
        crate::core::dirty_registry::accumulated_scroll_cached(self, eid)
    }

    /// Apply the inverse of the accumulated transform from root to `eid`.
    pub fn inverse_transform_point(
        &self,
        _eid: ElementId,
        point: crate::style::Point,
    ) -> crate::style::Point {
        point
    }

    /// Find the deepest visible, mouse-accepting leaf at `point`.
    /// NOTE: Performs O(N) linear scan over all registered elements.
    /// The spatial_hit_test path covers most interactivity; this fallback
    /// exists for elements not yet in the spatial index. If performance
    /// becomes an issue, consider ensuring all interactive elements are
    /// registered in the spatial index via register_bounds().
    pub fn hit_test_leaf(&self, point: crate::style::Point) -> Option<ElementId> {
        #[cfg(debug_assertions)]
        crate::core::dirty_registry::inc_hittest_leaf_fallback();

        let mut best: Option<(ElementId, u32, i32)> = None;
        for (eid, el) in self.iter() {
            let visible = !el.slot_inactive.get()
                && crate::core::dirty_registry::is_elinfo_visible(eid)
                && (self
                    .comp_lc(eid)
                    .is_none_or(|lc| lc.reactive_visible.as_ref().is_none_or(|c| c.get())));
            let accepts = self.comp_interact(eid).is_none_or(|i| i.accepts_mouse);
            let pass = self
                .comp_interact(eid)
                .is_some_and(|i| i.input_pass_through);
            if !visible || !accepts || pass {
                continue;
            }
            let b = el.screen_bounds;
            // Render draws this element at (screen_bounds + position_offset);
            // map the click back into layout space before the bounds test so
            // the hit region tracks the visual position (parity with the GPU/
            // CPU paint paths and spatial_hit_test).
            let p = match el.position_offset() {
                Some(off) => {
                    let o = off.get();
                    crate::style::Point::new(point.x - o.x, point.y - o.y)
                }
                None => point,
            };
            if b.contains(p) {
                match best {
                    Some((_, best_depth, best_z)) => {
                        let el_z = el.z_index;
                        if el_z > best_z || (el_z == best_z && el.depth > best_depth) {
                            best = Some((eid, el.depth, el_z));
                        }
                    }
                    None => best = Some((eid, el.depth, el.z_index)),
                }
            }
        }
        best.map(|(eid, _, _)| eid)
    }

    /// Collect the ancestor chain from `eid` to root (inclusive).
    pub fn path_to_root(&self, eid: ElementId) -> Vec<ElementId> {
        let mut path = Vec::new();
        let mut cur = Some(eid);
        while let Some(cid) = cur {
            path.push(cid);
            cur = self.get(cid).and_then(|el| el.parent);
        }
        path
    }
}

pub fn process_exits(arena: &mut ElementArena) {
    let exits = crate::core::dirty_registry::drain_exits();
    if exits.is_empty() {
        return;
    }
    let mut still_active = Vec::new();
    for eid in exits {
        let done = arena.comp_anim(eid).is_none_or(|a| {
            a.exit_pending
                .is_none_or(|(_, _, done_cell)| done_cell.get())
        });
        if done {
            arena.set_visible(eid, false);
        } else {
            still_active.push(eid);
        }
    }
    if still_active.is_empty() {
        // EXIT_LIST is now empty → is_exit_pending_active() naturally returns false.
    } else {
        for eid in still_active {
            crate::core::dirty_registry::register_exit(eid);
        }
    }
}

pub fn reapply_element_theme(
    arena: &mut ElementArena,
    _root: ElementId,
    theme: &dyn crate::theme::Theme,
) {
    let eids = crate::ecs::drain_theme_elements();
    for eid in eids {
        let role = {
            let tables = arena.component_tables.borrow();
            tables.lc.get(&eid).and_then(|lc| lc.component_role.clone())
        };

        if let Some(ref role) = role {
            let resolved = theme.resolve_component(role);
            let overrides = {
                let tables = arena.component_tables.borrow();
                tables
                    .lc
                    .get(&eid)
                    .and_then(|lc| lc.style_refinement.clone())
                    .unwrap_or_default()
            };
            if let Some(el) = arena.get_mut(eid) {
                crate::theme::apply::apply_style_to_element(
                    el,
                    &resolved,
                    &overrides,
                    theme.is_dark(),
                    0.5,
                );
            }
        }

        crate::ecs::register_theme_element(eid);
    }
}

pub fn apply_drag_layouts(arena: &mut ElementArena, _root: ElementId) {
    let eids = crate::ecs::drain_drag_elements();
    for eid in eids {
        let mut cb = None;
        if let Some(lc) = arena.component_tables.borrow_mut().lc.get_mut(&eid) {
            cb = lc.apply_drag_layout.take();
        }
        if let Some(ref f) = cb {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                f(arena, eid);
            }));
            if let Err(panic) = result {
                push_error(UiError::CallbackPanic {
                    context: "apply_drag_layouts".into(),
                    window_id: None,
                    element_id: Some(eid),
                    message: panic_to_string(&panic),
                });
            }
        }
        if let Some(f_val) = cb {
            if let Some(lc) = arena.component_tables.borrow_mut().lc.get_mut(&eid) {
                lc.apply_drag_layout = Some(f_val);
            }
            crate::ecs::register_drag_element(eid);
        }
    }
}

pub fn mark_subtree_repaint(arena: &ElementArena, root: ElementId) {
    let mut s = vec![root];
    while let Some(eid) = s.pop() {
        if let Some(el) = arena.get(eid) {
            el.mark_repaint();
            for &c in el.children.iter().rev() {
                s.push(c);
            }
        }
    }
}

/// Fire on_mount callbacks for elements that were registered.
pub fn fire_on_mount(arena: &mut ElementArena) {
    let pending = crate::ecs::drain_mount_callbacks();
    for eid in pending {
        if let Some(lc) = arena.component_tables.borrow_mut().lc.get_mut(&eid) {
            if let Some(rc) = &mut lc.on_mount {
                if let Some(f) = rc.borrow_mut().take() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        f();
                    }));
                    if let Err(panic) = result {
                        push_error(UiError::CallbackPanic {
                            context: "fire_on_mount".into(),
                            window_id: None,
                            element_id: Some(eid),
                            message: panic_to_string(&panic),
                        });
                    }
                }
            }
        }
        // Also fire the Element-level on_mount hook (third-party extension).
        if let Some(el) = arena.get(eid) {
            if let Some(hook) = el.on_mount.clone() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    hook(eid);
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_on_mount (element hook)".into(),
                        window_id: None,
                        element_id: Some(eid),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod arena_depth_tests {
    use super::*;

    /// When a subtree is built (child under parent) BEFORE the parent is attached
    /// to a deeper ancestor, attaching the parent must re-propagate depth through
    /// the subtree so `child.depth == parent.depth + 1` holds. hit_test_leaf's
    /// depth tie-break relies on this invariant.
    #[test]
    fn add_child_propagates_depth_to_prebuilt_subtree() {
        let mut arena = ElementArena::new();
        let grandparent = arena.allocate();
        let parent = arena.allocate();
        let child = arena.allocate();

        // Build child under parent while parent.depth is still 0.
        arena.add_child(parent, child);
        // Attach parent under grandparent -> parent.depth becomes 1.
        arena.add_child(grandparent, parent);

        assert_eq!(
            arena.get(parent).map(|e| e.depth),
            Some(1),
            "parent depth = grandparent.depth + 1",
        );
        assert_eq!(
            arena.get(child).map(|e| e.depth),
            Some(2),
            "child depth must re-propagate to parent.depth + 1 when the ancestor attaches",
        );
    }
}
