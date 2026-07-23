use auralis_signal::Signal;
use std::collections::HashSet;
use std::rc::Rc;

use crate::core::context::MountContext;
use crate::core::dirty_registry;
use crate::core::element::DirtyFlags;
use crate::core::widget::Widget;
use crate::style::styled::{StyleRefinement, Styled};
use crate::widgets::input::Button;

// ── AccordionSection ──

pub struct AccordionSection {
    pub title: String,
    pub subtitle: Option<String>,
    pub content: Option<Box<dyn Widget>>,
    pub disabled: bool,
}

impl AccordionSection {
    pub fn new(title: impl Into<String>, content: impl Widget + 'static) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            content: Some(Box::new(content)),
            disabled: false,
        }
    }

    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

// ── Accordion ──

pub struct Accordion {
    sections: Vec<AccordionSection>,
    open_signal: Signal<HashSet<usize>>,
    allow_multiple: bool,
    on_toggle: Option<Rc<dyn Fn(usize, bool)>>,
    style: StyleRefinement,
}

impl Accordion {
    pub fn new(open_signal: Signal<HashSet<usize>>) -> Self {
        Self {
            sections: Vec::new(),
            open_signal,
            allow_multiple: false,
            on_toggle: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn section(mut self, title: impl Into<String>, content: impl Widget + 'static) -> Self {
        self.sections.push(AccordionSection::new(title, content));
        self
    }

    pub fn section_with(mut self, sec: AccordionSection) -> Self {
        self.sections.push(sec);
        self
    }

    pub fn allow_multiple(mut self) -> Self {
        self.allow_multiple = true;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(usize, bool) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}

// ── Arrow helpers ──

const ARROW_OPEN: &str = "\u{25BC}";
const ARROW_CLOSED: &str = "\u{25B6}";

// ── Styled ──

impl Styled for Accordion {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

// ── Widget ──

impl Widget for Accordion {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> crate::core::element::ElementId {
        let allow_multiple = self.allow_multiple;
        let on_toggle = self.on_toggle;
        let gap = self.style.gap.unwrap_or(0.0);

        let container_id = ctx.arena.allocate();
        {
            let Some(container) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            container.set_layout_direction(crate::core::LayoutDirection::Vertical);
            container.set_gap(gap);
        }

        for (i, sec) in self.sections.into_iter().enumerate() {
            let is_open = self.open_signal.read().contains(&i);

            // ── Header button ──
            let arrow = if is_open { ARROW_OPEN } else { ARROW_CLOSED };
            let header_label = format!(" {}  {}", arrow, sec.title);

            let mut header_btn = Button::new(header_label)
                .appearance(crate::theme::Appearance::Text)
                .size(crate::theme::ControlSize::Small);

            if is_open {
                header_btn = header_btn.primary();
            }

            if sec.disabled {
                header_btn = header_btn.disabled();
            }

            let os = self.open_signal.clone();
            let dirty = ctx.arena.get(container_id).unwrap().dirty.clone();
            let ot = on_toggle.clone();
            let cid = container_id;

            header_btn = header_btn.on_click(move || {
                os.update(|set| {
                    if set.contains(&i) {
                        set.remove(&i);
                    } else {
                        if !allow_multiple {
                            set.clear();
                        }
                        set.insert(i);
                    }
                });
                if let Some(ref cb) = ot {
                    cb(i, os.read().contains(&i));
                }
                dirty.set(dirty.get() | DirtyFlags::MEASURE);
                dirty_registry::register_dirty(cid, DirtyFlags::MEASURE);
                dirty_registry::bump_subtree_gen(cid);
            });

            let header_id =
                Box::new(header_btn).mount_box(&mut ctx.child_with_events(container_id));
            {
                let Some(hd) = ctx.arena.get_mut(header_id) else {
                    return container_id;
                };
                hd.set_flex_shrink(0.0);
            }
            ctx.arena.add_child(container_id, header_id);

            // ── Content ──
            if let Some(widget) = sec.content {
                let content_id = ctx.arena.allocate();
                ctx.preallocate(
                    content_id,
                    crate::ecs::components::LAYOUT | crate::ecs::components::LIFECYCLE,
                );
                {
                    let Some(content_el) = ctx.arena.get_mut(content_id) else {
                        return container_id;
                    };
                    content_el.set_layout_direction(crate::core::LayoutDirection::Vertical);
                    content_el.set_padding(crate::style::Padding {
                        left: 32.0,
                        right: 8.0,
                        top: 4.0,
                        bottom: 8.0,
                    });

                    // Instant toggle path: slot_inactive controls layout
                    // (true = collapsed, no Taffy space). With animations
                    // enabled (Phase 3.5), toggling runs a 200ms height
                    // transition instead: a Prepass frame_tick computes
                    // `f(clock::animation_millis())` and drives
                    // preferred_height via MEASURE — the driver phase runs
                    // after layout, so a height animation must live here.
                    content_el.set_slot_inactive(!is_open);

                    // (tween, collapse) — None = idle. LayoutTween is the
                    // shared Prepass layout-animation primitive.
                    const HEIGHT_ANIM_MS: f32 = 200.0;
                    type HeightAnim = (crate::animation::LayoutTween, bool);
                    let anim: Rc<std::cell::RefCell<Option<HeightAnim>>> =
                        Rc::new(std::cell::RefCell::new(None));
                    let last_height: Rc<std::cell::Cell<f32>> = Rc::new(std::cell::Cell::new(0.0));
                    let wake_key = crate::core::scheduler::keys::element_key(
                        crate::core::scheduler::keys::NS_ACCORDION,
                        content_id,
                    );

                    let os2 = self.open_signal.clone();
                    let si = content_el.slot_inactive.clone();
                    let cd = content_el.dirty.clone();
                    let ci = content_id;
                    let os2_sub = os2.clone();
                    let anim_sub = anim.clone();
                    let last_h_sub = last_height.clone();
                    crate::core::signal_bridge::subscribe_owned(content_id, &os2_sub, move || {
                        let set = os2.read();
                        let v = set.contains(&i);
                        let currently_active = !si.get();
                        let animating = anim_sub.borrow().is_some();
                        if currently_active != v || animating {
                            if crate::animation::animations_enabled() {
                                if v {
                                    // Expand: participate in layout at once,
                                    // animate 0 → recorded height (first open
                                    // has no record → instant).
                                    si.set(false);
                                    let target = last_h_sub.get();
                                    if target > 1.0 {
                                        *anim_sub.borrow_mut() = Some((
                                            crate::animation::LayoutTween::start(
                                                0.01,
                                                target,
                                                HEIGHT_ANIM_MS,
                                                crate::animation::EasingCurve::EaseInOut,
                                            ),
                                            false,
                                        ));
                                        crate::core::scheduler::acquire_element_continuous(
                                            wake_key,
                                        );
                                    }
                                } else {
                                    // Collapse: record the natural height and
                                    // animate down; deactivate on completion.
                                    let cur = crate::core::dirty_registry::bounds_of(ci)
                                        .map_or(0.0, |r| r.height);
                                    if cur > 1.0 {
                                        last_h_sub.set(cur);
                                        si.set(false); // stay active while animating
                                        *anim_sub.borrow_mut() = Some((
                                            crate::animation::LayoutTween::start(
                                                cur,
                                                0.01,
                                                HEIGHT_ANIM_MS,
                                                crate::animation::EasingCurve::EaseInOut,
                                            ),
                                            true,
                                        ));
                                        crate::core::scheduler::acquire_element_continuous(
                                            wake_key,
                                        );
                                    } else {
                                        si.set(true);
                                    }
                                }
                            } else {
                                *anim_sub.borrow_mut() = None;
                                si.set(!v);
                            }
                            cd.set(cd.get() | DirtyFlags::REPAINT);
                            dirty_registry::register_dirty(ci, DirtyFlags::REPAINT);
                            dirty_registry::mark_structurally_changed(cid);
                            dirty_registry::register_dirty(cid, DirtyFlags::MEASURE);
                            dirty_registry::bump_subtree_gen(cid);
                        }
                    });

                    // Prepass height-transition tick (renewal-model wake).
                    let anim_tick = anim.clone();
                    let acc_id = cid;
                    content_el.set_frame_tick(Box::new(move || {
                        let Some((tween, collapse)) = *anim_tick.borrow() else {
                            return;
                        };
                        crate::core::scheduler::acquire_element_continuous(wake_key);
                        let (h, done) = tween.value_now();
                        if done {
                            *anim_tick.borrow_mut() = None;
                        }
                        dirty_registry::defer_action(move |arena, _, _| {
                            if done {
                                if collapse {
                                    // Deactivate; height override becomes moot.
                                    if let Some(el) = arena.get(ci) {
                                        el.slot_inactive.set(true);
                                    }
                                } else if let Some(el) = arena.get_mut(ci) {
                                    // Back to natural child-driven height.
                                    el.set_affected_by_child_size(true);
                                }
                            } else if let Some(el) = arena.get_mut(ci) {
                                el.set_preferred_height(h.max(0.01));
                                el.set_affected_by_child_size(false);
                            }
                            dirty_registry::register_dirty(acc_id, DirtyFlags::MEASURE);
                            dirty_registry::register_dirty(ci, DirtyFlags::REPAINT);
                            dirty_registry::bump_subtree_gen(acc_id);
                        });
                    }));
                }

                let child_id = {
                    let mut cc = ctx.child_with_events(content_id);
                    widget.mount_box(&mut cc)
                };
                ctx.arena.add_child(content_id, child_id);
                ctx.arena.add_child(container_id, content_id);
            }
        }

        // Auto-subscribe container to open_signal for reactive layout
        {
            let app_weak = ctx.app.clone();
            let Some(container) = ctx.arena.get_mut(container_id) else {
                return container_id;
            };
            let _obs = crate::core::signal_bridge::observe_element(container, app_weak);
            crate::core::signal_bridge::set_implicit_dirty(DirtyFlags::MEASURE);
            let _ = self.open_signal.read();
            drop(_obs);
            crate::core::signal_bridge::apply_observed_subscriptions(container);
        }
        container_id
    }
}

// ── Default + Debug ──

impl Default for Accordion {
    fn default() -> Self {
        Self::new(Signal::new(HashSet::new()))
    }
}

impl std::fmt::Debug for Accordion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Accordion")
            .field("sections", &self.sections.len())
            .field("allow_multiple", &self.allow_multiple)
            .finish_non_exhaustive()
    }
}
