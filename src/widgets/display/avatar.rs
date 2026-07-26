use std::cell::RefCell;
use std::rc::Rc;

use crate::core::context::MountContext;
use crate::core::widget::Widget;
use crate::core::ElementId;
use crate::ecs::components;
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::styled::{StyleRefinement, Styled};
use crate::style::Color;
use crate::style::Dimension;
use crate::style::TextAlign;
use crate::theme::m3::roles::{ComponentRole, DisplayRole};
use crate::widgets::display::{ContentFit, ImageData};

/// A circular user avatar with initials or an image.
pub struct Avatar {
    name: String,
    size: f32,
    background: Option<Color>,
    image: Option<AvatarImage>,
    style: StyleRefinement,
}

/// Image data for an avatar.
pub struct AvatarImage {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl AvatarImage {
    pub fn from_rgba(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
        }
    }

    #[cfg(feature = "ext-image")]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let img = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| format!("format detect: {}", e))?
            .decode()
            .map_err(|e| format!("decode: {}", e))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(Self {
            pixels: rgba.into_raw(),
            width: w,
            height: h,
        })
    }

    fn hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.width.hash(&mut h);
        self.height.hash(&mut h);
        for &b in &self.pixels[..64.min(self.pixels.len())] {
            b.hash(&mut h);
        }
        h.finish()
    }
}

impl Avatar {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            size: 40.0,
            background: None,
            image: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }
    pub fn image(mut self, img: AvatarImage) -> Self {
        self.image = Some(img);
        self
    }
}

impl Styled for Avatar {
    fn style_refinement(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Widget for Avatar {
    fn component_mask(&self) -> u64 {
        components::STYLE
            | components::LAYOUT
            | components::TRANSFORM
            | components::ACCESSIBLE
            | components::LIFECYCLE
    }

    fn mount_box(mut self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        let role = ComponentRole::Display(DisplayRole::Avatar);
        let a_resolved = match ctx.theme.resolve_component(&role) {
            crate::theme::m3::roles::ResolvedComponentStyle::Avatar(s) => s,
            _ => unreachable!(),
        };
        let actual_size = self.size;
        let is_image = self.image.is_some();
        let id = ctx.arena.allocate();
        ctx.preallocate(id, self.component_mask());
        {
            let Some(element) = ctx.arena.get_mut(id) else {
                return id;
            };
            element.set_accessible_role(accesskit::Role::Image);
            element.set_accessible_label(self.name.clone());
            element.set_affected_by_child_size(false);
            element.set_preferred_width(Some(actual_size));
            element.set_preferred_height(actual_size);
            element.set_flex_grow(0.0);
            element.set_flex_shrink(0.0);
            element.set_corner_radii(self.style.corner_radius.unwrap_or(a_resolved.corner_radius));

            if let Some(img) = self.image {
                let hash = img.hash();
                let pixels_rc = Rc::new(img.pixels);
                crate::render::wgpu::register_image_for(
                    id,
                    hash,
                    img.width,
                    img.height,
                    pixels_rc.clone(),
                );
                element.insert_user_data(ImageData {
                    pixels: pixels_rc,
                    width: img.width,
                    height: img.height,
                    hash,
                    fit: ContentFit::Cover,
                });
            } else {
                let bg = self
                    .background
                    .or(self.style.background)
                    .unwrap_or_else(|| avatar_color(&self.name));
                element.set_background(bg);
                element.set_foreground(self.style.text_color.unwrap_or(Color::WHITE));
                element.set_font_size(actual_size * 0.4);
                element.set_font_weight(600);
                element.set_text_align(TextAlign::Center);
                self.style.font_size = Some(actual_size * 0.4);
                self.style.font_weight = Some(600);

                let initials = get_initials(&self.name);
                let buf = Rc::new(RefCell::new(create_buffer(
                    &initials,
                    element.font_size(),
                    element.line_height(),
                    element.font_weight(),
                    element.font_family().as_deref(),
                    Some(actual_size),
                    element.text_align(),
                )));
                element.set_text_buffer(buf);
            }

            if let Some(zi) = self.style.z_index {
                element.set_z_index(zi);
            }
            if let Some(o) = self.style.opacity {
                element.set_opacity(o);
            }
            if let Some(tx) = self.style.transform {
                element.set_transform(Some(tx));
            }
        }
        if is_image {
            if let Some(lc) = ctx.arena.component_tables.borrow_mut().lc.get_mut(&id) {
                lc.component_role = Some(role.clone());
            }
            crate::ecs::register_theme_element(id);
        } else {
            self.style.height = Some(Dimension::Pixels(actual_size));
            self.style.width = Some(Dimension::Pixels(actual_size));
            ctx.register_theme_component(
                id,
                &crate::theme::m3::roles::ResolvedComponentStyle::Avatar(a_resolved.clone()),
                &role,
                &self.style,
            );
        }
        id
    }
}

impl std::fmt::Debug for Avatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Avatar")
            .field("name", &self.name)
            .field("size", &self.size)
            .field("has_image", &self.image.is_some())
            .finish_non_exhaustive()
    }
}

fn get_initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

fn avatar_color(name: &str) -> Color {
    let hash: u32 = name
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let colors: [(u8, u8, u8); 8] = [
        (59, 130, 246),
        (239, 68, 68),
        (34, 197, 94),
        (245, 158, 11),
        (168, 85, 247),
        (236, 72, 153),
        (20, 184, 166),
        (251, 146, 60),
    ];
    let (r, g, b) = colors[(hash % 8) as usize];
    Color::rgba8(r, g, b, 255)
}
