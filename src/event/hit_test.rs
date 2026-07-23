use crate::core::element::ElementArena;
use crate::core::ElementId;
use crate::style::Point;

pub struct HitTestResult {
    pub target: ElementId,
    pub path: Vec<ElementId>,
}

pub fn hit_test(arena: &ElementArena, point: Point) -> Option<HitTestResult> {
    let target = crate::core::dirty_registry::hit_test_with_fallback(arena, point)?;
    let path = arena.path_to_root(target);
    if path.is_empty() {
        return None;
    }
    Some(HitTestResult { target, path })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_visible_element() {
        let mut arena = ElementArena::new();
        let root_id = arena.allocate();
        arena.set_root(root_id);
        if let Some(el) = arena.get_mut(root_id) {
            el.set_bounds(crate::style::Rect::new(0.0, 0.0, 200.0, 100.0));
            el.screen_bounds = el.bounds();
        }
        crate::core::dirty_registry::register_bounds(
            root_id,
            arena.get(root_id).unwrap().screen_bounds,
        );
        let result = hit_test(&arena, Point::new(50.0, 50.0));
        assert!(result.is_some());
        assert_eq!(result.unwrap().target, root_id);
    }

    #[test]
    fn hit_test_invisible_ignored() {
        let mut arena = ElementArena::new();
        let root_id = arena.allocate();
        arena.set_root(root_id);
        if let Some(el) = arena.get_mut(root_id) {
            el.set_bounds(crate::style::Rect::new(0.0, 0.0, 200.0, 100.0));
            el.screen_bounds = el.bounds();
            el.set_visible(false);
        }
        crate::core::dirty_registry::register_bounds(
            root_id,
            arena.get(root_id).unwrap().screen_bounds,
        );
        assert!(hit_test(&arena, Point::new(50.0, 50.0)).is_none());
    }

    #[test]
    fn hit_test_outside_bounds() {
        let mut arena = ElementArena::new();
        let root_id = arena.allocate();
        arena.set_root(root_id);
        if let Some(el) = arena.get_mut(root_id) {
            el.set_bounds(crate::style::Rect::new(0.0, 0.0, 100.0, 100.0));
            el.screen_bounds = el.bounds();
        }
        crate::core::dirty_registry::register_bounds(
            root_id,
            arena.get(root_id).unwrap().screen_bounds,
        );
        assert!(hit_test(&arena, Point::new(200.0, 50.0)).is_none());
    }
}
