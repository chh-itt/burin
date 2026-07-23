//! Semantic selectors for finding elements in the GUI tree.
//!
//! # Basic selectors
//! ```ignore
//! use burin::testing::selector::{by_test_id, by_role, by_label, by_text, by_name};
//! ```
//!
//! # Composite selectors
//! ```ignore
//! use burin::testing::selector::{all, any, descendant_of, ancestor_of, left_of, right_of, above, below};
//! ```

use crate::core::element::ElementArena;
use crate::core::ElementId;

/// A semantic query that matches zero or more elements in a GUI tree.
#[derive(Clone, Debug)]
pub enum Selector {
    // ── Basic ──
    ByTestId(String),
    ByRole(accesskit::Role),
    ByLabel(String),
    ByText(String),
    ByName(String),

    // ── Composite ──
    All(Vec<Selector>),
    Any(Vec<Selector>),
    Not(Box<Selector>),
    DescendantOf(Box<Selector>),
    AncestorOf(Box<Selector>),

    // ── Geometry (requires screen_bounds — valid after first frame) ──
    LeftOf(Box<Selector>),
    RightOf(Box<Selector>),
    Above(Box<Selector>),
    Below(Box<Selector>),
}

impl Selector {
    /// Returns `true` if `eid` satisfies this selector.
    pub fn matches(&self, arena: &ElementArena, eid: ElementId) -> bool {
        let Some(el) = arena.get(eid) else {
            return false;
        };

        match self {
            Selector::ByTestId(id) => el.test_id().as_deref() == Some(id.as_str()),
            Selector::ByRole(role) => el.accessible_role() == Some(*role),
            Selector::ByLabel(text) => el
                .accessible_label()
                .map(|l| l.contains(text.as_str()))
                .unwrap_or(false),
            Selector::ByText(text) => {
                let label_match = el
                    .accessible_label()
                    .map(|l| l.contains(text.as_str()))
                    .unwrap_or(false);
                if label_match {
                    return true;
                }
                el.text_buffer()
                    .as_ref()
                    .map(|buf| {
                        let buf = buf.borrow();
                        buf.lines
                            .iter()
                            .any(|run| run.text().contains(text.as_str()))
                    })
                    .unwrap_or(false)
            }
            Selector::ByName(name) => el.name().as_deref() == Some(name.as_str()),

            Selector::All(selectors) => selectors.iter().all(|s| s.matches(arena, eid)),
            Selector::Any(selectors) => selectors.iter().any(|s| s.matches(arena, eid)),
            Selector::Not(sel) => !sel.matches(arena, eid),

            Selector::DescendantOf(ancestor) => {
                let mut cur = el.parent;
                while let Some(pid) = cur {
                    if ancestor.matches(arena, pid) {
                        return true;
                    }
                    cur = arena.get(pid).and_then(|p| p.parent);
                }
                false
            }
            Selector::AncestorOf(descendant) => {
                // Check if any descendant of eid matches
                Self::any_descendant_matches(arena, eid, descendant)
            }

            Selector::LeftOf(ref_sel) => {
                let Some(ref_id) = Self::first_match(arena, ref_sel) else {
                    return false;
                };
                let Some(ref_el) = arena.get(ref_id) else {
                    return false;
                };
                let sb = el.screen_bounds;
                let rsb = ref_el.screen_bounds;
                sb.width > 0.0 && rsb.width > 0.0 && sb.x + sb.width <= rsb.x
            }
            Selector::RightOf(ref_sel) => {
                let Some(ref_id) = Self::first_match(arena, ref_sel) else {
                    return false;
                };
                let Some(ref_el) = arena.get(ref_id) else {
                    return false;
                };
                let sb = el.screen_bounds;
                let rsb = ref_el.screen_bounds;
                sb.width > 0.0 && rsb.width > 0.0 && sb.x >= rsb.x + rsb.width
            }
            Selector::Above(ref_sel) => {
                let Some(ref_id) = Self::first_match(arena, ref_sel) else {
                    return false;
                };
                let Some(ref_el) = arena.get(ref_id) else {
                    return false;
                };
                let sb = el.screen_bounds;
                let rsb = ref_el.screen_bounds;
                sb.height > 0.0 && rsb.height > 0.0 && sb.y + sb.height <= rsb.y
            }
            Selector::Below(ref_sel) => {
                let Some(ref_id) = Self::first_match(arena, ref_sel) else {
                    return false;
                };
                let Some(ref_el) = arena.get(ref_id) else {
                    return false;
                };
                let sb = el.screen_bounds;
                let rsb = ref_el.screen_bounds;
                sb.height > 0.0 && rsb.height > 0.0 && sb.y >= rsb.y + rsb.height
            }
        }
    }

    /// Find the first element matching `inner` via a DFS (pre-order) walk from
    /// the arena root. Deterministic — not dependent on hash iteration order.
    fn first_match(arena: &ElementArena, inner: &Selector) -> Option<ElementId> {
        fn walk(arena: &ElementArena, eid: ElementId, sel: &Selector) -> Option<ElementId> {
            if sel.matches(arena, eid) {
                return Some(eid);
            }
            if let Some(el) = arena.get(eid) {
                for &cid in &el.children {
                    if let Some(found) = walk(arena, cid, sel) {
                        return Some(found);
                    }
                }
            }
            None
        }
        arena.root_id.and_then(|root| walk(arena, root, inner))
    }

    /// Recursively check if any descendant of `eid` matches `sel`.
    fn any_descendant_matches(arena: &ElementArena, eid: ElementId, sel: &Selector) -> bool {
        if let Some(el) = arena.get(eid) {
            for &cid in &el.children {
                if sel.matches(arena, cid) {
                    return true;
                }
                if Self::any_descendant_matches(arena, cid, sel) {
                    return true;
                }
            }
        }
        false
    }
}

// ── Constructor functions (ergonomic public API) ────────────────────

pub fn by_test_id(id: impl Into<String>) -> Selector {
    Selector::ByTestId(id.into())
}
pub fn by_role(role: accesskit::Role) -> Selector {
    Selector::ByRole(role)
}
pub fn by_label(text: impl Into<String>) -> Selector {
    Selector::ByLabel(text.into())
}
pub fn by_text(text: impl Into<String>) -> Selector {
    Selector::ByText(text.into())
}
pub fn by_name(name: impl Into<String>) -> Selector {
    Selector::ByName(name.into())
}

pub fn all(selectors: Vec<Selector>) -> Selector {
    Selector::All(selectors)
}
pub fn any(selectors: Vec<Selector>) -> Selector {
    Selector::Any(selectors)
}
pub fn not(sel: Selector) -> Selector {
    Selector::Not(Box::new(sel))
}

pub fn descendant_of(sel: Selector) -> Selector {
    Selector::DescendantOf(Box::new(sel))
}
pub fn ancestor_of(sel: Selector) -> Selector {
    Selector::AncestorOf(Box::new(sel))
}
pub fn left_of(sel: Selector) -> Selector {
    Selector::LeftOf(Box::new(sel))
}
pub fn right_of(sel: Selector) -> Selector {
    Selector::RightOf(Box::new(sel))
}
pub fn above(sel: Selector) -> Selector {
    Selector::Above(Box::new(sel))
}
pub fn below(sel: Selector) -> Selector {
    Selector::Below(Box::new(sel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::element::ElementArena;
    use crate::style::Rect;

    fn new_arena() -> (ElementArena, ElementId) {
        let mut arena = ElementArena::new();
        let root = arena.allocate();
        arena.set_root(root);
        (arena, root)
    }

    fn make_element(arena: &mut ElementArena, parent: ElementId, test_id: &str) -> ElementId {
        let id = arena.allocate();
        if let Some(el) = arena.get_mut(id) {
            el.set_test_id(test_id);
        }
        arena.add_child(parent, id);
        id
    }

    fn with_role(arena: &mut ElementArena, parent: ElementId, role: accesskit::Role) -> ElementId {
        let id = arena.allocate();
        if let Some(el) = arena.get_mut(id) {
            el.set_accessible_role(role);
        }
        arena.add_child(parent, id);
        id
    }

    fn with_label(arena: &mut ElementArena, parent: ElementId, label: &str) -> ElementId {
        let id = arena.allocate();
        if let Some(el) = arena.get_mut(id) {
            el.set_accessible_label(label);
        }
        arena.add_child(parent, id);
        id
    }

    // ── Basic selector tests ──

    #[test]
    fn by_test_id_matches() {
        let (mut arena, root) = new_arena();
        let a = make_element(&mut arena, root, "foo");
        let b = make_element(&mut arena, root, "bar");
        assert!(by_test_id("foo").matches(&arena, a));
        assert!(!by_test_id("foo").matches(&arena, b));
        assert!(!by_test_id("foo").matches(&arena, root));
    }

    #[test]
    fn by_role_matches() {
        let (mut arena, root) = new_arena();
        let btn = with_role(&mut arena, root, accesskit::Role::Button);
        let txt = with_role(&mut arena, root, accesskit::Role::Label);
        assert!(by_role(accesskit::Role::Button).matches(&arena, btn));
        assert!(!by_role(accesskit::Role::Button).matches(&arena, txt));
    }

    #[test]
    fn by_label_substring() {
        let (mut arena, root) = new_arena();
        let el = with_label(&mut arena, root, "Submit form");
        assert!(by_label("Submit").matches(&arena, el));
        assert!(by_label("form").matches(&arena, el));
        assert!(!by_label("cancel").matches(&arena, el));
    }

    #[test]
    fn by_name_exact() {
        let (mut arena, root) = new_arena();
        let id = arena.allocate();
        if let Some(el) = arena.get_mut(id) {
            el.set_name("MyWidget");
        }
        arena.add_child(root, id);
        assert!(by_name("MyWidget").matches(&arena, id));
        assert!(!by_name("My").matches(&arena, id));
    }

    #[test]
    fn by_text_matches_label_or_buffer() {
        let (mut arena, root) = new_arena();
        let el = with_label(&mut arena, root, "Hello World");
        assert!(by_text("Hello").matches(&arena, el));
        assert!(by_text("World").matches(&arena, el));
        assert!(!by_text("Xyzzy").matches(&arena, el));
    }

    // ── Composite tests ──

    #[test]
    fn all_requires_both() {
        let (mut arena, root) = new_arena();
        let el = make_element(&mut arena, root, "btn");
        if let Some(e) = arena.get_mut(el) {
            e.set_accessible_role(accesskit::Role::Button);
        }
        assert!(all(vec![by_test_id("btn"), by_role(accesskit::Role::Button)]).matches(&arena, el));
        assert!(!all(vec![by_test_id("btn"), by_role(accesskit::Role::Label)]).matches(&arena, el));
    }

    #[test]
    fn any_requires_one() {
        let (mut arena, root) = new_arena();
        let el = make_element(&mut arena, root, "btn");
        assert!(any(vec![by_test_id("btn"), by_test_id("nonexistent")]).matches(&arena, el));
        assert!(!any(vec![by_test_id("a"), by_test_id("b")]).matches(&arena, el));
    }

    #[test]
    fn not_inverts() {
        let (mut arena, root) = new_arena();
        let el = make_element(&mut arena, root, "btn");
        assert!(not(by_test_id("other")).matches(&arena, el));
        assert!(!not(by_test_id("btn")).matches(&arena, el));
    }

    #[test]
    fn descendant_of_matches() {
        let (mut arena, root) = new_arena();
        let parent = make_element(&mut arena, root, "container");
        let child = arena.allocate();
        arena.add_child(parent, child);
        assert!(descendant_of(by_test_id("container")).matches(&arena, child));
        assert!(!descendant_of(by_test_id("container")).matches(&arena, root));
    }

    #[test]
    fn ancestor_of_matches() {
        let (mut arena, root) = new_arena();
        let parent = make_element(&mut arena, root, "container");
        let _child = make_element(&mut arena, parent, "item");
        let sibling = make_element(&mut arena, root, "sibling");
        // parent is ancestor of item
        assert!(ancestor_of(by_test_id("item")).matches(&arena, parent));
        // sibling is NOT ancestor of item
        assert!(!ancestor_of(by_test_id("item")).matches(&arena, sibling));
    }

    // ── Geometry tests (requires screen_bounds) ──

    #[test]
    fn geometry_left_right() {
        let (mut arena, root) = new_arena();
        let left = make_element(&mut arena, root, "left");
        let right = make_element(&mut arena, root, "right");
        if let Some(el) = arena.get_mut(left) {
            el.screen_bounds = Rect::new(0.0, 0.0, 100.0, 50.0);
        }
        if let Some(el) = arena.get_mut(right) {
            el.screen_bounds = Rect::new(120.0, 0.0, 100.0, 50.0);
        }
        assert!(right_of(by_test_id("left")).matches(&arena, right));
        assert!(left_of(by_test_id("right")).matches(&arena, left));
        assert!(!right_of(by_test_id("left")).matches(&arena, left));
    }

    #[test]
    fn geometry_above_below() {
        let (mut arena, root) = new_arena();
        let top = make_element(&mut arena, root, "top");
        let bot = make_element(&mut arena, root, "bot");
        if let Some(el) = arena.get_mut(top) {
            el.screen_bounds = Rect::new(0.0, 0.0, 100.0, 50.0);
        }
        if let Some(el) = arena.get_mut(bot) {
            el.screen_bounds = Rect::new(0.0, 60.0, 100.0, 50.0);
        }
        assert!(below(by_test_id("top")).matches(&arena, bot));
        assert!(above(by_test_id("bot")).matches(&arena, top));
    }

    #[test]
    fn geometry_ignores_zero_size_bounds() {
        let (mut arena, root) = new_arena();
        let left = make_element(&mut arena, root, "left");
        let right = make_element(&mut arena, root, "right");
        // Zero-size bounds — geometry matches should fail gracefully
        assert!(!right_of(by_test_id("left")).matches(&arena, right));
        assert!(!left_of(by_test_id("right")).matches(&arena, left));
    }
}
