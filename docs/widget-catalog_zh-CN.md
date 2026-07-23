# Widget 一览

60 个内置 Widget。纯 Rust。无 DSL。

## 布局

| Widget | 说明 |
|--------|------|
| `VStack` | 垂直堆叠，支持 gap |
| `HStack` | 水平堆叠，支持 gap |
| `ZStack` | 层叠堆叠（z-order） |
| `Center` | 将子元素居中 |
| `Expanded` | 填充剩余空间（flex-grow） |
| `Flexible` | 按比例伸缩 |
| `SizedBox` | 固定或边界尺寸容器 |
| `Spacer` | 空白填充 |
| `SafeArea` | 系统 UI 安全区域内边距 |
| `Padding` | 子元素内边距 |
| `Conditional` | 基于 Signal 显示两个子元素之一 |
| `GridRow` | CSS Grid 行，支持列跨度 |
| `StickyHeader` | ScrollView 吸顶头部 |
| `SplitPane` | 可拖拽调整大小的分栏 |
| `ScrollView` | 可滚动容器（无限/虚拟） |

## 显示

| Widget | 说明 |
|--------|------|
| `Text` | 文本，支持字号、粗细、对齐 |
| `Image` | 位图（PNG/JPEG/GIF/WebP） |
| `SvgImage` | SVG 矢量图 |
| `Badge` | 小型计数/状态指示 |
| `Chip` | 紧凑标签或筛选项 |
| `Avatar` | 圆形头像（图片或首字母） |
| `Icon` | Material/内嵌图标 |
| `Progress` | 线性或环形进度条 |
| `Skeleton` | 加载占位骨架屏 |
| `BarChart` | 柱状图 |
| `LineChart` | 折线/面积图 |
| `List` | 单列可选列表 |
| `Table` | 多列网格，支持排序/列宽调整/虚拟滚动 |
| `Tree` | 层级树，支持展开/折叠 |
| `PropertyGrid` | 键值属性面板 |
| `Calendar` | 月历网格 |
| `EmptyState` | 空状态占位，含标题和操作 |

## 输入

| Widget | 说明 |
|--------|------|
| `Button` | 可点击按钮（primary、secondary、text 变体） |
| `IconButton` | 纯图标按钮 |
| `TextInput` | 单行文本输入 |
| `NumberInput` | 带增减按钮的数字输入 |
| `PasswordInput` | 密码输入 |
| `Checkbox` | 带标签的复选框 |
| `Switch` | 开关切换 |
| `Slider` | 带步长的范围滑块 |
| `RadioButton` | 单选按钮 |
| `ComboBox` | 可编辑下拉搜索框 |
| `Select` | 下拉选择列表 |
| `DatePicker` | 带日历弹窗的日期选择 |
| `ColorPicker` | 带调色板的颜色选择 |
| `TextEditor` | 多行富文本编辑器 |
| `Form` | 带验证的表单容器 |

## 覆盖层

| Widget | 说明 |
|--------|------|
| `Modal` | 全屏模态覆盖 |
| `Dialog` | 警告或确认对话框 |
| `Popover` | 锚定浮动面板 |
| `Tooltip` | 悬停提示 |
| `Toast` | 自动消失的通知 |
| `ContextMenu` | 右键上下文菜单 |

## 复合

| Widget | 说明 |
|--------|------|
| `TabBar` | 标签导航栏 |
| `TabPanel` | 标签内容面板 |
| `Accordion` | 可展开的折叠面板 |
| `AudioPlayer` | 音频播放控件 |

## 自定义 Widget

实现 `Widget` trait：

```rust
use burin::core::{Widget, MountContext, ElementId};

struct MyWidget;

impl Widget for MyWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        if let Some(el) = ctx.arena.get_mut(id) {
            el.set_preferred_width(Some(100.0));
            el.set_preferred_height(30.0);
            el.set_background(ctx.theme.scheme.surface);
            el.set_paint_fn(|el, ctx, _fcx| {
                // 自定义绘制逻辑
            });
        }
        id
    }
}
```
