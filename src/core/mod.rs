//! Core abstractions: Widget trait, Element, Context types, Prop, Compositor.

pub mod app_context;
pub mod blink;
pub mod clock;
pub mod compositor;
pub mod config;
pub mod context;
pub mod dirty_registry;
pub mod element;
pub mod error;
pub mod frame_context;
pub mod frame_driver;
pub mod frame_pipeline;
pub mod id;
pub mod perf;
pub mod prop;
pub mod scheduler;
pub mod signal_bridge;
pub mod undo;
pub mod widget;

pub use signal_bridge::{
    apply_observed_subscriptions, bind_dirty, bind_dirty_measure, bind_dirty_reposition,
    observe_element, set_implicit_dirty, store_subscription, subscribe_owned,
};

pub use compositor::Compositor;
pub use context::{EventCtx, LayoutCtx, MountContext, PaintCtx};
pub use element::{DirtyFlags, Element, ElementArena, LayoutDirection};
pub use id::ElementId;
pub use prop::Prop;
pub use widget::{StaticWidget, Widget};
