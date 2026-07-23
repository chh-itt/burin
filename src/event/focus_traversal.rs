//! Focus traversal policies: pluggable strategies for Tab order and
//! arrow-key directional navigation.
//!
//! Each [`FocusScope`](super::FocusScope) can carry a policy that controls
//! the order of focusable elements within that scope. The default
//! [`TabOrderPolicy`] matches the framework's original behaviour
//! (`tab_index` + `tree_order`).  [`ReadingOrderPolicy`] sorts spatially
//! (top→bottom, left→right).  Directonal navigation (arrow keys) uses
//! `nearest_in_direction` under the hood.

use crate::core::dirty_registry;
use crate::core::element::ElementArena;
use crate::core::ElementId;
use crate::style::Rect;

/// Cardinal direction for arrow-key focus traversal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Pluggable focus traversal strategy.
///
/// Assigned per [`FocusScope`](super::FocusScope) to control:
/// - **Tab / Shift+Tab** order via [`sorted`](TraversalPolicy::sorted)
/// - **Arrow-key** navigation via [`in_direction`](TraversalPolicy::in_direction)
pub trait TraversalPolicy: std::fmt::Debug {
    /// Return all focusable descendants of `scope_root`, ordered as this
    /// policy dictates (for Tab / Shift+Tab navigation).
    fn sorted(&self, arena: &ElementArena, scope_root: ElementId) -> Vec<ElementId>;

    /// Starting from `focused`, find the element reachable by moving in
    /// `direction` — or `None` if no element exists in that direction.
    fn in_direction(
        &self,
        arena: &ElementArena,
        focused: ElementId,
        direction: Direction,
    ) -> Option<ElementId>;
}

// ── TabOrderPolicy ──────────────────────────────────────────────

/// Default policy: elements are ordered by `(tab_index DESC, tree_order ASC)`
/// — exactly the order produced by [`ensure_focus_order`].
#[derive(Debug)]
pub struct TabOrderPolicy;

impl TraversalPolicy for TabOrderPolicy {
    fn sorted(&self, arena: &ElementArena, scope_root: ElementId) -> Vec<ElementId> {
        let all = dirty_registry::ensure_focus_order(arena);
        all.into_iter()
            .filter(|&eid| dirty_registry::is_descendant_of(eid, scope_root))
            .collect()
    }

    fn in_direction(
        &self,
        arena: &ElementArena,
        focused: ElementId,
        direction: Direction,
    ) -> Option<ElementId> {
        let all = dirty_registry::ensure_focus_order(arena);
        let pos = all.iter().position(|&eid| eid == focused)?;
        // Walk through the sorted order in the given direction
        let len = all.len();
        match direction {
            Direction::Down | Direction::Right => {
                for i in 1..len {
                    let idx = (pos + i) % len;
                    if dirty_registry::is_visible_chain_fast(all[idx]) {
                        return Some(all[idx]);
                    }
                }
                None
            }
            Direction::Up | Direction::Left => {
                for i in 1..len {
                    let idx = if pos >= i {
                        pos - i
                    } else {
                        len - (i - pos) % len
                    };
                    if idx >= len {
                        break;
                    }
                    if dirty_registry::is_visible_chain_fast(all[idx]) {
                        return Some(all[idx]);
                    }
                }
                None
            }
        }
    }
}

// ── WidgetOrderPolicy ───────────────────────────────────────────

/// Policy: elements are ordered by their creation order (`tree_order ASC`).
/// This is the simplest possible ordering — first created = first focused.
#[derive(Debug)]
pub struct WidgetOrderPolicy;

impl TraversalPolicy for WidgetOrderPolicy {
    fn sorted(&self, _arena: &ElementArena, scope_root: ElementId) -> Vec<ElementId> {
        // Collect all focusable elements sorted by tree_order within scope
        use std::collections::BTreeSet;
        let mut set = BTreeSet::new();
        // Walk all known focusable elements via FOCUSABLE_SET
        dirty_registry::visit_focusable(|eid, tree_order| {
            if dirty_registry::is_descendant_of(eid, scope_root) {
                set.insert((tree_order, eid));
            }
        });
        set.into_iter().map(|(_, eid)| eid).collect()
    }

    fn in_direction(
        &self,
        arena: &ElementArena,
        focused: ElementId,
        direction: Direction,
    ) -> Option<ElementId> {
        TabOrderPolicy.in_direction(arena, focused, direction)
    }
}

// ── ReadingOrderPolicy ──────────────────────────────────────────

/// Policy: elements are sorted by their screen position —
/// top→bottom, left→right (natural reading order).
///
/// Within the same row (vertical overlap > half height), elements
/// are sorted left→right. Rows are top→bottom.
#[derive(Debug)]
pub struct ReadingOrderPolicy;

impl TraversalPolicy for ReadingOrderPolicy {
    fn sorted(&self, _arena: &ElementArena, scope_root: ElementId) -> Vec<ElementId> {
        use std::cmp::Ordering;
        let mut result: Vec<ElementId> = Vec::new();
        dirty_registry::visit_focusable(|eid, _| {
            if dirty_registry::is_descendant_of(eid, scope_root) {
                result.push(eid);
            }
        });
        let halved: Vec<(ElementId, f32, f32, f32)> = result
            .iter()
            .filter_map(|&eid| {
                let bounds = dirty_registry::bounds_of(eid)?;
                let cy = bounds.y + bounds.height / 2.0;
                let cx = bounds.x + bounds.width / 2.0;
                Some((eid, bounds.y, cx, cy))
            })
            .collect();
        let mut pairs: Vec<(ElementId, f32, f32)> = halved
            .into_iter()
            .map(|(eid, top, cx, _cy)| (eid, top, cx))
            .collect();
        pairs.sort_by(|&(_, ay, ax), &(_, by, bx)| {
            // Same row if vertical overlap > half the smaller height
            if (ay - by).abs() < 20.0 {
                ax.partial_cmp(&bx).unwrap_or(Ordering::Equal)
            } else {
                ay.partial_cmp(&by).unwrap_or(Ordering::Equal)
            }
        });
        pairs.into_iter().map(|(eid, _, _)| eid).collect()
    }

    fn in_direction(
        &self,
        arena: &ElementArena,
        focused: ElementId,
        direction: Direction,
    ) -> Option<ElementId> {
        // Use all focusable descendants of parent as candidates
        let scope_root = dirty_registry::parent_of(focused).unwrap_or(focused);
        let candidates = self.sorted(arena, scope_root);
        nearest_in_direction(arena, focused, &candidates, direction)
    }
}

// ── Directional navigation helpers ──────────────────────────────

/// Find the nearest focusable element from `focused` going in `direction`,
/// using a band-based spatial algorithm (inspired by Flutter's
/// `DirectionalFocusTraversalPolicyMixin`).
///
/// 1. Filter candidates that lie in the general direction.
/// 2. Within those, prefer ones whose projection overlaps with `focused`
///    in the orthogonal axis.
/// 3. Pick the closest by primary-axis distance.
/// Maximum distance (in pixels) for directional focus navigation.
/// Candidates farther than this are ignored to prevent cross-section jumps.
const MAX_DIRECTIONAL_DIST: f32 = 2000.0;

pub fn nearest_in_direction(
    _arena: &crate::core::element::ElementArena,
    focused: ElementId,
    candidates: &[ElementId],
    direction: Direction,
) -> Option<ElementId> {
    let fb = dirty_registry::bounds_of(focused)?;

    // Phase 1 — strict band: only candidates whose projection overlaps
    // the focused element in the orthogonal axis (same row for Left/Right,
    // same column for Up/Down).
    let band_best = find_best_in_band(candidates, focused, &fb, direction, true);
    if band_best.is_some() {
        return band_best;
    }

    // Phase 2 — edge band: expand to candidates whose edge is closest
    // to the focused element's orthogonal band (but still within MAX_DIST).
    find_best_in_band(candidates, focused, &fb, direction, false)
}

/// Finds the best candidate in `direction` from `focused`.
///
/// When `strict_band` is `true`, only candidates whose orthogonal projection
/// overlaps the focused element are considered (e.g., same row for Right/Left).
/// When `false`, candidates are also accepted if their orthogonal gap from the
/// focused band is within `band_margin × focused_size` — this prevents jumping
/// to elements in completely different visual areas.
fn find_best_in_band(
    candidates: &[ElementId],
    focused: ElementId,
    fb: &Rect,
    direction: Direction,
    strict_band: bool,
) -> Option<ElementId> {
    // Band margin: how far a candidate can be from the focused element's
    // projection band and still be considered (in Phase 2 only).
    let band_margin = match direction {
        Direction::Right | Direction::Left => (fb.height * 3.0).max(60.0),
        Direction::Up | Direction::Down => (fb.width * 3.0).max(60.0),
    };

    let mut best: Option<(ElementId, f32, bool)> = None; // (eid, primary_dist, has_ortho_overlap)

    for &cid in candidates {
        if cid == focused {
            continue;
        }
        let cb = match dirty_registry::bounds_of(cid) {
            Some(b) => b,
            None => continue,
        };

        let (primary_dist, has_ortho, ortho_gap) = match direction {
            Direction::Down => {
                if cb.y + cb.height <= fb.y {
                    continue;
                }
                let p = cb.y - fb.y - fb.height;
                let h = ortho(cb.x, cb.width, fb.x, fb.width);
                (
                    p,
                    h,
                    if h {
                        0.0
                    } else {
                        (cb.x - fb.x)
                            .abs()
                            .min((cb.x + cb.width - fb.x - fb.width).abs())
                    },
                )
            }
            Direction::Up => {
                if cb.y >= fb.y + fb.height {
                    continue;
                }
                let p = fb.y - cb.y - cb.height;
                let h = ortho(cb.x, cb.width, fb.x, fb.width);
                (
                    p,
                    h,
                    if h {
                        0.0
                    } else {
                        (cb.x - fb.x)
                            .abs()
                            .min((cb.x + cb.width - fb.x - fb.width).abs())
                    },
                )
            }
            Direction::Right => {
                if cb.x + cb.width <= fb.x {
                    continue;
                }
                let p = cb.x - fb.x - fb.width;
                let h = ortho(cb.y, cb.height, fb.y, fb.height);
                (
                    p,
                    h,
                    if h {
                        0.0
                    } else {
                        (cb.y - fb.y)
                            .abs()
                            .min((cb.y + cb.height - fb.y - fb.height).abs())
                    },
                )
            }
            Direction::Left => {
                if cb.x >= fb.x + fb.width {
                    continue;
                }
                let p = fb.x - cb.x - cb.width;
                let h = ortho(cb.y, cb.height, fb.y, fb.height);
                (
                    p,
                    h,
                    if h {
                        0.0
                    } else {
                        (cb.y - fb.y)
                            .abs()
                            .min((cb.y + cb.height - fb.y - fb.height).abs())
                    },
                )
            }
        };

        if primary_dist < 0.0 {
            continue;
        }

        // Strict band: must have orthogonal overlap
        if strict_band && !has_ortho {
            continue;
        }

        // Phase 2 (non-strict): candidates without ortho overlap must be
        // within the band margin — prevents cross-section jumps.
        if !strict_band && !has_ortho && ortho_gap > band_margin {
            continue;
        }

        // Distance cutoff
        if primary_dist > MAX_DIRECTIONAL_DIST {
            continue;
        }

        let overlap_score = if has_ortho { 1i8 } else { 0i8 };

        let is_better = match best {
            Some((_, bp, bo)) => {
                let best_overlap = if bo { 1i8 } else { 0i8 };
                overlap_score > best_overlap || (overlap_score == best_overlap && primary_dist < bp)
            }
            None => true,
        };

        if is_better {
            best = Some((cid, primary_dist, has_ortho));
        }
    }

    best.map(|(eid, _, _)| eid)
}

/// Whether the candidate's projection overlaps the focused element's
/// projection in the given axis (vertical for Left/Right, horizontal
/// for Up/Down).
fn ortho(cpos: f32, csize: f32, ppos: f32, psize: f32) -> bool {
    let c_end = cpos + csize;
    let p_end = ppos + psize;
    c_end > ppos && cpos < p_end
}
