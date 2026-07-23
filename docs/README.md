# Burin Documentation

[中文](#文档索引)

## Core

| Document | Content |
|----------|---------|
| [作者有话说](作者有话说.md) | 我们为什么做 Burin——一则关于范式选择的思考（[English](words-from-the-author.md)） |
| [PIPELINE.md](PIPELINE.md) | The full rendering pipeline: Signal → dirty → layout → paint → GPU/CPU |
| [getting-started.md](getting-started.md) | Installation, minimal app, WindowConfig, feature flags |
| [widget-catalog.md](widget-catalog.md) | 60 built-in widgets + custom widget guide |
| [event-system.md](event-system.md) | Event types, HitTest, GestureArena, propagation, focus, keyboard |
| [layout-system.md](layout-system.md) | Taffy incremental layout, DirtyFlags, dirty propagation, layout boundaries |
| [rendering.md](rendering.md) | Painter API, dual GPU/CPU backend, SceneCache, SubtreeCache |
| [theme.md](theme.md) | M3Theme, HCT color engine, Theme trait, dynamic color schemes |

## Platform & Testing

| Document | Content |
|----------|---------|
| [testing.md](testing.md) | TestHarness, snapshot regression, O(k) assertions, record-replay, DevTools causal tracing |
| [platform.md](platform.md) | Window, Portal, Clipboard, IME, AccessKit, drag-drop |
| [animation.md](animation.md) | AnimationDriver, easing curves, spring physics |
| [i18n.md](i18n.md) | Fluent internationalization |
| [extensibility.md](extensibility.md) | All extension points: Widget, Event, ECS, Render, Theme |
| [contributing.md](contributing.md) | Project structure, conventions, testing guide |

---

## 文档索引

## 核心

| 文档 | 内容 |
|------|------|
| [作者有话说](作者有话说.md) | 我们为什么做 Burin——一则关于范式选择的思考（[English](words-from-the-author.md)） |
| [PIPELINE_zh-CN.md](PIPELINE_zh-CN.md) | 完整渲染管线：Signal → 脏标记 → 布局 → 绘制 → GPU/CPU |
| [getting-started_zh-CN.md](getting-started_zh-CN.md) | 安装、最小应用、WindowConfig、Feature Flags |
| [widget-catalog_zh-CN.md](widget-catalog_zh-CN.md) | 60 个内置 Widget + 自定义 Widget 指南 |
| [event-system_zh-CN.md](event-system_zh-CN.md) | 事件类型、HitTest、手势竞技场、传播、焦点、键盘 |
| [layout-system_zh-CN.md](layout-system_zh-CN.md) | Taffy 增量布局、DirtyFlags、脏传播、布局边界 |
| [rendering_zh-CN.md](rendering_zh-CN.md) | Painter API、GPU/CPU 双后端、SceneCache、SubtreeCache |
| [theme_zh-CN.md](theme_zh-CN.md) | M3Theme、HCT 色彩引擎、Theme trait、动态配色 |

## 平台与测试

| 文档 | 内容 |
|------|------|
| [testing_zh-CN.md](testing_zh-CN.md) | TestHarness、快照回归、O(k) 断言、录制回放、DevTools 因果追踪 |
| [platform_zh-CN.md](platform_zh-CN.md) | Window、Portal、Clipboard、IME、AccessKit、拖放 |
| [animation_zh-CN.md](animation_zh-CN.md) | AnimationDriver、缓动曲线、弹簧物理 |
| [i18n_zh-CN.md](i18n_zh-CN.md) | Fluent 国际化 |
| [extensibility_zh-CN.md](extensibility_zh-CN.md) | 所有扩展点：Widget、Event、ECS、Render、Theme |
| [contributing_zh-CN.md](contributing_zh-CN.md) | 项目结构、编码规范、测试指南 |
