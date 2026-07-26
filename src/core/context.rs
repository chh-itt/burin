use crate::core::app_context::AppContext;
use crate::core::element::ElementArena;
use crate::core::id::ElementId;
use crate::event::EventRegistry;
use crate::platform::clipboard::Clipboard;
use crate::platform::WindowHandle;
use crate::style::StyleRefinement;
use crate::theme::m3::roles::{ComponentRole, ResolvedComponentStyle};
use crate::theme::M3Theme;
use std::rc::Weak;

/// Context passed to [`Widget::mount_box`](crate::core::Widget::mount_box).
///
/// Gives the widget access to the element arena, event registry, theme,
/// clipboard, and the process-level app context (as a weak reference).
/// Call [`child_with_events`](Self::child_with_events) to create scoped
/// contexts when mounting children.
pub struct MountContext<'a> {
    /// The single-source-of-truth element arena.
    pub arena: &'a mut ElementArena,
    pub parent_id: Option<ElementId>,
    /// Event registry for registering click handlers etc.
    pub event_registry: Option<&'a mut EventRegistry>,
    /// Active theme. Widgets read this to resolve colors and sizing.
    pub theme: &'a M3Theme,
    /// Handle for runtime window control (minimize, maximize, fullscreen, etc.).
    /// `None` in headless/test environments.
    pub window_handle: Option<&'a WindowHandle>,
    /// Clipboard access.  Always present (zero-sized handle); operations
    /// return `Err(ClipboardError::NotAvailable)` when the feature is
    /// disabled or the platform does not support the clipboard.
    pub clipboard: Clipboard,
    /// Weak handle to the process-level AppContext. Widgets clone this into
    /// event/signal callbacks to enqueue dirty intents without holding a strong ref.
    pub app: Weak<AppContext>,
    #[cfg(feature = "i18n")]
    pub i18n: Option<&'a crate::i18n::I18n>,
}

impl<'a> MountContext<'a> {
    pub fn new(
        arena: &'a mut ElementArena,
        parent_id: Option<ElementId>,
        registry: Option<&'a mut EventRegistry>,
        theme: &'a M3Theme,
        window_handle: Option<&'a WindowHandle>,
        app: Weak<AppContext>,
    ) -> Self {
        Self {
            arena,
            parent_id,
            event_registry: registry,
            theme,
            window_handle,
            clipboard: Clipboard::new(),
            app,
            #[cfg(feature = "i18n")]
            i18n: None,
        }
    }

    /// Create a child context that inherits event registry AND theme.
    pub fn child_with_events<'b>(&'b mut self, parent_id: ElementId) -> MountContext<'b>
    where
        'a: 'b,
    {
        MountContext {
            arena: self.arena,
            parent_id: Some(parent_id),
            event_registry: self.event_registry.as_deref_mut(),
            theme: self.theme,
            window_handle: self.window_handle,
            clipboard: Clipboard::new(),
            app: self.app.clone(),
            #[cfg(feature = "i18n")]
            i18n: self.i18n,
        }
    }

    pub fn parent_id(&self) -> Option<ElementId> {
        self.parent_id
    }

    /// Pre-allocate component table entries for an element.
    /// Call this after `arena.allocate()` and before setters when not using
    /// `ElementBuilder`, so O(k) component-filtered queries see the element.
    pub fn preallocate(&mut self, id: ElementId, mask: u64) {
        self.arena
            .component_tables
            .borrow_mut()
            .preallocate(id, mask);
    }

    /// Apply theme style + register component role + track for dynamic theme updates.
    ///
    /// Call this AFTER the element has been built (ElementBuilder::build / arena.allocate),
    /// passing the already-resolved `ResolvedComponentStyle`. This replaces the 7-line
    /// boilerplate (`apply_style_to_element` + `lc.component_role` + `register_theme_element`)
    /// that every widget repeats at the end of `mount_box`.
    pub fn register_theme_component(
        &mut self,
        id: ElementId,
        resolved: &ResolvedComponentStyle,
        role: &ComponentRole,
        style: &StyleRefinement,
    ) {
        if let Some(el) = self.arena.get_mut(id) {
            crate::theme::apply::apply_style_to_element(
                el,
                resolved,
                style,
                self.theme.is_dark,
                self.theme.scheme.design_interaction,
            );
        }
        if let Some(lc) = self.arena.component_tables.borrow_mut().lc.get_mut(&id) {
            lc.component_role = Some(role.clone());
            lc.style_refinement = Some(style.clone());
        }
        crate::ecs::register_theme_element(id);
    }
}

/// Context passed to a widget's layout callback.
pub struct LayoutCtx<'a> {
    pub(crate) _phantom: std::marker::PhantomData<&'a ()>,
}

/// Context passed to a widget's paint callback.
///
/// Carries the current viewport size and scale factor so the widget
/// can position its draw commands in screen space.
pub struct PaintCtx<'a> {
    pub viewport: crate::style::Size,
    pub scale_factor: f64,
    /// Handle for runtime window control. `None` in headless/test environments.
    pub window_handle: Option<&'a WindowHandle>,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> PaintCtx<'a> {
    pub fn new(
        viewport: crate::style::Size,
        scale_factor: f64,
        window_handle: Option<&'a WindowHandle>,
    ) -> Self {
        Self {
            viewport,
            scale_factor,
            window_handle,
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Context passed to event handlers registered via [`EventRegistry`].
///
/// Lets handlers stop propagation, prevent default behaviour, and
/// interact with the window and clipboard.
pub struct EventCtx<'a> {
    pub(crate) propagation_stopped: &'a mut bool,
    pub(crate) default_prevented: &'a mut bool,
    /// Handle for runtime window control. `None` in headless/test environments.
    pub window_handle: Option<&'a WindowHandle>,
    /// Clipboard access.  Always present (zero-sized handle); operations
    /// return `Err(ClipboardError::NotAvailable)` when the feature is
    /// disabled or the platform does not support the clipboard.
    pub clipboard: Clipboard,
}

impl<'a> EventCtx<'a> {
    pub fn new(
        propagation_stopped: &'a mut bool,
        default_prevented: &'a mut bool,
        window_handle: Option<&'a WindowHandle>,
    ) -> Self {
        Self {
            propagation_stopped,
            default_prevented,
            window_handle,
            clipboard: Clipboard::new(),
        }
    }
    pub fn stop_propagation(&mut self) {
        *self.propagation_stopped = true;
    }
    pub fn prevent_default(&mut self) {
        *self.default_prevented = true;
    }
    pub fn is_propagation_stopped(&self) -> bool {
        *self.propagation_stopped
    }
    pub fn is_default_prevented(&self) -> bool {
        *self.default_prevented
    }
}
