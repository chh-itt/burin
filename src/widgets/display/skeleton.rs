//! Skeleton — loading placeholder with shimmer animation.
//!
//! ## Animation strategy
//!
//! Uses a `frame_tick` callback that fires opportunistically — only when the
//! event loop is already awake (mouse movement, keyboard, other scheduled
//! deadlines).  No `schedule_at` / `schedule_continuous` is called, so the
//! skeleton never drives frames proactively.
//!
//! **Architectural choice**: "万物皆不主动跑帧" — nothing in the framework
//! drives the event loop.  The shimmer advances only as a side-effect of user
//! interaction.  Making it self-sustaining would require per-element discrete
//! timers that survive full event-loop sleeps — feasible but not implemented.
//! TextInput's cursor blink is the intended target pattern (discrete scheduler,
//! self-wake at 500 ms boundaries).
use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Color;
use crate::theme::m3::roles::{ComponentRole, DisplayRole};

pub struct Skeleton {
    width: f32,
    height: f32,
    circle: bool,
    animated: bool,
    style: StyleRefinement,
}

impl Skeleton {
    pub fn new() -> Self {
        Self {
            width: 200.0,
            height: 16.0,
            circle: false,
            animated: true,
            style: StyleRefinement::default(),
        }
    }

    pub fn rect(mut self, w: f32, h: f32) -> Self {
        self.width = w;
        self.height = h;
        self
    }
    pub fn circle(mut self, d: f32) -> Self {
        self.width = d;
        self.height = d;
        self.circle = true;
        self
    }
    pub fn animated(mut self, v: bool) -> Self {
        self.animated = v;
        self
    }
}

impl Styled for Skeleton {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Shimmer blend factor (0.0..=0.30) at `ms` on the animation timeline.
/// Period: 2600 ms sine wave — pure function, frame-rate independent.
pub fn shimmer_alpha(ms: u64) -> f32 {
    let phase = (ms % 2600) as f32 / 2600.0 * std::f32::consts::TAU;
    ((phase.sin() * 0.5 + 0.5) * 0.30).clamp(0.0, 0.30)
}

impl Widget for Skeleton {
    fn component_mask(&self) -> u64 {
        components::STYLE | components::LAYOUT | components::TRANSFORM | components::LIFECYCLE
    }

    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Display(DisplayRole::Skeleton);
        let s_resolved = match ctx.theme.resolve_component(&role) {
            crate::theme::m3::roles::ResolvedComponentStyle::Skeleton(s) => s,
            _ => unreachable!(),
        };
        let base = s_resolved.background;
        let highlight = s_resolved.shimmer;

        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        let Some(element) = ctx.arena.get_mut(id) else {
            return id;
        };
        element.set_preferred_width(Some(self.width));
        element.set_preferred_height(self.height);
        element.set_background(base);

        if self.circle {
            element.set_corner_radii(crate::style::CornerRadii::all(self.width * 0.5));
        } else {
            let cr = self
                .style
                .corner_radius
                .unwrap_or(crate::style::CornerRadii::all(4.0));
            element.set_corner_radii(cr);
        }

        if self.animated {
            let anim_id = id;
            let anim_dirty = element.dirty.clone();
            element.set_frame_tick(Box::new(move || {
                if !crate::core::dirty_registry::is_visible_chain_fast(anim_id) {
                    return;
                }
                let viewport = crate::core::frame_driver::CURRENT_VIEWPORT.with(|c| c.get());
                if crate::core::dirty_registry::is_offscreen(anim_id, viewport) {
                    return;
                }
                let alpha = shimmer_alpha(crate::core::clock::animation_millis());
                let c = Color::rgba8(
                    ((base.r + (highlight.r - base.r) * alpha) * 255.0) as u8,
                    ((base.g + (highlight.g - base.g) * alpha) * 255.0) as u8,
                    ((base.b + (highlight.b - base.b) * alpha) * 255.0) as u8,
                    255,
                );
                crate::core::dirty_registry::defer_action({
                    let cid = anim_id;
                    move |arena, _, _| {
                        let mut ct = arena.component_tables.borrow_mut();
                        ct.style.entry(cid).or_default().background = Some(c);
                        crate::core::dirty_registry::bump_surface_gen_remote(cid);
                    }
                });
                anim_dirty.set(anim_dirty.get() | crate::core::element::DirtyFlags::REPAINT);
            }));
        }

        if let Some(zi) = self.style.z_index {
            element.set_z_index(zi);
        }
        if let Some(o) = self.style.opacity {
            element.set_opacity(o);
        }
        if let Some(tx) = self.style.transform {
            element.set_transform(Some(tx));
        }

        ctx.register_theme_component(
            id,
            &crate::theme::m3::roles::ResolvedComponentStyle::Skeleton(s_resolved.clone()),
            &role,
            &self.style,
        );
        id
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Skeleton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Skeleton")
            .field("w", &self.width)
            .field("h", &self.height)
            .field("circle", &self.circle)
            .field("animated", &self.animated)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shimmer_alpha_is_time_pure_and_bounded() {
        for ms in [0u64, 100, 650, 1300, 1950, 2600, 10_000] {
            let a = shimmer_alpha(ms);
            assert!(
                (0.0..=0.30).contains(&a),
                "alpha {a} out of range at {ms}ms"
            );
        }
        assert!(
            (shimmer_alpha(0) - shimmer_alpha(2600)).abs() < 1e-4,
            "period is 2600ms"
        );
        assert!(
            shimmer_alpha(650) > shimmer_alpha(0),
            "rising phase in first quarter"
        );
        let huge = 650 + 2600 * 1_000_000_000u64;
        assert!(
            (shimmer_alpha(huge) - shimmer_alpha(650)).abs() < 1e-3,
            "no precision loss on large timestamps"
        );
    }
}
