//! Bridge between burin layout config and the taffy flexbox engine.
//!
//! Supports both full-tree build (first frame) and incremental updates:
//! - `ensure_subtree()` for structural changes (rebuilds a subtree in-place)
//! - `update_style()` for MEASURE-only changes (in-place style rewrite)

use std::collections::HashMap;

use taffy::prelude::*;

use crate::core::element::ElementArena;
use crate::core::ElementId;
use crate::ecs::components::LayoutComponent;
use crate::style::{Dimension, Padding, Rect, Size};

/// Wrap a `taffy::TaffyTree` with helpers for widget layout.
///
/// Maintains bidirectional mapping between ElementId and taffy NodeId.
/// The tree is kept **persistent** across frames — only dirty subtrees
/// are rebuilt, and style properties are updated in-place.
pub struct TaffyBridge {
    pub tree: taffy::TaffyTree<()>,
    /// Maps taffy NodeIds to our ElementIds for reverse lookup.
    id_map: HashMap<taffy::NodeId, ElementId>,
    /// Maps ElementIds to taffy NodeIds for forward lookup.
    node_map: HashMap<ElementId, taffy::NodeId>,
}

#[allow(dead_code)]
impl TaffyBridge {
    pub fn new() -> Self {
        let mut tree = taffy::TaffyTree::new();
        tree.disable_rounding();
        Self {
            tree,
            id_map: HashMap::new(),
            node_map: HashMap::new(),
        }
    }

    /// Clear all nodes (for emergency full rebuild).
    pub fn clear(&mut self) {
        let mut tree = taffy::TaffyTree::new();
        tree.disable_rounding();
        self.tree = tree;
        self.id_map.clear();
        self.node_map.clear();
    }

    /// Check if an element has a taffy node.
    pub fn has_node(&self, id: ElementId) -> bool {
        self.node_map.contains_key(&id)
    }

    /// Get the taffy NodeId for an ElementId.
    pub fn node_for(&self, id: ElementId) -> Option<taffy::NodeId> {
        self.node_map.get(&id).copied()
    }

    /// Get the ElementId for a taffy NodeId.
    pub fn element_for(&self, node: taffy::NodeId) -> Option<ElementId> {
        self.id_map.get(&node).copied()
    }

    // ── Internal helpers ──

    fn register_node(&mut self, element_id: ElementId, node: taffy::NodeId) {
        self.id_map.insert(node, element_id);
        self.node_map.insert(element_id, node);
    }

    fn unregister_node(&mut self, node: taffy::NodeId) {
        if let Some(eid) = self.id_map.remove(&node) {
            self.node_map.remove(&eid);
        }
    }

    // ── Full-tree build (first frame) ──

    /// Build the entire taffy tree from the arena, starting from the given root.
    /// Returns the root taffy NodeId. Used for the first frame.
    pub fn build_full_tree(&mut self, arena: &ElementArena, root_id: ElementId) -> taffy::NodeId {
        build_element_taffy_tree(arena, root_id, self)
    }

    // ── Incremental: in-place style update (MEASURE-only changes) ──

    /// Update the taffy style for a single element in-place.
    /// Does NOT touch children — only rewrites the element's own style properties.
    /// Used for MEASURE changes that don't affect tree structure.
    pub fn update_style(&mut self, arena: &ElementArena, eid: ElementId) {
        let Some(node) = self.node_for(eid) else {
            return;
        };
        let Some(element) = arena.get(eid) else {
            return;
        };

        let has_active_children = element.children.iter().any(|&cid| {
            arena
                .get(cid)
                .map(|c| !c.slot_inactive.get())
                .unwrap_or(false)
        });

        let style = element_taffy_style(arena, eid, !has_active_children);
        let _ = self.tree.set_style(node, style);
    }

    // ── Incremental: ensure a subtree exists and matches current structure ──

    /// Ensure the subtree rooted at `eid` has an up-to-date taffy representation.
    ///
    /// If the element has no taffy node: builds one from scratch.
    /// If the element already has a node: keeps the same NodeId,
    /// removes old children, rebuilds active children, and updates style.
    ///
    /// Returns `(node_id, old_node_if_replaced)`:
    /// - `node_id` is the correct NodeId for this element.
    /// - If `old_node` is `Some`, the caller must relink the parent.
    /// - If `old_node` is `None`, no relinking is needed (NodeId unchanged,
    ///   or it's a new node that hasn't been linked yet).
    pub fn ensure_subtree(
        &mut self,
        arena: &ElementArena,
        eid: ElementId,
    ) -> (taffy::NodeId, Option<taffy::NodeId>) {
        let Some(element) = arena.get(eid) else {
            return (build_element_taffy_tree(arena, eid, self), None);
        };

        let children = element.children.clone();
        let active_child_ids: Vec<ElementId> = children
            .iter()
            .filter(|&&cid| {
                arena
                    .get(cid)
                    .map(|c| !c.slot_inactive.get())
                    .unwrap_or(false)
            })
            .copied()
            .collect();

        if let Some(existing_node) = self.node_for(eid) {
            // ── Node exists: keep it, only replace children ──
            // Remove old child subtrees (descendants only, not this node)
            if let Ok(existing_children) = self.tree.children(existing_node) {
                for child_node in existing_children {
                    self.remove_subtree_nodes(child_node);
                }
            }

            // Build new child nodes for currently-active children
            let grid_cols = arena.comp_layout(eid).map_or(0, |l| l.grid_columns);
            let child_nodes: Vec<taffy::NodeId> = active_child_ids
                .iter()
                .map(|&cid| {
                    let child_node = build_element_taffy_tree(arena, cid, self);
                    // Grid children: set grid-column span/offset on the taffy node
                    if grid_cols > 0 {
                        let span = arena.comp_layout(cid).map_or(0, |l| l.grid_column_span);
                        let offset = arena.comp_layout(cid).map_or(0, |l| l.grid_column_offset);
                        if span > 0 || offset > 0 {
                            let mut child_style = self
                                .tree
                                .style(child_node)
                                .expect("just-built taffy child node must have a style")
                                .clone();
                            use taffy::style::GridPlacement;
                            child_style.grid_column = if span > 0 {
                                taffy::prelude::Line {
                                    start: GridPlacement::Line((offset as i16 + 1).into()),
                                    end: GridPlacement::Line(
                                        (offset as i16 + span as i16 + 1).into(),
                                    ),
                                }
                            } else {
                                taffy::prelude::Line {
                                    start: GridPlacement::Line((offset as i16 + 1).into()),
                                    end: GridPlacement::Auto,
                                }
                            };
                            self.tree
                                .set_style(child_node, child_style)
                                .expect("just-built taffy child node accepts set_style");
                        }
                    }
                    child_node
                })
                .collect();

            // Update style
            let is_leaf = active_child_ids.is_empty();
            let mut style = element_taffy_style(arena, eid, is_leaf);
            let dyn_w = element
                .get_user_data::<std::rc::Rc<std::cell::Cell<f32>>>()
                .map(|c| c.get());
            if let Some(w) = dyn_w {
                style.size.width = taffy::Dimension::length(w);
            }
            let _ = self.tree.set_style(existing_node, style);

            // Replace children list
            let _ = self.tree.set_children(existing_node, &child_nodes);

            // NodeId unchanged — no relinking needed
            (existing_node, None)
        } else {
            // ── No existing node: build from scratch ──
            let new_node = build_element_taffy_tree(arena, eid, self);
            (new_node, None)
        }
    }

    /// Recursively remove all taffy nodes in the subtree rooted at `node`.
    /// Cleans up both taffy tree and our id_map/node_map.
    fn remove_subtree_nodes(&mut self, node: taffy::NodeId) {
        // Collect all descendants (BFS)
        let mut to_remove: Vec<taffy::NodeId> = Vec::new();
        let mut queue: Vec<taffy::NodeId> = vec![node];
        while let Some(current) = queue.pop() {
            to_remove.push(current);
            // Collect children (if this is a parent node)
            if let Ok(children) = self.tree.children(current) {
                for child in children {
                    queue.push(child);
                }
            }
        }

        // Clean maps in reverse order (children first)
        for n in to_remove.iter().rev() {
            self.unregister_node(*n);
        }

        // Remove from taffy tree (must remove children before parent)
        for n in to_remove.iter().rev() {
            let _ = self.tree.remove(*n);
        }
    }

    /// Re-link a rebuilt subtree to its parent in the taffy tree.
    /// Uses the provided `old_child_node` (from before the rebuild) to
    /// remove the stale child entry, then adds `new_child_node`.
    pub fn relink_to_parent(
        &mut self,
        parent_id: ElementId,
        new_child_node: taffy::NodeId,
        old_child_node: Option<taffy::NodeId>,
    ) {
        let Some(parent_node) = self.node_for(parent_id) else {
            return;
        };

        if let Some(old) = old_child_node {
            let _ = self.tree.remove_child(parent_node, old);
        }
        let _ = self.tree.add_child(parent_node, new_child_node);
    }

    // ── Layout computation ──

    /// Compute layout for a sub-tree and return element bounds.
    pub fn compute_layout(
        &mut self,
        node: taffy::NodeId,
        available: Size,
    ) -> Vec<(ElementId, Rect)> {
        let size = taffy::Size {
            width: to_available_space(available.width),
            height: to_available_space(available.height),
        };
        if let Err(e) = self.tree.compute_layout(node, size) {
            // Only reachable with an invalid node id (arena/taffy desync).
            // Keep the previous frame's bounds instead of crashing.
            debug_assert!(false, "taffy compute_layout failed: {e:?}");
            return Vec::new();
        }

        let mut results = Vec::new();
        self.collect_bounds(node, 0.0, 0.0, &mut results);
        results
    }

    /// Isolated subtree layout for a relayout boundary.
    ///
    /// `frozen` is the boundary's cached outer size (both axes definite, taken
    /// from the previous layout pass). `origin` is the boundary's UNROUNDED
    /// absolute top-left, used to seed absolute-coordinate rounding in
    /// `collect_bounds`. Returns bounds for the boundary subtree only.
    ///
    /// The boundary's `style.size` is TEMPORARILY overridden to `frozen` so
    /// that, computed as a root, it reproduces the size the parent injected
    /// (stretch/flex-grow) in the full pass instead of shrink-wrapping to
    /// content. The original style is restored before returning, so a later
    /// full-tree compute is never affected by the frozen override.
    pub fn compute_subtree(
        &mut self,
        node: taffy::NodeId,
        indep: crate::ecs::components::AxisPair,
        cached: Size,
        origin: (f32, f32),
    ) -> (Vec<(ElementId, Rect)>, Size) {
        // Save the original style so we can restore it after the isolated pass;
        // otherwise the frozen size would leak into subsequent full-tree passes
        // (e.g. a later reposition frame that does not restyle this boundary).
        //
        // Freeze only the INDEPENDENT axes to their cached definite size;
        // dependent axes keep their real (auto) style and are recomputed from
        // content (available = MaxContent). The returned `Size` is the boundary's
        // new outer size — the caller verifies the dependent axes did not change
        // (a single-axis boundary that failed this must escalate to a full pass).
        let original_style = self.tree.style(node).ok().cloned();
        if let Some(cur) = &original_style {
            let mut s = cur.clone();
            if indep.x {
                s.size.width = taffy::Dimension::length(cached.width);
            }
            if indep.y {
                s.size.height = taffy::Dimension::length(cached.height);
            }
            let _ = self.tree.set_style(node, s);
        }
        let avail = taffy::Size {
            width: if indep.x {
                taffy::AvailableSpace::Definite(cached.width)
            } else {
                taffy::AvailableSpace::MaxContent
            },
            height: if indep.y {
                taffy::AvailableSpace::Definite(cached.height)
            } else {
                taffy::AvailableSpace::MaxContent
            },
        };
        if let Err(e) = self.tree.compute_layout(node, avail) {
            // Invalid node id (arena/taffy desync) — keep previous bounds and
            // report "unchanged size" so the caller does not escalate on
            // garbage data.
            debug_assert!(false, "taffy compute_subtree failed: {e:?}");
            if let Some(s) = original_style {
                let _ = self.tree.set_style(node, s);
            }
            return (Vec::new(), cached);
        }
        let new_size = self
            .tree
            .layout(node)
            .map(|l| Size::new(l.size.width, l.size.height))
            .unwrap_or(cached);
        let mut results = Vec::new();
        self.collect_bounds(node, origin.0, origin.1, &mut results);
        // Restore the original style (bounds already collected into `results`).
        if let Some(s) = original_style {
            let _ = self.tree.set_style(node, s);
        }
        (results, new_size)
    }

    /// Walk the computed subtree and emit rounded screen `Rect`s.
    ///
    /// Rounding is done using ABSOLUTE cumulative coordinates (`abs_x`/`abs_y`
    /// are the UNROUNDED absolute origin of `node`). This makes rounding
    /// translation-invariant: a subtree computed in isolation and offset by its
    /// boundary origin rounds identically to the same node in a full-tree pass.
    /// (Taffy's own rounding is disabled in `new`/`clear`.)
    fn collect_bounds(
        &self,
        node: taffy::NodeId,
        abs_x: f32,
        abs_y: f32,
        out: &mut Vec<(ElementId, Rect)>,
    ) {
        if let Ok(layout) = self.tree.layout(node) {
            let ax = abs_x + layout.location.x;
            let ay = abs_y + layout.location.y;
            let rx = ax.round();
            let ry = ay.round();
            let rw = (ax + layout.size.width).round() - rx;
            let rh = (ay + layout.size.height).round() - ry;

            if let Some(&id) = self.id_map.get(&node) {
                out.push((id, Rect::new(rx, ry, rw, rh)));
            }

            for child in self.tree.child_ids(node) {
                self.collect_bounds(child, ax, ay, out);
            }
        }
    }
}

impl Default for TaffyBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ── Conversion helpers ──

pub fn to_taffy_dim(d: Dimension) -> taffy::Dimension {
    match d {
        Dimension::Pixels(px) => taffy::Dimension::length(px),
        Dimension::Percent(pct) => taffy::Dimension::percent(pct),
        Dimension::Auto => taffy::Dimension::auto(),
    }
}

/// Resolve width for taffy, preferring the original Dimension (Percent support).
fn width_taffy_dim(layout: &Option<LayoutComponent>, pixel_w: f32) -> taffy::Dimension {
    if let Some(lc) = layout {
        if let Some(ref dim) = lc.width_dim {
            if matches!(dim, Dimension::Percent(_)) {
                return to_taffy_dim(*dim);
            }
        }
    }
    if pixel_w > 0.0 {
        taffy::Dimension::length(pixel_w)
    } else {
        taffy::Dimension::auto()
    }
}

/// Resolve height for taffy, preferring the original Dimension (Percent support).
fn height_taffy_dim(layout: &Option<LayoutComponent>, pixel_h: f32) -> taffy::Dimension {
    if let Some(lc) = layout {
        if matches!(&lc.height_dim, Dimension::Percent(_)) {
            return to_taffy_dim(lc.height_dim);
        }
    }
    if pixel_h > 0.0 {
        taffy::Dimension::length(pixel_h)
    } else {
        taffy::Dimension::auto()
    }
}

#[allow(dead_code)]
pub fn to_taffy_padding(p: Padding) -> taffy::Rect<taffy::LengthPercentageAuto> {
    taffy::Rect {
        left: taffy::LengthPercentageAuto::length(p.left),
        right: taffy::LengthPercentageAuto::length(p.right),
        top: taffy::LengthPercentageAuto::length(p.top),
        bottom: taffy::LengthPercentageAuto::length(p.bottom),
    }
}

fn to_available_space(v: f32) -> taffy::AvailableSpace {
    if v.is_finite() && v > 0.0 {
        taffy::AvailableSpace::Definite(v)
    } else {
        taffy::AvailableSpace::MaxContent
    }
}

// ── Base layout style presets ──

pub fn vstack_style() -> taffy::Style {
    taffy::Style {
        display: taffy::Display::Flex,
        flex_direction: taffy::FlexDirection::Column,
        align_items: Some(taffy::AlignItems::STRETCH),
        ..Default::default()
    }
}

pub fn hstack_style() -> taffy::Style {
    taffy::Style {
        display: taffy::Display::Flex,
        flex_direction: taffy::FlexDirection::Row,
        align_items: Some(taffy::AlignItems::STRETCH),
        ..Default::default()
    }
}

// ═══════════════════════ Size independence detection ═══════════════════════

pub fn size_independent_of_children(
    arena: &ElementArena,
    eid: ElementId,
) -> crate::ecs::components::AxisPair {
    let mut memo = std::collections::HashMap::new();
    size_independent_memo(arena, eid, &mut memo)
}

/// Per-axis independence, transitive through percent parents. Memoized via
/// `memo` (bounded O(depth) amortized). Reads arena styles + parent chain
/// because the taffy tree is built bottom-up (a parent's value is not yet
/// recorded when a child is processed).
pub(crate) fn size_independent_memo(
    arena: &ElementArena,
    eid: ElementId,
    memo: &mut std::collections::HashMap<ElementId, crate::ecs::components::AxisPair>,
) -> crate::ecs::components::AxisPair {
    use crate::ecs::components::AxisPair;
    use crate::style::Dimension;
    if let Some(&v) = memo.get(&eid) {
        return v;
    }
    // Cycle guard: conservative default while recursing.
    memo.insert(eid, AxisPair::BOTH_DEP);

    // The root has no parent and IS the definite viewport (its bounds are the
    // window/harness size) → independent on both axes. Check BEFORE the layout
    // component (the root typically has none). This anchors the transitive
    // percent/stretch chains (children resolve against it).
    if crate::core::dirty_registry::parent_of(eid).is_none() {
        let v = AxisPair::both(true);
        memo.insert(eid, v);
        return v;
    }

    let Some(l) = arena.comp_layout(eid) else {
        return AxisPair::BOTH_DEP;
    };

    if l.overflow == crate::core::config::Overflow::Scroll {
        let v = AxisPair::both(true);
        memo.insert(eid, v);
        return v;
    }

    let parent = crate::core::dirty_registry::parent_of(eid);
    let parent_x = match parent {
        None => true,
        Some(p) => size_independent_memo(arena, p, memo).x,
    };
    let parent_y = match parent {
        None => true,
        Some(p) => size_independent_memo(arena, p, memo).y,
    };

    let mut def_w = matches!(l.width_dim, Some(Dimension::Pixels(w)) if w > 0.0)
        || (l.preferred_width.is_some_and(|w| w > 0.0) && !l.affected_by_child_size)
        || (matches!(l.width_dim, Some(Dimension::Percent(_))) && parent_x);

    let mut def_h = matches!(l.height_dim, Dimension::Pixels(h) if h > 0.0)
        || (l.preferred_height > 0.0 && !l.affected_by_child_size)
        || (matches!(l.height_dim, Dimension::Percent(_)) && parent_y);

    // Cross-axis align-stretch: a child with an AUTO cross-axis dimension inside
    // a stretch (NoWrap) parent inherits independence on the CROSS axis from the
    // parent's independence on that axis (column cross = width; row cross =
    // height). Default alignment=Start maps to align-items:stretch, so this
    // covers the common stretch layouts.
    if let Some(p) = parent {
        // The parent's layout component may be absent (notably the root), which
        // in `element_taffy_style` defaults to a Vertical, align-Start (=stretch),
        // NoWrap flex container — so default it here too, otherwise the root's
        // children never inherit stretch independence and the chain breaks.
        let pl = arena.comp_layout(p).unwrap_or_default();
        {
            use crate::core::element::LayoutDirection;
            use crate::style::Alignment;
            let cross_stretch = matches!(pl.alignment, Alignment::Start | Alignment::Stretch)
                && pl.flex_wrap == crate::core::config::FlexWrap::NoWrap;
            if cross_stretch {
                match pl.layout_direction {
                    LayoutDirection::Vertical => {
                        // column: cross axis = width
                        if matches!(l.width_dim, None | Some(Dimension::Auto)) && parent_x {
                            def_w = true;
                        }
                    }
                    LayoutDirection::Horizontal => {
                        // row: cross axis = height
                        if matches!(l.height_dim, Dimension::Auto) && parent_y {
                            def_h = true;
                        }
                    }
                }
            }
        }
    }

    let v = if l.aspect_ratio.is_some() {
        AxisPair::both(def_w && def_h)
    } else {
        AxisPair { x: def_w, y: def_h }
    };
    memo.insert(eid, v);
    v
}

// ═══════════════════════ Style extraction ═══════════════════════

/// Build a taffy Style for a single element, independent of its children.
/// Virtualized pool slot: absolutely position the element at its virtual
/// content-space Y (relative to the pool container). left:0 + right:0
/// stretches the row to the container width; height stays the row's own
/// preferred height. Applied by BOTH style paths (`element_taffy_style` for
/// restyles and `build_element_taffy_tree` for tree builds) — diverging here
/// leaves rows in the flex flow with coincidentally-correct initial positions
/// that break on the first partial remap. See `VirtualSlotY` docs.
fn apply_virtual_slot_y(arena: &ElementArena, eid: ElementId, style: &mut taffy::Style) {
    if let Some(vsy) = arena
        .get(eid)
        .and_then(|el| el.get_user_data::<crate::widgets::display::list::VirtualSlotY>())
    {
        style.position = taffy::Position::Absolute;
        style.inset = taffy::Rect {
            left: taffy::LengthPercentageAuto::length(0.0),
            right: taffy::LengthPercentageAuto::length(0.0),
            top: taffy::LengthPercentageAuto::length(vsy.0.get()),
            bottom: taffy::LengthPercentageAuto::auto(),
        };
    }
}

/// `is_leaf` is true when the element has no active children (all are slot_inactive).
pub fn element_taffy_style(arena: &ElementArena, eid: ElementId, is_leaf: bool) -> taffy::Style {
    let mut style = if is_leaf {
        leaf_taffy_style(arena, eid)
    } else {
        let grid_cols = arena.comp_layout(eid).map_or(0, |l| l.grid_columns);
        if grid_cols > 0 {
            grid_container_taffy_style(arena, eid, grid_cols)
        } else {
            flex_container_taffy_style(arena, eid)
        }
    };
    apply_virtual_slot_y(arena, eid, &mut style);
    style
}

fn to_taffy_padding_lp(p: Padding) -> taffy::Rect<taffy::LengthPercentage> {
    taffy::Rect {
        left: taffy::LengthPercentage::length(p.left),
        right: taffy::LengthPercentage::length(p.right),
        top: taffy::LengthPercentage::length(p.top),
        bottom: taffy::LengthPercentage::length(p.bottom),
    }
}

fn grid_container_taffy_style(
    arena: &ElementArena,
    eid: ElementId,
    grid_columns: u32,
) -> taffy::Style {
    let layout = arena.comp_layout(eid);
    let gap = layout.as_ref().map_or(0.0, |l| l.gap);
    let padding = layout.as_ref().map_or(Padding::ZERO, |l| l.padding);
    let alignment = layout
        .as_ref()
        .map_or(crate::style::Alignment::Start, |l| l.alignment);

    let ai = match alignment {
        crate::style::Alignment::Center => Some(taffy::AlignItems::CENTER),
        crate::style::Alignment::End => Some(taffy::AlignItems::END),
        crate::style::Alignment::Stretch => Some(taffy::AlignItems::STRETCH),
        _ => None,
    };
    let content_align = layout
        .as_ref()
        .map_or(crate::style::Alignment::Start, |l| l.content_align);
    let jc = match content_align {
        crate::style::Alignment::Center => Some(taffy::JustifyContent::CENTER),
        crate::style::Alignment::End => Some(taffy::JustifyContent::END),
        _ => None,
    };

    // If explicit column widths are set, use them; otherwise N × 1fr.
    let col_widths = layout
        .as_ref()
        .map(|l| &l.grid_column_widths)
        .filter(|v| !v.is_empty());
    let tracks: Vec<_> = if let Some(widths) = col_widths {
        widths
            .iter()
            .map(|&w| {
                if w > 0.0 {
                    taffy::prelude::length(w)
                } else {
                    // w <= 0 means flex fraction; use 1fr with min-content floor
                    taffy::prelude::fr(1.0_f32)
                }
            })
            .collect()
    } else {
        (0..grid_columns)
            .map(|_| taffy::prelude::fr(1.0_f32))
            .collect()
    };

    taffy::Style {
        display: taffy::Display::Grid,
        grid_template_columns: tracks,
        grid_auto_flow: taffy::style::GridAutoFlow::Row,
        gap: taffy::Size {
            width: taffy::LengthPercentage::length(gap),
            height: taffy::LengthPercentage::length(gap),
        },
        padding: to_taffy_padding_lp(padding),
        align_items: ai,
        justify_content: jc,
        ..Default::default()
    }
}

/// Build taffy style for a flex container (has active children).
fn flex_container_taffy_style(arena: &ElementArena, eid: ElementId) -> taffy::Style {
    let layout = arena.comp_layout(eid);
    let text_comp = arena.comp_text(eid);
    let scroll_comp = arena.comp_scroll(eid);
    let el = arena.get(eid);

    let layout_dir = layout
        .as_ref()
        .map_or(crate::core::element::LayoutDirection::Vertical, |l| {
            l.layout_direction
        });
    let mut style = match layout_dir {
        crate::core::element::LayoutDirection::Vertical => vstack_style(),
        crate::core::element::LayoutDirection::Horizontal => hstack_style(),
    };

    let alignment = layout
        .as_ref()
        .map_or(crate::style::Alignment::Start, |l| l.alignment);
    let ai = match alignment {
        crate::style::Alignment::Center => Some(taffy::AlignItems::CENTER),
        crate::style::Alignment::End => Some(taffy::AlignItems::FLEX_END),
        crate::style::Alignment::Stretch => Some(taffy::AlignItems::STRETCH),
        _ => None,
    };
    let content_align = layout
        .as_ref()
        .map_or(crate::style::Alignment::Start, |l| l.content_align);
    let jc = match content_align {
        crate::style::Alignment::Center => Some(taffy::JustifyContent::CENTER),
        crate::style::Alignment::End => Some(taffy::JustifyContent::FLEX_END),
        _ => None,
    };
    if let Some(a) = ai {
        style.align_items = Some(a);
    }
    if let Some(j) = jc {
        style.justify_content = Some(j);
    }

    let gap = layout.as_ref().map_or(0.0, |l| l.gap);
    style.gap = taffy::Size {
        width: taffy::LengthPercentage::length(gap),
        height: taffy::LengthPercentage::length(gap),
    };
    let padding = layout.as_ref().map_or(Padding::ZERO, |l| l.padding);
    style.padding = taffy::Rect {
        left: taffy::LengthPercentage::length(padding.left),
        right: taffy::LengthPercentage::length(padding.right),
        top: taffy::LengthPercentage::length(padding.top),
        bottom: taffy::LengthPercentage::length(padding.bottom),
    };
    let flex_wrap = layout
        .as_ref()
        .map_or(crate::core::config::FlexWrap::NoWrap, |l| l.flex_wrap);
    style.flex_wrap = match flex_wrap {
        crate::core::config::FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
        crate::core::config::FlexWrap::Wrap => taffy::FlexWrap::Wrap,
        crate::core::config::FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
    };
    let flex_grow = layout.as_ref().map_or(0.0, |l| l.flex_grow);
    if flex_grow > 0.0 {
        style.flex_grow = flex_grow;
    }
    style.flex_shrink = layout.as_ref().map_or(1.0, |l| l.flex_shrink);
    let flex_basis = layout.as_ref().map_or(0.0, |l| l.flex_basis);
    // Prefer original Dimension for percent support
    let fb_percent_dim = layout
        .as_ref()
        .map(|lc| lc.flex_basis_dim)
        .filter(|d| matches!(d, Dimension::Percent(_)));
    if let Some(d) = fb_percent_dim {
        style.flex_basis = to_taffy_dim(d);
    } else if flex_basis > 0.0 {
        style.flex_basis = taffy::Dimension::length(flex_basis);
    }
    if let Some(ar) = layout.as_ref().and_then(|l| l.aspect_ratio) {
        style.aspect_ratio = Some(ar);
    }
    if text_comp
        .as_ref()
        .map_or(crate::style::TextDirection::Ltr, |t| t.text_direction)
        == crate::style::TextDirection::Rtl
    {
        style.direction = taffy::Direction::Rtl;
    }
    let overflow = layout
        .as_ref()
        .map_or(crate::core::config::Overflow::Visible, |l| l.overflow);
    if overflow != crate::core::config::Overflow::Visible {
        let (ox, oy) = match overflow {
            crate::core::config::Overflow::Clip => {
                (taffy::Overflow::Hidden, taffy::Overflow::Hidden)
            }
            crate::core::config::Overflow::Scroll => {
                (taffy::Overflow::Scroll, taffy::Overflow::Scroll)
            }
            _ => (taffy::Overflow::Visible, taffy::Overflow::Visible),
        };
        style.overflow = taffy::Point { x: ox, y: oy };
    }

    // min_main: >= 0 sets min_size to fixed value on both axes
    // (allows shrink below content min-size in flex layouts)
    if let Some(l) = layout.as_ref() {
        if l.min_main >= 0.0 {
            let mm = taffy::Dimension::length(l.min_main);
            style.min_size = taffy::Size {
                width: mm,
                height: mm,
            };
        }
    }

    // Apply explicit dimensions to taffy's size when either:
    // 1. affected_by_child_size is false (the element declares size independence), or
    // 2. width_dim/height_dim are explicit Pixels (user explicitly called .width()/.height()).
    //
    // Without this, taffy sees deep fixed-size flex chains as auto-sized and
    // recursively computes min-content/max-content, leading to exponential-time
    // layout (e.g. depth 16 = 190ms, 20 = 2.7s, 24 = 72s).
    let affected_by_child = layout.as_ref().is_none_or(|l| l.affected_by_child_size);
    let w_has_px = layout
        .as_ref()
        .and_then(|l| l.width_dim.as_ref())
        .is_some_and(|d| matches!(d, crate::style::Dimension::Pixels(_)));
    let h_has_px = matches!(
        layout.as_ref().map(|l| &l.height_dim),
        Some(crate::style::Dimension::Pixels(_))
    );
    let apply_w = !affected_by_child || w_has_px;
    let apply_h = !affected_by_child || h_has_px;

    if apply_w {
        if let Some(pw) = layout.as_ref().and_then(|l| l.preferred_width) {
            if pw > 0.0 {
                style.size.width = width_taffy_dim(&layout, pw);
            }
        }
    }
    if apply_h {
        let ph = layout.as_ref().map_or(0.0, |l| l.preferred_height);
        if ph > 0.0 {
            style.size.height = height_taffy_dim(&layout, ph);
        }
    }

    let dyn_w = el
        .and_then(|el| el.get_user_data::<std::rc::Rc<std::cell::Cell<f32>>>())
        .map(|c| c.get());

    // Portal: absolute positioning
    let z_index = el.map_or(0, |el| el.z_index);
    if z_index > 0 {
        // Guard: z_index > 0 on a non-portal container triggers
        // Position::Absolute, pulling the element out of flex flow.
        // Portal/overlay widgets supply user_data (PortalHeight,
        // position_cell, PopoverGeometry, or backdrop flag) — if none
        // of these are present, this is likely a misuse. Paint-only
        // elevation should use set_z_index_floor instead.
        let has_portal_data = el.is_some_and(|el| {
            el.get_user_data::<crate::platform::portal::PortalHeight>()
                .is_some()
                || el
                    .get_user_data::<std::rc::Rc<std::cell::Cell<(f32, f32, f32)>>>()
                    .is_some()
                || el
                    .get_user_data::<crate::widgets::overlay::PopoverGeometry>()
                    .is_some()
                || el.backdrop()
        });
        if !has_portal_data {
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "[AURALIS-UI] z_index > 0 on a non-portal element → Position::Absolute, \
                    pulled out of flex flow. Use set_z_index_floor for paint-only elevation."
                );
            }
        }

        style.position = taffy::Position::Absolute;
        let dyn_h: Option<std::rc::Rc<std::cell::Cell<f32>>> = el
            .and_then(|el| el.get_user_data::<crate::platform::portal::PortalHeight>())
            .map(|ph| ph.0.clone());
        // Position-cell elements are content-sized on the unconstrained
        // axes — use 'auto' for right/bottom so the element sizes to its
        // content rather than stretching to the parent (which would break
        // resize handles and other fixed-position, content-sized widgets).
        let auto_inset = true;
        if let Some(pos) =
            el.and_then(|el| el.get_user_data::<std::rc::Rc<std::cell::Cell<(f32, f32, f32)>>>())
        {
            let (x, y, w) = pos.get();
            style.inset = taffy::Rect {
                left: taffy::LengthPercentageAuto::length(x),
                top: taffy::LengthPercentageAuto::length(y),
                right: if auto_inset {
                    taffy::LengthPercentageAuto::auto()
                } else {
                    taffy::LengthPercentageAuto::length(0.0)
                },
                bottom: if auto_inset {
                    taffy::LengthPercentageAuto::auto()
                } else {
                    taffy::LengthPercentageAuto::length(0.0)
                },
            };
            style.size.width = taffy::Dimension::length(w);
            if let Some(h) = dyn_h {
                let height = h.get();
                if height > 0.0 {
                    style.size.height = taffy::Dimension::length(height);
                }
            }
        } else if let Some(geo) = el.and_then(|el| {
            el.get_user_data::<std::rc::Rc<std::cell::Cell<crate::widgets::overlay::PopoverGeometry>>>()
        }) {
            let g = geo.get();
            style.inset = taffy::Rect {
                left: taffy::LengthPercentageAuto::length(g.x),
                top: taffy::LengthPercentageAuto::length(g.y),
                right: if auto_inset {
                    taffy::LengthPercentageAuto::auto()
                } else {
                    taffy::LengthPercentageAuto::length(0.0)
                },
                bottom: if auto_inset {
                    taffy::LengthPercentageAuto::auto()
                } else {
                    taffy::LengthPercentageAuto::length(0.0)
                },
            };
            style.size.width = taffy::Dimension::length(g.width);
            if let Some(h) = dyn_h {
                let height = h.get();
                if height > 0.0 {
                    style.size.height = taffy::Dimension::length(height);
                }
            }
        } else {
            // Portal element without a position cell (Modal, Dialog backdrops).
            // If the element has backdrop=true, pin to all four edges so taffy
            // produces full-viewport bounds, enabling correct hit-test blocking
            // and backdrop rendering coverage.
            let is_backdrop = el.is_some_and(|el| el.backdrop());
            if is_backdrop {
                style.inset = taffy::Rect {
                    left: taffy::LengthPercentageAuto::length(0.0),
                    top: taffy::LengthPercentageAuto::length(0.0),
                    right: taffy::LengthPercentageAuto::length(0.0),
                    bottom: taffy::LengthPercentageAuto::length(0.0),
                };
            }
            if let Some(h) = dyn_h {
                let height = h.get();
                if height > 0.0 {
                    style.size.height = taffy::Dimension::length(height);
                }
            }
        }
    }

    // Scrollable container
    if scroll_comp.is_some() {
        if let Some(pw) = layout.as_ref().and_then(|l| l.preferred_width) {
            style.size.width = width_taffy_dim(&layout, pw);
        }
        if flex_grow > 0.0 {
            style.flex_grow = flex_grow;
        }
        let h = if let Some(dyn_h) =
            el.and_then(|el| el.get_user_data::<std::rc::Rc<std::cell::Cell<f32>>>())
        {
            dyn_h.get()
        } else {
            layout.as_ref().map_or(36.0, |l| l.preferred_height)
        };
        style.size.height = height_taffy_dim(&layout, h);
        // Keep the container's default align_items (e.g. Stretch for VStack).
        // FlexStart was previously forced here, which prevented children from
        // stretching to fill the container's cross-axis (e.g. List items
        // couldn't fill the full row width).
    }

    if let Some(w) = dyn_w {
        style.size.width = taffy::Dimension::length(w);
    }

    style
}

fn leaf_taffy_style(arena: &ElementArena, eid: ElementId) -> taffy::Style {
    let el = arena.get(eid);
    let role = el.and_then(|el| el.accessible_role());
    let label_len = el
        .and_then(|el| el.accessible_label())
        .map(|l| l.len())
        .unwrap_or(0);
    let fs = arena.comp_text(eid).map_or(18.0, |t| t.font_size);
    // Dynamic text: when preferred_width is set, it serves as a minimum floor
    // (e.g. Checkbox sets min_w). Use max(measured_text_width, preferred_width)
    // so the element never shrinks below its configured minimum.
    // When preferred_width is absent, the natural text width applies.
    // Non-dynamic elements go straight to preferred_width (else branch).
    let pw = if el.is_some_and(|el| el.lazy_label().is_some()) {
        let mtw = arena.comp_text(eid).and_then(|t| {
            let v = t.measured_text_width.get();
            if v > 0.0 {
                Some(v)
            } else {
                None
            }
        });
        let pref = arena.comp_layout(eid).and_then(|l| l.preferred_width);
        match (mtw, pref) {
            (Some(text_w), Some(min_w)) => Some(text_w.max(min_w)),
            (mtw, pref) => mtw.or(pref),
        }
    } else {
        arena.comp_layout(eid).and_then(|l| l.preferred_width)
    };
    let ph = arena.comp_layout(eid).map_or(36.0, |l| l.preferred_height);

    let (w, h) = match role {
        Some(accesskit::Role::Button) => {
            let w = pw.unwrap_or((label_len as f32 * fs * 0.65 + ph).max(ph).min(600.0));
            (w, ph)
        }
        Some(accesskit::Role::TextInput) => {
            let h = ph.max(28.0);
            let w = pw.unwrap_or(200.0);
            (w, h)
        }
        Some(accesskit::Role::CheckBox | accesskit::Role::Switch) => {
            let w = pw.unwrap_or((label_len as f32 * 10.0 + 36.0).max(60.0));
            (w, ph)
        }
        Some(accesskit::Role::Slider) => {
            let w = pw.unwrap_or(200.0);
            (w, ph)
        }
        Some(accesskit::Role::Image) => {
            // Images should stretch to the parent's cross-axis width.
            // ContentFit handles scaling at paint time.
            let w = pw.unwrap_or(200.0);
            (w, ph)
        }
        _ => {
            let w = pw.unwrap_or((label_len as f32 * 10.0 + 16.0).max(40.0));
            (w, ph)
        }
    };

    let layout = arena.comp_layout(eid);
    let padding = layout.as_ref().map_or(Padding::ZERO, |l| l.padding);
    let affected = layout.as_ref().is_none_or(|l| l.affected_by_child_size);

    taffy::Style {
        size: if !affected {
            // affected_by_child_size(false): only set axes with explicit values.
            // Cross-axis dimensions not set here become auto(), letting
            // align-items: stretch fill the container on that axis.
            let pw = layout.as_ref().and_then(|l| l.preferred_width);
            let ph = layout.as_ref().map_or(0.0, |l| l.preferred_height);
            let mut sz = taffy::Size {
                width: taffy::Dimension::auto(),
                height: taffy::Dimension::auto(),
            };
            if let Some(pw_val) = pw {
                if pw_val > 0.0 {
                    sz.width = width_taffy_dim(&layout, pw_val + padding.left + padding.right);
                }
            }
            if ph > 0.0 {
                sz.height = height_taffy_dim(&layout, ph + padding.top + padding.bottom);
            }
            sz
        } else {
            taffy::Size {
                width: match role {
                    Some(accesskit::Role::Image) => taffy::Dimension::auto(),
                    _ => width_taffy_dim(&layout, w + padding.left + padding.right),
                },
                height: height_taffy_dim(&layout, h + padding.top + padding.bottom),
            }
        },
        flex_grow: arena.comp_layout(eid).map_or(0.0, |l| l.flex_grow),
        flex_shrink: arena.comp_layout(eid).map_or(1.0, |l| l.flex_shrink),
        min_size: match arena.comp_layout(eid).map(|l| l.min_main).unwrap_or(-1.0) {
            mm if mm >= 0.0 => {
                let dim = taffy::Dimension::length(mm);
                taffy::Size {
                    width: dim,
                    height: dim,
                }
            }
            _ => taffy::Size {
                width: taffy::Dimension::auto(),
                height: taffy::Dimension::auto(),
            },
        },
        aspect_ratio: layout.as_ref().and_then(|l| l.aspect_ratio),
        ..Default::default()
    }
}

// ═══════════════════════ Recursive tree builder ═══════════════════════

/// Build taffy tree recursively from arena elements.
/// Used by both full-tree build and subtree rebuild.
fn build_element_taffy_tree(
    arena: &ElementArena,
    eid: ElementId,
    taffy: &mut TaffyBridge,
) -> taffy::NodeId {
    let Some(element) = arena.get(eid) else {
        // Callers validate ids against the arena before recursing, so this
        // is unreachable unless an element is torn down mid-build. Degrade
        // to an empty leaf (renders nothing) instead of crashing the frame.
        debug_assert!(false, "build_element_taffy_tree: {eid:?} not in arena");
        let node = taffy
            .tree
            .new_leaf(taffy::Style::default())
            .expect("taffy new_leaf is infallible");
        taffy.register_node(eid, node);
        return node;
    };
    let children = element.children.clone();
    let dyn_w = element
        .get_user_data::<std::rc::Rc<std::cell::Cell<f32>>>()
        .map(|c| c.get());

    let active_child_ids: Vec<ElementId> = children
        .iter()
        .filter(|&&cid| {
            arena
                .get(cid)
                .map(|c| !c.slot_inactive.get())
                .unwrap_or(false)
        })
        .copied()
        .collect();

    if active_child_ids.is_empty() {
        let mut style = leaf_taffy_style(arena, eid);
        if let Some(w) = dyn_w {
            style.size.width = taffy::Dimension::length(w);
        }
        apply_virtual_slot_y(arena, eid, &mut style);
        let node = taffy
            .tree
            .new_leaf(style)
            .expect("taffy new_leaf is infallible");
        taffy.register_node(eid, node);
        return node;
    }

    let grid_cols = arena.comp_layout(eid).map_or(0, |l| l.grid_columns);

    let child_nodes: Vec<taffy::NodeId> = active_child_ids
        .iter()
        .map(|&cid| {
            let child_node = build_element_taffy_tree(arena, cid, taffy);
            // Grid children: set grid-column span/offset on the taffy node
            if grid_cols > 0 {
                let span = arena.comp_layout(cid).map_or(0, |l| l.grid_column_span);
                let offset = arena.comp_layout(cid).map_or(0, |l| l.grid_column_offset);
                // Note: offset 0 is valid (column line 1).  The old condition
                // `offset > 0` prevented cells at column 0 from being explicitly
                // placed, causing all of them to auto-place into column 1.
                if span > 0 || offset > 0 {
                    let mut child_style = taffy
                        .tree
                        .style(child_node)
                        .expect("just-built taffy child node must have a style")
                        .clone();
                    use taffy::style::GridPlacement;
                    child_style.grid_column = if span > 0 {
                        taffy::prelude::Line {
                            start: GridPlacement::Line((offset as i16 + 1).into()),
                            end: GridPlacement::Line((offset as i16 + span as i16 + 1).into()),
                        }
                    } else {
                        taffy::prelude::Line {
                            start: GridPlacement::Line((offset as i16 + 1).into()),
                            end: GridPlacement::Auto,
                        }
                    };
                    taffy
                        .tree
                        .set_style(child_node, child_style)
                        .expect("just-built taffy child node accepts set_style");
                }
            }
            child_node
        })
        .collect();

    let mut style = if grid_cols > 0 {
        grid_container_taffy_style(arena, eid, grid_cols)
    } else {
        flex_container_taffy_style(arena, eid)
    };

    if let Some(w) = dyn_w {
        style.size.width = taffy::Dimension::length(w);
    }

    apply_virtual_slot_y(arena, eid, &mut style);

    let node = taffy
        .tree
        .new_with_children(style, &child_nodes)
        .expect("taffy new_with_children is infallible");
    taffy.register_node(eid, node);
    node
}
