//! Toast / Snackbar — brief, auto-dismissing notifications.
//!
//! Flutter-style: single-at-a-time FIFO queue with slide-up enter animation.
//!
//! ## Quick start
//! ```ignore
//! // Mount once in your app root:
//! App::new().window(config, Compositor::new(|_scope| {
//!     ZStack::new()  // or your root container
//!         .push(ToastContainer::new())
//! }))
//!
//! // Call from anywhere (same thread):
//! toast::show("File saved", ToastKind::Success);
//! toast::show_action("Deleted", ToastKind::Info, "Undo", || { /* restore */ });
//! ```
//!
//! ## Architecture
//! ```text
//! show() → per-window ToastDomain queue (FIFO, AppContext extension)
//!   → ToastContainer.frame_tick: poll queue → populate slot → animate
//!     → ENTERING (300ms): position_offset.y slides from +100 → 0, opacity 0→1
//!     → VISIBLE (default 4s): stays put, schedules auto-dismiss
//!     → EXITING  (200ms): position_offset.y slides 0 → +100, opacity 1→0
//!     → DONE: check queue for next
//! ```
//!
//! The container element uses portal absolute positioning via `(x,y,w)` cell +
//! `PortalHeight`, placing it at the viewport bottom. `position_offset` drives
//! the visual slide animation without affecting taffy layout (and hit-testing
//! correctly follows offset).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use web_time::Instant;

use crate::core::clock;
use crate::core::config::EventHandler;
use crate::core::context::MountContext;
use crate::core::element::{DirtyFlags, ElementId};
use crate::core::scheduler;
use crate::core::widget::Widget;
use crate::ecs::components;
use crate::platform::portal::PortalHeight;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::{Color, Padding, Vec2};
pub use crate::theme::m3::roles::ToastKind;
use crate::theme::m3::roles::{ComponentRole, DisplayRole, ResolvedComponentStyle};

// ═══════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════

/// Show a toast with default 4 s duration.
pub fn show(message: impl Into<String>, kind: ToastKind) {
    show_duration(message, kind, DEFAULT_DURATION_MS);
}

/// Show a toast with a custom duration in milliseconds.
/// Pass `0` for a persistent toast (dismissed only by user action).
pub fn show_duration(message: impl Into<String>, kind: ToastKind, duration_ms: u64) {
    enqueue(ToastItem {
        message: message.into(),
        kind,
        duration_ms,
        action: None,
    });
}

/// Show a toast with an action button (e.g. "Undo").
pub fn show_action(
    message: impl Into<String>,
    kind: ToastKind,
    action_label: impl Into<String>,
    on_action: impl Fn() + 'static,
) {
    enqueue(ToastItem {
        message: message.into(),
        kind,
        duration_ms: DEFAULT_DURATION_MS,
        action: Some(ToastAction {
            label: action_label.into(),
            callback: Rc::new(on_action),
        }),
    });
}

/// Remove all pending toasts from the queue (currently-showing toast is not affected).
pub fn clear_queue() {
    toast_domain().queue.borrow_mut().clear();
}

/// Number of toasts waiting in the queue.
pub fn queue_len() -> usize {
    toast_domain().queue.borrow().len()
}

// ═══════════════════════════════════════════════════════════════════
// Internal types
// ═══════════════════════════════════════════════════════════════════

struct ToastAction {
    label: String,
    callback: Rc<dyn Fn()>,
}

struct ToastItem {
    message: String,
    kind: ToastKind,
    duration_ms: u64,
    action: Option<ToastAction>,
}

fn enqueue(item: ToastItem) {
    let dom = toast_domain();
    dom.queue.borrow_mut().push_back(item);
    // Event-driven wake: the container hides itself (reactive_visible=false)
    // between toasts, and hidden elements are skipped by the frame_tick pass
    // (perf 9826a45). Un-hide + dirty here so the container's tick resumes
    // and dequeues on the next frame — no per-frame queue polling while idle.
    let wake = dom.wake.borrow().clone();
    if let Some((cid, visible)) = wake {
        visible.set(true);
        crate::core::dirty_registry::register_dirty(cid, DirtyFlags::REPAINT);
    }
}

/// Per-window toast queue (audit 2026-07-18 multi-window pass): each
/// window's ToastContainer drains only its own window's queue.
#[derive(Default)]
pub(crate) struct ToastDomain {
    queue: RefCell<VecDeque<ToastItem>>,
    /// Wake registration from the mounted ToastContainer:
    /// `(container_id, reactive_visible cell)` — see [`enqueue`].
    wake: RefCell<Option<(ElementId, Rc<Cell<bool>>)>>,
}

fn toast_domain() -> Rc<ToastDomain> {
    crate::core::app_context::current_app().extension::<ToastDomain>()
}

// ═══════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════

const ENTER_MS: f32 = 300.0;
const EXIT_MS: f32 = 200.0;
const DEFAULT_DURATION_MS: u64 = 4000;
const MAX_WIDTH: f32 = 560.0;
const MARGIN: f32 = 16.0;
const TOAST_HEIGHT: f32 = 48.0;
const SCHEDULER_KEY: u64 = scheduler::keys::TOAST;

// ═══════════════════════════════════════════════════════════════════
// Easing
// ═══════════════════════════════════════════════════════════════════

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
fn ease_in_cubic(t: f32) -> f32 {
    t.powi(3)
}

// ═══════════════════════════════════════════════════════════════════
// Animation state machine
// ═══════════════════════════════════════════════════════════════════

enum AnimState {
    Entering { start: Instant },
    Visible { deadline: Instant },
    Exiting { start: Instant },
}

// ═══════════════════════════════════════════════════════════════════
// ToastContainer widget
// ═══════════════════════════════════════════════════════════════════

/// Vertical screen edge where toasts appear.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastPosition {
    Top,
    Bottom,
}

/// A container that displays queued toast notifications.
pub struct ToastContainer {
    position: ToastPosition,
    max_width: f32,
    margin: f32,
    style: StyleRefinement,
}

impl ToastContainer {
    pub fn new() -> Self {
        Self {
            position: ToastPosition::Bottom,
            max_width: MAX_WIDTH,
            margin: MARGIN,
            style: StyleRefinement::default(),
        }
    }

    pub fn position(mut self, p: ToastPosition) -> Self {
        self.position = p;
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = w;
        self
    }
    pub fn margin(mut self, m: f32) -> Self {
        self.margin = m;
        self
    }
}

impl Default for ToastContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for ToastContainer {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl std::fmt::Debug for ToastContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToastContainer")
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

impl Widget for ToastContainer {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let theme = ctx.theme;
        let position = self.position;
        let max_w = self.max_width;
        let margin = self.margin;

        // ── Resolve info style as default ──
        let info_role = ComponentRole::Display(DisplayRole::Toast {
            kind: ToastKind::Info,
        });
        let info_style = match theme.resolve_component(&info_role) {
            ResolvedComponentStyle::Toast(s) => s.clone(),
            _ => unreachable!(),
        };
        let font_size = info_style.font_size;
        let icon_size = 18.0;

        // ── Snapshot viewport (initial; used for positioning until resize) ──
        let vp_h: Rc<Cell<f32>> = Rc::new(Cell::new(
            ctx.arena
                .root_id
                .and_then(|rid| ctx.arena.get(rid))
                .map(|r| r.bounds().height.max(1.0))
                .unwrap_or(768.0),
        ));
        let vp_w: Rc<Cell<f32>> = Rc::new(Cell::new(
            ctx.arena
                .root_id
                .and_then(|rid| ctx.arena.get(rid))
                .map(|r| r.bounds().width.max(1.0))
                .unwrap_or(1024.0),
        ));

        // ═══════════════════════════════════════════════════
        // Portal container
        // ═══════════════════════════════════════════════════
        let container_id = ctx.arena.allocate();
        ctx.preallocate(container_id, components::LAYOUT | components::LIFECYCLE);

        let pos_cell: Rc<Cell<(f32, f32, f32)>> = Rc::new(Cell::new((0.0, 0.0, max_w)));
        let portal_h_cell: Rc<Cell<f32>> = Rc::new(Cell::new(TOAST_HEIGHT));
        let visible: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let pos_offset: Rc<Cell<Vec2>> = Rc::new(Cell::new(Vec2::ZERO));

        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_z_index(theme.z_index.toast);
            el.z_index_floor = Some(theme.z_index.toast);
            el.set_layout_direction(crate::core::LayoutDirection::Vertical);
            el.set_flex_shrink(0.0);
            el.set_reactive_visible(visible.clone());
            el.set_scrollbar_policy(crate::core::config::ScrollbarPolicy::Never);
            el.set_position_offset(pos_offset.clone());
            el.insert_user_data(pos_cell.clone());
            el.insert_user_data(PortalHeight(portal_h_cell.clone()));
            // No anchor_id → update_portal_positions skips us — we manage position ourselves
        }
        // Register the wake handle so `enqueue` can un-hide this container
        // (its frame_tick is skipped while reactive-hidden).
        {
            let dom = toast_domain();
            *dom.wake.borrow_mut() = Some((container_id, visible.clone()));
            // Backlog: toasts enqueued before this container mounted.
            if !dom.queue.borrow().is_empty() {
                visible.set(true);
                crate::core::dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);
            }
        }

        // ═══════════════════════════════════════════════════
        // Toast slot: HStack [icon_glyph] [gap] [message] [gap] [action_btn] [close_btn]
        // ═══════════════════════════════════════════════════
        let slot_id = ctx.arena.allocate();
        ctx.preallocate(slot_id, components::LAYOUT | components::LIFECYCLE);
        {
            let Some(el) = ctx.arena.get_mut(slot_id) else {
                return container_id;
            };
            el.set_layout_direction(crate::core::LayoutDirection::Horizontal);
            el.set_flex_shrink(0.0);
            el.set_padding(Padding {
                left: 16.0,
                right: 8.0,
                top: 12.0,
                bottom: 12.0,
            });
            el.set_text_vertical_center(true);
            el.set_position_offset(pos_offset.clone());
        }
        // Cross-axis centering for the HStack children
        crate::core::element::with_ct_mut(|ct| {
            let lc = ct.layout.entry(slot_id).or_default();
            lc.content_align = crate::style::Alignment::Center;
        });
        ctx.arena.add_child(container_id, slot_id);

        // ── Shared dynamic cells ──
        let msg_buf: Rc<RefCell<cosmic_text::Buffer>> = Rc::new(RefCell::new(create_buffer(
            "",
            font_size,
            1.3,
            400,
            None,
            None,
            crate::style::TextAlign::Start,
        )));
        let icon_buf: Rc<RefCell<cosmic_text::Buffer>> = Rc::new(RefCell::new(create_buffer(
            "",
            icon_size,
            1.3,
            400,
            None,
            None,
            crate::style::TextAlign::Center,
        )));
        let action_buf: Rc<RefCell<cosmic_text::Buffer>> = Rc::new(RefCell::new(create_buffer(
            "",
            font_size,
            1.3,
            500,
            None,
            None,
            crate::style::TextAlign::Center,
        )));
        let action_handler: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let exit_req: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // ── Icon glyph ──
        let icon_id = ctx.arena.allocate();
        ctx.preallocate(icon_id, components::TEXT | components::LAYOUT);
        {
            let Some(el) = ctx.arena.get_mut(icon_id) else {
                return container_id;
            };
            el.set_preferred_width(Some(icon_size + 4.0));
            el.set_preferred_height(icon_size + 4.0);
            el.set_flex_shrink(0.0);
            el.set_font_size(icon_size);
            el.set_text_buffer(icon_buf.clone());
            el.set_text_generation(Rc::new(Cell::new(1u64)));
            el.set_text_vertical_center(true);
        }
        ctx.arena.add_child(slot_id, icon_id);

        // ── Gap 1 ──
        let gap1 = ctx.arena.allocate();
        {
            let Some(el) = ctx.arena.get_mut(gap1) else {
                return container_id;
            };
            el.set_preferred_width(Some(8.0));
            el.set_preferred_height(1.0);
            el.set_flex_shrink(0.0);
        }
        ctx.arena.add_child(slot_id, gap1);

        // ── Message ──
        let text_id = ctx.arena.allocate();
        ctx.preallocate(text_id, components::TEXT | components::LAYOUT);
        {
            let Some(el) = ctx.arena.get_mut(text_id) else {
                return container_id;
            };
            el.set_flex_grow(1.0);
            el.set_flex_shrink(1.0);
            el.set_preferred_height(font_size * 1.3 + 2.0);
            el.set_font_size(font_size);
            el.set_text_buffer(msg_buf.clone());
            el.set_text_generation(Rc::new(Cell::new(1u64)));
            el.set_text_vertical_center(true);
        }
        ctx.arena.add_child(slot_id, text_id);

        // ── Gap 2 (collapses when no buttons) ──
        let gap2 = ctx.arena.allocate();
        {
            let Some(el) = ctx.arena.get_mut(gap2) else {
                return container_id;
            };
            el.set_preferred_width(Some(8.0));
            el.set_preferred_height(1.0);
            el.set_flex_shrink(1.0);
        }
        ctx.arena.add_child(slot_id, gap2);

        // ── Action button ──
        let action_id = ctx.arena.allocate();
        ctx.preallocate(
            action_id,
            components::TEXT | components::LAYOUT | components::LIFECYCLE,
        );
        let action_si: Rc<Cell<bool>>;
        let action_rv: Rc<Cell<bool>>;
        {
            let Some(el) = ctx.arena.get_mut(action_id) else {
                return container_id;
            };
            el.set_flex_shrink(0.0);
            el.set_font_size(font_size);
            el.set_font_weight(500);
            el.set_text_buffer(action_buf.clone());
            el.set_text_generation(Rc::new(Cell::new(1u64)));
            el.set_text_vertical_center(true);
            el.set_padding(Padding {
                left: 8.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            });
            el.set_accepts_mouse(true);
            el.set_focusable(true);
            el.set_accessible_role(accesskit::Role::Button);
            el.set_slot_inactive(true);
            action_si = el.slot_inactive.clone();
            action_rv = Rc::new(Cell::new(false));
            el.set_reactive_visible(action_rv.clone());
        }
        ctx.arena.add_child(slot_id, action_id);

        // ── Close button (×) ──
        let close_id = ctx.arena.allocate();
        ctx.preallocate(close_id, components::TEXT | components::LAYOUT);
        {
            let cb = Rc::new(RefCell::new(create_buffer(
                "\u{2715}",
                16.0,
                1.0,
                400,
                None,
                Some(28.0),
                crate::style::TextAlign::Center,
            )));
            let Some(el) = ctx.arena.get_mut(close_id) else {
                return container_id;
            };
            el.set_preferred_width(Some(28.0));
            el.set_preferred_height(28.0);
            el.set_flex_shrink(0.0);
            el.set_font_size(16.0);
            el.set_text_buffer(cb);
            el.set_text_generation(Rc::new(Cell::new(1u64)));
            el.set_text_vertical_center(true);
            el.set_accepts_mouse(true);
            el.set_focusable(true);
            el.set_accessible_role(accesskit::Role::Button);
            el.set_accessible_label("Dismiss".to_string());
        }
        ctx.arena.add_child(slot_id, close_id);

        // ═══════════════════════════════════════════════════
        // Event handlers
        // ═══════════════════════════════════════════════════

        // Close → request exit
        if let Some(reg) = ctx.event_registry.as_mut() {
            EventHandler::new()
                .on_click({
                    let e = exit_req.clone();
                    move || {
                        e.set(true);
                    }
                })
                .on_action({
                    let e = exit_req.clone();
                    move |a| {
                        if a.kind == crate::event::action::ActionKind::Activate {
                            e.set(true);
                            crate::event::action::ActionOutcome::Consumed
                        } else {
                            crate::event::action::ActionOutcome::Unhandled
                        }
                    }
                })
                .register_all(reg, close_id);

            // Action → fire callback + exit
            EventHandler::new()
                .on_click({
                    let h = action_handler.clone();
                    let e = exit_req.clone();
                    move || {
                        if let Some(ref f) = *h.borrow() {
                            f();
                        }
                        e.set(true);
                    }
                })
                .register_all(reg, action_id);
        }

        // ═══════════════════════════════════════════════════
        // Portal registration
        // ═══════════════════════════════════════════════════
        crate::platform::portal::register_portal(container_id);

        // ═══════════════════════════════════════════════════
        // Initial position + style (BEFORE frame_tick captures variables)
        // ═══════════════════════════════════════════════════
        update_pos(
            vp_w.get(),
            vp_h.get(),
            margin,
            max_w,
            &pos_cell,
            TOAST_HEIGHT,
            position,
        );
        {
            let Some(el) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            el.set_background(info_style.background);
            el.set_foreground(info_style.foreground);
            el.set_corner_radius(info_style.corner_radius.top_left);
            el.set_border_width(0.0);
        }
        ctx.register_theme_component(
            container_id,
            &ResolvedComponentStyle::Toast(info_style),
            &info_role,
            &self.style,
        );

        // ═══════════════════════════════════════════════════
        // frame_tick — lifecycle loop
        // ═══════════════════════════════════════════════════
        let anim_state: Rc<RefCell<Option<AnimState>>> = Rc::new(RefCell::new(None));
        let current: Rc<RefCell<Option<ToastItem>>> = Rc::new(RefCell::new(None));
        let Some(el_cd) = ctx.arena.get_mut(container_id) else {
            return container_id;
        };
        let container_dirty = el_cd.dirty.clone();

        let Some(el_ft) = ctx.arena.get_mut(container_id) else {
            return container_id;
        };
        el_ft.set_frame_tick(Box::new(move || {
            let now = clock::now();
            let (vw, vh) = crate::core::frame_driver::CURRENT_VIEWPORT.with(|c| c.get());
            let vw = if vw < 1.0 { 960.0 } else { vw };
            let vh = if vh < 1.0 { 768.0 } else { vh };
            let mut changed = false;

            // ── Poll queue → start new toast ──
            let try_dequeue = |anim: &mut Option<AnimState>, cur: &mut Option<ToastItem>| {
                if anim.is_some() {
                    return;
                }
                let item = toast_domain().queue.borrow_mut().pop_front();
                if let Some(item) = item {
                    apply_content(
                        &item,
                        container_id,
                        icon_id,
                        text_id,
                        action_id,
                        &msg_buf,
                        &icon_buf,
                        &action_buf,
                        &action_handler,
                        &action_si,
                        &action_rv,
                        font_size,
                        icon_size,
                    );
                    *anim = Some(match position {
                        ToastPosition::Top => AnimState::Entering { start: now },
                        _ => AnimState::Entering { start: now },
                    });
                    *cur = Some(item);
                    visible.set(true);
                    portal_h_cell.set(TOAST_HEIGHT);
                    schedule_continuous();
                }
            };

            // ── Tick animation ──
            {
                let mut anim = anim_state.borrow_mut();
                match *anim {
                    Some(AnimState::Entering { start }) => {
                        let t = ((now - start).as_secs_f32() * 1000.0 / ENTER_MS).clamp(0.0, 1.0);
                        let eased = ease_out_cubic(t);
                        let slide = match position {
                            ToastPosition::Top => -100.0 + 100.0 * eased,
                            ToastPosition::Bottom => 100.0 - 100.0 * eased,
                        };
                        pos_offset.set(Vec2::new(0.0, slide));
                        set_opacity(container_id, eased);
                        update_pos(vw, vh, margin, max_w, &pos_cell, TOAST_HEIGHT, position);
                        changed = true;

                        if t >= 1.0 {
                            pos_offset.set(Vec2::ZERO);
                            set_opacity(container_id, 1.0);
                            let dur = current
                                .borrow()
                                .as_ref()
                                .map(|i| i.duration_ms)
                                .unwrap_or(DEFAULT_DURATION_MS);
                            let dl = if dur == 0 {
                                now + std::time::Duration::from_secs(86_400) // persistent
                            } else {
                                now + std::time::Duration::from_millis(dur)
                            };
                            *anim = Some(AnimState::Visible { deadline: dl });
                            scheduler::cancel(SCHEDULER_KEY);
                            if dur > 0 {
                                scheduler::schedule_at(dl, SCHEDULER_KEY);
                            }
                        }
                    }
                    Some(AnimState::Visible { deadline }) => {
                        if exit_req.get() {
                            exit_req.set(false);
                            *anim = Some(AnimState::Exiting { start: now });
                            scheduler::cancel(SCHEDULER_KEY);
                            schedule_continuous();
                            changed = true;
                        } else if deadline <= now {
                            *anim = Some(AnimState::Exiting { start: now });
                            scheduler::cancel(SCHEDULER_KEY);
                            schedule_continuous();
                            changed = true;
                        }
                        changed |=
                            update_pos(vw, vh, margin, max_w, &pos_cell, TOAST_HEIGHT, position);
                    }
                    Some(AnimState::Exiting { start }) => {
                        let t = ((now - start).as_secs_f32() * 1000.0 / EXIT_MS).clamp(0.0, 1.0);
                        let eased = ease_in_cubic(t);
                        let slide = match position {
                            ToastPosition::Top => -100.0 * (1.0 - eased),
                            ToastPosition::Bottom => 100.0 * eased,
                        };
                        pos_offset.set(Vec2::new(0.0, slide));
                        set_opacity(container_id, 1.0 - eased);
                        update_pos(vw, vh, margin, max_w, &pos_cell, TOAST_HEIGHT, position);
                        changed = true;

                        if t >= 1.0 {
                            *anim = None;
                            *current.borrow_mut() = None;
                            visible.set(false);
                            portal_h_cell.set(0.0);
                            pos_offset.set(Vec2::ZERO);
                            scheduler::cancel(SCHEDULER_KEY);
                        }
                    }
                    None => {
                        try_dequeue(&mut anim, &mut current.borrow_mut());
                    }
                }
            }

            // Second dequeue attempt (queue may have been filled during exit)
            {
                let mut guard = anim_state.borrow_mut();
                if guard.is_none() {
                    try_dequeue(&mut guard, &mut current.borrow_mut());
                }
            }

            if changed {
                container_dirty
                    .set(container_dirty.get() | DirtyFlags::MEASURE | DirtyFlags::REPAINT);
                crate::core::dirty_registry::register_dirty(container_id, DirtyFlags::MEASURE);
                crate::core::dirty_registry::register_dirty(container_id, DirtyFlags::REPAINT);
                crate::core::dirty_registry::bump_subtree_gen(container_id);
            }
        }));

        // ═══════════════════════════════════════════════════
        // Unmount cleanup
        // ═══════════════════════════════════════════════════
        {
            let did = container_id;
            let on_unmount = Rc::new(RefCell::new(Some(Box::new(move || {
                crate::platform::portal::remove_portal(did);
                scheduler::cancel(SCHEDULER_KEY);
                let dom = toast_domain();
                let mut wake = dom.wake.borrow_mut();
                if wake.as_ref().is_some_and(|(cid, _)| *cid == did) {
                    *wake = None;
                }
            }) as Box<dyn FnOnce()>)));
            crate::core::element::with_ct_mut(|ct| {
                ct.lc.entry(did).or_default().on_unmount = Some(on_unmount);
            });
        }

        container_id
    }
}

// ═══════════════════════════════════════════════════════════════════
// Internal helpers
// ═══════════════════════════════════════════════════════════════════

fn schedule_continuous() {
    scheduler::acquire_continuous(SCHEDULER_KEY);
}

fn kind_glyph(kind: ToastKind) -> &'static str {
    match kind {
        ToastKind::Info => "\u{2139}",
        ToastKind::Success => "\u{2713}",
        ToastKind::Warning => "\u{26A0}",
        ToastKind::Error => "\u{2717}",
    }
}

fn kind_colors(kind: ToastKind) -> (Color, Color) {
    match kind {
        ToastKind::Info | ToastKind::Success => (
            Color::rgba8(0x67, 0x79, 0xE8, 0xFF),
            Color::rgba8(0xFF, 0xFF, 0xFF, 0xFF),
        ),
        ToastKind::Warning => (
            Color::rgba8(0xFF, 0xD5, 0x4F, 0xFF),
            Color::rgba8(0x1A, 0x1C, 0x1E, 0xFF),
        ),
        ToastKind::Error => (
            Color::rgba8(0xF2, 0xB8, 0xB5, 0xFF),
            Color::rgba8(0x1A, 0x1C, 0x1E, 0xFF),
        ),
    }
}

/// Write the portal position cell (used by taffy for absolute positioning).
/// Returns `true` when the position actually changed (repaint needed).
fn update_pos(
    vw: f32,
    vh: f32,
    margin: f32,
    max_w: f32,
    pos_cell: &Rc<Cell<(f32, f32, f32)>>,
    height: f32,
    position: ToastPosition,
) -> bool {
    let w = (vw - 2.0 * margin).min(max_w);
    let x = (vw - w) * 0.5;
    let y = match position {
        ToastPosition::Bottom => vh - margin - height,
        ToastPosition::Top => margin,
    };
    let cur = pos_cell.get();
    if (cur.0 - x).abs() > 0.5 || (cur.1 - y).abs() > 0.5 || (cur.2 - w).abs() > 0.5 {
        pos_cell.set((x, y, w));
        return true;
    }
    false
}

/// Set the container's visual opacity via component tables.
fn set_opacity(eid: ElementId, opacity: f32) {
    crate::core::element::with_ct_mut(|ct| {
        ct.style.entry(eid).or_default().opacity = opacity.clamp(0.0, 1.0);
    });
}

/// Apply a new toast item's content to the pre-created slot elements.
#[allow(clippy::too_many_arguments)]
fn apply_content(
    item: &ToastItem,
    container_id: ElementId,
    icon_id: ElementId,
    text_id: ElementId,
    action_id: ElementId,
    msg_buf: &Rc<RefCell<cosmic_text::Buffer>>,
    icon_buf: &Rc<RefCell<cosmic_text::Buffer>>,
    action_buf: &Rc<RefCell<cosmic_text::Buffer>>,
    action_handler: &Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    action_si: &Rc<Cell<bool>>,
    action_rv: &Rc<Cell<bool>>,
    font_size: f32,
    icon_size: f32,
) {
    let (bg, fg) = kind_colors(item.kind);

    // Container background + foreground
    crate::core::element::with_ct_mut(|ct| {
        let s = ct.style.entry(container_id).or_default();
        s.background = Some(bg);
        s.foreground = Some(fg);
    });

    // Icon
    *icon_buf.borrow_mut() = create_buffer(
        kind_glyph(item.kind),
        icon_size,
        1.3,
        400,
        None,
        None,
        crate::style::TextAlign::Center,
    );
    crate::core::signal_bridge::force_refresh_label(icon_id);
    crate::core::element::with_ct_mut(|ct| {
        ct.style.entry(icon_id).or_default().foreground = Some(fg);
    });

    // Message
    *msg_buf.borrow_mut() = create_buffer(
        &item.message,
        font_size,
        1.3,
        400,
        None,
        Some(400.0),
        crate::style::TextAlign::Start,
    );
    crate::core::signal_bridge::force_refresh_label(text_id);
    crate::core::element::with_ct_mut(|ct| {
        ct.style.entry(text_id).or_default().foreground = Some(fg);
    });

    // Action button
    if let Some(ref action) = item.action {
        *action_buf.borrow_mut() = create_buffer(
            &action.label,
            font_size,
            1.3,
            500,
            None,
            None,
            crate::style::TextAlign::Center,
        );
        crate::core::signal_bridge::force_refresh_label(action_id);
        *action_handler.borrow_mut() = Some(action.callback.clone());
        action_si.set(false);
        action_rv.set(true);
        crate::core::element::with_ct_mut(|ct| {
            ct.style.entry(action_id).or_default().foreground = Some(fg);
        });
    } else {
        *action_handler.borrow_mut() = None;
        action_si.set(true);
        action_rv.set(false);
    }
}
