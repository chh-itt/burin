//! StickyHeader — stackable sticky headers driven by change-guarded frame
//! processing.
//!
//! Features:
//! - Single stick-to-top / stick-to-bottom (`StickyDirection`)
//! - Multi-stack (section headers accumulate below each other by REAL height)
//! - Push-up mode (`StickyMode::PushUp` / `.push_up()`): iOS-style — the
//!   next section header pushes the previous one up and out
//! - Teardown-safe: entries are reclaimed via the core teardown-hook protocol
//! - Change-guarded: when neither the parent scroll offset nor the layout
//!   changed, `process_all` performs zero writes and registers zero dirty —
//!   an idle app with sticky headers stays idle (one f32 compare per entry
//!   per driven frame; no frame is ever driven *by* this module).
//!
//! Coordinate model: `original_y`/`height` are captured from the element's
//! post-layout `screen_bounds` (layout space — paint subtracts the scroll
//! offset). `position_offset` is a paint-time translation, so it never feeds
//! back into the captured bounds. Captures refresh when the element's
//! `layout_generation` changes (one-frame lag after a relayout, same model as
//! portal anchors).

use std::cell::{Cell, RefCell};

use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::{DirtyFlags, ElementArena, ElementId};
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Vec2;

// ── Direction ──

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StickyDirection {
    Top,
    Bottom,
}

/// How consecutive stuck headers interact (Top direction only).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum StickyMode {
    /// Headers accumulate below each other by their real height.
    #[default]
    Stack,
    /// iOS-style: only one header is pinned; the next approaching header
    /// pushes the previous one up and out of the viewport.
    PushUp,
}

// ── Registry (thread_local) ──

struct StickyEntry {
    eid: ElementId,
    top_offset: f32,
    direction: StickyDirection,
    mode: StickyMode,
    /// Resolved nearest scrollable ancestor (lazy; re-resolved if it dies).
    scroll_parent: Cell<Option<ElementId>>,
    /// Layout-space y captured post-layout. None until first valid capture.
    original_y: Cell<Option<f32>>,
    /// Real element height from screen_bounds (stacking accumulator input).
    height: Cell<f32>,
    /// Element layout_generation at capture time — recapture on change.
    layout_gen_seen: Cell<u64>,
    /// Scroll offset at the last processed frame (change guard).
    last_scroll_y: Cell<f32>,
    /// Last position_offset.y written (write/dirty suppression).
    last_applied_y: Cell<f32>,
}

/// Per-window sticky-header registry (audit 2026-07-18 multi-window pass):
/// window A's frame previously scanned (and could mis-resolve) window B's
/// entries. The `AppContext::extension` anymap keeps the widget-layer
/// `StickyEntry` type out of `core` — no reverse dependency. Lifecycle is
/// handled by the teardown hook below (audit 2026-07-17 round 3, A+B).
#[derive(Default)]
struct StickyDomain {
    entries: RefCell<Vec<StickyEntry>>,
}

fn sticky_domain() -> std::rc::Rc<StickyDomain> {
    crate::core::app_context::current_app().extension::<StickyDomain>()
}

pub fn register(eid: ElementId, top_offset: f32, direction: StickyDirection) {
    register_with_mode(eid, top_offset, direction, StickyMode::default());
}

pub fn register_with_mode(
    eid: ElementId,
    top_offset: f32,
    direction: StickyDirection,
    mode: StickyMode,
) {
    dirty_registry::register_teardown_hook(teardown_cleanup);
    dirty_registry::spatial_register_position_offset(eid);
    sticky_domain().entries.borrow_mut().push(StickyEntry {
        eid,
        top_offset,
        direction,
        mode,
        scroll_parent: Cell::new(None),
        original_y: Cell::new(None),
        height: Cell::new(0.0),
        layout_gen_seen: Cell::new(0),
        last_scroll_y: Cell::new(f32::NAN),
        last_applied_y: Cell::new(0.0),
    });
}

pub fn unregister(eid: ElementId) {
    sticky_domain()
        .entries
        .borrow_mut()
        .retain(|e| e.eid != eid);
}

fn teardown_cleanup(id: ElementId) {
    unregister(id);
}

/// Test-only introspection: registered sticky entry count.
#[doc(hidden)]
pub fn debug_entry_len() -> usize {
    sticky_domain().entries.borrow().len()
}

/// Test-only introspection: (eid, applied position_offset.y) per entry, in
/// registration order.
#[doc(hidden)]
pub fn debug_applied_offsets() -> Vec<(ElementId, f32)> {
    sticky_domain()
        .entries
        .borrow()
        .iter()
        .map(|e| (e.eid, e.last_applied_y.get()))
        .collect()
}

/// Test-only introspection: (eid, captured original_y, height) per entry.
#[doc(hidden)]
pub fn debug_captures() -> Vec<(ElementId, Option<f32>, f32)> {
    sticky_domain()
        .entries
        .borrow()
        .iter()
        .map(|e| (e.eid, e.original_y.get(), e.height.get()))
        .collect()
}

// ── Frame processing ──

/// Per-frame sticky pass (called from `run_pre_passes`). Change-guarded:
/// entries whose parent scroll offset and layout are unchanged are skipped
/// with a single f32/u64 compare — no writes, no dirty, no allocation.
pub fn process_all(arena: &ElementArena) {
    let dom = sticky_domain();
    let entries = dom.entries.borrow();
    if entries.is_empty() {
        return;
    }

    // Pass 1 — refresh captures + detect change. Indices of entries that
    // need a position recompute this frame.
    let mut active = false;
    for e in entries.iter() {
        let Some(el) = arena.get(e.eid) else { continue };

        // (Re)capture layout data when layout moved under us.
        let lgen = el.layout_generation.get();
        if e.original_y.get().is_none() || e.layout_gen_seen.get() != lgen {
            let b = el.screen_bounds;
            if b.height > 0.0 {
                e.original_y.set(Some(b.y));
                e.height.set(b.height);
                e.layout_gen_seen.set(lgen);
                // Force recompute after recapture.
                e.last_scroll_y.set(f32::NAN);
            }
        }

        // Resolve (and cache) the nearest scrollable ancestor.
        let parent = match e.scroll_parent.get() {
            Some(p) if arena.get(p).is_some() => Some(p),
            _ => {
                let p = find_scroll_parent(e.eid);
                e.scroll_parent.set(p);
                p
            }
        };
        let scroll_y = parent
            .and_then(|p| {
                crate::core::element::with_ct(|ct| {
                    ct.scroll.get(&p).map(|sc| sc.scroll_offset.get().y)
                })
            })
            .unwrap_or(0.0);

        let last = e.last_scroll_y.get();
        if e.original_y.get().is_some() && (last.is_nan() || (scroll_y - last).abs() > 0.01) {
            active = true;
        }
    }
    if !active {
        return;
    }

    // Pass 2 — group by scroll parent, sort by original_y, stack by real
    // height, and write only changed offsets. Entry counts are tiny
    // (headers per scroll area), so the sort is O(k log k) on scroll
    // frames only.
    let mut order: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].original_y.get().is_some())
        .collect();
    order.sort_by(|&a, &b| {
        let ea = &entries[a];
        let eb = &entries[b];
        ea.scroll_parent.get().cmp(&eb.scroll_parent.get()).then(
            ea.original_y
                .get()
                .unwrap_or(0.0)
                .partial_cmp(&eb.original_y.get().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let mut group_parent: Option<ElementId> = None;
    let mut accumulated = 0.0f32;
    for (k, &i) in order.iter().enumerate() {
        let e = &entries[i];
        if arena.get(e.eid).is_none() {
            continue;
        }
        let parent = e.scroll_parent.get();
        if parent != group_parent {
            group_parent = parent;
            accumulated = 0.0;
        }
        let (scroll_y, container_y, viewport_h) = match parent {
            Some(p) => {
                let sy = crate::core::element::with_ct(|ct| {
                    ct.scroll.get(&p).map(|sc| sc.scroll_offset.get().y)
                })
                .unwrap_or(0.0);
                let (cy, vh) = arena
                    .get(p)
                    .map(|pe| (pe.screen_bounds.y, pe.screen_bounds.height))
                    .unwrap_or((0.0, 0.0));
                (sy, cy, vh)
            }
            None => (0.0, 0.0, 0.0),
        };
        e.last_scroll_y.set(scroll_y);

        let original_y = e.original_y.get().unwrap_or(0.0);
        let height = e.height.get();
        // Content-space y relative to the scroll container's top.
        let content_y = original_y - container_y;
        // Where the element would render without our offset.
        let viewport_y = content_y - scroll_y;

        let new_y = match e.direction {
            StickyDirection::Top => match e.mode {
                StickyMode::Stack => {
                    let target = e.top_offset + accumulated;
                    if viewport_y < target {
                        accumulated += height;
                        target - viewport_y
                    } else {
                        0.0
                    }
                }
                StickyMode::PushUp => {
                    // iOS-style: pin at top_offset, but let the NEXT
                    // header of the same group push this one up as its
                    // natural position approaches the pinned bottom.
                    if viewport_y < e.top_offset {
                        let next_vy = order.get(k + 1).and_then(|&j| {
                            let n = &entries[j];
                            (n.scroll_parent.get() == parent).then(|| {
                                let n_cy = n.original_y.get().unwrap_or(0.0) - container_y;
                                n_cy - scroll_y
                            })
                        });
                        let pinned = match next_vy {
                            Some(nvy) => e.top_offset.min(nvy - height),
                            None => e.top_offset,
                        };
                        pinned - viewport_y
                    } else {
                        0.0
                    }
                }
            },
            StickyDirection::Bottom => {
                // Stick to the viewport bottom edge (offset from bottom).
                let target = viewport_h - e.top_offset - height;
                if viewport_y > target && viewport_h > 0.0 {
                    target - viewport_y
                } else {
                    0.0
                }
            }
        };

        if (new_y - e.last_applied_y.get()).abs() > 0.01 {
            e.last_applied_y.set(new_y);
            crate::core::element::with_ct_mut(|ct| {
                ct.xform
                    .entry(e.eid)
                    .or_default()
                    .position_offset
                    .set(Vec2::new(0.0, new_y));
            });
            dirty_registry::mark_dirty(e.eid, DirtyFlags::REPAINT);
            dirty_registry::register_dirty(e.eid, DirtyFlags::REPAINT);
        }
    }
}

fn find_scroll_parent(eid: ElementId) -> Option<ElementId> {
    let mut cur = eid;
    loop {
        {
            let pid = dirty_registry::parent_of(cur)?;
            let is_scroll = crate::core::element::with_ct(|ct| {
                ct.layout.get(&pid).map(|l| l.overflow)
                    == Some(crate::core::config::Overflow::Scroll)
            });
            if is_scroll {
                return Some(pid);
            }
            cur = pid;
        }
    }
}

// ── Widget ──

pub struct StickyHeader {
    child: Option<Box<dyn Widget>>,
    top_offset: f32,
    direction: StickyDirection,
    mode: StickyMode,
    style: StyleRefinement,
}

impl StickyHeader {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Some(Box::new(child)),
            top_offset: 0.0,
            direction: StickyDirection::Top,
            mode: StickyMode::default(),
            style: StyleRefinement::default(),
        }
    }
    pub fn top(mut self, px: f32) -> Self {
        self.top_offset = px;
        self
    }
    pub fn bottom(mut self, px: f32) -> Self {
        self.top_offset = px;
        self.direction = StickyDirection::Bottom;
        self
    }
    /// iOS-style section headers: the next header pushes the previous one
    /// up and out instead of stacking below it (Top direction only).
    pub fn push_up(mut self) -> Self {
        self.mode = StickyMode::PushUp;
        self
    }
}

impl Styled for StickyHeader {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for StickyHeader {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());

        // Mount child — the wrapper stays in normal flex flow (component
        // mask excludes TRANSFORM) so taffy writes real screen_bounds.y
        // for the capture pass. Position_offset is wired manually below.
        if let Some(child) = self.child {
            let mut child_ctx = ctx.child_with_events(id);
            let child_id = Box::new(child).mount_box(&mut child_ctx);
            ctx.arena.add_child(id, child_id);
        }

        // Create the position_offset cell (normally done by preallocate
        // via the TRANSFORM component mask, which we omit intentionally —
        // with TRANSFORM the element is treated as a leaf by taffy, which
        // triggers absolute positioning (leaf_taffy_style z_index gate)
        // and pulls it out of flex flow, breaking screen_bounds.y capture).
        let pos_cell = std::rc::Rc::new(std::cell::Cell::new(Vec2::ZERO));
        crate::core::element::with_ct_mut(|ct| {
            ct.xform.entry(id).or_default().position_offset = pos_cell;
        });
        // Elevate the paint-floor so the stuck header renders above rows
        // that were allocated after it (same mechanism as drag elevation).
        // We do NOT touch z_index — taffy's leaf_taffy_style uses z_index > 0
        // as an absolute-positioning gate; z_index_floor is paint-only.
        if let Some(el) = ctx.arena.get_mut(id) {
            el.z_index_floor = Some(1);
        }
        register_with_mode(id, self.top_offset, self.direction, self.mode);

        id
    }
}
