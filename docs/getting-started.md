# Getting Started

## Installation

```toml
[dependencies]
burin = { git = "https://github.com/chh-itt/burin" }
auralis-signal = "0.1"
```

Default features include: wgpu GPU backend, tiny-skia CPU backend, Material 3
theme, clipboard, image/SVG support, and global hotkeys.

## Minimal App

```rust
use burin::prelude::*;
use auralis_signal::Signal;

fn main() {
    App::new()
        .window(WindowConfig::default(), Text::new("Hello, world!"))
        .run()
        .unwrap();
}
```

## Reactive Counter

```rust
use burin::prelude::*;
use auralis_signal::Signal;

fn counter() -> impl Widget {
    Compositor::new(|_scope| {
        let count = Signal::new(0i32);
        let label = Signal::new("0".to_string());

        VStack::new()
            .gap(12.0)
            .push(Text::new("Counter").font_size(24.0))
            .push(
                Text::new("0")
                    .bind(label.clone())
                    .font_size(48.0),
            )
            .push(Button::new("+1").on_click({
                let c = count.clone();
                let l = label.clone();
                move || {
                    let n = c.read() + 1;
                    c.set(n);
                    l.set(n.to_string());
                }
            }))
    })
}

fn main() {
    App::new()
        .window(WindowConfig::default(), counter())
        .run()
        .unwrap();
}
```

## Signals and Bindings

`Signal<T>` is the core state primitive. Reading auto-subscribes; writing auto-notifies.

```rust
let name = Signal::new("Alice".to_string());

// .bind() creates a reactive text widget
Text::new("").bind(name.clone());

// .on_click() fires when the button is activated
Button::new("Uppercase").on_click(move || {
    name.set(name.read().to_uppercase());
});
```

## Layout

```rust
VStack::new()
    .gap(8.0)
    .push(Text::new("Header").font_size(20.0))
    .push(
        HStack::new()
            .gap(4.0)
            .push(Button::new("Save").primary())
            .push(Button::new("Cancel")),
    )
    .push(Expanded::new(ScrollView::new().child(content_list)))
```

## Window Configuration

```rust
use burin::platform::WindowConfig;

let config = WindowConfig::default()
    .title("My App")
    .size(800.0, 600.0)
    .min_size(400.0, 300.0)
    .resizable(true);

App::new()
    .window(config, my_ui())
    .run()
    .unwrap();
```

## Feature Flags

```toml
[dependencies]
burin = { git = "...", default-features = false, features = [
    "backend-wgpu",
    "text-cosmic",
    "ext-image",
] }
```

| Feature | Default | Description |
|---------|:------:|-------------|
| `backend-wgpu` | ✓ | GPU rendering via wgpu |
| `backend-tiny-skia` | ✓ | CPU rasterization |
| `text-cosmic` | ✓ | Text shaping |
| `system-theme` | ✓ | Auto light/dark detection |
| `clipboard` | ✓ | Platform clipboard |
| `ext-image` | ✓ | Image loading (PNG/JPEG/GIF/WebP) |
| `ext-svg` | ✓ | SVG rendering |
| `ssr` | | Server-side render to PNG |
| `devtools` | | Signal inspector + perf panel |
| `i18n` | | Fluent internationalization |
| `tray` | | System tray icon |
| `file-dialog` | | Native file dialogs |
| `ext-jiff` | | Date/time support |
| `ext-audio` | | Audio playback |

## Next Steps

- [The Pipeline](PIPELINE.md), which describes how Signal becomes pixels
- [Widget Catalog](widget-catalog.md), which lists all 60 built-in widgets
- [Testing](testing.md), which covers TestHarness headless testing
