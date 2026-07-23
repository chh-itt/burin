use crate::gallery::demo_panel::DemoPanel;
use crate::gallery::{section_sub, section_title};
use auralis_signal::Signal;
use burin::core::{Compositor, Widget};
use burin::resource::icons::Icon as IconKind;
use burin::style::{Color, Padding as Pad, Styled};
use burin::widgets::display::{
    Avatar, AvatarImage, Badge, Chip, ChipVariant, ColumnWidth, ContentFit, EmptyState, Icon,
    Image, List, Progress, ProgressKind, Skeleton, SvgImage, Table, TableColumn, Text, Tree,
    TreeNode,
};
use burin::widgets::input::Button;
use burin::widgets::layout::*;
use burin::widgets::overlay::ContextMenuItem;
use std::rc::Rc;

pub fn text_section() -> impl Widget {
    Compositor::new(|_scope| {
        let content_sig = Signal::new("Dynamic text".to_string());

        VStack::new()
            .gap(8.0)
            .push(section_title("Text  G1"))
            .push(section_sub(
                "Core label widget. Verify bind, Styled path, reapply_theme.",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(Text::new("Default (body style)"))
                            .push(
                                Text::new("Blue text").text_color(Color::rgba8(59, 130, 246, 255)),
                            )
                            .push(
                                Text::new("Large bold (24px)")
                                    .font_size(24.0)
                                    .font_weight(700),
                            )
                            .push(Text::new("Small (10px)").font_size(10.0))
                            .push(
                                Text::new("With background + padding")
                                    .background(Color::rgba8(255, 220, 100, 255))
                                    .padding(Pad::all(4.0)),
                            )
                            .push(Text::new("Shadow text").shadow(
                                Color::rgba8(0, 0, 0, 80),
                                1.0,
                                1.0,
                                3.0,
                            ))
                            .push(
                                Text::new("Center aligned")
                                    .text_align(burin::style::TextAlign::Center)
                                    .width(200.0)
                                    .background(Color::rgba8(240, 240, 240, 255)),
                            )
                            .push(Text::new(content_sig.read()).bind(content_sig.clone())),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Content", content_sig.clone())
                            .info("Style", "body size/weight"),
                    ),
            )
    })
}

pub fn icon_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Icon  G1"))
            .push(section_sub(
                "Unicode glyph icons with configurable size and color.",
            ))
            .push(
                VStack::new()
                    .gap(8.0)
                    .push(
                        HStack::new()
                            .gap(8.0)
                            .push(Icon::new(IconKind::Search))
                            .push(Icon::new(IconKind::Home))
                            .push(Icon::new(IconKind::User))
                            .push(Icon::new(IconKind::Settings))
                            .push(Icon::new(IconKind::Folder))
                            .push(Icon::new(IconKind::File))
                            .push(Icon::new(IconKind::Mail))
                            .push(Icon::new(IconKind::Info))
                            .push(Icon::new(IconKind::AlertCircle))
                            .push(Icon::new(IconKind::Check))
                            .push(Icon::new(IconKind::X))
                            .push(Icon::new(IconKind::Play))
                            .push(Icon::new(IconKind::Pause)),
                    )
                    .push(
                        HStack::new()
                            .gap(8.0)
                            .push(Icon::new(IconKind::ArrowLeft))
                            .push(Icon::new(IconKind::ArrowRight))
                            .push(Icon::new(IconKind::ArrowUp))
                            .push(Icon::new(IconKind::ArrowDown))
                            .push(Icon::new(IconKind::Refresh))
                            .push(Icon::new(IconKind::Plus))
                            .push(Icon::new(IconKind::Minus))
                            .push(Icon::new(IconKind::Menu))
                            .push(Icon::new(IconKind::Filter))
                            .push(Icon::new(IconKind::Edit))
                            .push(Icon::new(IconKind::Delete))
                            .push(Icon::new(IconKind::Save))
                            .push(Icon::new(IconKind::Volume)),
                    )
                    .push(
                        HStack::new()
                            .gap(8.0)
                            .push(Text::new("Size: ").font_size(12.0))
                            .push(Icon::new(IconKind::Search).size(12.0))
                            .push(Icon::new(IconKind::Search).size(16.0))
                            .push(Icon::new(IconKind::Search).size(20.0))
                            .push(Icon::new(IconKind::Search).size(28.0))
                            .push(Icon::new(IconKind::Search).size(36.0)),
                    )
                    .push(
                        HStack::new()
                            .gap(8.0)
                            .push(Text::new("Color: ").font_size(12.0))
                            .push(
                                Icon::new(IconKind::AlertCircle)
                                    .size(20.0)
                                    .color(Color::rgba8(239, 68, 68, 255)),
                            ),
                    ),
            )
    })
}

pub fn badge_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Badge  G2"))
            .push(section_sub(
                "Status indicator with M3 colors + ARIA status role.",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(
                                HStack::new()
                                    .gap(6.0)
                                    .push(Text::new("Labels: ").font_size(12.0))
                                    .push(Badge::new("New"))
                                    .push(Badge::new("3"))
                                    .push(Badge::new("99+"))
                                    .push(
                                        Badge::new("Updated").color(Color::rgba8(34, 197, 94, 255)),
                                    )
                                    .push(
                                        Badge::new("Error").color(Color::rgba8(239, 68, 68, 255)),
                                    ),
                            )
                            .push(
                                HStack::new()
                                    .gap(6.0)
                                    .push(Text::new("Sizes: ").font_size(12.0))
                                    .push(Badge::new("XS").font_size(9.0))
                                    .push(Badge::new("SM").font_size(11.0))
                                    .push(Badge::new("MD").font_size(13.0))
                                    .push(Badge::new("LG").font_size(16.0)),
                            ),
                    )
                    .push(DemoPanel::new().info("Role", "status (AccessKit)")),
            )
    })
}

pub fn chip_section() -> impl Widget {
    Compositor::new(|_scope| {
        let click_str = Signal::new("0".to_string());

        VStack::new()
            .gap(8.0)
            .push(section_title("Chip  G2"))
            .push(section_sub(
                "Interactive chip — click to trigger. Selected variant shows blue border.",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(
                                HStack::new()
                                    .gap(6.0)
                                    .push(Text::new("Variants: ").font_size(12.0))
                                    .push(Chip::new("Assist"))
                                    .push(Chip::new("Filter").variant(ChipVariant::Filter))
                                    .push(Chip::new("Input").variant(ChipVariant::Input))
                                    .push(Chip::new("Suggestion").variant(ChipVariant::Suggestion)),
                            )
                            .push(
                                HStack::new()
                                    .gap(6.0)
                                    .push(Text::new("Icon: ").font_size(12.0))
                                    .push(Chip::new("Filter").icon(IconKind::Filter))
                                    .push(Chip::new("Search").icon(IconKind::Search))
                                    .push(Chip::new("Settings").icon(IconKind::Settings)),
                            )
                            .push(
                                HStack::new()
                                    .gap(6.0)
                                    .push(Text::new("Colors: ").font_size(12.0))
                                    .push(
                                        Chip::new("Blue")
                                            .background(Color::rgba8(59, 130, 246, 255))
                                            .text_color(Color::WHITE),
                                    )
                                    .push(
                                        Chip::new("Green")
                                            .background(Color::rgba8(34, 197, 94, 255))
                                            .text_color(Color::WHITE),
                                    )
                                    .push(
                                        Chip::new("Red")
                                            .background(Color::rgba8(239, 68, 68, 255))
                                            .text_color(Color::WHITE),
                                    ),
                            )
                            .push(
                                HStack::new()
                                    .gap(6.0)
                                    .push(Text::new("Click: ").font_size(12.0))
                                    .push(Chip::new("Tap me!").on_click({
                                        let c = click_str.clone();
                                        move || {
                                            let n: i32 = c.read().parse().unwrap_or(0);
                                            c.set((n + 1).to_string());
                                        }
                                    })),
                            ),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Clicks", click_str.clone())
                            .info("Role", "button (AccessKit)"),
                    ),
            )
    })
}

pub fn skeleton_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new()
            .gap(8.0)
            .push(section_title("Skeleton  G1"))
            .push(section_sub(
                "Loading placeholder with optional shimmer animation. Decorative (no ARIA).",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(
                                VStack::new()
                                    .gap(4.0)
                                    .push(Text::new("Rectangles:").font_size(12.0))
                                    .push(Skeleton::new().rect(200.0, 16.0))
                                    .push(Skeleton::new().rect(160.0, 16.0))
                                    .push(Skeleton::new().rect(180.0, 16.0)),
                            )
                            .push(
                                HStack::new()
                                    .gap(8.0)
                                    .push(Text::new("Circle: ").font_size(12.0))
                                    .push(Skeleton::new().circle(40.0))
                                    .push(Skeleton::new().circle(32.0))
                                    .push(Skeleton::new().circle(48.0)),
                            )
                            .push(
                                HStack::new()
                                    .gap(8.0)
                                    .push(Text::new("Static: ").font_size(12.0))
                                    .push(Skeleton::new().rect(120.0, 16.0).animated(false))
                                    .push(Skeleton::new().circle(40.0).animated(false)),
                            ),
                    )
                    .push(DemoPanel::new().info("Role", "none (decorative)")),
            )
    })
}

pub fn progress_section() -> impl Widget {
    Compositor::new(|_scope| {
        let val = Signal::new(60.0);

        VStack::new().gap(8.0)
            .push(section_title("Progress  G2"))
            .push(section_sub("Linear + Circular progress with determinate and indeterminate modes. ARIA progressbar role."))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        VStack::new().gap(8.0)
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Linear:").font_size(12.0))
                                    .push(Progress::new(val.clone()))
                                    .push(Progress::new(val.clone()).indeterminate())
                            )
                            .push(
                                HStack::new().gap(8.0)
                                    .push(Text::new("Circular: ").font_size(12.0))
                                    .push(Progress::new(val.clone()).kind(ProgressKind::Circular))
                                    .push(Progress::new(val.clone()).kind(ProgressKind::Circular).indeterminate())
                            )
                    )
                    .push(
                        DemoPanel::new()
                            .info("Value", format!("{:.0}", val.read()))
                            .info("Role", "progressbar (AccessKit)")
                    )
            )
    })
}

pub fn image_section() -> impl Widget {
    Compositor::new(|_scope| {
        let photo = match Image::from_bytes(include_bytes!("../a.png")) {
            Ok(img) => img,
            Err(_) => {
                return VStack::new()
                    .gap(8.0)
                    .push(section_title("Image  G1"))
                    .push(section_sub("ext-image feature not enabled"));
            }
        };

        VStack::new().gap(8.0)
            .push(section_title("Image  G1"))
            .push(section_sub("Raster image with ContentFit variants. Feature-gated (ext-image). Verifies cache + GPU bridge + mipmap downscaling."))
            .push(
                VStack::new().gap(8.0)
                    .push(
                        VStack::new().gap(4.0)
                            .push(Text::new("Original (1919×1079, auto height):").font_size(12.0))
                            .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Contain).height(300.0))
                    )
                    .push(
                        HStack::new().gap(8.0)
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Fill:").font_size(12.0))
                                    .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Fill).height(80.0).width(120.0))
                            )
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Cover:").font_size(12.0))
                                    .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Cover).height(80.0).width(120.0))
                            )
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Contain:").font_size(12.0))
                                    .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Contain).height(80.0).width(120.0))
                            )
                    )
                    .push(
                        VStack::new().gap(4.0)
                            .push(Text::new("Extreme downscale (1919×1079 → 40×40, test mipmap):").font_size(12.0))
                            .push(HStack::new().gap(4.0)
                                .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Cover).height(40.0).width(40.0))
                                .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Cover).height(32.0).width(32.0))
                                .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Cover).height(24.0).width(24.0))
                                .push(Image::from_rgba(photo.pixels.clone(), photo.width, photo.height).fit(ContentFit::Cover).height(16.0).width(16.0))
                            )
                    )
            )
    })
}

#[cfg(feature = "ext-svg")]
pub fn svg_image_section() -> impl Widget {
    Compositor::new(|_scope| {
        let svg = match SvgImage::from_bytes(include_bytes!("../柴犬.svg")) {
            Ok(img) => img,
            Err(_) => {
                return VStack::new()
                    .gap(8.0)
                    .push(section_title("SvgImage  G1"))
                    .push(section_sub("ext-svg feature not enabled"));
            }
        };

        VStack::new().gap(8.0)
            .push(section_title("SvgImage  G1"))
            .push(section_sub("Rasterized SVG with ContentFit variants. Verifies cache + GPU bridge parity with Image."))
            .push(
                VStack::new().gap(8.0)
                    .push(
                        VStack::new().gap(4.0)
                            .push(Text::new("Intrinsic size (auto height):").font_size(12.0))
                            .push(svg.fit(ContentFit::Contain).height(200.0))
                    )
                    .push(
                        HStack::new().gap(8.0)
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Fill:").font_size(12.0))
                                    .push(SvgImage::from_bytes(include_bytes!("../柴犬.svg")).unwrap().fit(ContentFit::Fill).height(80.0).width(120.0))
                            )
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Cover:").font_size(12.0))
                                    .push(SvgImage::from_bytes(include_bytes!("../柴犬.svg")).unwrap().fit(ContentFit::Cover).height(80.0).width(120.0))
                            )
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Contain:").font_size(12.0))
                                    .push(SvgImage::from_bytes(include_bytes!("../柴犬.svg")).unwrap().fit(ContentFit::Contain).height(80.0).width(120.0))
                            )
                    )
                    .push(
                        VStack::new().gap(4.0)
                            .push(Text::new("Explicit 100×100 + corner radius:").font_size(12.0))
                            .push(SvgImage::from_bytes(include_bytes!("../柴犬.svg")).unwrap().fit(ContentFit::Cover).height(100.0).width(100.0).corner_radius(burin::style::CornerRadii::all(16.0)))
                    )
            )
    })
}

fn avatar_photo() -> Option<Avatar> {
    #[cfg(feature = "ext-image")]
    {
        AvatarImage::from_bytes(include_bytes!("../a.png"))
            .ok()
            .map(|img| Avatar::new("Photo").image(img))
    }
    #[cfg(not(feature = "ext-image"))]
    {
        None
    }
}

pub fn empty_state_section() -> impl Widget {
    Compositor::new(|_scope| {
        VStack::new().gap(8.0)
            .push(section_title("EmptyState  G1"))
            .push(section_sub("Composite placeholder for empty lists, search results, etc. Centered layout with optional icon, title, description, and action."))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        VStack::new().gap(8.0)
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Title only:").font_size(12.0))
                                    .push(SizedBox::new().width(300.0).height(100.0).child(EmptyState::new().title("No items found")))
                            )
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Title + description:").font_size(12.0))
                                    .push(SizedBox::new().width(300.0).height(120.0).child(EmptyState::new().title("No results").description("Try adjusting your search or filters.")))
                            )
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Icon + title + description:").font_size(12.0))
                                    .push(SizedBox::new().width(300.0).height(140.0).child(
                                        EmptyState::new()
                                            .icon(Icon::new(IconKind::Search))
                                            .title("No matches")
                                            .description("We couldn't find anything matching your query."),
                                    ))
                            )
                            .push(
                                VStack::new().gap(4.0)
                                    .push(Text::new("Full (icon + title + desc + action):").font_size(12.0))
                                    .push(SizedBox::new().width(300.0).height(160.0).child(
                                        EmptyState::new()
                                            .icon(Icon::new(IconKind::AlertCircle))
                                            .title("Connection lost")
                                            .description("Please check your network and try again.")
                                            .action(Button::new("Retry")),
                                    ))
                            )
                    )
                    .push(
                        DemoPanel::new()
                            .info("Role", "none (composite)")
                            .info("Composition", "Center > VStack > Text/Icon/Button")
                    )
            )
    })
}

pub fn avatar_section() -> impl Widget {
    Compositor::new(|_scope| {
        let photo_avatar = avatar_photo();

        VStack::new().gap(8.0)
            .push(section_title("Avatar  G2"))
            .push(section_sub("Circular user avatar with photo or initials fallback. M3 CircleAvatar + ARIA img role."))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        VStack::new().gap(8.0)
                            .push(
                                HStack::new().gap(8.0)
                                    .push(Text::new("Initials: ").font_size(12.0))
                                    .push(Avatar::new("Alice"))
                                    .push(Avatar::new("Bob"))
                                    .push(Avatar::new("Charlie"))
                                    .push(Avatar::new("Diana"))
                                    .push(Avatar::new("Eve"))
                                    .push(Avatar::new("Frank"))
                            )
                            .push(
                                HStack::new().gap(8.0)
                                    .push(Text::new("Sizes: ").font_size(12.0))
                                    .push(Avatar::new("XS").size(24.0))
                                    .push(Avatar::new("S").size(32.0))
                                    .push(Avatar::new("M").size(40.0))
                                    .push(Avatar::new("L").size(56.0))
                                    .push(Avatar::new("XL").size(72.0))
                            )
                            .push(
                                HStack::new().gap(8.0)
                                    .push(Text::new("Color: ").font_size(12.0))
                                    .push(Avatar::new("Gray").color(Color::rgba8(107, 114, 128, 255)))
                            )
                            .push(
                                HStack::new().gap(8.0)
                                    .push(Text::new("Photo: ").font_size(12.0))
                                    .push(photo_avatar.unwrap_or_else(|| Avatar::new("NA").color(Color::rgba8(200, 200, 200, 255))))
                            )
                    )
                    .push(
                        DemoPanel::new()
                            .info("Type", "Initials / Photo")
                            .info("Role", "img (AccessKit)")
                    )
            )
    })
}

pub fn list_section() -> impl Widget {
    Compositor::new(|_scope| {
        let items = Signal::new(
            (0..20)
                .map(|i| format!("Item {}", i + 1))
                .collect::<Vec<_>>(),
        );
        let selected: Signal<Option<usize>> = Signal::new(None);
        let sel_display = Signal::new("None".to_string());
        let count_display = Signal::new(items.read().len().to_string());
        let disabled_set: Signal<std::collections::HashSet<usize>> =
            Signal::new([2, 5, 8, 13].iter().copied().collect());
        let reorder_count = Signal::new(0usize);
        let reorder_display = Signal::new("0".to_string());

        {
            let sd = sel_display.clone();
            let sel_inner = selected.clone();
            auralis_signal::subscribe(
                &selected,
                Rc::new(move || {
                    sd.set(
                        sel_inner
                            .read()
                            .map_or("None".into(), |i| format!("Item {}", i + 1)),
                    );
                }),
            );
        }
        {
            let cd = count_display.clone();
            let items_inner = items.clone();
            auralis_signal::subscribe(
                &items,
                Rc::new(move || {
                    cd.set(items_inner.read().len().to_string());
                }),
            );
        }

        VStack::new()
            .gap(8.0)
            .push(section_title("List  G3"))
            .push(section_sub(
                "Virtual-scroll list with keyboard nav + drag-to-reorder.\n\
                Arrow keys: move focus, Space/Enter: select, Click: select directly.\n\
                Items 3, 6, 9, 14 are disabled. Drag any non-disabled item to reorder.",
            ))
            .push(
                HStack::new()
                    .gap(12.0)
                    .push(
                        VStack::new()
                            .gap(8.0)
                            .push(Text::new("Reorderable list (20 items):").font_size(12.0))
                            .push(
                                SizedBox::new().width(280.0).height(320.0).child(
                                    List::new(items.clone())
                                        .render(|item: &String, _i| item.clone())
                                        .item_height(28.0)
                                        .selected(selected.clone())
                                        .disabled_items(disabled_set.clone())
                                        .reorderable(true)
                                        .on_reorder({
                                            let it = items.clone();
                                            let rc = reorder_count.clone();
                                            let rd = reorder_display.clone();
                                            let ds = disabled_set.clone();
                                            move |src, dst| {
                                                let mut v: Vec<String> = it.read().to_vec();
                                                let item = v.remove(src);
                                                v.insert(dst, item);
                                                it.set(v);

                                                let mut dis = ds.read().clone();
                                                let mut next = std::collections::HashSet::new();
                                                for d in dis.drain() {
                                                    let new_d = if d == src {
                                                        dst
                                                    } else if src < dst && d > src && d <= dst {
                                                        d - 1
                                                    } else if src > dst && d >= dst && d < src {
                                                        d + 1
                                                    } else {
                                                        d
                                                    };
                                                    next.insert(new_d);
                                                }
                                                ds.set(next);

                                                rc.set(rc.read() + 1);
                                                rd.set(rc.read().to_string());
                                            }
                                        }),
                                ),
                            ),
                    )
                    .push(
                        DemoPanel::new()
                            .field("Selected", sel_display.clone())
                            .field("Items", count_display.clone())
                            .field("Reorders", reorder_display.clone())
                            .info("Keyboard", "Up/Down/Home/End/Space")
                            .info("Drag", "Hold + drag to reorder")
                            .info("Role", "ListBox (AccessKit)"),
                    ),
            )
    })
}

pub fn table_section() -> impl Widget {
    Compositor::new(|_scope| {
        let rows = Signal::new(
            (0..20)
                .map(|i| format!("Row {}", i + 1))
                .collect::<Vec<_>>(),
        );
        let selected = Signal::new(None);
        let multi_selected: Signal<std::collections::HashSet<usize>> =
            Signal::new(std::collections::HashSet::new());
        let sel_display = Signal::new("None".to_string());
        let disabled_set: Signal<std::collections::HashSet<usize>> =
            Signal::new([2, 5, 8, 13].iter().copied().collect());
        let col_reorder_count = Signal::new(0usize);
        let rows_display = Signal::new(rows.read().len().to_string());
        let col_reorder_display = Signal::new("0".to_string());
        let footer_txt = Signal::new(vec!["".into(), "".into(), "".into(), "".into()]);
        {
            let r = rows.clone();
            let ds = disabled_set.clone();
            let ft = footer_txt.clone();
            let calc = move || {
                let data = r.read();
                let dis = ds.read();
                let active: Vec<&String> = data
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !dis.contains(i))
                    .map(|(_, s)| s)
                    .collect();
                let total_val: f64 = active.len() as f64 * (active.len() as f64 - 1.0) / 2.0 * 10.0;
                let mut cats = std::collections::HashSet::new();
                for (i, _) in data.iter().enumerate() {
                    if !dis.contains(&i) {
                        match i % 3 {
                            0 => cats.insert("Alpha"),
                            1 => cats.insert("Beta"),
                            _ => cats.insert("Gamma"),
                        };
                    }
                }
                ft.set(vec![
                    format!("{}r/{}a", data.len(), active.len()),
                    format!("Σ={total_val:.0}"),
                    format!("{} cats", cats.len()),
                    "—".into(),
                ]);
            };
            auralis_signal::subscribe(
                &rows,
                Rc::new({
                    let c = calc.clone();
                    move || c()
                }),
            );
            auralis_signal::subscribe(
                &disabled_set,
                Rc::new({
                    let c = calc.clone();
                    move || c()
                }),
            );
            calc(); // initial computation
        }

        {
            let sd = sel_display.clone();
            let sel = selected.clone();
            let ms = multi_selected.clone();
            auralis_signal::subscribe(
                &selected,
                Rc::new(move || {
                    let m = ms.read();
                    if !m.is_empty() {
                        sd.set(format!("{} rows (multi)", m.len()));
                    } else {
                        sd.set(
                            sel.read()
                                .map_or("None".into(), |i| format!("Row {}", i + 1)),
                        );
                    }
                }),
            );
        }
        {
            let sd2 = sel_display.clone();
            let sel2 = selected.clone();
            let ms2 = multi_selected.clone();
            auralis_signal::subscribe(
                &multi_selected,
                Rc::new(move || {
                    let m = ms2.read();
                    if !m.is_empty() {
                        sd2.set(format!("{} rows (multi)", m.len()));
                    } else {
                        sd2.set(
                            sel2.read()
                                .map_or("None".into(), |i| format!("Row {}", i + 1)),
                        );
                    }
                }),
            );
        }

        VStack::new().gap(8.0)
            .push(section_title("Table  G3"))
            .push(section_sub("Data table with sort, resize, column reorder, row hover, keyboard nav, and disabled rows. ARIA grid pattern.\nRows 3, 6, 9, 14 are disabled. Drag header labels to reorder columns."))
            .push(
                Button::new("Add row").on_click({
                    let r = rows.clone();
                    let rd = rows_display.clone();
                    move || {
                        let mut v = r.read().clone();
                        let n = v.len();
                        v.push(format!("Row {}", n + 1));
                        let new_len = n + 1;
                        r.set(v);
                        rd.set(new_len.to_string());
                    }
                })
            )
            .push(
                HStack::new().gap(12.0)
                    .push(
                        SizedBox::new().width(500.0).height(300.0)
                            .child(
                                Table::new(rows.clone())
                                    .columns(vec![
                                        TableColumn::new("Name", ColumnWidth::Fixed(120.0)).render(|r: &String, _, _| r.clone()),
                                        TableColumn::new("Value", ColumnWidth::Fixed(80.0)).render(|_: &String, ri, _| format!("{:.1}", ri as f32 * 10.0)),
                                        TableColumn::new("Category", ColumnWidth::Fixed(100.0)).render(|_: &String, ri, _| {
                                            match ri % 3 { 0 => "Alpha", 1 => "Beta", _ => "Gamma" }.to_string()
                                        }).resizable(),
                                        TableColumn::new("Notes", ColumnWidth::Flex(1.0)).render(|_: &String, ri, _| {
                                            format!("Description for row {}", ri + 1)
                                        }).resizable().min_width(80.0),
                                    ])
                                    .selection_signal(selected.clone())
                                    .multi_select(multi_selected.clone())
                                    .row_height(28.0)
                                    .striped(true)
                                    .disabled_rows(disabled_set)
                                    .footer(footer_txt.clone())
                                    .context_menu(vec![
                                        ContextMenuItem::new("Reset columns")
                                            .icon(IconKind::Check)
                                            .shortcut("Ctrl+R")
                                            .action({
                                                let r = rows.clone();
                                                move || { r.set(r.read()); }
                                            }),
                                        ContextMenuItem::new("Sort by").submenu(vec![
                                            // ── child → grandchild ──
                                            ContextMenuItem::new("By name").submenu(vec![
                                                ContextMenuItem::new("Name A-Z").action(|| {}),
                                                ContextMenuItem::new("Name Z-A").action(|| {}),
                                            ]),
                                            // ── child → grandchild → great-grandchild ──
                                            ContextMenuItem::new("By age").submenu(vec![
                                                ContextMenuItem::new("Ascending").submenu(vec![
                                                    ContextMenuItem::new("Strict order").action(|| {}),
                                                    ContextMenuItem::new("Loose order").disabled(),
                                                    ContextMenuItem::new("Tie-break").submenu(vec![
                                                        ContextMenuItem::new("By id").action(|| {}),
                                                        ContextMenuItem::new("By insertion").action(|| {}),
                                                    ]),
                                                ]),
                                                ContextMenuItem::new("Descending").action(|| {}),
                                            ]),
                                            ContextMenuItem::separator(),
                                            // ── deep cascade stress test (6 menus deep) ──
                                            ContextMenuItem::new("Go deep").submenu(vec![
                                                ContextMenuItem::new("Deeper").submenu(vec![
                                                    ContextMenuItem::new("Deeper still").submenu(vec![
                                                        ContextMenuItem::new("Almost there").submenu(vec![
                                                            ContextMenuItem::new("Bottom A").action(|| {}),
                                                            ContextMenuItem::new("Bottom B").disabled(),
                                                            ContextMenuItem::new("Bottom C").action(|| {}),
                                                        ]),
                                                    ]),
                                                ]),
                                            ]),
                                        ]),
                                        ContextMenuItem::separator(),
                                        // ── Checkbox items (state is a snapshot at open time) ──
                                        ContextMenuItem::new("Word wrap").checked(false).action(|| {}),
                                        ContextMenuItem::new("Show grid").checked(true).action(|| {}),
                                        // ── Radio group inside a submenu ──
                                        ContextMenuItem::new("Density").submenu(vec![
                                            ContextMenuItem::new("Compact").radio(false).action(|| {}),
                                            ContextMenuItem::new("Cozy").radio(true).action(|| {}),
                                            ContextMenuItem::new("Comfortable").radio(false).action(|| {}),
                                        ]),
                                        // ── Long submenu (30 items) to exercise scrolling ──
                                        ContextMenuItem::new("Recent files").submenu(
                                            (1..=30)
                                                .map(|i| {
                                                    ContextMenuItem::new(format!("Document {i:02}.txt"))
                                                        .action(|| {})
                                                })
                                                .collect(),
                                        ),
                                        ContextMenuItem::separator(),
                                        ContextMenuItem::new("Inspect").disabled(),
                                    ])
                                    .columns_reorderable(true)
                                    .on_reorder_column({
                                        let rc = col_reorder_count.clone();
                                        let crd = col_reorder_display.clone();
                                        move |_src, _dst| {
                                            let n = rc.read() + 1;
                                            rc.set(n);
                                            crd.set(n.to_string());
                                        }
                                    })
                            )
                    )
                    .push(
                        DemoPanel::new()
                            .field("Selected", sel_display.clone())
                            .field("Rows", rows_display.clone())
                            .field("Col swaps", col_reorder_display.clone())
                            .info("Keyboard", "Up/Down/Home/End")
                            .info("Drag", "Drag header to reorder columns")
                            .info("Role", "Table (AccessKit)")
                    )
            )
    })
}

pub fn tree_section() -> impl Widget {
    #[derive(Clone)]
    struct FileNode {
        name: String,
        children: Vec<FileNode>,
    }
    impl FileNode {
        fn file(name: &str) -> Self {
            Self {
                name: name.to_string(),
                children: vec![],
            }
        }
        fn dir(name: &str, children: Vec<FileNode>) -> Self {
            Self {
                name: name.to_string(),
                children,
            }
        }
    }
    impl TreeNode for FileNode {
        type Id = String;
        fn id(&self) -> String {
            self.name.clone()
        }
        fn label(&self) -> String {
            self.name.clone()
        }
        fn children(&self) -> &[Self] {
            &self.children
        }
    }

    Compositor::new(|_scope| {
        let roots = Signal::new(vec![
            FileNode::dir(
                "src",
                vec![
                    FileNode::dir(
                        "widgets",
                        vec![
                            FileNode::file("tree.rs"),
                            FileNode::file("list.rs"),
                            FileNode::file("table.rs"),
                        ],
                    ),
                    FileNode::dir(
                        "core",
                        vec![FileNode::file("element.rs"), FileNode::file("widget.rs")],
                    ),
                    FileNode::file("lib.rs"),
                ],
            ),
            FileNode::dir("examples", vec![FileNode::file("gallery.rs")]),
            FileNode::file("Cargo.toml"),
            FileNode::file("README.md"),
        ]);

        let expanded = Signal::new(
            ["src"]
                .iter()
                .map(|s| s.to_string())
                .collect::<std::collections::HashSet<_>>(),
        );

        let selected_sig = Signal::new(None::<String>);
        let sel_display = Signal::new(String::new());

        VStack::new().gap(8.0)
            .push(section_title("Tree  G3"))
            .push(section_sub("File tree. Arrow keys to navigate, Left/Right to collapse/expand, click to select."))
            .push(
                HStack::new().gap(12.0)
                    .push(
                        SizedBox::new().width(320.0).height(320.0)
                            .child(
                                Tree::new(roots)
                                    .expanded(expanded)
                                    .selected(selected_sig.clone())
                                    .on_select({
                                        let sd = sel_display.clone();
                                        move |id| { sd.set(id); }
                                    })
                                    .indent(20.0)
                                    .row_height(30.0)
                            )
                    )
                    .push(
                        DemoPanel::new()
                            .field("Selected", sel_display.clone())
                            .info("Keyboard", "Up/Down/Left/Right/Enter/Home/End")
                            .info("Role", "Tree (AccessKit)")
                    )
            )
    })
}
