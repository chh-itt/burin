//! Blanket extension trait for chaining `.test_id()` / `.name()` on any Widget.
//!
//! The `Tagged<W>` wrapper mounts the inner widget, then calls
//! `Element::set_test_id` / `Element::set_name` on the returned element
//! — it does NOT add a child element, so the tree structure is unchanged.

use crate::core::context::MountContext;
use crate::core::element::ElementId;
use crate::core::widget::Widget;

/// A widget wrapper that tags its inner element with a `test_id` and/or `name`
/// after mounting. Does not introduce an extra tree node.
pub struct Tagged<W: Widget> {
    inner: W,
    test_id: Option<String>,
    name: Option<String>,
}

impl<W: Widget> Widget for Tagged<W> {
    fn component_mask(&self) -> u64 {
        self.inner.component_mask()
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let this = *self;
        let id = Box::new(this.inner).mount_box(ctx);
        if let Some(t) = this.test_id {
            if let Some(el) = ctx.arena.get_mut(id) {
                el.set_test_id(t);
            }
        }
        if let Some(n) = this.name {
            if let Some(el) = ctx.arena.get_mut(id) {
                el.set_name(n);
            }
        }
        id
    }
}

/// Blanket trait that adds `.test_id(...)` and `.name(...)` to every
/// `Widget` type.
///
/// ```ignore
/// use burin::testing::WidgetTestExt;
/// let id = h.mount(Button::new("Click").test_id("btn"));
/// ```
pub trait WidgetTestExt: Widget + Sized {
    fn test_id(self, id: impl Into<String>) -> Tagged<Self> {
        Tagged {
            inner: self,
            test_id: Some(id.into()),
            name: None,
        }
    }

    fn name(self, n: impl Into<String>) -> Tagged<Self> {
        Tagged {
            inner: self,
            test_id: None,
            name: Some(n.into()),
        }
    }
}

impl<W: Widget> WidgetTestExt for W {}
