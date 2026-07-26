//! Test harness for driving the full frame lifecycle without a window.
//!
//! ```ignore
//! use burin::testing::TestHarness;
//! use burin::widgets::input::Button;
//! use burin::widgets::display::Text;
//! use burin::widgets::layout::VStack;
//!
//! let mut h = TestHarness::new(800.0, 600.0);
//! let id = h.mount(
//!     VStack::new()
//!         .push(Button::new("Click me").primary())
//!         .push(Text::new("Hello")),
//! );
//! h.run_frame();
//! h.click(id).run_frame();
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use auralis_signal::Signal;
use rustc_hash::FxHashMap;

use crate::animation::AnimationDriver;
use crate::core::config::StateFlags;
use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::ElementArena;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::event::action::{Action, ActionKind};
use crate::event::{ClickCounter, EventRegistry, FocusManager, FocusReason};
use crate::layout::taffy_bridge::TaffyBridge;
use crate::render::DrawCommand;
use crate::render::{CachedScene, CachedSubtree};
use crate::style::{Point, Rect, Size};
use crate::testing::selector::Selector;

/// Internal phase tag for the touch simulation helpers.
enum TouchSim {
    Down,
    Move,
    Up,
}

/// A headless test driver that simulates the full frame lifecycle.
///
/// Composes `ElementArena`, `TaffyBridge`, `EventRegistry`, `FocusManager`,
/// `ClickCounter`, and `AnimationDriver` — everything `Window::on_frame`
/// uses — without requiring winit, a GPU, or a window.
pub struct TestHarness {
    pub arena: ElementArena,
    app: std::rc::Rc<crate::core::app_context::AppContext>,
    root_id: ElementId,
    taffy: TaffyBridge,
    events: EventRegistry,
    focus_manager: FocusManager,
    #[allow(dead_code)]
    click_counter: ClickCounter,
    animations: AnimationDriver,
    size: Size,
    frame_id: u64,
    hovered: Option<ElementId>,
    /// Full hovered ancestor chain (production PointerMoved parity).
    hovered_chain: Vec<ElementId>,
    #[allow(dead_code)]
    pressed: Option<ElementId>,
    last_cursor: Option<Point>,
    scene_cache: RefCell<FxHashMap<ElementId, Rc<CachedScene>>>,
    subtree_cache: RefCell<FxHashMap<ElementId, Rc<CachedSubtree>>>,
    scroll_kinetic: Option<crate::core::frame_driver::ScrollKinetic>,
    scroll_kinetic_target: Option<ElementId>,
    /// Currently z-elevated dragged row (SEAM-2 drag-z, shared with window).
    drag_elevated: Option<(ElementId, Option<i32>)>,
    /// Production configuration source — scroll physics, theme, colors are
    /// derived from the SAME `WindowConfig` the window uses (audit round 4:
    /// the harness used to hardcode friction/theme, so tests could pass with
    /// values production never runs).
    config: crate::platform::WindowConfig,
    last_frame_instant: Option<web_time::Instant>,
    /// Touch-down anchor for click synthesis in `touch_up_at`.
    touch_down_pos: Option<Point>,
    /// Per-frame O(k) metrics (updated each `run_frame`).
    frame_incremental: u64,
    frame_escalation: u64,
    frame_dirty_set: usize,
    /// Paint commands captured from the most recent frame.
    pub last_scene: Vec<crate::render::DrawCommand>,
    /// Text area descriptors from the most recent frame.
    pub last_text_areas: Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
    /// Whether the most recent frame actually painted (repaint work existed).
    last_painted: bool,
    /// DevTools ring buffer handle (shared, installed globally).
    #[cfg(feature = "devtools")]
    devtools_buf: Option<crate::debug::devtools::DevtoolsRingBuffer>,
}

impl TestHarness {
    pub fn new(width: f32, height: f32) -> Self {
        // Each harness owns a fresh AppContext, so per-instance state is
        // isolated by construction — no manual global reset needed. Create the
        // AppContext BEFORE allocating the root element so the root's ElInfo is
        // registered into this harness's own AppContext (required by
        // `spatial_is_visible_chain_fast` which walks ancestors to the root).
        let app = std::rc::Rc::new(crate::core::app_context::AppContext::new());
        crate::core::app_context::set_current_app(&app);

        let mut arena = ElementArena::new();
        let root_id = arena.allocate();
        arena.set_root(root_id);

        crate::core::clock::install_virtual();
        crate::core::scheduler::reset();
        auralis_task::init_time_source(
            std::rc::Rc::new(crate::core::clock::ClockTimeSource::new()),
        );

        Self {
            arena,
            app,
            root_id,
            taffy: TaffyBridge::new(),
            events: EventRegistry::new(),
            focus_manager: FocusManager::new(),
            click_counter: ClickCounter::new(),
            animations: AnimationDriver::new(),
            size: Size::new(width, height),
            frame_id: 0,
            hovered: None,
            hovered_chain: Vec::new(),
            pressed: None,
            last_cursor: None,
            scene_cache: RefCell::new(FxHashMap::default()),
            subtree_cache: RefCell::new(FxHashMap::default()),
            scroll_kinetic: None,
            scroll_kinetic_target: None,
            drag_elevated: None,
            config: crate::platform::WindowConfig::default(),
            last_frame_instant: None,
            touch_down_pos: None,
            frame_incremental: 0,
            frame_escalation: 0,
            frame_dirty_set: 0,
            last_scene: Vec::new(),
            last_text_areas: Vec::new(),
            last_painted: false,
            #[cfg(feature = "devtools")]
            devtools_buf: crate::debug::devtools::with_ring_buffer(|b| b.clone()),
        }
    }

    /// This harness's private AppContext (multi-window tests install it via
    /// `set_current_app` to simulate per-window event scopes).
    pub fn app(&self) -> &std::rc::Rc<crate::core::app_context::AppContext> {
        &self.app
    }

    /// Read an element's `reactive_visible` cell (portal popups toggle this
    /// on open/close). `None` when the element has no reactive-visibility.
    pub fn reactive_visible_of(&self, id: ElementId) -> Option<bool> {
        crate::core::app_context::set_current_app(&self.app);
        crate::core::dirty_registry::reactive_visible_of(id)
    }

    /// Walk ancestors from `from` (inclusive) to the first element carrying a
    /// `reactive_visible` cell — the portal popup root for dropdown content.
    pub fn popup_root_of(&self, from: ElementId) -> Option<ElementId> {
        crate::core::app_context::set_current_app(&self.app);
        let mut cur = Some(from);
        while let Some(id) = cur {
            if self.reactive_visible_of(id).is_some() {
                return Some(id);
            }
            cur = self.app.parent_of(id);
        }
        None
    }

    /// Mount a widget into the test harness, returning its root `ElementId`.
    pub fn mount(&mut self, widget: impl Widget) -> ElementId {
        crate::core::app_context::set_current_app(&self.app);
        crate::core::element::set_component_tables(self.arena.component_tables.clone());
        let theme = self.config.theme.clone();
        let app_weak = std::rc::Rc::downgrade(&self.app);
        let mut ctx = MountContext::new(
            &mut self.arena,
            Some(self.root_id),
            Some(&mut self.events),
            &theme,
            None,
            app_weak,
        );
        crate::core::dirty_registry::begin_mount_batch();
        let child_id = Box::new(widget).mount_box(&mut ctx);
        crate::core::dirty_registry::end_mount_batch();
        self.arena.add_child(self.root_id, child_id);
        for portal in crate::platform::portal::drain_portals() {
            self.arena.add_child(self.root_id, portal);
        }
        child_id
    }

    /// Run one full frame via the unified `drive_frame` — the SAME pipeline
    /// that `Window::on_frame` runs. Test path == production path.
    pub fn run_frame(&mut self) -> &mut Self {
        crate::core::app_context::set_current_app(&self.app);
        // Production parity: App::about_to_wait reopens the wake gate before
        // each window frame (one wake per event-loop turn). Mid-frame
        // notifies are suppressed by the FramePhase gate, so resetting here
        // leaves the gate open after the frame — exactly the production
        // steady state.
        self.app.reset_dirty_redraw();
        crate::platform::wake::drain_ui_queue();
        self.frame_id += 1;
        crate::core::perf::perf_reset_frame();
        // Reset per-frame debug stats (dirty/process-step counters) so the
        // invariant checks reflect a single frame, not a cross-frame/cross-case
        // accumulation (mirrors window::on_frame).
        #[cfg(debug_assertions)]
        crate::core::dirty_registry::reset_stats();
        crate::core::dirty_registry::devtools_reset_dirty();

        // Anchor the frame instant (deterministic via the virtual clock).
        let now = crate::core::clock::now();
        self.last_frame_instant = Some(now);

        let input = crate::core::frame_driver::FrameInput {
            size: self.size,
            frame_id: self.frame_id,
            is_first_frame: self.frame_id == 1,
            force_layout: false,
            scale_factor: 1.0,
            bg: self.config.theme.scheme.surface,
            fg: self.config.theme.scheme.on_surface,
            highlight_mode: self.focus_manager.highlight_mode(),
            now,
            scroll_friction: self.config.scroll_friction,
            scroll_stop_speed: self.config.scroll_stop_speed,
            skip_paint: false,
        };

        let fcx = crate::core::frame_context::FrameContext::new(
            &self.app,
            &self.scene_cache,
            &self.subtree_cache,
        );
        // Snapshot cumulative layout counters to derive per-frame deltas.
        let inc_before = crate::core::frame_pipeline::incremental_taken_count();
        let esc_before = crate::core::frame_pipeline::escalation_taken_count();
        // Phase 1: layout.
        let stage = {
            let st = crate::core::frame_driver::FrameState {
                arena: &mut self.arena,
                taffy: &mut self.taffy,
                events: &mut self.events,
                animations: &mut self.animations,
                focus: &mut self.focus_manager,
                scroll_kinetic: &mut self.scroll_kinetic,
                scroll_kinetic_target: &mut self.scroll_kinetic_target,
            };
            let mut hook = crate::core::frame_driver::NoHook;
            crate::core::frame_driver::drive_frame_layout(st, &input, &mut hook)
        };
        // ── SEAM 2 (shared platform-frame work: long-press wins, drag ghost,
        //   drag-z, autofocus, a11y dispatch — same code path as the window;
        //   audit round 4) ──
        {
            let st = crate::core::frame_driver::FrameState {
                arena: &mut self.arena,
                taffy: &mut self.taffy,
                events: &mut self.events,
                animations: &mut self.animations,
                focus: &mut self.focus_manager,
                scroll_kinetic: &mut self.scroll_kinetic,
                scroll_kinetic_target: &mut self.scroll_kinetic_target,
            };
            let args = crate::core::frame_driver::PlatformArgs {
                drag_ghost: None,
                drag_elevated: &mut self.drag_elevated,
            };
            let mut hook = crate::core::frame_driver::NoHook;
            crate::core::frame_driver::drive_frame_platform(st, &stage, &input, args, &mut hook);
        }
        // Phase 2: paint.
        let out = {
            let st = crate::core::frame_driver::FrameState {
                arena: &mut self.arena,
                taffy: &mut self.taffy,
                events: &mut self.events,
                animations: &mut self.animations,
                focus: &mut self.focus_manager,
                scroll_kinetic: &mut self.scroll_kinetic,
                scroll_kinetic_target: &mut self.scroll_kinetic_target,
            };
            crate::core::frame_driver::drive_frame_paint(st, &fcx, &input, stage)
        };

        // Per-frame O(k) metrics.
        self.frame_incremental =
            crate::core::frame_pipeline::incremental_taken_count() - inc_before;
        self.frame_escalation = crate::core::frame_pipeline::escalation_taken_count() - esc_before;
        self.frame_dirty_set = out.processed_all.len();
        self.last_scene = out.commands;
        self.last_text_areas = out.text_areas;
        self.last_painted = out.painted;

        // DevTools: collect FrameSnapshot into the global ring buffer.
        #[cfg(feature = "devtools")]
        if self.devtools_buf.is_some() {
            let fps = self
                .last_frame_instant
                .map(|prev| {
                    let us = prev.elapsed().as_micros().max(1) as f32;
                    1_000_000.0 / us
                })
                .unwrap_or(0.0);
            let total_us = self
                .last_frame_instant
                .map(|prev| prev.elapsed().as_micros() as u64)
                .unwrap_or(0);
            let snapshot = crate::debug::devtools::collect_frame_snapshot(
                &self.arena,
                self.frame_id,
                0,
                self.frame_dirty_set,
                fps,
                total_us,
                0,
            );
            crate::debug::devtools::push_snapshot_for_test(snapshot);
        }

        // Accessibility tree update (harness builds directly for assertions;
        // divergence #4 — a11y stays caller-side, both callers build their own way).
        if crate::core::dirty_registry::is_a11y_dirty() {
            let focus_id = self.focus_manager.focused();
            crate::platform::build_accessibility_tree(&self.arena, self.root_id, focus_id);
            crate::core::dirty_registry::clear_a11y_dirty();
        }

        // Invariant checks (debug builds only).
        #[cfg(debug_assertions)]
        crate::testing::invariant::check_all(
            &self.arena,
            self.root_id,
            &self.focus_manager,
            self.frame_id,
        );

        // Process async task timers (virtual-clock-aware via ClockTimeSource).
        auralis_task::flush_all();

        // Production parity with App::about_to_wait: element wakes decay
        // unless renewed by their frame_tick this frame (renewal model).
        crate::core::scheduler::sweep_stale_element_wakes();

        self
    }

    /// Run `n` frames via `settle` semantics: stop early if quiescent.
    pub fn run_frames(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.run_frame();
        }
        self
    }

    /// Run one frame inside `catch_unwind`. Returns `Ok(&mut Self)` on
    /// success, or `Err(panic_message)` if a widget callback panicked.
    /// The harness state is preserved even after a panic.
    pub fn run_frame_safe(&mut self) -> Result<&mut Self, String> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_frame();
        }));
        match result {
            Ok(()) => Ok(self),
            Err(panic) => {
                let msg = crate::core::error::panic_to_string(&panic);
                Err(msg)
            }
        }
    }

    /// Like `settle()` but survives panics in widget callbacks.
    /// Returns the number of frames that completed before the first panic
    /// (or all frames if none panicked).
    pub fn settle_safe(&mut self, max_frames: usize) -> (usize, Option<String>) {
        for i in 0..max_frames {
            if !self.has_active_work() {
                return (i, None);
            }
            match self.run_frame_safe() {
                Ok(_) => {}
                Err(msg) => return (i, Some(msg)),
            }
            if i == max_frames - 1 {
                return (max_frames, None);
            }
        }
        (max_frames, None)
    }

    // ── Interactions ────────────────────────────────────────────────

    /// Simulate a click at a screen position. Performs hit testing,
    /// updates focus, fires click handlers on the hit element.
    pub fn click_at(&mut self, pos: Point) -> &mut Self {
        let hit = dirty_registry::hit_test_with_fallback(&self.arena, pos);
        if let Some(hit_id) = hit {
            if let Some(el) = self.arena.get_mut(hit_id) {
                el.set_state_dirty(StateFlags::PRESSED, true);
            }
            let hit_path = self.arena.path_to_root(hit_id);
            // Mirror the real window's press-release chain through the shared
            // propagation dispatcher: PointerDown handles focus transfer,
            // positioned clicks (`on_click_at`) and drag_start; PointerUp
            // fires drag_end; the Click event fires basic `on_click` handlers.
            // Hand-rolling only propagate_click here (the old behaviour)
            // silently skipped every `on_click_at` widget (Slider, ComboBox
            // trigger, ColorPicker…).
            let down = crate::event::Event::PointerDown {
                position: pos,
                button: crate::event::MouseButton::Left,
                finger_id: None,
            };
            crate::event::propagation::dispatch_event(
                &mut self.arena,
                &down,
                &hit_path,
                &mut self.focus_manager,
                &mut self.events,
                crate::event::types::Modifiers::NONE,
            );
            let up = crate::event::Event::PointerUp {
                position: pos,
                button: crate::event::MouseButton::Left,
                finger_id: None,
            };
            crate::event::propagation::dispatch_event(
                &mut self.arena,
                &up,
                &hit_path,
                &mut self.focus_manager,
                &mut self.events,
                crate::event::types::Modifiers::NONE,
            );
            crate::event::propagation::propagate_click(
                &self.arena,
                &hit_path,
                pos,
                crate::event::types::Modifiers::NONE,
                &mut self.events,
            );
            if let Some(el) = self.arena.get_mut(hit_id) {
                el.set_state_dirty(StateFlags::PRESSED, false);
            }
        }
        // Mirror window.rs: outside-click dismiss for portal-based overlays.
        crate::platform::portal::fire_dismiss(&self.arena, pos);
        self.last_cursor = Some(pos);
        #[cfg(feature = "devtools")]
        {
            let ts = auralis_signal::now_us();
            crate::debug::devtools::record_interaction(
                self.frame_id,
                crate::debug::devtools::InteractionKind::PointerDown {
                    x: pos.x,
                    y: pos.y,
                    button: 1,
                    modifiers: 0,
                },
                ts,
            );
            crate::debug::devtools::record_interaction(
                self.frame_id,
                crate::debug::devtools::InteractionKind::PointerUp {
                    x: pos.x,
                    y: pos.y,
                    button: 1,
                    modifiers: 0,
                },
                ts + 1,
            );
        }
        self
    }

    /// Raw PointerDown at a position — full production event flow (hit
    /// test → gesture arena → propagation). Pair with `pointer_up_at`
    /// to test press-hold-release sequences (long-press, drag arbitration)
    /// with virtual-clock time between the two.
    pub fn pointer_down_at(&mut self, pos: Point) -> &mut Self {
        let hit = dirty_registry::hit_test_with_fallback(&self.arena, pos);
        if let Some(hit_id) = hit {
            if let Some(el) = self.arena.get_mut(hit_id) {
                el.set_state_dirty(StateFlags::PRESSED, true);
            }
            let hit_path = self.arena.path_to_root(hit_id);
            let down = crate::event::Event::PointerDown {
                position: pos,
                button: crate::event::MouseButton::Left,
                finger_id: None,
            };
            crate::event::propagation::dispatch_event(
                &mut self.arena,
                &down,
                &hit_path,
                &mut self.focus_manager,
                &mut self.events,
                crate::event::types::Modifiers::NONE,
            );
        }
        self.last_cursor = Some(pos);
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerDown {
                x: pos.x,
                y: pos.y,
                button: 1,
                modifiers: 0,
            },
            auralis_signal::now_us(),
        );
        self
    }

    /// Raw PointerMove at a position — full production event flow (hit
    /// test → gesture arena → propagation). Completes the
    /// `pointer_down_at` / `pointer_up_at` trio for gesture testing.
    pub fn pointer_move_at(&mut self, pos: Point) -> &mut Self {
        let hit = dirty_registry::hit_test_with_fallback(&self.arena, pos);
        if let Some(hit_id) = hit {
            let hit_path = self.arena.path_to_root(hit_id);
            let mv = crate::event::Event::PointerMove {
                position: pos,
                finger_id: None,
            };
            crate::event::propagation::dispatch_event(
                &mut self.arena,
                &mv,
                &hit_path,
                &mut self.focus_manager,
                &mut self.events,
                crate::event::types::Modifiers::NONE,
            );
        }
        self.last_cursor = Some(pos);
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerMove { x: pos.x, y: pos.y },
            auralis_signal::now_us(),
        );
        self
    }

    /// Raw PointerUp at a position — completes a `pointer_down_at`
    /// sequence through the production event flow.
    pub fn pointer_up_at(&mut self, pos: Point) -> &mut Self {
        let hit = dirty_registry::hit_test_with_fallback(&self.arena, pos);
        if let Some(hit_id) = hit {
            let hit_path = self.arena.path_to_root(hit_id);
            let up = crate::event::Event::PointerUp {
                position: pos,
                button: crate::event::MouseButton::Left,
                finger_id: None,
            };
            crate::event::propagation::dispatch_event(
                &mut self.arena,
                &up,
                &hit_path,
                &mut self.focus_manager,
                &mut self.events,
                crate::event::types::Modifiers::NONE,
            );
            if let Some(el) = self.arena.get_mut(hit_id) {
                el.set_state_dirty(StateFlags::PRESSED, false);
            }
        }
        self.last_cursor = Some(pos);
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerUp {
                x: pos.x,
                y: pos.y,
                button: 1,
                modifiers: 0,
            },
            auralis_signal::now_us(),
        );
        self
    }

    /// Raw touch-down at a position (finger_id = 1): the touch twin of
    /// `pointer_down_at`. Touch pointers activate touch-only arena
    /// members (ScrollRecognizer).
    pub fn touch_down_at(&mut self, pos: Point) -> &mut Self {
        self.pointer_event_at(pos, TouchSim::Down);
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerDown {
                x: pos.x,
                y: pos.y,
                button: 1,
                modifiers: 0,
            },
            auralis_signal::now_us(),
        );
        self
    }

    /// Raw touch-move (finger_id = 1) through the production event flow.
    pub fn touch_move_at(&mut self, pos: Point) -> &mut Self {
        self.pointer_event_at(pos, TouchSim::Move);
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerMove { x: pos.x, y: pos.y },
            auralis_signal::now_us(),
        );
        self
    }

    /// Raw touch-up (finger_id = 1). Mirrors the window's release logic:
    /// when the gesture arena did not suppress the click AND the pointer
    /// stayed within the drag threshold, a Click event is synthesized —
    /// the same contract production applies on touch release.
    pub fn touch_up_at(&mut self, pos: Point) -> &mut Self {
        self.pointer_event_at(pos, TouchSim::Up);
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerUp {
                x: pos.x,
                y: pos.y,
                button: 1,
                modifiers: 0,
            },
            auralis_signal::now_us(),
        );
        self
    }

    fn pointer_event_at(&mut self, pos: Point, phase: TouchSim) {
        const FINGER: u64 = 1;
        let hit = dirty_registry::hit_test_with_fallback(&self.arena, pos);
        let Some(hit_id) = hit else {
            self.last_cursor = Some(pos);
            return;
        };
        let hit_path = self.arena.path_to_root(hit_id);
        let evt = match phase {
            TouchSim::Down => {
                self.touch_down_pos = Some(pos);
                crate::event::Event::PointerDown {
                    position: pos,
                    button: crate::event::MouseButton::Left,
                    finger_id: Some(FINGER),
                }
            }
            TouchSim::Move => crate::event::Event::PointerMove {
                position: pos,
                finger_id: Some(FINGER),
            },
            TouchSim::Up => crate::event::Event::PointerUp {
                position: pos,
                button: crate::event::MouseButton::Left,
                finger_id: Some(FINGER),
            },
        };
        crate::event::propagation::dispatch_event(
            &mut self.arena,
            &evt,
            &hit_path,
            &mut self.focus_manager,
            &mut self.events,
            crate::event::types::Modifiers::NONE,
        );
        if matches!(phase, TouchSim::Up) {
            // Production-parity click synthesis on release.
            let within = self
                .touch_down_pos
                .take()
                .is_some_and(|d| pos.distance(&d) <= crate::event::recognizer::TAP_DRAG_THRESHOLD);
            let suppressed = crate::event::recognizer::take_click_suppressed(FINGER);
            if within && !suppressed {
                crate::event::propagation::propagate_click(
                    &self.arena,
                    &hit_path,
                    pos,
                    crate::event::types::Modifiers::NONE,
                    &mut self.events,
                );
            }
        }
        self.last_cursor = Some(pos);
    }

    /// Directly fire a click on the element with the given id (no hit test).
    pub fn click(&mut self, id: ElementId) -> &mut Self {
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerDown {
                x: 0.0,
                y: 0.0,
                button: 1,
                modifiers: 0,
            },
            auralis_signal::now_us(),
        );
        let old_focus = self.focus_manager.focused();
        if old_focus != Some(id) {
            if let Some(old_id) = old_focus {
                if let Some(el) = self.arena.get_mut(old_id) {
                    el.set_state_dirty(StateFlags::FOCUSED, false);
                    el.last_focus_reason.set(Some(FocusReason::PointerClick));
                }
                self.events
                    .fire_focus_out(old_id, FocusReason::PointerClick);
            }
            self.focus_manager.set_focused(Some(id));
            if let Some(el) = self.arena.get_mut(id) {
                el.set_state_dirty(StateFlags::FOCUSED, true);
                el.last_focus_reason.set(Some(FocusReason::PointerClick));
            }
            self.events.fire_focus_in(id, FocusReason::PointerClick);
        }
        self.events.fire_click(id);
        self
    }

    /// Type text into a `TextInput` element, one character at a time.
    pub fn type_text(&mut self, id: ElementId, text: &str) -> &mut Self {
        for ch in text.chars() {
            self.events.fire_text_input(id, ch);
            #[cfg(feature = "devtools")]
            crate::debug::devtools::record_interaction(
                self.frame_id,
                crate::debug::devtools::InteractionKind::ImeCommit {
                    text: ch.to_string(),
                },
                auralis_signal::now_us(),
            );
        }
        self
    }

    /// Fire a key-down event on the focused element.
    /// Also translates navigation/activation keys to actions and fires
    /// them through the action pipeline, mirroring the real window's
    /// keyboard → key-binding → action dispatch path.
    /// Fire a key-down event on the focused element.
    ///
    /// Mirrors the production window's keyboard path exactly (audit round
    /// 6): the key is resolved through the REAL `KeyBindingMap` (so
    /// chords like Ctrl+A → SelectAll work) and the resulting action is
    /// dispatched along the focused element's ancestor path via
    /// `propagation::dispatch_action` — handlers registered on container
    /// elements (Table, List) receive it, same as production. Previously
    /// this used a hand-written key→action table (no chords) and fired
    /// the action only at the focused element itself.
    pub fn press_key(
        &mut self,
        key: crate::event::Key,
        mods: crate::event::Modifiers,
    ) -> &mut Self {
        if let Some(id) = self.focus_manager.focused() {
            self.events.fire_key_down(id, key.clone(), mods);
            let bindings = crate::event::bindings::KeyBindingMap::new();
            if let Some(kind) = bindings.find(Some(id), &key, &mods) {
                let action = if mods.shift {
                    Action::new(kind).with_selection()
                } else {
                    Action::new(kind)
                };
                let path = self.arena.path_to_root(id);
                let outcome = crate::event::propagation::dispatch_action(
                    &mut self.arena,
                    &action,
                    &path,
                    &mut self.events,
                    &[],
                );
                // Production fallback: unhandled Activate/NewLine clicks
                // the focused element (window.rs dispatch_action parity).
                if !outcome.is_handled()
                    && matches!(kind, ActionKind::Activate | ActionKind::NewLine)
                {
                    self.events.fire_click(id);
                }
            }
        }
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::KeyPress {
                key_name: format!("{:?}", key),
                modifiers: crate::debug::devtools::modifiers_to_u32(mods),
            },
            auralis_signal::now_us(),
        );
        self
    }

    /// Fire a key-up event on the focused element.
    pub fn release_key(
        &mut self,
        key: crate::event::Key,
        mods: crate::event::Modifiers,
    ) -> &mut Self {
        if let Some(id) = self.focus_manager.focused() {
            self.events.fire_key_up(id, key.clone(), mods);
        }
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::KeyRelease {
                key_name: format!("{:?}", key),
                modifiers: crate::debug::devtools::modifiers_to_u32(mods),
            },
            auralis_signal::now_us(),
        );
        self
    }

    /// Fire a double-click event on the given element.
    pub fn double_click(&mut self, id: ElementId) -> &mut Self {
        self.events.fire_double_click(id);
        self
    }

    /// Fire a long-press event on the given element.
    pub fn long_press(&mut self, id: ElementId) -> &mut Self {
        self.events.fire_long_press(id);
        self
    }

    /// Simulate hovering the cursor at a screen position.
    ///
    /// Mirrors the production PointerMoved hover semantics
    /// (window.rs): full hit-test *chain* diff — leave fires deepest-first
    /// on elements no longer under the cursor, enter fires on newly covered
    /// elements (skipping disabled subtrees), and every entered/left element
    /// gets its HOVERED flag flipped (SEAM-0 parity; the old version only
    /// touched the single leaf element).
    pub fn hover_at(&mut self, pos: Point) -> &mut Self {
        let new_chain: Vec<ElementId> = crate::event::hit_test::hit_test(&self.arena, pos)
            .map(|r| r.path)
            .unwrap_or_default();
        let old_chain = std::mem::take(&mut self.hovered_chain);

        // Leaf-first ancestor chains: intersection == common suffix (same
        // O(depth) diff as window.rs PointerMoved).
        let common = {
            let mut n = 0;
            while n < old_chain.len()
                && n < new_chain.len()
                && old_chain[old_chain.len() - 1 - n] == new_chain[new_chain.len() - 1 - n]
            {
                n += 1;
            }
            n
        };

        // Leave: in old but not new, deepest first.
        for &eid in &old_chain[..old_chain.len() - common] {
            if let Some(el) = self.arena.get_mut(eid) {
                el.set_state_dirty(StateFlags::HOVERED, false);
            }
            self.events.fire_hover_leave(eid);
        }
        // Enter: in new but not old, deepest first.
        for &eid in &new_chain[..new_chain.len() - common] {
            if dirty_registry::is_element_or_ancestor_disabled(eid) {
                continue;
            }
            if let Some(el) = self.arena.get_mut(eid) {
                el.set_state_dirty(StateFlags::HOVERED, true);
            }
            self.events.fire_hover_enter(eid);
        }

        self.hovered = new_chain.last().copied();
        self.hovered_chain = new_chain;
        self.last_cursor = Some(pos);
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::PointerMove { x: pos.x, y: pos.y },
            auralis_signal::now_us(),
        );
        self
    }

    /// Clear the current hover state, firing `hover_leave` (deepest first)
    /// on every element of the hovered chain.
    pub fn unhover(&mut self) -> &mut Self {
        let old_chain = std::mem::take(&mut self.hovered_chain);
        for &eid in &old_chain {
            if let Some(el) = self.arena.get_mut(eid) {
                el.set_state_dirty(StateFlags::HOVERED, false);
            }
            self.events.fire_hover_leave(eid);
        }
        self.hovered = None;
        self.last_cursor = None;
        self
    }

    /// Simulate a press-drag-release gesture from `from` to `to`.
    ///
    /// Mirrors the drag pipeline in `platform::window::WindowState::dispatch_events`:
    /// hit-tests at `from` to pick the drag target, fires `drag_start` (press),
    /// then `drag_update` at each interpolated step, then `drag_end` (release).
    /// Positions passed to handlers are `(local, absolute)` exactly as window.rs computes.
    pub fn drag(&mut self, from: Point, to: Point) -> &mut Self {
        let target = match dirty_registry::hit_test_with_fallback(&self.arena, from) {
            Some(t) => t,
            None => {
                self.last_cursor = Some(to);
                return self;
            }
        };
        let local_of = |arena: &ElementArena, abs: Point| -> Point {
            if let Some(el) = arena.get(target) {
                let sb = el.screen_bounds;
                let (sx, sy) = arena.accumulated_scroll(target);
                Point::new(abs.x - sb.x + sx, abs.y - sb.y + sy)
            } else {
                abs
            }
        };

        if self.events.has_drag_start(target) {
            let local = local_of(&self.arena, from);
            self.events.fire_drag_start(target, local, from);
        }
        // Interpolate a few steps so widgets that integrate deltas behave realistically.
        const STEPS: usize = 4;
        for i in 1..=STEPS {
            let t = i as f32 / STEPS as f32;
            let abs = Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
            if self.events.has_drag_update(target) {
                let local = local_of(&self.arena, abs);
                self.events.fire_drag_update(target, local, abs);
            }
        }
        if self.events.has_drag_end(target) {
            let local = local_of(&self.arena, to);
            self.events.fire_drag_end(target, local, to);
        }
        self.last_cursor = Some(to);
        #[cfg(feature = "devtools")]
        {
            let ts = auralis_signal::now_us();
            crate::debug::devtools::record_interaction(
                self.frame_id,
                crate::debug::devtools::InteractionKind::PointerDown {
                    x: from.x,
                    y: from.y,
                    button: 1,
                    modifiers: 0,
                },
                ts,
            );
            crate::debug::devtools::record_interaction(
                self.frame_id,
                crate::debug::devtools::InteractionKind::PointerMove { x: to.x, y: to.y },
                ts + 1,
            );
            crate::debug::devtools::record_interaction(
                self.frame_id,
                crate::debug::devtools::InteractionKind::PointerUp {
                    x: to.x,
                    y: to.y,
                    button: 1,
                    modifiers: 0,
                },
                ts + 2,
            );
        }
        self
    }

    /// Simulate a scroll delta on the given element (typically a `ScrollView` or
    /// `ScrollArea`). Propagates through the capture-bubble path; if no handler
    /// consumes it, falls through to `do_scroll` (direct offset mutation) —
    /// exactly matching the real window's scroll dispatch (no double-apply).
    pub fn scroll(&mut self, id: ElementId, dx: f32, dy: f32) -> &mut Self {
        let path = self.arena.path_to_root(id);
        let consumed = crate::event::propagation::propagate_scroll(&path, dx, dy, &mut self.events);
        if !consumed {
            crate::widgets::bundle::scroll::do_scroll(&self.arena, id, dx, dy);
        }
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::Scroll {
                x: 0.0,
                y: 0.0,
                delta_x: dx,
                delta_y: dy,
            },
            auralis_signal::now_us(),
        );
        self
    }

    /// Advance the virtual clock by `millis` milliseconds.
    /// Useful for testing animations and time-dependent behaviour (tooltips,
    /// auto-dismiss timers).
    pub fn advance_time(&mut self, millis: u64) -> &mut Self {
        crate::core::clock::advance(Duration::from_millis(millis));
        self
    }

    /// Advance the virtual clock to the scheduler's next deadline
    /// and run one frame. Deterministic: no wall-clock dependency.
    /// Returns false if no deadline was pending (no frame was due).
    pub fn advance_to_next_deadline(&mut self) -> bool {
        if let Some(deadline) = crate::core::scheduler::next_deadline() {
            let now = crate::core::clock::now();
            if deadline > now {
                let dur = deadline.duration_since(now);
                crate::core::clock::advance(dur);
            }
            self.run_frame();
            true
        } else {
            false
        }
    }

    /// Set a Signal value programmatically.
    pub fn set_signal<T: Clone + 'static>(&mut self, signal: &Signal<T>, value: T) -> &mut Self {
        signal.set(value);
        self
    }

    /// Read a Signal value.
    pub fn read_signal<T: Clone + 'static>(&self, signal: &Signal<T>) -> T {
        signal.read()
    }

    /// Resize the harness viewport, triggering a REPOSITION-only taffy path
    /// on the next frame.
    pub fn resize(&mut self, width: f32, height: f32) -> &mut Self {
        self.size = Size::new(width, height);
        if let Some(el) = self.arena.get_mut(self.root_id) {
            el.mark_reposition();
        }
        #[cfg(feature = "devtools")]
        crate::debug::devtools::record_interaction(
            self.frame_id,
            crate::debug::devtools::InteractionKind::Resize { width, height },
            auralis_signal::now_us(),
        );
        self
    }

    /// Get the taffy NodeId for an element, if it exists in the tree.
    pub fn taffy_node(&self, id: ElementId) -> Option<taffy::NodeId> {
        self.taffy.node_for(id)
    }

    // ── Queries ─────────────────────────────────────────────────────

    pub fn find(&self, id: ElementId) -> Option<&crate::core::element::Element> {
        self.arena.get(id)
    }

    pub fn find_mut(&mut self, id: ElementId) -> Option<&mut crate::core::element::Element> {
        self.arena.get_mut(id)
    }

    pub fn focused(&self) -> Option<ElementId> {
        self.focus_manager.focused()
    }

    /// Count the number of elements with the REPAINT dirty flag.
    pub fn dirty_count(&self) -> usize {
        fn count(arena: &ElementArena, eid: ElementId) -> usize {
            let n = if arena.get(eid).is_some_and(|el| el.needs_repaint()) {
                1
            } else {
                0
            };
            let cids = arena
                .get(eid)
                .map(|el| el.children.clone())
                .unwrap_or_default();
            n + cids.iter().map(|&cid| count(arena, cid)).sum::<usize>()
        }
        count(&self.arena, self.root_id)
    }

    /// Count the number of elements with the MEASURE dirty flag.
    pub fn measure_dirty_count(&self) -> usize {
        fn count(arena: &ElementArena, eid: ElementId) -> usize {
            let n = if arena.get(eid).is_some_and(|el| el.needs_measure()) {
                1
            } else {
                0
            };
            let cids = arena
                .get(eid)
                .map(|el| el.children.clone())
                .unwrap_or_default();
            n + cids.iter().map(|&cid| count(arena, cid)).sum::<usize>()
        }
        count(&self.arena, self.root_id)
    }

    /// Number of incremental (relayout-boundary) layout passes taken so far
    /// (process-wide thread-local counter; useful to assert the incremental
    /// path is actually exercised).
    pub fn incremental_taken(&self) -> u64 {
        crate::core::frame_pipeline::incremental_taken_count()
    }

    /// Number of times the incremental path escalated to a full pass (a
    /// single-axis boundary's dependent axis changed).
    pub fn escalation_taken(&self) -> u64 {
        crate::core::frame_pipeline::escalation_taken_count()
    }

    // ═══════════════ O(k) assertions (auralis-unique) ═══════════════
    //
    // Assert that the framework did O(k) *work*, not merely produced
    // O(k)-correct output. No other GUI test framework exposes this.

    /// Number of subtree-cache replays in the most recent frame (O(k) paint win).
    pub fn subtree_cache_hits(&self) -> u64 {
        crate::core::frame_pipeline::subtree_cache_hits()
    }

    /// Number of subtree-cache misses (re-records) in the most recent frame.
    pub fn subtree_cache_misses(&self) -> u64 {
        crate::core::frame_pipeline::subtree_cache_misses()
    }

    /// Number of incremental (relayout-boundary) layout passes in the most
    /// recent frame (per-frame delta, unlike the cumulative `incremental_taken`).
    pub fn frame_incremental_layouts(&self) -> u64 {
        self.frame_incremental
    }

    /// Number of relayout-boundary escalations in the most recent frame.
    pub fn frame_escalations(&self) -> u64 {
        self.frame_escalation
    }

    /// Size of the processed dirty set in the most recent frame (≈ k).
    pub fn frame_dirty_set_size(&self) -> usize {
        self.frame_dirty_set
    }

    /// Number of paint commands produced by the most recent frame.
    pub fn paint_command_count(&self) -> usize {
        self.last_scene.len()
    }

    /// Whether the most recent `run_frame` produced paint output.
    /// `false` means the frame was fully quiescent (O(k) win: no repaint).
    pub fn last_painted(&self) -> bool {
        self.last_painted
    }

    /// Enable per-frame phase timing for all subsequent `run_frame` calls.
    pub fn enable_perf(&mut self) {
        crate::core::perf::perf_enable();
    }

    /// Disable per-frame phase timing.
    pub fn disable_perf(&mut self) {
        crate::core::perf::perf_disable();
    }

    /// Per-phase timing breakdown for the most recent frame (microseconds).
    /// Returns zeros if `enable_perf()` was not called.
    pub fn frame_timing(&self) -> crate::core::perf::FrameTiming {
        crate::core::perf::perf_take_frame()
    }

    /// Total wall-clock time for the most recent frame in microseconds.
    pub fn last_frame_us(&self) -> u64 {
        crate::core::perf::perf_take_frame().total_us
    }

    /// Assert the most recent frame replayed exactly `n` subtree caches.
    pub fn assert_subtree_cache_hits(&self, n: u64) -> &Self {
        assert_eq!(
            self.subtree_cache_hits(),
            n,
            "expected {} subtree-cache hits, got {} (misses: {})",
            n,
            self.subtree_cache_hits(),
            self.subtree_cache_misses()
        );
        self
    }

    /// Assert the most recent frame replayed at least `n` subtree caches.
    pub fn assert_min_subtree_cache_hits(&self, n: u64) -> &Self {
        assert!(
            self.subtree_cache_hits() >= n,
            "expected >= {} subtree-cache hits, got {}",
            n,
            self.subtree_cache_hits()
        );
        self
    }

    /// Assert the most recent frame's incremental layout did NOT escalate to a
    /// full pass (the O(k) layout guarantee held).
    pub fn assert_no_relayout_escalation(&self) -> &Self {
        assert_eq!(
            self.frame_escalation, 0,
            "expected no relayout escalation, got {} this frame",
            self.frame_escalation
        );
        self
    }

    /// Assert the most recent frame took the incremental (relayout-boundary) path.
    pub fn assert_incremental_layout_taken(&self) -> &Self {
        assert!(
            self.frame_incremental >= 1,
            "expected incremental layout this frame, got {} passes",
            self.frame_incremental
        );
        self
    }

    /// Assert the most recent frame's dirty set was at most `max_k` elements
    /// (the O(k) dirty-tracking guarantee).
    pub fn assert_dirty_set_size(&self, max_k: usize) -> &Self {
        assert!(
            self.frame_dirty_set <= max_k,
            "expected dirty set <= {}, got {} elements",
            max_k,
            self.frame_dirty_set
        );
        self
    }

    /// Assert the most recent frame produced at most `max_n` paint commands
    /// (no full-tree re-record).
    pub fn assert_paint_command_count(&self, max_n: usize) -> &Self {
        assert!(
            self.last_scene.len() <= max_n,
            "expected <= {} paint commands, got {}",
            max_n,
            self.last_scene.len()
        );
        self
    }

    pub fn size(&self) -> Size {
        self.size
    }
    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }
    pub fn root(&self) -> &ElementArena {
        &self.arena
    }
    pub fn root_id(&self) -> ElementId {
        self.root_id
    }
    pub fn events(&self) -> &EventRegistry {
        &self.events
    }
    pub fn events_mut(&mut self) -> &mut EventRegistry {
        &mut self.events
    }

    /// Mutably access the production-configuration source (scroll physics,
    /// theme, colours — derived from the SAME `WindowConfig` the window uses).
    pub fn config_mut(&mut self) -> &mut crate::platform::WindowConfig {
        &mut self.config
    }

    // ── Assertions ──────────────────────────────────────────────────

    pub fn assert_text(&self, id: ElementId, expected: &str) -> &Self {
        // Queries must read THIS harness's component tables even when
        // another harness ran more recently (multi-window tests).
        crate::core::app_context::set_current_app(&self.app);
        let el = self.arena.get(id).expect("assert_text: element not found");
        // Signal-bound Texts update via lazy_label (paint-time shaping);
        // the mount-time accessible_label is a stale snapshot for them.
        let lazy =
            crate::core::element::with_ct(|ct| ct.text.get(&id).and_then(|t| t.lazy_label.clone()));
        let label = lazy
            .map(|cell| {
                let v = cell.take();
                cell.set(v.clone());
                v
            })
            .or_else(|| el.accessible_label())
            .or_else(|| {
                el.text_buffer().as_ref().map(|b| {
                    let buf = b.borrow();
                    let mut lines = Vec::new();
                    for run in buf.lines.iter() {
                        lines.push(run.text());
                    }
                    lines.join("")
                })
            })
            .unwrap_or_default();
        assert_eq!(label, expected, "Text mismatch for element {:?}", id);
        self
    }

    pub fn assert_visible(&self, id: ElementId) -> &Self {
        let el = self
            .arena
            .get(id)
            .expect("assert_visible: element not found");
        assert!(el.is_visible(), "Element {:?} should be visible", id);
        self
    }

    pub fn assert_not_visible(&self, id: ElementId) -> &Self {
        let el = self
            .arena
            .get(id)
            .expect("assert_not_visible: element not found");
        assert!(!el.is_visible(), "Element {:?} should NOT be visible", id);
        self
    }

    pub fn assert_bounds(&self, id: ElementId, x: f32, y: f32, w: f32, h: f32) -> &Self {
        let el = self
            .arena
            .get(id)
            .expect("assert_bounds: element not found");
        let b = el.screen_bounds;
        assert!(
            (b.x - x).abs() < 1.0
                && (b.y - y).abs() < 1.0
                && (b.width - w).abs() < 1.0
                && (b.height - h).abs() < 1.0,
            "Bounds mismatch for {:?}: expected ({}, {}, {}, {}), got ({}, {}, {}, {})",
            id,
            x,
            y,
            w,
            h,
            b.x,
            b.y,
            b.width,
            b.height,
        );
        self
    }

    pub fn assert_focused(&self, id: ElementId) -> &Self {
        assert_eq!(
            self.focus_manager.focused(),
            Some(id),
            "Expected {:?} to be focused",
            id
        );
        self
    }

    pub fn assert_not_focused(&self) -> &Self {
        assert!(
            self.focus_manager.focused().is_none(),
            "Expected no element to be focused"
        );
        self
    }

    pub fn assert_dirty(&self, id: ElementId) -> &Self {
        let el = self.arena.get(id).expect("assert_dirty: element not found");
        assert!(el.needs_repaint(), "Element {:?} should be dirty", id);
        self
    }

    pub fn assert_not_dirty(&self, id: ElementId) -> &Self {
        let el = self
            .arena
            .get(id)
            .expect("assert_not_dirty: element not found");
        assert!(!el.needs_repaint(), "Element {:?} should NOT be dirty", id);
        self
    }

    pub fn assert_child_count(&self, id: ElementId, n: usize) -> &Self {
        let el = self
            .arena
            .get(id)
            .expect("assert_child_count: element not found");
        assert_eq!(el.children.len(), n, "Child count mismatch for {:?}", id);
        self
    }

    // ═══════════════ Accessibility ═══════════════

    /// Build and return the current accessibility tree.
    pub fn a11y_tree(&self) -> accesskit::TreeUpdate {
        crate::platform::build_accessibility_tree(
            &self.arena,
            self.root_id,
            self.focus_manager.focused(),
        )
    }

    /// Find the accesskit `Node` for the given element in the tree.
    fn find_node(tree: &accesskit::TreeUpdate, id: ElementId) -> Option<&accesskit::Node> {
        let nid = accesskit::NodeId(id.to_u64());
        tree.nodes
            .iter()
            .find(|(node_id, _)| *node_id == nid)
            .map(|(_, node)| node)
    }

    pub fn assert_a11y_node_count(&self, n: usize) -> &Self {
        let tree = self.a11y_tree();
        assert_eq!(tree.nodes.len(), n, "A11y node count mismatch");
        self
    }

    pub fn assert_a11y_role(&self, id: ElementId, role: accesskit::Role) -> &Self {
        let tree = self.a11y_tree();
        let node =
            Self::find_node(&tree, id).unwrap_or_else(|| panic!("A11y: no node for {:?}", id));
        assert_eq!(node.role(), role, "A11y role mismatch for {:?}", id);
        self
    }

    pub fn assert_a11y_label(&self, id: ElementId, expected: &str) -> &Self {
        let tree = self.a11y_tree();
        let node =
            Self::find_node(&tree, id).unwrap_or_else(|| panic!("A11y: no node for {:?}", id));
        assert_eq!(
            node.label(),
            Some(expected),
            "A11y label mismatch for {:?}",
            id
        );
        self
    }

    pub fn assert_a11y_value(&self, id: ElementId, expected: &str) -> &Self {
        let tree = self.a11y_tree();
        let node =
            Self::find_node(&tree, id).unwrap_or_else(|| panic!("A11y: no node for {:?}", id));
        assert_eq!(
            node.value(),
            Some(expected),
            "A11y value mismatch for {:?}",
            id
        );
        self
    }

    pub fn assert_a11y_disabled(&self, id: ElementId) -> &Self {
        let tree = self.a11y_tree();
        let node =
            Self::find_node(&tree, id).unwrap_or_else(|| panic!("A11y: no node for {:?}", id));
        assert!(node.is_disabled(), "A11y: {:?} should be disabled", id);
        self
    }

    pub fn assert_a11y_hidden(&self, id: ElementId) -> &Self {
        let tree = self.a11y_tree();
        let node =
            Self::find_node(&tree, id).unwrap_or_else(|| panic!("A11y: no node for {:?}", id));
        assert!(node.is_hidden(), "A11y: {:?} should be hidden", id);
        self
    }

    pub fn assert_a11y_required(&self, id: ElementId) -> &Self {
        let tree = self.a11y_tree();
        let node =
            Self::find_node(&tree, id).unwrap_or_else(|| panic!("A11y: no node for {:?}", id));
        assert!(node.is_required(), "A11y: {:?} should be required", id);
        self
    }

    pub fn assert_a11y_focus(&self, id: ElementId) -> &Self {
        let tree = self.a11y_tree();
        let expected = accesskit::NodeId(id.to_u64());
        assert_eq!(
            tree.focus, expected,
            "A11y focus mismatch: expected {:?}, got {:?}",
            id, tree.focus
        );
        self
    }

    // ═══════════════ Pixel-level assertions (Plan A) ════════════════

    /// Render the most recent frame's `DrawCommand` output to a `PixelBuffer`
    /// via tiny-skia (fully headless — no window/GPU). Call this right after a
    /// painting frame: a settled frame clears `last_scene`, so the buffer would
    /// be empty. Scope: solid fills only (see `testing::pixel`).
    #[cfg(feature = "backend-tiny-skia")]
    pub fn render_to_pixels(&self) -> crate::testing::pixel::PixelBuffer {
        let bg = self.config.theme.scheme.surface;
        crate::testing::pixel::rasterize_commands(
            &self.last_scene,
            self.size.width.ceil() as u32,
            self.size.height.ceil() as u32,
            1.0,
            bg,
        )
    }

    /// Read the rendered colour at logical pixel `(x, y)`.
    #[cfg(feature = "backend-tiny-skia")]
    pub fn pixel_at(&self, x: u32, y: u32) -> Option<crate::style::Color> {
        self.render_to_pixels().pixel_color(x, y)
    }

    /// Assert the rendered pixel at logical `(x, y)` matches `expected`
    /// (within a small tolerance for anti-aliasing).
    #[cfg(feature = "backend-tiny-skia")]
    pub fn assert_pixel(&self, x: u32, y: u32, expected: crate::style::Color) -> &Self {
        let got = self.pixel_at(x, y);
        let tol = 3.0 / 255.0;
        let ok = got.is_some_and(|c| {
            (c.r - expected.r).abs() < tol
                && (c.g - expected.g).abs() < tol
                && (c.b - expected.b).abs() < tol
                && (c.a - expected.a).abs() < tol
        });
        assert!(
            ok,
            "pixel ({x}, {y}) expected ~rgba({:.3},{:.3},{:.3},{:.3}), got {}",
            expected.r,
            expected.g,
            expected.b,
            expected.a,
            got.map(|c| format!("rgba({:.3},{:.3},{:.3},{:.3})", c.r, c.g, c.b, c.a))
                .unwrap_or_else(|| "out-of-bounds".into()),
        );
        self
    }

    // ═══════════════ Golden / snapshot testing ══════════════════════

    /// Compare the most recent frame against the committed baseline
    /// `<crate_dir>/tests/snapshots/<name>.png` with the given options.
    /// Prefer the `assert_snapshot!` macro, which fills `crate_dir` in
    /// automatically via `env!("CARGO_MANIFEST_DIR")`.
    #[cfg(feature = "backend-tiny-skia")]
    pub fn assert_snapshot_at_with(
        &self,
        crate_dir: &str,
        name: &str,
        opts: &crate::testing::snapshot::SnapshotOptions,
    ) -> &Self {
        let buf = self.render_to_pixels();
        if let Err(msg) = crate::testing::snapshot::check_snapshot(&buf, crate_dir, name, opts) {
            panic!("{msg}");
        }
        self
    }

    /// Snapshot with default options.
    #[cfg(feature = "backend-tiny-skia")]
    pub fn assert_snapshot_at(&self, crate_dir: &str, name: &str) -> &Self {
        self.assert_snapshot_at_with(
            crate_dir,
            name,
            &crate::testing::snapshot::SnapshotOptions::default(),
        )
    }

    // ═══════════════ Component-level interaction API ────────────────
    // These methods compose state-machine transitions + callback fires
    // into semantic component-level interactions. They bypass the full
    // event pipeline (hit-test / recognizer / propagation) and directly
    // exercise the component's internal state machine and callbacks,
    // making tests deterministic and focused on component correctness.

    /// Activate a button-like component: PRESSED state → fire click →
    /// clear PRESSED → run frame. Tests both the visual press feedback
    /// (state-style resolution) AND the click callback.
    pub fn activate_button(&mut self, id: ElementId) -> &mut Self {
        if let Some(el) = self.arena.get_mut(id) {
            el.set_state_dirty(StateFlags::PRESSED, true);
        }
        self.click(id);
        if let Some(el) = self.arena.get_mut(id) {
            el.set_state_dirty(StateFlags::PRESSED, false);
        }
        self.run_frame();
        self
    }

    /// Set or clear interaction state flags on an element.
    /// Accepts a single flag (PRESSED, HOVERED, FOCUSED, etc.) and a bool.
    /// Runs a frame after so visual resolution reflects the new state.
    pub fn set_state(&mut self, id: ElementId, flag: StateFlags, on: bool) -> &mut Self {
        if let Some(el) = self.arena.get_mut(id) {
            el.set_state_dirty(flag, on);
        }
        self.run_frame();
        self
    }

    /// Hover an element by id — no position needed, directly sets HOVERED
    /// and fires on_hover_enter. Runs a frame for visual resolution.
    pub fn hover(&mut self, id: ElementId) -> &mut Self {
        // Fire hover_enter if we're entering a new element.
        if self.hovered != Some(id) {
            if let Some(prev) = self.hovered.take() {
                if let Some(el) = self.arena.get_mut(prev) {
                    el.set_state_dirty(StateFlags::HOVERED, false);
                }
                self.events.fire_hover_leave(prev);
            }
            self.hovered = Some(id);
            if let Some(el) = self.arena.get_mut(id) {
                el.set_state_dirty(StateFlags::HOVERED, true);
            }
            self.events.fire_hover_enter(id);
        }
        self.run_frame();
        self
    }

    /// Remove hover state from the currently hovered element.
    pub fn unhover_id(&mut self) -> &mut Self {
        if let Some(id) = self.hovered.take() {
            if let Some(el) = self.arena.get_mut(id) {
                el.set_state_dirty(StateFlags::HOVERED, false);
            }
            self.events.fire_hover_leave(id);
        }
        self.run_frame();
        self
    }

    /// Assert that an element has (or does not have) a specific state flag.
    pub fn assert_state(&self, id: ElementId, flag: StateFlags, expected: bool) -> &Self {
        let has = self
            .arena
            .get(id)
            .is_some_and(|el| el.state.get().contains(flag));
        assert_eq!(
            has, expected,
            "element {:?} state {:?} was {} (expected {})",
            id, flag, has, expected
        );
        self
    }

    /// Get a clone of the element's StyleComponent from the ECS tables.
    pub fn style_component_of(
        &self,
        id: ElementId,
    ) -> Option<crate::ecs::components::StyleComponent> {
        self.arena.component_tables.borrow().style.get(&id).cloned()
    }

    // ═══════════════ Selector-based queries ─────────────────────────

    /// Find the first element matching the selector in DFS (pre-order) tree
    /// order. Deterministic — does NOT depend on `FxHashMap` iteration order.
    pub fn find_sel<S: Into<Selector>>(&self, sel: S) -> Option<ElementId> {
        let sel = sel.into();
        fn walk(arena: &ElementArena, eid: ElementId, sel: &Selector) -> Option<ElementId> {
            if sel.matches(arena, eid) {
                return Some(eid);
            }
            if let Some(el) = arena.get(eid) {
                for &cid in &el.children {
                    if let Some(found) = walk(arena, cid, sel) {
                        return Some(found);
                    }
                }
            }
            None
        }
        walk(&self.arena, self.root_id, &sel)
    }

    /// Find all elements matching the selector in DFS (pre-order) tree order.
    pub fn find_all_sel<S: Into<Selector>>(&self, sel: S) -> Vec<ElementId> {
        let sel = sel.into();
        fn walk(arena: &ElementArena, eid: ElementId, sel: &Selector, out: &mut Vec<ElementId>) {
            if sel.matches(arena, eid) {
                out.push(eid);
            }
            if let Some(el) = arena.get(eid) {
                for &cid in &el.children {
                    walk(arena, cid, sel, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.arena, self.root_id, &sel, &mut out);
        out
    }

    // ═══════════════ Accessibility semantic finders ═════════════════
    //
    // testing-library / egui-kittest / Slint style: locate widgets by their
    // accessibility semantics (label, role), not by tree structure. `get_by_*`
    // asserts existence (panics with a clear message if none matched);
    // `query_by_*` returns `Option`.

    /// Query the first element whose accessible label EXACTLY equals `label`.
    pub fn query_by_label(&self, label: &str) -> Option<ElementId> {
        fn walk(arena: &ElementArena, eid: ElementId, label: &str) -> Option<ElementId> {
            if arena
                .get(eid)
                .and_then(|el| el.accessible_label())
                .as_deref()
                == Some(label)
            {
                return Some(eid);
            }
            if let Some(el) = arena.get(eid) {
                for &cid in &el.children {
                    if let Some(f) = walk(arena, cid, label) {
                        return Some(f);
                    }
                }
            }
            None
        }
        walk(&self.arena, self.root_id, label)
    }

    /// Get the element whose accessible label EXACTLY equals `label` (panics if none).
    pub fn get_by_label(&self, label: &str) -> ElementId {
        self.query_by_label(label).unwrap_or_else(|| {
            panic!("get_by_label: no element with accessible label == {label:?}")
        })
    }

    /// Get the first element whose accessible label CONTAINS `substr` (panics if none).
    pub fn get_by_label_contains(&self, substr: &str) -> ElementId {
        self.find_sel(crate::testing::selector::by_label(substr))
            .unwrap_or_else(|| {
                panic!("get_by_label_contains: no element with label containing {substr:?}")
            })
    }

    /// Get the first element with accessible role `role` (panics if none).
    pub fn get_by_role(&self, role: accesskit::Role) -> ElementId {
        self.find_sel(crate::testing::selector::by_role(role))
            .unwrap_or_else(|| panic!("get_by_role: no element with role {role:?}"))
    }

    /// All elements with accessible role `role`, in DFS order.
    pub fn get_all_by_role(&self, role: accesskit::Role) -> Vec<ElementId> {
        self.find_all_sel(crate::testing::selector::by_role(role))
    }

    // ── Selector-based actions (chainable) ──

    pub fn click_on<S: Into<Selector>>(&mut self, sel: S) -> &mut Self {
        if let Some(id) = self.find_sel(sel) {
            self.click(id);
        }
        self
    }

    pub fn type_into<S: Into<Selector>>(&mut self, sel: S, text: &str) -> &mut Self {
        if let Some(id) = self.find_sel(sel) {
            self.type_text(id, text);
        }
        self
    }

    // ── Selector-based assertions ──

    pub fn assert_text_sel<S: Into<Selector>>(&self, sel: S, expected: &str) -> &Self {
        let id = self
            .find_sel(sel)
            .expect("assert_text_sel: no element matched selector");
        self.assert_text(id, expected)
    }

    pub fn assert_visible_sel<S: Into<Selector>>(&self, sel: S) -> &Self {
        let id = self
            .find_sel(sel)
            .expect("assert_visible_sel: no element matched selector");
        self.assert_visible(id)
    }

    pub fn assert_not_visible_sel<S: Into<Selector>>(&self, sel: S) -> &Self {
        let id = self
            .find_sel(sel)
            .expect("assert_not_visible_sel: no element matched selector");
        self.assert_not_visible(id)
    }

    pub fn assert_focused_sel<S: Into<Selector>>(&self, sel: S) -> &Self {
        let id = self
            .find_sel(sel)
            .expect("assert_focused_sel: no element matched selector");
        self.assert_focused(id)
    }

    pub fn assert_dirty_sel<S: Into<Selector>>(&self, sel: S) -> &Self {
        let id = self
            .find_sel(sel)
            .expect("assert_dirty_sel: no element matched selector");
        self.assert_dirty(id)
    }

    // ═══════════════ Scene introspection ────────────────────────────

    /// Drain and return the paint commands from the most recent frame.
    pub fn drain_scene(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.last_scene)
    }

    /// Return a human-readable text dump of the last frame's paint commands.
    pub fn dump_scene(&self) -> String {
        let mut out = String::new();
        for (i, cmd) in self.last_scene.iter().enumerate() {
            use std::fmt::Write;
            let _ = writeln!(out, "{:3}  {}", i, describe_command(cmd));
        }
        out
    }

    /// Return a human-readable tree dump of the element hierarchy.
    pub fn dump_tree(&self) -> String {
        let mut out = String::new();
        self.write_subtree(self.root_id, 0, &mut out);
        out
    }

    fn write_subtree(&self, eid: ElementId, depth: usize, out: &mut String) {
        use std::fmt::Write;
        let Some(el) = self.arena.get(eid) else {
            return;
        };
        let indent = "  ".repeat(depth);
        let label = el.accessible_label().unwrap_or_default();
        let role = el
            .accessible_role()
            .map(|r| format!("{:?}", r))
            .unwrap_or_default();
        let tid = el
            .test_id()
            .map(|s| format!(" test_id=\"{}\"", s))
            .unwrap_or_default();
        let name = el
            .name()
            .map(|s| format!(" name=\"{}\"", s))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "{}<{} label=\"{}\" test_id=\"{}\" name=\"{}\" />",
            indent, role, label, tid, name
        );
        for &cid in &el.children {
            self.write_subtree(cid, depth + 1, out);
        }
    }

    /// Assert that the last scene contains at least one `FillRect` / `StrokeRect`
    /// whose axis-aligned bounds (in screen space) overlap `rect` within `tolerance`.
    pub fn assert_contains_rect(&self, rect: Rect) -> &Self {
        let found = self.last_scene.iter().any(|cmd| {
            let r = match cmd {
                DrawCommand::FillRect { rect, .. } | DrawCommand::StrokeRect { rect, .. } => *rect,
                _ => return false,
            };
            let overlap_x = r.x < rect.x + rect.width && r.x + r.width > rect.x;
            let overlap_y = r.y < rect.y + rect.height && r.y + r.height > rect.y;
            overlap_x && overlap_y
        });
        assert!(
            found,
            "assert_contains_rect: no overlapping rect found for {:?}",
            rect
        );
        self
    }

    // ═══════════════ Temporal assertions ────────────────────────────

    /// Returns `true` if any work remains: repaint-dirty elements,
    /// active animations, pending scheduler deadlines, or queued
    /// registry work (deferred tree mutations / signal-driven dirty)
    /// that has not yet been applied to element flags.
    pub fn has_active_work(&self) -> bool {
        self.dirty_count() > 0
            || self.animations.has_active()
            || crate::core::scheduler::any_active()
            || crate::core::dirty_registry::has_pending_actions()
            || crate::core::dirty_registry::has_pending_dirty()
    }

    /// Run frames until the system is quiescent (no dirty elements,
    /// no active animations, no pending scheduler work), or until
    /// `max_frames` is reached. Returns false if the limit was hit.
    pub fn settle(&mut self, max_frames: usize) -> bool {
        for i in 0..max_frames {
            if !self.has_active_work() {
                return true;
            }
            self.run_frame();
            if i == max_frames - 1 {
                return false;
            }
        }
        false
    }

    /// Run frames until `predicate` returns true, or `max_frames` is reached.
    /// Returns `true` if the predicate was satisfied.
    pub fn wait_until(&mut self, predicate: impl Fn(&Self) -> bool, max_frames: usize) -> bool {
        for i in 0..max_frames {
            if predicate(self) {
                return true;
            }
            self.run_frame();
            if i == max_frames - 1 {
                return false;
            }
        }
        false
    }
}

// ── Scene description helper ────────────────────────────────────────

fn describe_command(cmd: &DrawCommand) -> String {
    match cmd {
        DrawCommand::FillRect {
            rect,
            color,
            radius,
            z_index,
            ..
        } => format!(
            "FillRect(x={:.0},y={:.0},{}x{}, color={:?}, r={:?}, z={})",
            rect.x, rect.y, rect.width, rect.height, color, radius, z_index
        ),
        DrawCommand::StrokeRect {
            rect,
            color,
            width,
            z_index,
            ..
        } => format!(
            "StrokeRect(x={:.0},y={:.0},{}x{}, color={:?}, w={}, z={})",
            rect.x, rect.y, rect.width, rect.height, color, width, z_index
        ),
        DrawCommand::FillShadow {
            rect,
            color,
            z_index,
            ..
        } => format!(
            "FillShadow(x={:.0},y={:.0},{}x{}, color={:?}, z={})",
            rect.x, rect.y, rect.width, rect.height, color, z_index
        ),
        DrawCommand::FillLinearGradient { rect, z_index, .. } => format!(
            "FillLinearGradient(x={:.0},y={:.0},{}x{}, z={})",
            rect.x, rect.y, rect.width, rect.height, z_index
        ),
        DrawCommand::DrawImage { rect, z_index, .. } => format!(
            "DrawImage(x={:.0},y={:.0},{}x{}, z={})",
            rect.x, rect.y, rect.width, rect.height, z_index
        ),
        DrawCommand::FillPath { z_index, .. } => format!("FillPath(…) z={}", z_index),
        DrawCommand::StrokePath { z_index, .. } => format!("StrokePath(…) z={}", z_index),
    }
}
