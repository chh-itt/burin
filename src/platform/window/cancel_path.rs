use crate::core::element::ElementArena;
use crate::core::ElementId;

pub(crate) fn cancel_path_for_visible(arena: &ElementArena, root_id: ElementId) -> Vec<ElementId> {
    let mut path = Vec::new();
    if find_cancel_handler(arena, root_id, &mut path) {
        path
    } else {
        Vec::new()
    }
}

pub(crate) fn find_cancel_handler(arena: &ElementArena, eid: ElementId, path: &mut Vec<ElementId>) -> bool {
    let el = match arena.get(eid) {
        Some(e) => e,
        None => return false,
    };
    if !el.is_visible() {
        return false;
    }
    path.push(eid);
    let child_ids = &el.children;
    for cid in child_ids.iter().rev() {
        if find_cancel_handler(arena, *cid, path) {
            return true;
        }
    }
    if el.reactive_visible().is_some() || (el.z_index > 0 && el.is_focusable()) {
        return true;
    }
    path.pop();
    false
}
