# 布局系统

Burin 使用 [Taffy](https://github.com/DioxusLabs/taffy) 实现 Flexbox 和 CSS Grid 布局，
并配有自定义增量桥接，避免全树重算。

## DirtyFlags（脏标记）

`src/core/element.rs`。三层布局脏标记：

```
REPAINT    = 0b001   表面变化（颜色、边框、文本）。无需布局。
REPOSITION = 0b011   位置变化。Taffy 重定位（单轴）。
MEASURE    = 0b111   尺寸变化。Taffy 完整测量（双轴）。
```

## 脏传播

`src/layout/dirty_propagation.rs:11`。`process_dirty_set()`：

1. 从全局脏集合收集脏元素。
2. 按深度排序（最深优先）。
3. 对每个元素：沿祖先上行，合并 `DirtyFlags`。
4. 在容纳边界停止：
   - `affected_by_child_size == false` → 停止 `REPOSITION` 上行
   - `size_independent` → 停止 `MEASURE` 上行
5. 返回 `(paint_roots, has_measure, processed, layout_roots)`。

## Taffy 增量路径

| 路径 | 触发条件 | 开销 |
|------|---------|------|
| `INCREMENTAL` | 重布局边界内的重定位 | O(subtree) |
| `REPOSITION` | 单轴位置变化 | O(subtree) |
| `ESCALATE` | 边界的依赖轴变化 | O(subtree) → O(full) |
| `FULL` | 冷启动或强制 | O(N) |

## 布局边界

当 `affected_by_child_size == false` 时，widget 成为**重布局边界**。
子元素尺寸变化在该边界停止。父元素及其兄弟不会被重新布局。

```rust
el.set_affected_by_child_size(false);
```

## 常用布局

```rust
// Flexbox
VStack::new().gap(8.0).push(a).push(b)
HStack::new().gap(4.0).push(a).push(b)

// 固定尺寸
SizedBox::new().width(200.0).height(100.0).child(widget)

// 扩展填充
Expanded::new(widget)

// 按比例伸缩
Flexible::new(2.0, widget)

// 网格
GridRow::new().columns(12)
    .push(GridItem::new(widget).cols(6))
    .push(GridItem::new(widget).cols(6))
```
