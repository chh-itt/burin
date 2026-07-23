# Animation

## Animated Properties

```rust
use burin::animation::Animation;

let opacity = Animation::new(0.0f32)
    .to(1.0)
    .duration_ms(300)
    .easing(EasingCurve::EaseOut);

// In paint: read current interpolated value
let current = opacity.value();
el.set_opacity(current);
```

## Easing Curves

```
EasingCurve::Linear
EasingCurve::EaseIn
EasingCurve::EaseOut
EasingCurve::EaseInOut
EasingCurve::Spring { stiffness, damping }
EasingCurve::Bounce
EasingCurve::Elastic
```

## Enter / Exit Animations

```rust
// Fade in on mount
Animation::new(0.0f32)
    .to(1.0)
    .duration_ms(200)
    .on_enter();

// Slide out on unmount
Animation::new(0.0f32)
    .to(100.0)
    .duration_ms(150)
    .on_exit();
```

## Animation Driver

`src/animation/`. `AnimationDriver` handles frame-by-frame interpolation.
Animations register with the driver via `app.register_animation(eid, flags)`.
On each frame, active animations are ticked, and elements affected by animation
are marked dirty automatically.

## Physics Simulations

```rust
use burin::physics::Spring;

let spring = Spring::new(stiffness: 200.0, damping: 20.0);
let position = spring.tick(target: 100.0, current: 0.0, dt: 0.016);
```
