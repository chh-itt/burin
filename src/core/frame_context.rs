//! Explicit per-frame context. Holds frame-only state (caches, spatial grid,
//! focus order, phase) borrowed as &RefCell handles, plus a shared &AppContext.
//! Only alive within on_frame()'s call stack; widgets never see it.
use crate::core::app_context::AppContext;
use crate::core::element::ElementId;
use crate::core::frame_pipeline::FramePhase;
use crate::render::{CachedScene, CachedSubtree};
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct FrameContext<'a> {
    pub app: &'a AppContext,
    pub phase: Cell<FramePhase>,
    pub scene_cache: &'a RefCell<FxHashMap<ElementId, Rc<CachedScene>>>,
    pub subtree_cache: &'a RefCell<FxHashMap<ElementId, Rc<CachedSubtree>>>,
}

impl<'a> FrameContext<'a> {
    pub fn new(
        app: &'a AppContext,
        scene_cache: &'a RefCell<FxHashMap<ElementId, Rc<CachedScene>>>,
        subtree_cache: &'a RefCell<FxHashMap<ElementId, Rc<CachedSubtree>>>,
    ) -> Self {
        FrameContext {
            app,
            phase: Cell::new(FramePhase::None),
            scene_cache,
            subtree_cache,
        }
    }
}
