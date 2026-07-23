use crate::core::config::StateFlags;
use crate::core::dirty_registry;
use crate::event::FocusReason;
use super::ime;
use super::window_state::WindowState;

impl WindowState {
    pub(crate) fn about_to_wait(&mut self, _event_loop: &dyn winit::event_loop::ActiveEventLoop) {
        if self.needs_cleanup {
            self.touches.clear();
            self.drag_state = None;
            self.scrollbar_drag = None;
            self.focus_manager.clear();
            self.needs_cleanup = false;
        }

        self.flush_scheduler.drain();
        auralis_task::drain_deferred_signal_callbacks();

        let deferred = self.focus_manager.drain_deferred_blurs();
        for fid in deferred {
            if let Some(el) = self.arena.get_mut(fid) {
                el.set_state_dirty(StateFlags::FOCUSED, false);
                el.last_focus_reason.set(Some(FocusReason::PointerClick));
            }
            self.event_registry
                .fire_focus_out(fid, FocusReason::PointerClick);
            if self.focus_manager.focused() == Some(fid) {
                self.focus_manager.set_focused(None);
            }
        }

        let Some(root_id) = self.arena.root_id else {
            if let Some(ref w) = self.winit_window {
                w.request_redraw();
            }
            return;
        };
        if dirty_registry::has_pending_dirty() {
            self.on_frame();
            while dirty_registry::has_pending_dirty() {
                self.coalesce_skip_paint = true;
                self.on_frame();
            }
            self.coalesce_skip_paint = false;
            if self
                .arena
                .get(root_id)
                .is_some_and(|r| r.dirty.get().has_repaint())
            {
                self.on_frame();
            }
        }
        // ── Scheduler: reconcile this window's feature-level continuous
        //    subscriptions. Keyed acquire/release (audit 2026-07-18
        //    animation pass) — widget-held keys (toast transitions,
        //    spinners) are never touched here, so no owner can evict
        //    another owner's wake. Discrete deadlines (tooltip, blink)
        //    are handled by WaitUntil in WinState::about_to_wait and
        //    expired_discrete here — no per-frame busy-pump.
        {
            use crate::core::scheduler::{self, keys};
            // Renewal sweep: element wakes (spinners) decay unless their
            // frame_tick renewed them this turn (hidden ticks are skipped
            // by the tick pass, so hiding stops the renewal).
            scheduler::sweep_stale_element_wakes();
            // Visibility-gated: fully-hidden animations let the loop sleep;
            // the reactive_visible flip that reveals them registers dirty,
            // which wakes the loop and resumes ticking at the right phase.
            if self.animations.has_active_visible(&self.arena) {
                scheduler::acquire_continuous(keys::ANIM_DRIVER);
            } else {
                scheduler::release_continuous(keys::ANIM_DRIVER);
            }
            if crate::core::dirty_registry::is_exit_pending_active() {
                scheduler::acquire_continuous(keys::EXIT_ANIMS);
            } else {
                scheduler::release_continuous(keys::EXIT_ANIMS);
            }
        }

        // ── IME cursor area sync (P1): after the frame settles, tell the
        //    OS where the composition caret sits. Runs once per frame,
        //    O(1), with dedup so we don't spam the platform.
        ime::sync_ime_cursor_area(self);

        let needs_redraw = self.paint_occurred_this_frame
            || crate::core::scheduler::has_continuous()
            || crate::core::scheduler::expired_discrete()
            || dirty_registry::has_pending_dirty()
            // Liveness: a defer_action queued outside the frame (event
            // callback) without an accompanying register_dirty would
            // otherwise sit until the next incidental frame. TestHarness
            // already checks this (test_harness.rs quiescence gate) — the
            // real window must match, or harness-green code can still
            // freeze UI updates in production. (audit 2026-07-15, C4)
            || crate::core::dirty_registry::has_pending_actions();
        self.paint_occurred_this_frame = false;
        if needs_redraw {
            if let Some(ref w) = self.winit_window {
                w.request_redraw();
            }
        }
    }
}
