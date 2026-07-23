# 主题系统

## Material 3

Burin 实现了 Material 3（Material You），配备自研的 HCT 色彩引擎。

```rust
use burin::theme::M3Theme;
use burin::style::Color;

// 从种子色生成
let theme = M3Theme::from_seed(Color::rgba8(103, 121, 232, 255));

// 从预设生成
let theme = M3Theme::from_preset(PresetTheme::neo_minimal_slate());

// 动态配色方案
let scheme = theme.scheme;
scheme.primary;       // 主色
scheme.surface;       // 背景色
scheme.on_surface;    // 背景上的文字色
scheme.error;         // 错误色
```

## Theme Trait

可插拔的 `Theme` trait，用于自定义设计系统：

```rust
pub trait Theme: Clone + 'static {
    fn scheme(&self) -> &ColorScheme;
    fn style_for(&self, role: &ComponentRole, state: StateFlags) -> ResolvedStyle;
    fn font_family(&self) -> &str;
    fn font_size(&self) -> f32;
}
```

运行时切换主题：

```rust
App::new()
    .window(config, my_ui())
    .theme(my_custom_theme)
    .run()
    .unwrap();
```

## HCT 色彩引擎

HCT（Hue、Chroma、Tone）是一个感知均匀的色彩空间。单个种子色可为每个色调生成
约 30 个色彩阶，并保证无障碍所需的对比度。

## 状态样式

Widget 自动响应交互状态：

```rust
// 样式解析器自动选择正确的变体：
StateFlags::NONE       // 默认
StateFlags::HOVERED    // 悬停
StateFlags::PRESSED    // 按下
StateFlags::FOCUSED    // 聚焦
StateFlags::DISABLED   // 禁用
StateFlags::SELECTED   // 选中/激活
```
