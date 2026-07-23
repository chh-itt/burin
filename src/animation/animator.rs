//! Simulation-backed animation entry points.
//!
//! `register_animation` feeds a [`Simulation`](crate::physics::simulation::Simulation)
//! into the unified `AnimationDriver` (Phase 2, audit 2026-07-18 animation
//! pass — this module was a silent no-op compat shim before). The
//! simulation runs in normalized 0..1 progress space; the driver maps it
//! onto the property's value range.

use crate::animation::curves::EasingCurve;
use crate::animation::{AnimatedValue, ProgressSource};
use crate::core::element::ElementId;

pub struct TweenAnimation {
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub easing: EasingCurve,
}

impl crate::physics::simulation::Simulation for TweenAnimation {
    fn x(&self, t: f32) -> f32 {
        let progress = (t / self.duration).min(1.0);
        let eased = crate::animation::curves::apply_easing(progress, self.easing);
        self.from + (self.to - self.from) * eased
    }
    fn dx(&self, _t: f32) -> f32 {
        (self.to - self.from) / self.duration
    }
    fn is_done(&self, t: f32) -> bool {
        t >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::simulation::Simulation;

    #[test]
    fn tween_applies_easing_curve() {
        let tw = TweenAnimation {
            from: 0.0,
            to: 1.0,
            duration: 1.0,
            easing: EasingCurve::EaseIn,
        };
        // EaseIn = t² → at t=0.5 the value is 0.25, NOT the linear 0.5.
        let v = tw.x(0.5);
        assert!((v - 0.25).abs() < 1e-6, "EaseIn(0.5) = 0.25, got {v}");
        assert_eq!(tw.x(1.0), 1.0);
        assert!(tw.is_done(1.0));
    }
}

pub struct SequenceAnimation {
    pub segments: Vec<(f32, Box<dyn crate::physics::simulation::Simulation>)>,
}
impl crate::physics::simulation::Simulation for SequenceAnimation {
    fn x(&self, t: f32) -> f32 {
        let mut accum = 0.0;
        for (dur, sim) in &self.segments {
            if t < accum + dur {
                return sim.x(t - accum);
            }
            accum += dur;
        }
        self.segments.last().map_or(0.0, |(_, s)| s.x(1.0))
    }
    fn dx(&self, t: f32) -> f32 {
        let mut accum = 0.0;
        for (dur, sim) in &self.segments {
            if t < accum + dur {
                return sim.dx(t - accum);
            }
            accum += dur;
        }
        self.segments.last().map_or(0.0, |(_, s)| s.dx(0.0))
    }
    fn is_done(&self, t: f32) -> bool {
        t >= self.segments.iter().map(|(d, _)| d).sum::<f32>()
    }
}

pub struct RepeatAnimation {
    pub inner: Box<dyn crate::physics::simulation::Simulation>,
    pub duration: f32,
    pub count: u32,
}
impl crate::physics::simulation::Simulation for RepeatAnimation {
    fn x(&self, t: f32) -> f32 {
        if self.count == 0 || ((t / self.duration).floor() as u32) < self.count {
            self.inner.x(t % self.duration)
        } else {
            self.inner.x(self.duration)
        }
    }
    fn dx(&self, t: f32) -> f32 {
        self.inner.dx(t % self.duration)
    }
    fn is_done(&self, t: f32) -> bool {
        self.count > 0 && t >= self.duration * self.count as f32
    }
}

pub struct ParallelAnimation {
    pub children: Vec<Box<dyn crate::physics::simulation::Simulation>>,
}
impl crate::physics::simulation::Simulation for ParallelAnimation {
    fn x(&self, t: f32) -> f32 {
        self.children.first().map_or(0.0, |c| c.x(t))
    }
    fn dx(&self, t: f32) -> f32 {
        self.children.first().map_or(0.0, |c| c.dx(t))
    }
    fn is_done(&self, t: f32) -> bool {
        self.children.iter().all(|c| c.is_done(t))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimatedProperty {
    OffsetY,
    Opacity,
}

fn map_property(
    p: AnimatedProperty,
) -> (
    crate::animation::AnimatedProperty,
    AnimatedValue,
    AnimatedValue,
) {
    match p {
        // Simulation output feeds `interpolate(from, to, x(t))`; unit
        // endpoints make the simulation's own value range pass through.
        AnimatedProperty::Opacity => (
            crate::animation::AnimatedProperty::Opacity,
            AnimatedValue::Float(0.0),
            AnimatedValue::Float(1.0),
        ),
        AnimatedProperty::OffsetY => (
            crate::animation::AnimatedProperty::Position,
            AnimatedValue::Vec2(crate::style::Vec2::ZERO),
            AnimatedValue::Vec2(crate::style::Vec2::new(0.0, 1.0)),
        ),
    }
}

/// Register a simulation-driven animation on `target_id`. The simulation's
/// `x(t)` output (normalized 0..1, overshoot allowed for springs) is mapped
/// onto the property. `on_finish` fires exactly once, deferred to the next
/// phase boundary.
pub fn register_animation(
    target_id: ElementId,
    property: AnimatedProperty,
    simulation: Box<dyn crate::physics::simulation::Simulation>,
    on_finish: Option<Box<dyn FnOnce()>>,
) {
    let (prop, from, to) = map_property(property);
    crate::animation::request_anim_with(
        target_id,
        prop,
        from,
        to,
        ProgressSource::Sim(simulation),
        on_finish,
    );
}

/// Cancel all animations (running and queued) that target `target_id`.
pub fn unregister_animations(target_id: ElementId) {
    crate::animation::request_cancel(target_id);
}

/// Convenience: fixed-duration eased float animation on a property.
pub fn animate_float(
    target_id: ElementId,
    property: AnimatedProperty,
    from: f32,
    to: f32,
    duration_ms: u64,
    easing: EasingCurve,
) {
    let anim = crate::animation::Animation {
        curve: easing,
        duration_secs: duration_ms as f32 / 1000.0,
    };
    match property {
        AnimatedProperty::Opacity => crate::animation::request_anim(
            target_id,
            crate::animation::AnimatedProperty::Opacity,
            AnimatedValue::Float(from),
            AnimatedValue::Float(to),
            anim,
        ),
        AnimatedProperty::OffsetY => crate::animation::request_anim(
            target_id,
            crate::animation::AnimatedProperty::Position,
            AnimatedValue::Vec2(crate::style::Vec2::new(0.0, from)),
            AnimatedValue::Vec2(crate::style::Vec2::new(0.0, to)),
            anim,
        ),
    }
}
