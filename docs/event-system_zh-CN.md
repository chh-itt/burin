# 事件系统

## 事件类型

```rust
pub enum Event {
    Click { position: Point, modifiers: Modifiers, finger_id: Option<u64> },
    PointerDown { position: Point, button: MouseButton, finger_id: Option<u64> },
    PointerMove { position: Point, finger_id: Option<u64> },
    PointerUp { position: Point, button: MouseButton, finger_id: Option<u64> },
    Scroll { delta_x: f32, delta_y: f32 },
    KeyDown { key: Key, modifiers: Modifiers },
    KeyUp { key: Key, modifiers: Modifiers },
}
```

## HitTest（命中测试）

`src/event/hit_test.rs:10` — 两级命中测试：

1. **`spatial_hit_test`**（O(1)）：空间哈希网格，按屏幕坐标索引。返回该点最深层的可见元素。
2. **`hit_test_leaf`**（O(N) 回退）：全深度遍历。仅在空间网格未命中时调用（元素尚未注册或已移出网格）。

结果：`HitTestResult { target, path (叶→根) }`。

## 传播

`src/event/propagation.rs:18` — `dispatch_event()` 通过命中路径路由事件：

- **捕获阶段**（根→叶）：Action（`KeyDown` → `dispatch_action`）先沿此方向遍历。
- **冒泡阶段**（叶→根）：大多数指针事件（`PointerDown`、`Click`）在最深层处理器处解决。

```rust
// 单元测试证明：最深处理器胜出
let path = [child, parent];  // 叶→根
propagate_click(&arena, &path, ...);
assert_eq!(fired.get(), "child");  // 不是 "parent"
```

## 手势竞技场

`src/event/recognizer.rs` — 7 种 Recognizer 在每个指针的单个竞技场中竞争：

| Recognizer | 胜出条件 |
|------------|---------|
| `TapRecognizer` | 快速按下-释放，无移动 |
| `DragRecognizer` | 超过 6px 移动阈值 |
| `EagerDragRecognizer` | PointerDown 时（无阈值） |
| `LongPressRecognizer` | 500ms 按住无移动 |
| `DoubleTapRecognizer` | 300ms 内两次点击 |
| `ScrollRecognizer` | 在可滚动表面上的触摸拖拽 |
| `Custom` | 用户自定义逻辑 |

关键保证：一个 `PointerDown` 永远不会同时触发点击和长按。

## 焦点

```rust
// 焦点遍历
h.press_key(Key::Tab, Modifiers::NONE);         // 下一个可聚焦元素
h.press_key(Key::Tab, Modifiers::SHIFT);         // 上一个可聚焦元素

// 编程式焦点
h.focus_manager.set_focused(Some(element_id));
```

## 键盘 → Action

`src/event/bindings.rs` — `KeyBindingMap` 将组合键映射为 Action：

```
Ctrl+A → ActionKind::SelectAll
Ctrl+C → ActionKind::Copy
Ctrl+V → ActionKind::Paste
Ctrl+Z → ActionKind::Undo
Tab    → ActionKind::FocusNext
Enter  → ActionKind::Activate
Escape → ActionKind::Cancel
...
```

`dispatch_action` 沿捕获→冒泡路由。如果未处理，`Activate`/`NewLine` 回退到对焦点元素执行 `fire_click`。

## 注册处理器

```rust
MouseRegion::new(widget)
    .on_click(|| {})
    .on_drag_start(|local, abs| {})
    .on_drag_update(|local, abs| {})
    .on_drag_end(|local, abs| {})
    .on_long_press(|| {})
    .on_hover_enter(|| {})
    .on_hover_leave(|| {})
    .on_scroll(|dx, dy| true)  // 返回 true 表示消费
```
