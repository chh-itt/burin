//! DevTools instrumentation layer (#[cfg(feature = "devtools")]).
//!
//! # Layer 3 — wall-clock timing + element diff + signal change stream
//!
//! ## Public API
//!
//! ```ignore
//! let inspector = DevtoolsInspector::attach().unwrap();
//! let snap = inspector.latest(window_id);
//! println!("frame {}: {}µs, {} elements, {} changed",
//!     snap.frame_id, snap.frame_timing.frame_total_us,
//!     snap.element_count, snap.element_changes.len());
//! ```
//!
//! ## Data model
//!
//! ```text
//! FrameSnapshot
//!   ├── frame_timing: FrameTiming      ← wall-clock (matches [dirty-bench])
//!   ├── elements: Vec<ElementFullSnapshot>  ← full tree (time travel)
//!   ├── element_changes: Vec<ElementChange> ← diff from previous frame
//!   ├── signal_changes: Vec<SignalChange>   ← which signals mutated this frame
//!   ├── dirty_events: Vec<DirtyEvent>       ← raw dirty propagation trace
//!   ├── cache_stats / layout_stats          ← aggregate stats
//!   └── counts: element / dirty / paint / fps
//! ```

use std::cell::RefCell;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use rustc_hash::FxHasher;

use serde::{Deserialize, Serialize};

use crate::core::config::StateFlags;
use crate::core::element::{DirtyFlags, ElementArena, ElementId};
use crate::style::{Color, Rect};

// ═══════════════════════ ElementFullSnapshot ═══════════════════════

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ElementFullSnapshot {
    pub id: ElementId,
    pub parent: Option<ElementId>,
    pub depth: u32,
    pub element_type: Option<String>,
    pub kind: String,
    pub debug_label: Option<String>,
    pub test_id: Option<String>,
    pub children: Vec<ElementId>,
    pub tree_order: u64,
    pub generation: u32,
    pub slot_inactive: bool,
    pub screen_bounds: Rect,
    pub z_index: i32,
    pub dirty_flags: DirtyFlags,
    pub state_flags: StateFlags,
    pub surface_gen: u64,
    pub decor_gen: u64,
    pub subtree_gen: u64,
    pub layout_gen: u64,
    pub background: Option<Color>,
    pub foreground: Option<Color>,
    pub border_width: f32,
    pub border_color: Option<Color>,
    pub corner_radius: f32,
    pub shadow: Option<crate::style::styled::Shadow>,
    pub opacity: f32,
    pub layout_direction: String,
    pub alignment: String,
    pub content_align: String,
    pub preferred_width: Option<f32>,
    pub preferred_height: f32,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub margin: crate::style::Margin,
    pub padding: crate::style::Padding,
    pub gap: f32,
    pub overflow: crate::core::config::Overflow,
    pub focusable: bool,
    pub tab_index: Option<usize>,
    pub selected: bool,
    pub read_only: bool,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub text_align: String,
    pub text_label: Option<String>,
    pub scroll_offset: Option<(f32, f32)>,
    pub is_scrollable: bool,
    pub draggable: bool,
    pub accessible_role: String,
    pub accessible_label: Option<String>,
    pub is_visible: bool,
}

fn derive_kind(
    el: &crate::core::element::Element,
    a11y: &Option<crate::ecs::components::AccessibleComponent>,
    scroll: &Option<crate::ecs::components::ScrollComponent>,
    text: &Option<crate::ecs::components::TextComponent>,
    interact: &Option<crate::ecs::components::InteractionComponent>,
) -> &'static str {
    if scroll.is_some() {
        return "ScrollView";
    }
    if let Some(a) = a11y {
        match a.accessible_role {
            Some(accesskit::Role::Button) => return "Button",
            Some(accesskit::Role::CheckBox) => return "Checkbox",
            Some(accesskit::Role::Tree) => return "Tree",
            Some(accesskit::Role::TreeItem) => return "TreeItem",
            Some(accesskit::Role::Row) => return "Row",
            Some(accesskit::Role::GridCell) => return "Cell",
            Some(accesskit::Role::Group) => {
                if text.is_some() {
                    return "Label";
                }
                return "Group";
            }
            Some(accesskit::Role::Image) => return "Image",
            Some(accesskit::Role::Slider) => return "Slider",
            Some(accesskit::Role::ProgressIndicator) => return "Progress",
            Some(accesskit::Role::List) => return "List",
            Some(accesskit::Role::ListItem) => return "Item",
            _ => {}
        }
    }
    if text.is_some() {
        return "Text";
    }
    if interact.as_ref().map_or(false, |i| i.focusable) {
        return "Focusable";
    }
    if el.children.is_empty() {
        return "Leaf";
    }
    match el.layout_direction() {
        crate::core::element::LayoutDirection::Vertical => "VStack",
        crate::core::element::LayoutDirection::Horizontal => "HStack",
    }
}

pub fn read_element_full(arena: &ElementArena, eid: ElementId) -> Option<ElementFullSnapshot> {
    let el = arena.get(eid)?;
    let style = arena.comp_style(eid);
    let layout = arena.comp_layout(eid);
    let interact = arena.comp_interact(eid);
    let text = arena.comp_text(eid);
    let scroll = arena.comp_scroll(eid);
    let a11y = arena.comp_a11y(eid);
    let dragdrop = arena.comp_dragdrop(eid);
    let lc = arena.comp_lc(eid);

    let scroll_offset = scroll.as_ref().map(|s| {
        let so = s.scroll_offset.get();
        (so.x, so.y)
    });
    let is_scrollable = scroll.is_some();
    let is_visible =
        !el.slot_inactive.get() && el.screen_bounds.width > 0.0 && el.screen_bounds.height > 0.0;
    let kind = derive_kind(el, &a11y, &scroll, &text, &interact);

    Some(ElementFullSnapshot {
        id: eid,
        parent: el.parent,
        depth: el.depth,
        element_type: el.element_type.map(|s| s.to_string()),
        kind: kind.to_string(),
        debug_label: lc.as_ref().and_then(|l| l.debug_label.clone()),
        test_id: lc.as_ref().and_then(|l| l.test_id.clone()),
        children: el.children.clone(),
        tree_order: el.tree_order,
        generation: el.generation,
        slot_inactive: el.slot_inactive.get(),
        screen_bounds: el.screen_bounds,
        z_index: el.z_index,
        dirty_flags: el.dirty.get(),
        state_flags: el.state.get(),
        surface_gen: el.surface_gen.get(),
        decor_gen: el.decor_gen.get(),
        subtree_gen: el.subtree_generation.get(),
        layout_gen: el.layout_generation.get(),
        background: style.as_ref().and_then(|s| s.background),
        foreground: style.as_ref().and_then(|s| s.foreground),
        border_width: style.as_ref().map_or(0.0, |s| s.border_width),
        border_color: style.as_ref().and_then(|s| s.border_color),
        corner_radius: style.as_ref().map_or(0.0, |s| s.corner_radius),
        shadow: style.as_ref().and_then(|s| s.shadow.clone()),
        opacity: style.as_ref().map_or(1.0, |s| s.opacity),
        layout_direction: match el.layout_direction() {
            crate::core::element::LayoutDirection::Vertical => "Vertical".to_string(),
            crate::core::element::LayoutDirection::Horizontal => "Horizontal".to_string(),
        },
        alignment: align_str(layout.as_ref().map(|l| l.alignment)).to_string(),
        content_align: align_str(layout.as_ref().map(|l| l.content_align)).to_string(),
        preferred_width: layout.as_ref().and_then(|l| l.preferred_width),
        preferred_height: layout.as_ref().map_or(0.0, |l| l.preferred_height),
        flex_grow: layout.as_ref().map_or(0.0, |l| l.flex_grow),
        flex_shrink: layout.as_ref().map_or(1.0, |l| l.flex_shrink),
        margin: layout
            .as_ref()
            .map_or(crate::style::Margin::ZERO, |l| l.margin),
        padding: layout
            .as_ref()
            .map_or(crate::style::Padding::ZERO, |l| l.padding),
        gap: layout.as_ref().map_or(0.0, |l| l.gap),
        overflow: layout
            .as_ref()
            .map_or(crate::core::config::Overflow::Visible, |l| l.overflow),
        focusable: interact.as_ref().map_or(false, |i| i.focusable),
        tab_index: interact.as_ref().and_then(|i| i.tab_index),
        selected: interact.as_ref().map_or(false, |i| i.selected),
        read_only: interact.as_ref().map_or(false, |i| i.read_only),
        font_size: text.as_ref().map_or(14.0, |t| t.font_size),
        font_weight: text.as_ref().map_or(400, |t| t.font_weight),
        line_height: text.as_ref().map_or(1.2, |t| t.line_height),
        text_align: match text.as_ref().map(|t| t.text_align) {
            Some(crate::style::TextAlign::Left) => "Left",
            Some(crate::style::TextAlign::Center) => "Center",
            Some(crate::style::TextAlign::Right) => "Right",
            Some(crate::style::TextAlign::Justify) => "Justify",
            Some(crate::style::TextAlign::Start) => "Start",
            Some(crate::style::TextAlign::End) => "End",
            None => "Left",
        }
        .to_string(),
        text_label: text.as_ref().and_then(|t| {
            t.lazy_label.as_ref().and_then(|l| {
                let s = l.take();
                let label = if s.is_empty() { None } else { Some(s.clone()) };
                l.set(s);
                label
            })
        }),
        scroll_offset,
        is_scrollable,
        draggable: dragdrop.as_ref().map_or(false, |d| d.draggable),
        accessible_role: role_str(a11y.as_ref().and_then(|a| a.accessible_role)).to_string(),
        accessible_label: a11y.as_ref().and_then(|a| a.accessible_label.clone()),
        is_visible,
    })
}

fn align_str(a: Option<crate::style::Alignment>) -> &'static str {
    match a {
        Some(crate::style::Alignment::Start) => "Start",
        Some(crate::style::Alignment::Center) => "Center",
        Some(crate::style::Alignment::End) => "End",
        Some(crate::style::Alignment::Stretch) => "Stretch",
        None => "Start",
    }
}

fn role_str(role: Option<accesskit::Role>) -> &'static str {
    match role {
        Some(accesskit::Role::Button) => "Button",
        Some(accesskit::Role::CheckBox) => "CheckBox",
        Some(accesskit::Role::ComboBox) => "ComboBox",
        Some(accesskit::Role::Dialog) => "Dialog",
        Some(accesskit::Role::Group) => "Group",
        Some(accesskit::Role::Grid) => "Grid",
        Some(accesskit::Role::GridCell) => "GridCell",
        Some(accesskit::Role::Image) => "Image",
        Some(accesskit::Role::Label) => "Label",
        Some(accesskit::Role::Link) => "Link",
        Some(accesskit::Role::List) => "List",
        Some(accesskit::Role::ListItem) => "ListItem",
        Some(accesskit::Role::Menu) => "Menu",
        Some(accesskit::Role::MenuItem) => "MenuItem",
        Some(accesskit::Role::ProgressIndicator) => "ProgressBar",
        Some(accesskit::Role::RadioButton) => "RadioButton",
        Some(accesskit::Role::Row) => "Row",
        Some(accesskit::Role::ScrollBar) => "ScrollBar",
        Some(accesskit::Role::Slider) => "Slider",
        Some(accesskit::Role::Splitter) => "Splitter",
        Some(accesskit::Role::Tab) => "Tab",
        Some(accesskit::Role::TabList) => "TabList",
        Some(accesskit::Role::TabPanel) => "TabPanel",
        Some(accesskit::Role::Table) => "Table",
        Some(accesskit::Role::TextInput) => "TextInput",
        Some(accesskit::Role::Tooltip) => "Tooltip",
        Some(accesskit::Role::Tree) => "Tree",
        Some(accesskit::Role::TreeItem) => "TreeItem",
        Some(accesskit::Role::Unknown) => "Unknown",
        None => "None",
        _ => "Other",
    }
}

// ═══════════════════════ Layer 3: wall-clock + diff + stream ═══════════════════════

/// Wall-clock frame timing — matches console `[dirty-bench]` output.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FrameTiming {
    /// `frame_start.elapsed()` at end of on_frame (total wall-clock from frame start).
    pub frame_total_us: u64,
    /// `paint_start.elapsed()` at end of on_frame (includes backend render).
    pub paint_total_us: u64,
    /// Per-phase wall-clock timing (indexed by PerfPhase discriminant).
    /// `per_phase_us[PerfPhase::Layout as usize]` gives layout time.
    pub per_phase_us: [u64; crate::core::perf::PerfPhase::COUNT as usize],
}

/// A structural change to an element between frames.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ElementChange {
    /// Element appeared this frame.
    Added(ElementId),
    /// Element disappeared this frame.
    Removed { id: ElementId, prev_kind: String },
    /// Element was modified — which fields changed?
    Modified {
        id: ElementId,
        kind: String,
        /// Human-readable diff description (e.g. "screen_bounds, dirty_flags").
        what_changed: Vec<String>,
    },
}

/// A signal mutation event captured this frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignalChange {
    pub signal_addr: usize,
    pub label: String,
    pub version: u64,
    pub update_count: u64,
    pub timestamp_us: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub subtree_cache_hits: u64,
    pub subtree_cache_misses: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f32 {
        let total = self.subtree_cache_hits + self.subtree_cache_misses;
        if total == 0 {
            0.0
        } else {
            self.subtree_cache_hits as f32 / total as f32 * 100.0
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LayoutStats {
    pub incremental_taken: bool,
    pub escalation_taken: bool,
    pub full_pass: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirtyEvent {
    pub element_id: ElementId,
    pub element_type: Option<String>,
    pub trigger: DirtyTrigger,
    pub timestamp_us: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DirtyTrigger {
    SignalSet { signal_name: String },
    PointerEvent { kind: String },
    FrameTick,
    ChildDirty,
    DeferredAction,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameSnapshot {
    pub frame_id: u64,
    pub timestamp_ms: u64,

    /// Wall-clock timing.
    pub frame_timing: FrameTiming,

    /// Full element tree (for time-travel inspection).
    pub elements: Vec<ElementFullSnapshot>,

    /// Structural element changes from previous frame.
    pub element_changes: Vec<ElementChange>,

    /// Signals that mutated this frame.
    pub signal_changes: Vec<SignalChange>,

    /// Raw dirty propagation events.
    pub dirty_events: Vec<DirtyEvent>,

    /// Causal links: which signal (by `state_addr`) caused which element
    /// to be marked dirty during subscriber callback execution.
    pub signal_element_links: Vec<crate::core::dirty_registry::SignalElementLink>,

    /// Aggregate stats.
    pub cache_stats: CacheStats,
    pub layout_stats: LayoutStats,

    /// Counts.
    pub element_count: usize,
    pub dirty_count: usize,
    pub paint_count: usize,
    pub fps: f32,
}

// ═══════════════════════ Ring Buffer ═══════════════════════

pub type DevtoolsRingBuffer =
    Rc<RefCell<FxHashMap<winit::window::WindowId, VecDeque<FrameSnapshot>>>>;

pub const RING_BUFFER_CAPACITY: usize = 600;

pub fn new_ring_buffer() -> DevtoolsRingBuffer {
    Rc::new(RefCell::new(FxHashMap::default()))
}

pub fn push_snapshot(
    buf: &DevtoolsRingBuffer,
    window_id: winit::window::WindowId,
    snapshot: FrameSnapshot,
) {
    let mut map = buf.borrow_mut();
    let deque = map
        .entry(window_id)
        .or_insert_with(|| VecDeque::with_capacity(RING_BUFFER_CAPACITY));
    if deque.len() >= RING_BUFFER_CAPACITY {
        deque.pop_front();
    }
    deque.push_back(snapshot);
}

/// Push a snapshot without requiring a `WindowId` — used by `TestHarness`
/// and other test infrastructure. Snapshots are keyed under a synthetic id.
#[cfg(feature = "devtools")]
pub fn push_snapshot_for_test(snapshot: FrameSnapshot) {
    TEST_SNAPSHOTS.with(|v| {
        let mut v = v.borrow_mut();
        if v.len() >= RING_BUFFER_CAPACITY {
            v.remove(0);
        }
        v.push(snapshot);
    });
}

thread_local! {
    static TEST_SNAPSHOTS: std::cell::RefCell<Vec<FrameSnapshot>> =
        std::cell::RefCell::new(Vec::with_capacity(RING_BUFFER_CAPACITY));
}

/// Drain the test snapshot buffer (used by probe tests to verify collected data).
#[cfg(feature = "devtools")]
pub fn drain_test_snapshots() -> Vec<FrameSnapshot> {
    TEST_SNAPSHOTS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

pub fn latest_snapshot(
    buf: &DevtoolsRingBuffer,
    window_id: winit::window::WindowId,
) -> Option<FrameSnapshot> {
    buf.borrow().get(&window_id).and_then(|d| d.back().cloned())
}

pub fn remove_window(buf: &DevtoolsRingBuffer, window_id: winit::window::WindowId) {
    buf.borrow_mut().remove(&window_id);
}

// ═══════════════════════ collect_frame_snapshot ═══════════════════════

pub fn collect_frame_snapshot(
    arena: &ElementArena,
    frame_id: u64,
    timestamp_ms: u64,
    painted_count: usize,
    fps: f32,
    frame_total_us: u64,
    paint_total_us: u64,
) -> FrameSnapshot {
    let elements: Vec<ElementFullSnapshot> = arena
        .iter()
        .filter(|(_, el)| !el.slot_inactive.get())
        .filter_map(|(eid, _)| read_element_full(arena, eid))
        .collect();

    let all_phases = crate::core::perf::perf_drain_frame();
    let per_phase_us = all_phases.phases;
    let frame_timing = FrameTiming {
        frame_total_us,
        paint_total_us,
        per_phase_us,
    };

    let cache_stats = CacheStats {
        subtree_cache_hits: crate::core::frame_pipeline::subtree_cache_hits(),
        subtree_cache_misses: crate::core::frame_pipeline::subtree_cache_misses(),
    };

    let inc = crate::core::frame_pipeline::incremental_taken_count() > 0;
    let esc = crate::core::frame_pipeline::escalation_taken_count() > 0;
    let layout_stats = LayoutStats {
        incremental_taken: inc,
        escalation_taken: esc,
        full_pass: !inc && !esc,
    };

    let dirty_events = crate::core::dirty_registry::drain_dirty_events_raw()
        .into_iter()
        .map(|(element_id, _flags, timestamp_us, trigger_tag)| {
            let element_type = arena
                .get(element_id)
                .and_then(|el| el.element_type.map(|s| s.to_string()));
            let trigger = match trigger_tag {
                crate::core::dirty_registry::DirtyTriggerTag::SignalSet => {
                    DirtyTrigger::SignalSet {
                        signal_name: String::new(),
                    }
                }
                crate::core::dirty_registry::DirtyTriggerTag::PointerEvent => {
                    DirtyTrigger::PointerEvent {
                        kind: String::new(),
                    }
                }
                crate::core::dirty_registry::DirtyTriggerTag::FrameTick => DirtyTrigger::FrameTick,
                crate::core::dirty_registry::DirtyTriggerTag::DeferredAction => {
                    DirtyTrigger::DeferredAction
                }
                crate::core::dirty_registry::DirtyTriggerTag::ChildDirty => {
                    DirtyTrigger::ChildDirty
                }
                crate::core::dirty_registry::DirtyTriggerTag::Animation
                | crate::core::dirty_registry::DirtyTriggerTag::Unknown => DirtyTrigger::Unknown,
            };
            DirtyEvent {
                element_id,
                element_type,
                trigger,
                timestamp_us,
            }
        })
        .collect();

    let element_changes = compute_element_changes(&elements);

    let signal_changes = drain_signal_changes();

    let signal_element_links = crate::core::dirty_registry::drain_signal_element_links();

    let dirty_count = crate::core::dirty_registry::devtools_dirty_count();
    let element_count = crate::core::dirty_registry::element_count();

    FrameSnapshot {
        frame_id,
        timestamp_ms,
        frame_timing,
        elements,
        element_changes,
        signal_changes,
        dirty_events,
        signal_element_links,
        cache_stats,
        layout_stats,
        element_count,
        dirty_count,
        paint_count: painted_count,
        fps,
    }
}

// ═══════════════════════ Element Diff ═══════════════════════

thread_local! {
    static PREV_ELEMENTS: RefCell<FxHashMap<ElementId, ElementHash>> = RefCell::new(FxHashMap::default());
}

#[derive(Clone, Debug, PartialEq)]
struct ElementHash {
    bounds: (f32, f32, f32, f32),
    z_index: i32,
    dirty_raw: u32,
    state_raw: u32,
    surface_gen: u64,
    decor_gen: u64,
    subtree_gen: u64,
    layout_gen: u64,
    opacity: u8,
    bg_hash: u64,
    fg_hash: u64,
    slot_inactive: bool,
    scroll_offset_hash: u64,
    text_label_hash: u64,
}

fn hash_color(c: Option<&Color>) -> u64 {
    let mut hasher = FxHasher::default();
    match c {
        Some(color) => {
            color.r.to_bits().hash(&mut hasher);
            color.g.to_bits().hash(&mut hasher);
            color.b.to_bits().hash(&mut hasher);
            color.a.to_bits().hash(&mut hasher);
        }
        None => {
            0u64.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn hash_element(snap: &ElementFullSnapshot) -> ElementHash {
    let mut hasher = FxHasher::default();
    snap.text_label.hash(&mut hasher);
    let text_label_hash = hasher.finish();

    ElementHash {
        bounds: (
            snap.screen_bounds.x,
            snap.screen_bounds.y,
            snap.screen_bounds.width,
            snap.screen_bounds.height,
        ),
        z_index: snap.z_index,
        dirty_raw: snap.dirty_flags.0,
        state_raw: snap.state_flags.0 as u32,
        surface_gen: snap.surface_gen,
        decor_gen: snap.decor_gen,
        subtree_gen: snap.subtree_gen,
        layout_gen: snap.layout_gen,
        opacity: (snap.opacity * 255.0).clamp(0.0, 255.0) as u8,
        bg_hash: hash_color(snap.background.as_ref()),
        fg_hash: hash_color(snap.foreground.as_ref()),
        slot_inactive: snap.slot_inactive,
        scroll_offset_hash: {
            let mut hasher = FxHasher::default();
            match snap.scroll_offset {
                Some((x, y)) => {
                    x.to_bits().hash(&mut hasher);
                    y.to_bits().hash(&mut hasher);
                }
                None => {
                    0u64.hash(&mut hasher);
                }
            }
            hasher.finish()
        },
        text_label_hash,
    }
}

fn compute_element_changes(current: &[ElementFullSnapshot]) -> Vec<ElementChange> {
    PREV_ELEMENTS.with(|prev_cell| {
        let mut prev = prev_cell.borrow_mut();
        let mut changes = Vec::new();
        let mut seen: FxHashMap<ElementId, bool> = FxHashMap::default();

        for snap in current {
            seen.insert(snap.id, true);
            let h = hash_element(snap);
            match prev.get(&snap.id) {
                None => {
                    changes.push(ElementChange::Added(snap.id));
                }
                Some(old_hash) if *old_hash != h => {
                    let mut diffs = Vec::new();
                    if old_hash.bounds != h.bounds {
                        diffs.push("screen_bounds");
                    }
                    if old_hash.z_index != h.z_index {
                        diffs.push("z_index");
                    }
                    if old_hash.dirty_raw != h.dirty_raw {
                        diffs.push("dirty_flags");
                    }
                    if old_hash.state_raw != h.state_raw {
                        diffs.push("state_flags");
                    }
                    if old_hash.surface_gen != h.surface_gen {
                        diffs.push("surface_gen");
                    }
                    if old_hash.decor_gen != h.decor_gen {
                        diffs.push("decor_gen");
                    }
                    if old_hash.subtree_gen != h.subtree_gen {
                        diffs.push("subtree_gen");
                    }
                    if old_hash.layout_gen != h.layout_gen {
                        diffs.push("layout_gen");
                    }
                    if old_hash.opacity != h.opacity {
                        diffs.push("opacity");
                    }
                    if old_hash.bg_hash != h.bg_hash {
                        diffs.push("background");
                    }
                    if old_hash.fg_hash != h.fg_hash {
                        diffs.push("foreground");
                    }
                    if old_hash.slot_inactive != h.slot_inactive {
                        diffs.push("slot_inactive");
                    }
                    if old_hash.scroll_offset_hash != h.scroll_offset_hash {
                        diffs.push("scroll_offset");
                    }
                    if old_hash.text_label_hash != h.text_label_hash {
                        diffs.push("text_label");
                    }
                    if diffs.is_empty() {
                        diffs.push("decor");
                    }
                    changes.push(ElementChange::Modified {
                        id: snap.id,
                        kind: snap.kind.to_string(),
                        what_changed: diffs.into_iter().map(|s| s.to_string()).collect(),
                    });
                }
                _ => {}
            }
        }

        for (id, _) in prev.iter() {
            if !seen.contains_key(id) {
                let kind = "?".to_string(); // element already torn down, kind lost
                changes.push(ElementChange::Removed {
                    id: *id,
                    prev_kind: kind,
                });
            }
        }

        *prev = current.iter().map(|s| (s.id, hash_element(s))).collect();
        changes
    })
}

// ═══════════════════════ Signal Change Stream ═══════════════════════

thread_local! {
    static SIGNAL_CHANGE_BUF: RefCell<Vec<SignalChange>> = RefCell::new(Vec::new());
}

/// Install the signal change observer. Called once from App::new().
/// Captures `(signal_addr, version)` on every `Signal::set()` call.
pub fn install_signal_observer() {
    use auralis_signal::add_schedule_observer_with_identity;
    use std::sync::OnceLock;
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        // Store token in a leaked Box to prevent drop (observer must live forever).
        let _token = Box::new(add_schedule_observer_with_identity(Box::new(
            move |addr, ver| {
                SIGNAL_CHANGE_BUF.with(|buf| {
                    buf.borrow_mut().push(SignalChange {
                        signal_addr: addr,
                        label: String::new(),
                        version: ver,
                        update_count: 0,
                        timestamp_us: auralis_signal::now_us(),
                    });
                });
            },
        )));
        Box::leak(_token);
    });
}

fn drain_signal_changes() -> Vec<SignalChange> {
    SIGNAL_CHANGE_BUF.with(|buf| buf.borrow_mut().drain(..).collect())
}

// ═══════════════════════ DevtoolsInspector — Public API ═══════════════════════

/// Public API for querying the DevTools ring buffer.
///
/// ```ignore
/// let inspector = DevtoolsInspector::attach().unwrap();
/// let snap = inspector.latest(window_id);
/// println!("frame {}: {}µs, {} dirty", snap.frame_id, snap.frame_timing.frame_total_us, snap.dirty_count);
/// ```
pub struct DevtoolsInspector {
    buf: DevtoolsRingBuffer,
}

impl DevtoolsInspector {
    /// Attach to the globally installed ring buffer. Returns `None` if
    /// devtools data collection is not active (not compiled with `devtools`
    /// feature, or `App::new()` hasn't run yet).
    pub fn attach() -> Option<Self> {
        GLOBAL_RING_BUF.with(|rb| rb.borrow().clone().map(|buf| Self { buf }))
    }

    /// Get the latest frame snapshot for a window.
    pub fn latest(&self, window_id: winit::window::WindowId) -> Option<FrameSnapshot> {
        latest_snapshot(&self.buf, window_id)
    }

    /// Find a snapshot by exact frame_id. O(n) scan — for frequent use,
    /// prefer `latest()` or iterate the deque directly.
    pub fn frame(
        &self,
        window_id: winit::window::WindowId,
        frame_id: u64,
    ) -> Option<FrameSnapshot> {
        self.buf
            .borrow()
            .get(&window_id)
            .and_then(|d| d.iter().find(|s| s.frame_id == frame_id).cloned())
    }

    /// Number of buffered frames across all windows.
    pub fn frame_count(&self) -> usize {
        self.buf.borrow().values().map(|d| d.len()).sum()
    }

    /// Export a range of snapshots for a window, oldest first.
    pub fn export_range(
        &self,
        window_id: winit::window::WindowId,
        start: usize,
        end: usize,
    ) -> Vec<FrameSnapshot> {
        self.buf
            .borrow()
            .get(&window_id)
            .map(|d| {
                d.iter()
                    .skip(start)
                    .take(end.saturating_sub(start))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Raw access to the ring buffer for advanced queries.
    pub fn with_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&FxHashMap<winit::window::WindowId, VecDeque<FrameSnapshot>>) -> R,
    {
        f(&self.buf.borrow())
    }
}

// ═══════════════════════ Global Access (thread-local) ═══════════════════════

thread_local! {
    static GLOBAL_RING_BUF: RefCell<Option<DevtoolsRingBuffer>> = const { RefCell::new(None) };
    static DISPLAY_SIGNAL: RefCell<Option<auralis_signal::Signal<String>>> = const { RefCell::new(None) };
}

pub fn install_ring_buffer(buf: DevtoolsRingBuffer) {
    GLOBAL_RING_BUF.with(|rb| *rb.borrow_mut() = Some(buf));
}

pub fn with_ring_buffer<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&DevtoolsRingBuffer) -> R,
{
    GLOBAL_RING_BUF.with(|rb| rb.borrow().as_ref().map(|b| f(b)))
}

pub fn install_display_signal(sig: auralis_signal::Signal<String>) {
    DISPLAY_SIGNAL.with(|ds| *ds.borrow_mut() = Some(sig));
}

pub fn notify_display(text: String) {
    DISPLAY_SIGNAL.with(|ds| {
        if let Some(ref sig) = *ds.borrow() {
            sig.set(text);
        }
    });
}

// ═══════════════════════ Interaction Recording ═══════════════════════

/// Recordable user interaction for DevTools time-travel replay.
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug)]
pub enum InteractionKind {
    PointerDown {
        x: f32,
        y: f32,
        button: u32,
        modifiers: u32,
    },
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: u32,
        modifiers: u32,
    },
    Scroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    },
    KeyPress {
        key_name: String,
        modifiers: u32,
    },
    KeyRelease {
        key_name: String,
        modifiers: u32,
    },
    ImeCommit {
        text: String,
    },
    ImePreedit {
        text: String,
        cursor_begin: Option<usize>,
        cursor_end: Option<usize>,
    },
    Resize {
        width: f32,
        height: f32,
    },
}

#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug)]
pub struct RecordedInteraction {
    pub seq: u64,
    pub frame_id: u64,
    pub timestamp_us: u64,
    pub kind: InteractionKind,
}

pub const INTERACTION_BUFFER_CAPACITY: usize = 10_000;

thread_local! {
    static INTERACTION_BUF: std::cell::RefCell<Vec<RecordedInteraction>> =
        std::cell::RefCell::new(Vec::with_capacity(INTERACTION_BUFFER_CAPACITY));
    static INTERACTION_SEQ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub fn record_interaction(frame_id: u64, kind: InteractionKind, timestamp_us: u64) {
    let seq = INTERACTION_SEQ.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    INTERACTION_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        if buf.len() >= INTERACTION_BUFFER_CAPACITY {
            buf.remove(0);
        }
        buf.push(RecordedInteraction {
            seq,
            frame_id,
            timestamp_us,
            kind,
        });
    });
}

pub fn drain_interactions() -> Vec<RecordedInteraction> {
    INTERACTION_BUF.with(|buf| std::mem::take(&mut *buf.borrow_mut()))
}

pub fn peek_interaction_count() -> usize {
    INTERACTION_BUF.with(|buf| buf.borrow().len())
}

/// Result of diffing two arbitrary FrameSnapshots.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub frame_id_prev: u64,
    pub frame_id_current: u64,
    pub element_changes: Vec<ElementChange>,
    pub signal_changes: Vec<SignalDiff>,
}

/// A signal-level diff between two snapshots.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SignalDiff {
    Added {
        addr: usize,
        label: String,
    },
    Changed {
        addr: usize,
        label: String,
        version_prev: u64,
        version_curr: u64,
    },
    Removed {
        addr: usize,
        prev_label: String,
    },
}

/// Compare two FrameSnapshots and produce a human-readable diff.
/// Stateless — no thread-local cache needed.
pub fn diff_snapshots(prev: &FrameSnapshot, curr: &FrameSnapshot) -> SnapshotDiff {
    let prev_by_id: FxHashMap<ElementId, &ElementFullSnapshot> =
        prev.elements.iter().map(|e| (e.id, e)).collect();
    let mut seen_curr: FxHashSet<ElementId> = FxHashSet::default();
    let mut element_changes = Vec::new();

    for snap in &curr.elements {
        seen_curr.insert(snap.id);
        match prev_by_id.get(&snap.id) {
            None => {
                element_changes.push(ElementChange::Added(snap.id));
            }
            Some(old_snap) => {
                let old_hash = hash_element(old_snap);
                let new_hash = hash_element(snap);
                if old_hash != new_hash {
                    let mut diffs = Vec::new();
                    if old_hash.bounds != new_hash.bounds {
                        diffs.push("screen_bounds");
                    }
                    if old_hash.z_index != new_hash.z_index {
                        diffs.push("z_index");
                    }
                    if old_hash.dirty_raw != new_hash.dirty_raw {
                        diffs.push("dirty_flags");
                    }
                    if old_hash.state_raw != new_hash.state_raw {
                        diffs.push("state_flags");
                    }
                    if old_hash.surface_gen != new_hash.surface_gen {
                        diffs.push("surface_gen");
                    }
                    if old_hash.decor_gen != new_hash.decor_gen {
                        diffs.push("decor_gen");
                    }
                    if old_hash.subtree_gen != new_hash.subtree_gen {
                        diffs.push("subtree_gen");
                    }
                    if old_hash.layout_gen != new_hash.layout_gen {
                        diffs.push("layout_gen");
                    }
                    if old_hash.opacity != new_hash.opacity {
                        diffs.push("opacity");
                    }
                    if old_hash.bg_hash != new_hash.bg_hash {
                        diffs.push("background");
                    }
                    if old_hash.fg_hash != new_hash.fg_hash {
                        diffs.push("foreground");
                    }
                    if old_hash.slot_inactive != new_hash.slot_inactive {
                        diffs.push("slot_inactive");
                    }
                    if old_hash.scroll_offset_hash != new_hash.scroll_offset_hash {
                        diffs.push("scroll_offset");
                    }
                    if old_hash.text_label_hash != new_hash.text_label_hash {
                        diffs.push("text_label");
                    }
                    if diffs.is_empty() {
                        diffs.push("decor");
                    }
                    element_changes.push(ElementChange::Modified {
                        id: snap.id,
                        kind: snap.kind.to_string(),
                        what_changed: diffs.into_iter().map(|s| s.to_string()).collect(),
                    });
                }
            }
        }
    }

    for (id, old_snap) in &prev_by_id {
        if !seen_curr.contains(id) {
            element_changes.push(ElementChange::Removed {
                id: *id,
                prev_kind: old_snap.kind.to_string(),
            });
        }
    }

    let prev_sigs: FxHashMap<usize, &SignalChange> = prev
        .signal_changes
        .iter()
        .map(|s| (s.signal_addr, s))
        .collect();
    let mut seen_sigs: FxHashSet<usize> = FxHashSet::default();
    let mut signal_changes = Vec::new();

    for sig in &curr.signal_changes {
        seen_sigs.insert(sig.signal_addr);
        match prev_sigs.get(&sig.signal_addr) {
            None => {
                signal_changes.push(SignalDiff::Added {
                    addr: sig.signal_addr,
                    label: sig.label.clone(),
                });
            }
            Some(old_sig) if old_sig.version != sig.version => {
                signal_changes.push(SignalDiff::Changed {
                    addr: sig.signal_addr,
                    label: sig.label.clone(),
                    version_prev: old_sig.version,
                    version_curr: sig.version,
                });
            }
            _ => {}
        }
    }
    for (addr, old_sig) in &prev_sigs {
        if !seen_sigs.contains(addr) {
            signal_changes.push(SignalDiff::Removed {
                addr: *addr,
                prev_label: old_sig.label.clone(),
            });
        }
    }

    SnapshotDiff {
        frame_id_prev: prev.frame_id,
        frame_id_current: curr.frame_id,
        element_changes,
        signal_changes,
    }
}

/// Encode [`crate::event::Modifiers`] as a u32 bitmask.
/// Bit 0 = shift, bit 1 = ctrl, bit 2 = alt, bit 3 = meta.
pub fn modifiers_to_u32(mods: crate::event::Modifiers) -> u32 {
    let mut bits = 0u32;
    if mods.shift {
        bits |= 1;
    }
    if mods.ctrl {
        bits |= 2;
    }
    if mods.alt {
        bits |= 4;
    }
    if mods.meta {
        bits |= 8;
    }
    bits
}
