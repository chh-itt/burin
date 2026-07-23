//! Event registry: maps ElementIds to event handlers.

use crate::core::config::EventHandler;
use crate::core::error::{panic_to_string, push_error, UiError};
use crate::core::ElementId;
use crate::event::action::{Action, ActionOutcome};
use crate::event::types::{GesturePhase, Key, Modifiers};
use crate::event::FocusReason;
use crate::style::Point;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

type Callback = Box<dyn Fn()>;
type FocusCallback = Box<dyn Fn(FocusReason)>;
type PositionedClick = Box<dyn FnMut(Point)>;
type ClickWithModsCallback = Box<dyn FnMut(Modifiers)>;
type ClickAtWithModsCallback = Box<dyn FnMut(Point, Modifiers)>;
type DragUpdateHandler = Box<dyn FnMut(Point, Point)>;
type KeyCallback = Box<dyn FnMut(Key, Modifiers) -> bool>;
type ResizeCallback = Box<dyn FnMut(f32, f32)>;
type ScrollCallback = Box<dyn FnMut(f32, f32) -> bool>;
type ActionCallback = Box<dyn FnMut(&Action) -> ActionOutcome>;

/// Consolidates the 7 identically-typed `Callback` handler maps into one.
#[derive(Default)]
struct BasicHandlers {
    click: Vec<Callback>,
    hover_enter: Vec<Callback>,
    hover_leave: Vec<Callback>,
    focus_in: Vec<FocusCallback>,
    focus_out: Vec<FocusCallback>,
    double_click: Vec<Callback>,
    triple_click: Vec<Callback>,
    long_press: Vec<Callback>,
}

/// How an element's drag handlers participate in gesture arbitration.
///
/// - `Eager` (default): drag_start fires at PointerDown and updates flow
///   from the first pixel — the historical "press = grab" feel that
///   Slider / text selection / ColorPicker / SplitPane depend on. Trades
///   tap-vs-drag disambiguation for instant response.
/// - `Threshold`: drag events are gated until the gesture arena's
///   DragRecognizer wins (6px) — a clean tap fires NO drag events.
///   For reorderable rows and other click-or-drag surfaces.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DragArbitration {
    #[default]
    Eager,
    Threshold,
}

pub struct EventRegistry {
    basic: HashMap<ElementId, BasicHandlers>,
    click_at_handlers: HashMap<ElementId, Vec<PositionedClick>>,
    click_with_mods_handlers: HashMap<ElementId, Vec<ClickWithModsCallback>>,
    click_at_with_mods_handlers: HashMap<ElementId, Vec<ClickAtWithModsCallback>>,
    drag_update_handlers: HashMap<ElementId, Vec<DragUpdateHandler>>,
    drag_start_handlers: HashMap<ElementId, Vec<DragUpdateHandler>>,
    drag_end_handlers: HashMap<ElementId, Vec<DragUpdateHandler>>,
    drag_arbitration: HashMap<ElementId, DragArbitration>,
    key_down_handlers: HashMap<ElementId, Vec<KeyCallback>>,
    key_up_handlers: HashMap<ElementId, Vec<KeyCallback>>,
    resize_handlers: HashMap<ElementId, Vec<ResizeCallback>>,
    scroll_handlers: HashMap<ElementId, Vec<ScrollCallback>>,
    pinch_handlers: HashMap<ElementId, Vec<Box<dyn FnMut(f64, GesturePhase) -> bool>>>,
    rotate_handlers: HashMap<ElementId, Vec<Box<dyn FnMut(f32, GesturePhase) -> bool>>>,
    text_input_handlers: HashMap<ElementId, Box<dyn FnMut(char)>>,
    preedit_handlers: HashMap<ElementId, Box<dyn FnMut(String, Option<(usize, usize)>)>>,
    preedit_data: HashMap<ElementId, Rc<RefCell<String>>>,
    ime_commit_handlers: HashMap<ElementId, Box<dyn FnMut(String)>>,
    ime_delete_handlers: HashMap<ElementId, Box<dyn FnMut(usize, usize)>>,
    clipboard_copy_handlers: HashMap<ElementId, Box<dyn Fn() -> String>>,
    clipboard_paste_handlers: HashMap<ElementId, Box<dyn FnMut(String)>>,
    autofocus_requests: Vec<ElementId>,
    action_handlers: HashMap<ElementId, Vec<ActionCallback>>,
    ime_suppressed: HashSet<ElementId>,
}

/// Fire a handler with panic isolation. All `fire_*` methods delegate to this.
#[inline]
fn safe_fire(context: &str, id: ElementId, f: impl FnOnce()) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    if let Err(panic) = result {
        push_error(UiError::CallbackPanic {
            context: context.into(),
            window_id: None,
            element_id: Some(id),
            message: panic_to_string(&panic),
        });
    }
}

impl EventRegistry {
    pub fn new() -> Self {
        Self {
            basic: HashMap::new(),
            click_at_handlers: HashMap::new(),
            click_with_mods_handlers: HashMap::new(),
            click_at_with_mods_handlers: HashMap::new(),
            drag_update_handlers: HashMap::new(),
            drag_start_handlers: HashMap::new(),
            drag_end_handlers: HashMap::new(),
            drag_arbitration: HashMap::new(),
            key_down_handlers: HashMap::new(),
            key_up_handlers: HashMap::new(),
            resize_handlers: HashMap::new(),
            scroll_handlers: HashMap::new(),
            pinch_handlers: HashMap::new(),
            rotate_handlers: HashMap::new(),
            text_input_handlers: HashMap::new(),
            preedit_handlers: HashMap::new(),
            preedit_data: HashMap::new(),
            ime_commit_handlers: HashMap::new(),
            ime_delete_handlers: HashMap::new(),
            clipboard_copy_handlers: HashMap::new(),
            clipboard_paste_handlers: HashMap::new(),
            autofocus_requests: Vec::new(),
            action_handlers: HashMap::new(),
            ime_suppressed: HashSet::new(),
        }
    }

    /// Remove every handler registered for `id` (audit 2026-07-16, F1).
    ///
    /// Called by the frame driver when draining element-teardown queues —
    /// handler closures capture `Rc`s into widget state, so leaving them
    /// behind after element removal leaks that state and keeps firing
    /// ghost callbacks for dead `ElementId`s.
    pub fn remove_element(&mut self, id: ElementId) {
        self.basic.remove(&id);
        self.click_at_handlers.remove(&id);
        self.click_with_mods_handlers.remove(&id);
        self.click_at_with_mods_handlers.remove(&id);
        self.drag_update_handlers.remove(&id);
        self.drag_start_handlers.remove(&id);
        self.drag_end_handlers.remove(&id);
        self.drag_arbitration.remove(&id);
        self.key_down_handlers.remove(&id);
        self.key_up_handlers.remove(&id);
        self.resize_handlers.remove(&id);
        self.scroll_handlers.remove(&id);
        self.pinch_handlers.remove(&id);
        self.rotate_handlers.remove(&id);
        self.text_input_handlers.remove(&id);
        self.preedit_handlers.remove(&id);
        self.preedit_data.remove(&id);
        self.ime_commit_handlers.remove(&id);
        self.ime_delete_handlers.remove(&id);
        self.clipboard_copy_handlers.remove(&id);
        self.clipboard_paste_handlers.remove(&id);
        self.autofocus_requests.retain(|&e| e != id);
        self.action_handlers.remove(&id);
        self.ime_suppressed.remove(&id);
    }

    pub fn on_click(&mut self, id: ElementId, handler: impl Fn() + 'static) {
        self.basic
            .entry(id)
            .or_default()
            .click
            .push(Box::new(handler));
    }

    pub fn on_hover_enter(&mut self, id: ElementId, handler: impl Fn() + 'static) {
        self.basic
            .entry(id)
            .or_default()
            .hover_enter
            .push(Box::new(handler));
    }

    pub fn on_hover_leave(&mut self, id: ElementId, handler: impl Fn() + 'static) {
        self.basic
            .entry(id)
            .or_default()
            .hover_leave
            .push(Box::new(handler));
    }

    pub fn on_focus_in(&mut self, id: ElementId, handler: impl Fn(FocusReason) + 'static) {
        self.basic
            .entry(id)
            .or_default()
            .focus_in
            .push(Box::new(handler));
    }

    pub fn on_focus_out(&mut self, id: ElementId, handler: impl Fn(FocusReason) + 'static) {
        self.basic
            .entry(id)
            .or_default()
            .focus_out
            .push(Box::new(handler));
    }

    pub fn has_handlers(&self, id: ElementId) -> bool {
        self.basic.get(&id).is_some_and(|b| {
            !b.click.is_empty()
                || !b.double_click.is_empty()
                || !b.triple_click.is_empty()
                || !b.long_press.is_empty()
        }) || self
            .click_with_mods_handlers
            .get(&id)
            .is_some_and(|v| !v.is_empty())
            || self
                .click_at_handlers
                .get(&id)
                .is_some_and(|v| !v.is_empty())
            || self
                .click_at_with_mods_handlers
                .get(&id)
                .is_some_and(|v| !v.is_empty())
    }

    pub fn fire_click(&mut self, id: ElementId) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.click {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h();
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_click".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    pub fn on_click_at(&mut self, id: ElementId, handler: impl FnMut(Point) + 'static) {
        self.click_at_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }

    pub fn fire_click_at(&mut self, id: ElementId, position: Point) {
        if let Some(handlers) = self.click_at_handlers.get_mut(&id) {
            for h in handlers {
                safe_fire("fire_click_at", id, || h(position));
            }
        }
    }

    pub fn on_click_with_mods(&mut self, id: ElementId, handler: impl FnMut(Modifiers) + 'static) {
        self.click_with_mods_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }

    pub fn fire_click_with_mods(&mut self, id: ElementId, mods: Modifiers) {
        if let Some(handlers) = self.click_with_mods_handlers.get_mut(&id) {
            for h in handlers {
                safe_fire("fire_click_with_mods", id, || h(mods));
            }
        }
    }

    pub fn on_click_at_with_mods(
        &mut self,
        id: ElementId,
        handler: impl FnMut(Point, Modifiers) + 'static,
    ) {
        self.click_at_with_mods_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }

    pub fn fire_click_at_with_mods(&mut self, id: ElementId, position: Point, mods: Modifiers) {
        if let Some(handlers) = self.click_at_with_mods_handlers.get_mut(&id) {
            for h in handlers {
                safe_fire("fire_click_at_with_mods", id, || h(position, mods));
            }
        }
    }

    pub fn has_click_at(&self, id: ElementId) -> bool {
        self.click_at_handlers
            .get(&id)
            .is_some_and(|v| !v.is_empty())
            || self
                .click_at_with_mods_handlers
                .get(&id)
                .is_some_and(|v| !v.is_empty())
    }

    /// Ensure this element competes in the gesture arena with a Drag-kind
    /// recognizer matching its arbitration mode. Called by every drag
    /// handler registration (declarative EventHandler AND imperative
    /// `register_drag_*` widget paths both land here — audit 2026-07-19).
    fn ensure_drag_recognizer(&mut self, id: ElementId) {
        use crate::event::recognizer::{self, RecognizerKind};
        if recognizer::has_recognizer_kind(id, RecognizerKind::Drag) {
            return;
        }
        let mode = self.drag_arbitration.get(&id).copied().unwrap_or_default();
        let rec: Box<dyn crate::event::recognizer::Recognizer> = match mode {
            DragArbitration::Eager => Box::new(recognizer::EagerDragRecognizer::new()),
            DragArbitration::Threshold => Box::new(recognizer::DragRecognizer::new()),
        };
        recognizer::register_recognizer(id, 100, RecognizerKind::Drag, rec, None);
    }

    /// Set how this element's drag handlers arbitrate against taps.
    /// Re-registers the arena recognizer when the mode changes after the
    /// drag handlers were installed.
    pub fn set_drag_arbitration(&mut self, id: ElementId, mode: DragArbitration) {
        use crate::event::recognizer::{self, RecognizerKind};
        let prev = self.drag_arbitration.insert(id, mode);
        if prev != Some(mode) && recognizer::has_recognizer_kind(id, RecognizerKind::Drag) {
            recognizer::unregister_recognizer_kind(id, RecognizerKind::Drag);
            self.ensure_drag_recognizer(id);
        }
    }

    pub fn register_drag_update(&mut self, id: ElementId, handler: Box<dyn FnMut(Point, Point)>) {
        self.ensure_drag_recognizer(id);
        self.drag_update_handlers
            .entry(id)
            .or_default()
            .push(handler);
    }

    pub fn fire_drag_update(&mut self, id: ElementId, local: Point, absolute: Point) {
        if let Some(handlers) = self.drag_update_handlers.get_mut(&id) {
            for h in handlers {
                safe_fire("fire_drag_update", id, || h(local, absolute));
            }
        }
    }

    pub fn has_drag_update(&self, id: ElementId) -> bool {
        self.drag_update_handlers
            .get(&id)
            .is_some_and(|v| !v.is_empty())
    }

    pub fn register_drag_start(&mut self, id: ElementId, handler: Box<dyn FnMut(Point, Point)>) {
        // Fired when this element's drag wins the arena (eager: at
        // PointerDown; threshold: at the 6px verdict). (local, absolute)
        self.ensure_drag_recognizer(id);
        self.drag_start_handlers
            .entry(id)
            .or_default()
            .push(handler);
    }

    pub fn fire_drag_start(&mut self, id: ElementId, local: Point, absolute: Point) {
        if let Some(handlers) = self.drag_start_handlers.get_mut(&id) {
            for h in handlers {
                safe_fire("fire_drag_start", id, || h(local, absolute));
            }
        }
    }

    pub fn has_drag_start(&self, id: ElementId) -> bool {
        self.drag_start_handlers
            .get(&id)
            .is_some_and(|v| !v.is_empty())
    }

    pub fn register_drag_end(&mut self, id: ElementId, handler: Box<dyn FnMut(Point, Point)>) {
        // Fired at PointerUp on the element holding the drag capture. (local, absolute)
        self.ensure_drag_recognizer(id);
        self.drag_end_handlers.entry(id).or_default().push(handler);
    }

    pub fn fire_drag_end(&mut self, id: ElementId, local: Point, absolute: Point) {
        if let Some(handlers) = self.drag_end_handlers.get_mut(&id) {
            for h in handlers {
                safe_fire("fire_drag_end", id, || h(local, absolute));
            }
        }
    }

    pub fn has_drag_end(&self, id: ElementId) -> bool {
        self.drag_end_handlers
            .get(&id)
            .is_some_and(|v| !v.is_empty())
    }

    pub fn fire_hover_enter(&mut self, id: ElementId) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.hover_enter {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h();
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_hover_enter".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    pub fn fire_hover_leave(&mut self, id: ElementId) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.hover_leave {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h();
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_hover_leave".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    pub fn fire_focus_in(&mut self, id: ElementId, reason: FocusReason) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.focus_in {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h(reason);
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_focus_in".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    pub fn fire_focus_out(&mut self, id: ElementId, reason: FocusReason) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.focus_out {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h(reason);
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_focus_out".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    pub fn register_text_input(&mut self, id: ElementId, handler: Box<dyn FnMut(char)>) {
        self.text_input_handlers.insert(id, handler);
    }

    pub fn fire_text_input(&mut self, id: ElementId, c: char) {
        if let Some(handler) = self.text_input_handlers.get_mut(&id) {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler(c);
            }));
            if let Err(panic) = result {
                push_error(UiError::CallbackPanic {
                    context: "fire_text_input".into(),
                    window_id: None,
                    element_id: Some(id),
                    message: panic_to_string(&panic),
                });
            }
        }
    }

    pub fn has_text_input(&self, id: ElementId) -> bool {
        self.text_input_handlers.contains_key(&id)
    }

    pub fn set_ime_suppressed(&mut self, id: ElementId, suppressed: bool) {
        if suppressed {
            self.ime_suppressed.insert(id);
        } else {
            self.ime_suppressed.remove(&id);
        }
    }

    pub fn is_ime_suppressed(&self, id: ElementId) -> bool {
        self.ime_suppressed.contains(&id)
    }

    pub fn register_preedit(
        &mut self,
        id: ElementId,
        handler: Box<dyn FnMut(String, Option<(usize, usize)>)>,
        buffer: Rc<RefCell<String>>,
    ) {
        self.preedit_data.insert(id, buffer);
        self.preedit_handlers.insert(id, handler);
    }

    /// Register a handler for IME `DeleteSurrounding` (audit 2026-07-17
    /// round 5, C5): `(before_bytes, after_bytes)` around the cursor.
    pub fn register_ime_delete(&mut self, id: ElementId, handler: Box<dyn FnMut(usize, usize)>) {
        self.ime_delete_handlers.insert(id, handler);
    }

    pub fn fire_ime_delete_surrounding(
        &mut self,
        id: ElementId,
        before_bytes: usize,
        after_bytes: usize,
    ) {
        if let Some(handler) = self.ime_delete_handlers.get_mut(&id) {
            safe_fire("fire_ime_delete_surrounding", id, || {
                handler(before_bytes, after_bytes)
            });
        }
    }

    pub fn fire_ime_preedit(
        &mut self,
        id: ElementId,
        text: String,
        cursor_range: Option<(usize, usize)>,
    ) {
        // Mirror the live preedit into the per-element buffer so
        // `commit_preedit` (focus-transfer flush) sees the pending text.
        if let Some(buf) = self.preedit_data.get(&id) {
            buf.borrow_mut().clone_from(&text);
        }
        if let Some(handler) = self.preedit_handlers.get_mut(&id) {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler(text, cursor_range);
            }));
            if let Err(panic) = result {
                push_error(UiError::CallbackPanic {
                    context: "fire_ime_preedit".into(),
                    window_id: None,
                    element_id: Some(id),
                    message: panic_to_string(&panic),
                });
            }
        }
    }

    /// Register a handler for atomic IME commit: the full committed string
    /// lands as ONE edit (one undo entry, one dirty pass) instead of a
    /// per-char `fire_text_input` storm.
    pub fn register_ime_commit(&mut self, id: ElementId, handler: Box<dyn FnMut(String)>) {
        self.ime_commit_handlers.insert(id, handler);
    }

    pub fn has_ime_commit(&self, id: ElementId) -> bool {
        self.ime_commit_handlers.contains_key(&id)
    }

    /// Fire the atomic IME commit handler. Returns `false` when the element
    /// has no handler — the caller must fall back to per-char `fire_text_input`.
    pub fn fire_ime_commit(&mut self, id: ElementId, text: String) -> bool {
        if let Some(handler) = self.ime_commit_handlers.get_mut(&id) {
            safe_fire("fire_ime_commit", id, || handler(text));
            true
        } else {
            false
        }
    }

    /// Flush in-progress IME preedit into the TextInput element.
    ///
    /// Called **only on focus transfer** — when the user clicks a different
    /// TextInput while an IME composition is active. On focus loss (clicking
    /// outside), the preedit is intentionally NOT committed; the user hasn't
    /// confirmed the composition. The OS IME retains its state, and the text
    /// appears when the user re-focuses and explicitly commits.
    pub fn commit_preedit(&mut self, id: ElementId) {
        let pending = self
            .preedit_data
            .get(&id)
            .map(|b| b.borrow().clone())
            .unwrap_or_default();
        if !pending.is_empty() {
            if self.has_ime_commit(id) {
                self.fire_ime_commit(id, pending);
            } else {
                for ch in pending.chars() {
                    if let Some(handler) = self.text_input_handlers.get_mut(&id) {
                        safe_fire("commit_preedit", id, || handler(ch));
                    }
                }
            }
        }
        self.fire_ime_preedit(id, String::new(), None);
    }

    pub fn register_clipboard_copy(&mut self, id: ElementId, handler: Box<dyn Fn() -> String>) {
        self.clipboard_copy_handlers.insert(id, handler);
    }

    pub fn fire_clipboard_copy(&mut self, id: ElementId) -> Option<String> {
        if let Some(h) = self.clipboard_copy_handlers.get(&id) {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(h)) {
                Ok(s) => Some(s),
                Err(panic) => {
                    push_error(UiError::CallbackPanic {
                        context: "fire_clipboard_copy".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn has_clipboard_copy(&self, id: ElementId) -> bool {
        self.clipboard_copy_handlers.contains_key(&id)
    }

    pub fn register_clipboard_paste(&mut self, id: ElementId, handler: Box<dyn FnMut(String)>) {
        self.clipboard_paste_handlers.insert(id, handler);
    }

    pub fn fire_clipboard_paste(&mut self, id: ElementId, text: String) {
        if let Some(h) = self.clipboard_paste_handlers.get_mut(&id) {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                h(text);
            }));
            if let Err(panic) = result {
                push_error(UiError::CallbackPanic {
                    context: "fire_clipboard_paste".into(),
                    window_id: None,
                    element_id: Some(id),
                    message: panic_to_string(&panic),
                });
            }
        }
    }

    pub fn has_clipboard_paste(&self, id: ElementId) -> bool {
        self.clipboard_paste_handlers.contains_key(&id)
    }

    pub(crate) fn request_autofocus(&mut self, id: ElementId) {
        self.autofocus_requests.push(id);
    }

    pub fn drain_autofocus(&mut self) -> Vec<ElementId> {
        std::mem::take(&mut self.autofocus_requests)
    }

    /// Public entry point for registering declarative event handlers.
    /// Used by integration tests to bypass the internal `EventHandler::register_all`.
    pub fn register_events(&mut self, id: ElementId, handler: EventHandler) {
        handler.register_all(self, id);
    }

    // ── Double click ──
    pub fn on_double_click(&mut self, id: ElementId, handler: impl Fn() + 'static) {
        self.basic
            .entry(id)
            .or_default()
            .double_click
            .push(Box::new(handler));
    }
    pub fn fire_double_click(&mut self, id: ElementId) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.double_click {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h();
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_double_click".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    // ── Triple click ──
    pub fn on_triple_click(&mut self, id: ElementId, handler: impl Fn() + 'static) {
        self.basic
            .entry(id)
            .or_default()
            .triple_click
            .push(Box::new(handler));
    }
    pub fn fire_triple_click(&mut self, id: ElementId) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.triple_click {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h();
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_triple_click".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    // ── Long press ──
    pub fn on_long_press(&mut self, id: ElementId, handler: impl Fn() + 'static) {
        // Sink registration: entering the gesture arena happens HERE, so
        // both the declarative (EventHandler) and any imperative path get
        // a LongPressRecognizer (audit 2026-07-19 — this lived in
        // config.rs apply() only, and the single-slot registry meant
        // drag + long-press overwrote each other).
        use crate::event::recognizer::{self, RecognizerKind};
        if !recognizer::has_recognizer_kind(id, RecognizerKind::LongPress) {
            recognizer::register_recognizer(
                id,
                50, // below drag: a moving pointer is a drag, not a hold
                RecognizerKind::LongPress,
                Box::new(recognizer::LongPressRecognizer::new()),
                Some(Box::new(move |eid, _phase| {
                    recognizer::push_long_press_win(eid);
                })),
            );
        }
        self.basic
            .entry(id)
            .or_default()
            .long_press
            .push(Box::new(handler));
    }
    pub fn fire_long_press(&mut self, id: ElementId) {
        if let Some(basic) = self.basic.get(&id) {
            for h in &basic.long_press {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    h();
                }));
                if let Err(panic) = result {
                    push_error(UiError::CallbackPanic {
                        context: "fire_long_press".into(),
                        window_id: None,
                        element_id: Some(id),
                        message: panic_to_string(&panic),
                    });
                }
            }
        }
    }

    // ── Key down / up ──
    pub fn on_key_down(
        &mut self,
        id: ElementId,
        handler: impl FnMut(Key, Modifiers) -> bool + 'static,
    ) {
        self.key_down_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }
    pub fn fire_key_down(&mut self, id: ElementId, key: Key, mods: Modifiers) -> bool {
        if let Some(handlers) = self.key_down_handlers.get_mut(&id) {
            for h in handlers {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h(key.clone(), mods)));
                match result {
                    Ok(true) => return true,
                    Ok(false) => {}
                    Err(panic) => {
                        push_error(UiError::CallbackPanic {
                            context: "fire_key_down".into(),
                            window_id: None,
                            element_id: Some(id),
                            message: panic_to_string(&panic),
                        });
                    }
                }
            }
        }
        false
    }
    pub fn on_key_up(
        &mut self,
        id: ElementId,
        handler: impl FnMut(Key, Modifiers) -> bool + 'static,
    ) {
        self.key_up_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }
    pub fn fire_key_up(&mut self, id: ElementId, key: Key, mods: Modifiers) -> bool {
        if let Some(handlers) = self.key_up_handlers.get_mut(&id) {
            for h in handlers {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h(key.clone(), mods)));
                match result {
                    Ok(true) => return true,
                    Ok(false) => {}
                    Err(panic) => {
                        push_error(UiError::CallbackPanic {
                            context: "fire_key_up".into(),
                            window_id: None,
                            element_id: Some(id),
                            message: panic_to_string(&panic),
                        });
                    }
                }
            }
        }
        false
    }

    // ── Resize ──
    pub fn on_resize(&mut self, id: ElementId, handler: impl FnMut(f32, f32) + 'static) {
        self.resize_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }
    pub fn fire_resize(&mut self, id: ElementId, width: f32, height: f32) {
        if let Some(handlers) = self.resize_handlers.get_mut(&id) {
            for h in handlers {
                safe_fire("fire_resize", id, || h(width, height));
            }
        }
    }

    // ── Scroll ──
    pub fn on_scroll(&mut self, id: ElementId, handler: impl FnMut(f32, f32) -> bool + 'static) {
        self.scroll_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }
    pub fn fire_scroll(&mut self, id: ElementId, dx: f32, dy: f32) -> bool {
        let mut handled = false;
        if let Some(handlers) = self.scroll_handlers.get_mut(&id) {
            for h in handlers {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h(dx, dy)));
                match result {
                    Ok(true) => handled = true,
                    Ok(false) => {}
                    Err(panic) => {
                        push_error(UiError::CallbackPanic {
                            context: "fire_scroll".into(),
                            window_id: None,
                            element_id: Some(id),
                            message: panic_to_string(&panic),
                        });
                    }
                }
            }
        }
        handled
    }

    // ── Pinch / Rotate ──

    pub fn on_pinch(
        &mut self,
        id: ElementId,
        handler: impl FnMut(f64, GesturePhase) -> bool + 'static,
    ) {
        self.pinch_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }
    pub fn fire_pinch(&mut self, id: ElementId, delta: f64, phase: GesturePhase) -> bool {
        if let Some(handlers) = self.pinch_handlers.get_mut(&id) {
            for h in handlers {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h(delta, phase)));
                match result {
                    Ok(true) => return true,
                    Ok(false) => {}
                    Err(panic) => {
                        push_error(UiError::CallbackPanic {
                            context: "fire_pinch".into(),
                            window_id: None,
                            element_id: Some(id),
                            message: panic_to_string(&panic),
                        });
                    }
                }
            }
        }
        false
    }

    pub fn on_rotate(
        &mut self,
        id: ElementId,
        handler: impl FnMut(f32, GesturePhase) -> bool + 'static,
    ) {
        self.rotate_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }
    pub fn fire_rotate(&mut self, id: ElementId, delta: f32, phase: GesturePhase) -> bool {
        if let Some(handlers) = self.rotate_handlers.get_mut(&id) {
            for h in handlers {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h(delta, phase)));
                match result {
                    Ok(true) => return true,
                    Ok(false) => {}
                    Err(panic) => {
                        push_error(UiError::CallbackPanic {
                            context: "fire_rotate".into(),
                            window_id: None,
                            element_id: Some(id),
                            message: panic_to_string(&panic),
                        });
                    }
                }
            }
        }
        false
    }

    // ── Action ──

    pub fn on_action(
        &mut self,
        id: ElementId,
        handler: impl FnMut(&Action) -> ActionOutcome + 'static,
    ) {
        self.action_handlers
            .entry(id)
            .or_default()
            .push(Box::new(handler));
    }

    pub fn fire_action(&mut self, id: ElementId, action: &Action) -> ActionOutcome {
        if let Some(handlers) = self.action_handlers.get_mut(&id) {
            for h in handlers {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h(action)));
                match result {
                    Ok(ActionOutcome::Blocked) => return ActionOutcome::Blocked,
                    Ok(ActionOutcome::Consumed) => return ActionOutcome::Consumed,
                    Ok(ActionOutcome::Unhandled) => {}
                    Err(panic) => {
                        push_error(UiError::CallbackPanic {
                            context: "fire_action".into(),
                            window_id: None,
                            element_id: Some(id),
                            message: panic_to_string(&panic),
                        });
                    }
                }
            }
        }
        ActionOutcome::Unhandled
    }
}

impl Default for EventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ElementId;
    use crate::style::Point;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn drag_start_and_end_fire_with_positions() {
        let mut reg = EventRegistry::new();
        let id = ElementId::allocate();

        let start_seen: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
        let end_seen: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));

        assert!(!reg.has_drag_start(id));
        assert!(!reg.has_drag_end(id));

        let s = start_seen.clone();
        reg.register_drag_start(
            id,
            Box::new(move |local, _abs| {
                s.set(Some((local.x, local.y)));
            }),
        );
        let e = end_seen.clone();
        reg.register_drag_end(
            id,
            Box::new(move |local, _abs| {
                e.set(Some((local.x, local.y)));
            }),
        );

        assert!(reg.has_drag_start(id));
        assert!(reg.has_drag_end(id));

        reg.fire_drag_start(id, Point::new(3.0, 4.0), Point::new(13.0, 14.0));
        reg.fire_drag_end(id, Point::new(5.0, 6.0), Point::new(15.0, 16.0));

        assert_eq!(start_seen.get(), Some((3.0, 4.0)));
        assert_eq!(end_seen.get(), Some((5.0, 6.0)));
    }

    #[test]
    fn has_handlers_detects_click_with_mods() {
        let mut reg = EventRegistry::new();
        let id = ElementId::allocate();

        assert!(!reg.has_handlers(id), "no handlers yet");

        let fired: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let f = fired.clone();
        reg.on_click_with_mods(id, move |_mods| {
            f.set(true);
        });

        assert!(
            reg.has_handlers(id),
            "has_handlers must return true after on_click_with_mods"
        );

        // Also verify fire_click_with_mods actually fires
        reg.fire_click_with_mods(id, Modifiers::NONE);
        assert!(fired.get(), "fire_click_with_mods must invoke the handler");
    }

    /// Audit 2026-07-17 round 5, A2: `on_click_at_with_mods` was a dead API —
    /// registrable but never fired, and invisible to `has_handlers` so an
    /// element with ONLY this handler shadowed ancestor click_at handlers.
    #[test]
    fn click_at_with_mods_is_visible_and_fires() {
        let mut reg = EventRegistry::new();
        let id = ElementId::allocate();

        assert!(!reg.has_handlers(id));
        assert!(!reg.has_click_at(id));

        let seen: Rc<Cell<Option<(f32, f32, bool)>>> = Rc::new(Cell::new(None));
        let s = seen.clone();
        reg.on_click_at_with_mods(id, move |pos, mods| {
            s.set(Some((pos.x, pos.y, mods.ctrl)));
        });

        assert!(
            reg.has_handlers(id),
            "has_handlers must see click_at_with_mods (A2 shadowing fix)"
        );
        assert!(
            reg.has_click_at(id),
            "has_click_at must see click_at_with_mods"
        );

        let mods = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        reg.fire_click_at_with_mods(id, Point::new(7.0, 8.0), mods);
        assert_eq!(
            seen.get(),
            Some((7.0, 8.0, true)),
            "fire_click_at_with_mods must invoke the handler with position + modifiers"
        );
    }
}
