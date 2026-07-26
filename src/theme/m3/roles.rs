//! ComponentRole — declarative widget identity for automatic theme resolution.

use crate::style::{Color, CornerRadii, Padding};
use crate::theme::m3::states::{DisabledColors, LayerColor};
use crate::theme::{Appearance, ControlShape, ControlSize, Intent};

/// The role a widget plays — determines which M3 color roles and tokens it gets.
#[derive(Clone, Debug)]
pub enum ComponentRole {
    Interactive(InteractiveRole),
    Display(DisplayRole),
}

#[derive(Clone, Debug)]
pub enum InteractiveRole {
    Button {
        intent: Intent,
        appearance: Appearance,
        size: ControlSize,
        shape: ControlShape,
    },
    TextInput {
        variant: InputVariant,
        size: ControlSize,
        disabled: bool,
        readonly: bool,
        is_valid: bool,
    },
    Select {
        size: ControlSize,
    },
    Checkbox {
        size: ControlSize,
    },
    Switch {
        size: ControlSize,
    },
    Radio {
        size: ControlSize,
    },
    Slider {
        size: ControlSize,
    },
    ColorPicker {
        size: ControlSize,
    },
    Chip {
        selected: bool,
    },
    Tab {
        selected: bool,
    },
    MenuItem,
    ListItem {
        selected: bool,
    },
    TreeItem {
        selected: bool,
        expanded: bool,
    },
    Progress {
        intent: Intent,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputVariant {
    Filled,
    Outlined,
}

#[derive(Clone, Debug)]
pub enum DisplayRole {
    Text { foreground: ColorRole },
    Badge { intent: Intent },
    Divider,
    Skeleton,
    Avatar,
    Icon,
    Tooltip,
    Popover,
    Modal,
    Dialog,
    Toast { kind: ToastKind },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRole {
    OnSurface,
    OnSurfaceVariant,
    Primary,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

// ── Resolved style types ──

#[derive(Clone, Debug)]
pub enum ResolvedComponentStyle {
    Button(ButtonStyle),
    TextInput(InputStyle),
    Select(SelectStyle),
    Checkbox(CheckboxStyle),
    Switch(SwitchStyle),
    Radio(RadioStyle),
    Slider(SliderStyle),
    ColorPicker(ColorPickerStyle),
    Chip(ChipStyle),
    Tab(TabStyle),
    MenuItem(MenuItemStyle),
    ListItem(ListItemStyle),
    TreeItem(TreeItemStyle),
    Progress(ProgressStyle),
    Text(TextStyle),
    Badge(BadgeStyle),
    Divider(DividerStyle),
    Skeleton(SkeletonStyle),
    Avatar(AvatarStyle),
    Icon(IconStyle),
    Tooltip(TooltipStyle),
    Popover(PopoverStyle),
    Modal(ModalStyle),
    Dialog(DialogStyle),
    Toast(ToastStyle),
}

#[derive(Clone, Debug)]
pub struct ButtonStyle {
    pub background: Color,
    pub foreground: Color,
    pub border: Option<Color>,
    pub(crate) hover: LayerColor,
    pub(crate) pressed: LayerColor,
    pub(crate) focused: LayerColor,
    pub(crate) disabled: DisabledColors,
    pub font_size: f32,
    pub font_weight: u16,
    pub letter_spacing: f32,
    pub corner_radius: CornerRadii,
    pub min_width: f32,
    pub height: f32,
    pub padding: Padding,
}

#[derive(Clone, Debug)]
pub struct InputStyle {
    pub background: Color,
    pub foreground: Color,
    pub border_color: Color,
    pub placeholder_color: Color,
    pub(crate) hover: LayerColor,
    pub(crate) pressed: LayerColor,
    pub(crate) focused: LayerColor,
    pub(crate) disabled: DisabledColors,
    pub font_size: f32,
    pub font_weight: u16,
    pub letter_spacing: f32,
    pub corner_radius: CornerRadii,
    pub height: f32,
    pub padding: Padding,
}

#[derive(Clone, Debug)]
pub struct SelectStyle {
    pub trigger_bg: Color,
    pub trigger_fg: Color,
    pub trigger_border: Color,
    pub trigger_hover_bg: Color,
    pub dropdown_bg: Color,
    pub dropdown_fg: Color,
    pub dropdown_border: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub option_hover_bg: Color,
    pub option_disabled_fg: Color,
    pub corner_radius: CornerRadii,
    pub height: f32,
    pub font_size: f32,
    pub shadow: Option<crate::style::styled::Shadow>,
}

#[derive(Clone, Debug)]
pub struct CheckboxStyle {
    pub checked_bg: Color,
    pub unchecked_bg: Color,
    pub unchecked_border: Color,
    pub checked_icon: Color,
    pub hover_bg: Color,
    pub pressed_bg: Color,
    pub disabled_bg: Color,
    pub disabled_border: Color,
    pub corner_radius: CornerRadii,
    pub size: f32,
}

#[derive(Clone, Debug)]
pub struct SwitchStyle {
    pub checked_bg: Color,
    pub unchecked_bg: Color,
    pub checked_thumb: Color,
    pub unchecked_thumb: Color,
    pub disabled_bg: Color,
    pub disabled_thumb: Color,
    pub width: f32,
    pub height: f32,
    pub thumb_size: f32,
    pub hover_bg: Color,
    pub hover_fg: Color,
    pub pressed_bg: Color,
    pub pressed_fg: Color,
    pub focused_bg: Color,
    pub focused_fg: Color,
}

#[derive(Clone, Debug)]
pub struct RadioStyle {
    pub checked_bg: Color,
    pub unchecked_bg: Color,
    pub unchecked_border: Color,
    pub checked_dot: Color,
    pub hover_bg: Color,
    pub disabled_bg: Color,
    pub disabled_border: Color,
    pub size: f32,
}

#[derive(Clone, Debug)]
pub struct SliderStyle {
    pub track_color: Color,
    pub fill_color: Color,
    pub thumb_color: Color,
    pub hover_thumb: Color,
    pub pressed_thumb: Color,
    pub disabled_track: Color,
    pub disabled_thumb: Color,
    pub track_height: f32,
    pub thumb_radius: f32,
}

#[derive(Clone, Debug)]
pub struct ColorPickerStyle {
    pub panel_bg: Color,
    pub panel_border: Color,
    pub trigger_bg: Color,
    pub trigger_border: Color,
    pub plane_handle_color: Color,
    pub plane_handle_border: Color,
    pub slider_handle_color: Color,
    pub slider_track_color: Color,
    pub preview_border: Color,
    pub preset_border: Color,
    pub label_color: Color,
    pub font_size: f32,
    pub corner_radius: CornerRadii,
    pub handle_radius: f32,
    pub hue_track_height: f32,
    pub alpha_track_height: f32,
    pub plane_size: f32,
    pub preset_size: f32,
    pub shadow: Option<crate::style::styled::Shadow>,
}

#[derive(Clone, Debug)]
pub struct ChipStyle {
    pub background: Color,
    pub foreground: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub border_color: Color,
    pub corner_radius: CornerRadii,
    pub height: f32,
    pub font_size: f32,
    pub padding_h: f32,
    pub hover_bg: Color,
    pub hover_fg: Color,
    pub pressed_bg: Color,
    pub pressed_fg: Color,
    pub focused_bg: Color,
    pub focused_fg: Color,
    pub(crate) disabled: DisabledColors,
}

#[derive(Clone, Debug)]
pub struct TabStyle {
    pub background: Color,
    pub foreground: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub indicator_color: Color,
    pub indicator_height: f32,
    pub hover_bg: Color,
    pub hover_fg: Color,
    pub pressed_bg: Color,
    pub pressed_fg: Color,
    pub focused_bg: Color,
    pub focused_fg: Color,
    pub(crate) disabled: DisabledColors,
    pub font_size: f32,
    pub height: f32,
    pub tab_gap: f32,
    pub pill_radius: CornerRadii,
}

#[derive(Clone, Debug)]
pub struct MenuItemStyle {
    pub background: Color,
    pub foreground: Color,
    pub hover_bg: Color,
    pub hover_fg: Color,
    pub disabled_fg: Color,
    pub font_size: f32,
    pub height: f32,
    pub padding: Padding,
}

#[derive(Clone, Debug)]
pub struct ListItemStyle {
    pub background: Color,
    pub foreground: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub hover_bg: Color,
    pub font_size: f32,
    pub height: f32,
    pub padding: Padding,
}

#[derive(Clone, Debug)]
pub struct TreeItemStyle {
    pub background: Color,
    pub foreground: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub hover_bg: Color,
    pub indent: f32,
    pub font_size: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct ProgressStyle {
    pub track_color: Color,
    pub fill_color: Color,
    pub height: f32,
    pub corner_radius: CornerRadii,
    pub circular_size: f32,
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub foreground: Color,
    pub font_size: f32,
    pub font_weight: u16,
    pub letter_spacing: f32,
}

#[derive(Clone, Debug)]
pub struct BadgeStyle {
    pub background: Color,
    pub foreground: Color,
    pub corner_radius: CornerRadii,
    pub font_size: f32,
    pub font_weight: u16,
    pub height: f32,
    pub padding_h: f32,
}

#[derive(Clone, Debug)]
pub struct DividerStyle {
    pub color: Color,
    pub thickness: f32,
}

#[derive(Clone, Debug)]
pub struct SkeletonStyle {
    pub background: Color,
    pub shimmer: Color,
}

#[derive(Clone, Debug)]
pub struct AvatarStyle {
    pub font_size: f32,
    pub font_weight: u16,
    pub size: f32,
    pub corner_radius: CornerRadii,
}

#[derive(Clone, Debug)]
pub struct IconStyle {
    pub color: Color,
    pub size: f32,
}

#[derive(Clone, Debug)]
pub struct TooltipStyle {
    pub background: Color,
    pub foreground: Color,
    pub font_size: f32,
    pub corner_radius: CornerRadii,
    pub padding: Padding,
}

#[derive(Clone, Debug)]
pub struct PopoverStyle {
    pub background: Color,
    pub border_color: Color,
    pub corner_radius: CornerRadii,
    pub shadow: Option<crate::style::styled::Shadow>,
}

#[derive(Clone, Debug)]
pub struct ModalStyle {
    pub backdrop_color: Color,
}

#[derive(Clone, Debug)]
pub struct DialogStyle {
    pub background: Color,
    pub foreground: Color,
    pub border_color: Color,
    pub corner_radius: CornerRadii,
    pub shadow: Option<crate::style::styled::Shadow>,
    pub title_font_size: f32,
    pub body_font_size: f32,
    pub padding: Padding,
    pub gap: f32,
    pub max_width: f32,
}

#[derive(Clone, Debug)]
pub struct ToastStyle {
    pub background: Color,
    pub foreground: Color,
    pub corner_radius: CornerRadii,
    pub font_size: f32,
}
