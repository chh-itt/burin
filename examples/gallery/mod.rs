pub mod demo_panel;
pub mod p1_layout;
pub mod p2_charts;
pub mod p2_display;
pub mod p3_input;
pub mod p4_effects;
pub mod p5_composites;
pub mod p6_virtual;

use burin::core::{Compositor, Widget};
use burin::style::Padding as Pad;
use burin::style::Styled;
use burin::widgets::display::Text;
use burin::widgets::layout::*;

pub fn section_title(text: &str) -> Text {
    Text::new(text).font_size(18.0).font_weight(700)
}

pub fn section_sub(text: &str) -> Text {
    Text::new(text).font_size(11.0)
}

#[cfg(feature = "ext-svg")]
fn svg_image_section_if_enabled() -> impl Widget {
    p2_display::svg_image_section()
}

#[cfg(not(feature = "ext-svg"))]
fn svg_image_section_if_enabled() -> impl Widget {
    burin::widgets::layout::SizedBox::new()
}

#[cfg(feature = "ext-jiff")]
fn datepicker_section_if_enabled() -> impl Widget {
    p3_input::datepicker_section()
}

#[cfg(not(feature = "ext-jiff"))]
fn datepicker_section_if_enabled() -> impl Widget {
    p3_input::datepicker_section() // feature-off stub: renders a hint Text
}

#[cfg(feature = "file-dialog")]
fn filepicker_section_if_enabled() -> impl Widget {
    p3_input::filepicker_section()
}

#[cfg(not(feature = "file-dialog"))]
fn filepicker_section_if_enabled() -> impl Widget {
    p3_input::filepicker_section() // feature-off stub: renders a hint Text
}

#[cfg(feature = "ext-audio")]
fn audio_player_section_if_enabled() -> impl Widget {
    p5_composites::audio_player_section()
}

#[cfg(not(feature = "ext-audio"))]
fn audio_player_section_if_enabled() -> impl Widget {
    burin::widgets::layout::SizedBox::new()
}

pub fn app() -> impl Widget {
    Compositor::new(|_scope| {
        ScrollView::new().child(
            VStack::new()
                .padding(Pad::all(16.0))
                .gap(12.0)
                .push(section_title("═ P4 Extra: GPU Effects ═"))
                .push(p4_effects::gradient_section())
                .push(p4_effects::shadow_section())
                .push(p4_effects::overview_section())
                .push(p4_effects::blend_mode_section())
                .push(p4_effects::gaussian_blur_section())
                .push(section_title("═ P1: Layout & Basics ═"))
                .push(p1_layout::sized_box_section())
                .push(p1_layout::spacer_section())
                .push(p1_layout::padding_section())
                .push(p1_layout::center_section())
                .push(p1_layout::opacity_section())
                .push(p1_layout::expanded_section())
                .push(p1_layout::flexible_section())
                .push(p1_layout::grid_section())
                .push(p1_layout::conditional_section())
                .push(p1_layout::split_pane_section())
                .push(section_title("═ P2: Display Widgets ═"))
                .push(p2_display::text_section())
                .push(p2_display::icon_section())
                .push(p2_display::avatar_section())
                .push(p2_display::badge_section())
                .push(p2_display::chip_section())
                .push(p2_display::skeleton_section())
                .push(p2_display::progress_section())
                .push(p2_display::image_section())
                .push(svg_image_section_if_enabled())
                .push(p2_display::empty_state_section())
                .push(p2_display::list_section())
                .push(p2_display::table_section())
                .push(p2_display::tree_section())
                .push(section_title("═ P2 Charts: Visualization ═"))
                .push(p2_charts::property_grid_section())
                .push(p2_charts::bar_chart_section())
                .push(p2_charts::line_chart_section())
                .push(section_title("═ P2b: Virtual Scrolling (10k rows) ═"))
                .push(p6_virtual::virtual_table_section())
                .push(p6_virtual::virtual_list_section())
                .push(section_title("═ P3: Input Widgets ═"))
                .push(p3_input::button_section())
                .push(p3_input::icon_button_section())
                .push(p3_input::checkbox_section())
                .push(p3_input::switch_section())
                .push(p3_input::radio_section())
                .push(p3_input::slider_section())
                .push(p3_input::color_picker_section())
                .push(p3_input::number_input_section())
                .push(p3_input::text_input_section())
                .push(p3_input::select_section())
                .push(p3_input::combobox_section())
                .push(datepicker_section_if_enabled())
                .push(filepicker_section_if_enabled())
                .push(p3_input::tooltip_section())
                .push(p3_input::popover_section())
                .push(p3_input::modal_section())
                .push(p3_input::toast_section())
                .push(section_title("═ P4: Composite Widgets ═"))
                .push(p3_input::accordion_section())
                .push(p5_composites::tab_bar_section())
                .push(audio_player_section_if_enabled())
                .push(section_title("═ P4 Extra: GPU Effects ═"))
                .push(p4_effects::overview_section())
                .push(p4_effects::gradient_section())
                .push(p4_effects::shadow_section())
                .push(p4_effects::blend_mode_section())
                .push(p4_effects::gaussian_blur_section()),
        )
    })
}
