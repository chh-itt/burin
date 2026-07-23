//! Probe: cost of one lyon stroke tessellation of typical Lucide icon
//! paths — the WGPU backend re-pays this on every path-cache MISS.
//!
//! The path cache key includes `hash_transform(full Affine2)`
//! (src/render/wgpu/mod.rs:992), so any scrolled icon (translation
//! changes every frame) misses the cache and re-tessellates.
//!
//! Run: cargo test --profile bench --test icon_tessellation_probe --features lyon -- --ignored --nocapture --test-threads 1

use std::time::Instant;

use lyon::math::point;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};

// Same conversion approach as render/wgpu/mod.rs::bezpath_to_lyon.
fn bezpath_to_lyon(bez: &kurbo::BezPath) -> LyonPath {
    let mut builder = LyonPath::builder();
    let mut open = false;
    for el in bez.elements() {
        match el {
            kurbo::PathEl::MoveTo(p) => {
                if open {
                    builder.end(false);
                }
                builder.begin(point(p.x as f32, p.y as f32));
                open = true;
            }
            kurbo::PathEl::LineTo(p) => {
                if open {
                    builder.line_to(point(p.x as f32, p.y as f32));
                }
            }
            kurbo::PathEl::QuadTo(c, p) => {
                if open {
                    builder.quadratic_bezier_to(
                        point(c.x as f32, c.y as f32),
                        point(p.x as f32, p.y as f32),
                    );
                }
            }
            kurbo::PathEl::CurveTo(c1, c2, p) => {
                if open {
                    builder.cubic_bezier_to(
                        point(c1.x as f32, c1.y as f32),
                        point(c2.x as f32, c2.y as f32),
                        point(p.x as f32, p.y as f32),
                    );
                }
            }
            kurbo::PathEl::ClosePath => {
                if open {
                    builder.end(true);
                    open = false;
                }
            }
        }
    }
    if open {
        builder.end(false);
    }
    builder.build()
}

const SETTINGS: &str = "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z";
const MENU: &str = "M4 12h16M4 6h16M4 18h16";
const FOLDER: &str = "M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z";

#[test]
#[ignore]
fn lucide_stroke_tessellation_cost() {
    for (name, d) in [("Settings", SETTINGS), ("Folder", FOLDER), ("Menu", MENU)] {
        let bez = burin::render::path::parse_svg_path(d).expect("parse");
        // Typical on-screen transform: 20px icon => scale ~0.83, plus offset.
        let xform = kurbo::Affine::translate((137.0, 442.5)) * kurbo::Affine::scale(20.0 / 24.0);

        const N: u32 = 200;
        let t0 = Instant::now();
        let mut total_verts = 0usize;
        for i in 0..N {
            // Simulate scroll: translation changes every frame -> fresh
            // transformed copy + fresh tessellation (the MISS path).
            let scrolled = kurbo::Affine::translate((0.0, -(i as f64))) * xform;
            let transformed = scrolled * bez.clone();
            let lyon_p = bezpath_to_lyon(&transformed);
            let mut buffers = VertexBuffers::<[f32; 2], u32>::new();
            let options = StrokeOptions::default()
                .with_line_width(1.5)
                .with_line_cap(lyon::tessellation::LineCap::Round)
                .with_line_join(lyon::tessellation::LineJoin::Round);
            StrokeTessellator::new()
                .tessellate_path(
                    &lyon_p,
                    &options,
                    &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex| {
                        let p = v.position();
                        [p.x, p.y]
                    }),
                )
                .expect("tessellate");
            total_verts += buffers.vertices.len();
        }
        let avg = t0.elapsed() / N;
        eprintln!(
            "{name:>8}: avg tessellation {avg:>9.2?} / miss  ({} verts)",
            total_verts / N as usize
        );
    }
}
