//! Animation Gallery — exercises every capability of the 2026-07-18
//! animation-architecture pass (Phases 0–3.5).
//!
//! What to look for:
//! 1. **Spinners** keep rotating even when the window is otherwise idle
//!    (pre-Phase-1 they froze until you moved the mouse). The wake is a
//!    renewal-model subscription: hide them and the event loop sleeps.
//! 2. **Skeleton shimmer** advances on the shared animation timeline
//!    (time-based phase — frame-rate independent).
//! 3. **Toast** slides in (300 ms), holds with ZERO repaints (watch the
//!    frame counter freeze), then slides out.
//! 4. **Driver animations** (fade / slide / spring) are pure functions of
//!    the clock — a dropped frame lands exactly on the analytic value.
//!    The spring uses `animator::register_animation` + SpringSimulation.
//! 5. **Visibility gating**: toggling the demo strip off releases every
//!    wake — the frame counter stops advancing (deep sleep).
//! 6. **Accordion** animates its height (200 ms, Prepass frame_tick +
//!    MEASURE — layout-class animation).
//!
//! The frame counter (top-right) is the observability probe: it advances
//! only when frames are actually produced.

use auralis_signal::Signal;
use burin::animation::{
    self, animator, set_animations_enabled, AnimatedProperty, AnimatedValue, Animation, EasingCurve,
};
use burin::core::context::MountContext;
use burin::core::widget::Widget;
use burin::core::{Compositor, ElementId};
use burin::physics::simulation::SpringSimulation;
use burin::platform::{App, WindowConfig};
use burin::style::styled::{StyleRefinement, Styled};
use burin::style::{Color, CornerRadii, Padding, Vec2};
use burin::theme::M3Theme;
use burin::widgets::composite::Accordion;
use burin::widgets::display::{Progress, ProgressKind, Skeleton, Text};
use burin::widgets::input::Button;
use burin::widgets::layout::*;
use burin::widgets::overlay::{toast, ToastContainer, ToastKind};
use std::collections::HashSet;

// ── Demo card: a colored box that animates itself on demand ────────────
//
// Widgets normally keep their ElementId internal; this demo card exposes
// "animate me" buttons by capturing its own id at mount time — the
// intended pattern for driver-animation consumers.

struct AnimTarget {
    color: Color,
    label: &'static str,
    style: StyleRefinement,
    on_mounted: Box<dyn FnOnce(ElementId)>,
}

impl AnimTarget {
    fn new(
        label: &'static str,
        color: Color,
        on_mounted: impl FnOnce(ElementId) + 'static,
    ) -> Self {
        Self {
            color,
            label,
            style: StyleRefinement::default(),
            on_mounted: Box::new(on_mounted),
        }
    }
}

impl Styled for AnimTarget {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl std::fmt::Debug for AnimTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnimTarget")
            .field("label", &self.label)
            .finish()
    }
}

impl Widget for AnimTarget {
    fn component_mask(&self) -> u64 {
        use burin::ecs::components;
        components::STYLE | components::LAYOUT | components::TRANSFORM | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let el = ctx.arena.get_mut(id).unwrap();
            el.set_preferred_width(Some(120.0));
            el.set_preferred_height(48.0);
            el.set_background(self.color);
            el.set_corner_radii(CornerRadii::all(8.0));
        }
        (self.on_mounted)(id);
        id
    }
}

fn driver_section() -> impl Widget {
    Compositor::new(|_| {
        let fade_id = Signal::new(ElementId::SENTINEL);
        let slide_id = Signal::new(ElementId::SENTINEL);
        let spring_id = Signal::new(ElementId::SENTINEL);

        let f1 = fade_id.clone();
        let f2 = slide_id.clone();
        let f3 = spring_id.clone();

        VStack::new()
            .gap(8.0)
            .push(
                Text::new("Driver animations (pure f(now) — Phase 2)")
                    .font_size(15.0)
                    .font_weight(700),
            )
            .push(
                HStack::new()
                    .gap(16.0)
                    .push(AnimTarget::new(
                        "fade",
                        Color::rgba8(0x67, 0x79, 0xE8, 0xFF),
                        move |id| f1.set(id),
                    ))
                    .push(AnimTarget::new(
                        "slide",
                        Color::rgba8(34, 197, 94, 255),
                        move |id| f2.set(id),
                    ))
                    .push(AnimTarget::new(
                        "spring",
                        Color::rgba8(239, 68, 68, 255),
                        move |id| f3.set(id),
                    )),
            )
            .push(
                HStack::new()
                    .gap(8.0)
                    .push(Button::new("Fade").small().on_click({
                        let s = fade_id.clone();
                        move || {
                            animation::request_anim(
                                s.read(),
                                AnimatedProperty::Opacity,
                                AnimatedValue::Float(0.0),
                                AnimatedValue::Float(1.0),
                                Animation {
                                    curve: EasingCurve::EaseInOut,
                                    duration_secs: 0.6,
                                },
                            );
                        }
                    }))
                    .push(Button::new("Slide (Vec2)").small().on_click({
                        let s = slide_id.clone();
                        move || {
                            animation::request_anim(
                                s.read(),
                                AnimatedProperty::Position,
                                AnimatedValue::Vec2(Vec2::new(-60.0, -10.0)),
                                AnimatedValue::Vec2(Vec2::ZERO),
                                Animation {
                                    curve: EasingCurve::EaseOut,
                                    duration_secs: 0.4,
                                },
                            );
                        }
                    }))
                    .push(Button::new("Spring (sim)").small().on_click({
                        let s = spring_id.clone();
                        move || {
                            // Underdamped spring: overshoots, then settles.
                            let sim = SpringSimulation::with_damping_ratio(
                                1.0, 160.0, 0.45, 0.0, 1.0, 0.0, 0.002,
                            );
                            animator::register_animation(
                                s.read(),
                                animator::AnimatedProperty::Opacity,
                                Box::new(sim),
                                Some(Box::new(|| {
                                    toast::show("spring settled", ToastKind::Success)
                                })),
                            );
                        }
                    })),
            )
    })
}

// ── Text demo target: a Text widget whose ElementId is captured ────────

struct TextTarget {
    label: &'static str,
    size: f32,
    on_mounted: Box<dyn FnOnce(ElementId)>,
}

impl std::fmt::Debug for TextTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextTarget")
            .field("label", &self.label)
            .finish()
    }
}

impl Widget for TextTarget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id =
            Box::new(Text::new(self.label).font_size(self.size).font_weight(700)).mount_box(ctx);
        (self.on_mounted)(id);
        id
    }
}

fn property_section() -> impl Widget {
    Compositor::new(|_| {
        let fg_id = Signal::new(ElementId::SENTINEL);
        let border_id = Signal::new(ElementId::SENTINEL);
        let shadow_id = Signal::new(ElementId::SENTINEL);
        let rot_id = Signal::new(ElementId::SENTINEL);

        let c1 = fg_id.clone();
        let c2 = border_id.clone();
        let c3 = shadow_id.clone();
        let c4 = rot_id.clone();

        VStack::new()
            .gap(8.0)
            .push(
                Text::new("Property surface (Phase-P: fg / border / shadow / rotation)")
                    .font_size(15.0)
                    .font_weight(700),
            )
            .push(
                HStack::new()
                    .gap(20.0)
                    .push(TextTarget {
                        label: "color me",
                        size: 18.0,
                        on_mounted: Box::new(move |id| c1.set(id)),
                    })
                    .push(AnimTarget::new(
                        "border",
                        Color::rgba8(0x2A, 0x2E, 0x3A, 0xFF),
                        move |id| c2.set(id),
                    ))
                    .push(AnimTarget::new(
                        "shadow",
                        Color::rgba8(0x67, 0x79, 0xE8, 0xFF),
                        move |id| c3.set(id),
                    ))
                    .push(AnimTarget::new(
                        "rotate",
                        Color::rgba8(34, 197, 94, 255),
                        move |id| c4.set(id),
                    )),
            )
            .push(
                HStack::new()
                    .gap(8.0)
                    .push(Button::new("Text color").small().on_click({
                        let s = fg_id.clone();
                        let flip = Signal::new(false);
                        move || {
                            let to_red = !flip.read();
                            flip.set(to_red);
                            let (from, to) = if to_red {
                                (
                                    Color::rgba8(0xE6, 0xE9, 0xF2, 0xFF),
                                    Color::rgba8(0xF2, 0x55, 0x4F, 0xFF),
                                )
                            } else {
                                (
                                    Color::rgba8(0xF2, 0x55, 0x4F, 0xFF),
                                    Color::rgba8(0xE6, 0xE9, 0xF2, 0xFF),
                                )
                            };
                            animation::request_anim(
                                s.read(),
                                AnimatedProperty::Foreground,
                                AnimatedValue::Color(from),
                                AnimatedValue::Color(to),
                                Animation {
                                    curve: EasingCurve::EaseInOut,
                                    duration_secs: 0.6,
                                },
                            );
                        }
                    }))
                    .push(Button::new("Border pulse").small().on_click({
                        let s = border_id.clone();
                        move || {
                            let id = s.read();
                            animation::request_anim(
                                id,
                                AnimatedProperty::BorderWidth,
                                AnimatedValue::Float(0.0),
                                AnimatedValue::Float(3.0),
                                Animation {
                                    curve: EasingCurve::EaseOut,
                                    duration_secs: 0.3,
                                },
                            );
                            animation::request_anim(
                                id,
                                AnimatedProperty::BorderColor,
                                AnimatedValue::Color(Color::rgba8(0x67, 0x79, 0xE8, 0x00)),
                                AnimatedValue::Color(Color::rgba8(0x67, 0x79, 0xE8, 0xFF)),
                                Animation {
                                    curve: EasingCurve::EaseOut,
                                    duration_secs: 0.3,
                                },
                            );
                        }
                    }))
                    .push(Button::new("Shadow lift").small().on_click({
                        let s = shadow_id.clone();
                        move || {
                            animation::request_anim(
                                s.read(),
                                AnimatedProperty::Shadow,
                                AnimatedValue::Shadow(burin::style::styled::Shadow::new(
                                    Color::rgba8(0, 0, 0, 0),
                                    0.0,
                                    0.0,
                                    0.0,
                                )),
                                AnimatedValue::Shadow(burin::style::styled::Shadow::new(
                                    Color::rgba8(0, 0, 0, 110),
                                    0.0,
                                    8.0,
                                    24.0,
                                )),
                                Animation {
                                    curve: EasingCurve::EaseOut,
                                    duration_secs: 0.3,
                                },
                            );
                        }
                    }))
                    .push(Button::new("Rotate 360°").small().on_click({
                        let s = rot_id.clone();
                        move || {
                            animation::request_anim(
                                s.read(),
                                AnimatedProperty::Rotation,
                                AnimatedValue::Float(0.0),
                                AnimatedValue::Float(360.0),
                                Animation {
                                    curve: EasingCurve::EaseInOut,
                                    duration_secs: 0.7,
                                },
                            );
                        }
                    })),
            )
    })
}

fn spinners_section() -> impl Widget {
    Compositor::new(|_| {
        VStack::new()
            .gap(8.0)
            .push(
                Text::new("Indeterminate spinners (renewal wake — Phases 0+1)")
                    .font_size(15.0)
                    .font_weight(700),
            )
            .push(
                Text::new("These keep moving on an idle window; hide them and the loop sleeps.")
                    .font_size(11.0),
            )
            .push(
                HStack::new()
                    .gap(24.0)
                    .push(
                        Progress::new(Signal::new(0.0))
                            .kind(ProgressKind::Circular)
                            .indeterminate(),
                    )
                    .push(Progress::new(Signal::new(0.0)).indeterminate())
                    .push(Progress::new(Signal::new(65.0))), // determinate: static, zero cost
            )
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(Skeleton::new().rect(160.0, 14.0))
                    .push(Skeleton::new().circle(28.0))
                    .push(Skeleton::new().rect(90.0, 14.0)),
            )
    })
}

fn accordion_section() -> impl Widget {
    Compositor::new(|_| {
        let open = Signal::new(HashSet::from([0usize]));
        VStack::new().gap(8.0)
            .push(Text::new("Accordion height animation (Prepass MEASURE — Phase 3.5)").font_size(15.0).font_weight(700))
            .push(Text::new("First open is instant (no recorded height); afterwards 200 ms ease-in-out.").font_size(11.0))
            .push(
                Accordion::new(open)
                    .section("What drives this?", Text::new(
                        "A Prepass frame_tick computes f(animation_millis) and writes\n\
                         preferred_height + MEASURE via defer_action — the driver phase\n\
                         runs after layout, so height animation must live in Prepass.").font_size(12.0))
                    .section("Second section", Text::new(
                        "Open and close these a few times.\nThe wake decays when idle (renewal model).").font_size(12.0))
            )
    })
}

fn toast_section() -> impl Widget {
    Compositor::new(|_| {
        VStack::new().gap(8.0)
            .push(Text::new("Toast (event-driven wake + quiescent hold — Phase 0)").font_size(15.0).font_weight(700))
            .push(Text::new("Enter 300 ms → hold 4 s with ZERO repaints (frame counter freezes) → exit 200 ms.").font_size(11.0))
            .push(
                HStack::new().gap(8.0)
                    .push(Button::new("Info toast").small().on_click(|| toast::show("saved — hold is quiescent", ToastKind::Info)))
                    .push(Button::new("Error toast").small().on_click(|| toast::show("something failed", ToastKind::Error)))
            )
    })
}

/// Frame counter probe: a frame_tick bumps a Cell every frame it runs.
/// When the loop sleeps, the number freezes — that IS the O(k) win.
struct FrameCounter;
impl std::fmt::Debug for FrameCounter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FrameCounter")
    }
}
impl Widget for FrameCounter {
    fn component_mask(&self) -> u64 {
        use burin::ecs::components;
        components::STYLE | components::LAYOUT | components::TEXT | components::LIFECYCLE
    }
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let label = Signal::new(String::from("frames: 0"));
        let n = std::rc::Rc::new(std::cell::Cell::new(0u64));
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let el = ctx.arena.get_mut(id).unwrap();
            el.set_layout_direction(burin::core::LayoutDirection::Horizontal);
            el.set_preferred_height(20.0);
            let label_tick = label.clone();
            el.set_frame_tick(Box::new(move || {
                n.set(n.get() + 1);
                label_tick.set(format!("frames: {}", n.get()));
            }));
        }
        let text_id = Box::new(Text::new("frames: 0").font_size(12.0).bind(label))
            .mount_box(&mut ctx.child_with_events(id));
        ctx.arena.add_child(id, text_id);
        id
    }
}

fn main() {
    // Enable automatic + accordion animations app-wide.
    set_animations_enabled(true);

    App::new()
        .window(WindowConfig {
            title: "Auralis UI — Animation Gallery".into(),
            width: 760.0,
            height: 720.0,
            theme: M3Theme::from_seed(Color::rgba8(0x67, 0x79, 0xE8, 0xFF))
                .preset(burin::theme::PresetTheme::neo_minimal_slate()),
            ..Default::default()
        }, Compositor::new(|_| {
            let show_anims = Signal::new(true);
            let toggle = show_anims.clone();
            ScrollView::new().child(
                VStack::new().padding(Padding::all(20.0)).gap(18.0)
                    .push(
                        HStack::new().gap(12.0)
                            .push(Text::new("Animation Gallery").font_size(20.0).font_weight(800))
                            .push(Spacer::new())
                            .push(FrameCounter)
                    )
                    .push(
                        HStack::new().gap(8.0)
                            .push(Button::new("Toggle animated strip").small().on_click(move || toggle.update(|v| *v = !*v)))
                            .push(Text::new("off → all wakes decay → frame counter freezes (deep sleep)").font_size(11.0))
                    )
                    .push(
                        Conditional::new(
                            show_anims.clone(),
                            VStack::new().gap(18.0)
                                .push(spinners_section())
                                .push(driver_section())
                                .push(property_section()),
                            Text::new("animated strip hidden — the event loop is asleep now").font_size(12.0),
                        )
                    )
                    .push(toast_section())
                    .push(accordion_section())
                    .push(ToastContainer::new())
            )
        }))
        .run()
        .expect("run");
}
