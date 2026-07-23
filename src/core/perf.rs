use std::cell::{Cell, RefCell};
use web_time::Instant;

/// Every measurable phase in the frame pipeline (in execution order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PerfPhase {
    KineticScroll = 0,
    Prepass = 1,
    ProcessDirty = 2,
    DeferredActions = 3,
    PortalPositions = 4,
    Layout = 5,
    Animation = 6,
    RecheckDirty = 7,
    Paint = 8,
    COUNT = 9,
}

impl PerfPhase {
    pub const ALL: [PerfPhase; PerfPhase::COUNT as usize] = [
        PerfPhase::KineticScroll,
        PerfPhase::Prepass,
        PerfPhase::ProcessDirty,
        PerfPhase::DeferredActions,
        PerfPhase::PortalPositions,
        PerfPhase::Layout,
        PerfPhase::Animation,
        PerfPhase::RecheckDirty,
        PerfPhase::Paint,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            PerfPhase::KineticScroll => "kinetic_scroll",
            PerfPhase::Prepass => "prepass",
            PerfPhase::ProcessDirty => "process_dirty",
            PerfPhase::DeferredActions => "deferred_actions",
            PerfPhase::PortalPositions => "portal_positions",
            PerfPhase::Layout => "layout",
            PerfPhase::Animation => "animation",
            PerfPhase::RecheckDirty => "recheck_dirty",
            PerfPhase::Paint => "paint",
            PerfPhase::COUNT => unreachable!(),
        }
    }
}

/// Per-frame timing breakdown (microseconds).
#[derive(Debug, Clone, Default)]
pub struct FrameTiming {
    pub phases: [u64; PerfPhase::COUNT as usize],
    pub total_us: u64,
}

#[derive(Debug, Clone, Default)]
struct PhaseAccum {
    sum_us: u64,
    count: u64,
    max_us: u64,
}

impl PhaseAccum {
    const ZERO: PhaseAccum = PhaseAccum {
        sum_us: 0,
        count: 0,
        max_us: 0,
    };
}

thread_local! {
    static PERF_ENABLED: Cell<bool> = const { Cell::new(false) };
    static PERF_CURRENT_PHASE: Cell<u8> = const { Cell::new(u8::MAX) };
    static PERF_PHASE_START: Cell<u64> = const { Cell::new(0) };
    static PERF_FRAME: RefCell<FrameTiming> = RefCell::new(FrameTiming::default());
    static PERF_ACCUM: RefCell<[PhaseAccum; PerfPhase::COUNT as usize]> =
        const { RefCell::new([PhaseAccum::ZERO; PerfPhase::COUNT as usize]) };
    static THREAD_EPOCH: Instant = Instant::now();
}

/// Enable per-frame phase timing. Safe to call multiple times.
pub fn perf_enable() {
    PERF_ENABLED.set(true);
}

/// Disable per-frame phase timing.
pub fn perf_disable() {
    PERF_ENABLED.set(false);
}

/// Returns whether phase timing is currently active.
pub fn perf_is_enabled() -> bool {
    PERF_ENABLED.get()
}

/// Begin timing a phase. Defensively ends any previously open phase.
#[inline(always)]
pub fn perf_begin(phase: PerfPhase) {
    if !PERF_ENABLED.get() {
        return;
    }
    let prev = PERF_CURRENT_PHASE.replace(phase as u8);
    if prev != u8::MAX {
        let now_us = now_micros();
        let start = PERF_PHASE_START.get();
        let elapsed = now_us.saturating_sub(start);
        add_to_frame(prev, elapsed);
    }
    PERF_PHASE_START.set(now_micros());
}

/// End the currently open phase.
#[inline(always)]
pub fn perf_end() {
    if !PERF_ENABLED.get() {
        return;
    }
    let phase = PERF_CURRENT_PHASE.replace(u8::MAX);
    if phase == u8::MAX {
        return;
    }
    let now_us = now_micros();
    let start = PERF_PHASE_START.get();
    let elapsed = now_us.saturating_sub(start);
    add_to_frame(phase, elapsed);
}

/// Prepare for a new frame: clear per-frame accumulators.
pub fn perf_reset_frame() {
    if !PERF_ENABLED.get() {
        return;
    }
    PERF_FRAME.with_borrow_mut(|f| *f = FrameTiming::default());
}

/// Take the accumulated timing for the frame just completed.
pub fn perf_take_frame() -> FrameTiming {
    PERF_FRAME.with_borrow(|f| f.clone())
}

/// Take and reset the per-frame timing accumulator in one atomic step.
pub fn perf_drain_frame() -> FrameTiming {
    let ft = perf_take_frame();
    PERF_FRAME.with_borrow_mut(|f| {
        f.phases.fill(0);
        f.total_us = 0;
    });
    ft
}

/// Accumulated statistics summary (cpu_perf style).
pub fn perf_accum_summary() -> String {
    if !PERF_ENABLED.get() {
        return String::new();
    }
    PERF_ACCUM.with_borrow(|acc| {
        let total: u64 = acc.iter().map(|a| a.sum_us).sum();
        if total == 0 {
            return String::new();
        }
        let mut lines = Vec::new();
        let frame_count = acc[0].count;
        lines.push(format!("── Frame Phase Timing ({} frames) ──", frame_count));
        for phase in PerfPhase::ALL {
            let a = &acc[phase as usize];
            if a.count == 0 {
                continue;
            }
            let avg = a.sum_us / a.count;
            let pct = if total > 0 {
                (a.sum_us as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            lines.push(format!(
                "  {:>16}: avg {:>6} us  max {:>6} us  {:>5.1}%  ({} samples)",
                phase.name(),
                avg,
                a.max_us,
                pct,
                a.count
            ));
        }
        lines.push(format!("  {:>16}: total {} us", "", total));
        lines.join("\n")
    })
}

/// Reset accumulators after printing summary (caller-side, not automatic).
pub fn perf_reset_accum() {
    PERF_ACCUM.with_borrow_mut(|acc| {
        *acc = [PhaseAccum::ZERO; PerfPhase::COUNT as usize];
    });
}

// ── internal helpers ──

fn now_micros() -> u64 {
    THREAD_EPOCH.with(|epoch| epoch.elapsed().as_micros() as u64)
}

fn add_to_frame(phase_raw: u8, elapsed_us: u64) {
    let idx = phase_raw as usize;
    if idx >= PerfPhase::COUNT as usize {
        return;
    }
    PERF_FRAME.with_borrow_mut(|f| {
        f.phases[idx] = f.phases[idx].saturating_add(elapsed_us);
    });
    PERF_ACCUM.with_borrow_mut(|acc| {
        let a = &mut acc[idx];
        a.sum_us += elapsed_us;
        a.count += 1;
        if elapsed_us > a.max_us {
            a.max_us = elapsed_us;
        }
    });
}
