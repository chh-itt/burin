//! Accessibility tree builder with O(k) incremental rebuild.
//!
//! Uses [`crate::ecs::mark_a11y_changed`] to track which elements need their
//! `Node` objects rebuilt (role, label, structure).  For unchanged elements,
//! the cached `Node` is reused and only bounds are updated.
//!
//! When the tree structure changes (`add_child`/`remove_child`), the parent
//! is marked via `mark_a11y_changed`, and the next build only reconstructs
//! nodes for elements in that set.  All other elements reuse their cached
//! `Node`, which is bounds-updated in-place.

use std::cell::RefCell;
use std::collections::HashMap;

use accesskit::{Live, Node, NodeId, Rect as A11yRect, Role, Tree, TreeId, TreeUpdate};

use crate::core::config::{AriaLive, StateFlags};
use crate::core::element::ElementArena;
use crate::core::ElementId;

thread_local! {
    /// Cached `Node` objects keyed by ElementId, reused across builds
    /// to avoid re-allocating nodes for unchanged elements.
    ///
    /// Lifecycle (audit 2026-07-17 round 3, Finding A): entries of torn-down
    /// elements are dropped via the teardown hook below; visibility-based
    /// eviction in `collect_nodes_incremental` only covers hidden-but-alive
    /// elements.
    static A11Y_NODE_CACHE: RefCell<HashMap<ElementId, Node>> = RefCell::new(HashMap::new());
}

fn teardown_cleanup(id: ElementId) {
    A11Y_NODE_CACHE.with(|cache| {
        cache.borrow_mut().remove(&id);
    });
}

/// Test-only introspection: cached node count.
#[doc(hidden)]
pub fn debug_node_cache_len() -> usize {
    A11Y_NODE_CACHE.with(|cache| cache.borrow().len())
}

/// Build the accessibility tree.  Iterates all visible elements; for those
/// marked `a11y-changed`, creates fresh `Node` instances and updates the cache.
/// For unchanged elements, reuses cached nodes (only bounds are rewritten).
pub fn build_accessibility_tree(
    arena: &ElementArena,
    root_id: ElementId,
    focus: Option<ElementId>,
) -> TreeUpdate {
    crate::core::dirty_registry::register_teardown_hook(teardown_cleanup);
    let changed = crate::ecs::drain_a11y_changed();
    let mut nodes: Vec<(NodeId, Node)> = Vec::new();

    A11Y_NODE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        collect_nodes_incremental(arena, root_id, &mut nodes, &changed, &mut cache);
    });

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(NodeId(id_to_u64(root_id)))),
        tree_id: TreeId::ROOT,
        focus: focus
            .map(|id| NodeId(id_to_u64(id)))
            .unwrap_or(NodeId(id_to_u64(root_id))),
    }
}

fn aria_live(live: AriaLive) -> Live {
    match live {
        AriaLive::Off => Live::Off,
        AriaLive::Polite => Live::Polite,
        AriaLive::Assertive => Live::Assertive,
    }
}

fn fill_node(element: &crate::core::element::Element, node: &mut Node) {
    if let Some(label) = element.accessible_label() {
        node.set_label(label);
    }
    if let Some(desc) = element.accessible_description() {
        node.set_description(desc);
    }

    let state = element.state.get();

    if let Some(v) = element.accessible_value() {
        node.set_value(format!("{}", v));
    }
    if state.contains(StateFlags::DISABLED) {
        node.set_disabled();
    }
    if let Some(checked) = element.accessible_checked() {
        node.set_toggled(if checked {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        });
    }
    if element.accessible_hidden() {
        node.set_hidden();
    }
    if element.accessible_required() {
        node.set_required();
    }
    if let Some(level) = element.accessible_level() {
        node.set_level(level as usize);
    }
    let live = element.accessible_live();
    if live != AriaLive::Off {
        node.set_live(aria_live(live));
    }
    if element.selected() {
        node.set_selected(true);
    }
    if let Some(desc_id) = element.active_descendant() {
        node.set_active_descendant(NodeId(id_to_u64(desc_id)));
    }
}

fn visible_children(arena: &ElementArena, element: &crate::core::element::Element) -> Vec<NodeId> {
    element
        .children
        .iter()
        .filter(|&&c| arena.get(c).is_some_and(|child| child.is_visible()))
        .map(|&c| NodeId(id_to_u64(c)))
        .collect()
}

fn collect_nodes_incremental(
    arena: &ElementArena,
    eid: ElementId,
    out: &mut Vec<(NodeId, Node)>,
    changed: &std::collections::HashSet<ElementId>,
    cache: &mut HashMap<ElementId, Node>,
) {
    let element = match arena.get(eid) {
        Some(el) => el,
        None => return,
    };

    if !element.is_visible() {
        cache.remove(&eid);
        return;
    }

    let node_id = NodeId(id_to_u64(eid));

    if changed.contains(&eid) {
        let role = element.accessible_role().unwrap_or(Role::GenericContainer);
        let mut node = Node::new(role);
        fill_node(element, &mut node);
        node.set_bounds(A11yRect::new(
            element.bounds().x as f64,
            element.bounds().y as f64,
            element.bounds().max_x() as f64,
            element.bounds().max_y() as f64,
        ));
        node.set_children(visible_children(arena, element));
        cache.insert(eid, node.clone());
        out.push((node_id, node));
    } else if let Some(cached) = cache.get(&eid).cloned() {
        let mut node = cached;
        node.set_bounds(A11yRect::new(
            element.bounds().x as f64,
            element.bounds().y as f64,
            element.bounds().max_x() as f64,
            element.bounds().max_y() as f64,
        ));
        node.set_children(visible_children(arena, element));
        out.push((node_id, node));
    } else {
        let role = element.accessible_role().unwrap_or(Role::GenericContainer);
        let mut node = Node::new(role);
        fill_node(element, &mut node);
        node.set_bounds(A11yRect::new(
            element.bounds().x as f64,
            element.bounds().y as f64,
            element.bounds().max_x() as f64,
            element.bounds().max_y() as f64,
        ));
        node.set_children(visible_children(arena, element));
        cache.insert(eid, node.clone());
        out.push((node_id, node));
    }

    for &child_id in &element.children {
        collect_nodes_incremental(arena, child_id, out, changed, cache);
    }
}

fn id_to_u64(id: ElementId) -> u64 {
    id.to_u64()
}
