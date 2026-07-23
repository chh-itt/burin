# 扩展性

Burin 的每个子系统都有开放的扩展点。没有封闭的接口。

## Widget

```rust
impl Widget for MyWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId { ... }
}
```

## 事件

```rust
// 自定义事件类型
pub enum Event { ..., Custom(MyCustomEvent) }

// 自定义 Recognizer
pub enum RecognizerKind { ..., Custom(Box<dyn CustomRecognizer>) }

// 注册处理器
registry.on_custom_event(id, |evt| { ... });
```

## ECS 组件

通过 `ComponentTables.extensions` 在元素上存储任意数据：

```rust
// 注册自定义组件类型
let type_id = TypeId::of::<MyComponent>();
ct.extensions.insert(type_id, HashMap::new());

// 存储在元素上
ct.extensions.get_mut(&type_id).unwrap().insert(eid, my_component);
```

## 渲染后端

```rust
impl RenderBackend for MyBackend {
    fn present(&mut self, commands: &[DrawCommand], size: Size) { ... }
}
```

## 主题

```rust
impl Theme for MyTheme {
    fn scheme(&self) -> &ColorScheme { ... }
    fn style_for(&self, role: &ComponentRole, state: StateFlags) -> ResolvedStyle { ... }
    // ...
}
```

## 自定义脏标记

```rust
// 第 8-31 位保留给第三方使用
const MY_DIRTY: DirtyFlags = DirtyFlags(1 << 8);
DirtyFlags::register_custom("my_flag", 1 << 8);
```
