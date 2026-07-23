//! Damage-rectangle computation for the CPU backend.
//!
//! Rewritten in the 2026-07-16 audit. The old version collected raw
//! `screen_bounds` of dirty elements, which under-covered reality in three
//! ways (audit C6):
//! - **shadows/outlines** paint outside `screen_bounds` (up to `blur+offset`,
//!   e.g. 40 px for M3 Level5 — the old fixed 5 px margin left fringes);
//! - **transforms** (drag ghost, scale/translate animations) move the painted
//!   pixels away from the layout bounds entirely;
//! - **moves**: the previously-painted area (the "vacated" region) was never
//!   repainted, leaving ghosts.
//!
//! [`DamageTracker`] therefore computes an **inflated visual rect** per dirty
//! element (bounds ∪ shadow/outline extent, transformed), unions it with the
//! element's *previous* visual rect, and finally makes the merged rectangles
//! **disjoint** — overlapping damage rects would double-composite translucent
//! commands (audit C5).

use crate::core::id::ElementId;
use crate::style::Rect;
use std::collections::HashMap;

/// Anti-aliasing / rounding safety margin (logical px).
const MARGIN: f32 = 3.0;

pub struct DamageTracker {
    /// Last inflated visual rect per element, keyed by element id. Entries
    /// are dropped when the element disappears from the arena.
    prev_visual: HashMap<ElementId, Rect>,
}

impl Default for DamageTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl DamageTracker {
    pub fn new() -> Self {
        Self {
            prev_visual: HashMap::new(),
        }
    }

    /// Compute merged, **disjoint** damage rectangles from the dirty set.
    /// Returns rects clamped to `viewport`; an empty result means "nothing
    /// to repaint" (the caller may still force a full-frame pass).
    pub fn compute(
        &mut self,
        dirty_ids: &[ElementId],
        arena: &crate::core::element::ElementArena,
        max_rects: usize,
        viewport: Rect,
    ) -> Vec<Rect> {
        let tables = arena.component_tables.clone();
        let ct = tables.borrow();
        let mut rects: Vec<Rect> = Vec::with_capacity(dirty_ids.len() * 2);

        for id in dirty_ids {
            match arena.get(*id) {
                Some(el) => {
                    let visual = inflated_visual_rect(*id, el, &ct);
                    if let Some(prev) = self.prev_visual.insert(*id, visual) {
                        if prev != visual && prev.width > 0.0 && prev.height > 0.0 {
                            rects.push(prev);
                        }
                    }
                    if visual.width > 0.0 && visual.height > 0.0 {
                        rects.push(visual);
                    }
                }
                None => {
                    // Element gone: repaint the area it used to occupy.
                    if let Some(prev) = self.prev_visual.remove(id) {
                        if prev.width > 0.0 && prev.height > 0.0 {
                            rects.push(prev);
                        }
                    }
                }
            }
        }

        rects = deduplicate_contained(rects);
        greedy_merge(&mut rects, max_rects);

        // Margin + viewport clamp.
        let mut out: Vec<Rect> = Vec::with_capacity(rects.len());
        for r in rects {
            let x0 = (r.x - MARGIN).max(viewport.x);
            let y0 = (r.y - MARGIN).max(viewport.y);
            let x1 = (r.x + r.width + MARGIN).min(viewport.x + viewport.width);
            let y1 = (r.y + r.height + MARGIN).min(viewport.y + viewport.height);
            if x1 > x0 && y1 > y0 {
                out.push(Rect::new(x0, y0, x1 - x0, y1 - y0));
            }
        }

        // Correctness: damage rects must not overlap (each pass re-executes
        // the full command list; translucent straddlers would double-blend).
        make_disjoint(&mut out);
        out
    }

    /// Forget tracked state (e.g. after a full-tree rebuild).
    pub fn clear(&mut self) {
        self.prev_visual.clear();
    }
}

/// The element's *visual* footprint: screen bounds, adjusted by paint-time
/// offsets, inflated by decoration overflow (shadow/outline), then passed
/// through the element's own transform (matching `paint_element_tree`).
fn inflated_visual_rect(
    eid: ElementId,
    el: &crate::core::element::Element,
    ct: &crate::ecs::tables::ComponentTables,
) -> Rect {
    let sb = el.screen_bounds;
    let mut x = sb.x;
    let mut y = sb.y;
    let mut w = sb.width.max(1.0);
    let mut h = sb.height.max(1.0);

    if let Some(xf) = ct.xform.get(&eid) {
        let off = xf.position_offset.get();
        x += off.x;
        y += off.y;
        let sc = xf.size_scale.get();
        w *= sc.x;
        h *= sc.y;
    }

    // Decoration overflow (mirrors paint: shadow blur/offset, outline ring).
    let mut grow = 0.0f32;
    if let Some(s) = ct.style.get(&eid) {
        let resolved = crate::style::state_style::resolve_style(el.state.get(), s);
        if let Some(sh) = resolved.shadow {
            grow = grow.max(sh.blur + sh.offset_x.abs().max(sh.offset_y.abs()));
        }
        if resolved.outline_width > 0.0 {
            grow = grow.max(resolved.outline_width + 1.0);
        }
    }
    let mut r = Rect::new(x - grow, y - grow, w + 2.0 * grow, h + 2.0 * grow);

    // Element transform (pivot semantics identical to paint_element_tree).
    if let Some(xf) = ct.xform.get(&eid) {
        if let Some(t) = xf.transform {
            let tx = glam::Affine2::from_cols_array(&t);
            let ox = xf.transform_origin_x * w;
            let oy = xf.transform_origin_y * h;
            let to_origin = glam::Affine2::from_translation(glam::Vec2::new(-(x + ox), -(y + oy)));
            let from_origin = glam::Affine2::from_translation(glam::Vec2::new(x + ox, y + oy));
            let m = from_origin * tx * to_origin;
            let c = [
                m.transform_point2(glam::Vec2::new(r.x, r.y)),
                m.transform_point2(glam::Vec2::new(r.x + r.width, r.y)),
                m.transform_point2(glam::Vec2::new(r.x + r.width, r.y + r.height)),
                m.transform_point2(glam::Vec2::new(r.x, r.y + r.height)),
            ];
            let min_x = c.iter().map(|p| p.x).fold(f32::MAX, f32::min);
            let min_y = c.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            let max_x = c.iter().map(|p| p.x).fold(f32::MIN, f32::max);
            let max_y = c.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            // Union with the untransformed rect: mid-animation frames must
            // also erase the untransformed position.
            let t_rect = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
            r = r.union(&t_rect);
        }
    }
    r
}

/// Remove rectangles that are fully contained by another rectangle.
fn deduplicate_contained(mut rects: Vec<Rect>) -> Vec<Rect> {
    let mut i = 0;
    while i < rects.len() {
        let r = rects[i];
        let contained = rects
            .iter()
            .enumerate()
            .any(|(j, &other)| j != i && other.contains_rect(&r));
        if contained {
            rects.swap_remove(i);
        } else {
            i += 1;
        }
    }
    rects
}

/// Greedily merge rectangles until ≤ `max` remain.
///
/// Each iteration finds the pair whose union has the smallest area increase
/// and merges them.  O(k²) on `k` rectangles (k ≤ 30 in practice).
fn greedy_merge(rects: &mut Vec<Rect>, max: usize) {
    while rects.len() > max {
        let mut best_area_increase = f32::MAX;
        let mut best_pair = (0usize, 0usize);
        let mut best_merged = Rect::ZERO;

        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let merged = rects[i].union(&rects[j]);
                let increase = merged.area() - rects[i].area() - rects[j].area()
                    + rects[i].intersection_area(&rects[j]);
                if increase < best_area_increase {
                    best_area_increase = increase;
                    best_pair = (i, j);
                    best_merged = merged;
                }
            }
        }

        let (i, j) = best_pair;
        rects.remove(j);
        rects.remove(i);
        rects.push(best_merged);
    }
}

/// Merge any overlapping pairs until all rectangles are pairwise disjoint.
fn make_disjoint(rects: &mut Vec<Rect>) {
    loop {
        let mut merged_any = false;
        'outer: for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                if rects[i].intersects(&rects[j]) {
                    let u = rects[i].union(&rects[j]);
                    rects.remove(j);
                    rects[i] = u;
                    merged_any = true;
                    break 'outer;
                }
            }
        }
        if !merged_any {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_contained() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(10.0, 10.0, 20.0, 20.0); // fully inside a
        let c = Rect::new(200.0, 200.0, 50.0, 50.0);
        let result = deduplicate_contained(vec![a, b, c]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_greedy_merge_single() {
        let mut rects = vec![Rect::new(0.0, 0.0, 100.0, 50.0)];
        greedy_merge(&mut rects, 4);
        assert_eq!(rects.len(), 1);
    }

    #[test]
    fn test_greedy_merge_overlapping() {
        let mut rects = vec![
            Rect::new(0.0, 0.0, 100.0, 50.0),
            Rect::new(80.0, 0.0, 100.0, 50.0),
            Rect::new(200.0, 10.0, 50.0, 30.0),
            Rect::new(210.0, 0.0, 60.0, 50.0),
            Rect::new(400.0, 400.0, 100.0, 100.0),
        ];
        greedy_merge(&mut rects, 3);
        assert_eq!(rects.len(), 3);
    }

    #[test]
    fn test_make_disjoint() {
        let mut rects = vec![
            Rect::new(0.0, 0.0, 100.0, 50.0),
            Rect::new(80.0, 0.0, 100.0, 50.0), // overlaps first
            Rect::new(400.0, 400.0, 100.0, 100.0),
        ];
        make_disjoint(&mut rects);
        assert_eq!(rects.len(), 2);
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                assert!(!rects[i].intersects(&rects[j]), "rects must be disjoint");
            }
        }
    }
}
