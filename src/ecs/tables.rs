//! Typed component tables: HashMap<ElementId, T> per component type.
//!
//! O(1) access, O(k) type-filtered queries. Components are populated on demand.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::core::id::ElementId;
use crate::ecs::components::*;

/// Per-component entry counts for the Arena & Memory DevTools panel.
#[derive(Clone, Debug, Default)]
pub struct ComponentEntryCounts {
    pub style: usize,
    pub layout: usize,
    pub interact: usize,
    pub text: usize,
    pub scroll: usize,
    pub cursor: usize,
    pub tooltip: usize,
    pub dragdrop: usize,
    pub anim: usize,
    pub xform: usize,
    pub a11y: usize,
    pub lifecycle: usize,
    pub extensions: usize,
    pub total: usize,
}

#[derive(Clone)]
pub struct ComponentTables {
    pub style: HashMap<ElementId, StyleComponent>,
    pub layout: HashMap<ElementId, LayoutComponent>,
    pub interact: HashMap<ElementId, InteractionComponent>,
    pub text: HashMap<ElementId, TextComponent>,
    pub scroll: HashMap<ElementId, ScrollComponent>,
    pub cursor: HashMap<ElementId, CursorComponent>,
    pub tooltip: HashMap<ElementId, TooltipComponent>,
    pub dragdrop: HashMap<ElementId, DragDropComponent>,
    pub anim: HashMap<ElementId, AnimationComponent>,
    pub xform: HashMap<ElementId, TransformComponent>,
    pub a11y: HashMap<ElementId, AccessibleComponent>,
    pub lc: HashMap<ElementId, LifecycleComponent>,
    /// Third-party component extension slots. Keyed by TypeId, each slot
    /// is a typed `HashMap<ElementId, T>`. Use `register_component<T>()`
    /// and `get_component<T>()` for type-safe access.
    /// Rc-wrapped for Clone support (dyn Any is not Clone).
    #[allow(clippy::type_complexity)]
    pub extensions: std::rc::Rc<std::cell::RefCell<HashMap<TypeId, Box<dyn Any>>>>,
}

impl ComponentTables {
    pub fn new() -> Self {
        Self {
            style: HashMap::new(),
            layout: HashMap::new(),
            interact: HashMap::new(),
            text: HashMap::new(),
            scroll: HashMap::new(),
            cursor: HashMap::new(),
            tooltip: HashMap::new(),
            dragdrop: HashMap::new(),
            anim: HashMap::new(),
            xform: HashMap::new(),
            a11y: HashMap::new(),
            lc: HashMap::new(),
            extensions: std::rc::Rc::new(std::cell::RefCell::new(HashMap::new())),
        }
    }

    /// Remove all component entries for an element.
    pub fn remove_element(&mut self, eid: ElementId) {
        self.style.remove(&eid);
        self.layout.remove(&eid);
        self.interact.remove(&eid);
        self.text.remove(&eid);
        self.scroll.remove(&eid);
        self.cursor.remove(&eid);
        self.tooltip.remove(&eid);
        self.dragdrop.remove(&eid);
        self.anim.remove(&eid);
        self.xform.remove(&eid);
        self.a11y.remove(&eid);
        self.lc.remove(&eid);
        for slot in self.extensions.borrow_mut().values_mut() {
            if let Some(map) = slot.downcast_mut::<HashMap<ElementId, Box<dyn Any>>>() {
                map.remove(&eid);
            }
        }
    }

    /// Register a new component type with the extension slot table.
    /// Returns the TypeId key for subsequent access.
    pub fn register_component<T: Clone + 'static>(&mut self) -> TypeId {
        let tid = TypeId::of::<T>();
        self.extensions
            .borrow_mut()
            .entry(tid)
            .or_insert_with(|| Box::new(HashMap::<ElementId, T>::new()));
        tid
    }

    /// Get a clone of a third-party component for an element.
    pub fn get_component<T: Clone + 'static>(&self, eid: ElementId) -> Option<T> {
        let tid = TypeId::of::<T>();
        let ext = self.extensions.borrow();
        ext.get(&tid)?
            .downcast_ref::<HashMap<ElementId, T>>()?
            .get(&eid)
            .cloned()
    }

    /// Mutate a third-party component for an element via a closure.
    /// Returns `true` if the component existed and was mutated.
    pub fn with_component<T: Clone + 'static>(
        &mut self,
        eid: ElementId,
        f: impl FnOnce(&mut T),
    ) -> bool {
        let tid = TypeId::of::<T>();
        let mut ext = self.extensions.borrow_mut();
        if let Some(map) = ext.get_mut(&tid) {
            if let Some(map) = map.downcast_mut::<HashMap<ElementId, T>>() {
                if let Some(val) = map.get_mut(&eid) {
                    f(val);
                    return true;
                }
            }
        }
        false
    }

    /// Insert or update a third-party component for an element.
    pub fn set_component<T: Clone + 'static>(&mut self, eid: ElementId, value: T) {
        let tid = TypeId::of::<T>();
        let mut ext = self.extensions.borrow_mut();
        let map = ext
            .entry(tid)
            .or_insert_with(|| Box::new(HashMap::<ElementId, T>::new()))
            .downcast_mut::<HashMap<ElementId, T>>()
            .expect("ComponentTables: type-id collision");
        map.insert(eid, value);
    }

    /// Pre-allocate component entries for an element based on its widget's
    /// declared component mask. Called by `ElementBuilder::build()` after
    /// `allocate()` and before any setter runs, so the HashMap entries exist
    /// up front rather than being lazily inserted by the first setter's
    /// `entry().or_default()`. This ensures the element is visible to O(k)
    /// component-filtered queries before any setter has been called.
    ///
    /// Widgets that implement a custom `mount_box` without `ElementBuilder`
    /// should call this directly if they want the same guarantee.
    pub fn preallocate(&mut self, eid: ElementId, mask: u64) {
        if mask & STYLE != 0 {
            self.style.entry(eid).or_default();
        }
        if mask & LAYOUT != 0 {
            self.layout.entry(eid).or_default();
        }
        if mask & INTERACTION != 0 {
            self.interact.entry(eid).or_default();
        }
        if mask & TEXT != 0 {
            self.text.entry(eid).or_default();
        }
        if mask & SCROLL != 0 {
            self.scroll.entry(eid).or_default();
        }
        if mask & CURSOR != 0 {
            self.cursor.entry(eid).or_default();
        }
        if mask & TOOLTIP != 0 {
            self.tooltip.entry(eid).or_default();
        }
        if mask & DRAG_DROP != 0 {
            self.dragdrop.entry(eid).or_default();
        }
        if mask & ANIMATION != 0 {
            self.anim.entry(eid).or_default();
        }
        if mask & TRANSFORM != 0 {
            self.xform.entry(eid).or_default();
        }
        if mask & ACCESSIBLE != 0 {
            self.a11y.entry(eid).or_default();
        }
        if mask & LIFECYCLE != 0 {
            self.lc.entry(eid).or_default();
        }
    }

    /// Return per-component entry counts for memory/arena diagnostics.
    pub fn entry_counts(&self) -> ComponentEntryCounts {
        let s = self.style.len();
        let ly = self.layout.len();
        let i = self.interact.len();
        let tx = self.text.len();
        let sc = self.scroll.len();
        let cu = self.cursor.len();
        let tt = self.tooltip.len();
        let dd = self.dragdrop.len();
        let an = self.anim.len();
        let xf = self.xform.len();
        let aa = self.a11y.len();
        let lc = self.lc.len();
        let ex = self.extensions.borrow().len();
        ComponentEntryCounts {
            style: s,
            layout: ly,
            interact: i,
            text: tx,
            scroll: sc,
            cursor: cu,
            tooltip: tt,
            dragdrop: dd,
            anim: an,
            xform: xf,
            a11y: aa,
            lifecycle: lc,
            extensions: ex,
            total: s + ly + i + tx + sc + cu + tt + dd + an + xf + aa + lc + ex,
        }
    }
}

impl Default for ComponentTables {
    fn default() -> Self {
        Self::new()
    }
}
