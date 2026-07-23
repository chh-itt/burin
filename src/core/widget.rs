use crate::core::context::MountContext;
use crate::core::element::ElementId;

/// The fundamental building block of burin.
///
/// Widgets can either override `mount_box` directly (full control),
/// or use the declarative `build_element` with `ElementBuilder`.
pub trait Widget: 'static {
    /// Consume this widget and create a retained [`Element`] in the arena.
    /// Returns the [`ElementId`] of the newly created element.
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId;

    /// Declare which ECS components this widget uses.
    ///
    /// The returned bitmask is passed to `ElementBuilder::with_components()`
    /// inside `mount_box`, which triggers `ComponentTables::preallocate()`
    /// right after the element is allocated. This ensures the element is
    /// immediately visible to O(k) component-filtered queries and avoids
    /// the lazy HashMap-insert that `entry().or_default()` would otherwise
    /// perform on the first setter call.
    ///
    /// Override this in concrete widget types (Button, TextInput, etc.)
    /// to declare the exact set of components they use.  Default is 0
    /// (no pre-allocation — still works, just no early query visibility).
    fn component_mask(&self) -> u64 {
        0
    }
}

/// Marker trait for widgets without signal subscriptions.
pub trait StaticWidget: 'static {
    fn mount_static(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId;
}

impl<T: StaticWidget> Widget for T {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        StaticWidget::mount_static(self, ctx)
    }
}

/// Mount a type-erased `Box<dyn Widget>`.
pub fn mount_erased(w: Box<dyn Widget>, ctx: &mut MountContext<'_>) -> ElementId {
    w.mount_box(ctx)
}
