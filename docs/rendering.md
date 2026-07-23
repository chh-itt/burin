# Rendering

## Dual Backend

| | GPU | CPU |
|------|-----|-----|
| Crate | `wgpu` | `tiny-skia` |
| API | Vulkan / Metal / DX12 | Software rasterizer |
| Feature | `backend-wgpu` | `backend-tiny-skia` |
| When | Desktop, performance | No-GPU, CI, SSR |

Both backends implement the same `Painter` trait. Widget code never knows which
backend is active.

## Painter API

`src/render/painter.rs`. Unified drawing primitives:

```rust
// Inside a paint function:
painter.fill_rect(rect, color, radius);
painter.stroke_rect(rect, color, width, radius);
painter.fill_path(path, brush);
painter.stroke_path(path, stroke, brush);
painter.fill_linear_gradient(rect, gradient);
painter.draw_text(text_area, color);
painter.draw_image(image_id, rect);
```

## Scene Cache / Subtree Cache

`src/render/paint_tree.rs`. Two-level cache:

1. **`CachedSubtree`**: Stores the `Vec<DrawCommand>` for a subtree. When an
   element's `subtree_gen == cache_gen`, the entire subtree's draw commands are
   replayed from cache at O(1) per element.
2. **`CachedScene`**: Per-frame scene cache keyed by surface properties.

Cache invalidation: `bump_subtree_gen(eid)` increments the generation counter.
Any ancestor's cache that includes this element is invalidated.

## Text Rendering

`cosmic-text` for shaping, `glyphon` for GPU glyph atlas, `swash` for CPU
rasterization. Text is cached per `(string, font_size, max_width)` key.

## SSR (Server-Side Rendering)

```rust
let png_bytes = burin::render::ssr::render_to_png(my_widget, 800, 600)?;
std::fs::write("output.png", &png_bytes)?;
```

Requires `ssr` feature (enables `backend-tiny-skia` + `image`). No window, no GPU.
