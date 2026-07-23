//! Paint tree traversal: surface, text, decoration, subtree caching, and scrollbar rendering.
//! Extracted from `platform/window.rs` (audit 2026-07-21).

use glam::Affine2;
use std::rc::Rc;

use crate::core::config::StateFlags;
use crate::core::element::{Element, ElementArena};
use crate::core::ElementId;
use crate::render::painter::{ClipInfo, Painter};
use crate::render::wgpu::glyphon_bridge::create_buffer;
use crate::style::resolve_style;
use crate::style::{Color, Rect};

// ── Paint ─────────────────────────────────────────────────────────

fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let r = (a.x + a.width).min(b.x + b.width);
    let btm = (a.y + a.height).min(b.y + b.height);
    if r > x && btm > y {
        Rect::new(x, y, r - x, btm - y)
    } else {
        Rect::new(x, y, 0.0, 0.0)
    }
}

/// Universal slack absorbing small decor overhang (underline strokes,
/// slider thumbs, 1-2px AA bleed) that paints slightly outside
/// `screen_bounds` without a dedicated style field.
const AABB_GROW_BASE: f32 = 8.0;

/// TextInput error text paints at `x + w + 8` with a shaped width of up to
/// 400px (see the decor pass) — extend the AABB rightwards to cover it.
const AABB_ERROR_TEXT_EXTEND: f32 = 408.0;

/// Layout-space visual AABB of `eid`'s subtree, memoized per element and
/// keyed by `subtree_generation` (audit 2026-07-17 L2 paint culling).
///
/// Covers: own rect + position_offset/size_scale + shadow/outline growth +
/// error-text overflow, unioned with all descendant AABBs, then passed
/// through the element's own transform (corner union, damage-tracker
/// semantics — descendants compose about this element's pivot).
///
/// Soundness notes:
/// - Nested scroll offsets move descendants UP/LEFT of their layout rects,
///   but a scroll offset only exists on Scroll/Clip containers whose own
///   clip (⊆ own rect ⊆ this AABB) bounds those descendants.
/// - slot_inactive children are included (conservative superset).
fn subtree_visual_aabb(
    arena: &ElementArena,
    ct: &crate::ecs::tables::ComponentTables,
    eid: ElementId,
) -> Rect {
    let Some(el) = arena.get(eid) else {
        return Rect::ZERO;
    };
    let gen = el.subtree_generation.get();
    if let Some((g, r)) = el.subtree_aabb.get() {
        if g == gen {
            return r;
        }
    }

    let sb = el.screen_bounds;
    let mut x = sb.x;
    let mut y = sb.y;
    let mut w = sb.width.max(1.0);
    let mut h = sb.height.max(1.0);

    let xform = ct.xform.get(&eid);
    if let Some(xf) = xform {
        let off = xf.position_offset.get();
        x += off.x;
        y += off.y;
        let sc = xf.size_scale.get();
        w *= sc.x;
        h *= sc.y;
    }

    let mut grow = AABB_GROW_BASE;
    if let Some(s) = ct.style.get(&eid) {
        if let Some(ref sh) = s.shadow {
            grow = grow.max(sh.blur + sh.offset_x.abs().max(sh.offset_y.abs()));
        }
        if s.outline_width > 0.0 {
            grow = grow.max(s.outline_width + 2.0);
        }
        // StateStyle overrides can add shadows on hover/press — include them.
        if let Some(ref st) = s.state_style {
            for variant in [
                &st.animated,
                &st.hovered,
                &st.pressed,
                &st.focused,
                &st.disabled,
                &st.checked,
                &st.loading,
                &st.invalid,
                &st.indeterminate,
                &st.drag_over,
            ] {
                if let Some(ref sh) = variant.shadow {
                    grow = grow.max(sh.blur + sh.offset_x.abs().max(sh.offset_y.abs()));
                }
            }
        }
    }
    let mut aabb = Rect::new(x - grow, y - grow, w + 2.0 * grow, h + 2.0 * grow);

    if ct.lc.get(&eid).is_some_and(|l| l.error_text.is_some()) {
        aabb.width += AABB_ERROR_TEXT_EXTEND;
    }

    for &cid in &el.children {
        let ca = subtree_visual_aabb(arena, ct, cid);
        if ca.width > 0.0 && ca.height > 0.0 {
            aabb = aabb.union(&ca);
        }
    }

    // Own transform: descendants compose about this element's pivot, so
    // applying it to the whole subtree union is the exact composition.
    // Union with the untransformed rect (mid-animation safety, mirrors
    // the damage tracker).
    if let Some(t) = xform.and_then(|xf| xf.transform) {
        let tx = glam::Affine2::from_cols_array(&t);
        let ox = xform.map_or(0.5, |xf| xf.transform_origin_x) * w;
        let oy = xform.map_or(0.5, |xf| xf.transform_origin_y) * h;
        let to_origin = glam::Affine2::from_translation(glam::Vec2::new(-(x + ox), -(y + oy)));
        let from_origin = glam::Affine2::from_translation(glam::Vec2::new(x + ox, y + oy));
        let m = from_origin * tx * to_origin;
        let c = [
            m.transform_point2(glam::Vec2::new(aabb.x, aabb.y)),
            m.transform_point2(glam::Vec2::new(aabb.x + aabb.width, aabb.y)),
            m.transform_point2(glam::Vec2::new(aabb.x + aabb.width, aabb.y + aabb.height)),
            m.transform_point2(glam::Vec2::new(aabb.x, aabb.y + aabb.height)),
        ];
        let min_x = c.iter().map(|p| p.x).fold(f32::MAX, f32::min);
        let min_y = c.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        let max_x = c.iter().map(|p| p.x).fold(f32::MIN, f32::max);
        let max_y = c.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        aabb = aabb.union(&Rect::new(min_x, min_y, max_x - min_x, max_y - min_y));
    }

    el.subtree_aabb.set(Some((gen, aabb)));
    aabb
}

/// Return children sorted by z_index, with generation-based caching.
/// Cache is invalidated by add_child/remove_child/clear_children and z_index changes.
fn sorted_children(arena: &ElementArena, eid: ElementId) -> Rc<Vec<ElementId>> {
    let Some(el) = arena.get(eid) else {
        return Rc::new(Vec::new());
    };
    // Check cache: valid if children list matches
    if let Some(ref cached) = *el.sorted_children.borrow() {
        if cached.len() == el.children.len() {
            let match_ids = cached.iter().zip(&el.children).all(|(a, b)| a == b);
            if match_ids {
                return Rc::clone(cached);
            }
        }
    }
    // Rebuild: sort children by z_index
    let children = &el.children;
    let mut sorted: Vec<(ElementId, i32)> = children
        .iter()
        .map(|&cid| (cid, arena.get(cid).map_or(0, |c| c.z_index)))
        .collect();
    sorted.sort_by_key(|&(_, z)| z);
    let result: Rc<Vec<ElementId>> = Rc::new(sorted.into_iter().map(|(id, _)| id).collect());
    *el.sorted_children.borrow_mut() = Some(Rc::clone(&result));
    result
}

pub(crate) fn paint_children_sorted(
    fcx: &crate::core::frame_context::FrameContext,
    ct: &crate::ecs::tables::ComponentTables,
    arena: &mut ElementArena,
    eid: ElementId,
    painter: &mut Painter,
    text_areas: &mut Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
    scale_factor: f32,
    scroll_offset: (f32, f32),
    parent_should_paint: bool,
    default_bg: Color,
    default_fg: Color,
    accum_clip: Rect,
    accum_clip_radius: crate::style::CornerRadii,
    accum_xform: Affine2,
    parent_floor: i32,
    must_paint: &std::collections::HashSet<ElementId>,
    accum_opacity: f32,
    highlight_mode: crate::event::FocusHighlightMode,
) {
    let child_ids: Rc<Vec<ElementId>> = sorted_children(arena, eid);

    let parent_scroll_gen = arena
        .get(eid)
        .and_then(|el| el.get_user_data::<crate::widgets::bundle::ScrollGeneration>())
        .map(|sg| sg.0.get())
        .unwrap_or(0);

    for &cid in child_ids.iter() {
        // `child_ids` is a cached sorted-children snapshot (Rc<Vec>). An element
        // can be removed mid-frame (e.g. a submenu portal trimmed by a menu
        // interaction), leaving a stale id in this snapshot. Skip gracefully
        // instead of unwrapping — the removal already invalidated the cache, so
        // the next frame rebuilds it correctly.
        let child = match arena.get(cid) {
            Some(c) => c,
            None => continue,
        };
        if child.slot_inactive.get()
            || !ct
                .lc
                .get(&cid)
                .and_then(|l| l.reactive_visible.as_ref())
                .is_none_or(|v| v.get())
        {
            continue;
        }
        // ── Subtree AABB cull (audit 2026-07-17 L2): skip whole child
        // subtrees fully outside the accumulated clip without descending.
        // Turns scroll-frame paint from O(N_content) into O(visible).
        // Gate: accum_xform must be translation-only (scroll containers fold
        // their -offset translation into it; that shift is mirrored by
        // scroll_offset in clip space, matching the per-leaf cull's
        // `rect - scroll_offset ∩ accum_clip` semantics). True rotation /
        // scale transforms fall through to the per-leaf cull, same as today.
        if accum_xform.matrix2 == glam::Mat2::IDENTITY {
            let aabb = subtree_visual_aabb(arena, ct, cid);
            let vis_x = aabb.x - scroll_offset.0;
            let vis_y = aabb.y - scroll_offset.1;
            if vis_x + aabb.width <= accum_clip.x
                || vis_y + aabb.height <= accum_clip.y
                || vis_x >= accum_clip.x + accum_clip.width
                || vis_y >= accum_clip.y + accum_clip.height
            {
                continue;
            }
        }
        // Ticking elements are cache-eligible: every frame_tick site self-
        // reports changes via register_dirty / gen bumps (audited 2026-07-16,
        // Layer 3-3), so needs_repaint + must_paint fully cover invalidation.
        if !must_paint.contains(&cid)
            && !child.needs_repaint()
            && try_skip_subtree(
                fcx,
                ct,
                arena,
                cid,
                painter,
                text_areas,
                scroll_offset,
                parent_should_paint,
                parent_scroll_gen,
            )
        {
            continue;
        }

        let cmd_start = painter.snapshot_len();
        let ta_start = text_areas.len();
        let bd_start = painter.backdrop_regions.len();
        let root_x = child.screen_bounds.x;
        let root_y = child.screen_bounds.y;
        let _ = child;

        paint_element_tree(
            fcx,
            ct,
            arena,
            cid,
            painter,
            text_areas,
            scale_factor,
            scroll_offset,
            parent_should_paint,
            default_bg,
            default_fg,
            accum_clip,
            accum_clip_radius,
            accum_xform,
            parent_floor,
            must_paint,
            accum_opacity,
            highlight_mode,
        );

        let gen_after = crate::core::dirty_registry::content_gen_of(cid);
        let layout_after = crate::core::dirty_registry::layout_gen_of(cid);
        {
            let mut new_commands: Vec<crate::render::DrawCommand> =
                painter.commands_since(cmd_start).to_vec();
            let mut new_ta: Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc> =
                text_areas[ta_start..].to_vec();
            if new_ta.is_empty()
                && arena.get(cid).is_some_and(|el| {
                    el.children.iter().any(|&ccid| {
                        arena.get(ccid).is_some_and(|_gc| {
                            ct.text
                                .get(&ccid)
                                .and_then(|t| t.text_buffer.clone())
                                .is_some()
                        })
                    })
                })
            {
                new_commands.clear();
                new_ta.clear();
            }
            {
                let c = fcx.subtree_cache;
                let new_backdrops: Vec<crate::render::BackdropRegion> =
                    painter.backdrop_regions[bd_start..].to_vec();
                c.borrow_mut().insert(
                    cid,
                    Rc::new(crate::render::CachedSubtree {
                        commands: new_commands,
                        text_areas: new_ta,
                        backdrop_regions: new_backdrops,
                        root_x,
                        root_y,
                        scroll_ox: scroll_offset.0,
                        scroll_oy: scroll_offset.1,
                        content_gen: gen_after,
                        layout_gen: layout_after,
                        scroll_gen: parent_scroll_gen,
                    }),
                );
            }
        }
    }
}

/// Try to skip painting a child subtree by replaying cached draw commands.
/// Returns `true` if the subtree was successfully replayed from cache.
/// Cache validity requires `content_gen` match (draw commands unchanged)
/// AND scroll position match (clip rects are scroll-dependent).
fn try_skip_subtree(
    fcx: &crate::core::frame_context::FrameContext,
    ct: &crate::ecs::tables::ComponentTables,
    arena: &ElementArena,
    child_id: ElementId,
    painter: &mut Painter,
    text_areas: &mut Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
    scroll_offset: (f32, f32),
    _parent_should_paint: bool,
    parent_scroll_gen: u64,
) -> bool {
    let child = match arena.get(child_id) {
        Some(c) => c,
        None => return true,
    };
    if child.slot_inactive.get()
        || !ct
            .lc
            .get(&child_id)
            .and_then(|l| l.reactive_visible.as_ref())
            .is_none_or(|v| v.get())
    {
        return true;
    }
    if child.needs_repaint() {
        crate::core::frame_pipeline::bump_subtree_cache_miss();
        return false;
    }
    let current_content = crate::core::dirty_registry::content_gen_of(child_id);
    let cache_hit = {
        let c = fcx.subtree_cache;
        let cache = c.borrow();
        cache.get(&child_id).is_some_and(|cs| {
            cs.content_gen == current_content
                && cs.scroll_gen == parent_scroll_gen
                && cs.scroll_ox == scroll_offset.0
                && cs.scroll_oy == scroll_offset.1
                && (!cs.commands.is_empty() || !cs.text_areas.is_empty())
                && !(cs.text_areas.is_empty()
                    && child.children.iter().any(|&ccid| {
                        arena.get(ccid).is_some_and(|_gc| {
                            ct.text
                                .get(&ccid)
                                .and_then(|t| t.text_buffer.clone())
                                .is_some()
                        })
                    }))
        })
    };
    if cache_hit {
        crate::core::frame_pipeline::bump_subtree_cache_hit();
        replay_subtree(fcx, arena, child_id, painter, text_areas, scroll_offset);
        return true;
    }
    crate::core::frame_pipeline::bump_subtree_cache_miss();
    false
}

fn replay_subtree(
    fcx: &crate::core::frame_context::FrameContext,
    arena: &ElementArena,
    child_id: ElementId,
    painter: &mut Painter,
    text_areas: &mut Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
    scroll_offset: (f32, f32),
) {
    let child = match arena.get(child_id) {
        Some(c) => c,
        None => return,
    };
    {
        let c = fcx.subtree_cache;
        if let Some(cs) = c.borrow().get(&child_id) {
            // Screen position delta (layout change)
            let pos_dx = child.screen_bounds.x - cs.root_x;
            let pos_dy = child.screen_bounds.y - cs.root_y;
            // Scroll delta: cached scroll minus current scroll
            let scroll_dx = cs.scroll_ox - scroll_offset.0;
            let scroll_dy = cs.scroll_oy - scroll_offset.1;

            let scroll_xform =
                glam::Affine2::from_translation(glam::Vec2::new(scroll_dx, scroll_dy));

            for cmd in &cs.commands {
                let mut c = cmd.offset(pos_dx, pos_dy);
                c.adjust_transform(scroll_xform);
                painter.commands.push(c);
            }
            text_areas.extend(cs.text_areas.iter().map(|ta| {
                let ta = ta.offset(pos_dx, pos_dy);
                ta.offset(scroll_dx, scroll_dy)
            }));
            // Re-emit backdrop-blur regions (offset like commands) so the effect
            // persists on cached/static frames.
            for br in &cs.backdrop_regions {
                let mut r = br.clone();
                r.rect = r.rect.offset(pos_dx, pos_dy);
                r.transform = scroll_xform * r.transform;
                painter.backdrop_regions.push(r);
            }
        }
    }
}

/// Stretch anchored-dropdown (Select/ComboBox) portal children to the portal's
/// content width. taffy leaf sizing gives text options a fixed content width
/// that defeats `align_items: STRETCH`, so we fill them to the portal width
/// here. Portal-native replacement for the old `overlay_layer` stretch pass.
/// Shared by the window frame loop and the TestHarness.
// (stretch_visible_anchored_portals / stretch_children_to_width moved to
// platform/portal.rs — audit round 3, ② phase 1)

fn paint_element_surface(
    element: &Element,
    style_comp: Option<&crate::ecs::components::StyleComponent>,
    lc_comp: Option<&crate::ecs::components::LifecycleComponent>,
    painter: &mut Painter,
    _x: f32,
    _y: f32,
    w: f32,
    h: f32,
    _element_clip: ClipInfo,
    _element_xform: Affine2,
    ez: i32,
    _accum_clip: ClipInfo,
    _opacity: f32,
    default_bg: Color,
    highlight_mode: crate::event::FocusHighlightMode,
) {
    let style_default;
    let s = match style_comp {
        Some(s) => s,
        None => {
            style_default = crate::ecs::components::StyleComponent::default();
            &style_default
        }
    };
    let resolved = resolve_style(element.state.get(), s);
    let corners = s.corners();

    if resolved.backdrop {
        let bg = resolved.background.unwrap_or(Color::rgba8(0, 0, 0, 160));
        let screen = Rect::new(0.0, 0.0, painter.viewport.width, painter.viewport.height);
        painter.fill_rect(screen, bg, screen.into(), glam::Affine2::IDENTITY, ez);
    }

    let focused = element.state.get().contains(StateFlags::FOCUSED);
    let show_auto_ring = focused
        && resolved.outline_width == 0.0
        && highlight_mode.show_focus_ring()
        && !resolved.backdrop; // backdrop paints full-screen gap fill that would erase the scrim
    let ow = if show_auto_ring {
        2.0
    } else {
        resolved.outline_width
    };
    if ow > 0.0 && (show_auto_ring || resolved.outline_width > 0.0) && !resolved.backdrop {
        let oc = resolved
            .outline_color
            .unwrap_or(Color::rgba8(59, 130, 246, 255));
        let gap = 0.0;
        painter.push_local_outline(ow, gap, oc, default_bg, w, h, corners);
    }

    if let Some(ref sh) = resolved.shadow {
        painter.push_local_shadow(*sh, w, h, corners);
    }

    // Compute the final interior fill colour BEFORE deciding border method,
    // so Outlined buttons (raw bg=TRANSPARENT → interior=default_bg) correctly
    // use BorderFill instead of falling back to thin StrokeRect.
    let mut interior: Option<Color> = None;
    if let Some(_grad) = resolved.gradient {
        // Gradient is pushed separately; interior remains None for now.
    } else if !resolved.backdrop {
        let bg = resolved.background;
        interior = match bg {
            Some(Color::TRANSPARENT)
                if resolved.border_width > 0.0 && resolved.border_color.is_some() =>
            {
                // Explicit TRANSPARENT + border (Outlined buttons, etc.) — fill
                // interior with default_bg so the border is visible against it.
                Some(default_bg)
            }
            other => other,
        };
    }

    if resolved.border_width > 0.0 {
        if let Some(bc) = if lc_comp
            .and_then(|l| l.invalid_hint.as_ref())
            .is_some_and(|h| h.get())
        {
            Some(Color::rgba8(220, 38, 38, 255))
        } else {
            resolved.border_color
        } {
            let hw = resolved.border_width;
            // Use BorderFill (large rect clipped to border) only when the
            // final interior is opaque (covers the center).  Otherwise
            // BorderFill would paint the whole element in bc, hiding parent
            // backgrounds (e.g. Table row zebra stripes).
            let has_interior = interior.is_some_and(|c| c != Color::TRANSPARENT)
                || resolved.gradient.is_some()
                || resolved.backdrop;
            if has_interior {
                let rh = hw;
                let border_corners = crate::style::CornerRadii {
                    top_left: corners.top_left + rh,
                    top_right: corners.top_right + rh,
                    bottom_right: corners.bottom_right + rh,
                    bottom_left: corners.bottom_left + rh,
                };
                painter.push_local_border_fill(
                    Rect::new(-hw, -hw, w + hw * 2.0, h + hw * 2.0),
                    bc,
                    border_corners,
                );
            } else {
                painter.push_local_stroke_rect(Rect::new(0.0, 0.0, w, h), bc, hw, corners);
            }
        }
    }

    // Push interior fill.
    if let Some(grad) = resolved.gradient {
        painter.push_local_linear_gradient(Rect::new(0.0, 0.0, w, h), grad, corners);
    } else if let Some(c) = interior {
        if resolved.blend_mode != 0 {
            painter.push_local_fill_rect_blend(
                Rect::new(0.0, 0.0, w, h),
                c,
                corners,
                resolved.blend_mode,
            );
        } else {
            painter.push_local_fill_rect(Rect::new(0.0, 0.0, w, h), c, corners);
        }
    }
}

fn record_element_text(
    element: &Element,
    text_comp: Option<&crate::ecs::components::TextComponent>,
    style_comp: Option<&crate::ecs::components::StyleComponent>,
    layout_comp: Option<&crate::ecs::components::LayoutComponent>,
    scroll_comp: Option<&crate::ecs::components::ScrollComponent>,
    local_ta: &mut Vec<crate::render::LocalTextArea>,
    _x: f32,
    _y: f32,
    w: f32,
    h: f32,
    scale_factor: f32,
    default_fg: Color,
) {
    let buf = text_comp.and_then(|t| t.text_buffer.clone());
    if let Some(ref buf) = buf {
        let scroll_x = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_x.get());
        let scroll_y = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_y.get());
        let is_ph = text_comp.is_some_and(|t| t.is_placeholder.get());
        let fg = if is_ph {
            text_comp
                .and_then(|t| t.placeholder_color)
                .unwrap_or(Color::rgba8(150, 150, 165, 255))
        } else {
            let style_default;
            let s = match style_comp {
                Some(s) => s,
                None => {
                    style_default = crate::ecs::components::StyleComponent::default();
                    &style_default
                }
            };
            let resolved = resolve_style(element.state.get(), s);
            resolved.foreground.unwrap_or(default_fg)
        };
        let lazy_fp = text_comp.and_then(|t| t.lazy_font_params.clone());
        let (eff_fs, eff_lh) = if let Some(ref fp) = lazy_fp {
            (fp.font_size, fp.line_height)
        } else {
            (
                text_comp.map_or(18.0, |t| t.font_size),
                text_comp.map_or(1.5, |t| t.line_height),
            )
        };
        let pad = layout_comp.map_or(crate::style::Padding::ZERO, |l| l.padding);
        let vcenter = text_comp.is_none_or(|t| t.text_vertical_center);
        let local_y_offset = if vcenter {
            let content_h = h - pad.top - pad.bottom;
            let text_h = eff_fs * eff_lh;
            pad.top + (content_h - text_h).max(0.0) / 2.0
        } else {
            pad.top
        };
        let tov = style_comp.map_or(crate::style::styled::TextOverflow::Clip, |s| {
            s.text_overflow
        });
        let (cl, ct_, cw, ch) = if tov == crate::style::styled::TextOverflow::Ellipsis {
            (
                pad.left,
                pad.top,
                (w - pad.left - pad.right).max(1.0),
                (h - pad.top - pad.bottom).max(1.0),
            )
        } else {
            (
                pad.left,
                pad.top,
                (w - pad.left - pad.right).max(1.0),
                (h - pad.top - pad.bottom).max(1.0),
            )
        };
        let mut local_left = pad.left;
        {
            let buf_ref = buf.borrow();
            let has_emoji = buf_ref
                .lines
                .iter()
                .any(|l| l.text().chars().any(|c| c as u32 > 0x1F000));
            if has_emoji
                && text_comp.map_or(crate::style::TextAlign::Start, |t| t.text_align)
                    == crate::style::TextAlign::Center
            {
                let content_w = (w - pad.left - pad.right).max(1.0);
                let mut glyph_w: f32 = 0.0;
                for run in buf_ref.layout_runs() {
                    for g in run.glyphs {
                        glyph_w = glyph_w.max(g.x + g.w);
                    }
                }
                if glyph_w > 0.0 && glyph_w < content_w {
                    local_left += (content_w - glyph_w) / 2.0;
                }
            }
        }
        local_ta.push(crate::render::LocalTextArea {
            buffer: buf.clone(),
            generation: text_comp.map_or(0, |t| t.text_generation.get()),
            scale: scale_factor,
            color: fg,
            scroll_x,
            scroll_y,
            local_left,
            local_top: local_y_offset,
            clip_local_x: cl,
            clip_local_y: ct_,
            clip_w: cw,
            clip_h: ch,
        });
    }
}

fn paint_element_decor(
    _element: &Element,
    text_comp: Option<&crate::ecs::components::TextComponent>,
    style_comp: Option<&crate::ecs::components::StyleComponent>,
    layout_comp: Option<&crate::ecs::components::LayoutComponent>,
    scroll_comp: Option<&crate::ecs::components::ScrollComponent>,
    cursor_comp: Option<&crate::ecs::components::CursorComponent>,
    lc_comp: Option<&crate::ecs::components::LifecycleComponent>,
    painter: &mut Painter,
    decor_ta: &mut Vec<crate::render::LocalTextArea>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    element_clip: ClipInfo,
    element_xform: Affine2,
    ez: i32,
    accum_clip: ClipInfo,
    opacity: f32,
    _default_fg: Color,
    scale_factor: f32,
) {
    let fade = |c: Color| -> Color {
        if opacity < 1.0 {
            c.with_alpha(c.a * opacity)
        } else {
            c
        }
    };

    if let Some(ref sr) = cursor_comp.map(|c| c.selection_rect.clone()) {
        let rects = sr.replace(Vec::new());
        if !rects.is_empty() {
            let scroll_x = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_x.get());
            let scroll_y = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_y.get());
            let highlight = text_comp
                .and_then(|t| t.selection_color)
                .unwrap_or(Color::rgba8(59, 130, 246, 128));
            let pad = layout_comp.map_or(crate::style::Padding::ZERO, |l| l.padding);
            let vis = Rect::new(x + pad.left, y, (w - pad.left - pad.right).max(1.0), h);
            let fs = text_comp.map_or(18.0, |t| t.font_size);
            let lh = text_comp.map_or(1.5, |t| t.line_height);
            let vcenter_off = if text_comp.is_none_or(|t| t.text_vertical_center) {
                let content_h = h - pad.top - pad.bottom;
                let text_h = fs * lh;
                (content_h - text_h).max(0.0) / 2.0
            } else {
                0.0
            };
            for sel_rect in rects.iter() {
                let sel = Rect::new(
                    x + sel_rect.x - scroll_x,
                    y + pad.top + vcenter_off + sel_rect.y - scroll_y,
                    sel_rect.width,
                    sel_rect.height,
                );
                let cx = vis.x.max(sel.x);
                let cy = vis.y.max(sel.y);
                let cr = (vis.x + vis.width).min(sel.x + sel.width);
                let cb = (vis.y + vis.height).min(sel.y + sel.height);
                if cr > cx && cb > cy {
                    painter.fill_rect(
                        Rect::new(cx, cy, cr - cx, cb - cy),
                        fade(highlight),
                        element_clip,
                        element_xform,
                        ez,
                    );
                }
            }
        }
        sr.set(rects);
    }

    if let Some(ref cx) = cursor_comp.map(|c| c.cursor_x.clone()) {
        if let Some(ref cv) = cursor_comp.map(|c| c.cursor_visible.clone()) {
            if cv.get() {
                let cx_val = cx.get();
                let scroll_x = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_x.get());
                let scroll_y = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_y.get());
                let line = cursor_comp.as_ref().map_or(0, |c| c.cursor_line.get());
                let font_size = text_comp.map_or(18.0, |t| t.font_size);
                let line_h = text_comp.map_or(1.5, |t| t.line_height);
                let line_h_val = font_size * line_h;
                let cursor_color = text_comp.and_then(|t| t.caret_color).unwrap_or_else(|| {
                    style_comp
                        .and_then(|s| s.foreground)
                        .unwrap_or(Color::rgba8(0, 0, 0, 255))
                });
                let cursor_w = 2.0;
                let pad = layout_comp.map_or(crate::style::Padding::ZERO, |l| l.padding);
                let vcenter_off2 = if text_comp.is_none_or(|t| t.text_vertical_center) {
                    let content_h = h - pad.top - pad.bottom;
                    let text_h = line_h_val;
                    (content_h - text_h).max(0.0) / 2.0
                } else {
                    0.0
                };
                let cursor_y = y + pad.top + vcenter_off2 + line as f32 * line_h_val
                    - font_size * 0.15
                    - scroll_y;
                let cursor_rect = Rect::new(
                    x + pad.left + cx_val - scroll_x,
                    cursor_y,
                    cursor_w,
                    font_size * 1.8,
                );
                painter.fill_rect(
                    cursor_rect,
                    fade(cursor_color),
                    element_clip,
                    element_xform,
                    ez,
                );
            }
        }
    }

    // IME composition underline — same transform family as the selection
    // rects above (rect carries pad-baked local coords from
    // composition_underline_rect; splice P0: computed-but-never-painted).
    if let Some(ref ul) = cursor_comp.map(|c| c.composition_underline_rect.clone()) {
        if let Some(r) = ul.get() {
            let scroll_x = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_x.get());
            let scroll_y = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_y.get());
            let pad = layout_comp.map_or(crate::style::Padding::ZERO, |l| l.padding);
            let fs = text_comp.map_or(18.0, |t| t.font_size);
            let lh = text_comp.map_or(1.5, |t| t.line_height);
            let vcenter_off = if text_comp.is_none_or(|t| t.text_vertical_center) {
                let content_h = h - pad.top - pad.bottom;
                ((content_h - fs * lh).max(0.0)) / 2.0
            } else {
                0.0
            };
            let color = text_comp.and_then(|t| t.caret_color).unwrap_or_else(|| {
                style_comp
                    .and_then(|s| s.foreground)
                    .unwrap_or(Color::rgba8(0, 0, 0, 255))
            });
            let ur = Rect::new(
                x + r.x - scroll_x,
                y + pad.top + vcenter_off + r.y - scroll_y,
                r.width,
                r.height,
            );
            let vis = Rect::new(x + pad.left, y, (w - pad.left - pad.right).max(1.0), h);
            let cx = vis.x.max(ur.x);
            let cy = vis.y.max(ur.y);
            let cr = (vis.x + vis.width).min(ur.x + ur.width);
            let cb = (vis.y + vis.height).min(ur.y + ur.height);
            if cr > cx && cb > cy {
                painter.fill_rect(
                    Rect::new(cx, cy, cr - cx, cb - cy),
                    fade(color),
                    element_clip,
                    element_xform,
                    ez,
                );
            }
        }
    }

    let text_dec = style_comp.map_or(crate::style::styled::TextDecoration::None, |s| {
        s.text_decoration
    });
    if text_dec != crate::style::styled::TextDecoration::None
        && text_comp.and_then(|t| t.text_buffer.clone()).is_some()
    {
        let font_size = text_comp.map_or(18.0, |t| t.font_size);
        let scroll_y = scroll_comp.as_ref().map_or(0.0, |s| s.text_scroll_y.get());
        let pad = layout_comp.map_or(crate::style::Padding::ZERO, |l| l.padding);
        let start_y = y + pad.top - scroll_y;
        let dec_color = style_comp
            .and_then(|s| s.foreground)
            .unwrap_or(Color::rgba8(200, 200, 210, 255));
        let dec_y = match text_dec {
            crate::style::styled::TextDecoration::Underline => start_y + font_size * 1.35,
            crate::style::styled::TextDecoration::Strikethrough => start_y - font_size * -0.8,
            crate::style::styled::TextDecoration::Overline => start_y - font_size * 0.8,
            _ => start_y,
        };
        let text_w = (w - pad.left - pad.right).max(10.0);
        painter.fill_rect(
            Rect::new(x + pad.left, dec_y, text_w, 2.0),
            fade(dec_color),
            element_clip,
            element_xform,
            ez,
        );
    }

    if let Some(ref et) = lc_comp.and_then(|l| l.error_text.clone()) {
        let guard = et.borrow();
        if let Some(ref err) = *guard {
            let font_size = text_comp.map_or(18.0, |t| t.font_size);
            let err_fs = font_size * 0.85;
            let err_local_left = w + 8.0;
            let err_local_top = (h.max(1.0) - font_size) * 0.5 - font_size * 0.25;
            let err_color = fade(Color::rgba8(220, 38, 38, 255));
            let buf = std::rc::Rc::new(std::cell::RefCell::new(create_buffer(
                err,
                err_fs,
                1.5,
                400,
                None,
                None,
                crate::style::TextAlign::Start,
            )));
            decor_ta.push(crate::render::LocalTextArea {
                buffer: buf,
                generation: 1,
                scale: scale_factor,
                color: err_color,
                scroll_x: 0.0,
                scroll_y: 0.0,
                local_left: err_local_left,
                local_top: err_local_top,
                clip_local_x: accum_clip.rect.x - x,
                clip_local_y: accum_clip.rect.y - y,
                clip_w: accum_clip.rect.width,
                clip_h: accum_clip.rect.height,
            });
        }
    }

    // Custom paint extension: third-party widgets can set element.paint_fn
    // to inject custom painting without forking paint_element_tree.
    if let Some(ref pf) = _element.paint_fn {
        let mut f = pf.borrow_mut();
        f(painter, Rect::new(x, y, w, h));
    }
}

pub(crate) fn paint_element_tree(
    fcx: &crate::core::frame_context::FrameContext,
    ct: &crate::ecs::tables::ComponentTables,
    arena: &mut ElementArena,
    eid: ElementId,
    painter: &mut Painter,
    text_areas: &mut Vec<crate::render::wgpu::glyphon_bridge::TextAreaDesc>,
    scale_factor: f32,
    scroll_offset: (f32, f32),
    parent_should_paint: bool,
    default_bg: Color,
    default_fg: Color,
    accum_clip: Rect,
    accum_clip_radius: crate::style::CornerRadii,
    accum_xform: Affine2,
    parent_floor: i32,
    must_paint: &std::collections::HashSet<ElementId>,
    accum_opacity: f32,
    highlight_mode: crate::event::FocusHighlightMode,
) {
    let Some(element) = arena.get(eid) else {
        return;
    };

    // Fast visibility check — pre-fetch lc before the full component batch below.
    let lc_comp = ct.lc.get(&eid);

    if element.slot_inactive.get()
        || !lc_comp
            .and_then(|l| l.reactive_visible.as_ref())
            .is_none_or(|v| v.get())
    {
        return;
    }

    let eff_floor = parent_floor.max(element.z_index_floor.unwrap_or(0));
    let mut ez = element.z_index.max(eff_floor);
    let should_paint = element.needs_repaint() || parent_should_paint;

    let sb = element.screen_bounds;
    let mut x = sb.x;
    let mut y = sb.y;
    let mut w = sb.width.max(1.0);
    let mut h = sb.height.max(1.0);

    // Pre-fetch all components once to avoid redundant HashMap lookups
    // throughout the paint path (audit 2026-07-17: ~72 → ~8 per element).
    let xform = ct.xform.get(&eid);
    let style_comp = ct.style.get(&eid);
    let text_comp = ct.text.get(&eid);
    let layout_comp = ct.layout.get(&eid);
    let scroll_comp = ct.scroll.get(&eid);
    let cursor_comp = ct.cursor.get(&eid);

    let element_xform_t = if let Some(ref t) = xform.and_then(|x| x.transform) {
        let tx = glam::Affine2::from_cols_array(t);
        let ox = xform.map_or(0.5, |x| x.transform_origin_x) * w;
        let oy = xform.map_or(0.5, |x| x.transform_origin_y) * h;
        let to_origin = glam::Affine2::from_translation(glam::Vec2::new(-(x + ox), -(y + oy)));
        let from_origin = glam::Affine2::from_translation(glam::Vec2::new(x + ox, y + oy));
        from_origin * tx * to_origin
    } else {
        glam::Affine2::IDENTITY
    };

    // Apply position_offset and size_scale BEFORE the cull so that the
    // raw_clip is computed from the correct visual rect.
    if let Some(ref off) = xform.map(|x| x.position_offset.clone()) {
        let o = off.get();
        x += o.x;
        y += o.y;
    }
    if let Some(ref sc) = xform.map(|x| x.size_scale.clone()) {
        let s = sc.get();
        w *= s.x;
        h *= s.y;
    }

    let element_xform = accum_xform * element_xform_t;

    // ── Early clip cull for leaves (audit 2026-07-17): compute raw_clip
    // before StyleComponent clone + resolve_style + opacity, saving ~40-60%
    // of the per-culled-leaf cost. Backdrop elements are never culled early
    // because their backdrop region must be emitted even when the element
    // rect is fully outside the clip.
    let raw_clip = intersect_rects(
        accum_clip,
        Rect::new(x - scroll_offset.0, y - scroll_offset.1, w, h),
    );
    let is_leaf = element.children.is_empty();
    if is_leaf && (raw_clip.width <= 0.0 || raw_clip.height <= 0.0) {
        // Backdrop-filter leaf whose rect is outside the clip still needs
        // its backdrop region emitted (it affects the visible area behind it).
        // This is extremely rare; pay the HashMap lookup rather than clone.
        if style_comp.and_then(|st| st.backdrop_filter).is_none() {
            if should_paint {
                element.clear_repaint();
            }
            return;
        }
        // Has backdrop_filter → fall through to the full path.
    }

    // ── Style borrow + resolve (only after cull check above; audit
    // 2026-07-17 follow-up: borrow instead of cloning the StyleComponent —
    // gradients/StateStyle carry heap data) ──
    let style_default;
    let s = match style_comp {
        Some(s) => s,
        None => {
            style_default = crate::ecs::components::StyleComponent::default();
            &style_default
        }
    };
    let resolved = resolve_style(element.state.get(), s);
    let opacity = resolved.opacity * accum_opacity;

    // Per-element outline z-offset (base style, matching the pre-audit gate).
    if s.outline_width > 0.0 {
        ez += crate::render::OUTLINE_Z_OFFSET;
    }

    // ── Backdrop-blur region (screen space) ──
    if let Some(bf) = resolved.backdrop_filter {
        painter
            .backdrop_regions
            .push(crate::render::BackdropRegion {
                rect: Rect::new(x, y, w, h),
                transform: element_xform,
                corner_radius: s.corners(),
                blur_radius: bf.blur_radius,
                tint: bf.tint,
                z_index: ez,
            });
    }

    let element_clip = ClipInfo::with_scroll(
        raw_clip,
        accum_clip_radius,
        [scroll_offset.0, scroll_offset.1],
    );

    // Cull non-leaf elements fully outside the viewport after scroll adjustment.
    // Never cull containers with children — their overflowing children may
    // extend into the visible area (e.g. Table body inside a scroll container).
    if !is_leaf && (raw_clip.width <= 0.0 || raw_clip.height <= 0.0) {
        // Container with all children outside clip: still paint it so that
        // its children are reached. The per-child clip will cull individual
        // leaf elements that are truly outside.
    } else if is_leaf && (raw_clip.width <= 0.0 || raw_clip.height <= 0.0) {
        if should_paint {
            element.clear_repaint();
        }
        return;
    }

    // Image / Progress / Slider
    let elem_cr = style_comp.map_or(4.0, |s| s.corner_radius);
    if elem_cr > 0.0 {
        // Override clip radius with element's own corner_radius so the
        // image is clipped to the element's shape (e.g. circular Avatar).
        let image_clip = ClipInfo::with_scroll(
            raw_clip,
            crate::style::CornerRadii::all(elem_cr),
            [scroll_offset.0, scroll_offset.1],
        );
        if let Some(img_data) = element.get_user_data::<crate::widgets::display::ImageData>() {
            let rect = Rect::new(x, y, w, h);
            painter.push_image(
                img_data.hash,
                rect,
                opacity,
                img_data.fit,
                image_clip,
                element_xform,
                ez,
            );
            if should_paint {
                element.clear_repaint();
            }
            return;
        }
    } else if let Some(img_data) = element.get_user_data::<crate::widgets::display::ImageData>() {
        let rect = Rect::new(x, y, w, h);
        painter.push_image(
            img_data.hash,
            rect,
            opacity,
            img_data.fit,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }
    if let Some(bd) = element.get_user_data::<crate::widgets::display::BarChartData>() {
        crate::widgets::display::bar_chart::paint_bar_chart(
            bd,
            painter,
            x,
            y,
            w,
            h,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }
    if let Some(ld) = element.get_user_data::<crate::widgets::display::LineChartData>() {
        crate::widgets::display::line_chart::paint_line_chart(
            ld,
            painter,
            x,
            y,
            w,
            h,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }
    if let Some(pd) = element.get_user_data::<crate::widgets::display::ProgressData>() {
        crate::widgets::display::progress::paint_progress(
            element,
            pd,
            painter,
            x,
            y,
            w,
            h,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }
    if let Some(sd) = element.get_user_data::<crate::widgets::input::SliderPaintData>() {
        crate::widgets::input::slider::paint_slider(
            sd,
            element,
            painter,
            x,
            y,
            w,
            h,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }
    if let Some(cpd) = element.get_user_data::<crate::widgets::input::ColorPlanePaintData>() {
        crate::widgets::input::color_picker::paint_color_plane(
            cpd,
            element,
            painter,
            x,
            y,
            w,
            h,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }
    if let Some(hpd) = element.get_user_data::<crate::widgets::input::HueBarPaintData>() {
        crate::widgets::input::color_picker::paint_hue_bar(
            hpd,
            element,
            painter,
            x,
            y,
            w,
            h,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }
    if let Some(apd) = element.get_user_data::<crate::widgets::input::AlphaBarPaintData>() {
        crate::widgets::input::color_picker::paint_alpha_bar(
            apd,
            element,
            painter,
            x,
            y,
            w,
            h,
            element_clip,
            element_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }

    // ── Path rendering (Icon, etc.) ──
    if let Some(ipd) = element.get_user_data::<crate::widgets::display::IconPathData>() {
        // Checkbox / switch icons may have dynamic visibility + brush colour.
        let brush: crate::style::Brush = if let Some(cis) =
            element.get_user_data::<Rc<crate::widgets::input::CheckboxIconState>>()
        {
            if !cis.visible.get() {
                if should_paint {
                    element.clear_repaint();
                }
                return;
            }
            crate::style::Brush::Solid(cis.brush_color.get())
        } else {
            ipd.brush.clone()
        };

        let scale = (w / 24.0).min(h / 24.0);
        let icon_clip = ClipInfo::with_scroll(
            raw_clip,
            crate::style::CornerRadii::ZERO,
            [scroll_offset.0, scroll_offset.1],
        );
        let path_xform = glam::Affine2::from_translation(glam::Vec2::new(x, y))
            * glam::Affine2::from_scale(glam::Vec2::new(scale, scale));
        let mut stroke = ipd.stroke.clone();
        stroke.width = (stroke.width * scale as f64).max(0.5);
        painter.stroke_path(
            &ipd.path,
            stroke,
            brush,
            icon_clip,
            element_xform * path_xform,
            ez,
        );
        if should_paint {
            element.clear_repaint();
        }
        return;
    }

    // ── Menu item icon (left of label text) ──
    if let Some(mi) = element.get_user_data::<crate::widgets::overlay::MenuItemIcon>() {
        let ix = x + 10.0;
        let iy = y + (h - mi.size) * 0.5;
        let ik = mi.size / 24.0;
        let path_xform = glam::Affine2::from_translation(glam::Vec2::new(ix, iy))
            * glam::Affine2::from_scale(glam::Vec2::new(ik, ik));
        let icon_clip = ClipInfo::with_scroll(
            raw_clip,
            crate::style::CornerRadii::ZERO,
            [scroll_offset.0, scroll_offset.1],
        );
        if mi.filled {
            painter.fill_path(
                &mi.path,
                crate::style::Brush::Solid(mi.color),
                icon_clip,
                element_xform * path_xform,
                ez,
            );
        } else {
            let mut stroke = kurbo::Stroke::new(1.5);
            stroke.join = kurbo::Join::Round;
            stroke.start_cap = kurbo::Cap::Round;
            stroke.end_cap = kurbo::Cap::Round;
            painter.stroke_path(
                &mi.path,
                stroke,
                crate::style::Brush::Solid(mi.color),
                icon_clip,
                element_xform * path_xform,
                ez,
            );
        }
    }

    // ── Submenu indicator (context menu arrow) ──
    if let Some(ind) = element.get_user_data::<crate::widgets::overlay::SubmenuIndicator>() {
        let indicator_size = ind.size;
        let ax = x + w - indicator_size - 8.0;
        let ay = y + (h - indicator_size * 0.8) * 0.5;
        let path_xform = glam::Affine2::from_translation(glam::Vec2::new(ax, ay))
            * glam::Affine2::from_scale(glam::Vec2::new(
                indicator_size / 5.0,
                indicator_size / 8.0,
            ));
        let arrow_clip = ClipInfo::with_scroll(
            raw_clip,
            crate::style::CornerRadii::ZERO,
            [scroll_offset.0, scroll_offset.1],
        );
        let fg = style_comp
            .and_then(|s| s.foreground)
            .unwrap_or(crate::style::Color::rgba8(200, 200, 210, 255));
        let mut stroke = kurbo::Stroke::new(1.5);
        stroke.join = kurbo::Join::Round;
        stroke.start_cap = kurbo::Cap::Round;
        stroke.end_cap = kurbo::Cap::Round;
        painter.stroke_path(
            &ind.path,
            stroke,
            crate::style::Brush::Solid(fg),
            arrow_clip,
            element_xform * path_xform,
            ez,
        );
    }

    // ── Lazy label buffer rebuild: text_generation vs buffer_gen ──
    let lbl = text_comp.and_then(|t| t.lazy_label.clone());
    let buf = text_comp.and_then(|t| t.text_buffer.clone());
    if let (Some(ref lbl), Some(ref buf)) = (&lbl, &buf) {
        let text_gen = text_comp.map_or(0, |t| t.text_generation.get());
        let buf_gen_val = text_comp.map_or(0, |t| t.buffer_gen.get());
        if text_gen != 0 && text_gen != buf_gen_val {
            let text = lbl.take();
            let text_str: &str = &text;
            let lazy_fp = text_comp.and_then(|t| t.lazy_font_params.clone());
            let font_family = text_comp.and_then(|t| t.font_family.clone());
            let (fs, lh, fw, ff, mut pw, ta) = if let Some(ref fp) = lazy_fp {
                (
                    fp.font_size,
                    fp.line_height,
                    fp.font_weight,
                    fp.font_family.as_deref(),
                    fp.max_width,
                    fp.text_align,
                )
            } else {
                (
                    text_comp.map_or(18.0, |t| t.font_size),
                    text_comp.map_or(1.5, |t| t.line_height),
                    text_comp.map_or(400, |t| t.font_weight),
                    font_family.as_deref(),
                    layout_comp.and_then(|l| l.preferred_width),
                    text_comp.map_or(crate::style::TextAlign::Start, |t| t.text_align),
                )
            };
            let tov = style_comp.map_or(crate::style::styled::TextOverflow::Clip, |s| {
                s.text_overflow
            });
            if tov == crate::style::styled::TextOverflow::Clip
                || tov == crate::style::styled::TextOverflow::Ellipsis
            {
                let pad = layout_comp.map_or(crate::style::Padding::ZERO, |l| l.padding);
                let cw = (element.screen_bounds.width - pad.left - pad.right).max(4.0);
                pw = Some(cw);
            }
            // Reuse the existing Buffer allocation and shape once
            // (audit round 3, ③) — replaces the old create_buffer path
            // that allocated a fresh Buffer and shaped twice.
            crate::render::wgpu::glyphon_bridge::reuse_buffer(
                &mut buf.borrow_mut(),
                text_str,
                fs,
                lh,
                fw,
                ff,
                pw,
                ta,
            );
            // Intrinsic width from the just-shaped buffer when it laid out
            // as a single Start/Left-aligned line — skips the SECOND full
            // shaping pass that measure_text_width would run (audit
            // 2026-07-17 round 2). Wrapped/multi-line/aligned text falls
            // back to the fresh unconstrained measure, preserving semantics.
            let single_line_w = if text_str.is_empty() {
                Some(0.0) // measure_text_width's empty-text semantics
            } else if matches!(
                ta,
                crate::style::TextAlign::Start | crate::style::TextAlign::Left
            ) {
                crate::render::wgpu::glyphon_bridge::intrinsic_width_from_buffer(&buf.borrow(), fs)
            } else {
                None
            };
            let measured = single_line_w.unwrap_or_else(|| {
                crate::render::text::measure_text_width(text_str, fs, fw, ff.map(|s| s.to_string()))
            });
            vgen!(
                "[VGEN:PAINT_BUILD] eid={:?} text_generation={} buffer_gen: {} -> {} width={:.1} text=\"{}\"",
                eid, text_gen, buf_gen_val, text_gen, pw.unwrap_or(0.0),
                text_str.chars().take(40).collect::<String>()
            );
            lbl.set(text);
            if let Some(ref mtw) = text_comp.map(|t| t.measured_text_width.clone()) {
                let new_w = measured.max(fs * 2.0);
                if (mtw.get() - new_w).abs() > 0.5 {
                    mtw.set(new_w);
                    element.mark_measure();
                }
            }
            if let Some(ref bg) = text_comp.map(|t| t.buffer_gen.clone()) {
                bg.set(text_gen);
            }
        }
    }

    #[cfg(debug_assertions)]
    {
        let dbg_lbl = text_comp.and_then(|t| t.lazy_label.clone());
        let dbg_buf = text_comp.and_then(|t| t.text_buffer.clone());
        if dbg_lbl.is_some() && dbg_buf.is_none() {
            panic!(
                "Element {:?} has lazy_label but no text_buffer — buffer will never be built",
                eid
            );
        }
    }

    let surf_gen = element.surface_gen.get();
    let text_gen = text_comp.map_or(0, |t| t.text_generation.get());
    let deco_gen = element.decor_gen.get();
    let needs_rp = element.needs_repaint();

    let (cached, surf_ok, text_ok, deco_ok) = {
        let c = fcx.scene_cache;
        let cache = c.borrow();
        if let Some(cs) = cache.get(&eid) {
            let scroll_ok = (cs.scroll_x - scroll_offset.0).abs() < 0.5
                && (cs.scroll_y - scroll_offset.1).abs() < 0.5;
            (
                Some(Rc::clone(cs)),
                cs.surface_gen == surf_gen && scroll_ok,
                cs.text_gen == text_gen && scroll_ok,
                cs.decor_gen == deco_gen && scroll_ok,
            )
        } else {
            (None, false, false, false)
        }
    };

    if !needs_rp && surf_ok && text_ok && deco_ok {
        let cs = cached.as_ref().unwrap();
        painter.replay_local_accum(
            &cs.local_items,
            x,
            y,
            ClipInfo::with_scroll(
                accum_clip,
                accum_clip_radius,
                [scroll_offset.0, scroll_offset.1],
            ),
            element_clip,
            element_xform,
            ez,
            opacity,
        );
        painter.replay(&cs.commands);
        for lta in &cs.local_text_areas {
            text_areas.push(lta.to_world_clipped(
                x,
                y,
                scroll_offset.0,
                scroll_offset.1,
                ez,
                eid,
                Some(element_clip.rect),
            ));
        }
    } else {
        #[cfg(debug_assertions)]
        if needs_rp && surf_ok && text_ok && deco_ok {
            let is_root = arena.root_id == Some(eid);
            if !is_root {
                let ctx = format!(
                    "role={:?} depth={} label={:?} dirty={:?} kids={} in_mustpaint={}",
                    ct.a11y.get(&eid).and_then(|a| a.accessible_role),
                    element.depth,
                    ct.a11y.get(&eid).and_then(|a| a.accessible_label.clone()),
                    element.dirty.get(),
                    element.children.len(),
                    must_paint.contains(&eid),
                );
                crate::debug::check_over_render(eid, surf_gen, deco_gen, &ctx);
            }
        }

        let redo_surface = !surf_ok || cached.is_none() || needs_rp;
        let redo_text = needs_rp || !text_ok || cached.is_none();
        let redo_decor = needs_rp || !deco_ok || cached.is_none();

        let items_start = painter.local_items_len();
        let cmd_start = painter.snapshot_len();

        if redo_surface {
            paint_element_surface(
                element,
                style_comp,
                lc_comp,
                painter,
                x,
                y,
                w,
                h,
                element_clip,
                element_xform,
                ez,
                accum_clip.into(),
                opacity,
                default_bg,
                highlight_mode,
            );
        }
        let surface_items = if redo_surface {
            painter.drain_local_items_from(items_start)
        } else {
            cached.as_ref().unwrap().local_items.clone()
        };
        let surface_cmds: Vec<crate::render::DrawCommand> = if redo_surface {
            painter.drain_from(cmd_start)
        } else {
            let cs = cached.as_ref().unwrap();
            cs.commands[..cs.decor_start].to_vec()
        };

        let mut main_ta: Vec<crate::render::LocalTextArea> = Vec::new();
        if redo_text {
            record_element_text(
                element,
                text_comp,
                style_comp,
                layout_comp,
                scroll_comp,
                &mut main_ta,
                x,
                y,
                w,
                h,
                scale_factor,
                default_fg,
            );
        } else {
            let cs = cached.as_ref().unwrap();
            main_ta = cs.local_text_areas[..cs.decor_text_start].to_vec();
        }

        let deco_cmd_start = painter.snapshot_len();
        let mut decor_ta: Vec<crate::render::LocalTextArea> = Vec::new();
        if redo_decor && should_paint {
            paint_element_decor(
                element,
                text_comp,
                style_comp,
                layout_comp,
                scroll_comp,
                cursor_comp,
                lc_comp,
                painter,
                &mut decor_ta,
                x,
                y,
                w,
                h,
                element_clip,
                element_xform,
                ez,
                accum_clip.into(),
                opacity,
                default_fg,
                scale_factor,
            );
        } else if let Some(cs) = &cached {
            decor_ta = cs.local_text_areas[cs.decor_text_start..].to_vec();
        }
        let decor_cmds: Vec<crate::render::DrawCommand> = if redo_decor {
            painter.drain_from(deco_cmd_start)
        } else {
            let cs = cached.as_ref().unwrap();
            cs.commands[cs.decor_start..].to_vec()
        };

        let mut all_commands = surface_cmds;
        let new_decor_start = all_commands.len();
        all_commands.extend(decor_cmds);

        let mut local_ta = main_ta;
        let decor_text_start = local_ta.len();
        local_ta.extend(decor_ta);

        {
            let c = fcx.scene_cache;
            let cs = Rc::new(crate::render::CachedScene {
                local_items: surface_items,
                commands: all_commands,
                local_text_areas: local_ta,
                surface_gen: surf_gen,
                text_gen: text_gen,
                decor_gen: deco_gen,
                decor_start: new_decor_start,
                decor_text_start,
                scroll_x: scroll_offset.0,
                scroll_y: scroll_offset.1,
            });
            c.borrow_mut().insert(eid, Rc::clone(&cs));

            painter.replay_local_accum(
                &cs.local_items,
                x,
                y,
                ClipInfo::with_scroll(
                    accum_clip,
                    accum_clip_radius,
                    [scroll_offset.0, scroll_offset.1],
                ),
                element_clip,
                element_xform,
                ez,
                opacity,
            );
            painter.replay(&cs.commands);
            for lta in &cs.local_text_areas {
                text_areas.push(lta.to_world_clipped(
                    x,
                    y,
                    scroll_offset.0,
                    scroll_offset.1,
                    ez,
                    eid,
                    Some(element_clip.rect),
                ));
            }
        }
    }

    // ── Children ──
    // Extract all needed data before dropping element for arena recursion
    let is_scrollable = layout_comp.is_some_and(|l| {
        l.overflow == crate::core::config::Overflow::Scroll
            || l.overflow == crate::core::config::Overflow::Clip
    });
    let so_x = scroll_comp
        .as_ref()
        .map_or(0.0, |s| s.scroll_offset.get().x);
    let so_y = scroll_comp
        .as_ref()
        .map_or(0.0, |s| s.scroll_offset.get().y);
    let children_clone = element.children.clone();
    let overflow = layout_comp.map_or(crate::core::config::Overflow::Visible, |l| l.overflow);
    let scrollbar_policy = layout_comp.map_or(crate::core::config::ScrollbarPolicy::Auto, |l| {
        l.scrollbar_policy
    });
    let sb_w = layout_comp.map_or(10.0, |l| l.scrollbar_width);
    let leaf_content_bounds = scroll_comp.map(|sc| sc.content_bounds.get());
    let _ = element;

    if is_scrollable {
        let mut cb_w = w;
        let mut cb_h = h;
        // If the scrollable element stores List item IDs (via ListItemIds
        // user_data), compute content bounds from the actual items rather
        // than from the clip child — this supports clips that have
        // affected_by_child_size(false) for full-width item stretching.
        let list_items = arena
            .get(eid)
            .and_then(|el| el.get_user_data::<crate::widgets::display::list::ListItemIds>());
        let virtual_cb = arena
            .get(eid)
            .and_then(|el| {
                el.get_user_data::<crate::widgets::display::list::VirtualContentBounds>()
            })
            .map(|vcb| vcb.0.get());
        if let Some(vcb) = virtual_cb {
            cb_w = vcb.width.max(cb_w);
            cb_h = vcb.height.max(cb_h);
        } else if let Some(ids) = list_items {
            // screen_bounds from taffy is in document-root space. Convert
            // to container-local by subtracting the container's position,
            // otherwise parent-chain offsets inflate content_bounds.
            for &iid in &ids.0 {
                if let Some(item) = arena.get(iid) {
                    let ir = item.screen_bounds;
                    let local_x = ir.x - x;
                    let local_y = ir.y - y;
                    let right = local_x + ir.width;
                    let bottom = local_y + ir.height;
                    if right > cb_w {
                        cb_w = right;
                    }
                    if bottom > cb_h {
                        cb_h = bottom;
                    }
                }
            }
        } else {
            for &cid in &children_clone {
                if let Some(child) = arena.get(cid) {
                    let cr = child.screen_bounds;
                    let right = cr.x + cr.width;
                    let bottom = cr.y + cr.height;
                    if right > cb_w {
                        cb_w = right;
                    }
                    if bottom > cb_h {
                        cb_h = bottom;
                    }
                }
            }
            cb_w -= x;
            cb_h -= y;
        }
        // Leaf widgets (e.g. multiline TextInput) set content_bounds in local
        // space via ECS — apply after children's coordinate conversion.
        if let Some(b) = leaf_content_bounds {
            cb_w = cb_w.max(b.width);
            cb_h = cb_h.max(b.height);
        }
        let has_v = cb_h > h;
        let has_h = cb_w > w;

        let gutter_w = if has_v { sb_w + 2.0 } else { 0.0 };
        let gutter_h = if has_h { sb_w + 2.0 } else { 0.0 };
        let sx = x - scroll_offset.0;
        let sy = y - scroll_offset.1;
        let content_clip = Rect::new(sx, sy, (w - gutter_w).max(1.0), (h - gutter_h).max(1.0));

        let scroll_xform_t = glam::Affine2::from_translation(glam::Vec2::new(-so_x, -so_y));
        paint_children_sorted(
            fcx,
            ct,
            arena,
            eid,
            painter,
            text_areas,
            scale_factor,
            (scroll_offset.0 + so_x, scroll_offset.1 + so_y),
            should_paint,
            default_bg,
            default_fg,
            content_clip,
            style_comp.map_or(crate::style::CornerRadii::all(4.0), |s| s.corners()),
            element_xform * scroll_xform_t,
            eff_floor,
            must_paint,
            opacity,
            highlight_mode,
        );

        // Scrollbars — use unadjusted x,y because element_xform already
        // includes the accumulated scroll transform from ancestors.
        let Some(element) = arena.get(eid) else {
            return;
        };
        let show_bars = scrollbar_policy != crate::core::config::ScrollbarPolicy::Never;
        if show_bars && has_v {
            let thumb_h = (h / cb_h * h).max(20.0);
            let thumb_y = y + (so_y / (cb_h - h)) * (h - thumb_h);
            let thumb_rect = Rect::new(x + w - sb_w - 2.0, thumb_y, sb_w, thumb_h);
            painter.fill_rect(
                Rect::new(x + w - sb_w - 2.0, y, sb_w, h),
                Color::rgba8(60, 60, 70, 60),
                element_clip,
                element_xform,
                ez,
            );
            painter.fill_rounded_rect(
                thumb_rect,
                Color::rgba8(100, 100, 120, 255),
                crate::style::CornerRadii::all(sb_w * 0.5),
                element_clip,
                element_xform,
                ez,
            );
        }
        if show_bars && has_h {
            let h_gutter = if has_v { sb_w + 2.0 } else { 0.0 };
            let thumb_w = (w / cb_w * w).max(20.0);
            let thumb_x = x + (so_x / (cb_w - w)) * (w - thumb_w - h_gutter);
            let thumb_rect = Rect::new(thumb_x, y + h - sb_w - 2.0, thumb_w, sb_w);
            painter.fill_rect(
                Rect::new(x, y + h - sb_w - 2.0, w - h_gutter, sb_w),
                Color::rgba8(60, 60, 70, 60),
                element_clip,
                element_xform,
                ez,
            );
            painter.fill_rounded_rect(
                thumb_rect,
                Color::rgba8(100, 100, 120, 255),
                crate::style::CornerRadii::all(sb_w * 0.5),
                element_clip,
                element_xform,
                ez,
            );
        }

        if let Some(sc) = scroll_comp {
            let cb_cell = &sc.content_bounds;
            let old = cb_cell.get();
            // VirtualContentBounds is the authoritative source (set by list/table frame_tick);
            // always sync to it so the scrollbar shrinks when data is removed.
            if virtual_cb.is_some() {
                if (cb_h - old.height).abs() > 0.01 || (cb_w - old.width).abs() > 0.01 {
                    cb_cell.set(Rect::new(0.0, 0.0, cb_w, cb_h));
                }
            } else if cb_h > old.height || cb_w > old.width {
                // Non-virtual: only grow — paint-computed values may be
                // unreliable (coordinate-space issues in nested scroll).
                cb_cell.set(Rect::new(
                    0.0,
                    0.0,
                    cb_w.max(old.width),
                    cb_h.max(old.height),
                ));
            }
        }
        let _ = element;
    } else {
        let child_clip = if overflow == crate::core::config::Overflow::Visible {
            accum_clip
        } else {
            element_clip.rect
        };
        let child_clip_radius = if overflow == crate::core::config::Overflow::Visible {
            accum_clip_radius
        } else {
            style_comp.map_or(crate::style::CornerRadii::all(4.0), |s| s.corners())
        };
        paint_children_sorted(
            fcx,
            ct,
            arena,
            eid,
            painter,
            text_areas,
            scale_factor,
            scroll_offset,
            should_paint,
            default_bg,
            default_fg,
            child_clip,
            child_clip_radius,
            element_xform,
            eff_floor,
            must_paint,
            opacity,
            highlight_mode,
        );
    }

    if should_paint {
        if let Some(element) = arena.get(eid) {
            element.clear_repaint();
        }
    }
}
