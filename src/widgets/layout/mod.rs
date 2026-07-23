//! Layout containers: VStack, HStack, ZStack, Center, Padding, Spacer, SizedBox, etc.

mod center;
mod conditional;
mod expanded;
mod flexible;
mod grid;
mod hstack;
mod opacity;
mod padding;
mod safe_area;
mod scroll;
mod sized_box;
mod spacer;
mod split_pane;
pub mod sticky_header;
mod vstack;
mod zstack;

pub use center::Center;
pub use conditional::Conditional;
pub use expanded::Expanded;
pub use flexible::Flexible;
pub use grid::{GridItem, GridRow};
pub use hstack::HStack;
pub use opacity::Opacity;
pub use padding::Padding;
pub use safe_area::SafeArea;
pub use scroll::{ScrollDirection, ScrollView};
pub use sized_box::SizedBox;
pub use spacer::Spacer;
pub use split_pane::{SplitDirection, SplitPane};
pub use sticky_header::{StickyDirection, StickyHeader, StickyMode};
pub use vstack::VStack;
pub use zstack::ZStack;

use crate::core::Element;
use crate::style::styled::StyleRefinement;
use crate::style::Dimension;

pub(crate) fn apply_style(style: &StyleRefinement, element: &mut Element) {
    if let Some(g) = style.gap {
        element.set_gap(g);
    }
    if let Some(p) = style.padding {
        element.set_padding(p);
    }
    if let Some(m) = style.margin {
        element.set_margin(m);
    }
    if let Some(bg) = style.background {
        element.set_background(bg);
    }
    if let Some(tc) = style.text_color {
        element.set_foreground(tc);
    }
    if let Some(bw) = style.border_width {
        element.set_border_width(bw);
    }
    if let Some(bc) = style.border_color {
        element.set_border_color(bc);
    }
    if let Some(cr) = style.corner_radius {
        element.set_corner_radii(cr);
    }
    if let Some(ow) = style.outline_width {
        element.set_outline_width(ow);
    }
    if let Some(oc) = style.outline_color {
        element.set_outline_color(oc);
    }
    if let Some(zi) = style.z_index {
        element.set_z_index(zi);
    }
    if let Some(sh) = style.shadow {
        element.set_shadow(Some(sh));
    }
    if let Some(bm) = style.blend_mode {
        element.set_blend_mode(bm.to_u8());
    }
    if let Some(bf) = style.backdrop_filter {
        element.set_backdrop_filter(Some(bf));
    }
    if let Some(grad) = style.gradient {
        element.set_gradient(Some(grad));
    }
    if let Some(o) = style.opacity {
        element.set_opacity(o);
    }
    if let Some(fg) = style.foreground {
        element.set_foreground(fg);
    }
    if let Some(ref ss) = style.state_style {
        element.with_state_style(|s| {
            macro_rules! apply_variant {
                ($dest:ident, $src:ident) => {
                    if let Some(ref v) = ss.$src.background {
                        s.$dest.background = Some(*v);
                    }
                    if let Some(ref v) = ss.$src.foreground {
                        s.$dest.foreground = Some(*v);
                    }
                    if let Some(ref v) = ss.$src.border_color {
                        s.$dest.border_color = Some(*v);
                    }
                    if let Some(v) = ss.$src.border_width {
                        s.$dest.border_width = Some(v);
                    }
                    if let Some(v) = ss.$src.opacity {
                        s.$dest.opacity = Some(v);
                    }
                    if let Some(ref v) = ss.$src.shadow {
                        s.$dest.shadow = Some(*v);
                    }
                };
            }
            apply_variant!(hovered, hovered);
            apply_variant!(pressed, pressed);
            apply_variant!(focused, focused);
            apply_variant!(disabled, disabled);
            apply_variant!(checked, checked);
            apply_variant!(loading, loading);
            apply_variant!(invalid, invalid);
            apply_variant!(indeterminate, indeterminate);
            apply_variant!(drag_over, drag_over);
        });
    }
    if let Some(td) = style.text_direction {
        element.set_text_direction(td);
    }
    if let Some(ta) = style.text_align {
        element.set_text_align(ta);
    }
    if let Some(tx) = style.transform {
        element.set_transform(Some(tx));
    }

    if let Some(Dimension::Pixels(px)) = style.width {
        if px > 0.0 {
            element.set_preferred_width(Some(px));
        }
    }
    if let Some(Dimension::Pixels(px)) = style.height {
        if px > 0.0 {
            element.set_preferred_height(px);
        }
    }
    // Store original Dimension for percent-aware taffy resolution
    if let Some(ref w) = style.width {
        element.set_width_dim(Some(*w));
    }
    if let Some(ref h) = style.height {
        element.set_height_dim(*h);
    }
    if let Some(fs) = style.font_size {
        element.set_font_size(fs);
    }
    if let Some(fw) = style.font_weight {
        element.set_font_weight(fw);
    }
    if let Some(lh) = style.line_height {
        element.set_line_height(lh);
    }
    if let Some(ref ff) = style.font_family {
        element.set_font_family(Some(ff.clone()));
    }
    if let Some(td) = style.text_decoration {
        element.set_text_decoration(td);
    }
    if let Some(to) = style.text_overflow {
        element.set_text_overflow(to);
    }
    if let Some(pc) = style.placeholder_color {
        element.set_placeholder_color(pc);
    }
    if let Some(vis) = style.visible {
        element.set_visible(vis);
    }
    if let Some(flex_grow) = style.flex_grow {
        element.set_flex_grow(flex_grow);
    }
    if let Some(flex_shrink) = style.flex_shrink {
        element.set_flex_shrink(flex_shrink);
    }
    if let Some(flex_basis) = style.flex_basis {
        if let Dimension::Pixels(px) = flex_basis {
            element.set_flex_basis(px)
        }
        element.set_flex_basis_dim(flex_basis);
    }
    if let Some(flex_wrap) = style.flex_wrap {
        element.set_flex_wrap(flex_wrap);
    }
    if let Some(overflow) = style.overflow {
        element.set_overflow(overflow);
    }
    if let Some(aspect_ratio) = style.aspect_ratio {
        element.set_aspect_ratio(aspect_ratio);
    }
    if let Some(order) = style.order {
        element.set_order(order);
    }
    if let Some(scrollbar_policy) = style.scrollbar_policy {
        element.set_scrollbar_policy(scrollbar_policy);
    }
    if let Some(content_align) = style.content_align {
        element.set_content_align(content_align);
    }
}
