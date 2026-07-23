# 平台

## 窗口

```rust
use burin::platform::{App, WindowConfig};

App::new()
    .window(
        WindowConfig::default()
            .title("My App")
            .size(800.0, 600.0),
        my_ui(),
    )
    .run()
    .unwrap();
```

多窗口：
```rust
App::new()
    .window(config_main, main_ui())
    .window(config_settings, settings_ui())
    .run()
    .unwrap();
```

## Portal 系统

`src/platform/portal.rs` — 基于 Portal 的覆盖层（下拉菜单、Popover、Tooltip）
在根级别渲染，但锚定到源元素。当源元素移动或视口调整大小时，Portal 会自动重新定位。

```rust
// Portal 由 widget 内部管理（Select、ComboBox、Tooltip）。
// Portal 系统处理：
// - 位置跟踪（跟随锚点元素）
// - Z 排序（渲染在所有元素之上）
// - 外部点击 / Escape 关闭
```

## 剪贴板

```rust
use burin::platform::clipboard::Clipboard;

Clipboard::write_text("copied text")?;
let text = Clipboard::read_text()?;
```

## 无障碍

所有 Widget 通过 AccessKit 自动生成无障碍树。树在每帧后构建并分发到平台无障碍 API。

```rust
// 构建 a11y 树（由帧驱动器自动处理）
burin::platform::build_accessibility_tree(&arena, root_id, focus_id);
```

## IME

IME 组合事件由 `TextInput` 处理：预编辑区域内联渲染，提交事件插入最终文本。

## 拖放

```rust
use burin::event::{DragData, DropType};

// 将元素标记为可拖拽
el.set_draggable(true);

// 注册拖拽处理器
MouseRegion::new(widget)
    .on_drag_start(|local, absolute| { /* 开始拖拽 */ })
    .on_drag_update(|local, absolute| { /* 拖拽移动 */ })
    .on_drag_end(|local, absolute| { /* 释放 */ });
```

## 文件对话框

```rust
// 需要 "file-dialog" feature
let file = burin::platform::file_dialog::open_file()?;
```

## 系统托盘

```rust
// 需要 "tray" feature
let tray = burin::platform::tray::TrayIcon::new()
    .icon(my_icon)
    .tooltip("My App")
    .build()?;
```
