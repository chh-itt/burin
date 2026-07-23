//! Shared path utilities for both GPU and CPU backends.
//!
//! - `kurbo::BezPath` ↔ `lyon_path::Path` conversion
//! - `kurbo::BezPath` ↔ `tiny_skia::Path` conversion
//! - BezPath content hashing (for cache keys)
//! - Minimal SVG path data parser (M/L/C/Q/Z/A subset)

use kurbo::{BezPath, PathEl, Point, Shape};

/// ── Hash ──

/// Compute a 64-bit hash of a BezPath's content for cache keying.
pub fn hash_bezpath(path: &BezPath) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                0u64.hash(&mut h);
                p.x.to_bits().hash(&mut h);
                p.y.to_bits().hash(&mut h);
            }
            PathEl::LineTo(p) => {
                1u64.hash(&mut h);
                p.x.to_bits().hash(&mut h);
                p.y.to_bits().hash(&mut h);
            }
            PathEl::QuadTo(p1, p2) => {
                2u64.hash(&mut h);
                p1.x.to_bits().hash(&mut h);
                p1.y.to_bits().hash(&mut h);
                p2.x.to_bits().hash(&mut h);
                p2.y.to_bits().hash(&mut h);
            }
            PathEl::CurveTo(p1, p2, p3) => {
                3u64.hash(&mut h);
                p1.x.to_bits().hash(&mut h);
                p1.y.to_bits().hash(&mut h);
                p2.x.to_bits().hash(&mut h);
                p2.y.to_bits().hash(&mut h);
                p3.x.to_bits().hash(&mut h);
                p3.y.to_bits().hash(&mut h);
            }
            PathEl::ClosePath => {
                4u64.hash(&mut h);
            }
        }
    }
    h.finish()
}

/// ── kurbo → lyon ──

#[cfg(feature = "lyon")]
pub fn bezpath_to_lyon(path: &BezPath) -> lyon::path::Path {
    let mut builder = lyon::path::Path::builder().with_svg();
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                builder.move_to(lyon::math::point(p.x as f32, p.y as f32));
            }
            PathEl::LineTo(p) => {
                builder.line_to(lyon::math::point(p.x as f32, p.y as f32));
            }
            PathEl::QuadTo(p1, p2) => {
                builder.quadratic_bezier_to(
                    lyon::math::point(p1.x as f32, p1.y as f32),
                    lyon::math::point(p2.x as f32, p2.y as f32),
                );
            }
            PathEl::CurveTo(p1, p2, p3) => {
                builder.cubic_bezier_to(
                    lyon::math::point(p1.x as f32, p1.y as f32),
                    lyon::math::point(p2.x as f32, p2.y as f32),
                    lyon::math::point(p3.x as f32, p3.y as f32),
                );
            }
            PathEl::ClosePath => {
                builder.close();
            }
        }
    }
    builder.build()
}

/// ── kurbo → tiny_skia ──

#[cfg(feature = "backend-tiny-skia")]
pub fn bezpath_to_tiny_skia(path: &BezPath) -> Option<tiny_skia::Path> {
    let mut builder = tiny_skia::PathBuilder::new();
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => builder.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => builder.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(p1, p2) => {
                builder.quad_to(p1.x as f32, p1.y as f32, p2.x as f32, p2.y as f32);
            }
            PathEl::CurveTo(p1, p2, p3) => {
                builder.cubic_to(
                    p1.x as f32,
                    p1.y as f32,
                    p2.x as f32,
                    p2.y as f32,
                    p3.x as f32,
                    p3.y as f32,
                );
            }
            PathEl::ClosePath => builder.close(),
        }
    }
    builder.finish()
}

/// ── BezPath bounding box (as style::Rect) ──

pub fn bezpath_bounds(path: &BezPath) -> Option<crate::style::Rect> {
    let bbox = path.bounding_box();
    if bbox.x0.is_infinite() {
        return None;
    }
    Some(crate::style::Rect::new(
        bbox.x0 as f32,
        bbox.y0 as f32,
        (bbox.x1 - bbox.x0) as f32,
        (bbox.y1 - bbox.y0) as f32,
    ))
}

/// ── Minimal SVG path data parser ──
///
/// Parses SVG path `d` attribute commands: M, L, C, Q, Z, A, H, V.
/// A (arc) is decomposed via kurbo's arc-to-cubic helper.
/// Both absolute and relative variants supported.

/// Parse SVG path data string into a kurbo::BezPath.
pub fn parse_svg_path(d: &str) -> Result<BezPath, String> {
    if d.is_empty() {
        return Err("empty SVG path data".into());
    }
    let mut path = BezPath::new();
    let tokens = tokenize_svg_path(d);
    let mut i = 0;

    let mut current = Point::new(0.0, 0.0);
    let mut start = Point::new(0.0, 0.0);
    let mut last_cmd = String::new();
    let mut relative = false;

    while i < tokens.len() {
        let tok = &tokens[i];
        if let Some(ch) = tok.chars().next() {
            if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
                let raw = &tokens[i];
                i += 1;
                relative = raw.chars().next().unwrap().is_ascii_lowercase();
                last_cmd = raw.to_ascii_uppercase();
            }
        }
        if last_cmd.is_empty() {
            return Err("SVG path must start with a command (M/m)".into());
        }

        match last_cmd.as_str() {
            "M" | "m" => {
                let (vals, consumed) = read_numbers(&tokens, i, 2);
                if consumed < 2 {
                    return Err("M needs 2 coords".into());
                }
                i += consumed;
                let p = make_pt(vals[0], vals[1], relative, current);
                path.move_to(p);
                start = p;
                current = p;
            }
            "L" | "l" => {
                let (vals, consumed) = read_numbers(&tokens, i, 2);
                if consumed < 2 {
                    return Err("L needs 2 coords".into());
                }
                i += consumed;
                let p = make_pt(vals[0], vals[1], relative, current);
                path.line_to(p);
                current = p;
            }
            "H" | "h" => {
                let (vals, consumed) = read_numbers(&tokens, i, 1);
                if consumed < 1 {
                    return Err("H needs 1 coord".into());
                }
                i += consumed;
                let x = if relative {
                    current.x + vals[0]
                } else {
                    vals[0]
                };
                let p = Point::new(x, current.y);
                path.line_to(p);
                current = p;
            }
            "V" | "v" => {
                let (vals, consumed) = read_numbers(&tokens, i, 1);
                if consumed < 1 {
                    return Err("V needs 1 coord".into());
                }
                i += consumed;
                let y = if relative {
                    current.y + vals[0]
                } else {
                    vals[0]
                };
                let p = Point::new(current.x, y);
                path.line_to(p);
                current = p;
            }
            "C" | "c" => {
                let (vals, consumed) = read_numbers(&tokens, i, 6);
                if consumed < 6 {
                    return Err("C needs 6 coords".into());
                }
                i += consumed;
                let p1 = make_pt(vals[0], vals[1], relative, current);
                let p2 = make_pt(vals[2], vals[3], relative, current);
                let p = make_pt(vals[4], vals[5], relative, current);
                path.curve_to(p1, p2, p);
                current = p;
            }
            "Q" | "q" => {
                let (vals, consumed) = read_numbers(&tokens, i, 4);
                if consumed < 4 {
                    return Err("Q needs 4 coords".into());
                }
                i += consumed;
                let p1 = make_pt(vals[0], vals[1], relative, current);
                let p = make_pt(vals[2], vals[3], relative, current);
                path.quad_to(p1, p);
                current = p;
            }
            "A" | "a" => {
                let (vals, consumed) = read_numbers(&tokens, i, 7);
                if vals.len() < 7 {
                    return Err("A needs 7 params".into());
                }
                i += consumed;
                let rx = vals[0].abs();
                let ry = vals[1].abs();
                let x_rot = vals[2];
                let large_flag = vals[3] != 0.0;
                let sweep_flag = vals[4] != 0.0;
                let target = make_pt(vals[5], vals[6], relative, current);
                if rx < 0.001 || ry < 0.001 {
                    path.line_to(target);
                } else {
                    let arc = kurbo::SvgArc {
                        from: current,
                        to: target,
                        radii: kurbo::Vec2::new(rx, ry),
                        x_rotation: x_rot.to_radians(),
                        large_arc: large_flag,
                        sweep: sweep_flag,
                    };
                    if let Some(a) = kurbo::Arc::from_svg_arc(&arc) {
                        a.to_cubic_beziers(0.001, |p1, p2, p3| {
                            path.curve_to(p1, p2, p3);
                        });
                    } else {
                        path.line_to(target);
                    }
                }
                current = target;
            }
            "Z" | "z" => {
                path.close_path();
                current = start;
            }
            _ => return Err(format!("unknown SVG command: {}", last_cmd)),
        }
    }

    Ok(path)
}

fn make_pt(x: f64, y: f64, relative: bool, current: Point) -> Point {
    if relative {
        Point::new(current.x + x, current.y + y)
    } else {
        Point::new(x, y)
    }
}

fn read_numbers(tokens: &[String], start: usize, needed: usize) -> (Vec<f64>, usize) {
    let mut vals = Vec::new();
    let mut i = start;
    while vals.len() < needed && i < tokens.len() {
        let tok = &tokens[i];
        let ch = tok.chars().next().unwrap_or(' ');
        if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
            break;
        }
        if let Ok(v) = tok.parse::<f64>() {
            vals.push(v);
            i += 1;
        } else {
            break;
        }
    }
    // Last resort: if still short, try splitting multi-digit all-digit tokens.
    // This handles SVG minifier artifacts where "00" should be "0 0", etc.
    // Only activates when normal parsing can't get enough values.
    if vals.len() < needed && !vals.is_empty() {
        let consumed = i - start;
        let mut split_vals: Vec<f64> = Vec::with_capacity(needed);
        for idx in 0..consumed {
            let tok = &tokens[start + idx];
            if tok.chars().all(|c| c.is_ascii_digit())
                && tok.len() > 1
                && split_vals.len() + tok.len() <= needed
            {
                for c in tok.chars() {
                    if let Some(d) = c.to_digit(10) {
                        split_vals.push(d as f64);
                    }
                }
            } else if let Ok(v) = tok.parse::<f64>() {
                split_vals.push(v);
            }
        }
        if split_vals.len() == needed {
            return (split_vals, consumed);
        }
    }
    (vals, i - start)
}

fn tokenize_svg_path(d: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut curr = String::new();
    let mut chars = d.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
            if !curr.is_empty() {
                tokens.push(std::mem::take(&mut curr));
            }
            tokens.push(ch.to_string());
        } else if ch == '-' {
            // SVG allows "M10-20" as shorthand for "M 10 -20"
            if !curr.is_empty() {
                tokens.push(std::mem::take(&mut curr));
            }
            curr.push(ch);
        } else if ch == '.' {
            // SVG allows "M10.5.5" → "10.5 0.5": a '.' starts a new number
            // if the current token already contains a '.'
            if curr.contains('.') {
                tokens.push(std::mem::take(&mut curr));
                curr.push('0');
            }
            curr.push(ch);
        } else if ch.is_ascii_digit() {
            curr.push(ch);
        } else if ch == '+' && curr.is_empty() {
            curr.push(ch);
        } else if (ch == 'e' || ch == 'E') && !curr.is_empty() {
            curr.push(ch);
            // After 'e'/'E', consume optional '+' or '-' sign
            if let Some(&next) = chars.peek() {
                if next == '-' || next == '+' {
                    curr.push(chars.next().unwrap());
                }
            }
        } else {
            if !curr.is_empty() {
                tokens.push(std::mem::take(&mut curr));
            }
        }
    }
    if !curr.is_empty() {
        tokens.push(curr);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_line_path() {
        let path = parse_svg_path("M10 20L30 40").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 2);
        match els[0] {
            PathEl::MoveTo(p) => {
                assert_eq!(p.x, 10.0);
                assert_eq!(p.y, 20.0);
            }
            _ => panic!("expected MoveTo"),
        }
        match els[1] {
            PathEl::LineTo(p) => {
                assert_eq!(p.x, 30.0);
                assert_eq!(p.y, 40.0);
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn test_close_path() {
        let path = parse_svg_path("M0 0L10 0L10 10Z").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 4);
        assert_eq!(*els[3], PathEl::ClosePath);
    }

    #[test]
    fn test_bezpath_hash_deterministic() {
        let p1 = parse_svg_path("M0 0L10 10").unwrap();
        let p2 = parse_svg_path("M0 0L10 10").unwrap();
        assert_eq!(hash_bezpath(&p1), hash_bezpath(&p2));
    }

    #[test]
    fn test_bezpath_hash_different() {
        let p1 = parse_svg_path("M0 0L10 10").unwrap();
        let p2 = parse_svg_path("M0 0L20 20").unwrap();
        assert_ne!(hash_bezpath(&p1), hash_bezpath(&p2));
    }

    #[test]
    fn test_arc_to_cubic() {
        // A rx ry x_rot large sweep x y
        // A simple arc: from (10,10) to (20,10) with r=5 should produce bezier segments
        let path = parse_svg_path("M10 10A5 5 0 0 1 20 10").unwrap();
        assert!(
            path.elements().len() >= 2,
            "arc should produce at least a curve"
        );
        // Should have at least one CurveTo
        let has_curve = path
            .elements()
            .iter()
            .any(|el| matches!(el, PathEl::CurveTo(..)));
        assert!(has_curve, "arc should be decomposed into cubic beziers");
    }

    #[test]
    fn test_relative_commands() {
        let path = parse_svg_path("M10 10l5 5").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 2);
        match &els[1] {
            PathEl::LineTo(p) => {
                assert_eq!(p.x, 15.0);
                assert_eq!(p.y, 15.0);
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn test_cubic_bezier() {
        let path = parse_svg_path("M0 0C10 10 20 10 30 0").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 2);
        match &els[1] {
            PathEl::CurveTo(..) => {}
            _ => panic!("expected CurveTo"),
        }
    }

    #[test]
    fn test_bezpath_bounds() {
        let path = parse_svg_path("M0 0L10 0L10 10L0 10Z").unwrap();
        let bounds = bezpath_bounds(&path).unwrap();
        assert!((bounds.x - 0.0).abs() < 0.001);
        assert!((bounds.y - 0.0).abs() < 0.001);
        assert!((bounds.width - 10.0).abs() < 0.001);
        assert!((bounds.height - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_minus_as_separator() {
        // "M10-20" is shorthand for "M 10 -20" in SVG
        let path = parse_svg_path("M10-20").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 1);
        match &els[0] {
            PathEl::MoveTo(p) => {
                assert_eq!(p.x, 10.0);
                assert_eq!(p.y, -20.0);
            }
            _ => panic!("expected MoveTo"),
        }
    }

    #[test]
    fn test_scientific_notation() {
        // "M1e2 20" is shorthand for M 100 20
        let path = parse_svg_path("M1e2 20").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        match &els[0] {
            PathEl::MoveTo(p) => {
                assert!((p.x - 100.0).abs() < 0.001);
            }
            _ => panic!("expected MoveTo"),
        }
    }

    #[test]
    fn test_comma_separated() {
        // Comma-separated values are valid in SVG
        let path = parse_svg_path("M10,20L30,40").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 2);
        match &els[0] {
            PathEl::MoveTo(p) => {
                assert_eq!(p.x, 10.0);
                assert_eq!(p.y, 20.0);
            }
            _ => panic!("expected MoveTo"),
        }
        match &els[1] {
            PathEl::LineTo(p) => {
                assert_eq!(p.x, 30.0);
                assert_eq!(p.y, 40.0);
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn test_horizontal_vertical() {
        // H and V commands
        let path = parse_svg_path("M0 0H10V10").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 3);
        match &els[1] {
            PathEl::LineTo(p) => {
                assert_eq!(p.x, 10.0);
                assert_eq!(p.y, 0.0);
            }
            _ => panic!("expected LineTo from H"),
        }
        match &els[2] {
            PathEl::LineTo(p) => {
                assert_eq!(p.x, 10.0);
                assert_eq!(p.y, 10.0);
            }
            _ => panic!("expected LineTo from V"),
        }
    }

    #[test]
    fn test_quadratic_bezier() {
        let path = parse_svg_path("M0 0Q10 10 20 0").unwrap();
        let els: Vec<_> = path.elements().iter().collect();
        assert_eq!(els.len(), 2);
        match &els[1] {
            PathEl::QuadTo(..) => {}
            _ => panic!("expected QuadTo"),
        }
    }

    #[test]
    fn test_empty_path() {
        let result = parse_svg_path("");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_on_unknown_command() {
        let result = parse_svg_path("M0 0X10 10");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_on_incomplete_coords() {
        let result = parse_svg_path("M10");
        assert!(result.is_err());
    }

    #[cfg(feature = "lyon")]
    #[test]
    fn test_all_icons_to_lyon() {
        use crate::resource::icons::Icon;
        let icons = [
            Icon::Check,
            Icon::X,
            Icon::Plus,
            Icon::Minus,
            Icon::Search,
            Icon::ArrowRight,
            Icon::ArrowLeft,
            Icon::ArrowUp,
            Icon::ArrowDown,
            Icon::Save,
            Icon::Delete,
            Icon::Edit,
            Icon::Home,
            Icon::User,
            Icon::Settings,
            Icon::Folder,
            Icon::File,
            Icon::Image,
            Icon::Menu,
            Icon::Refresh,
            Icon::Mail,
            Icon::MessageCircle,
            Icon::Phone,
            Icon::Link,
            Icon::AlertCircle,
            Icon::Info,
            Icon::Play,
            Icon::Pause,
            Icon::Volume,
            Icon::Filter,
        ];
        for icon in &icons {
            let d = icon.path_data();
            if d.is_empty() {
                continue;
            }
            let bp = parse_svg_path(d).unwrap();
            let _lyon = bezpath_to_lyon(&bp); // should not panic
        }
    }

    #[test]
    fn test_all_icons_parse() {
        use crate::resource::icons::Icon;
        let icons = [
            Icon::Check,
            Icon::X,
            Icon::Plus,
            Icon::Minus,
            Icon::Search,
            Icon::ArrowRight,
            Icon::ArrowLeft,
            Icon::ArrowUp,
            Icon::ArrowDown,
            Icon::Save,
            Icon::Delete,
            Icon::Edit,
            Icon::Home,
            Icon::User,
            Icon::Settings,
            Icon::Folder,
            Icon::File,
            Icon::Image,
            Icon::Menu,
            Icon::Refresh,
            Icon::Mail,
            Icon::MessageCircle,
            Icon::Phone,
            Icon::Link,
            Icon::AlertCircle,
            Icon::Info,
            Icon::Play,
            Icon::Pause,
            Icon::Volume,
            Icon::Filter,
        ];
        for icon in &icons {
            let d = icon.path_data();
            if d.is_empty() {
                continue;
            }
            let result = parse_svg_path(d);
            assert!(
                result.is_ok(),
                "icon {:?} failed: {:?}, path_data={:?}",
                icon,
                result,
                d
            );
        }
    }

    #[cfg(feature = "lyon")]
    #[test]
    fn test_stroke_tessellate_check_icon() {
        use crate::resource::icons::Icon;
        let bp = Icon::Check.build_path().unwrap();
        let lyon_p = bezpath_to_lyon(&bp);

        // Same stroke params as the Icon widget uses
        let stroke = kurbo::Stroke {
            width: 2.0,
            start_cap: kurbo::Cap::Round,
            end_cap: kurbo::Cap::Round,
            join: kurbo::Join::Round,
            ..Default::default()
        };

        let mut buffers = lyon::tessellation::VertexBuffers::<[f32; 2], u32>::new();
        let options = lyon::tessellation::StrokeOptions::default()
            .with_line_width(stroke.width as f32)
            .with_line_cap(lyon::tessellation::LineCap::Round)
            .with_line_join(lyon::tessellation::LineJoin::Round);

        let result = lyon::tessellation::StrokeTessellator::new().tessellate_path(
            &lyon_p,
            &options,
            &mut lyon::tessellation::BuffersBuilder::new(
                &mut buffers,
                |v: lyon::tessellation::StrokeVertex| {
                    let pos = v.position();
                    [pos.x, pos.y]
                },
            ),
        );
        assert!(result.is_ok(), "stroke tessellation failed: {:?}", result);
        let vert_count = buffers.vertices.len();
        let idx_count = buffers.indices.len();
        eprintln!("Check icon: {} vertices, {} indices", vert_count, idx_count);
        assert!(
            vert_count >= 6,
            "stroke should produce at least 6 vertices (got {})",
            vert_count
        );
        assert!(
            idx_count >= 6,
            "stroke should produce at least 6 indices (got {})",
            idx_count
        );
    }

    #[cfg(feature = "lyon")]
    #[test]
    fn test_stroke_tessellate_all_icons() {
        use crate::resource::icons::Icon;
        let icons = [
            Icon::Check,
            Icon::X,
            Icon::Plus,
            Icon::Minus,
            Icon::Search,
            Icon::ArrowRight,
            Icon::ArrowLeft,
            Icon::ArrowUp,
            Icon::ArrowDown,
            Icon::Save,
            Icon::Delete,
            Icon::Edit,
            Icon::Home,
            Icon::User,
            Icon::Settings,
            Icon::Folder,
            Icon::File,
            Icon::Image,
            Icon::Menu,
            Icon::Refresh,
            Icon::Mail,
        ];
        for icon in &icons {
            let Some(bp) = icon.build_path() else {
                continue;
            };
            let lyon_p = bezpath_to_lyon(&bp);
            let mut buffers = lyon::tessellation::VertexBuffers::<[f32; 2], u32>::new();
            let options = lyon::tessellation::StrokeOptions::default()
                .with_line_width(2.0)
                .with_line_cap(lyon::tessellation::LineCap::Round)
                .with_line_join(lyon::tessellation::LineJoin::Round);
            let result = lyon::tessellation::StrokeTessellator::new().tessellate_path(
                &lyon_p,
                &options,
                &mut lyon::tessellation::BuffersBuilder::new(
                    &mut buffers,
                    |v: lyon::tessellation::StrokeVertex| [v.position().x, v.position().y],
                ),
            );
            assert!(result.is_ok(), "icon {:?} stroke tessellation failed", icon);
            assert!(
                buffers.vertices.len() >= 4,
                "icon {:?} produced only {} vertices",
                icon,
                buffers.vertices.len()
            );
        }
    }

    #[test]
    fn test_search_icon_debug() {
        let bp = crate::resource::icons::Icon::Search.build_path().unwrap();
        let elements = bp.elements();
        eprintln!("Search icon has {} path elements:", elements.len());
        for (i, el) in elements.iter().enumerate() {
            eprintln!("  {}: {:?}", i, el);
        }
        // Should have: MoveTo + CurveTo* + CurveTo* + ClosePath + MoveTo + LineTo
        let curve_count = elements
            .iter()
            .filter(|el| matches!(el, PathEl::CurveTo(..)))
            .count();
        assert!(
            curve_count >= 2,
            "Search should have at least 2 CurveTo from arc decomposition, got {}",
            curve_count
        );
    }
}
