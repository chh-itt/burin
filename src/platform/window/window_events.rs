use crate::core::config::StateFlags;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::error::{panic_to_string, push_error, UiError};
use crate::core::ElementId;
use crate::event::action::Action;
use crate::event::FocusReason;
use crate::style::{Color, Point, Rect, Vec2};
use crate::theme::M3Theme;
use std::time::Instant as StdInstant;
use super::action;
use super::finger;
use super::frame_hook::{SUBMENU_DELAY_MS, SUBMENU_TIMER_KEY};
use super::ime;
use super::submenu;
use super::winit_map::{map_mouse_button, map_touch_phase, map_winit_key, map_winit_action_key};
use super::window_state::WindowState;

impl WindowState {
    pub(crate) fn handle_event(
        &mut self,
        _event_loop: &dyn winit::event_loop::ActiveEventLoop,
        event: winit::event::WindowEvent,
    ) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let win = self.winit_window.clone();
            let rid = self.arena.root_id;

            match event {
                winit::event::WindowEvent::CloseRequested => {
                    self.renderer.take();
                    self.close_requested = true;
                }
                winit::event::WindowEvent::RedrawRequested => {
                    #[cfg(debug_assertions)]
                    if self.frame_id == 0 {
                        eprintln!("[event] first RedrawRequested received");
                    }
                    self.on_frame();
                }
                winit::event::WindowEvent::SurfaceResized(size) => {
                    let sf = self.scale_factor as f32;
                    self.config.width = size.width as f32 / sf;
                    self.config.height = size.height as f32 / sf;
                    self.needs_taffy = true;
                    #[cfg(feature = "devtools")]
                    crate::debug::devtools::record_interaction(
                        self.frame_id,
                        crate::debug::devtools::InteractionKind::Resize {
                            width: self.config.width,
                            height: self.config.height,
                        },
                        auralis_signal::now_us(),
                    );
                    if let Some(ref mut r) = self.renderer {
                        r.resize_gpu(size.width, size.height);
                        r.resize_cpu(self.config.width, self.config.height, sf);
                    }
                    if let Some(ref w) = win {
                        w.request_redraw();
                    }
                }
                winit::event::WindowEvent::PointerMoved {
                    position, source, ..
                } => {
                    let sf = self.scale_factor as f32;
                    let lx = position.x as f32 / sf;
                    let ly = position.y as f32 / sf;
                    #[cfg(feature = "devtools")]
                    crate::debug::devtools::record_interaction(
                        self.frame_id,
                        crate::debug::devtools::InteractionKind::PointerMove { x: lx, y: ly },
                        auralis_signal::now_us(),
                    );
                    let pos = Point::new(lx, ly);
                    let (state_key, event_fid) = finger::finger_id_from_source(&source);
                    self.last_cursor = Some(pos);

                    let old_chain: Vec<ElementId> = self
                        .touches
                        .get_mut(&state_key)
                        .map(|s| std::mem::take(&mut s.hovered_chain))
                        .unwrap_or_default();
                    let new_chain =
                        if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                            result.path
                        } else {
                            Vec::new()
                        };

                    // Chains are leaf-first ancestor paths to the same root, so
                    // their set intersection is exactly the common SUFFIX —
                    // diffing is O(depth), not the old O(depth²) contains() scan
                    // (which also iterated .rev() while claiming "deepest first").
                    let common = {
                        let mut n = 0;
                        while n < old_chain.len()
                            && n < new_chain.len()
                            && old_chain[old_chain.len() - 1 - n]
                                == new_chain[new_chain.len() - 1 - n]
                        {
                            n += 1;
                        }
                        n
                    };

                    // Fire leave on elements in old but not new, deepest first.
                    for &eid in &old_chain[..old_chain.len() - common] {
                        {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                            // Clear pending submenu timer if leaving the item
                            crate::core::app_context::current_app().hovered_submenu_with(|s| {
                                if s.as_ref().is_some_and(|(id, _)| *id == eid)
                                    && !crate::widgets::overlay::is_submenu_recently_opened()
                                {
                                    *s = None;
                                    crate::core::scheduler::cancel(SUBMENU_TIMER_KEY);
                                }
                            });
                        }
                    }
                    // Fire enter on elements in new but not old, deepest first.
                    for &eid in &new_chain[..new_chain.len() - common] {
                        {
                            if dirty_registry::is_element_or_ancestor_disabled(eid) {
                                continue;
                            }
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, true);
                            }
                            self.event_registry.fire_hover_enter(eid);
                            // Arm the hover-to-open-submenu timer ONLY for rows that
                            // belong to an already-open menu — not for ordinary host
                            // widgets (e.g. a Table) that merely carry ContextMenuItems.
                            if self
                                .arena
                                .get(eid)
                                .and_then(|el| {
                                    el.get_user_data::<crate::widgets::overlay::ContextMenuItems>()
                                })
                                .is_some()
                                && crate::widgets::overlay::row_belongs_to_open_menu(eid)
                            {
                                let keep = crate::widgets::overlay::is_submenu_recently_opened()
                                    && crate::core::app_context::current_app()
                                        .hovered_submenu_with(|s| s.is_some());
                                if !keep {
                                    crate::core::app_context::current_app().set_hovered_submenu(
                                        Some((eid, crate::core::clock::now())),
                                    );
                                    // Schedule a frame at the deadline so
                                    // on_after_dirty can check elapsed() even
                                    // when the app is otherwise idle.
                                    crate::core::scheduler::schedule_at(
                                        crate::core::clock::now()
                                            + std::time::Duration::from_millis(
                                                SUBMENU_DELAY_MS as u64,
                                            ),
                                        SUBMENU_TIMER_KEY,
                                    );
                                }
                            }
                        }
                    }

                    // Apply cursor from deepest hovered element
                    let new_cursor = new_chain
                        .iter()
                        .filter_map(|&eid| self.arena.get(eid).and_then(|el| el.cursor_icon()))
                        .next()
                        .unwrap_or(crate::platform::CursorIcon::DEFAULT);
                    if let Some(ref wh) = self.window_handle {
                        wh.set_cursor(new_cursor);
                    }

                    // Screen-position check covers the parent–submenu gap.
                    crate::widgets::overlay::update_submenu_autoclose(
                        &mut self.arena,
                        pos,
                        &new_chain,
                    );

                    let state = self.finger_state(state_key);
                    state.position = pos;
                    state.hovered_chain = new_chain;

                    // Scrollbar drag update.
                    if let Some(ref drag) = self.scrollbar_drag {
                        crate::widgets::overlay::dismiss_context_menu_immediate(&mut self.arena);
                        let eid = drag.0;
                        let axis = drag.1;
                        let gf = drag.2;
                        let sb = self
                            .arena
                            .get(eid)
                            .map(|el| el.screen_bounds)
                            .unwrap_or(Rect::ZERO);
                        let sc = self.arena.comp_scroll(eid);
                        let lc = self.arena.comp_layout(eid);
                        if let Some(sc) = sc {
                            let own_so = sc.scroll_offset.get();
                            let cb = sc.content_bounds.get();
                            let sbw = lc.map_or(10.0, |l| l.scrollbar_width);
                            let cw = cb.width.max(1.0);
                            let ch = cb.height.max(1.0);
                            let (asx, asy) = crate::core::dirty_registry::accumulated_scroll_cached(
                                &self.arena,
                                eid,
                            );
                            let ancestor_x = asx - own_so.x;
                            let ancestor_y = asy - own_so.y;

                            // Try to use ScrollBundle for physics-aware scrollbar
                            let via_bundle = crate::widgets::bundle::scroll::try_set_offset(
                                &self.arena,
                                eid,
                                |bundle, vp| match axis {
                                    crate::widgets::bundle::scroll::ScrollAxis::Vertical if ch > sb.height => {
                                        let thumb_h = (sb.height / ch * sb.height).max(20.0);
                                        let trk = sb.height - thumb_h;
                                        let adj = pos.y + ancestor_y - sb.y - gf * thumb_h;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (ch - sb.height);
                                        let mut v = own_so;
                                        v.y = off;
                                        bundle.set_offset_with_physics(v, vp);
                                    }
                                    crate::widgets::bundle::scroll::ScrollAxis::Horizontal if cw > sb.width => {
                                        let h_gutter = if ch > sb.height { sbw + 2.0 } else { 0.0 };
                                        let thumb_w = (sb.width / cw * sb.width).max(20.0);
                                        let trk = sb.width - thumb_w - h_gutter;
                                        let adj = pos.x + ancestor_x - sb.x - gf * thumb_w;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (cw - sb.width);
                                        let mut v = own_so;
                                        v.x = off;
                                        bundle.set_offset_with_physics(v, vp);
                                    }
                                    _ => {}
                                },
                            );

                            // Fallback: direct ECS write if no ScrollBundle available
                            if !via_bundle {
                                match axis {
                                    crate::widgets::bundle::scroll::ScrollAxis::Vertical if ch > sb.height => {
                                        let thumb_h = (sb.height / ch * sb.height).max(20.0);
                                        let trk = sb.height - thumb_h;
                                        let adj = pos.y + ancestor_y - sb.y - gf * thumb_h;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (ch - sb.height);
                                        let mut v = sc.scroll_offset.get();
                                        v.y = off;
                                        sc.scroll_offset.set(v);
                                        dirty_registry::spatial_update_scroll(
                                            eid, v.x, v.y,
                                        );
                                    }
                                    crate::widgets::bundle::scroll::ScrollAxis::Horizontal if cw > sb.width => {
                                        let h_gutter = if ch > sb.height { sbw + 2.0 } else { 0.0 };
                                        let thumb_w = (sb.width / cw * sb.width).max(20.0);
                                        let trk = sb.width - thumb_w - h_gutter;
                                        let adj = pos.x + ancestor_x - sb.x - gf * thumb_w;
                                        let frac = (adj / trk.max(1.0)).clamp(0.0, 1.0);
                                        let off = frac * (cw - sb.width);
                                        let mut v = sc.scroll_offset.get();
                                        v.x = off;
                                        sc.scroll_offset.set(v);
                                        dirty_registry::spatial_update_scroll(
                                            eid, v.x, v.y,
                                        );
                                    }
                                    _ => {}
                                }
                                if let Some(el) = self.arena.get(eid) {
                                    el.mark_repaint();
                                }
                            }
                        }
                    }

                    // Scrollbar hover tracking (single combined scan — audit 2026-07-17).
                    self.scrollbar_hover = None;
                    if rid.is_some() && self.scrollbar_drag.is_none() {
                        if let Some(hit) =
                            crate::widgets::bundle::scroll::hit_scrollbar(&self.arena, pos)
                        {
                            self.scrollbar_hover = Some((hit.eid, hit.axis));
                        }
                    }
                    // Override cursor icon when hovering over a scrollbar (thumb or track)
                    if self.scrollbar_hover.is_some() {
                        if let Some(ref wh) = self.window_handle {
                            wh.set_cursor(crate::platform::CursorIcon::DEFAULT);
                        }
                    }

                    let events = self.translator.cursor_moved(lx, ly, event_fid);
                    let _prev = dirty_registry::current_trigger();
                    dirty_registry::set_current_trigger(
                        dirty_registry::DirtyTriggerTag::PointerEvent,
                    );
                    self.dispatch_events(&events);
                    dirty_registry::set_current_trigger(_prev);
                }
                winit::event::WindowEvent::PointerButton { state, button, .. } => {
                    let pressed = state == winit::event::ElementState::Pressed;
                    let (state_key, event_fid) = finger::finger_id_from_button(&button);
                    let btn = button
                        .mouse_button()
                        .map(map_mouse_button)
                        .unwrap_or(crate::event::MouseButton::Left);
                    #[cfg(feature = "devtools")]
                    if let Some(ref pos) = self.last_cursor {
                        let raw_button: u32 = match btn {
                            crate::event::MouseButton::Left => 1,
                            crate::event::MouseButton::Right => 2,
                            crate::event::MouseButton::Middle => 3,
                            crate::event::MouseButton::Back => 4,
                            crate::event::MouseButton::Forward => 5,
                            crate::event::MouseButton::Other(v) => v as u32,
                        };
                        let kind = if pressed {
                            crate::debug::devtools::InteractionKind::PointerDown {
                                x: pos.x,
                                y: pos.y,
                                button: raw_button,
                                modifiers: crate::debug::devtools::modifiers_to_u32(
                                    self.translator.modifiers(),
                                ),
                            }
                        } else {
                            crate::debug::devtools::InteractionKind::PointerUp {
                                x: pos.x,
                                y: pos.y,
                                button: raw_button,
                                modifiers: crate::debug::devtools::modifiers_to_u32(
                                    self.translator.modifiers(),
                                ),
                            }
                        };
                        crate::debug::devtools::record_interaction(
                            self.frame_id,
                            kind,
                            auralis_signal::now_us(),
                        );
                    }

                    let hover_from_move = self
                        .touches
                        .get(&state_key)
                        .and_then(|s| s.hovered_chain.first().copied());
                    let old_pressed = self.touches.get(&state_key).and_then(|s| s.pressed);
                    let gesture_target = old_pressed;

                    // On mouse DOWN, perform a fresh hit test so PRESSED state is
                    // set on the correct element even without a prior PointerMove.
                    let old_hovered = if pressed {
                        if let Some(ref pos) = self.last_cursor {
                            crate::event::hit_test::hit_test(&self.arena, *pos)
                                .map(|r| r.target)
                                .or(hover_from_move)
                        } else {
                            hover_from_move
                        }
                    } else {
                        hover_from_move
                    };

                    let mut new_pressed: Option<ElementId> = None;

                    if pressed {
                        let mut scrollbar_return = false;
                        if let Some(ref pos) = self.last_cursor {
                            if rid.is_some() {
                                if let Some(hit) =
                                    crate::widgets::bundle::scroll::hit_scrollbar(&self.arena, *pos)
                                {
                                    crate::widgets::overlay::dismiss_context_menu_immediate(
                                        &mut self.arena,
                                    );
                                    self.scroll_kinetic = None;
                                    if hit.on_thumb {
                                        self.scrollbar_drag =
                                            Some((hit.eid, hit.axis, hit.fraction));
                                    } else {
                                        crate::widgets::bundle::scroll::scrollbar_jump_to(
                                            &self.arena,
                                            hit.eid,
                                            hit.axis,
                                            hit.fraction,
                                        );
                                        self.scrollbar_drag = Some((hit.eid, hit.axis, 0.5));
                                    }
                                    self.scrollbar_hover = Some((hit.eid, hit.axis));
                                    scrollbar_return = true;
                                }
                            }
                        }
                        if scrollbar_return {
                            // Clear pressed state to prevent stale primary_pressed() values
                            // from blocking subsequent PointerDown dispatch.
                            self.touches.entry(state_key).or_default().pressed = None;
                            return;
                        }
                        new_pressed = old_hovered;

                        let old_focus = self.focus_manager.focused();
                        let focus_target = old_hovered.and_then(|hit| {
                            let path = self.arena.path_to_root(hit);
                            path.iter().find_map(|&id| {
                                self.arena.get(id).and_then(|el| {
                                    if el.is_focusable() {
                                        Some(id)
                                    } else {
                                        None
                                    }
                                })
                            })
                        });

                        const REASON: FocusReason = FocusReason::PointerClick;
                        if let Some(hit_id) = focus_target {
                            if old_focus != Some(hit_id) {
                                if let Some(old_id) = old_focus {
                                    // Flush IME composition into the old TextInput
                                    // before focus transfers to the new element.
                                    self.event_registry.commit_preedit(old_id);
                                    if let Some(el) = self.arena.get_mut(old_id) {
                                        el.set_state_dirty(StateFlags::FOCUSED, false);
                                        el.last_focus_reason.set(Some(REASON));
                                    }
                                    self.event_registry.fire_focus_out(old_id, REASON);
                                }
                                if let Some(el) = self.arena.get_mut(hit_id) {
                                    el.set_state_dirty(StateFlags::FOCUSED, true);
                                    el.last_focus_reason.set(Some(REASON));
                                }
                                self.event_registry.fire_focus_in(hit_id, REASON);
                                self.focus_manager.set_focused(Some(hit_id));
                                if let Some(ref w) = win {
                                    ime::request_ime_enable(w, &self.event_registry, hit_id);
                                }
                            }
                        } else if let Some(old_id) = old_focus {
                            // Only defer blur if the element has an active IME
                            // composition. Otherwise, blur immediately to avoid
                            // frame-by-frame focus oscillation.
                            if self.event_registry.has_text_input(old_id) {
                                // On focus loss (click outside), do NOT commit
                                // the IME preedit — the user hasn't confirmed the
                                // composition. Instead, just clear the local
                                // composition state so the TextInput's visual is
                                // clean. The OS IME keeps its state; when the user
                                // re-focuses and commits, the composed text appears
                                // as expected (matching WeChat, VS Code, etc.).
                                //
                                // For focus TRANSFER (clicking another TextInput),
                                // commit_preedit is called above to flush text into
                                // the old element before focus moves.
                                self.event_registry
                                    .fire_ime_preedit(old_id, String::new(), None);
                                self.focus_manager.defer_blur(old_id);
                            } else {
                                if let Some(el) = self.arena.get_mut(old_id) {
                                    el.set_state_dirty(StateFlags::FOCUSED, false);
                                    el.last_focus_reason.set(Some(FocusReason::PointerClick));
                                }
                                self.event_registry
                                    .fire_focus_out(old_id, FocusReason::PointerClick);
                                self.focus_manager.set_focused(None);
                            }
                        }
                    }

                    let press_target = if pressed { old_hovered } else { old_pressed };
                    if let Some(target_id) = press_target {
                        if !dirty_registry::is_element_or_ancestor_disabled(target_id) {
                            if let Some(el) = self.arena.get_mut(target_id) {
                                el.set_state_dirty(StateFlags::PRESSED, pressed);
                            }
                        }
                    }
                    if !pressed {
                        self.scrollbar_drag = None;
                        self.scrollbar_hover = None;
                        // Force-clear pressed after scrollbar drag to prevent
                        // stale primary_pressed() from blocking PointerDown dispatch.
                        if let Some(state) = self.touches.get_mut(&state_key) {
                            state.pressed = None;
                        }
                        // Fire hover-leave and re-hover on next PointerMoved
                        let old_chain: Vec<ElementId> = self
                            .touches
                            .get(&state_key)
                            .map_or(Vec::new(), |s| s.hovered_chain.clone());
                        for &eid in old_chain.iter().rev() {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                        }
                        if let Some(state) = self.touches.get_mut(&state_key) {
                            // Keep the leaf-most entry so old_hovered is available
                            // for the next pointer_down without PointerMoved.
                            let last = state.hovered_chain.last().copied();
                            state.hovered_chain.clear();
                            if let Some(eid) = last {
                                state.hovered_chain.push(eid);
                            }
                        }
                    }

                    let events = self.translator.mouse_input(pressed, btn, event_fid);
                    if pressed {
                        if let Some(ref pos) = self.last_cursor {
                            self.click_counter
                                .pointer_down(*pos, crate::core::clock::now());
                        }
                    } else {
                        if let Some(ref pos) = self.last_cursor {
                            match self
                                .click_counter
                                .pointer_up(*pos, crate::core::clock::now())
                            {
                                crate::event::ClickResult::Double { .. } => {
                                    if let Some(id) = gesture_target {
                                        self.event_registry.fire_double_click(id);
                                    }
                                }
                                crate::event::ClickResult::Triple { .. } => {
                                    if let Some(id) = gesture_target {
                                        self.event_registry.fire_triple_click(id);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // Publish the freshly-hit press target BEFORE dispatching, so the
                    // PointerDown handler's drag routing (which reads primary_pressed()
                    // to fire drag_start / set drag_text_input) sees THIS press, not the
                    // previous one — otherwise a drag widget's first drag is lost (the
                    // drag is routed to the previously-pressed element). Releases keep
                    // the old pressed until after dispatch so the DragEnd handler can
                    // still resolve the element being released.
                    if pressed {
                        self.touches.entry(state_key).or_default().pressed = new_pressed;
                    }
                    let _prev = dirty_registry::current_trigger();
                    dirty_registry::set_current_trigger(
                        dirty_registry::DirtyTriggerTag::PointerEvent,
                    );
                    self.dispatch_events(&events);
                    dirty_registry::set_current_trigger(_prev);
                    // Right-click context menu: walk the hit-test path and open
                    // the first element with a context_menu attached.
                    if !pressed && btn == crate::event::MouseButton::Right {
                        if let Some(ref pos) = self.last_cursor {
                            let rid = self.arena.root_id;
                            if let Some(result) =
                                crate::event::hit_test::hit_test(&self.arena, *pos)
                            {
                                for &eid in &result.path {
                                    let items = self.arena.get(eid)
                                    .and_then(|el| el.get_user_data::<crate::widgets::overlay::ContextMenuItems>().cloned());
                                    if let Some(cmi) = items {
                                        if let Some(root_id) = rid {
                                            let sw = self.config.width / self.scale_factor as f32;
                                            let sh = self.config.height / self.scale_factor as f32;
                                            let x = pos.x.min(sw - 220.0).max(0.0);
                                            let y = if pos.y + 100.0 > sh {
                                                (pos.y - 100.0).max(0.0)
                                            } else {
                                                pos.y
                                            };
                                            let adjusted = Point::new(x, y);
                                            crate::widgets::overlay::open_context_menu(
                                                cmi.0,
                                                adjusted,
                                                &mut self.arena,
                                                root_id,
                                                Some(&mut self.event_registry),
                                                None,
                                                false,
                                                sh,
                                            ); // root menu (no parent, opens right)
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    // Left-click on a context menu item that has submenu children:
                    // open the nested menu at the right edge of the clicked item.
                    // Dismiss only if the click is outside ALL open menus.
                    if !pressed && btn == crate::event::MouseButton::Left {
                        let mut hit_menu = false;
                        if let Some(ref pos) = self.last_cursor {
                            let rid = self.arena.root_id;
                            if let Some(result) =
                                crate::event::hit_test::hit_test(&self.arena, *pos)
                            {
                                for &eid in &result.path {
                                    // Only treat a ContextMenuItems-carrying element as a
                                    // submenu trigger when it actually lives inside an OPEN
                                    // menu. An ordinary widget that merely set .context_menu()
                                    // (e.g. a Table with a right-click menu) also carries
                                    // ContextMenuItems; without this guard a LEFT-click on it
                                    // spuriously opens its menu and steals focus. Mirrors the
                                    // hover-path guard (`row_belongs_to_open_menu`).
                                    if !crate::widgets::overlay::row_belongs_to_open_menu(eid) {
                                        continue;
                                    }
                                    let has_submenu = self.arena.get(eid)
                                    .and_then(|el| el.get_user_data::<crate::widgets::overlay::ContextMenuItems>().cloned());
                                    if let Some(sub) = has_submenu {
                                        hit_menu = true;
                                        let sb = self.arena.get(eid).map(|el| el.screen_bounds);
                                        if let (Some(sb), Some(root_id)) = (sb, rid) {
                                            let parent =
                                                crate::core::dirty_registry::parent_of(eid);
                                            let prefer_left = parent
                                            .and_then(|p| self.arena.get(p))
                                            .and_then(|pel| pel.get_user_data::<crate::widgets::overlay::MenuOpenDir>())
                                            .is_some_and(|d| d.0);
                                            let sub_h = sub
                                                .0
                                                .iter()
                                                .filter(|i| !i.separator)
                                                .count()
                                                .max(1)
                                                as f32
                                                * 32.0;
                                            let screen_w =
                                                self.config.width / self.scale_factor as f32;
                                            let screen_h =
                                                self.config.height / self.scale_factor as f32;
                                            let (sub_x, opened_left) =
                                                submenu::submenu_x(sb.x, sb.width, screen_w, prefer_left);
                                            let sub_y = submenu::submenu_y(sb.y, sb.height, sub_h, screen_h);
                                            let sub_pos = Point::new(sub_x, sub_y);
                                            crate::widgets::overlay::open_context_menu(
                                                sub.0,
                                                sub_pos,
                                                &mut self.arena,
                                                root_id,
                                                Some(&mut self.event_registry),
                                                parent,
                                                opened_left,
                                                screen_h,
                                            );
                                        }
                                        break;
                                    }
                                }
                            }
                            // Also consider click "on menu" if the cursor is inside any
                            // open menu container (covers gap between parent & submenu).
                            if !hit_menu {
                                hit_menu =
                                    crate::widgets::overlay::menu_chain_contains(&self.arena, *pos);
                            }
                        }
                        if !hit_menu {
                            crate::widgets::overlay::dismiss_context_menu_immediate(
                                &mut self.arena,
                            );
                        }
                    }
                    if !pressed {
                        self.touches.entry(state_key).or_default().pressed = new_pressed;
                    }
                }
                winit::event::WindowEvent::PointerEntered { position, kind, .. } => {
                    let sf = self.scale_factor as f32;
                    let lx = position.x as f32 / sf;
                    let ly = position.y as f32 / sf;
                    let pos = Point::new(lx, ly);
                    let (state_key, _event_fid) = finger::finger_id_from_kind(&kind);

                    let new_chain =
                        if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                            result.path
                        } else {
                            Vec::new()
                        };
                    let old_chain: Vec<ElementId> = self
                        .touches
                        .get(&state_key)
                        .map_or(Vec::new(), |s| s.hovered_chain.clone());

                    for &eid in old_chain.iter().rev() {
                        if !new_chain.contains(&eid) {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                        }
                    }
                    for &eid in &new_chain {
                        if !old_chain.contains(&eid) {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, true);
                            }
                            self.event_registry.fire_hover_enter(eid);
                        }
                    }

                    let state = self.touches.entry(state_key).or_default();
                    state.position = pos;
                    state.hovered_chain = new_chain;
                }
                winit::event::WindowEvent::PointerLeft { kind, .. } => {
                    let (state_key, _) = finger::finger_id_from_kind(&kind);
                    if state_key == 0 {
                        self.last_cursor = None;
                    }
                    if let Some(state) = self.touches.get_mut(&state_key) {
                        for &eid in state.hovered_chain.iter().rev() {
                            if let Some(el) = self.arena.get_mut(eid) {
                                el.set_state_dirty(StateFlags::HOVERED, false);
                            }
                            self.event_registry.fire_hover_leave(eid);
                        }
                        state.hovered_chain.clear();
                        state.pressed = None;
                    }
                    if let Some(ref wh) = self.window_handle {
                        wh.set_cursor(crate::platform::CursorIcon::DEFAULT);
                    }
                }
                winit::event::WindowEvent::MouseWheel { delta, phase, .. } => {
                    // Scrolling over an open menu scrolls the menu itself; only a
                    // wheel event *outside* any open menu dismisses it.
                    let over_menu = self.last_cursor.is_some_and(|pos| {
                        crate::widgets::overlay::menu_chain_contains(&self.arena, pos)
                    });
                    if !over_menu {
                        crate::widgets::overlay::dismiss_context_menu_immediate(&mut self.arena);
                    }
                    let (dx, dy) = match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => (
                            x * super::scroll_physics::WHEEL_PIXELS_PER_LINE * super::scroll_physics::WHEEL_LINES_PER_NOTCH,
                            y * super::scroll_physics::WHEEL_PIXELS_PER_LINE * super::scroll_physics::WHEEL_LINES_PER_NOTCH,
                        ),
                        winit::event::MouseScrollDelta::PixelDelta(pos) => {
                            (pos.x as f32, pos.y as f32)
                        }
                    };
                    #[cfg(feature = "devtools")]
                    if let Some(ref pos) = self.last_cursor {
                        crate::debug::devtools::record_interaction(
                            self.frame_id,
                            crate::debug::devtools::InteractionKind::Scroll {
                                x: pos.x,
                                y: pos.y,
                                delta_x: dx,
                                delta_y: dy,
                            },
                            auralis_signal::now_us(),
                        );
                    }
                    let now = StdInstant::now();

                    // Velocity tracking for kinetic scroll on trackpad gesture end
                    match phase {
                        winit::event::TouchPhase::Started => {
                            self.scroll_kinetic = None;
                            self.velocity_history.clear();
                        }
                        winit::event::TouchPhase::Moved => {
                            self.velocity_history.push((dx, dy, now));
                            self.velocity_history.retain(|(_, _, t)| {
                                now.duration_since(*t).as_millis() < super::scroll_physics::VELOCITY_HISTORY_MAX_MS
                            });
                        }
                        winit::event::TouchPhase::Ended => {
                            if let Some(target) = self.scroll_kinetic_target {
                                let vel = crate::widgets::bundle::scroll::compute_scroll_velocity(
                                    &self.velocity_history,
                                );
                                crate::widgets::bundle::scroll::try_fling(
                                    &self.arena,
                                    target,
                                    Vec2::new(-vel.x, -vel.y),
                                );
                            }
                            self.velocity_history.clear();
                        }
                        _ => {}
                    }

                    // Propagate scroll through capture → bubble first.
                    // Reuse the hit-test result for the fallback below (B3):
                    // previously every wheel event paid TWO full point queries.
                    let mut propagated = false;
                    let mut wheel_hit: Option<crate::event::hit_test::HitTestResult> = None;
                    if let Some(ref pos) = self.last_cursor {
                        wheel_hit = crate::event::hit_test::hit_test(&self.arena, *pos);
                        if let Some(ref result) = wheel_hit {
                            let evt = crate::event::Event::Scroll {
                                delta_x: dx,
                                delta_y: dy,
                            };
                            let modifiers = self.translator.modifiers();
                            propagated = crate::event::propagation::dispatch_event(
                                &mut self.arena,
                                &evt,
                                &result.path,
                                &mut self.focus_manager,
                                &mut self.event_registry,
                                modifiers,
                            )
                            .handled;
                        }
                    }

                    // Default scroll behavior — only if no widget consumed the event.
                    if !propagated {
                        if let Some(ref pos) = self.last_cursor {
                            let mut handled = false;
                            let hit_target = wheel_hit.as_ref().map(|r| r.target).or_else(|| {
                                dirty_registry::hit_test_with_fallback(&self.arena, *pos)
                            });

                            // ── Text scroll on hit target ──
                            if let Some(hit) = hit_target {
                                if let Some(el) = self.arena.get(hit) {
                                    if let Some(ref sy) = el.text_scroll_y() {
                                        let old_v = sy.get();
                                        let mut v = old_v - dy;
                                        let max_v = el
                                            .max_scroll_y()
                                            .as_ref()
                                            .map_or(f32::MAX, |c| c.get());
                                        v = v.clamp(0.0, max_v);
                                        if v != old_v {
                                            sy.set(v);
                                            el.mark_repaint();
                                            handled = true;
                                        }
                                    }
                                }
                            }

                            // ── Nested scroll chain: innermost-first, pass unconsumed ──
                            if !handled {
                                let chain = dirty_registry::spatial_scroll_chain(&self.arena, *pos);
                                if !chain.is_empty() {
                                    self.scroll_kinetic_target = Some(chain[0]);
                                    let mut rem_x = dx;
                                    let mut rem_y = dy;
                                    for &scrollable in &chain {
                                        if rem_x == 0.0 && rem_y == 0.0 {
                                            break;
                                        }
                                        let vp = self
                                            .arena
                                            .get(scrollable)
                                            .map_or(Rect::ZERO, |el| {
                                                el.screen_bounds
                                            });
                                        if let Some((ux, uy)) =
                                            crate::widgets::bundle::scroll::try_scroll_by(
                                                &self.arena,
                                                scrollable,
                                                rem_x,
                                                rem_y,
                                                vp.height,
                                                vp.width,
                                            )
                                        {
                                            rem_x = ux;
                                            rem_y = uy;
                                            handled = true;
                                        }
                                    }
                                }
                            }

                            // ── Raw SCROLL component WITHOUT ScrollBundle (portal fallback) ──
                            if !handled {
                                if let Some(hit) = hit_target {
                                    let mut cur = Some(hit);
                                    while let Some(eid) = cur {
                                        if self.arena.get(eid).and_then(|el| el.get_user_data::<crate::widgets::bundle::scroll::ScrollBundleRef>()).is_none()
                                        && self.arena.comp_scroll(eid).is_some()
                                    {
                                        let sc = self.arena.comp_scroll(eid).unwrap();
                                        let mut off = sc.scroll_offset.get();
                                        off.y = (off.y - dy).max(0.0).min(sc.max_scroll_y.get());
                                        sc.scroll_offset.set(off);
                                        dirty_registry::spatial_update_scroll(eid, 0.0, off.y);
                                        dirty_registry::bump_subtree_gen(eid);
                                        dirty_registry::register_dirty(eid, DirtyFlags::REPAINT);
                                        if let Some(el) = self.arena.get(eid) { el.mark_repaint(); }
                                        handled = true;
                                        break;
                                    }
                                        cur = dirty_registry::parent_of(eid);
                                    }
                                }
                            }

                            // ── Last-resort: tree-walk fallback ──
                            if !handled {
                                if let Some(root_id) = rid {
                                    let target =
                                        crate::widgets::bundle::scroll::find_scrollable_at_position(
                                            &self.arena,
                                            root_id,
                                            *pos,
                                        );
                                    self.scroll_kinetic_target = target;
                                    if let Some(eid) = target {
                                        let vp = self
                                            .arena
                                            .get(eid)
                                            .map_or(Rect::ZERO, |el| {
                                                el.screen_bounds
                                            });
                                        crate::widgets::bundle::scroll::try_scroll_by(
                                            &self.arena,
                                            eid,
                                            dx,
                                            dy,
                                            vp.height,
                                            vp.width,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if let Some(ref w) = self.winit_window {
                        w.request_redraw();
                    }
                }
                winit::event::WindowEvent::ModifiersChanged(mods) => {
                    self.translator.set_modifiers(crate::event::Modifiers {
                        shift: mods.state().shift_key(),
                        ctrl: mods.state().control_key(),
                        alt: mods.state().alt_key(),
                        meta: mods.state().meta_key(),
                    });
                }
                winit::event::WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    #[cfg(feature = "devtools")]
                    {
                        let key_name = format!("{:?}", key_event.logical_key);
                        let mods = self.translator.modifiers();
                        let kind = if key_event.state == winit::event::ElementState::Pressed {
                            crate::debug::devtools::InteractionKind::KeyPress {
                                key_name,
                                modifiers: crate::debug::devtools::modifiers_to_u32(mods),
                            }
                        } else {
                            crate::debug::devtools::InteractionKind::KeyRelease {
                                key_name,
                                modifiers: crate::debug::devtools::modifiers_to_u32(mods),
                            }
                        };
                        crate::debug::devtools::record_interaction(
                            self.frame_id,
                            kind,
                            auralis_signal::now_us(),
                        );
                    }
                    if key_event.state == winit::event::ElementState::Pressed {
                        self.focus_manager
                            .set_highlight_mode(crate::event::FocusHighlightMode::Traditional);
                        let mods = self.translator.modifiers();
                        let focused = self.focus_manager.focused();
                        let mut handled = false;

                        let action_path: Vec<ElementId> = focused
                            .map(|fid| self.arena.path_to_root(fid))
                            .unwrap_or_default();

                        // 1. Try action dispatch
                        if let winit::keyboard::Key::Character(c) = &key_event.logical_key {
                            if !c.is_empty() {
                                let chr = if mods.ctrl || mods.shift {
                                    c.to_lowercase()
                                } else {
                                    c.to_string()
                                };
                                if let Some(action_kind) = self.key_bindings.find(
                                    focused,
                                    &crate::event::Key::Character(chr.clone()),
                                    &mods,
                                ) {
                                    let action = if mods.shift {
                                        Action::new(action_kind).with_selection()
                                    } else {
                                        Action::new(action_kind)
                                    };
                                    action::dispatch_action(self, &action, &action_path);
                                    handled = true;
                                }
                            }
                        }

                        // 2. Named keys / special characters.
                        if !handled {
                            let is_in_textinput =
                                focused.is_some_and(|fid| self.event_registry.has_text_input(fid));
                            let lookup_key = match &key_event.logical_key {
                                winit::keyboard::Key::Character(c)
                                    if is_in_textinput && c == " " =>
                                {
                                    crate::event::Key::Character("?".into())
                                }
                                _ => map_winit_action_key(&key_event.logical_key),
                            };
                            if let Some(action_kind) =
                                self.key_bindings.find(focused, &lookup_key, &mods)
                            {
                                let action = if mods.shift {
                                    Action::new(action_kind).with_selection()
                                } else {
                                    Action::new(action_kind)
                                };
                                action::dispatch_action(self, &action, &action_path);
                                handled = true;
                            }
                        }

                        // Context-menu keyboard actions (Esc / activate / submenu leaf)
                        // mark portals for removal via dismiss_context_menu(). Drain
                        // them now, then invalidate the vacated region: arena.remove
                        // marks the parent for *relayout* but NOT repaint, so without
                        // this the overlay's pixels linger until the next event frame.
                        if handled {
                            let mut removed_any = false;
                            for removed in crate::platform::portal::drain_portal_removals() {
                                self.arena.remove(removed);
                                removed_any = true;
                            }
                            if removed_any {
                                action::invalidate_after_menu_change(self);
                            }
                        }

                        // Keyboard submenu open/close requests need arena + root_id,
                        // so they are fulfilled here (same pattern as HOVERED_SUBMENU).
                        if let Some(req) = crate::widgets::overlay::take_kb_menu_request() {
                            match req {
                                crate::widgets::overlay::KbMenuRequest::OpenSubmenu(row_eid) => {
                                    let info = self.arena.get(row_eid).map(|el| (
                                    el.screen_bounds,
                                    el.get_user_data::<crate::widgets::overlay::ContextMenuItems>().cloned(),
                                ));
                                    if let Some((sb, Some(cmi))) = info {
                                        let win_w = self.config.width / self.scale_factor as f32;
                                        let win_h = self.config.height / self.scale_factor as f32;
                                        let sub_h =
                                            cmi.0.iter().filter(|i| !i.separator).count().max(1)
                                                as f32
                                                * 32.0;
                                        let parent =
                                            crate::core::dirty_registry::parent_of(row_eid);
                                        let prefer_left = parent
                                        .and_then(|p| self.arena.get(p))
                                        .and_then(|pel| pel.get_user_data::<crate::widgets::overlay::MenuOpenDir>())
                                        .is_some_and(|d| d.0);
                                        let (sub_x, opened_left) =
                                            submenu::submenu_x(sb.x, sb.width, win_w, prefer_left);
                                        let sub_y = submenu::submenu_y(sb.y, sb.height, sub_h, win_h);
                                        if let Some(rid) = self.arena.root_id {
                                            crate::widgets::overlay::open_context_menu(
                                                cmi.0,
                                                Point::new(sub_x, sub_y),
                                                &mut self.arena,
                                                rid,
                                                Some(&mut self.event_registry),
                                                parent,
                                                opened_left,
                                                win_h,
                                            );
                                            crate::widgets::overlay::mark_submenu_opened();
                                        }
                                    }
                                }
                                crate::widgets::overlay::KbMenuRequest::CloseSubmenu => {
                                    if let Some(parent) =
                                        crate::widgets::overlay::close_deepest_submenu(
                                            &mut self.arena,
                                        )
                                    {
                                        action::transfer_focus(self, parent, FocusReason::Programmatic);
                                    }
                                    action::invalidate_after_menu_change(self);
                                }
                            }
                        }

                        // 3. Fallback: printable character -> text_input.
                        if !handled {
                            if let winit::keyboard::Key::Character(c) = &key_event.logical_key {
                                if !c.is_empty() {
                                    if let Some(focused_id) = focused {
                                        if self.event_registry.has_text_input(focused_id) {
                                            for ch in c.chars() {
                                                self.event_registry.fire_text_input(focused_id, ch);
                                            }
                                            let _ = true;
                                        }
                                    }
                                }
                            }
                        }

                        // 4. Always emit KeyDown/KeyUp for widget-level handlers.
                        if let Some(k) = map_winit_key(&key_event.logical_key) {
                            let key_events = self.translator.keyboard_input(true, k);
                            self.dispatch_events(&key_events);
                        }
                    }
                    if let Some(ref w) = self.winit_window {
                        w.request_redraw();
                    }
                }
                winit::event::WindowEvent::Ime(ime) => {
                    use winit::event::Ime;
                    if let Some(focused_id) = self.focus_manager.focused() {
                        if self.event_registry.has_text_input(focused_id) {
                            match ime {
                                Ime::Commit(text) => {
                                    #[cfg(feature = "devtools")]
                                    crate::debug::devtools::record_interaction(
                                        self.frame_id,
                                        crate::debug::devtools::InteractionKind::ImeCommit {
                                            text: text.clone(),
                                        },
                                        auralis_signal::now_us(),
                                    );
                                    // Atomic path: the whole commit lands as one
                                    // edit. Fallback: per-char for widgets that
                                    // only register text_input.
                                    if self.event_registry.has_ime_commit(focused_id) {
                                        self.event_registry.fire_ime_commit(focused_id, text);
                                    } else {
                                        for ch in text.chars() {
                                            self.event_registry.fire_text_input(focused_id, ch);
                                        }
                                    }
                                    self.event_registry.fire_ime_preedit(
                                        focused_id,
                                        String::new(),
                                        None,
                                    );
                                }
                                Ime::Preedit(text, cursor) => {
                                    #[cfg(feature = "devtools")]
                                    crate::debug::devtools::record_interaction(
                                        self.frame_id,
                                        crate::debug::devtools::InteractionKind::ImePreedit {
                                            text: text.clone(),
                                            cursor_begin: cursor.map(|(s, _)| s),
                                            cursor_end: cursor.map(|(_, e)| e),
                                        },
                                        auralis_signal::now_us(),
                                    );
                                    self.event_registry.fire_ime_preedit(
                                        focused_id,
                                        text,
                                        cursor.map(|(s, e)| (s, e)),
                                    );
                                }
                                Ime::Enabled => {
                                    // Force the next sync to send a fresh area —
                                    // the OS IME just came up and needs coordinates
                                    // now, not after the next caret move (IME area
                                    // dedup P1).
                                    self.last_sent_ime_area = None;
                                }
                                Ime::Disabled => {
                                    self.last_sent_ime_area = None;
                                    // Do NOT set ime_suppressed here — that flag is
                                    // for widgets that permanently opt out of IME
                                    // (password fields). The OS will send Ime::Enabled
                                    // when the user reactivates the IME; we must
                                    // honour that (Ctrl+Space fix).
                                }
                                Ime::DeleteSurrounding {
                                    before_bytes,
                                    after_bytes,
                                } => {
                                    self.event_registry.fire_ime_delete_surrounding(
                                        focused_id,
                                        before_bytes,
                                        after_bytes,
                                    );
                                }
                            }
                        }
                    }
                }
                winit::event::WindowEvent::ThemeChanged(theme) => {
                    if let Some(ref sig) = self.theme_signal {
                        let is_dark = matches!(theme, winit::window::Theme::Dark);
                        let seed = Color::rgba8(0x67, 0x79, 0xE8, 0xFF);
                        if is_dark {
                            sig.set(M3Theme::from_seed(seed).with_is_dark(true));
                        } else {
                            sig.set(M3Theme::from_seed(seed));
                        }
                        let new_theme = sig.read();
                        self.config.theme = new_theme;
                        self.needs_theme_reapply = true;
                        if let Some(ref mut r) = self.renderer {
                            r.set_clear_color(self.config.theme.scheme.surface);
                        }
                    }
                }
                // ── Touch gestures (macOS / Wayland) ──
                winit::event::WindowEvent::PinchGesture { delta, phase, .. } => {
                    let pos = self.last_cursor.unwrap_or(Point::ZERO);
                    if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                        let gphase = map_touch_phase(phase);
                        let modifiers = self.translator.modifiers();
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            &crate::event::Event::Pinch {
                                delta,
                                position: pos,
                                phase: gphase,
                            },
                            &result.path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                winit::event::WindowEvent::RotationGesture { delta, phase, .. } => {
                    let pos = self.last_cursor.unwrap_or(Point::ZERO);
                    if let Some(result) = crate::event::hit_test::hit_test(&self.arena, pos) {
                        let gphase = map_touch_phase(phase);
                        let modifiers = self.translator.modifiers();
                        let _ = crate::event::propagation::dispatch_event(
                            &mut self.arena,
                            &crate::event::Event::Rotate {
                                delta,
                                position: pos,
                                phase: gphase,
                            },
                            &result.path,
                            &mut self.focus_manager,
                            &mut self.event_registry,
                            modifiers,
                        );
                    }
                }
                // ── Drag & Drop ──
                winit::event::WindowEvent::DragEntered { paths, position } => {
                    let sf = self.scale_factor;
                    let pos = Point::new(
                        position.x as f32 / sf as f32,
                        position.y as f32 / sf as f32,
                    );
                    if let Some(hit) = dirty_registry::hit_test_with_fallback(&self.arena, pos) {
                        if let Some(el) = self.arena.get_mut(hit) {
                            if el.drop_target() {
                                el.state
                                    .set(el.state.get() | StateFlags::HOVERED);
                                el.mark_repaint();
                            }
                        }
                    }
                    let _ = paths;
                }
                winit::event::WindowEvent::DragMoved { position } => {
                    let sf = self.scale_factor;
                    let pos = Point::new(
                        position.x as f32 / sf as f32,
                        position.y as f32 / sf as f32,
                    );
                    if let Some(hit) = dirty_registry::hit_test_with_fallback(&self.arena, pos) {
                        if let Some(el) = self.arena.get(hit) {
                            if el.drop_target() {
                                el.mark_repaint();
                            }
                        }
                    }
                }
                winit::event::WindowEvent::DragDropped { paths, position } => {
                    let sf = self.scale_factor;
                    let pos = Point::new(
                        position.x as f32 / sf as f32,
                        position.y as f32 / sf as f32,
                    );
                    if let Some(hit) = dirty_registry::hit_test_with_fallback(&self.arena, pos) {
                        if let Some(el) = self.arena.get_mut(hit) {
                            if el.drop_target() {
                                let drag_data = crate::event::DragData {
                                    kind: crate::event::DragKind::Files,
                                    text: None,
                                    paths,
                                    position: Some(pos),
                                    label: None,
                                };
                                if let Some(ref handler) = el.on_drop() {
                                    handler(drag_data);
                                }
                            }
                        }
                    }
                    let _ = paths;
                }
                winit::event::WindowEvent::DragLeft { .. } => {}
                winit::event::WindowEvent::Focused(gained) => {
                    if !gained {
                        if let Some(old_id) = self.focus_manager.focused() {
                            if let Some(el) = self.arena.get_mut(old_id) {
                                el.set_state_dirty(StateFlags::FOCUSED, false);
                                el.last_focus_reason
                                    .set(Some(FocusReason::WindowActivation));
                            }
                            self.event_registry
                                .fire_focus_out(old_id, FocusReason::WindowActivation);
                            self.focus_manager.set_focused(None);
                        }
                    } else {
                        self.skip_frame = false;
                        if let Some(ref w) = win {
                            w.request_redraw();
                        }
                    }
                }
                winit::event::WindowEvent::Occluded(occluded) => {
                    self.skip_frame = occluded;
                    if !occluded {
                        if let Some(ref w) = win {
                            w.request_redraw();
                        }
                    }
                }
                winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    self.scale_factor = scale_factor;
                    if let Some(ref mut r) = self.renderer {
                        r.set_scale_factor_gpu(scale_factor);
                        r.resize_cpu(self.config.width, self.config.height, scale_factor as f32);
                    }
                    if let Some(ref w) = win {
                        w.request_redraw();
                    }
                }
                _ => {}
            }
            self.flush_scheduler.drain();
            auralis_task::drain_deferred_signal_callbacks();

            if let Some(_root_id) = rid {
                if crate::core::dirty_registry::has_pending_dirty() {
                    self.on_frame();
                }
            }
        }));
        if let Err(panic) = result {
            push_error(UiError::CallbackPanic {
                context: "handle_event".into(),
                window_id: self
                    .window_handle
                    .as_ref()
                    .map(|h| h.id().into_raw() as u64),
                element_id: None,
                message: panic_to_string(&panic),
            });
            self.needs_rebuild = true;
        }
    }
}
