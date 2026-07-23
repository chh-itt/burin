# Changelog

## 0.1.0 — First Release

### Reactive Core

- `Signal<T>` auto-subscribes on read, auto-notifies on write, based on [Auralis](https://github.com/chh-itt/auralis)
- O(1) `register_dirty` + O(k) `process_dirty_set` ancestor propagation
- Subscription lifecycle managed automatically by Rust ownership (`Drop`)

### Rendering Pipeline

- Taffy incremental layout with 4 code paths: MEASURE / REPOSITION / partial bypass / full bypass
- SceneCache + SubtreeCache: unchanged subtrees replay from cache, zero re-recording
- GPU backend (wgpu: Vulkan / Metal / DX12) and CPU backend (tiny-skia + softbuffer) sharing a unified `Painter` API

### Event System

- GestureArena: 7 recognizers (Tap, Drag, EagerDrag, LongPress, DoubleTap, Scroll, Custom) competing in a single arena
- HitTest: O(1) spatial hashing
- Capture + bubble propagation
- Focus management with keyboard traversal

### Layout System

- Standard Flexbox + Grid via Taffy
- Incremental dirty propagation with containment boundary short-circuit
- `DirtyFlags` bitmask: MEASURE / REPOSITION / REPAINT

### Built-in Widgets (60)

| Category | Widgets |
|----------|---------|
| Layout | VStack, HStack, ZStack, Center, Expanded, Flexible, SizedBox, Spacer, SafeArea, Padding, Conditional, GridRow, StickyHeader, SplitPane, ScrollView |
| Display | Text, Image, SvgImage, Badge, Chip, Avatar, Icon, Progress, Skeleton, BarChart, LineChart, List, Table, Tree, PropertyGrid, Calendar, EmptyState |
| Input | Button, IconButton, TextInput, NumberInput, PasswordInput, Checkbox, Switch, Slider, RadioButton, ComboBox, Select, DatePicker, ColorPicker, TextEditor, Form |
| Overlay | Modal, Dialog, Popover, Tooltip, Toast, ContextMenu |
| Composite | TabBar, TabPanel, Accordion, AudioPlayer |

### Testing

- `TestHarness`: headless full-frame simulation — no window, no GPU needed
- Snapshot regression (`assert_snapshot!`)
- Record-replay
- Per-dimension performance regression suite

### Theming

- Material 3 HCT color engine
- Light / dark auto-detection
- Pluggable `Theme` trait

### Platform

- AccessKit accessibility integration (Windows / macOS / Linux)
- Clipboard, IME, drag-and-drop, file dialogs
- System tray, global hotkeys
- Multi-window support
- SSR: render to PNG headless (`render_to_png`)

### Diagnostics

- DevTools: signal inspector, element tree, performance panel
- Tracing and file-logging support
