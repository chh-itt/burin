# Platform

## Window

```rust
use burin::platform::{App, WindowConfig};

App::new()
    .window(
        WindowConfig::default()
            .title("My App")
            .size(800.0, 600.0),
        my_ui(),
    )
    .run()
    .unwrap();
```

Multi-window:
```rust
App::new()
    .window(config_main, main_ui())
    .window(config_settings, settings_ui())
    .run()
    .unwrap();
```

## Portal System

`src/platform/portal.rs` — Portal-based overlays (dropdown menus, popovers,
tooltips) render at the root level but are anchored to their source element.
Portals automatically reposition when the source element moves or the viewport
resizes.

```rust
// Portals are managed by widgets internally (Select, ComboBox, Tooltip).
// The portal system handles:
// - Position tracking (follows anchor element)
// - Z-ordering (renders above everything)
// - Dismiss on outside click / Escape
```

## Clipboard

```rust
use burin::platform::clipboard::Clipboard;

Clipboard::write_text("copied text")?;
let text = Clipboard::read_text()?;
```

## Accessibility

All widgets generate an accessibility tree via AccessKit automatically.
The tree is built after each frame and dispatched to the platform accessibility API.

```rust
// Build the a11y tree (handled automatically by the frame driver)
burin::platform::build_accessibility_tree(&arena, root_id, focus_id);
```

## IME

IME composition events are handled by `TextInput`: preedit regions are rendered
inline, and commit events insert the finalized text.

## Drag and Drop

```rust
use burin::event::{DragData, DropType};

// Mark an element as draggable
el.set_draggable(true);

// Register drag handlers
MouseRegion::new(widget)
    .on_drag_start(|local, absolute| { /* initiate drag */ })
    .on_drag_update(|local, absolute| { /* drag move */ })
    .on_drag_end(|local, absolute| { /* drop */ });
```

## File Dialogs

```rust
// Requires "file-dialog" feature
let file = burin::platform::file_dialog::open_file()?;
```

## System Tray

```rust
// Requires "tray" feature
let tray = burin::platform::tray::TrayIcon::new()
    .icon(my_icon)
    .tooltip("My App")
    .build()?;
```
