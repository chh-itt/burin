//! Platform accessibility bridge: wraps [`accesskit`] platform adapters
//! directly without going through `accesskit_winit`.
//!
//! ## Platform support
//!
//! | Platform | Backend | Status |
//! |----------|---------|--------|
//! | Windows  | `accesskit_windows::SubclassingAdapter` | ✅ |
//! | macOS    | `accesskit_macos::SubclassingAdapter` | ✅ |
//! | Linux    | `accesskit_unix::Adapter` (AT-SPI) | ✅ |
//! | Android  | `accesskit_android::Adapter` | deferred |
//! | iOS      | `accesskit_ios::SubclassingAdapter` | deferred |
//! | Other    | Null adapter (no-op) | — |
//!
//! The bridge is created in `can_create_surfaces` (after the winit window
//! exists) and receives tree updates each frame. Screen-reader action
//! requests arrive via platform-specific callbacks and are queued for
//! execution on the next frame.

use std::sync::{Mutex, OnceLock};

use accesskit::{Action, ActionData, ActionRequest, TreeUpdate};

#[cfg(all(target_os = "macos", feature = "a11y-platform"))]
use burin_platform::accessibility::create_macos_adapter;

use crate::core::ElementId;

// ── Cross-thread action queue ──────────────────────────────────────

#[allow(dead_code)]
#[doc(hidden)]
pub enum A11yAction {
    Click(ElementId),
    Focus(ElementId),
    Blur(ElementId),
    ScrollIntoView(ElementId),
    Increment(ElementId),
    Decrement(ElementId),
    Expand(ElementId),
    Collapse(ElementId),
    ShowTooltip(ElementId),
    HideTooltip(ElementId),
    ScrollDown(ElementId),
    ScrollUp(ElementId),
    ScrollLeft(ElementId),
    ScrollRight(ElementId),
    SetScrollOffset {
        target: ElementId,
        x: f32,
        y: f32,
    },
    ScrollToPoint {
        target: ElementId,
        x: f32,
        y: f32,
    },
    SetValue {
        target: ElementId,
        value: f64,
    },
    ReplaceSelectedText {
        target: ElementId,
        text: String,
    },
    SetTextSelection {
        target: ElementId,
        start: usize,
        end: usize,
    },
    SetSequentialFocusStart(ElementId),
    CustomAction {
        target: ElementId,
        id: u32,
    },
}

static A11Y_QUEUE: OnceLock<Mutex<Vec<A11yAction>>> = OnceLock::new();

fn queue() -> &'static Mutex<Vec<A11yAction>> {
    A11Y_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Drain all pending accessibility actions (called from the UI thread in `on_frame`).
pub(crate) fn drain_a11y_actions() -> Vec<A11yAction> {
    let mut guard = queue().lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *guard)
}

/// Enqueue an accessibility action. Platform adapters push here from
/// their callback threads; tests use it to exercise the exact SEAM-2
/// dispatch path the production window runs.
#[doc(hidden)]
pub fn push_a11y_action(action: A11yAction) {
    let mut guard = queue().lock().unwrap_or_else(|e| e.into_inner());
    guard.push(action);
}

// ── SEAM-2 dispatch (shared by window and TestHarness) ─────────────
//
// Moved out of `WindowState::on_frame` (audit 2026-07-16 round 4): the
// dispatch is pure arena/registry/focus work — keeping it window-only
// meant screen-reader actions were untested and the harness diverged
// from production. Platform follow-ups (IME on focus) go through
// `FrameHook::on_focus_transferred`.

/// Marker user_data: numeric value set requested via accessibility.
#[allow(dead_code)]
pub(crate) struct A11ySetValue {
    pub value: f64,
}
/// Marker user_data: replace-selected-text requested via accessibility.
#[allow(dead_code)]
pub(crate) struct A11yReplaceText {
    pub text: String,
}
/// Marker user_data: text-selection change requested via accessibility.
#[allow(dead_code)]
pub(crate) struct A11yTextSelection {
    pub start: usize,
    pub end: usize,
}

/// Execute all queued accessibility actions against the frame state.
pub(crate) fn dispatch_a11y_actions(
    arena: &mut crate::core::element::ElementArena,
    events: &mut crate::event::EventRegistry,
    focus: &mut crate::event::FocusManager,
    hook: &mut dyn crate::core::frame_driver::FrameHook,
) {
    use crate::core::config::StateFlags;
    use crate::event::action::{Action, ActionKind};
    use crate::event::focus_manager::{scroll_focused_into_view, transfer_focus};
    use crate::event::FocusReason;

    let do_focus = |arena: &mut crate::core::element::ElementArena,
                    events: &mut crate::event::EventRegistry,
                    focus: &mut crate::event::FocusManager,
                    hook: &mut dyn crate::core::frame_driver::FrameHook,
                    eid: ElementId| {
        transfer_focus(arena, events, focus, eid, FocusReason::Programmatic);
        hook.on_focus_transferred(events, eid);
    };

    for action in drain_a11y_actions() {
        match action {
            A11yAction::Click(eid) => {
                events.fire_click(eid);
            }
            A11yAction::Focus(eid) => {
                do_focus(arena, events, focus, hook, eid);
            }
            A11yAction::Blur(eid) => {
                if Some(eid) == focus.focused() {
                    if let Some(el) = arena.get_mut(eid) {
                        el.set_state_dirty(StateFlags::FOCUSED, false);
                        el.last_focus_reason.set(Some(FocusReason::Programmatic));
                    }
                    events.fire_focus_out(eid, FocusReason::Programmatic);
                    focus.set_focused(None);
                }
            }
            A11yAction::ScrollIntoView(eid) => {
                scroll_focused_into_view(arena, eid);
            }
            A11yAction::Increment(eid) => {
                events.fire_action(eid, &Action::new(ActionKind::A11yIncrement));
            }
            A11yAction::Decrement(eid) => {
                events.fire_action(eid, &Action::new(ActionKind::A11yDecrement));
            }
            A11yAction::Expand(eid) => {
                events.fire_action(eid, &Action::new(ActionKind::A11yExpand));
            }
            A11yAction::Collapse(eid) => {
                events.fire_action(eid, &Action::new(ActionKind::A11yCollapse));
            }
            A11yAction::ShowTooltip(eid) => {
                if let Some(el) = arena.get(eid) {
                    if let Some(tv) = el.tooltip_visible() {
                        tv.set(true);
                    }
                    el.mark_repaint();
                }
            }
            A11yAction::HideTooltip(eid) => {
                if let Some(el) = arena.get(eid) {
                    if let Some(tv) = el.tooltip_visible() {
                        tv.set(false);
                    }
                    el.mark_repaint();
                }
            }
            // Scroll actions route through the SAME application path as
            // wheel scrolling (`do_scroll`: clamped to content bounds,
            // subtree-cache invalidated). Audit round 5 fix: the old
            // mapping was inverted on all four axes (ScrollDown moved the
            // viewport up, etc.) and never clamped to the max offset.
            A11yAction::ScrollDown(eid) => {
                crate::widgets::bundle::scroll::do_scroll(arena, eid, 0.0, -40.0);
            }
            A11yAction::ScrollUp(eid) => {
                crate::widgets::bundle::scroll::do_scroll(arena, eid, 0.0, 40.0);
            }
            A11yAction::ScrollLeft(eid) => {
                crate::widgets::bundle::scroll::do_scroll(arena, eid, 40.0, 0.0);
            }
            A11yAction::ScrollRight(eid) => {
                crate::widgets::bundle::scroll::do_scroll(arena, eid, -40.0, 0.0);
            }
            A11yAction::SetScrollOffset { target, x, y }
            | A11yAction::ScrollToPoint { target, x, y } => {
                crate::widgets::bundle::scroll::set_scroll_offset_clamped(arena, target, x, y);
            }
            A11yAction::SetValue { target, value } => {
                if let Some(el) = arena.get_mut(target) {
                    el.insert_user_data(A11ySetValue { value });
                    el.mark_repaint();
                }
            }
            A11yAction::ReplaceSelectedText { target, text } => {
                if let Some(el) = arena.get_mut(target) {
                    el.insert_user_data(A11yReplaceText { text });
                    el.mark_repaint();
                }
            }
            A11yAction::SetTextSelection { target, start, end } => {
                if let Some(el) = arena.get_mut(target) {
                    el.insert_user_data(A11yTextSelection { start, end });
                    el.mark_repaint();
                }
            }
            A11yAction::SetSequentialFocusStart(eid) => {
                do_focus(arena, events, focus, hook, eid);
            }
            A11yAction::CustomAction { target, id: _ } => {
                let action = Action::new(ActionKind::Activate);
                events.fire_action(target, &action);
            }
        }
    }
}

fn node_to_eid(node: accesskit::NodeId) -> ElementId {
    ElementId::from_u64(node.0)
}

// ── Bridge ─────────────────────────────────────────────────────────

/// Bridge to the platform accessibility API.
///
/// Constructed after the winit window is created; wraps a platform-specific
/// adapter.  On unsupported platforms this is a no-op.
pub struct A11yBridge {
    inner: Option<PlatformAdapter>,
    /// Snapshot of the most recent tree update, for inspection and testing.
    snapshot: Mutex<Option<TreeUpdate>>,
}

enum PlatformAdapter {
    #[cfg(target_os = "windows")]
    Windows {
        adapter: accesskit_windows::SubclassingAdapter,
    },
    #[cfg(all(target_os = "macos", feature = "a11y-platform"))]
    MacOS {
        adapter: accesskit_macos::SubclassingAdapter,
    },
    #[cfg(target_os = "linux")]
    Unix { adapter: accesskit_unix::Adapter },
}

impl A11yBridge {
    pub fn new() -> Self {
        Self {
            inner: None,
            snapshot: Mutex::new(None),
        }
    }

    /// Initialise the platform adapter. Must be called once per process.
    /// Subsequent calls are no-ops.
    pub fn init(&mut self, raw_handle: raw_window_handle::RawWindowHandle) {
        // accesskit_windows::SubclassingAdapter can only be created once.
        // We store initialized state in a thread-local so the first window
        // across all WindowState instances claims the adapter; subsequent
        // windows skip init.
        use std::cell::Cell;
        thread_local! {
            static A11Y_INITIALIZED: Cell<bool> = const { Cell::new(false) };
        }
        if A11Y_INITIALIZED.replace(true) {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::RawWindowHandle;
            if let RawWindowHandle::Win32(handle) = raw_handle {
                let hwnd = handle.hwnd.get() as *mut _;
                let adapter = accesskit_windows::SubclassingAdapter::new(
                    accesskit_windows::HWND(hwnd),
                    A11yActivationHandler,
                    A11yActionHandler,
                );
                self.inner = Some(PlatformAdapter::Windows { adapter });
                return;
            }
            #[cfg(any(feature = "devtools", feature = "file-logging"))]
            tracing::warn!("A11yBridge: unsupported raw-window-handle variant on Windows");
            return;
        }

        #[cfg(all(target_os = "macos", feature = "a11y-platform"))]
        {
            use raw_window_handle::RawWindowHandle;
            if let RawWindowHandle::AppKit(handle) = raw_handle {
                let view = handle.ns_view.as_ptr() as *mut std::ffi::c_void;
                let adapter = create_macos_adapter(view, A11yActivationHandler, A11yActionHandler);
                self.inner = Some(PlatformAdapter::MacOS { adapter });
                return;
            }
            #[cfg(any(feature = "devtools", feature = "file-logging"))]
            tracing::warn!("A11yBridge: unsupported raw-window-handle variant on macOS");
            return;
        }

        #[cfg(target_os = "linux")]
        {
            let _ = raw_handle;
            let adapter = accesskit_unix::Adapter::new(
                A11yActivationHandler,
                A11yActionHandler,
                A11yDeactivationHandler,
            );
            self.inner = Some(PlatformAdapter::Unix { adapter });
            return;
        }

        #[allow(unreachable_code)]
        {
            let _ = raw_handle;
        }
    }

    /// Send a tree update to the platform adapter (if active).
    ///
    /// The closure is only called when the screen reader is connected,
    /// avoiding unnecessary tree serialisation.
    pub fn update(&mut self, updater: impl FnOnce() -> TreeUpdate) {
        match &mut self.inner {
            #[cfg(target_os = "windows")]
            Some(PlatformAdapter::Windows { adapter }) => {
                let tree = updater();
                let _ = self.snapshot.lock().map(|mut s| *s = Some(tree.clone()));
                if let Some(events) = adapter.update_if_active(|| tree) {
                    events.raise();
                }
            }
            #[cfg(target_os = "macos")]
            Some(PlatformAdapter::MacOS { adapter }) => {
                let tree = updater();
                let _ = self.snapshot.lock().map(|mut s| *s = Some(tree.clone()));
                if let Some(events) = adapter.update_if_active(|| tree) {
                    events.raise();
                }
            }
            #[cfg(target_os = "linux")]
            Some(PlatformAdapter::Unix { adapter }) => {
                let tree = updater();
                let _ = self.snapshot.lock().map(|mut s| *s = Some(tree.clone()));
                adapter.update_if_active(|| tree);
            }
            _ => {
                let tree = updater();
                let _ = self.snapshot.lock().map(|mut s| *s = Some(tree));
            }
        }
    }

    /// The most recent tree snapshot (for testing / debugging).
    pub fn latest_tree_update(&self) -> Option<TreeUpdate> {
        self.snapshot.lock().ok()?.clone()
    }
}

impl Default for A11yBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ── Platform handlers ──────────────────────────────────────────────

/// Called by the platform adapter when a screen reader connects and
/// requests the initial accessibility tree.
struct A11yActivationHandler;

impl accesskit::ActivationHandler for A11yActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        None
    }
}

/// Called by the platform adapter when a screen reader performs an action
/// (click, focus, set value, etc.). Actions are queued and executed on the
/// next frame by the UI thread.
struct A11yActionHandler;

impl accesskit::ActionHandler for A11yActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        let eid = node_to_eid(request.target_node);
        let action = match request.action {
            Action::Click => A11yAction::Click(eid),
            Action::Focus => A11yAction::Focus(eid),
            Action::Blur => A11yAction::Blur(eid),
            Action::ScrollIntoView => A11yAction::ScrollIntoView(eid),
            Action::Increment => A11yAction::Increment(eid),
            Action::Decrement => A11yAction::Decrement(eid),
            Action::Expand => A11yAction::Expand(eid),
            Action::Collapse => A11yAction::Collapse(eid),
            Action::ShowTooltip => A11yAction::ShowTooltip(eid),
            Action::HideTooltip => A11yAction::HideTooltip(eid),
            Action::ScrollDown => A11yAction::ScrollDown(eid),
            Action::ScrollUp => A11yAction::ScrollUp(eid),
            Action::ScrollLeft => A11yAction::ScrollLeft(eid),
            Action::ScrollRight => A11yAction::ScrollRight(eid),
            Action::SetSequentialFocusNavigationStartingPoint => {
                A11yAction::SetSequentialFocusStart(eid)
            }
            Action::SetScrollOffset => {
                let (x, y) = request
                    .data
                    .and_then(|d| match d {
                        ActionData::SetScrollOffset(s) => Some((s.x as f32, s.y as f32)),
                        _ => None,
                    })
                    .unwrap_or((0.0, 0.0));
                A11yAction::SetScrollOffset { target: eid, x, y }
            }
            Action::ScrollToPoint => {
                let (x, y) = request
                    .data
                    .and_then(|d| match d {
                        ActionData::ScrollToPoint(s) => Some((s.x as f32, s.y as f32)),
                        _ => None,
                    })
                    .unwrap_or((0.0, 0.0));
                A11yAction::ScrollToPoint { target: eid, x, y }
            }
            Action::SetValue => {
                let value = request
                    .data
                    .and_then(|d| match d {
                        ActionData::Value(v) => v.parse::<f64>().ok(),
                        ActionData::NumericValue(v) => Some(v),
                        _ => None,
                    })
                    .unwrap_or(0.0);
                A11yAction::SetValue { target: eid, value }
            }
            Action::ReplaceSelectedText => {
                let text = request
                    .data
                    .and_then(|d| match d {
                        ActionData::Value(v) => Some(v.to_string()),
                        _ => None,
                    })
                    .unwrap_or_default();
                A11yAction::ReplaceSelectedText { target: eid, text }
            }
            Action::SetTextSelection => {
                let (start, end) = request
                    .data
                    .and_then(|d| match d {
                        ActionData::SetTextSelection(s) => {
                            Some((s.anchor.character_index, s.focus.character_index))
                        }
                        _ => None,
                    })
                    .unwrap_or((0, 0));
                A11yAction::SetTextSelection {
                    target: eid,
                    start,
                    end,
                }
            }
            Action::CustomAction => {
                let id = request
                    .data
                    .and_then(|d| match d {
                        ActionData::CustomAction(c) => Some(c),
                        _ => None,
                    })
                    .unwrap_or(0);
                A11yAction::CustomAction {
                    target: eid,
                    id: id as u32,
                }
            }
            _ => return,
        };
        if let Ok(mut guard) = queue().lock() {
            guard.push(action);
        }
    }
}

/// Linux AT-SPI: adapter deactivation handler (no-op).
#[cfg(target_os = "linux")]
struct A11yDeactivationHandler;

#[cfg(target_os = "linux")]
impl accesskit::DeactivationHandler for A11yDeactivationHandler {
    fn deactivate_accessibility(&mut self) {}
}
