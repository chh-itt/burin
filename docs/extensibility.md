# Extensibility

Every subsystem in Burin has open extension points. Nothing is sealed.

## Widget

```rust
impl Widget for MyWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId { ... }
}
```

## Event

```rust
// Custom event type
pub enum Event { ..., Custom(MyCustomEvent) }

// Custom recognizer
pub enum RecognizerKind { ..., Custom(Box<dyn CustomRecognizer>) }

// Register handlers
registry.on_custom_event(id, |evt| { ... });
```

## ECS Component

Store arbitrary data on elements via `ComponentTables.extensions`:

```rust
// Register a custom component type
let type_id = TypeId::of::<MyComponent>();
ct.extensions.insert(type_id, HashMap::new());

// Store on an element
ct.extensions.get_mut(&type_id).unwrap().insert(eid, my_component);
```

## Render Backend

```rust
impl RenderBackend for MyBackend {
    fn present(&mut self, commands: &[DrawCommand], size: Size) { ... }
}
```

## Theme

```rust
impl Theme for MyTheme {
    fn scheme(&self) -> &ColorScheme { ... }
    fn style_for(&self, role: &ComponentRole, state: StateFlags) -> ResolvedStyle { ... }
    // ...
}
```

## Custom Dirty Flags

```rust
// Bits 8-31 are reserved for third-party use
const MY_DIRTY: DirtyFlags = DirtyFlags(1 << 8);
DirtyFlags::register_custom("my_flag", 1 << 8);
```
