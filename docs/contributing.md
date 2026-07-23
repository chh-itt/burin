# Contributing

## Project Structure

```
burin/
├── src/
│   ├── core/        Widget trait, Element, ElementArena, dirty registry
│   ├── event/       Event types, hit testing, gesture arena, propagation
│   ├── layout/      Taffy bridge, dirty propagation
│   ├── render/      Painter API, wgpu GPU, tiny-skia CPU, text shaping
│   ├── platform/    Window (winit), App, Portal, Clipboard, IME, AccessKit
│   ├── theme/       Material 3, HCT color engine, Theme trait
│   ├── animation/   Animation driver, easing curves, spring physics
│   ├── widgets/     Built-in widget library (layout/display/input/overlay/composite)
│   ├── style/       Color, Rect, Size, Dimension, Styled trait
│   ├── ecs/         Component tables, tracking sets
│   ├── testing/     TestHarness, snapshot, record/replay
│   └── debug/       DevTools, OverRenderDetector
├── tests/           Integration tests, perf regression suite
├── examples/        Demo applications
├── docs/            Documentation
├── auralis/         Signal kernel (auralis-signal, auralis-task, auralis-devtools)
└── burin-platform/  Platform FFI crate
```

## Conventions

- `#![forbid(unsafe_code)]` everywhere except platform boundaries
- `Signal<T>` is the only state primitive
- No proc macros, no DSL, no template language
- Composition over inheritance (traits, not class hierarchies)
- All external state must be behind `Signal<T>`

## Testing

```bash
# All tests
cargo test

# Perf regression suite
cargo test --profile bench --test perf_suite -- --ignored --nocapture

# DevTools causal tests
cargo test --profile bench --test perf_causal --features devtools -- --ignored --nocapture

# Snapshot tests (requires tiny-skia)
cargo test --test visual_regression --features backend-tiny-skia
```

## Code Style

Follow the existing patterns in each module. Key idioms:

- Widgets mount via `mount_box(&mut MountContext)` → `ElementId`
- Properties mutate via `Element::set_*` methods (with `dirty_registry::register_dirty`)
- Event handlers register on `EventRegistry`
- Custom components store data in `ComponentTables`

## License

MIT OR Apache-2.0
