use std::sync::Arc;

use crate::core::element::ElementArena;
use crate::core::frame_driver::FrameHook;
use crate::event::EventRegistry;
use crate::core::ElementId;
use super::submenu::{submenu_x, submenu_y};

/// Window's `FrameHook` impl: SEAM 1 (after-dirty) platform work — the hovered
/// submenu delayed-open, plus signaling whether a full relayout is forced.
pub(crate) const SUBMENU_DELAY_MS: u128 = 200;
pub(crate) const SUBMENU_TIMER_KEY: u64 = 0x5B6D;
pub(crate) struct WindowFrameHook<'a> {
    pub config: &'a super::config::WindowConfig,
    pub scale_factor: f64,
    pub winit_window: Option<&'a Arc<dyn winit::window::Window>>,
}

impl FrameHook for WindowFrameHook<'_> {
    fn on_focus_transferred(&mut self, events: &EventRegistry, new_id: ElementId) {
        if let Some(w) = self.winit_window {
            super::ime::request_ime_enable(w, events, new_id);
        }
    }
    fn on_after_dirty(&mut self, arena: &mut ElementArena, events: &mut EventRegistry) -> bool {
        // ── Hovered submenu: open after delay ──
        let mut open_sub = None;
        crate::core::app_context::current_app().hovered_submenu_with(|s| {
            if let Some((eid, since)) = s.as_ref() {
                if since.elapsed().as_millis() >= SUBMENU_DELAY_MS {
                    if let Some(el) = arena.get(*eid) {
                        let sb = el.screen_bounds;
                        let items = el
                            .get_user_data::<crate::widgets::overlay::ContextMenuItems>()
                            .cloned();
                        let screen_w = self.config.width / self.scale_factor as f32;
                        let screen_h = self.config.height / self.scale_factor as f32;
                        let parent = crate::core::dirty_registry::parent_of(*eid);
                        let prefer_left = parent
                            .and_then(|p| arena.get(p))
                            .and_then(|pel| {
                                pel.get_user_data::<crate::widgets::overlay::MenuOpenDir>()
                            })
                            .is_some_and(|d| d.0);
                        open_sub = items.map(|cmi| {
                            let sub_h =
                                cmi.0.iter().filter(|i| !i.separator).count().max(1) as f32 * 32.0;
                            let (sub_x, opened_left) =
                                submenu_x(sb.x, sb.width, screen_w, prefer_left);
                            let sub_y = submenu_y(sb.y, sb.height, sub_h, screen_h);
                            (
                                cmi.0,
                                crate::style::Point::new(sub_x, sub_y),
                                parent,
                                opened_left,
                            )
                        });
                    }
                    *s = None;
                    crate::core::scheduler::cancel(SUBMENU_TIMER_KEY);
                }
            }
        });
        if let Some((items, pos, parent, opened_left)) = open_sub {
            if let Some(rid) = arena.root_id {
                crate::widgets::overlay::open_context_menu(
                    items,
                    pos,
                    arena,
                    rid,
                    Some(events),
                    parent,
                    opened_left,
                    self.config.height / self.scale_factor as f32,
                );
                crate::widgets::overlay::mark_submenu_opened();
            }
        }

        // Force a full relayout if deferred actions / portal adds / submenu
        // open registered structural or dirty changes (window's needs_taffy).
        crate::core::dirty_registry::has_structurally_changed()
            || crate::core::dirty_registry::has_pending_dirty()
    }
}
