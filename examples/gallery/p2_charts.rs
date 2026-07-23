use crate::gallery::{section_sub, section_title};
use auralis_signal::Signal;
use burin::core::{Compositor, Widget};
use burin::style::{Color, Styled};
use burin::widgets::display::{BarChart, BarGroup, LineChart, PropertyGrid, PropertyRow, Text};
use burin::widgets::layout::*;

pub fn property_grid_section() -> impl Widget {
    Compositor::new(|_scope| {
        let id_val = Signal::new("123".to_string());
        let type_val = Signal::new("VStack".to_string());
        let bounds_val = Signal::new("(0, 0, 800, 600)".to_string());
        let depth_val = Signal::new("1".to_string());
        let dirty_val = Signal::new("REPAINT | LAYOUT".to_string());
        let state_val = Signal::new("HOVERED".to_string());
        let children_val = Signal::new("3".to_string());
        let pref_w = Signal::new("200".to_string());
        let pref_h = Signal::new("40".to_string());
        let flex_g = Signal::new("0".to_string());
        let margin_val = Signal::new("(8, 4, 8, 4)".to_string());
        let bg_val = Signal::new("#1E1E2E".to_string());
        let border_val = Signal::new("0".to_string());
        let radius_val = Signal::new("8".to_string());
        let opacity_val = Signal::new("1.0".to_string());

        VStack::new()
            .gap(8.0)
            .push(section_title("Property Grid  NEW"))
            .push(section_sub(
                "Two-column key-value inspector with grouped sections.",
            ))
            .push(
                PropertyGrid::new()
                    .label_width(100.0)
                    .section(
                        "Identity",
                        vec![
                            PropertyRow {
                                label: "id".into(),
                                value: id_val,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "type".into(),
                                value: type_val,
                                value_color: Some(Color::rgba8(100, 200, 255, 255)),
                            },
                            PropertyRow {
                                label: "bounds".into(),
                                value: bounds_val,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "depth".into(),
                                value: depth_val,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "dirty".into(),
                                value: dirty_val,
                                value_color: Some(Color::rgba8(255, 180, 100, 255)),
                            },
                            PropertyRow {
                                label: "state".into(),
                                value: state_val,
                                value_color: Some(Color::rgba8(120, 255, 120, 255)),
                            },
                            PropertyRow {
                                label: "children".into(),
                                value: children_val,
                                value_color: None,
                            },
                        ],
                    )
                    .section(
                        "Layout",
                        vec![
                            PropertyRow {
                                label: "pref_w".into(),
                                value: pref_w,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "pref_h".into(),
                                value: pref_h,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "flex_g".into(),
                                value: flex_g,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "margin".into(),
                                value: margin_val,
                                value_color: None,
                            },
                        ],
                    )
                    .section(
                        "Style",
                        vec![
                            PropertyRow {
                                label: "bg".into(),
                                value: bg_val,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "border".into(),
                                value: border_val,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "radius".into(),
                                value: radius_val,
                                value_color: None,
                            },
                            PropertyRow {
                                label: "opacity".into(),
                                value: opacity_val,
                                value_color: None,
                            },
                        ],
                    ),
            )
    })
}

pub fn bar_chart_section() -> impl Widget {
    Compositor::new(|_scope| {
        let bar_data = Signal::new(vec![
            BarGroup {
                label: "F#1".into(),
                values: vec![1.0, 3.0, 4.0, 5.0, 0.5, 1.5, 1.0],
            },
            BarGroup {
                label: "F#2".into(),
                values: vec![0.5, 4.0, 3.0, 6.0, 1.0, 0.5, 2.0],
            },
            BarGroup {
                label: "F#3".into(),
                values: vec![2.0, 2.0, 5.0, 3.0, 1.5, 2.0, 0.5],
            },
            BarGroup {
                label: "F#4".into(),
                values: vec![1.5, 3.5, 2.0, 4.0, 2.0, 1.0, 1.5],
            },
            BarGroup {
                label: "F#5".into(),
                values: vec![0.8, 2.5, 4.5, 5.5, 0.8, 1.8, 1.2],
            },
            BarGroup {
                label: "F#6".into(),
                values: vec![1.2, 3.0, 3.5, 4.5, 1.2, 0.6, 2.0],
            },
            BarGroup {
                label: "F#7".into(),
                values: vec![2.0, 1.5, 6.0, 2.0, 1.8, 2.5, 1.0],
            },
            BarGroup {
                label: "F#8".into(),
                values: vec![1.0, 4.5, 3.0, 5.0, 0.5, 1.5, 1.8],
            },
            BarGroup {
                label: "F#9".into(),
                values: vec![1.8, 2.0, 4.0, 4.0, 1.0, 2.0, 0.8],
            },
            BarGroup {
                label: "F#10".into(),
                values: vec![0.5, 3.5, 5.0, 3.5, 1.5, 1.0, 2.2],
            },
        ]);

        VStack::new()
            .gap(8.0)
            .push(section_title("Bar Chart  NEW"))
            .push(section_sub(
                "Stacked bar chart. Simulated per-frame phase breakdown (ms).",
            ))
            .push(
                BarChart::new(bar_data)
                    .legend(vec![
                        "Prepass".into(),
                        "Portal".into(),
                        "Dirty".into(),
                        "Layout".into(),
                        "Anim".into(),
                        "Recheck".into(),
                        "Paint".into(),
                    ])
                    .colors(vec![
                        Color::rgba8(100, 143, 255, 255),
                        Color::rgba8(255, 200, 100, 255),
                        Color::rgba8(255, 120, 120, 255),
                        Color::rgba8(120, 200, 120, 255),
                        Color::rgba8(180, 130, 255, 255),
                        Color::rgba8(100, 200, 200, 255),
                        Color::rgba8(200, 160, 255, 255),
                    ])
                    .max_value(16.6)
                    .chart_width(700.0)
                    .chart_height(220.0),
            )
    })
}

pub fn line_chart_section() -> impl Widget {
    Compositor::new(|_scope| {
        let fps_data = Signal::new(vec![
            (0.0, 60.0),
            (1.0, 59.5),
            (2.0, 58.0),
            (3.0, 57.0),
            (4.0, 59.0),
            (5.0, 60.0),
            (6.0, 55.0),
            (7.0, 52.0),
            (8.0, 54.0),
            (9.0, 58.0),
            (10.0, 60.0),
            (11.0, 59.0),
            (12.0, 58.5),
            (13.0, 57.0),
            (14.0, 59.5),
            (15.0, 60.0),
            (16.0, 48.0),
            (17.0, 50.0),
            (18.0, 56.0),
            (19.0, 60.0),
        ]);

        let cache_data = Signal::new(vec![
            (0.0, 95.0),
            (1.0, 94.0),
            (2.0, 93.5),
            (3.0, 96.0),
            (4.0, 94.5),
            (5.0, 92.0),
            (6.0, 90.0),
            (7.0, 91.0),
            (8.0, 93.0),
            (9.0, 95.0),
            (10.0, 94.0),
            (11.0, 93.0),
            (12.0, 92.5),
            (13.0, 94.0),
            (14.0, 95.5),
            (15.0, 96.0),
            (16.0, 88.0),
            (17.0, 89.0),
            (18.0, 92.0),
            (19.0, 95.0),
        ]);

        VStack::new()
            .gap(8.0)
            .push(section_title("Line Chart  NEW"))
            .push(section_sub(
                "Line chart with fill-below. Left: FPS over frames. Right: Cache hit rate (%).",
            ))
            .push(
                HStack::new()
                    .gap(24.0)
                    .push(
                        VStack::new()
                            .gap(4.0)
                            .push(Text::new("FPS (60fps target)").font_size(11.0))
                            .push(
                                LineChart::new(fps_data)
                                    .fill_below(Color::rgba8(100, 143, 255, 80))
                                    .max_y(70.0)
                                    .max_x(19.0)
                                    .chart_width(350.0)
                                    .chart_height(160.0),
                            ),
                    )
                    .push(
                        VStack::new()
                            .gap(4.0)
                            .push(Text::new("Cache Hit Rate (%)").font_size(11.0))
                            .push(
                                LineChart::new(cache_data)
                                    .line_color(Color::rgba8(120, 200, 120, 255))
                                    .fill_below(Color::rgba8(120, 200, 120, 60))
                                    .max_y(100.0)
                                    .max_x(19.0)
                                    .chart_width(350.0)
                                    .chart_height(160.0),
                            ),
                    ),
            )
    })
}
