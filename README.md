# Burin

**A Rust GUI framework where Signal is the only state.**

*[中文](README_zh-CN.md)* | [Pipeline](docs/PIPELINE.md) | [Docs](docs/README.md)

No virtual DOM. No diff. No dependency graph. No reconciliation. Just `Signal<T>` →
dirty flag → incremental layout → subtree cache → paint.

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

---

## How it works

Burin builds on [Auralis](https://github.com/chh-itt/auralis), a standalone
reactive kernel. `Signal<T>` auto-subscribes on read and auto-notifies on write.
From there, a single-direction pipeline connects each signal change to the
rendering layer (no virtual DOM, no tree diff, no reconciliation):

```
Signal::set()
  → register_dirty           O(1) — mark the element
  → process_dirty_set        O(k) — walk up ancestors, stop at containment boundary
  → Taffy incremental layout 4 paths — MEASURE / REPOSITION / full bypass
  → SubtreeCache check       O(1) — unchanged siblings replay from cache
  → paint_element_tree       only dirty subtrees re-record
  → GPU (wgpu) or CPU (tiny-skia)    same Painter API for both
```

Different languages lead to different architectures. Flutter showed that
retained-mode incremental rendering works at scale. Burin provides the same
architectural guarantees (fine-grained dirty marking, incremental layout,
subtree caching) in a fraction of the code.

---

## Widgets

60 built-in widgets in pure Rust, with no DSL.

| Category | Widgets |
|----------|---------|
| Layout | VStack, HStack, ZStack, Center, Expanded, Flexible, SizedBox, Spacer, SafeArea, Padding, Conditional, GridRow, StickyHeader, SplitPane, ScrollView |
| Display | Text, Image, SvgImage, Badge, Chip, Avatar, Icon, Progress, Skeleton, BarChart, LineChart, List, Table, Tree, PropertyGrid, Calendar, EmptyState |
| Input | Button, IconButton, TextInput, NumberInput, PasswordInput, Checkbox, Switch, Slider, RadioButton, ComboBox, Select, DatePicker, ColorPicker, TextEditor, Form |
| Overlay | Modal, Dialog, Popover, Tooltip, Toast, ContextMenu |
| Composite | TabBar, TabPanel, Accordion, AudioPlayer |

---

## Dual backend

| | GPU | CPU |
|------|-----|-----|
| Stack | wgpu (Vulkan / Metal / DX12) | tiny-skia + softbuffer |
| When | Desktop, performance-critical | No-GPU, headless, CI, SSR |
| API | `Painter` trait | Same `Painter` trait |

Widgets don't need to know which backend is running. The same code works on any backend.

```
// Desktop window
App::new().window(config, my_ui()).run();

// Headless PNG (no window, no GPU — pure CPU)
let png = burin::render::ssr::render_to_png(my_ui(), 1200, 800)?;
```

---

## Gesture arena

Not a flat `on_click` callback list. A Flutter-style multi-recognizer arena that
arbitrates gesture conflicts:

```rust
MouseRegion::new(widget)
    .on_tap(|| println!("tap"))
    .on_drag(|dx, dy| println!("dragging {dx},{dy}"))
    .on_long_press(|| println!("held 500ms"))
```

7 recognizers (Tap, Drag, EagerDrag, LongPress, DoubleTap, Scroll, Custom)
compete in one arena. One PointerDown never fires both click and drag.

---

## Material 3 theme

HCT color engine. Single seed color → full light/dark palette:

```rust
let theme = M3Theme::from_seed(Color::rgba8(103, 121, 232, 255));
```

Pluggable `Theme` trait, with support for Fluent, Cupertino, or custom brand themes.

---

## TestHarness: headless full-frame testing

The test harness runs the **exact same** `drive_frame_*` functions as the real
window, with no mocking and the same hit test, gesture arena, and layout.

```rust
let mut h = TestHarness::new(800.0, 600.0);
let id = h.mount(Button::new("OK").on_click(|| println!("clicked")));
h.settle(8);

h.click_at(Point::new(40.0, 20.0)).run_frame();
h.assert_text(id, "OK");
```

Also: snapshot regression (`assert_snapshot!`), O(k) assertions (`assert_subtree_cache_hits`),
record-replay, and per-dimension perf regression suite.

---

## Performance

| Scenario | Cost | Why |
|----------|------|-----|
| Static UI (idle) | O(1) | SceneCache full hit, zero repaint |
| Single signal change | O(k) | k = depth to nearest layout boundary |
| Scrolling | O(1) | Offset transform, SubtreeCache replay |
| Animation | O(k) | Only dirty subtree re-records |
| Cold first frame | O(N) | Full layout + full paint |

[Full performance suite →](docs/PIPELINE.md)

---

## Feature flags

| Feature | Default | What |
|---------|:------:|------|
| `backend-wgpu` | ✓ | GPU rendering (Vulkan / Metal / DX12) |
| `backend-tiny-skia` | ✓ | CPU rasterization |
| `text-cosmic` | ✓ | Text shaping & caching |
| `system-theme` | ✓ | Auto light/dark |
| `clipboard` | ✓ | Platform clipboard |
| `ext-image` | ✓ | PNG / JPEG / GIF / WebP |
| `ext-svg` | ✓ | SVG rendering |
| `ssr` | | Render to PNG headless |
| `devtools` | | Signal inspector + element tree + perf panel |
| `i18n` | | Fluent internationalization |
| `tray` | | System tray icon |
| `file-dialog` | | Native file dialogs |
| `ext-jiff` | | Date/time (Calendar, DatePicker) |
| `ext-audio` | | Audio playback |

---

## Quick start

```toml
[dependencies]
burin = { git = "https://github.com/chh-itt/burin" }
auralis-signal = "0.1"
```

```rust
use burin::prelude::*;
use auralis_signal::Signal;

fn hello() -> impl Widget {
    Compositor::new(|_scope| {
        let msg = Signal::new("Hello, world!".to_string());
        Center::new(Padding::all(24.0, Text::new("").bind(msg)))
    })
}

fn main() {
    App::new()
        .window(WindowConfig::default(), hello())
        .run()
        .unwrap();
}
```

*Burin depends on `winit 0.31` (currently in beta). We track upstream closely.*

*If this project helps you, consider [buying me a coffee](https://www.paypal.com/paypalme/chhitt) or [sponsoring via 爱发电](https://ifdian.net/a/chhitt).*

---

## License

MIT OR Apache-2.0
