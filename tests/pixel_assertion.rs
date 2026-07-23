//! Pixel-level assertion tests (Plan A). Requires `backend-tiny-skia` feature.

#[cfg(feature = "backend-tiny-skia")]
mod pixel_tests {
    use burin::render::{ClipInfo, DrawCommand};
    use burin::style::{Color, CornerRadii, LinearGradient, Rect};
    use burin::testing::pixel::rasterize_commands;
    use burin::testing::TestHarness;
    use burin::widgets::display::Text;
    use burin::widgets::layout::SizedBox;
    use glam::{Affine2, Vec2};

    const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    const RED: Color = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    fn close(a: Color, b: Color) -> bool {
        let t = 6.0 / 255.0;
        (a.r - b.r).abs() < t
            && (a.g - b.g).abs() < t
            && (a.b - b.b).abs() < t
            && (a.a - b.a).abs() < t
    }

    #[test]
    fn center_pixel_matches_solid_background() {
        let mut h = TestHarness::new(200.0, 100.0);
        let blue = Color {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };
        let id = h.mount(
            SizedBox::new()
                .width(100.0)
                .height(50.0)
                .child(Text::new("")),
        );
        h.find_mut(id).unwrap().set_background(blue);
        h.run_frame();

        let el = h.find(id).unwrap();
        let cx = (el.screen_bounds.x + el.screen_bounds.width / 2.0) as u32;
        let cy = (el.screen_bounds.y + el.screen_bounds.height / 2.0) as u32;
        h.assert_pixel(cx, cy, blue);
    }

    /// A white fill covering the whole 100×100 viewport, clipped to a circle
    /// (corner radius 50 on a 100×100 rect). The CPU backend must clip the
    /// rounded corners just like the GPU shader's `clip_alpha`: corners show
    /// the background, the centre stays white.
    #[test]
    fn fill_rect_respects_rounded_clip() {
        let clip = ClipInfo::with_radius(Rect::new(0.0, 0.0, 100.0, 100.0), CornerRadii::all(50.0));
        let cmds = vec![DrawCommand::FillRect {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            color: WHITE,
            radius: CornerRadii::ZERO,
            clip,
            transform: Affine2::IDENTITY,
            z_index: 0,
            blend_mode: 0,
        }];
        let buf = rasterize_commands(&cmds, 100, 100, 1.0, BLACK);

        assert!(
            close(buf.pixel_color(50, 50).unwrap(), WHITE),
            "centre of the rounded clip should be filled, got {:?}",
            buf.pixel_color(50, 50)
        );
        assert!(
            close(buf.pixel_color(3, 3).unwrap(), BLACK),
            "rounded corner must be clipped to background, got {:?}",
            buf.pixel_color(3, 3)
        );
    }

    /// A gradient covering the whole viewport but clipped to the top-left
    /// 40×40 square. Previously the CPU backend ignored clip for gradients,
    /// so the fill bled across the whole window. Now it must be clipped.
    #[test]
    fn gradient_respects_clip() {
        let grad = LinearGradient::new((0.0, 0.0), (1.0, 1.0), &[(RED, 0.0), (RED, 1.0)]);
        let clip = ClipInfo::new(Rect::new(0.0, 0.0, 40.0, 40.0));
        let cmds = vec![DrawCommand::FillLinearGradient {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            gradient: grad,
            radius: CornerRadii::ZERO,
            stroke_width: 0.0,
            clip,
            transform: Affine2::IDENTITY,
            z_index: 0,
        }];
        let buf = rasterize_commands(&cmds, 100, 100, 1.0, BLACK);

        assert!(
            close(buf.pixel_color(20, 20).unwrap(), RED),
            "inside the clip the gradient should be drawn, got {:?}",
            buf.pixel_color(20, 20)
        );
        assert!(
            close(buf.pixel_color(60, 60).unwrap(), BLACK),
            "outside the clip the gradient must NOT bleed, got {:?}",
            buf.pixel_color(60, 60)
        );
    }

    /// Regression: a scroll container's clip lives in raw (pre-transform) rect
    /// space; the content is drawn through a scroll-translation transform. The
    /// clip mask must be rasterised through that same transform so it stays
    /// pinned to the fixed on-screen viewport. A naive device-space mask drifts
    /// with the scroll offset and hides everything it clips after scrolling —
    /// which manifested as backgrounds/borders/inner-scrollbars vanishing once
    /// the user scrolled.
    #[test]
    fn scrolled_fill_respects_fixed_viewport() {
        // Viewport: top-left 100×50 of a 100×100 window, scrolled down by 30.
        let clip = ClipInfo::with_scroll(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            CornerRadii::ZERO,
            [0.0, 30.0],
        );
        let xform = Affine2::from_translation(Vec2::new(0.0, -30.0));

        // Child A (raw y 40..48 ⊂ abs_clip raw y 30..80) → on-screen y 10..18,
        // inside the viewport → must be VISIBLE.
        let child_a = DrawCommand::FillRect {
            rect: Rect::new(0.0, 40.0, 100.0, 8.0),
            color: WHITE,
            radius: CornerRadii::ZERO,
            clip,
            transform: xform,
            z_index: 0,
            blend_mode: 0,
        };
        // Child B (raw y 100..108, OUTSIDE abs_clip) → on-screen y 70..78,
        // outside the viewport → must be CLIPPED.
        let child_b = DrawCommand::FillRect {
            rect: Rect::new(0.0, 100.0, 100.0, 8.0),
            color: WHITE,
            radius: CornerRadii::ZERO,
            clip,
            transform: xform,
            z_index: 0,
            blend_mode: 0,
        };
        let buf = rasterize_commands(&[child_a, child_b], 100, 100, 1.0, BLACK);

        assert!(
            close(buf.pixel_color(50, 13).unwrap(), WHITE),
            "scrolled content inside the viewport must stay visible, got {:?}",
            buf.pixel_color(50, 13)
        );
        assert!(
            close(buf.pixel_color(50, 73).unwrap(), BLACK),
            "content scrolled past the viewport must be clipped, got {:?}",
            buf.pixel_color(50, 73)
        );
    }
}
