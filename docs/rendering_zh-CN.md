# 渲染

## 双后端

| | GPU | CPU |
|------|-----|-----|
| Crate | `wgpu` | `tiny-skia` |
| API | Vulkan / Metal / DX12 | 软件光栅化 |
| Feature | `backend-wgpu` | `backend-tiny-skia` |
| 场景 | 桌面、高性能 | 无 GPU、CI、SSR |

两个后端实现同一个 `Painter` trait。Widget 代码不需要知道当前使用的是哪个渲染器。

## Painter API

`src/render/painter.rs`。统一绘制原语：

```rust
// 在绘制函数内部:
painter.fill_rect(rect, color, radius);
painter.stroke_rect(rect, color, width, radius);
painter.fill_path(path, brush);
painter.stroke_path(path, stroke, brush);
painter.fill_linear_gradient(rect, gradient);
painter.draw_text(text_area, color);
painter.draw_image(image_id, rect);
```

## 场景缓存 / 子树缓存

`src/render/paint_tree.rs`。两级缓存：

1. **`CachedSubtree`**：存储子树的 `Vec<DrawCommand>`。当元素的 `subtree_gen == cache_gen`
   时，整个子树的绘制命令从缓存回放，O(1) 每个元素。
2. **`CachedScene`**：按 surface 属性键控的每帧场景缓存。

缓存失效：`bump_subtree_gen(eid)` 递增世代计数器。任何包含该元素的祖先缓存都会失效。

## 文本渲染

`cosmic-text` 用于塑形，`glyphon` 用于 GPU 字形图集，`swash` 用于 CPU 光栅化。
文本按 `(string, font_size, max_width)` 键缓存。

## SSR（服务端渲染）

```rust
let png_bytes = burin::render::ssr::render_to_png(my_widget, 800, 600)?;
std::fs::write("output.png", &png_bytes)?;
```

需要 `ssr` feature（启用 `backend-tiny-skia` + `image`）。无需窗口、无需 GPU。
