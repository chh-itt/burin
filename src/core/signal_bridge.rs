use std::cell::RefCell;
use std::rc::Rc;

use auralis_signal::subscription::{subscribe_to_dyn, SubscriptionHandle};
use auralis_signal::Signal;

use crate::core::dirty_registry;
use crate::core::element::{DirtyFlags, Element, ElementId};
use crate::style::Color;

// ── Subscription lifecycle (audit 2026-07-16, F1) ───────────────────
//
// Every binding created during mount stores its RAII handle on the
// element's `LifecycleComponent.subscriptions`. When the element is torn
// down (`ElementArena::remove` & friends), dropping the component drops
// the handles, which unsubscribes from the signals — no leaked callbacks,
// no ghost dirty writes for dead ElementIds.

/// Store a subscription handle on `eid`'s LifecycleComponent so it is
/// dropped (and unsubscribed) when the element is removed from the arena.
///
/// Widgets that subscribe to signals directly (outside the `bind_*`
/// helpers) should route their subscriptions through this so unmount
/// cleans them up.
pub fn store_subscription(eid: ElementId, handle: SubscriptionHandle) {
    crate::core::element::with_ct_mut(|ct| {
        ct.lc.entry(eid).or_default().subscriptions.push(handle);
    });
}

/// Subscribe `callback` to `signal` and tie the subscription's lifetime to
/// `eid`. Convenience wrapper over [`store_subscription`].
pub fn subscribe_owned<T: 'static>(
    eid: ElementId,
    signal: &Signal<T>,
    callback: impl Fn() + 'static,
) {
    store_subscription(eid, subscribe_to_dyn(signal, Box::new(callback)));
}

// ── Explicit bindings (still available for opt-in use) ─────────────

pub fn bind_dirty<T: Clone + 'static>(
    element: &Element,
    signal: &Signal<T>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> T {
    let dirty = element.dirty.clone();
    let eid = element.id();
    subscribe_owned(eid, signal, move || {
        dirty.set(dirty.get() | DirtyFlags::REPAINT);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, DirtyFlags::REPAINT);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

pub fn bind_dirty_reposition<T: Clone + 'static>(
    element: &Element,
    signal: &Signal<T>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> T {
    let dirty = element.dirty.clone();
    let eid = element.id();
    subscribe_owned(eid, signal, move || {
        dirty.set(dirty.get() | DirtyFlags::REPOSITION);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, DirtyFlags::REPOSITION);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

pub fn bind_dirty_measure<T: Clone + 'static>(
    element: &Element,
    signal: &Signal<T>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> T {
    let dirty = element.dirty.clone();
    let eid = element.id();
    subscribe_owned(eid, signal, move || {
        dirty.set(dirty.get() | DirtyFlags::MEASURE);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, DirtyFlags::MEASURE);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

// ── Zero-copy property bindings ─────────────────────────────────────

/// Bind a `Signal<String>` to element label text, with REPAINT on change.
/// Reads initial value, registers subscription.
#[allow(dead_code)]
pub(crate) fn bind_label(
    element_id: crate::core::ElementId,
    dirty: Rc<std::cell::Cell<DirtyFlags>>,
    signal: &Signal<String>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> String {
    let d = dirty.clone();
    let eid = element_id;
    subscribe_owned(eid, signal, move || {
        d.set(d.get() | DirtyFlags::REPAINT);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, DirtyFlags::REPAINT);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

/// Bind a `Signal<bool>` to element visibility, with REPAINT on change.
#[allow(dead_code)]
pub(crate) fn bind_visible(
    element_id: crate::core::ElementId,
    dirty: Rc<std::cell::Cell<DirtyFlags>>,
    signal: &Signal<bool>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> bool {
    let d = dirty.clone();
    let eid = element_id;
    subscribe_owned(eid, signal, move || {
        d.set(d.get() | DirtyFlags::REPAINT);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, DirtyFlags::REPAINT);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

/// Bind a `Signal<Color>` to element rendering, with surface-level REPAINT.
#[allow(dead_code)]
pub(crate) fn bind_color(
    element_id: crate::core::ElementId,
    dirty: Rc<std::cell::Cell<DirtyFlags>>,
    signal: &Signal<Color>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> Color {
    let d = dirty.clone();
    let eid = element_id;
    subscribe_owned(eid, signal, move || {
        d.set(d.get() | DirtyFlags::REPAINT);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, DirtyFlags::REPAINT);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

/// Bind a `Signal<f32>` to element rendering, with REPAINT on change.
#[allow(dead_code)]
pub(crate) fn bind_f32(
    element_id: crate::core::ElementId,
    dirty: Rc<std::cell::Cell<DirtyFlags>>,
    signal: &Signal<f32>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> f32 {
    let d = dirty.clone();
    let eid = element_id;
    subscribe_owned(eid, signal, move || {
        d.set(d.get() | DirtyFlags::REPAINT);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, DirtyFlags::REPAINT);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

/// Bind a `Signal<String>` lazily: signal callback only sets the label
/// Cell and marks dirty. The text buffer is rebuilt lazily at paint time,
/// avoiding per-callback buffer rebuild overhead.
/// Marks both REPAINT and MEASURE since text content changes can affect
/// the element's preferred size.
/// Returns the initial signal value.
pub(crate) fn bind_label_lazy(
    label_cell: Rc<std::cell::Cell<String>>,
    text_gen_cell: Rc<std::cell::Cell<u64>>,
    dirty: Rc<std::cell::Cell<DirtyFlags>>,
    eid: crate::core::ElementId,
    signal: &Signal<String>,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
    lazy_font_params: Option<Rc<crate::core::element::LazyFontParams>>,
    measured_width: Option<Rc<std::cell::Cell<f32>>>,
) -> String {
    let sig = signal.clone();
    subscribe_owned(eid, signal, move || {
        let old_gen = text_gen_cell.get();
        let new_val_str = {
            let v = sig.read();
            label_cell.set(v.clone());
            text_gen_cell.set(old_gen.wrapping_add(1));
            v
        };

        if let (Some(ref lfp), Some(ref mw)) = (&lazy_font_params, &measured_width) {
            let new_w = crate::render::text::measure_text_width(
                &new_val_str,
                lfp.font_size,
                lfp.font_weight,
                lfp.font_family.clone(),
            )
            .max(lfp.font_size * 2.0);
            let old_w = mw.get();
            if (old_w - new_w).abs() > 0.5 {
                mw.set(new_w);
                crate::core::element::with_ct_mut(|ct| {
                    ct.layout.entry(eid).or_default().preferred_width = Some(new_w);
                });
            }
        }

        vgen!(
            "[VGEN:LAZY_BIND] eid={:?} text_generation: {} -> {}",
            eid,
            old_gen,
            old_gen.wrapping_add(1),
        );
        let flags = DirtyFlags::REPAINT | DirtyFlags::MEASURE;
        dirty.set(dirty.get() | flags);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, flags);
            app.bump_subtree_gen(eid);
        }
    });
    signal.read()
}

// ── Implicit binding: auto-subscribe during mount ─────────────────

type CleanupList = Rc<RefCell<Vec<Box<dyn FnOnce()>>>>;

thread_local! {
    /// Pending cleanup list for the element currently being mounted.
    /// Set by `observe_element`, consumed by `apply_observed_subscriptions`.
    static PENDING_CLEANUPS: RefCell<Option<CleanupList>> = RefCell::new(None);

    /// Current dirty level for implicit signal reads.
    static IMPLICIT_DIRTY_LEVEL: RefCell<DirtyFlags> = const { RefCell::new(DirtyFlags::REPAINT) };
}

/// Set the dirty level for the next signal read within an implicit binding scope.
pub fn set_implicit_dirty(level: DirtyFlags) {
    IMPLICIT_DIRTY_LEVEL.with(|l| *l.borrow_mut() = level);
}

/// Begin observing an element. All `Signal::read()` calls within this scope
/// auto-subscribe the element for dirty marking.
///
/// Drop the returned guard to stop observing. Then call
/// [`apply_observed_subscriptions`] to store cleanup closures on the element.
///
/// ```ignore
/// fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> Element {
///     let mut element = Element::new();
///     let _obs = observe_element(&element);
///     let label = self.label_signal.as_ref().map(|s| s.read());
///     drop(_obs);
///     apply_observed_subscriptions(&mut element);
///     element
/// }
/// ```
pub fn observe_element<'e>(
    element: &'e Element,
    app: std::rc::Weak<crate::core::app_context::AppContext>,
) -> impl Drop + 'e {
    let dirty = element.dirty.clone();
    let eid = element.id();

    let dirty_cb: Rc<dyn Fn()> = Rc::new(move || {
        let level = IMPLICIT_DIRTY_LEVEL.with(|l| *l.borrow());
        dirty.set(dirty.get() | level);
        if let Some(app) = app.upgrade() {
            app.register_dirty(eid, level);
            app.bump_subtree_gen(eid);
        }
    });

    let cleanups: CleanupList = Rc::new(RefCell::new(Vec::new()));
    PENDING_CLEANUPS.with(|c| *c.borrow_mut() = Some(cleanups.clone()));

    let on_sub: Rc<dyn Fn(usize, Box<dyn FnOnce()>)> = Rc::new({
        let cleanups = cleanups.clone();
        move |_addr, cleanup| {
            cleanups.borrow_mut().push(cleanup);
        }
    });

    let guard = auralis_signal::install_observer(dirty_cb, on_sub);

    ObserveScope { _guard: guard }
}

struct ObserveScope {
    _guard: auralis_signal::ObserverGuard,
}

impl Drop for ObserveScope {
    fn drop(&mut self) {}
}

/// Store pending signal cleanup subscriptions on the element.
/// Must be called after the observe_element guard is dropped.
///
/// Cleanups are wrapped as [`SubscriptionHandle`]s on the element's
/// `LifecycleComponent`, so element teardown unsubscribes them (audit
/// 2026-07-16, F1 — previously they were parked as user_data and never
/// executed).
pub fn apply_observed_subscriptions(element: &mut Element) {
    PENDING_CLEANUPS.with(|c| {
        if let Some(cleanups) = c.borrow_mut().take() {
            let eid = element.id();
            let cleanups = std::mem::take(&mut *cleanups.borrow_mut());
            crate::core::element::with_ct_mut(|ct| {
                let lc = ct.lc.entry(eid).or_default();
                for cleanup in cleanups {
                    lc.subscriptions
                        .push(SubscriptionHandle::from_cleanup(cleanup));
                }
            });
        }
    });
}

/// Force a lazy label to re-shape at the next paint pass without changing
/// its logical text.  Use when the rendering context changes (container
/// width, font metrics, theme switch) but the text string stays the same.
///
/// Bumps `text_generation` and marks the element dirty.
pub(crate) fn force_refresh_label(eid: ElementId) {
    crate::core::element::with_ct_mut(|ct| {
        if let Some(tc) = ct.text.get_mut(&eid) {
            let old = tc.text_generation.get();
            tc.text_generation.set(old.wrapping_add(1));
            vgen!(
                "[VGEN:FORCE_REFRESH] eid={:?} text_generation: {} -> {}",
                eid,
                old,
                old.wrapping_add(1)
            );
        }
    });
    dirty_registry::mark_dirty(eid, DirtyFlags::REPAINT);
    dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
    dirty_registry::bump_subtree_gen(eid);
}

/// Read the `CURRENT_NOTIFYING_SIGNAL` thread-local from auralis-signal.
/// Returns 0 when no notification is in progress or the diagnostics
/// feature is not enabled.  Used by DevTools to attribute dirty registrations
/// to the signal whose subscriber callback is currently executing.
#[cfg(feature = "devtools")]
pub(crate) fn read_current_signal_addr() -> usize {
    auralis_signal::CURRENT_NOTIFYING_SIGNAL.with(|c| c.get())
}
