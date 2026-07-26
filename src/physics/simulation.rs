/// 1D physics simulation trait.
/// Pure math: no side effects, no interior mutability.
pub trait Simulation {
    fn x(&self, t: f32) -> f32;
    fn dx(&self, t: f32) -> f32;
    fn is_done(&self, t: f32) -> bool;
}

// ── FrictionSimulation ──────────────────────────────────────────
//
// Fluid drag deceleration (exponential decay + optional constant term).
// Position:
//   x(t) = x₀ + v₀ × (d^t - 1) / ln(d)
// Velocity:
//   dx(t) = v₀ × d^t
//
// Where d = drag coefficient (0.135 for UIScrollView-like feel).
// Final position (t → ∞): x₀ - v₀ / ln(d)

pub struct FrictionSimulation {
    x0: f32,
    v0: f32,
    drag: f32,
    drag_ln: f32, // cached ln(drag)
    velocity_tolerance: f32,
}

impl FrictionSimulation {
    pub fn new(x0: f32, v0: f32, drag: f32, velocity_tolerance: f32) -> Self {
        let drag_ln = drag.ln();
        Self {
            x0,
            v0,
            drag,
            drag_ln,
            velocity_tolerance,
        }
    }
}

impl Simulation for FrictionSimulation {
    fn x(&self, t: f32) -> f32 {
        let pow = self.drag.powf(t);
        self.x0 + self.v0 * (pow - 1.0) / self.drag_ln
    }

    fn dx(&self, t: f32) -> f32 {
        self.v0 * self.drag.powf(t)
    }

    fn is_done(&self, t: f32) -> bool {
        self.dx(t).abs() < self.velocity_tolerance
    }
}

// ── SpringSimulation ────────────────────────────────────────────
//
// Damped harmonic oscillator:
//   m·x'' + c·x' + k·x = 0
//
// Three cases based on discriminant Δ = c² - 4mk:
//   Underdamped  (Δ < 0):  x(t) = e^(rt)[C₁·cos(ω·t) + C₂·sin(ω·t)]
//   Critically   (Δ = 0):  x(t) = (C₁ + C₂·t)·e^(rt)
//   Overdamped   (Δ > 0):  x(t) = C₁·e^(r₁·t) + C₂·e^(r₂·t)

enum SpringSolution {
    Underdamped {
        r: f32,
        omega: f32,
        c1: f32,
        c2: f32,
    },
    Critical {
        r: f32,
        c1: f32,
        c2: f32,
    },
    Overdamped {
        r1: f32,
        r2: f32,
        c1: f32,
        c2: f32,
    },
}

pub struct SpringSimulation {
    end: f32,
    solution: SpringSolution,
    distance_tolerance: f32,
    velocity_tolerance: f32,
}

impl SpringSimulation {
    /// Spring that settles at `end` from initial `position - end` with `velocity`.
    pub fn new(
        mass: f32,
        stiffness: f32,
        damping: f32,
        position: f32,
        end: f32,
        velocity: f32,
        distance_tolerance: f32,
        velocity_tolerance: f32,
    ) -> Self {
        // Solve in offset-from-end space
        let x0 = position - end;
        let disc = damping * damping - 4.0 * mass * stiffness;

        let solution = if disc > 0.0 {
            let sqrt_disc = disc.sqrt();
            let r1 = (-damping - sqrt_disc) / (2.0 * mass);
            let r2 = (-damping + sqrt_disc) / (2.0 * mass);
            let c2 = (velocity - r1 * x0) / (r2 - r1);
            let c1 = x0 - c2;
            SpringSolution::Overdamped { r1, r2, c1, c2 }
        } else if disc < 0.0 {
            let omega = (-disc).sqrt() / (2.0 * mass);
            let r = -damping / (2.0 * mass);
            let c1 = x0;
            let c2 = (velocity - r * x0) / omega;
            SpringSolution::Underdamped { r, omega, c1, c2 }
        } else {
            let r = -damping / (2.0 * mass);
            let c1 = x0;
            let c2 = velocity - r * x0;
            SpringSolution::Critical { r, c1, c2 }
        };

        Self {
            end,
            solution,
            distance_tolerance,
            velocity_tolerance,
        }
    }

    /// Convenience: underdamped spring with damping ratio (1.0 = critical, <1 = underdamped).
    pub fn with_damping_ratio(
        mass: f32,
        stiffness: f32,
        ratio: f32,
        position: f32,
        end: f32,
        velocity: f32,
        tolerance: f32,
    ) -> Self {
        let damping = ratio * 2.0 * (mass * stiffness).sqrt();
        Self::new(
            mass,
            stiffness,
            damping,
            position,
            end,
            velocity,
            tolerance,
            tolerance * 10.0,
        )
    }
}

impl Simulation for SpringSimulation {
    fn x(&self, t: f32) -> f32 {
        let offset = match self.solution {
            SpringSolution::Underdamped { r, omega, c1, c2 } => {
                let e = (r * t).exp();
                e * (c1 * (omega * t).cos() + c2 * (omega * t).sin())
            }
            SpringSolution::Critical { r, c1, c2 } => {
                let e = (r * t).exp();
                (c1 + c2 * t) * e
            }
            SpringSolution::Overdamped { r1, r2, c1, c2 } => {
                c1 * (r1 * t).exp() + c2 * (r2 * t).exp()
            }
        };
        self.end + offset
    }

    fn dx(&self, t: f32) -> f32 {
        match self.solution {
            SpringSolution::Underdamped { r, omega, c1, c2 } => {
                let e = (r * t).exp();
                let cos = (omega * t).cos();
                let sin = (omega * t).sin();
                e * (c2 * omega * cos - c1 * omega * sin) + r * e * (c2 * sin + c1 * cos)
            }
            SpringSolution::Critical { r, c1, c2 } => {
                let e = (r * t).exp();
                r * (c1 + c2 * t) * e + c2 * e
            }
            SpringSolution::Overdamped { r1, r2, c1, c2 } => {
                c1 * r1 * (r1 * t).exp() + c2 * r2 * (r2 * t).exp()
            }
        }
    }

    fn is_done(&self, t: f32) -> bool {
        (self.x(t) - self.end).abs() < self.distance_tolerance
            && self.dx(t).abs() < self.velocity_tolerance
    }
}

// ── ClampedSimulation (decorator) ───────────────────────────────

pub struct ClampedSimulation<S: Simulation> {
    inner: S,
    min: f32,
    max: f32,
}

impl<S: Simulation> ClampedSimulation<S> {
    pub fn new(inner: S, min: f32, max: f32) -> Self {
        Self { inner, min, max }
    }
}

impl<S: Simulation> Simulation for ClampedSimulation<S> {
    fn x(&self, t: f32) -> f32 {
        self.inner.x(t).clamp(self.min, self.max)
    }

    fn dx(&self, t: f32) -> f32 {
        // When clamped, velocity is 0
        let x = self.inner.x(t);
        if x < self.min || x > self.max {
            0.0
        } else {
            self.inner.dx(t)
        }
    }

    fn is_done(&self, t: f32) -> bool {
        let x = self.inner.x(t);
        (x >= self.min && x <= self.max && self.inner.is_done(t))
            || (x <= self.min && self.inner.dx(t) >= 0.0)
            || (x >= self.max && self.inner.dx(t) <= 0.0)
    }
}

// ── ConstantDecelSim (fixed deceleration) ──────────────────────

pub struct ConstantDecelSim {
    x0: f32,
    v0: f32,
    decel: f32, // always positive
}

impl ConstantDecelSim {
    pub fn new(x0: f32, v0: f32, decel: f32) -> Self {
        Self {
            x0,
            v0,
            decel: decel.abs(),
        }
    }

    pub fn with_distance(distance: f32, duration: f32) -> Self {
        let v0 = distance / (0.5 * duration);
        let decel = v0 / duration;
        Self::new(0.0, v0, decel)
    }
}

impl Simulation for ConstantDecelSim {
    fn x(&self, t: f32) -> f32 {
        let stop_t = self.v0 / self.decel;
        if t >= stop_t {
            self.x0 + self.v0 * stop_t - 0.5 * self.decel * stop_t * stop_t
        } else {
            self.x0 + self.v0 * t - 0.5 * self.decel * t * t
        }
    }

    fn dx(&self, t: f32) -> f32 {
        let stop_t = self.v0 / self.decel;
        if t >= stop_t {
            0.0
        } else {
            self.v0 - self.decel * t
        }
    }

    fn is_done(&self, t: f32) -> bool {
        t >= self.v0 / self.decel
    }
}

// ── BouncingScrollSimulation (iOS-style: friction → spring at boundary) ──
//
// Combines a FrictionSimulation for in-bounds deceleration with automatic
// transition to a SpringSimulation if the friction would overshoot the boundary.
// This creates the characteristic iOS "bounce past, then spring back" feel.

pub(crate) struct BouncingScrollSimulation {
    friction: FrictionSimulation,
    spring: Option<(SpringSimulation, f32)>,
}

impl BouncingScrollSimulation {
    pub fn new(
        position: f32,
        velocity: f32,
        leading: f32,
        trailing: f32,
        mass: f32,
        stiffness: f32,
        damping_ratio: f32,
    ) -> Self {
        let friction =
            FrictionSimulation::new(position, velocity, 0.135, DEFAULT_VELOCITY_TOLERANCE);
        let spring = Self::build_spring(
            &friction,
            position,
            velocity,
            leading,
            trailing,
            mass,
            stiffness,
            damping_ratio,
        );
        Self { friction, spring }
    }

    fn build_spring(
        friction: &FrictionSimulation,
        position: f32,
        velocity: f32,
        leading: f32,
        trailing: f32,
        mass: f32,
        stiffness: f32,
        damping_ratio: f32,
    ) -> Option<(SpringSimulation, f32)> {
        if position < leading {
            let s = SpringSimulation::with_damping_ratio(
                mass,
                stiffness,
                damping_ratio,
                position,
                leading,
                velocity,
                DEFAULT_DISTANCE_TOLERANCE,
            );
            return Some((s, 0.0));
        }
        if position > trailing {
            let s = SpringSimulation::with_damping_ratio(
                mass,
                stiffness,
                damping_ratio,
                position,
                trailing,
                velocity,
                DEFAULT_DISTANCE_TOLERANCE,
            );
            return Some((s, 0.0));
        }

        let drag_coeff: f32 = 0.135;
        let solve_for_boundary = |target: f32| -> Option<f32> {
            let drag_ln = drag_coeff.ln();
            let rhs = (target - position) * drag_ln / velocity + 1.0;
            if rhs <= 0.0 {
                return None;
            }
            let t = rhs.ln() / drag_ln;
            if t <= 0.0 || t > 10.0 {
                return None;
            }
            Some(t)
        };

        if velocity > 0.0 && position < trailing {
            let t = solve_for_boundary(trailing)?;
            let x_at_t = friction.x(t);
            let v_at_t = friction.dx(t).min(5000.0);
            let s = SpringSimulation::with_damping_ratio(
                mass,
                stiffness,
                damping_ratio,
                x_at_t,
                trailing,
                v_at_t,
                DEFAULT_DISTANCE_TOLERANCE,
            );
            Some((s, t))
        } else if velocity < 0.0 && position > leading {
            let t = solve_for_boundary(leading)?;
            let x_at_t = friction.x(t);
            let v_at_t = friction.dx(t).min(5000.0);
            let s = SpringSimulation::with_damping_ratio(
                mass,
                stiffness,
                damping_ratio,
                x_at_t,
                leading,
                v_at_t,
                DEFAULT_DISTANCE_TOLERANCE,
            );
            Some((s, t))
        } else {
            None
        }
    }
}

impl Simulation for BouncingScrollSimulation {
    fn x(&self, time: f32) -> f32 {
        match &self.spring {
            Some((spring, offset)) if time >= *offset => spring.x(time - offset),
            _ => self.friction.x(time),
        }
    }

    fn dx(&self, time: f32) -> f32 {
        match &self.spring {
            Some((spring, offset)) if time >= *offset => spring.dx(time - offset),
            _ => self.friction.dx(time),
        }
    }

    fn is_done(&self, time: f32) -> bool {
        match &self.spring {
            Some((spring, offset)) if time >= *offset => spring.is_done(time - offset),
            Some(_) => false,
            None => self.friction.is_done(time),
        }
    }
}

// ── ClampingScrollSimulation (Android-style deceleration) ──────
//
// Models the Android OverScroller / SplineOverScroller deceleration
// curve. Uses a power deceleration rather than exponential drag.
//
// kDecelerationRate = ln(0.78) / ln(0.9) ≈ 2.358
// physicalCoeff = g * 39.37 * 160 * 0.84 ≈ 518_276 px/s²
//
// Duration: computes from the Android physics model
// Distance: v₀ * duration / kDecelerationRate
// Position: x(t) = pos + dist × (1 − (1 − t/dur)^k)
// Velocity: v(t) = v₀ × (1 − t/dur)^(k−1)

pub(crate) struct ClampingScrollSimulation {
    position: f32,
    velocity: f32,
    distance: f32,
    pub(crate) duration: f32,
    deceleration_rate: f32,
    tolerance: f32,
}

impl ClampingScrollSimulation {
    const DECELERATION_RATE: f32 = -0.24846136 / -0.10536052; // ln(0.78) / ln(0.9)
    const INFLEXION: f32 = 0.35;
    const PHYSICAL_COEFF: f32 = 9.80665 * 39.37 * 160.0 * 0.84;

    pub fn new(position: f32, velocity: f32, friction: f32, tolerance: f32) -> Self {
        let deceleration_rate = Self::DECELERATION_RATE;
        let reference_velocity = friction * Self::PHYSICAL_COEFF / Self::INFLEXION;
        let android_duration =
            (velocity.abs() / reference_velocity).powf(1.0 / (deceleration_rate - 1.0));
        let duration = deceleration_rate * Self::INFLEXION * android_duration;
        let distance = velocity * duration / deceleration_rate;

        Self {
            position,
            velocity,
            distance,
            duration,
            deceleration_rate,
            tolerance,
        }
    }

    fn normalized_time(&self, time: f32) -> f32 {
        (time / self.duration).clamp(0.0, 1.0)
    }
}

impl Simulation for ClampingScrollSimulation {
    fn x(&self, time: f32) -> f32 {
        let t = self.normalized_time(time);
        self.position + self.distance * (1.0 - (1.0 - t).powf(self.deceleration_rate))
    }

    fn dx(&self, time: f32) -> f32 {
        let t = self.normalized_time(time);
        self.velocity * (1.0 - t).powf(self.deceleration_rate - 1.0)
    }

    fn is_done(&self, time: f32) -> bool {
        time >= self.duration || self.dx(time).abs() < self.tolerance
    }
}

// ── Tolerance constants ────────────────────────────────────────

pub const DEFAULT_VELOCITY_TOLERANCE: f32 = 10.0; // px/s
pub const DEFAULT_DISTANCE_TOLERANCE: f32 = 0.5; // px
pub const FLING_VELOCITY_THRESHOLD: f32 = 20.0; // px/s — below this, no fling
pub const FRICTION_DRAG: f32 = 0.135; // UIScrollView-like
