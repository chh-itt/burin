# 快速开始

## 安装

```toml
[dependencies]
burin = { git = "https://github.com/chh-itt/burin" }
auralis-signal = "0.1"
```

默认启用：wgpu GPU 后端、tiny-skia CPU 后端、Material 3 主题、剪贴板、图片/SVG 支持、全局快捷键。

## 最小应用

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

## 响应式计数器

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

## Signal 和绑定

`Signal<T>` 是核心状态原语。读取时自动订阅，写入时自动通知。

```rust
let name = Signal::new("Alice".to_string());

// .bind() 创建响应式文本 widget
Text::new("").bind(name.clone());

// .on_click() 在按钮激活时触发
Button::new("Uppercase").on_click(move || {
    name.set(name.read().to_uppercase());
});
```

## 布局

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

## 窗口配置

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

| Feature | 默认 | 说明 |
|---------|:---:|------|
| `backend-wgpu` | ✓ | wgpu GPU 渲染 |
| `backend-tiny-skia` | ✓ | CPU 光栅化 |
| `text-cosmic` | ✓ | 文本塑形 |
| `system-theme` | ✓ | 系统亮/暗模式检测 |
| `clipboard` | ✓ | 系统剪贴板 |
| `ext-image` | ✓ | 图片加载 (PNG/JPEG/GIF/WebP) |
| `ext-svg` | ✓ | SVG 渲染 |
| `ssr` | | 服务端渲染为 PNG |
| `devtools` | | Signal 检查器 + 性能面板 |
| `i18n` | | Fluent 国际化 |
| `tray` | | 系统托盘图标 |
| `file-dialog` | | 原生文件对话框 |
| `ext-jiff` | | 日期时间支持 |
| `ext-audio` | | 音频播放 |

## 下一步

- [渲染管线](PIPELINE_zh-CN.md) — Signal 如何变成像素
- [Widget 一览](widget-catalog_zh-CN.md) — 全部 60 个内置 Widget
- [自动化测试](testing_zh-CN.md) — TestHarness 无头测试
