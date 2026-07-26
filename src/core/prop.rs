use std::cell::RefCell;
use std::rc::Rc;
use std::task::Poll;

/// Unified trait for static and dynamic values.
///
/// `Prop<T>` allows widget APIs to accept either a plain value (which
/// never changes) or a reactive `Signal<T>` (which triggers updates
/// when changed).  This eliminates the need for separate `*_dynamic`
/// method variants.
///
/// # Example
///
/// ```
/// use burin::core::Prop;
/// use auralis_signal::Signal;
///
/// // Accepts both:
/// fn show_label(text: impl Prop<String>) { /* ... */ }
///
/// show_label("static text".to_string());
/// show_label(Signal::new("dynamic text".to_string()));
/// ```
pub trait Prop<T: Clone + 'static> {
    /// Read the current value.
    fn read(&self) -> T;

    /// Register a callback to be invoked when the value changes.
    ///
    /// Returns a handle that, when dropped, unregisters the callback.
    /// For static values, the callback is never called.
    fn on_change(&self, callback: Box<dyn Fn()>) -> PropSubscription;
}

/// Handle returned by [`Prop::on_change`].  Drops the subscription when
/// this handle is dropped.
pub struct PropSubscription {
    _cleanup: Option<Box<dyn FnOnce()>>,
}

impl PropSubscription {
    /// Create a no-op subscription (for static values).
    pub fn noop() -> Self {
        Self { _cleanup: None }
    }

    /// Create a subscription with a cleanup closure.
    pub fn new(cleanup: impl FnOnce() + 'static) -> Self {
        Self {
            _cleanup: Some(Box::new(cleanup)),
        }
    }
}

impl Drop for PropSubscription {
    fn drop(&mut self) {
        if let Some(cleanup) = self._cleanup.take() {
            cleanup();
        }
    }
}

// ── Blanket impl for static (plain) values ──

impl<T: Clone + 'static> Prop<T> for T {
    fn read(&self) -> T {
        self.clone()
    }

    fn on_change(&self, _callback: Box<dyn Fn()>) -> PropSubscription {
        PropSubscription::noop() // Static values never change
    }
}

// ── Impl for auralis_signal::Signal<T> ──

impl<T: Clone + 'static> Prop<T> for auralis_signal::Signal<T> {
    fn read(&self) -> T {
        auralis_signal::Signal::read(self)
    }

    fn on_change(&self, callback: Box<dyn Fn()>) -> PropSubscription {
        let id = auralis_signal::subscribe(self, std::rc::Rc::new(callback));
        let sig = self.clone();
        PropSubscription::new(move || {
            auralis_signal::unsubscribe(&sig, id);
        })
    }
}

// ── Impl for std::rc::Rc<T> referenced values ──

impl<T: Clone + 'static> Prop<T> for std::rc::Rc<T> {
    fn read(&self) -> T {
        (**self).clone()
    }

    fn on_change(&self, _callback: Box<dyn Fn()>) -> PropSubscription {
        PropSubscription::noop()
    }
}

// ── Impl for &str → String convenience ──

impl Prop<String> for &str {
    fn read(&self) -> String {
        self.to_string()
    }

    fn on_change(&self, _callback: Box<dyn Fn()>) -> PropSubscription {
        PropSubscription::noop()
    }
}

/// Returns a future that resolves when the prop changes.
///
/// Useful in async contexts inside `TaskScope::spawn`:
///
/// ```ignore
/// scope.spawn(async move {
///     loop {
///         text_prop.until_change().await;
///         // update the UI...
///     }
/// });
/// ```
pub fn until_change<T: Clone + 'static>(prop: &impl Prop<T>) -> PropChangeFuture<T> {
    PropChangeFuture {
        prop_value: Some(prop.read()),
        _subscription: None,
        waker: Rc::new(RefCell::new(None)),
    }
}

/// A future that resolves when a `Prop` value changes.
pub struct PropChangeFuture<T: Clone + 'static> {
    prop_value: Option<T>,
    _subscription: Option<PropSubscription>,
    #[allow(dead_code)]
    waker: Rc<RefCell<Option<std::task::Waker>>>,
}

impl<T: Clone + 'static + std::marker::Unpin> std::future::Future for PropChangeFuture<T> {
    type Output = T;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(val) = this.prop_value.take() {
            Poll::Ready(val)
        } else {
            Poll::Pending
        }
    }
}
