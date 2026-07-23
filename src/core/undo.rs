use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use auralis_signal::Signal;
use web_time::Instant;

use crate::core::Element;

// ── Public configuration types ──

/// Configuration for undo behaviour.
#[derive(Clone, Debug)]
pub struct UndoConfig {
    /// Maximum number of undo entries (default: 100).
    pub max_depth: usize,
    /// How consecutive changes are merged into a single undo step.
    pub merge_policy: MergePolicy,
}

impl Default for UndoConfig {
    fn default() -> Self {
        Self {
            max_depth: 100,
            merge_policy: MergePolicy::default(),
        }
    }
}

/// Determines whether consecutive mutations collapse into one undo step.
#[derive(Clone, Debug)]
pub enum MergePolicy {
    /// Every `.set()` creates its own undo entry.
    None,
    /// Consecutive `.set()` calls within the time window replace the
    /// previous entry (i.e. they form a single undo step).
    TimeWindow(std::time::Duration),
    /// Text-input-friendly: configurable time window plus explicit
    /// boundary support (call [`UndoableSignal::push_boundary`] on
    /// focus loss to prevent merging across focus boundaries).
    TextInput(TextInputConfig),
}

impl Default for MergePolicy {
    fn default() -> Self {
        Self::TextInput(TextInputConfig::default())
    }
}

/// Configuration for text-input-style merging.
#[derive(Clone, Debug)]
pub struct TextInputConfig {
    /// Changes within this many milliseconds are merged (default: 400).
    pub merge_window_ms: u64,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            merge_window_ms: 400,
        }
    }
}

// ── Internal history state machine ──

struct UndoHistoryInternal<T> {
    stack: Vec<T>,
    idx: usize,
    max_depth: usize,
    merge_policy: MergePolicy,
    last_push: Option<Instant>,
    boundary_pending: bool,
}

impl<T: Clone + PartialEq + 'static> UndoHistoryInternal<T> {
    fn new(config: UndoConfig) -> Self {
        Self {
            stack: Vec::new(),
            idx: 0,
            max_depth: config.max_depth,
            merge_policy: config.merge_policy,
            last_push: None,
            boundary_pending: false,
        }
    }

    fn seed(&mut self, value: T) {
        self.stack.push(value);
        self.idx = 0;
    }

    fn push(&mut self, value: T) {
        let should_merge = match &self.merge_policy {
            MergePolicy::None => false,
            MergePolicy::TimeWindow(dur) => self.last_push.is_some_and(|t| t.elapsed() < *dur),
            MergePolicy::TextInput(cfg) => {
                if self.boundary_pending {
                    self.boundary_pending = false;
                    false
                } else {
                    self.last_push.is_some_and(|t| {
                        t.elapsed() < std::time::Duration::from_millis(cfg.merge_window_ms)
                    })
                }
            }
        };

        if should_merge && !self.stack.is_empty() {
            // Replace the top entry so the undo goes straight back to the
            // state before the burst, skipping all intermediate values.
            *self.stack.last_mut().unwrap() = value;
        } else {
            self.stack.truncate(self.idx + 1);
            self.stack.push(value);
            self.idx += 1;
        }

        self.last_push = Some(crate::core::clock::now());

        // Cap depth: keep at most max_depth+1 entries (current + max_depth past)
        while self.stack.len() > self.max_depth + 1 {
            self.stack.remove(0);
            self.idx = self.idx.saturating_sub(1);
        }
    }

    fn undo(&mut self) -> Option<T> {
        if self.idx > 0 {
            self.idx -= 1;
            self.last_push = None;
            Some(self.stack[self.idx].clone())
        } else {
            None
        }
    }

    fn redo(&mut self) -> Option<T> {
        if self.idx + 1 < self.stack.len() {
            self.idx += 1;
            self.last_push = None;
            Some(self.stack[self.idx].clone())
        } else {
            None
        }
    }

    fn push_boundary(&mut self) {
        self.boundary_pending = true;
    }
}

// ── Element-wide undo/redo dispatch (stored in user_data) ──

pub(crate) struct UndoEntry {
    pub undo: Box<dyn Fn() -> bool>,
    pub redo: Box<dyn Fn() -> bool>,
}

pub(crate) struct ElementUndoState {
    pub entries: Vec<UndoEntry>,
}

impl ElementUndoState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, undo: Box<dyn Fn() -> bool>, redo: Box<dyn Fn() -> bool>) {
        self.entries.push(UndoEntry { undo, redo });
    }

    pub fn undo_all(&self) -> bool {
        let mut handled = false;
        for entry in self.entries.iter().rev() {
            if (entry.undo)() {
                handled = true;
            }
        }
        handled
    }

    pub fn redo_all(&self) -> bool {
        let mut handled = false;
        for entry in self.entries.iter().rev() {
            if (entry.redo)() {
                handled = true;
            }
        }
        handled
    }
}

// ── UndoableSignal ──

/// A `Signal<T>` wrapper that records undo history on every `.set()`.
///
/// Created via [`enable_undo`].  All existing patterns (`read()`,
/// `subscribe()`, `bind_dirty()`) work transparently through `Deref`.
/// The only difference is that `.set()` now pushes the previous value
/// onto an undo stack so the framework can restore it on `Ctrl+Z`.
pub struct UndoableSignal<T> {
    inner: Signal<T>,
    history: Rc<RefCell<UndoHistoryInternal<T>>>,
}

impl<T: Clone + PartialEq + 'static> UndoableSignal<T> {
    /// Create a standalone `UndoableSignal` with the given signal and config.
    ///
    /// Unlike [`enable_undo`], this does NOT register undo/redo callbacks
    /// on an element — the caller must route `Undo`/`Redo` actions manually.
    /// Useful for testing or when you want to manage undo dispatch yourself.
    pub fn new(signal: Signal<T>, config: UndoConfig) -> Self {
        let history: Rc<RefCell<UndoHistoryInternal<T>>> =
            Rc::new(RefCell::new(UndoHistoryInternal::new(config)));
        {
            let mut h = history.borrow_mut();
            h.seed(signal.read_untracked());
        }
        Self {
            inner: signal,
            history,
        }
    }

    /// Restore the previous value from the undo stack.
    /// Returns `true` when something was undone.
    pub fn undo(&self) -> bool {
        let mut hist = self.history.borrow_mut();
        if let Some(old) = hist.undo() {
            drop(hist);
            self.inner.set(old);
            true
        } else {
            false
        }
    }

    /// Re-apply a previously undone value.
    /// Returns `true` when something was redone.
    pub fn redo(&self) -> bool {
        let mut hist = self.history.borrow_mut();
        if let Some(next) = hist.redo() {
            drop(hist);
            self.inner.set(next);
            true
        } else {
            false
        }
    }

    /// Prevent the next `.set()` from merging with the previous entry.
    /// Useful for marking focus boundaries in text input.
    pub fn push_boundary(&self) {
        self.history.borrow_mut().push_boundary();
    }

    /// Immutable reference to the inner signal (for `subscribe`, `bind_dirty`, etc.)
    pub fn inner(&self) -> &Signal<T> {
        &self.inner
    }
}

impl<T: Clone + PartialEq + 'static> UndoableSignal<T> {
    /// Set the value AND record the previous value in the undo history.
    ///
    /// This shadows `Signal::set` because inherent methods take priority
    /// over `Deref` — callers that go through `(&*signal).set(val)` will
    /// bypass undo recording (documented limitation).
    pub fn set(&self, value: T) {
        let old = self.inner.read_untracked();
        if old != value {
            self.history.borrow_mut().push(value.clone());
        }
        self.inner.set(value);
    }
}

impl<T> Deref for UndoableSignal<T> {
    type Target = Signal<T>;
    fn deref(&self) -> &Signal<T> {
        &self.inner
    }
}

impl<T: Clone> Clone for UndoableSignal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            history: self.history.clone(),
        }
    }
}

// ── Public API ──

/// Wrap `signal` with undo/redo tracking and attach it to `element`.
///
/// * The undo stack is seeded with the signal's current value.
/// * Undo/redo closures are registered in `element.user_data` so the
///   framework can route `Undo`/`Redo` actions automatically.
/// * The returned [`UndoableSignal`] can be cloned freely — all clones
///   share the same undo history.
pub fn enable_undo<T: Clone + PartialEq + 'static>(
    element: &mut Element,
    signal: Signal<T>,
    config: UndoConfig,
) -> UndoableSignal<T> {
    let history: Rc<RefCell<UndoHistoryInternal<T>>> =
        Rc::new(RefCell::new(UndoHistoryInternal::new(config.clone())));

    // Seed stack with the current value
    {
        let mut h = history.borrow_mut();
        h.seed(signal.read_untracked());
    }

    let undo_signal = UndoableSignal {
        inner: signal,
        history: history.clone(),
    };

    // Build undo/redo closures (type-erased)
    let hs = history.clone();
    let sig_undo = undo_signal.inner.clone();
    let undo_fn: Box<dyn Fn() -> bool> = Box::new(move || -> bool {
        let mut h = hs.borrow_mut();
        if let Some(old) = h.undo() {
            drop(h);
            sig_undo.set(old);
            true
        } else {
            false
        }
    });

    let hs2 = history;
    let sig_redo = undo_signal.inner.clone();
    let redo_fn: Box<dyn Fn() -> bool> = Box::new(move || -> bool {
        let mut h = hs2.borrow_mut();
        if let Some(next) = h.redo() {
            drop(h);
            sig_redo.set(next);
            true
        } else {
            false
        }
    });

    // Store in element.user_data — create or append
    if let Some(state) = element.get_user_data_mut::<ElementUndoState>() {
        state.push(undo_fn, redo_fn);
    } else {
        let mut state = ElementUndoState::new();
        state.push(undo_fn, redo_fn);
        element.insert_user_data(state);
    }

    undo_signal
}
