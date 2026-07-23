# 动画

## 动画属性

```rust
use burin::animation::Animation;

let opacity = Animation::new(0.0f32)
    .to(1.0)
    .duration_ms(300)
    .easing(EasingCurve::EaseOut);

// 在 paint 中: 读取当前插值
let current = opacity.value();
el.set_opacity(current);
```

## 缓动曲线

```
EasingCurve::Linear
EasingCurve::EaseIn
EasingCurve::EaseOut
EasingCurve::EaseInOut
EasingCurve::Spring { stiffness, damping }
EasingCurve::Bounce
EasingCurve::Elastic
```

## 进入 / 退出动画

```rust
// 挂载时淡入
Animation::new(0.0f32)
    .to(1.0)
    .duration_ms(200)
    .on_enter();

// 卸载时滑出
Animation::new(0.0f32)
    .to(100.0)
    .duration_ms(150)
    .on_exit();
```

## AnimationDriver

`src/animation/`。`AnimationDriver` 处理逐帧插值。动画通过
`app.register_animation(eid, flags)` 向驱动器注册。每帧，活跃的动画被 tick，
受动画影响的元素自动标记为脏。

## 物理模拟

```rust
use burin::physics::Spring;

let spring = Spring::new(stiffness: 200.0, damping: 20.0);
let position = spring.tick(target: 100.0, current: 0.0, dt: 0.016);
```
