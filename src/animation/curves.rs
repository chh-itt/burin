//! Easing curves and animation configuration.

/// A declarative animation description.
#[derive(Clone, Copy, Debug)]
pub struct Animation {
    pub curve: EasingCurve,
    pub duration_secs: f32,
}

impl Animation {
    pub fn linear(duration_secs: f32) -> Self {
        Self {
            curve: EasingCurve::Linear,
            duration_secs,
        }
    }

    pub fn ease_in(duration_secs: f32) -> Self {
        Self {
            curve: EasingCurve::EaseIn,
            duration_secs,
        }
    }

    pub fn ease_out(duration_secs: f32) -> Self {
        Self {
            curve: EasingCurve::EaseOut,
            duration_secs,
        }
    }

    pub fn ease_in_out(duration_secs: f32) -> Self {
        Self {
            curve: EasingCurve::EaseInOut,
            duration_secs,
        }
    }

    pub fn spring(damping: f32, stiffness: f32) -> Self {
        Self {
            curve: EasingCurve::Spring { damping, stiffness },
            duration_secs: 0.5,
        }
    }

    /// Default animation: 200ms ease-in-out.
    pub fn default() -> Self {
        Self {
            curve: EasingCurve::EaseInOut,
            duration_secs: 0.2,
        }
    }

    /// 150ms ease-out (for toggles, checkboxes).
    pub fn toggle() -> Self {
        Self {
            curve: EasingCurve::EaseOut,
            duration_secs: 0.15,
        }
    }

    /// 350ms spring (for panels, drawers).
    pub fn slide() -> Self {
        Self {
            curve: EasingCurve::Spring {
                damping: 10.0,
                stiffness: 150.0,
            },
            duration_secs: 0.35,
        }
    }

    /// 50ms ease-out (for button press feedback).
    pub fn press() -> Self {
        Self {
            curve: EasingCurve::EaseOut,
            duration_secs: 0.05,
        }
    }
}

impl Default for Animation {
    fn default() -> Self {
        Self::ease_in_out(0.2)
    }
}

/// Easing curve algorithms.
#[derive(Clone, Copy, Debug)]
pub enum EasingCurve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Spring { damping: f32, stiffness: f32 },
}

/// Apply an easing curve to linear progress (0.0..1.0).
pub fn apply_easing(t: f32, curve: EasingCurve) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        EasingCurve::Linear => t,
        EasingCurve::EaseIn => t * t,
        EasingCurve::EaseOut => t * (2.0 - t),
        EasingCurve::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        EasingCurve::CubicBezier(x1, y1, x2, y2) => cubic_bezier(t, x1, y1, x2, y2),
        EasingCurve::Spring { damping, stiffness } => spring(t, damping, stiffness),
    }
}

fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // Newton-Raphson solve for x → t, then evaluate y(t)
    let mut guess = t;
    for _ in 0..8 {
        let x = cubic_bezier_val(guess, x1, x2) - t;
        let dx = cubic_bezier_deriv(guess, x1, x2);
        if dx.abs() < 1e-7 {
            break;
        }
        guess -= x / dx;
    }
    cubic_bezier_val(guess.clamp(0.0, 1.0), y1, y2)
}

fn cubic_bezier_val(t: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

fn cubic_bezier_deriv(t: f32, p1: f32, p2: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

fn spring(t: f32, damping: f32, stiffness: f32) -> f32 {
    // Approximate spring: 1 - e^(-damping*t) * cos(stiffness*t)
    let decay = (-damping * t).exp();
    let oscillation = (stiffness * t).cos();
    (1.0 - decay * oscillation).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_is_identity() {
        assert!((apply_easing(0.5, EasingCurve::Linear) - 0.5).abs() < 0.001);
    }

    #[test]
    fn ease_in_starts_slow() {
        let t = apply_easing(0.5, EasingCurve::EaseIn);
        assert!(t < 0.3); // Should be behind linear at midpoint
    }

    #[test]
    fn ease_out_starts_fast() {
        let t = apply_easing(0.5, EasingCurve::EaseOut);
        assert!(t > 0.7);
    }

    #[test]
    fn end_values_equal_one() {
        assert!((apply_easing(1.0, EasingCurve::EaseInOut) - 1.0).abs() < 0.001);
        assert!(
            (apply_easing(
                1.0,
                EasingCurve::Spring {
                    damping: 10.0,
                    stiffness: 100.0
                }
            ) - 1.0)
                .abs()
                < 0.01
        );
    }
}
