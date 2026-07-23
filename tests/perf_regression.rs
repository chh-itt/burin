use auralis_signal::Signal;
use burin::core::context::MountContext;
use burin::core::element::ElementId;
use burin::core::perf::PerfPhase;
use burin::core::widget::Widget;
use burin::style::{Dimension, Styled};
use burin::testing::TestHarness;
use burin::widgets::display::Text;
use burin::widgets::layout::{HStack, SizedBox, VStack};

struct BoxedWidget(Box<dyn Widget>);
impl Widget for BoxedWidget {
    fn mount_box(self: Box<Self>, ctx: &mut MountContext<'_>) -> ElementId {
        self.0.mount_box(ctx)
    }
}

fn pct(p: f32) -> Dimension {
    Dimension::Percent(p)
}
fn px(w: f32) -> Dimension {
    Dimension::Pixels(w)
}

fn bw(w: Box<dyn Widget>) -> BoxedWidget {
    BoxedWidget(w)
}

/// Per-phase frame sample with element-wise min (baseline stabilization).
#[derive(Clone, Copy)]
struct FrameSample([u64; PerfPhase::COUNT as usize]);
impl FrameSample {
    fn min(&self, other: &FrameSample) -> FrameSample {
        let mut out = self.0;
        for (o, b) in out.iter_mut().zip(other.0.iter()) {
            *o = (*o).min(*b);
        }
        FrameSample(out)
    }
}

// ── Tree generators ──

fn build_deep_chain(depth: usize) -> Box<dyn Widget> {
    if depth == 0 {
        return Box::new(
            SizedBox::new()
                .width(80.0)
                .height(24.0)
                .child(Text::new("leaf")),
        );
    }
    Box::new(
        VStack::new()
            .push(bw(build_deep_chain(depth - 1)))
            .width(px(200.0))
            .height(px(400.0)),
    )
}

fn build_wide_flat(width: usize) -> Box<dyn Widget> {
    let mut s = HStack::new();
    for i in 0..width {
        s = s.push(Text::new(format!("item{}", i)));
    }
    Box::new(s.width(px(1600.0)).height(px(28.0)))
}

fn build_balanced(branching: usize, depth: usize) -> Box<dyn Widget> {
    if depth == 0 {
        return Box::new(Text::new("leaf"));
    }
    let use_v = depth % 2 == 0;
    if use_v {
        let mut s = VStack::new().width(px(300.0)).height(px(600.0));
        for _ in 0..branching {
            s = s.push(bw(build_balanced(branching, depth - 1)));
        }
        Box::new(s)
    } else {
        let mut s = HStack::new().width(px(300.0)).height(px(600.0));
        for _ in 0..branching {
            s = s.push(bw(build_balanced(branching, depth - 1)));
        }
        Box::new(s)
    }
}

fn build_stretch_layout(size: usize) -> Box<dyn Widget> {
    let mut s = VStack::new();
    for i in 0..size {
        let mut row = HStack::new();
        row = row.push(Text::new(format!("a{}", i)));
        row = row.push(Text::new(format!("b{}", i)));
        row = row.push(Text::new(format!("c{}", i)));
        s = s.push(bw(Box::new(row)));
    }
    Box::new(s)
}

fn build_grid_layout(cols: usize, rows: usize) -> Box<dyn Widget> {
    use burin::widgets::layout::{GridItem, GridRow};
    let mut g = GridRow::new().columns(cols as u32);
    for _ in 0..rows {
        for _ in 0..cols {
            g = g.push(GridItem::new(Text::new("cell")));
        }
    }
    Box::new(g)
}

fn box_text(t: &str) -> Box<dyn Widget> {
    Box::new(
        SizedBox::new()
            .width(80.0)
            .height(20.0)
            .child(Text::new(t.to_string())),
    )
}

fn build_mixed(size: usize) -> Box<dyn Widget> {
    let mut s = VStack::new().width(pct(1.0)).height(px(1600.0)).gap(8.0);
    for i in 0..size {
        if i % 3 == 0 {
            let mut row = HStack::new().gap(4.0);
            for j in 0..3 {
                row = row.push(Text::new(format!("row{}_{}", i, j)));
            }
            s = s.push(bw(Box::new(row)));
        } else if i % 3 == 1 {
            s = s.push(bw(box_text(&format!("item{}", i))));
        } else {
            let mut inner = VStack::new().gap(2.0);
            inner = inner.push(Text::new(format!("section{}", i)));
            inner = inner.push(bw(box_text("a")));
            inner = inner.push(bw(box_text("b")));
            s = s.push(bw(Box::new(inner)));
        }
    }
    Box::new(s)
}

// ── Helpers ──

fn element_count(h: &TestHarness) -> usize {
    dfs_ids(h).len()
}

fn print_frame_timing(name: &str, h: &TestHarness) {
    let t = h.frame_timing();
    let total = t.phases.iter().sum::<u64>();
    println!("── {name} ──");
    println!(
        "  elements={:<6} layout={:>7}us  paint={:>7}us  dirty={:>6}us  total={:>7}us",
        element_count(h),
        t.phases[PerfPhase::Layout as usize],
        t.phases[PerfPhase::Paint as usize],
        t.phases[PerfPhase::ProcessDirty as usize],
        total
    );
    println!(
        "  scroll={}  prepass={}  deferred={}  portal={}  anim={}  recheck={}",
        t.phases[PerfPhase::KineticScroll as usize],
        t.phases[PerfPhase::Prepass as usize],
        t.phases[PerfPhase::DeferredActions as usize],
        t.phases[PerfPhase::PortalPositions as usize],
        t.phases[PerfPhase::Animation as usize],
        t.phases[PerfPhase::RecheckDirty as usize],
    );
    println!(
        "  incremental_taken={}  escalation_taken={}  subtree_hit={}  subtree_miss={}",
        h.incremental_taken(),
        h.escalation_taken(),
        h.subtree_cache_hits(),
        h.subtree_cache_misses(),
    );
}

// ── Benchmarks (all #[ignore]) ──

#[test]
#[ignore]
fn phase_breakdown_steady_state() {
    let cases: Vec<(&str, Box<dyn Fn() -> Box<dyn Widget>>)> = vec![
        // deep_100 removed: nested fixed-size flex chains layout in EXPONENTIAL
        // time (~4x per +4 depth; depth 24 = 72s, depth 100 = heat death).
        // Pre-existing taffy-bridge issue, filed 2026-07-17 — the case made
        // generate_baseline unrunnable, so the baseline never existed.
        ("deep_16", Box::new(|| build_deep_chain(16))),
        ("wide_100", Box::new(|| build_wide_flat(100))),
        ("wide_500", Box::new(|| build_wide_flat(500))),
        ("balanced_6x4", Box::new(|| build_balanced(6, 4))),
        ("balanced_8x4", Box::new(|| build_balanced(8, 4))),
        ("stretch_200", Box::new(|| build_stretch_layout(200))),
        ("stretch_500", Box::new(|| build_stretch_layout(500))),
        ("grid_4x25", Box::new(|| build_grid_layout(4, 25))),
        ("grid_10x50", Box::new(|| build_grid_layout(10, 50))),
        ("mixed_200", Box::new(|| build_mixed(200))),
    ];

    for (name, mk) in &cases {
        let mut h = TestHarness::new(1600.0, 1200.0);
        h.enable_perf();
        h.mount(bw(mk()));
        h.run_frame();
        h.run_frame();
        print_frame_timing(name, &h);
        println!();
    }
}

#[test]
#[ignore]
fn scaling_by_size() {
    let scales: Vec<(usize, &str, Box<dyn Fn(usize) -> Box<dyn Widget>>)> = vec![
        (50, "wide", Box::new(|n| build_wide_flat(n))),
        (100, "wide", Box::new(|n| build_wide_flat(n))),
        (200, "wide", Box::new(|n| build_wide_flat(n))),
        (400, "wide", Box::new(|n| build_wide_flat(n))),
        (100, "stretch", Box::new(|n| build_stretch_layout(n))),
        (200, "stretch", Box::new(|n| build_stretch_layout(n))),
        (400, "stretch", Box::new(|n| build_stretch_layout(n))),
        (600, "stretch", Box::new(|n| build_stretch_layout(n))),
    ];

    println!(
        "{:>25} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "tree", "nodes", "layout", "paint", "dirty", "total", "incr", "escal"
    );
    for (size, shape, mk) in &scales {
        let mut h = TestHarness::new(1600.0, 1200.0);
        h.enable_perf();
        h.mount(bw(mk(*size)));
        h.run_frame();
        h.run_frame();
        let t = h.frame_timing();
        let total = t.phases.iter().sum::<u64>();
        println!(
            "{:>25} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}",
            format!("{}_{}", shape, size),
            element_count(&h),
            t.phases[PerfPhase::Layout as usize],
            t.phases[PerfPhase::Paint as usize],
            t.phases[PerfPhase::ProcessDirty as usize],
            total,
            h.incremental_taken(),
            h.escalation_taken(),
        );
    }
}

#[test]
#[ignore]
fn text_mutation_phases() {
    let sig = Signal::new(String::from("Hello"));
    let mut h = TestHarness::new(800.0, 600.0);
    h.enable_perf();
    h.mount(
        VStack::new()
            .push(Text::new(String::new()).bind(sig.clone()))
            .push(Text::new("static"))
            .push(Text::new("also static")),
    );
    h.run_frame();
    h.run_frame();

    println!("── text_mutation: baseline ──");
    print_frame_timing("baseline", &h);

    h.set_signal(
        &sig,
        "This is a much longer text string that should trigger a layout change".into(),
    );
    h.run_frame();
    h.run_frame();
    println!("── text_mutation: after change ──");
    print_frame_timing("changed", &h);
}

#[test]
#[ignore]
fn subtree_cache_effectiveness() {
    let sig = Signal::new(String::from("x"));

    println!(
        "{:>25} {:>6} {:>8} {:>8} {:>7} {:>7} {:>7}",
        "tree", "nodes", "hit", "miss", "hit%", "incr", "escal"
    );

    let cases: Vec<(&str, Box<dyn Fn() -> Box<dyn Widget>>)> = vec![
        ("stretch_200", Box::new(|| build_stretch_layout(200))),
        ("balanced_5x3", Box::new(|| build_balanced(5, 3))),
        ("deep_20", Box::new(|| build_deep_chain(20))),
        ("wide_200", Box::new(|| build_wide_flat(200))),
    ];

    for (name, mk) in &cases {
        let mut h = TestHarness::new(1600.0, 1200.0);
        h.enable_perf();
        h.mount(
            VStack::new()
                .push(bw(mk()))
                .push(Text::new(String::new()).bind(sig.clone())),
        );
        h.run_frame();
        h.run_frame();

        let s = "y".repeat(20);
        h.set_signal(&sig, s);
        h.run_frame(); // mutation frame — cache counters meaningful here
        let hits = h.subtree_cache_hits();
        let misses = h.subtree_cache_misses();
        let escal = h.frame_escalations();
        let incr = h.frame_incremental_layouts();
        let t = h.frame_timing();
        h.run_frame(); // stabilize (counters may be 0 if paint skipped)

        let total_hits = hits + misses;
        let hit_pct = if total_hits > 0 {
            (hits as f64 / total_hits as f64) * 100.0
        } else {
            0.0
        };

        println!(
            "{:>25} {:>6} {:>8} {:>8} {:>7.1}% {:>7} {:>7} {:>7}us {:>7}us",
            name,
            element_count(&h),
            hits,
            misses,
            hit_pct,
            incr,
            escal,
            t.phases[PerfPhase::Layout as usize],
            t.phases[PerfPhase::Paint as usize],
        );
    }
}

#[test]
#[ignore]
fn first_frame_vs_steady_state() {
    let cases: Vec<(&str, Box<dyn Fn() -> Box<dyn Widget>>)> = vec![
        ("balanced_6x3", Box::new(|| build_balanced(6, 3))),
        ("stretch_100", Box::new(|| build_stretch_layout(100))),
        ("wide_100", Box::new(|| build_wide_flat(100))),
    ];

    println!(
        "{:>20} {:>6} {:>10} {:>10} {:>10} {:>10}",
        "tree", "nodes", "first_us", "steady_us", "ratio", "incr"
    );

    for (name, mk) in &cases {
        let mut h = TestHarness::new(1600.0, 1200.0);
        h.enable_perf();
        h.mount(bw(mk()));
        h.run_frame();
        let t_first = h.frame_timing();
        let first_total: u64 = t_first.phases.iter().sum();

        h.run_frame();
        let t_steady = h.frame_timing();
        let steady_total: u64 = t_steady.phases.iter().sum();

        let ratio = if steady_total > 0 {
            first_total as f64 / steady_total as f64
        } else {
            0.0
        };

        println!(
            "{:>20} {:>6} {:>10} {:>10} {:>9.1}x {:>10}",
            name,
            element_count(&h),
            first_total,
            steady_total,
            ratio,
            h.incremental_taken(),
        );
    }
}

// ── CI regression baseline ──

#[test]
#[ignore]
fn generate_baseline() {
    let cases: Vec<(&str, Box<dyn Fn() -> Box<dyn Widget>>)> = vec![
        ("balanced_6x4", Box::new(|| build_balanced(6, 4))),
        ("balanced_8x4", Box::new(|| build_balanced(8, 4))),
        ("stretch_200", Box::new(|| build_stretch_layout(200))),
        ("stretch_500", Box::new(|| build_stretch_layout(500))),
        // deep_100 removed: nested fixed-size flex chains layout in EXPONENTIAL
        // time (~4x per +4 depth; depth 24 = 72s, depth 100 = heat death).
        // Pre-existing taffy-bridge issue, filed 2026-07-17 — the case made
        // generate_baseline unrunnable, so the baseline never existed.
        ("deep_16", Box::new(|| build_deep_chain(16))),
        ("wide_100", Box::new(|| build_wide_flat(100))),
        ("wide_500", Box::new(|| build_wide_flat(500))),
        ("grid_10x50", Box::new(|| build_grid_layout(10, 50))),
        ("mixed_200", Box::new(|| build_mixed(200))),
    ];

    let mut baseline = serde_json::Map::new();

    for (name, mk) in &cases {
        // 3 samples, keep the per-phase MIN — the most stable representative
        // (single-shot values swing 2-4x on a busy machine).
        let mut best: Option<FrameSample> = None;
        let mut elements = 0usize;
        let mut incr = 0u64;
        let mut escal = 0u64;
        for _ in 0..3 {
            let mut h = TestHarness::new(1600.0, 1200.0);
            h.enable_perf();
            h.mount(bw(mk()));
            h.run_frame();
            h.run_frame();
            let t = h.frame_timing();
            elements = element_count(&h);
            incr = h.incremental_taken();
            escal = h.escalation_taken();
            let sample = FrameSample(t.phases);
            best = Some(match best {
                None => sample,
                Some(prev) => prev.min(&sample),
            });
        }
        let phases = best.unwrap().0;
        let mut entry = serde_json::Map::new();
        for phase in PerfPhase::ALL {
            entry.insert(
                phase.name().to_string(),
                serde_json::Value::Number(serde_json::Number::from(phases[phase as usize])),
            );
        }
        entry.insert(
            "element_count".to_string(),
            serde_json::Value::Number(serde_json::Number::from(elements)),
        );
        entry.insert(
            "incremental_taken".to_string(),
            serde_json::Value::Number(serde_json::Number::from(incr)),
        );
        entry.insert(
            "escalation_taken".to_string(),
            serde_json::Value::Number(serde_json::Number::from(escal)),
        );
        baseline.insert(name.to_string(), serde_json::Value::Object(entry));
    }

    let mut meta = serde_json::Map::new();
    meta.insert("version".to_string(), serde_json::Value::Number(1.into()));
    meta.insert(
        "generated_at".to_string(),
        serde_json::Value::String(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default(),
        ),
    );
    baseline.insert("_meta".to_string(), serde_json::Value::Object(meta));

    let json = serde_json::to_string_pretty(&baseline).unwrap();
    std::fs::write("tests/perf_baseline.json", &json).unwrap();
    println!("Baseline written to tests/perf_baseline.json");
}

#[test]
#[ignore]
fn check_regression() {
    let baseline: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string("tests/perf_baseline.json").unwrap())
            .unwrap();

    let cases: Vec<(&str, Box<dyn Fn() -> Box<dyn Widget>>)> = vec![
        ("balanced_6x4", Box::new(|| build_balanced(6, 4))),
        ("balanced_8x4", Box::new(|| build_balanced(8, 4))),
        ("stretch_200", Box::new(|| build_stretch_layout(200))),
        ("stretch_500", Box::new(|| build_stretch_layout(500))),
        // deep_100 removed: nested fixed-size flex chains layout in EXPONENTIAL
        // time (~4x per +4 depth; depth 24 = 72s, depth 100 = heat death).
        // Pre-existing taffy-bridge issue, filed 2026-07-17 — the case made
        // generate_baseline unrunnable, so the baseline never existed.
        ("deep_16", Box::new(|| build_deep_chain(16))),
        ("wide_100", Box::new(|| build_wide_flat(100))),
        ("wide_500", Box::new(|| build_wide_flat(500))),
        ("grid_10x50", Box::new(|| build_grid_layout(10, 50))),
        ("mixed_200", Box::new(|| build_mixed(200))),
    ];

    let mut failures = Vec::new();

    // Ignore sub-noise-floor movement: phase timings under a few hundred µs
    // swing 2-4x with machine load; only deltas above the floor are signal.
    const NOISE_FLOOR_US: u64 = 300;

    for (name, mk) in &cases {
        // Best-of-3: compare the most favourable run against the baseline —
        // a genuine regression persists across all three.
        let mut best: Option<FrameSample> = None;
        for _ in 0..3 {
            let mut h = TestHarness::new(1600.0, 1200.0);
            h.enable_perf();
            h.mount(bw(mk()));
            h.run_frame();
            h.run_frame();
            let sample = FrameSample(h.frame_timing().phases);
            best = Some(match best {
                None => sample,
                Some(prev) => prev.min(&sample),
            });
        }
        let phases = best.unwrap().0;

        if let Some(base) = baseline.get(*name) {
            let base_obj = base.as_object().unwrap();
            for phase in PerfPhase::ALL {
                let base_us = base_obj
                    .get(phase.name())
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cur_us = phases[phase as usize];
                if base_us == 0 || cur_us.saturating_sub(base_us) <= NOISE_FLOOR_US {
                    continue;
                }
                if cur_us > base_us.saturating_mul(2) {
                    failures.push(format!(
                        "{}::{}: {} us → {} us ({}x, exceeds 2.0x threshold)",
                        name,
                        phase.name(),
                        base_us,
                        cur_us,
                        cur_us as f64 / base_us as f64
                    ));
                } else if cur_us > base_us.saturating_mul(3) / 2
                    && matches!(phase, PerfPhase::Layout | PerfPhase::Paint)
                {
                    failures.push(format!(
                        "{}::{}: {} us → {} us ({}x, exceeds 1.5x for critical phase)",
                        name,
                        phase.name(),
                        base_us,
                        cur_us,
                        cur_us as f64 / base_us as f64
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Performance regression detected:\n{}",
        failures.join("\n")
    );
}

fn dfs_ids(h: &TestHarness) -> Vec<ElementId> {
    fn walk(h: &TestHarness, id: ElementId, out: &mut Vec<ElementId>) {
        out.push(id);
        if let Some(el) = h.find(id) {
            let children = el.children.clone();
            for c in children {
                walk(h, c, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(h, h.root_id(), &mut out);
    out
}
