# 贡献指南

## 项目结构

```
burin/
├── src/
│   ├── core/        Widget trait, Element, ElementArena, dirty registry
│   ├── event/       事件类型、HitTest、手势竞技场、传播
│   ├── layout/      Taffy 桥接、脏传播
│   ├── render/      Painter API、wgpu GPU、tiny-skia CPU、文本塑形
│   ├── platform/    窗口 (winit)、App、Portal、剪贴板、IME、AccessKit
│   ├── theme/       Material 3、HCT 色彩引擎、Theme trait
│   ├── animation/   动画驱动器、缓动曲线、弹簧物理
│   ├── widgets/     内置 Widget 库 (布局/显示/输入/覆盖层/复合)
│   ├── style/       Color、Rect、Size、Dimension、Styled trait
│   ├── ecs/         组件表、跟踪集
│   ├── testing/     TestHarness、快照、录制/回放
│   └── debug/       DevTools、OverRenderDetector
├── tests/           集成测试、性能回归套件
├── examples/        演示应用
├── docs/            文档
├── auralis/         Signal 内核 (auralis-signal, auralis-task, auralis-devtools)
└── burin-platform/  平台 FFI crate
```

## 约定

- `#![forbid(unsafe_code)]` 适用于除平台边界外的所有代码
- `Signal<T>` 是唯一的状态原语
- 无过程宏、无 DSL、无模板语言
- 组合优于继承（trait，而非类继承）
- 所有外部状态必须放在 `Signal<T>` 后面

## 测试

```bash
# 所有测试
cargo test

# 性能回归套件
cargo test --profile bench --test perf_suite -- --ignored --nocapture

# DevTools 因果测试
cargo test --profile bench --test perf_causal --features devtools -- --ignored --nocapture

# 快照测试（需要 tiny-skia）
cargo test --test visual_regression --features backend-tiny-skia
```

## 代码风格

遵循每个模块中的现有模式。关键惯用法：

- Widget 通过 `mount_box(&mut MountContext)` → `ElementId` 挂载
- 属性通过 `Element::set_*` 方法修改（配合 `dirty_registry::register_dirty`）
- 事件处理器在 `EventRegistry` 上注册
- 自定义组件在 `ComponentTables` 中存储数据

## 许可

MIT OR Apache-2.0
