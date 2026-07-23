# 测试

Burin 提供 `TestHarness`——一个无头、无窗口、无 GPU 的测试驱动，运行的是
**与生产窗口完全相同**的 `drive_frame_*` 管线。

```rust
use burin::testing::TestHarness;
use burin::widgets::input::Button;
use burin::style::Point;

let mut h = TestHarness::new(800.0, 600.0);
let id = h.mount(Button::new("OK"));
h.settle(8);

// 通过真实的手势竞技场 + 事件传播来点击按钮。
h.click_at(Point::new(40.0, 20.0)).run_frame();
h.assert_text(id, "OK");
```

## 核心 API

```rust
// 创建
let mut h = TestHarness::new(width, height);
let id = h.mount(widget);

// 帧执行
h.run_frame();                  // 执行一帧
h.settle(max_frames);           // 运行到静止

// 交互模拟
h.click_at(pos);                // 通过真实 hit test + 竞技场的点击
h.pointer_down_at(pos);         // 原始 PointerDown（激活手势竞技场）
h.pointer_move_at(pos);         // 原始 PointerMove
h.pointer_up_at(pos);           // 原始 PointerUp
h.hover_at(pos);                // 完整的 hit-test 链差异
h.drag(from, to);               // drag_start → drag_update → drag_end
h.scroll(id, dx, dy);           // 滚动传播
h.press_key(key, mods);         // 键盘 → KeyBindingMap → dispatch_action
h.type_text(id, text);          // 逐字符输入

// 时间控制
h.advance_time(millis);         // 推进虚拟时钟
h.advance_to_next_deadline();   // 推进到调度器的下一个截止时间

// Signal 操作
h.set_signal(&signal, value);   // 设置 Signal 值
h.read_signal(&signal);         // 读取 Signal 值

// 断言
h.assert_text(id, "expected");           // 文本内容
h.assert_visible(id);                    // 可见性
h.assert_bounds(id, x, y, w, h);         // 屏幕位置
h.assert_focused(id);                    // 焦点状态
h.assert_child_count(id, count);         // 子元素数量
h.assert_dirty(id);                      // 脏标记
```

## O(k) 性能断言

与其他 GUI 测试框架不同，Burin 暴露了量化的性能保证：

```rust
// 断言未变兄弟从缓存回放。
h.assert_subtree_cache_hits(4);

// 断言布局保持了增量模式（未升级到全量）。
h.assert_no_relayout_escalation();

// 断言本帧处理的脏集合保持在较小规模。
h.assert_dirty_set_size(10);

// 断言绘制命令数量有上界。
h.assert_paint_command_count(50);
```

这些断言验证的是框架做了 O(k) 的**工作量**，而不仅仅是输出了正确的结果。

## 快照回归

```rust
use burin::assert_snapshot;

let mut h = TestHarness::new(400.0, 300.0);
h.mount(Button::new("Primary").primary());
h.settle(8);

assert_snapshot!(h, "button_primary");
```

将渲染像素与 `tests/snapshots/<name>.png` 比对。用 `AURALIS_UPDATE_SNAPSHOTS=1` 更新基线。
需要 `backend-tiny-skia` feature。

## 录制 / 回放

```rust
use burin::testing::TestRecorder;

// 录制交互。
let mut rec = TestRecorder::new(800.0, 600.0);
let id = rec.harness.mount(Button::new("Click me"));
rec.harness.find_mut(id).unwrap().set_test_id("btn");
rec.run_frame();
rec.click_on("btn");
let events = rec.into_events();

// 在全新的 harness 上回放。
let replayed = replay_events(
    |h| { h.mount(Button::new("Click me")); },
    &events,
);
```

## 性能回归套件

```bash
cargo test --profile bench --test perf_suite -- --ignored --nocapture
cargo test --profile bench --test perf_causal --features devtools -- --ignored --nocapture
```

12 个吞吐量维度：帧阶段耗时、视口绑定绘制、hover 空闲、注册表生命周期、
结构不变量、文本重建、启动成本、Signal 延迟、空闲缩放、Arena 清理、
缓存边界、Arena 碎片。

4 个因果维度（DevTools）：Signal→元素因果链、帧差异稳定性、布局震荡检测。

## 生产等价性

测试框架运行的是**与真实窗口完全相同的函数**：

| 子系统 | 生产环境 | 测试框架 | 相同代码？ |
|--------|---------|---------|:--------:|
| 帧管线 | `drive_frame_layout` → `drive_frame_paint` | 相同函数 | ✓ |
| HitTest | `spatial_hit_test` → fallback | 相同函数 | ✓ |
| 事件分发 | `propagation::dispatch_event` | 相同函数 | ✓ |
| 手势竞技场 | `process_pointer_event` | 相同函数 | ✓ |
| 键盘 | `KeyBindingMap::find` → `dispatch_action` | 相同函数 | ✓ |
| Hover 链 | 链差异 + 离开/进入传播 | 相同算法 | ✓ |
| 可访问性 | `build_accessibility_tree` | 相同函数 | ✓ |

这意味着：如果测试通过了，生产代码路径就是经过验证的——不是 mock。
