# Burin 渲染管线

## 为什么每个 GUI 框架都需要 diff 策略

每个保留模式 GUI 框架都必须回答一个问题：**"什么变了，需要重绘什么？"**

| 框架 | 策略 | 代价 |
|------|------|------|
| Flutter | 三棵树（Widget → Element → RenderObject）+ 四种脏标记 | O(N) 元素 diff |
| React | 虚拟 DOM + reconciliation | O(N) 虚拟树 diff |
| Iced | Elm 架构：`Message → update → view()` | O(N) 树重建 |
| GPUI | 每帧全量元素树重建 | O(N) 每帧重建 |
| Xilem | `View::rebuild(prev, new, state)` | O(N) 视图比较 |

这些策略存在的原因是 JavaScript、Dart 和 Elm 缺乏响应式原语。JavaScript 无法内建"这个变量变了，通知它的依赖者"。Dart 有 `ChangeNotifier` 但跟踪是手动的。Elm 的纯函数模型完全禁止副作用。Rust 用 `Signal<T>` 填补了这个空白。

---

## `Signal<T>` in Rust

[`auralis-signal`](https://github.com/chh-itt/auralis) 提供了一个原语，用于解决 diff 问题：

```rust
let count = Signal::new(0);

// 读取时自动订阅当前观察者作用域。
// 如果这次 read 发生在 paint 函数内，元素会自动订阅。
let current = count.read();       // 订阅

// 写入时自动通知所有订阅者。
count.set(1);                     // 通知 → 标记元素为脏
```

两个关键属性：

1. **读时订阅。** `Signal::read()` 检查是否有观察者处于活跃状态
   （`observe_element`，`src/core/signal_bridge.rs:230`）。如果有，Signal 将元素
   注册为依赖者。无需手动接线。

2. **写时通知。** `Signal::set()` 遍历订阅者列表，触发每个回调。Burin 的回调就是
   `register_dirty(eid, REPAINT)`。

这是根本性的不同：
- **Flutter 的 `setState`**：手动触发，标记整个 widget 子树。
- **Iced 的 `Message`**：类型级路由，每个 widget 都需要样板代码。
- **GPUI 的 `cx.notify()`**：手动通知，按 entity 操作。
- **Ribir 的 `Stateful<T>` / rxrust**：基于 Rx，跟踪字段级变化但需要显式的 `part_writer` 设置。

Burin 的 `Signal<T>` 是零样板、自动跟踪的，通知开销为 O(1)。

---

## 完整管线：单向，无循环

```
┌─────────────────────────────────────────────────────────────┐
│  winit 事件循环 (src/platform/window.rs:1569)                │
│                                                              │
│  原始输入 → EventTranslator → Event 枚举                      │
│       │                                                      │
│       ▼                                                      │
│  HitTest (src/event/hit_test.rs:10)                          │
│    → spatial_hit_test (O(1) 空间哈希网格)                    │
│    → 回退: hit_test_leaf (O(N)，仅网格未命中时)              │
│    → 返回 HitTestResult { target, path (叶→根) }             │
│       │                                                      │
│       ▼                                                      │
│  手势竞技场 (src/event/recognizer.rs)                        │
│    → process_pointer_event(phase, position, pointer_id)      │
│    → 7 种 Recognizer 竞争:                                   │
│      Tap | Drag | EagerDrag | LongPress | DoubleTap |        │
│      Scroll | Custom                                         │
│    → 竞技场裁决: 每个指针只有一个胜出者                      │
│       │                                                      │
│       ▼                                                      │
│  事件传播 (src/event/propagation.rs:18)                      │
│    → dispatch_event(arena, event, hit_path, focus, registry) │
│    → PointerDown: propagate_pointer_down (叶→根)             │
│    → KeyDown: dispatch_action (捕获 根→叶, 冒泡 叶→根)       │
│       │                                                      │
│       ▼                                                      │
│  EventRegistry 回调                                          │
│    → on_click, on_drag_start, on_drag_update, on_key_down    │
│                                                              │
│  ────── 回调内部: 用户修改 Signal ──────────                  │
│                                                              │
│  Signal::set(value)                                          │
│    → 订阅者回调触发:                                         │
│      → element.dirty |= REPAINT | MEASURE | REPOSITION       │
│      → app.register_dirty(eid, flags)       O(1)             │
│      → app.bump_subtree_gen(eid)            缓存失效          │
│                                                              │
│  ═══════════════════ on_frame() ═══════════════════════════  │
│                                                              │
│  Phase 1: drive_frame_layout (src/core/frame_driver.rs)     │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Prepass: blink, frame_tick, scroll kinetic, portals    │ │
│  │                                                        │ │
│  │ process_dirty_set (src/layout/dirty_propagation.rs:11) │ │
│  │   → 按深度排序（最深优先）                              │ │
│  │   → 对每个脏元素沿祖先链上行                            │ │
│  │   → 在祖先处合并 DirtyFlags                             │ │
│  │   → 在容纳边界停止:                                     │ │
│  │     • affected_by_child_size == false → 停止 REPOSITION │ │
│  │     • size_independent → 停止 MEASURE                   │ │
│  │   → 返回 (paint_roots, has_measure, processed,          │ │
│  │            layout_roots)                                │ │
│  │                                                        │ │
│  │ Taffy 增量布局 (4 条路径):                              │ │
│  │   1. INCREMENTAL  — 仅重新计算脏子树                    │ │
│  │   2. REPOSITION   — 单轴重定位                          │ │
│  │   3. ESCALATE     — 单轴升级为全量                      │ │
│  │   4. FULL         — 完整重算 (冷启动)                   │ │
│  │                                                        │ │
│  │ write_bounds → 空间网格更新 → HitTest 有效              │ │
│  └────────────────────────────────────────────────────────┘ │
│                          │                                   │
│  Phase 2: SEAM (平台: a11y, IME, drag, focus)               │
│                          │                                   │
│  Phase 3: drive_frame_paint (src/core/frame_driver.rs)     │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ 动画 tick → 插值 → 应用 → 重新检查脏标记               │ │
│  │                                                        │ │
│  │ paint_element_tree (src/render/paint_tree.rs)          │ │
│  │   → 遍历 must_paint 根集合（process_dirty_set 产出）   │ │
│  │   → 对每个元素:                                         │ │
│  │     • 有脏标记? → 重新录制 DrawCommands + 文本区域     │ │
│  │     • 干净?    → 从 CachedSubtree 回放 (O(1))          │ │
│  │     • subtree_gen == cache_gen? → 跳过整个子树          │ │
│  │                                                        │ │
│  │ 清除脏标记                                              │ │
│  └────────────────────────────────────────────────────────┘ │
│                          │                                   │
│                          ▼                                   │
│  BackendRenderer (src/render/)                               │
│    → GPU: wgpu swapchain present                             │
│    → CPU: tiny-skia pixmap → softbuffer present              │
└─────────────────────────────────────────────────────────────┘
```

---

## 我们没有的东西

### 没有虚拟 DOM

虚拟 DOM 是真实 DOM 的轻量副本，用于计算差异。Burin **没有虚拟 DOM**，
因为真实的元素树本身就是唯一的数据源。当 Signal 改变时，元素的属性就地修改。
没有东西需要 diff。

Flutter: Widget 树（短暂） → Element 树（持久） → RenderObject 树 → diff
Burin: Element 树（持久） → 脏标记 → process

### 精准寻址式推送

许多响应式系统维护一个显式的依赖图：节点 A 依赖 B 依赖 C。
Burin **不需要依赖图**——每个绑定走的是**精准寻址式推送**：

挂载时，当元素订阅一个 `Signal<T>`，订阅闭包里已经写死了目标 `ElementId`
和具体脏标记类型（如 `REPAINT`）。`Signal::set()` 触发时，直接调用：

```
register_dirty(element_id, REPAINT)
```

不查图、不遍历、不拓扑排序。绑定闭包在创建时就知道了更新该落在哪个元素上：
subscriber 列表只是一个平坦的 Vec，不是一棵依赖树。

**代价呢？** 每个绑定一个闭包，存在元素的 `LifecycleComponent` 里。元素销毁时
`Drop` 自动退订，无需手动清理，也不会有悬垂回调。auralis executor 在每帧开头
批量执行延迟通知，同一帧内多次 `set()` 合并为一次 `process_dirty_set`。

这套机制的维护成本实质上是零。Rust 的 `Drop` 替我们做了所有关系保洁：不需要图遍历、
不需要 GC、不需要手动取消订阅。Signal 直接推送到元素。元素销毁时，
订阅一并消亡。一切订阅关系在编译期就已经被类型系统和所有权模型静态化。

- Slint: Property<T> 运行时追踪，通过 thread-local CURRENT_BINDING 注册
- Ribir: rxrust 订阅图
- Burin: 精准寻址式推送——每个订阅者在挂载时已知目标 ElementId

### 没有 Reconciliation（调和）

调和是将旧树与新树比较，找出差异的过程。Burin **没有调和**，因为元素是就地修改的。
`DirtyFlags` 位掩码精确记录了变化内容（MEASURE, REPOSITION, REPAINT），
`process_dirty_set` 只向上传播必要的层级。

### 没有树重建

在 Elm 风格或即时模式框架中，每次状态变化都触发完整的树重建。Burin 使用
**保留模式**：元素在挂载时分配一次，所有后续更新都是就地修改。树的拓扑结构
永远不会因状态更新而改变。

---

## 所有权和 RAII

Burin 在三个关键点利用 Rust 的所有权模型：

1. **元素生命周期绑定到 Signal 订阅。**
   当元素通过 `bind_label_lazy`（`src/core/signal_bridge.rs:167`）绑定到 Signal 时，
   订阅被存储在元素的 `LifecycleComponent` 中。当元素从 arena 中移除时，释放
   component 会释放所有订阅句柄 → 没有泄漏的回调，没有为已销毁 ElementId 写入的幽灵脏标记。

2. **WeakSignal 保证异步安全。**
   异步回调（定时器、网络响应）使用 `WeakSignal` 防止悬垂引用。如果元素在回调触发前
   已被销毁，`WeakSignal::upgrade()` 返回 `None` → 回调成为空操作。

3. **`forbid(unsafe_code)` 涵盖除平台边界外的所有代码。**
   `src/lib.rs:80`：`#![forbid(unsafe_code)]` 指令适用于所有框架代码。唯一的
   `unsafe` 块存在于平台边界 crate（winit FFI、accesskit 包装、clipboard）。

---

## 为什么这是可测试的：TestHarness

测试框架（`src/testing/test_harness.rs`）运行的是**与生产窗口完全相同**的
`drive_frame_*` 函数。这不是 mock 测试——是真实管线在无头环境中运行。

```
生产环境:  winit 事件 → dispatch_events → hit_test → dispatch_event
            → on_frame → drive_frame_layout → drive_frame_paint → GPU present

TestHarness: 手动输入 → hit_test → dispatch_event
              → run_frame → drive_frame_layout → drive_frame_paint → CPU rasterize
```

关键推论：
- **Signal → dirty → paint** 可通过 `assert_subtree_cache_hits` 和
  `assert_frame_dirty_set_size` 验证
- **事件路由** 可通过 `click_at` → 真实 `dispatch_event` → 真实
  `process_pointer_event`（手势竞技场）验证
- **手势仲裁** 由 `tests/gesture_audit.rs` 证明了正确性——它发现并修复了
  手势竞技场中的真实生产 Bug
- **性能回归** 覆盖 15 个维度：帧阶段耗时、视口绑定绘制、Signal 延迟、空闲缩放、
  Arena 清理、缓存边界，以及通过 DevTools `signal_element_links` 的因果追踪

---

## 代码：用一段函数说明整个管线

```rust
use burin::prelude::*;
use auralis_signal::Signal;

fn pipeline_demo() -> impl Widget {
    Compositor::new(|_scope| {
        // ── 第 1 步: 创建 Signal ──
        let text = Signal::new("Hello".to_string());

        // ── 第 2 步: 绑定到 Text widget ──
        //   挂载时: bind_label_lazy() 订阅。
        //   Signal::set() 时: register_dirty(eid, MEASURE|REPAINT)
        let display = Text::new("").bind(text.clone());

        // ── 第 3 步: 一个修改 Signal 的按钮 ──
        let btn = Button::new("Change").on_click(move || {
            text.set("World!".to_string());
            // ── 接下来发生的事 ──
            // 1. text.set() 触发订阅者回调
            // 2. display.dirty |= MEASURE | REPAINT     (O(1))
            // 3. app.register_dirty(display_eid)         (O(1))
            // 4. 下一帧: process_dirty_set → 找到 display
            //    → 沿祖先上行到 VStack → pct 边界 → 停止
            // 5. Taffy 重定位（文字宽度变了）
            // 6. SubtreeCache: 兄弟回放, display 重录
            // 7. GPU present
        });

        VStack::new().gap(8.0).push(display).push(btn)
    })
}
```

框架处理从第 2 步到第 7 步的一切。开发者只需写 `Signal::new()`、`.bind()`、`.set()`。

---

## 代码对比

```rust
// Flutter — 手动 setState, widget 重建, element diff
// setState(() { _count++; }); // 标记子树为脏, 重建 widget 树

// Iced — message 枚举, update 函数, view 重建
// enum Message { Increment }
// fn update(&mut self, msg: Message) { ... }  // 手动路由
// fn view(&self) -> Element<Message> { ... }  // 完整树重建

// GPUI — 每帧全量元素重建 via Window::draw
// fn render(cx: &mut WindowContext) -> impl IntoElement { ... }

// Burin — 零样板 Signal 绑定
let count = Signal::new(0);
Button::new("+1").on_click(move || count.set(count.read() + 1));
Text::new("0").bind(count);
// 就这样。没有 setState，没有 Message 枚举，没有每帧重建。
```

---

## 扩展阅读

- [docs/PIPELINE_zh-CN.md](PIPELINE_zh-CN.md)：本文档
- [docs/testing_zh-CN.md](testing_zh-CN.md)：TestHarness、快照回归、O(k) 断言
- [docs/getting-started_zh-CN.md](getting-started_zh-CN.md)：安装、最小应用、Feature Flags
- [src/lib.rs](../src/lib.rs)：模块地图与设计原则
- [src/core/dirty_registry.rs](../src/core/dirty_registry.rs)：脏传播系统
- [src/event/propagation.rs](../src/event/propagation.rs)：事件分发与 Action 路由
- [tests/ok_assertions.rs](../tests/ok_assertions.rs)：O(k) 保证测试
