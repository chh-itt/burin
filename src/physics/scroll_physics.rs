use crate::physics::simulation::*;

/// Pluggable scroll physics policy.
///
/// ## Input paths
///
/// | Input | ballistic? | boundary path |
/// |-------|-----------|---------------|
/// | Mouse wheel (discrete) | No → instant apply | `apply_boundary_conditions` ← NEW |
/// | Trackpad / drag (continuous) | No → instant apply | `apply_user_offset` + `apply_boundary_conditions` |
/// | Release / fling (velocity) | Yes → `ballistic_simulation` | Simulation runs unbounded, spring handles overscroll |
pub trait ScrollPhysics {
    /// Create a ballistic simulation (fling / snap-back) when scrolling ends.
    /// Return None → go idle immediately (velocity too low or within bounds).
    /// The simulation's x(t) output is fed to `ScrollBundle::apply_offset()` which
    /// does NOT clamp — overscroll is allowed so spring can bounce back.
    fn ballistic_simulation(
        &self,
        position: f32,
        velocity: f32,
        min_extent: f32,
        max_extent: f32,
    ) -> Option<Box<dyn Simulation>>;

    /// Determine how much of a desired pixel position change is rejected.
    ///
    /// Returns the overscroll amount — the actual pixel value is `desired - overscroll`.
    /// This delta semantics is critical: when position is already out-of-bounds
    /// (e.g. spring-back animation at -50) and moving back toward bounds (-40),
    /// returns 0.0 (allow), enabling smooth spring animation. Simple clamp-to-range
    /// would snap back to 0, losing the animation.
    ///
    /// Called for ALL position writes — user input, scrollbar, programmatic scroll.
    fn apply_boundary_conditions(
        &self,
        position: f32,
        desired: f32,
        min_extent: f32,
        max_extent: f32,
    ) -> f32;

    /// Modify a user drag delta during overscroll.
    /// Only called during continuous input (trackpad, drag) when `position` is
    /// outside [min_extent, max_extent]. Not called for discrete mouse wheel events.
    fn apply_user_offset(
        &self,
        position: f32,
        offset: f32,
        min_extent: f32,
        max_extent: f32,
    ) -> f32;
}

// ── ClampPhysics (Android-style) ────────────────────────────────

pub struct ClampPhysics;

impl ScrollPhysics for ClampPhysics {
    fn ballistic_simulation(
        &self,
        position: f32,
        velocity: f32,
        min_extent: f32,
        max_extent: f32,
    ) -> Option<Box<dyn Simulation>> {
        let out_of_range = position < min_extent || position > max_extent;

        if out_of_range {
            let end = if position < min_extent {
                min_extent
            } else {
                max_extent
            };
            let spring = SpringSimulation::with_damping_ratio(
                0.5,
                100.0,
                1.1,
                position,
                end,
                velocity.min(0.0),
                DEFAULT_DISTANCE_TOLERANCE,
            );
            return Some(Box::new(spring));
        }

        if velocity.abs() < FLING_VELOCITY_THRESHOLD {
            return None;
        }

        // At boundary and pushing outward: no fling
        if velocity > 0.0 && position >= max_extent {
            return None;
        }
        if velocity < 0.0 && position <= min_extent {
            return None;
        }

        let sim =
            ClampingScrollSimulation::new(position, velocity, 0.015, DEFAULT_VELOCITY_TOLERANCE);

        if sim.x(sim.duration) < min_extent || sim.x(sim.duration) > max_extent {
            Some(Box::new(ClampedSimulation::new(
                sim, min_extent, max_extent,
            )))
        } else {
            Some(Box::new(sim))
        }
    }

    fn apply_boundary_conditions(
        &self,
        position: f32,
        desired: f32,
        min_extent: f32,
        max_extent: f32,
    ) -> f32 {
        if min_extent == max_extent {
            return desired - position;
        }
        // Already underscrolled, trying to go further under → reject entire delta
        if desired < position && position <= min_extent {
            return desired - position;
        }
        // Already overscrolled, trying to go further over → reject entire delta
        if max_extent <= position && position < desired {
            return desired - position;
        }
        // In bounds, trying to go past the top → reject overflow portion
        if desired < min_extent && min_extent < position {
            return desired - min_extent;
        }
        // In bounds, trying to go past the bottom → reject overflow portion
        if position < max_extent && max_extent < desired {
            return desired - max_extent;
        }
        0.0
    }

    fn apply_user_offset(
        &self,
        _position: f32,
        offset: f32,
        _min_extent: f32,
        _max_extent: f32,
    ) -> f32 {
        offset
    }
}

// ── BouncePhysics (iOS-style) ──────────────────────────────────

pub struct BouncePhysics {
    pub spring_mass: f32,
    pub spring_stiffness: f32,
    pub spring_damping_ratio: f32,
    pub deceleration_rate: f32,
}

impl Default for BouncePhysics {
    fn default() -> Self {
        Self {
            spring_mass: 0.3,
            spring_stiffness: 75.0,
            spring_damping_ratio: 1.3,
            deceleration_rate: 0.998, // UIScrollView.decelerationRate == fast → 0.998? no, 0.135 is drag
        }
    }
}

impl ScrollPhysics for BouncePhysics {
    fn ballistic_simulation(
        &self,
        position: f32,
        velocity: f32,
        min_extent: f32,
        max_extent: f32,
    ) -> Option<Box<dyn Simulation>> {
        if velocity.abs() < FLING_VELOCITY_THRESHOLD
            && position >= min_extent
            && position <= max_extent
        {
            return None;
        }

        let sim = BouncingScrollSimulation::new(
            position,
            velocity,
            min_extent,
            max_extent,
            self.spring_mass,
            self.spring_stiffness,
            self.spring_damping_ratio,
        );
        Some(Box::new(sim))
    }

    fn apply_boundary_conditions(
        &self,
        _position: f32,
        _desired: f32,
        _min_extent: f32,
        _max_extent: f32,
    ) -> f32 {
        0.0
    }

    fn apply_user_offset(
        &self,
        position: f32,
        offset: f32,
        min_extent: f32,
        max_extent: f32,
    ) -> f32 {
        let range = max_extent - min_extent;
        if range <= 0.0 {
            return offset;
        }

        if position < min_extent {
            let overscroll_frac = (min_extent - position) / range;
            offset * (1.0 - overscroll_frac * overscroll_frac).max(0.1)
        } else if position > max_extent {
            let overscroll_frac = (position - max_extent) / range;
            offset * (1.0 - overscroll_frac * overscroll_frac).max(0.1)
        } else {
            offset
        }
    }
}

// ── PlatformPhysics ────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub type PlatformPhysics = BouncePhysics;

#[cfg(not(target_os = "macos"))]
pub type PlatformPhysics = ClampPhysics;

/// Create the platform-default scroll physics.
pub fn platform_physics() -> Box<dyn ScrollPhysics> {
    #[cfg(target_os = "macos")]
    {
        Box::new(BouncePhysics::default())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(ClampPhysics)
    }
}
