//! `ApplicationHandler` implementation for `App` — winit event loop bridge.

use crate::core::error::{panic_to_string, push_error, UiError};
use crate::core::widget::Widget;
use raw_window_handle::HasWindowHandle;
use std::rc::Rc;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;

use super::app::{App, PENDING_WINDOWS};
use super::config::WindowConfig;
use super::handle::WindowHandle;
use super::WindowState;
use crate::event::action::Action;

impl App {
    pub(crate) fn create_window_inner(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        config: WindowConfig,
        widget: Box<dyn Widget>,
    ) {
        if let Some(max) = self.max_windows {
            if self.windows.len() >= max {
                #[cfg(any(feature = "devtools", feature = "file-logging"))]
                tracing::warn!("Window limit ({max}) reached; ignoring create_window call");
                return;
            }
        }
        let mut win_attrs = winit::window::WindowAttributes::default()
            .with_title(&config.title)
            .with_surface_size(winit::dpi::LogicalSize::new(
                config.width as f64,
                config.height as f64,
            ))
            .with_visible(false);

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            win_attrs = win_attrs
                .with_resizable(config.resizable)
                .with_decorations(config.decorations)
                .with_transparent(config.transparent)
                .with_maximized(config.maximized)
                .with_enabled_buttons(config.enabled_buttons.inner());
            if config.always_on_top {
                win_attrs = win_attrs.with_window_level(winit::window::WindowLevel::AlwaysOnTop);
            }
            if config.fullscreen {
                #[allow(unused_mut)]
                let mut fs_monitor: Option<winit::monitor::MonitorHandle> = None;
                #[cfg(feature = "display")]
                if let Some(ref m) = config.monitor {
                    fs_monitor = Some(m.inner().clone());
                }
                win_attrs = win_attrs
                    .with_fullscreen(Some(winit::monitor::Fullscreen::Borderless(fs_monitor)));
            }
            if let Some(ref icon) = config.window_icon {
                win_attrs = win_attrs.with_window_icon(Some(icon.to_winit_icon()));
            }
            if let Some((x, y)) = config.position {
                win_attrs =
                    win_attrs.with_position(winit::dpi::LogicalPosition::new(x as f64, y as f64));
            }
            if let Some(min_w) = config.min_width.or(config.min_height.map(|_| 0.0)) {
                let min_h = config.min_height.unwrap_or(0.0);
                win_attrs = win_attrs.with_min_surface_size(winit::dpi::LogicalSize::new(
                    min_w as f64,
                    min_h as f64,
                ));
            }
            if let Some(max_w) = config.max_width.or(config.max_height.map(|_| f32::MAX)) {
                let max_h = config.max_height.unwrap_or(f32::MAX);
                win_attrs = win_attrs.with_max_surface_size(winit::dpi::LogicalSize::new(
                    max_w as f64,
                    max_h as f64,
                ));
            }
        }

        #[cfg(all(
            feature = "display",
            not(any(target_os = "android", target_os = "ios"))
        ))]
        if config.position.is_none() {
            if let Some(ref monitor) = config.monitor {
                if let Some(pos) = monitor.position() {
                    let sf = monitor.scale_factor();
                    let logical_size =
                        winit::dpi::LogicalSize::new(config.width as f64, config.height as f64);
                    let physical_size: winit::dpi::PhysicalSize<f64> = logical_size.to_physical(sf);
                    let msize = monitor.size().unwrap_or((0, 0));
                    let cx = (pos.0 as f64 + (msize.0 as f64 - physical_size.width) / 2.0).max(0.0);
                    let cy =
                        (pos.1 as f64 + (msize.1 as f64 - physical_size.height) / 2.0).max(0.0);
                    win_attrs = win_attrs
                        .with_position(winit::dpi::PhysicalPosition::new(cx as i32, cy as i32));
                }
            }
        }

        let window = match event_loop.create_window(win_attrs) {
            Ok(w) => w,
            Err(e) => {
                push_error(UiError::WindowCreate(e.to_string()));
                return;
            }
        };
        let id = window.id();
        let sf = window.scale_factor();
        let win_arc: Arc<dyn winit::window::Window> = Arc::from(window);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let visible = config.visible;
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let visible = true;

        let mut state = WindowState::new(config);
        #[cfg(feature = "devtools")]
        {
            state.devtools_buf = Some(self.devtools_buf.clone());
        }
        state.scale_factor = sf;
        state.winit_window = Some(win_arc.clone());
        let raw_handle = match win_arc.window_handle() {
            Ok(h) => h.as_raw(),
            Err(e) => {
                push_error(UiError::SurfaceCreate(e.to_string()));
                return;
            }
        };
        state.a11y.init(raw_handle);
        state.window_handle = Some(WindowHandle {
            window: win_arc.clone(),
        });
        win_arc.set_visible(visible);
        {
            // Per-window wake: every register_dirty on THIS window's
            // AppContext requests a redraw of THIS window (audit 2026-07-18
            // multi-window pass — replaces the process-global ON_DIRTY slot).
            let w = win_arc.clone();
            state.app.set_on_dirty(Rc::new(move || {
                w.request_redraw();
            }));
        }
        state.ensure_renderer();
        state.mount_root(widget);

        #[cfg(feature = "tray")]
        if let Some(builder) = state.config.tray.take() {
            match builder.build() {
                Ok(tray) => state.tray_icon = Some(tray),
                Err(_e) => {
                    #[cfg(any(feature = "devtools", feature = "file-logging"))]
                    tracing::warn!(target: "burin", "[tray] failed to create tray icon: {_e}");
                }
            }
        }

        if let Some(ref w) = state.winit_window {
            w.request_redraw();
        }
        self.windows.insert(id, state);
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let pending: Vec<_> = self.pending.drain(..).collect();
        for (config, widget) in pending {
            self.create_window_inner(event_loop, config, widget);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let Some(state) = self.windows.get_mut(&window_id) {
            // Install this window's AppContext as the active one for the whole
            // event-handling scope.
            crate::core::app_context::set_current_app(&state.app);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.handle_event(event_loop, event);
            }));
            if let Err(panic) = result {
                let wid = state
                    .window_handle
                    .as_ref()
                    .map(|h| h.id().into_raw() as u64);
                push_error(UiError::CallbackPanic {
                    context: "window_event".into(),
                    window_id: wid,
                    element_id: None,
                    message: panic_to_string(&panic),
                });
                state.needs_rebuild = true;
            }
            if state.close_requested {
                if let Some(ref win) = state.winit_window {
                    win.set_visible(false);
                }
                self.windows.remove(&window_id);
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
        }

        // Drain pending windows queued by widget callbacks during this event.
        PENDING_WINDOWS.with(|q| {
            for (config, widget) in q.borrow_mut().drain(..) {
                self.create_window_inner(event_loop, config, widget);
            }
        });
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        crate::platform::wake::drain_ui_queue();

        // ── Global hotkey poll (system-global, works when app is backgrounded) ──
        #[cfg(feature = "global-hotkey")]
        {
            let actions = self.hotkey_manager.poll();
            if !actions.is_empty() {
                for state in self.windows.values_mut() {
                    if state.close_requested {
                        continue;
                    }
                    crate::core::app_context::set_current_app(&state.app);
                    if let Some(root_id) = state.arena.root_id {
                        let path = state.arena.path_to_root(root_id);
                        for action_kind in &actions {
                            let action = Action::new(*action_kind);
                            super::action::dispatch_action(state, &action, &path);
                        }
                    }
                }
            }
        }

        self.flush_scheduler.drain();
        auralis_task::drain_deferred_signal_callbacks();

        // ── Async timer bridge (audit 2026-07-17 round 5, A1) ──
        if auralis_task::next_timer_delay_ms() == Some(0) {
            auralis_task::flush_all();
            self.flush_scheduler.drain();
            auralis_task::drain_deferred_signal_callbacks();
        }

        // ── Multi-window dirty redistribution (audit 2026-07-18) ──
        let window_ids: Vec<winit::window::WindowId> = self.windows.keys().copied().collect();
        for wid in &window_ids {
            let Some(state) = self.windows.get(wid) else {
                continue;
            };
            if !state.app.has_foreign_dirty() {
                continue;
            }
            let orphans = state.app.take_foreign_dirty();
            for (eid, flags) in orphans {
                for target in self.windows.values() {
                    if target.arena.get(eid).is_some() {
                        target.app.register_dirty(eid, flags);
                        break;
                    }
                }
            }
        }

        for state in self.windows.values_mut() {
            if state.close_requested {
                continue;
            }
            crate::core::app_context::set_current_app(&state.app);
            state.app.reset_dirty_redraw();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.about_to_wait(event_loop);
            }));
            if let Err(panic) = result {
                push_error(UiError::CallbackPanic {
                    context: "about_to_wait".into(),
                    window_id: state
                        .window_handle
                        .as_ref()
                        .map(|h| h.id().into_raw() as u64),
                    element_id: None,
                    message: panic_to_string(&panic),
                });
                state.needs_rebuild = true;
            }
        }

        // ── Scheduler deadline folding ──
        let mut deadline: Option<web_time::Instant> = None;
        for state in self.windows.values() {
            crate::core::app_context::set_current_app(&state.app);
            if let Some(d) = crate::core::scheduler::next_deadline() {
                deadline = Some(deadline.map_or(d, |cur: web_time::Instant| cur.min(d)));
            }
        }
        if let Some(delay_ms) = auralis_task::next_timer_delay_ms() {
            let task_at = crate::core::clock::now() + std::time::Duration::from_millis(delay_ms);
            deadline = Some(deadline.map_or(task_at, |d| d.min(task_at)));
        }
        if let Some(deadline) = deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }

    fn suspended(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            state.skip_frame = true;
            if let Some(ref mut renderer) = state.renderer {
                renderer.destroy_surface();
            }
        }
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            state.skip_frame = false;
            state.ensure_renderer();
            if let Some(ref w) = state.winit_window {
                w.request_redraw();
            }
        }
    }

    fn destroy_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for state in self.windows.values_mut() {
            if let Some(ref mut renderer) = state.renderer {
                renderer.destroy_surface();
            }
        }
    }
}
