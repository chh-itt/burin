//! Shared performance-probe infrastructure.
//!
//! Scene builders, measurement collectors, and structural assertion helpers
//! extracted from the existing test suite (audit_bench, scroll_paint_probe,
//! hover_cross_probe, paint_cull, etc.).  Used by both the individual probe
//! tests and the unified `tests/perf_suite.rs`.

use crate::core::element::ElementArena;
use crate::core::perf::{FrameTiming, PerfPhase};
use crate::core::ElementId;
use crate::style::Point;
use crate::testing::TestHarness;
use crate::widgets::display::Text;
use crate::widgets::layout::{HStack, ScrollView, SizedBox, VStack};

/// Collects per-phase frame-timing stats over N frames.
///
/// Used by every measurement loop in the test suite — extract once, use
/// everywhere.
pub struct PhaseTiming {
    pub frames: usize,
    pub layout: Vec<u64>,
    pub prepass: Vec<u64>,
    pub deferred: Vec<u64>,
    pub dirty: Vec<u64>,
    pub paint: Vec<u64>,
    pub total: Vec<u64>,
}

impl PhaseTiming {
    pub fn new() -> Self {
        Self {
            frames: 0,
            layout: Vec::new(),
            prepass: Vec::new(),
            deferred: Vec::new(),
            dirty: Vec::new(),
            paint: Vec::new(),
            total: Vec::new(),
        }
    }

    pub fn record(&mut self, t: &FrameTiming) {
        self.frames += 1;
        self.layout.push(t.phases[PerfPhase::Layout as usize]);
        self.prepass.push(t.phases[PerfPhase::Prepass as usize]);
        self.deferred
            .push(t.phases[PerfPhase::DeferredActions as usize]);
        self.dirty.push(t.phases[PerfPhase::ProcessDirty as usize]);
        self.paint.push(t.phases[PerfPhase::Paint as usize]);
        self.total.push(t.phases.iter().sum());
    }

    pub fn avg(v: &[u64]) -> u64 {
        v.iter().sum::<u64>() / v.len().max(1) as u64
    }

    pub fn layout_avg(&self) -> u64 {
        Self::avg(&self.layout)
    }
    pub fn prepass_avg(&self) -> u64 {
        Self::avg(&self.prepass)
    }
    pub fn paint_avg(&self) -> u64 {
        Self::avg(&self.paint)
    }
    pub fn total_avg(&self) -> u64 {
        Self::avg(&self.total)
    }
    pub fn total_max(&self) -> u64 {
        self.total.iter().copied().max().unwrap_or(0)
    }
    pub fn dirty_avg(&self) -> u64 {
        Self::avg(&self.dirty)
    }

    /// CSV-ish one-liner for script parsing: `name layout_us prepass_us ... total_us`
    pub fn csv_line(&self, name: &str) -> String {
        format!(
            "{name} {} {} {} {} {} {} {}",
            self.frames,
            self.layout_avg(),
            self.prepass_avg(),
            self.deferred_avg(),
            self.dirty_avg(),
            self.paint_avg(),
            self.total_avg(),
        )
    }

    /// Human-readable multi-line report.
    pub fn report(&self, name: &str) -> String {
        let mut s = format!("═══ {name} ({nframes} frames) ═══\n", nframes = self.frames);
        s.push_str(&format!(
            "  layout   avg {:>6}us  max {:>6}us\n",
            self.layout_avg(),
            Self::max(&self.layout)
        ));
        s.push_str(&format!(
            "  prepass  avg {:>6}us  max {:>6}us\n",
            self.prepass_avg(),
            Self::max(&self.prepass)
        ));
        s.push_str(&format!(
            "  deferred avg {:>6}us  max {:>6}us\n",
            self.deferred_avg(),
            Self::max(&self.deferred)
        ));
        s.push_str(&format!(
            "  dirty    avg {:>6}us  max {:>6}us\n",
            self.dirty_avg(),
            Self::max(&self.dirty)
        ));
        s.push_str(&format!(
            "  paint    avg {:>6}us  max {:>6}us\n",
            self.paint_avg(),
            Self::max(&self.paint)
        ));
        s.push_str(&format!(
            "  TOTAL    avg {:>6}us  max {:>6}us\n",
            self.total_avg(),
            Self::max(&self.total)
        ));
        s
    }

    fn max(v: &[u64]) -> u64 {
        v.iter().copied().max().unwrap_or(0)
    }
    fn deferred_avg(&self) -> u64 {
        Self::avg(&self.deferred)
    }
}

// ── Tree-walking helpers (DRY across audit_bench / scroll_paint_probe / paint_cull) ──

/// Find the scrollable descendant with the largest content height, given a
/// mounted root. Panics if no scroll container is found.
pub fn find_tallest_scroll_container(h: &TestHarness, root: ElementId) -> ElementId {
    let mut stack = vec![root];
    let mut best: Option<(ElementId, f32)> = None;
    while let Some(id) = stack.pop() {
        if let Some(sc) = h.root().comp_scroll(id) {
            let cb = sc.content_bounds.get().height;
            if best.is_none_or(|(_, b)| cb > b) {
                best = Some((id, cb));
            }
        }
        if let Some(el) = h.find(id) {
            for &c in &el.children {
                stack.push(c);
            }
        }
    }
    best.expect("no scroll container found").0
}

pub fn element_count(arena: &ElementArena, root: ElementId) -> usize {
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        count += 1;
        if let Some(el) = arena.get(id) {
            for &c in &el.children {
                stack.push(c);
            }
        }
    }
    count
}

/// Count elements whose HOVERED flag is set in a subtree.
pub fn count_hovered(arena: &ElementArena, root: ElementId) -> usize {
    use crate::core::config::StateFlags;
    let mut count = 0;
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(el) = arena.get(id) {
            if el.state.get().contains(StateFlags::HOVERED) {
                count += 1;
            }
            for &c in &el.children {
                stack.push(c);
            }
        }
    }
    count
}

// ── Measurement loops ──

/// Scroll `target` by `dy` pixels, `frames` times, recording timing each
/// frame. Returns `PhaseTiming` covering the scroll frames only (warm-up
/// frames are the caller's responsibility).
pub fn measure_scroll_frames(
    h: &mut TestHarness,
    target: ElementId,
    dy: f32,
    frames: usize,
) -> PhaseTiming {
    let mut t = PhaseTiming::new();
    for _ in 0..frames {
        h.scroll(target, 0.0, dy);
        h.run_frame();
        t.record(&h.frame_timing());
    }
    t
}

/// Measure `frames` idle frames (nothing changed, no dirty). Uses the
/// existing harness state — caller must have settled the tree first.
pub fn measure_idle_frames(h: &mut TestHarness, frames: usize) -> PhaseTiming {
    let mut t = PhaseTiming::new();
    for _ in 0..frames {
        h.run_frame();
        t.record(&h.frame_timing());
    }
    t
}

/// Hover-crossing measurement: alternate `h.hover_at(pos_a)` and
/// `h.hover_at(pos_b)` for `iterations` frames. Returns separate paint,
/// total, and subtree-cache-miss vectors.
pub struct HoverCrossTiming {
    pub paints: Vec<u64>,
    pub totals: Vec<u64>,
    pub cache_misses: Vec<u64>,
}

pub fn measure_hover_crossings(
    h: &mut TestHarness,
    pos_a: Point,
    pos_b: Point,
    iterations: usize,
) -> HoverCrossTiming {
    let mut paints = Vec::with_capacity(iterations);
    let mut totals = Vec::with_capacity(iterations);
    let mut misses = Vec::with_capacity(iterations);
    for i in 0..iterations {
        let pos = if i % 2 == 0 { pos_a } else { pos_b };
        h.hover_at(pos);
        h.run_frame();
        let t = h.frame_timing();
        paints.push(t.phases[PerfPhase::Paint as usize]);
        totals.push(t.phases.iter().sum());
        misses.push(h.subtree_cache_misses());
    }
    HoverCrossTiming {
        paints,
        totals,
        cache_misses: misses,
    }
}

// ── Scene builders (the most-reused widget trees across probes) ──

/// Build a static ScrollView containing `lines` rows of text. Returns the
/// mounted root. After calling this, run 3-5 settle frames before measuring.
pub fn build_static_scroll_page(
    h: &mut TestHarness,
    lines: usize,
    viewport_w: f32,
    viewport_h: f32,
) -> ElementId {
    let mut content = VStack::new();
    for i in 0..lines {
        content = content.push(Text::new(format!(
            "line {i} — the quick brown fox jumps over the lazy dog"
        )));
    }
    h.mount(
        SizedBox::new()
            .width(viewport_w)
            .height(viewport_h)
            .child(ScrollView::new().child(content)),
    )
}

/// Two side-by-side text panels (L/R) with `rows` rows each, pure static
/// content (no hover styles anywhere). Used for hover-crossing probes.
pub fn build_dual_panel_scene(h: &mut TestHarness, rows: usize) {
    use crate::style::styled::Styled;
    use crate::style::Color;

    fn panel(rows: usize, tag: &str, row_bg: Color, container_bg: Color) -> VStack {
        let mut v = VStack::new();
        for i in 0..rows {
            v = v.push(
                HStack::new()
                    .background(row_bg)
                    .push(Text::new(format!("{tag} row {i} — label")))
                    .push(Text::new("value 42")),
            );
        }
        VStack::new().background(container_bg).push(v)
    }

    h.mount(
        HStack::new()
            .push(SizedBox::new().width(560.0).height(760.0).child(panel(
                rows,
                "L",
                Color::rgba8(38, 38, 46, 255),
                Color::rgba8(26, 26, 31, 255),
            )))
            .push(SizedBox::new().width(560.0).height(760.0).child(panel(
                rows,
                "R",
                Color::rgba8(46, 38, 38, 255),
                Color::rgba8(31, 26, 26, 255),
            ))),
    );
}

/// Build a balanced alternating VStack/HStack tree: `branching^depth`
/// leaves. The audit-bench "app tree" shape — (6, 4) ≈ 1.5k elements.
///
/// Guarded against parameter explosions: panics if the projected node count
/// exceeds 100k (a (200, 4) typo once produced a 1.6-billion-element mount
/// and a 10 GB hang).
pub fn build_balanced_tree(branching: usize, depth: usize) -> Box<dyn crate::core::widget::Widget> {
    let projected = (branching as u64).saturating_pow(depth as u32);
    assert!(
        projected <= 100_000,
        "build_balanced_tree({branching}, {depth}) would mount ~{projected} elements — refusing"
    );
    build_balanced_inner(branching, depth)
}

/// Adapter so `Box<dyn Widget>` can be `push`ed into stacks.
pub struct BoxedWidget(pub Box<dyn crate::core::widget::Widget>);
impl crate::core::widget::Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut crate::core::context::MountContext<'_>) -> ElementId {
        self.0.mount_box(ctx)
    }
}

fn build_balanced_inner(branching: usize, depth: usize) -> Box<dyn crate::core::widget::Widget> {
    use crate::style::{styled::Styled, Dimension};
    if depth == 0 {
        return Box::new(Text::new("leaf"));
    }
    if depth.is_multiple_of(2) {
        let mut s = VStack::new()
            .width(Dimension::Pixels(300.0))
            .height(Dimension::Pixels(600.0));
        for _ in 0..branching {
            s = s.push(BoxedWidget(build_balanced_inner(branching, depth - 1)));
        }
        Box::new(s)
    } else {
        let mut s = HStack::new()
            .width(Dimension::Pixels(300.0))
            .height(Dimension::Pixels(600.0));
        for _ in 0..branching {
            s = s.push(BoxedWidget(build_balanced_inner(branching, depth - 1)));
        }
        Box::new(s)
    }
}

/// Check: after `h.hover_at(pos)`, does `dirty_registry::has_pending_dirty()`
/// return false? (i.e. hover over static content registered zero repaint).
pub fn hover_registers_dirty(h: &mut TestHarness, pos: Point) -> bool {
    h.hover_at(pos);
    crate::core::dirty_registry::has_pending_dirty()
}
