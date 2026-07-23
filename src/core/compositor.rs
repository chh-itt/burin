use auralis_task::TaskScope;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;

/// A state-holding container that manages a child [`TaskScope`].
///
/// `Compositor` is the primary way to introduce local reactive state
/// (`Signal`, `Memo`) into the widget tree.  It creates a child scope
/// for the enclosed widget, ensuring that:
///
/// - Signals created inside are cleaned up when the compositor is removed.
/// - Async tasks spawned inside are cancelled on drop.
/// - Suspend / resume propagates correctly through the scope tree.
///
/// # Example
///
/// ```
/// use burin::core::{Compositor, Widget};
/// use auralis_signal::Signal;
///
/// fn counter() -> impl Widget {
///     Compositor::new(|_scope| {
///         let _count = Signal::new(0);
///         burin::widgets::display::Text::new("counter")
///     })
/// }
/// ```
pub struct Compositor<F, W> {
    build: F,
    _phantom: std::marker::PhantomData<fn() -> W>,
}

impl<F, W> Compositor<F, W> {
    pub fn new(build: F) -> Self
    where
        F: FnOnce(&TaskScope) -> W,
    {
        Self {
            build,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F, W> Widget for Compositor<F, W>
where
    F: FnOnce(&TaskScope) -> W + 'static,
    W: Widget + 'static,
{
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let child_scope = TaskScope::new();
        let widget: W = (self.build)(&child_scope);
        Box::new(widget).mount_box(ctx)
    }
}

impl<F, W> std::fmt::Debug for Compositor<F, W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compositor").finish_non_exhaustive()
    }
}
