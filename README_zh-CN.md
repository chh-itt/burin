# Burin

**一个 Rust GUI 框架，Signal 是唯一的状态。**

*[English](README.md)* | [架构全链路](docs/PIPELINE_zh-CN.md) | [文档索引](docs/README.md)

没有虚拟 DOM。没有 diff。没有依赖图。没有 reconciliation。只有 `Signal<T>` →
脏标记 → 增量布局 → 子树缓存 → 绘制。

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

## 为什么这么设计

Burin 基于独立的响应式内核 [Auralis](https://github.com/chh-itt/auralis) 构建。
`Signal<T>` 在读取时自动订阅，在写入时自动通知。以此为起点，一条单向管线将每个
信号变更连接到渲染层——没有虚拟 DOM、没有树 diff、没有 reconciliation：

```
Signal::set()
  → register_dirty           O(1) — 标记元素
  → process_dirty_set        O(k) — 沿祖先链上行，在容纳边界停止
  → Taffy 增量布局           4 条路径 — MEASURE / REPOSITION / 全跳过
  → SubtreeCache 检查        O(1) — 未变兄弟从缓存回放
  → paint_element_tree       仅重录脏子树
  → GPU (wgpu) 或 CPU (tiny-skia)    同一套 Painter API
```

每种语言催生不同的架构。Flutter 用百万行代码证明了保留模式增量渲染可行。
Burin 用少得多的代码达到了同等的架构质量——不是因为我们更出色，是 Rust
让这条路成为可能。

---

## Widget 一览

60 个内置 Widget。纯 Rust。无 DSL。

| 类别 | Widget |
|------|--------|
| 布局 | VStack, HStack, ZStack, Center, Expanded, Flexible, SizedBox, Spacer, SafeArea, Padding, Conditional, GridRow, StickyHeader, SplitPane, ScrollView |
| 显示 | Text, Image, SvgImage, Badge, Chip, Avatar, Icon, Progress, Skeleton, BarChart, LineChart, List, Table, Tree, PropertyGrid, Calendar, EmptyState |
| 输入 | Button, IconButton, TextInput, NumberInput, PasswordInput, Checkbox, Switch, Slider, RadioButton, ComboBox, Select, DatePicker, ColorPicker, TextEditor, Form |
| 覆盖层 | Modal, Dialog, Popover, Tooltip, Toast, ContextMenu |
| 复合 | TabBar, TabPanel, Accordion, AudioPlayer |

---

## 双渲染后端

| | GPU | CPU |
|------|-----|-----|
| 技术栈 | wgpu (Vulkan / Metal / DX12) | tiny-skia + softbuffer |
| 场景 | 桌面应用、高性能渲染 | 无 GPU 环境、CI、SSR |
| API | `Painter` trait | 同一个 `Painter` trait |

Widget 不需要知道自己在用哪个渲染器。写一次，到处渲染。

```
// 桌面窗口
App::new().window(config, my_ui()).run();

// 无头渲染为 PNG（不需要窗口、不需要 GPU）
let png = burin::render::ssr::render_to_png(my_ui(), 1200, 800)?;
```

---

## 手势竞技场

不是简单的 `on_click` 回调列表。采用 Flutter 风格的多手势仲裁系统：

```rust
MouseRegion::new(widget)
    .on_tap(|| println!("tap"))
    .on_drag(|dx, dy| println!("dragging {dx},{dy}"))
    .on_long_press(|| println!("held 500ms"))
```

7 种 Recognizer（Tap, Drag, EagerDrag, LongPress, DoubleTap, Scroll, Custom）在同一个竞技场中仲裁。一个 PointerDown 不会同时触发点击和拖拽。

---

## Material 3 主题

HCT 色彩引擎。单个种子色 → 完整亮色/暗色调色板：

```rust
let theme = M3Theme::from_seed(Color::rgba8(103, 121, 232, 255));
```

可插拔 `Theme` trait——可接入 Fluent、Cupertino 或自定义品牌主题。

---

## TestHarness——无头全帧测试

测试框架运行的是**与真实窗口完全相同**的 `drive_frame_*` 函数。不是 mock。同一个 HitTest。同一个手势竞技场。同一个布局引擎。

```rust
let mut h = TestHarness::new(800.0, 600.0);
let id = h.mount(Button::new("OK").on_click(|| println!("clicked")));
h.settle(8);

h.click_at(Point::new(40.0, 20.0)).run_frame();
h.assert_text(id, "OK");
```

还有：快照回归 (`assert_snapshot!`)、O(k) 断言 (`assert_subtree_cache_hits`)、录制回放、以及按维度拆分的性能回归套件。

---

## 性能

| 场景 | 开销 | 原因 |
|------|------|------|
| 静态 UI (空闲) | O(1) | SceneCache 全命中，零重绘 |
| 单 Signal 变化 | O(k) | k = 到最近布局边界的深度 |
| 滚动 | O(1) | 偏移变换，SubtreeCache 回放 |
| 动画 | O(k) | 仅脏子树重新录制 |
| 冷首帧 | O(N) | 全量布局 + 全量绘制 |

[完整性能套件 →](docs/PIPELINE_zh-CN.md)

---

## Feature Flags

| Feature | 默认 | 说明 |
|---------|:---:|------|
| `backend-wgpu` | ✓ | GPU 渲染 (Vulkan / Metal / DX12) |
| `backend-tiny-skia` | ✓ | CPU 光栅化 |
| `text-cosmic` | ✓ | 文本塑形与缓存 |
| `system-theme` | ✓ | 系统亮/暗模式检测 |
| `clipboard` | ✓ | 系统剪贴板 |
| `ext-image` | ✓ | 图片加载 (PNG / JPEG / GIF / WebP) |
| `ext-svg` | ✓ | SVG 渲染 |
| `ssr` | | 无头渲染为 PNG |
| `devtools` | | Signal 检查器 + 元素树 + 性能面板 |
| `i18n` | | Fluent 国际化 |
| `tray` | | 系统托盘图标 |
| `file-dialog` | | 原生文件对话框 |
| `ext-jiff` | | 日期时间 (Calendar, DatePicker) |
| `ext-audio` | | 音频播放 |

---

## 快速开始

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

*Burin 依赖 `winit 0.31`（当前为 beta）。我们持续跟踪上游进展。*

*如果这个项目帮到了你，可以考虑[请我喝杯咖啡](https://ifdian.net/a/chhitt) ([PayPal](https://www.paypal.com/paypalme/chhitt))。*

---

## 许可

MIT OR Apache-2.0
