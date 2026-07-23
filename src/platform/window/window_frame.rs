use crate::core::config::StateFlags;
use crate::core::dirty_registry;
use crate::core::element::{apply_drag_layouts, reapply_element_theme, DirtyFlags};
use crate::core::error::{panic_to_string, push_error, UiError};
use crate::debug::FrameMetrics;
use crate::event::Event;
use crate::render::Painter;
use crate::style::{Rect, Size};
use std::time::Instant as StdInstant;
use super::drag;
use super::frame_hook::WindowFrameHook;
use super::window_state::WindowState;

impl WindowState {
    pub(crate) fn on_frame(&mut self) {
        crate::core::app_context::set_current_app(&self.app);
        crate::core::error::set_current_frame_id(Some(self.frame_id));
        // ── Freeze check: skip expensive work (layout, paint, animations) ──
        if self.app.is_frozen() {
            self.frame_id += 1;
            self.paint_occurred_this_frame = false;
            #[cfg(feature = "devtools")]
            if let Some(ref buf) = self.devtools_buf {
                if let Some(ref win) = self.winit_window {
                    let fps = self.metrics.latest().map(|m| m.fps).unwrap_or(0.0);
                    let snapshot = crate::debug::devtools::collect_frame_snapshot(
                        &self.arena,
                        self.frame_id,
                        0,
                        0,
                        fps,
                        0,
                        0,
                    );
                    crate::debug::devtools::push_snapshot(buf, win.id(), snapshot);
                }
            }
            crate::core::error::set_current_frame_id(None);
            return;
        }
        if self.needs_rebuild {
            self.rebuild_state();
        }
        if self.skip_frame {
            crate::core::error::set_current_frame_id(None);
            return;
        }
        #[cfg(feature = "backend-tiny-skia")]
        super::cpu_perf::announce_once();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let frame_start = StdInstant::now();
            self.frame_id += 1;

            // ── Perf activation (devtools auto-enables, otherwise opt-in via AURALIS_PERF=1) ──
            {
                use std::sync::OnceLock;
                static PERF_CHECKED: OnceLock<bool> = OnceLock::new();
                PERF_CHECKED.get_or_init(|| {
                    let enable = cfg!(feature = "devtools")
                        || std::env::var("AURALIS_PERF").is_ok_and(|v| v == "1");
                    if enable {
                        crate::core::perf::perf_enable();
                    }
                    enable
                });
            }
            crate::core::perf::perf_reset_frame();

            self.last_frame_instant = Some(frame_start);

            #[cfg(feature = "tray")]
            if let Some(ref mut tray) = self.tray_icon {
                tray.poll();
            }

            self.flush_scheduler.drain();
            auralis_task::drain_deferred_signal_callbacks();
            #[cfg(debug_assertions)]
            crate::core::dirty_registry::reset_stats();
            crate::core::dirty_registry::devtools_reset_dirty();

            let is_first_frame = self.frame_id == 1;
            let root_id = match self.arena.root_id {
                Some(rid) => rid,
                None => return,
            };

            // ── Theme reapply (pre-frame; forces a full relayout) ──
            if self.needs_theme_reapply {
                reapply_element_theme(&mut self.arena, root_id, &self.config.theme);
                self.needs_theme_reapply = false;
                self.needs_taffy = true;
            }
            let force_layout = self.needs_taffy;
            self.needs_taffy = false;

            // ── Frame input (divergence points explicit) ──
            let input = crate::core::frame_driver::FrameInput {
                size: Size::new(self.config.width, self.config.height),
                frame_id: self.frame_id,
                is_first_frame,
                force_layout,
                scale_factor: self.scale_factor as f32,
                bg: self.config.theme.scheme.surface,
                fg: self.config.theme.scheme.on_surface,
                highlight_mode: self.focus_manager.highlight_mode(),
                now: crate::core::clock::now(),
                scroll_friction: self.config.scroll_friction,
                scroll_stop_speed: self.config.scroll_stop_speed,
                skip_paint: self.coalesce_skip_paint,
            };

            // ── Phase 1: drive_frame_layout (kinetic → dirty → SEAM1 → layout) ──
            let mut hook = WindowFrameHook {
                config: &self.config,
                scale_factor: self.scale_factor,
                winit_window: self.winit_window.as_ref(),
            };
            let stage = {
                let st = crate::core::frame_driver::FrameState {
                    arena: &mut self.arena,
                    taffy: &mut self.taffy,
                    events: &mut self.event_registry,
                    animations: &mut self.animations,
                    focus: &mut self.focus_manager,
                    scroll_kinetic: &mut self.scroll_kinetic,
                    scroll_kinetic_target: &mut self.scroll_kinetic_target,
                };
                crate::core::frame_driver::drive_frame_layout(st, &input, &mut hook)
            };
            let root_id = stage.root_id;

            // ── SEAM 2 (shared platform-frame work: long-press wins, drag
            //   ghost, drag-z, autofocus, a11y dispatch — same code path as
            //   TestHarness::run_frame; audit round 4) ──
            {
                let st = crate::core::frame_driver::FrameState {
                    arena: &mut self.arena,
                    taffy: &mut self.taffy,
                    events: &mut self.event_registry,
                    animations: &mut self.animations,
                    focus: &mut self.focus_manager,
                    scroll_kinetic: &mut self.scroll_kinetic,
                    scroll_kinetic_target: &mut self.scroll_kinetic_target,
                };
                let args = crate::core::frame_driver::PlatformArgs {
                    drag_ghost: self
                        .drag_state
                        .as_ref()
                        .and_then(|ds| ds.ghost.map(|g| (g, ds.cursor))),
                    drag_elevated: &mut self.drag_elevated,
                };
                crate::core::frame_driver::drive_frame_platform(
                    st, &stage, &input, args, &mut hook,
                );
            }
            drop(hook);

            // ── Phase 2: drive_frame_paint (animation → paint → exits) ──
            let paint_start = StdInstant::now();
            let out = {
                let fcx = crate::core::frame_context::FrameContext::new(
                    &self.app,
                    &self.scene_cache,
                    &self.subtree_cache,
                );
                let st = crate::core::frame_driver::FrameState {
                    arena: &mut self.arena,
                    taffy: &mut self.taffy,
                    events: &mut self.event_registry,
                    animations: &mut self.animations,
                    focus: &mut self.focus_manager,
                    scroll_kinetic: &mut self.scroll_kinetic,
                    scroll_kinetic_target: &mut self.scroll_kinetic_target,
                };
                crate::core::frame_driver::drive_frame_paint(st, &fcx, &input, stage)
            };

            self.paint_occurred_this_frame = out.painted;

            // Startup probe — fires exactly once, on the first frame that
            // actually painted. env AURALIS_STARTUP_PROBE=1 enables it.
            #[cfg(feature = "backend-tiny-skia")]
            if self.paint_occurred_this_frame {
                static FIRED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !FIRED.swap(true, std::sync::atomic::Ordering::Relaxed)
                    && std::env::var("AURALIS_STARTUP_PROBE").is_ok()
                {
                    use std::sync::OnceLock;
                    static T0: OnceLock<std::time::Instant> = OnceLock::new();
                    let t0 = *T0.get_or_init(std::time::Instant::now);
                    eprintln!(
                        "[STARTUP] first painted frame (id={}) at {:.0} ms",
                        self.frame_id,
                        t0.elapsed().as_millis()
                    );
                    std::process::exit(0);
                }
            }

            if out.painted && self.renderer.is_some() {
                self.frame_stats_painted += 1;
                let repaint_ids = out.repaint_ids;
                let dmg_pr_n = out.paint_roots.len();
                let dmg_rp_n = repaint_ids.len();
                let mut commands = out.commands;
                let mut text_areas = out.text_areas;
                let backdrop_regions = out.backdrop_regions;

                if commands.is_empty() && text_areas.is_empty() {
                    // Skip present — nothing to render, avoid overwriting
                    // previously rendered content (e.g. during enter-animation frames).
                } else {
                    match &mut self.renderer {
                        #[cfg(feature = "backend-tiny-skia")]
                        Some(crate::render::BackendRenderer::Cpu(ref mut cpu)) => {
                            // gen phase = everything since paint_start (paint_element_tree + take_commands)
                            let gen_us = paint_start.elapsed().as_micros() as u64;
                            let cmd_count = commands.len();
                            let ta_count = text_areas.len();

                            // Only elements that actually have REPAINT set (filter
                            // out "repaint-boundary" solid-bg containers that are in
                            // paint_roots but whose own surface did not change).
                            // `repaint_ids` was captured BEFORE paint_element_tree
                            // cleared the dirty flags.
                            let pr_n = dmg_pr_n;
                            let rp_n = dmg_rp_n;
                            let viewport =
                                Rect::new(0.0, 0.0, self.config.width, self.config.height);
                            let damage_rects: Vec<Rect> = {
                                // Inflated (shadow/outline/transform-aware), disjoint
                                // damage rects with previous-position tracking —
                                // see render::cpu::damage (audit 2026-07-16 C5/C6).
                                let raw = if !repaint_ids.is_empty() {
                                    cpu.damage_tracker.compute(
                                        &repaint_ids,
                                        &self.arena,
                                        8,
                                        viewport,
                                    )
                                } else {
                                    vec![]
                                };
                                if raw.is_empty() {
                                    vec![viewport]
                                } else {
                                    raw
                                }
                            };

                            let dmg_n = damage_rects.len();
                            let dmg_frac = {
                                let win_area = (self.config.width * self.config.height).max(1.0);
                                let dmg_area: f32 =
                                    damage_rects.iter().map(|r| r.width * r.height).sum();
                                dmg_area / win_area
                            };

                            let t_raster = StdInstant::now();
                            cpu.render_damage(
                                &damage_rects,
                                &mut commands,
                                &mut text_areas,
                                &backdrop_regions,
                            );
                            let raster_us = t_raster.elapsed().as_micros() as u64;
                            let t_present = StdInstant::now();
                            cpu.present(&damage_rects);
                            let present_us = t_present.elapsed().as_micros() as u64;
                            cpu.end_frame();
                            super::cpu_perf::record(
                                gen_us,
                                raster_us,
                                present_us,
                                cmd_count,
                                ta_count,
                                dmg_n,
                                dmg_frac,
                                pr_n,
                                rp_n,
                                self.config.width,
                                self.config.height,
                            );
                        }
                        #[cfg(feature = "backend-wgpu")]
                        Some(crate::render::BackendRenderer::Gpu(ref mut gpu)) => {
                            match gpu.begin_frame() {
                                Ok(mut frame) => {
                                    gpu.draw_commands(
                                        &mut frame,
                                        &commands,
                                        &text_areas,
                                        &backdrop_regions,
                                        Size::new(self.config.width, self.config.height),
                                    );
                                    gpu.end_frame(frame);
                                }
                                Err(e) => {
                                    push_error(UiError::GpuRender(e.to_string()));
                                }
                            }
                        }
                        _ => {}
                    }
                } // else block for commands.is_empty() guard

                #[cfg(debug_assertions)]
                if self.frame_id > 2 {
                    let (dc, _ps) = crate::core::dirty_registry::stats();
                    let ec = crate::core::dirty_registry::element_count();
                    let now = StdInstant::now();
                    let layout_us = if crate::core::perf::perf_is_enabled() {
                        crate::core::perf::perf_take_frame().phases
                            [crate::core::perf::PerfPhase::Layout as usize]
                    } else {
                        0
                    };
                    self.metrics.push(FrameMetrics {
                        frame_id: self.frame_id,
                        element_count: ec,
                        dirty_measure_count: 0,
                        dirty_reposition_count: 0,
                        dirty_repaint_count: dc,
                        layout_time_us: layout_us,
                        paint_time_us: (now - paint_start).as_micros() as u64,
                        total_time_us: (now - frame_start).as_micros() as u64,
                        fps: {
                            let us = (now - frame_start).as_micros().max(1) as f32;
                            1_000_000.0 / us
                        },
                    });
                    let paint_us = (now - paint_start).as_micros() as u64;
                    let total_us = (now - frame_start).as_micros() as u64;
                    println!(
                        "[dirty-bench] frame#{} | tree={} | dirty={} | paint={}µs | total={}µs",
                        self.frame_id, ec, dc, paint_us, total_us
                    );
                    let hl_count = crate::core::dirty_registry::hittest_leaf_fallback_count();
                    if hl_count > 0 {
                        eprintln!(
                            "[hittest] hit_test_leaf O(N) fallback triggered {} time(s) this frame",
                            hl_count
                        );
                    }
                }
            }

            // ── Accessibility (caller-side; window uses its platform adapter) ──
            let focus_id = self.focus_manager.focused();
            if crate::core::dirty_registry::is_a11y_dirty() {
                self.a11y.update_if_active(|| {
                    crate::platform::build_accessibility_tree(&self.arena, root_id, focus_id)
                });
                crate::core::dirty_registry::clear_a11y_dirty();
            }

            self.focus_manager.check_alive(|_id| true);

            #[cfg(debug_assertions)]
            for w in crate::debug::drain_over_render_warnings() {
                eprintln!("{w}");
            }
            #[cfg(feature = "devtools")]
            {
                self.last_frame_us = frame_start.elapsed().as_micros() as u64;
                self.last_paint_us = paint_start.elapsed().as_micros() as u64;
            }
        }));
        if let Err(panic) = result {
            push_error(UiError::CallbackPanic {
                context: "on_frame".into(),
                window_id: self
                    .window_handle
                    .as_ref()
                    .map(|h| h.id().into_raw() as u64),
                element_id: None,
                message: panic_to_string(&panic),
            });
            self.needs_rebuild = true;
        }
        #[cfg(feature = "devtools")]
        if let Some(ref buf) = self.devtools_buf {
            if let Some(ref win) = self.winit_window {
                let fps = self.metrics.latest().map(|m| m.fps).unwrap_or_else(|| {
                    if let Some(prev) = self.last_frame_instant {
                        let us = prev.elapsed().as_micros().max(1) as f32;
                        1_000_000.0 / us
                    } else {
                        0.0
                    }
                });
                let snapshot = crate::debug::devtools::collect_frame_snapshot(
                    &self.arena,
                    self.frame_id,
                    0,
                    self.frame_stats_painted as usize,
                    fps,
                    self.last_frame_us,
                    self.last_paint_us,
                );
                let display_text =
                    format!(
                    "Frame #{} | {:.1} fps | {} elements | {} dirty | paint: {}µs | cache: {:.0}%",
                    self.frame_id, fps,
                    snapshot.element_count, snapshot.dirty_count,
                    snapshot.frame_timing.paint_total_us, snapshot.cache_stats.hit_rate(),
                );
                crate::debug::devtools::push_snapshot(buf, win.id(), snapshot);
                crate::debug::devtools::notify_display(display_text);
            }
        }
        crate::core::error::set_current_frame_id(None);
    }

    pub(crate) fn dispatch_events(&mut self, events: &[Event]) {
        for evt in events {
            match evt {
                Event::PointerDown { position, .. } => {
                    // Reuse the hit-test's ancestor path (B4) instead of
                    // rebuilding it via path_to_root.
                    let (hit, hit_path) = match self.primary_pressed() {
                        Some(t) => (Some(t), None),
                        None => match crate::event::hit_test::hit_test(&self.arena, *position) {
                            Some(r) => (Some(r.target), Some(r.path)),
                            None => (None, None),
                        },
                    };
                    if let Some(target) = hit {
                        // If element is draggable, start drag state — skip text_input capture
                        if self.arena.get(target).is_some_and(|el| el.draggable()) {
                            drag::start_drag(self, target, *position);
                        } else {
                            let path = hit_path.unwrap_or_else(|| self.arena.path_to_root(target));
                            let outcome = crate::event::propagation::dispatch_event(
                                &mut self.arena,
                                evt,
                                &path,
                                &mut self.focus_manager,
                                &mut self.event_registry,
                                self.translator.modifiers(),
                            );
                            // If gesture arena determined this is a drag, capture the element
                            if let Some(drag_elem) = outcome.drag_winner {
                                self.drag_text_input = Some(drag_elem);
                            } else {
                                self.drag_text_input = Some(target);
                            }
                        }
                    }
                    if self.arena.root_id.is_some() {
                        // Overlay dismiss (portal system — Select, ComboBox, etc.)
                        crate::platform::portal::fire_dismiss(&self.arena, *position);
                    }
                }
                Event::PointerMove { position, .. } => {
                    let has_drag = self.drag_state.is_some();
                    if has_drag {
                        drag::update_drag_cursor(self, *position);
                    } else if let Some(id) = self.drag_text_input {
                        let path = self.arena.path_to_root(id);
                        let modifiers = self.translator.modifiers();
                        let _outcome = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                Event::PointerUp { position: _, .. } => {
                    if let Some(id) = self.drag_text_input {
                        let path = self.arena.path_to_root(id);
                        let modifiers = self.translator.modifiers();
                        let _outcome = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                        if let Some(el) = self.arena.get_mut(id) {
                            el.set_state_dirty(StateFlags::PRESSED, false);
                        }
                    }
                    drag::end_drag(self);
                    self.drag_text_input = None;
                }
                Event::Click {
                    position,
                    finger_id,
                    ..
                } => {
                    // Gesture-arena click suppression (mobile-groundwork
                    // W2): a non-Tap win (scroll/drag/long-press) on this
                    // pointer sequence eats the synthesized Click —
                    // scrolling over a button must not press it.
                    let pid = finger_id.unwrap_or(0);
                    if crate::event::recognizer::take_click_suppressed(pid) {
                        continue;
                    }
                    let modifiers = self.translator.modifiers();
                    if let Some(result) = crate::event::hit_test::hit_test(&self.arena, *position) {
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &result.path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                    crate::platform::portal::fire_dismiss(&self.arena, *position);
                }
                Event::KeyDown { .. } | Event::KeyUp { .. } => {
                    if let Some(fid) = self.focus_manager.focused() {
                        let path = self.arena.path_to_root(fid);
                        let modifiers = self.translator.modifiers();
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            evt,
                            &path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                Event::DragEnd { .. } => {
                    if self.drag_state.is_some() {
                        drag::end_drag(self);
                    } else if let Some(pid) = self.primary_pressed() {
                        let container_id = dirty_registry::parent_of(pid);
                        if let Some(cid) = container_id {
                            apply_drag_layouts(&mut self.arena, cid);
                            if let Some(el) = self.arena.get_mut(cid) {
                                el.dirty.set(el.dirty.get() | DirtyFlags::MEASURE);
                            }
                            dirty_registry::register_dirty(cid, DirtyFlags::MEASURE);
                        }
                        if let Some(state) = self.touches.get_mut(&0) {
                            state.pressed = None;
                        }
                    }
                }
                Event::DragCancel { .. } => {
                    drag::end_drag(self);
                    self.drag_text_input = None;
                }
                Event::DragStart { .. } | Event::DragMove { .. } => {
                    // Handled via PointerDown/Move capture + drag_text_input routing;
                    // DragEnd is handled above (applies drag layouts).
                }
                _ => {}
            }
        }
    }

    #[allow(dead_code)]
    pub fn animations(&mut self) -> &mut crate::animation::AnimationDriver {
        &mut self.animations
    }
    fn rebuild_state(&mut self) {
        self.needs_rebuild = false;
        self.needs_taffy = true;
        self.frame_id = 0;
        self.scroll_kinetic = None;
        self.drag_state = None;
        self.touches.clear();
        self.focus_manager.clear();
        crate::core::dirty_registry::mark_all_dirty();
        self.skip_frame = false;
    }
    #[allow(dead_code)]
    pub fn painter(&self) -> Painter {
        Painter::new(Rect::new(0.0, 0.0, self.config.width, self.config.height))
    }
}
