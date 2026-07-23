# Widget Catalog

60 built-in widgets. All pure Rust. No DSL.

## Layout

| Widget | Description |
|--------|-------------|
| `VStack` | Vertical stack with gap |
| `HStack` | Horizontal stack with gap |
| `ZStack` | Layered stack (z-order) |
| `Center` | Centers child in available space |
| `Expanded` | Fills remaining space (flex-grow) |
| `Flexible` | Proportional flex with factor |
| `SizedBox` | Fixed or bounded size container |
| `Spacer` | Empty space filler |
| `SafeArea` | Pads around system UI (notch, taskbar) |
| `Padding` | Padding around child |
| `Conditional` | Shows one of two children based on signal |
| `GridRow` | CSS Grid row with column spans |
| `StickyHeader` | ScrollView header that sticks to top |
| `SplitPane` | Resizable split with drag divider |
| `ScrollView` | Scrollable container (infinite/virtual) |

## Display

| Widget | Description |
|--------|-------------|
| `Text` | Text with font size, weight, alignment |
| `Image` | Raster image (PNG/JPEG/GIF/WebP) |
| `SvgImage` | SVG vector image |
| `Badge` | Small count/status indicator |
| `Chip` | Compact tag or filter |
| `Avatar` | Circular user avatar with image or initials |
| `Icon` | Material/embedded icon glyph |
| `Progress` | Linear or circular progress bar |
| `Skeleton` | Loading placeholder shimmer |
| `BarChart` | Vertical bar chart |
| `LineChart` | Line/area chart |
| `List` | Single-column selectable list |
| `Table` | Multi-column grid with sort/resize/virtual scroll |
| `Tree` | Hierarchical tree with expand/collapse |
| `PropertyGrid` | Key-value property inspector |
| `Calendar` | Month calendar grid |
| `EmptyState` | Empty placeholder with title and action |

## Input

| Widget | Description |
|--------|-------------|
| `Button` | Clickable button (primary, secondary, text variants) |
| `IconButton` | Icon-only button |
| `TextInput` | Single-line text entry |
| `NumberInput` | Numeric entry with increment/decrement |
| `PasswordInput` | Masked text entry |
| `Checkbox` | Boolean toggle with label |
| `Switch` | Toggle switch |
| `Slider` | Range slider with step |
| `RadioButton` | Radio group option |
| `ComboBox` | Editable dropdown with search |
| `Select` | Dropdown selection list |
| `DatePicker` | Date selection with calendar popup |
| `ColorPicker` | Color selection with palette |
| `TextEditor` | Multi-line rich text editor |
| `Form` | Form container with validation |

## Overlay

| Widget | Description |
|--------|-------------|
| `Modal` | Full-screen modal overlay |
| `Dialog` | Alert or confirmation dialog |
| `Popover` | Anchored floating panel |
| `Tooltip` | Hover tooltip |
| `Toast` | Auto-dismiss notification |
| `ContextMenu` | Right-click context menu |

## Composite

| Widget | Description |
|--------|-------------|
| `TabBar` | Tab navigation bar |
| `TabPanel` | Tab content panel |
| `Accordion` | Expandable sections |
| `AudioPlayer` | Audio playback with controls |

## Custom Widget

Implement the `Widget` trait:

```rust
use burin::core::{Widget, MountContext, ElementId};

struct MyWidget;

impl Widget for MyWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let id = ctx.arena.allocate();
        if let Some(el) = ctx.arena.get_mut(id) {
            el.set_preferred_width(Some(100.0));
            el.set_preferred_height(30.0);
            el.set_background(ctx.theme.scheme.surface);
            el.set_paint_fn(|el, ctx, _fcx| {
                // Custom paint logic
            });
        }
        id
    }
}
```
