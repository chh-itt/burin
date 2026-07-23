//! Top-level Window with winit event loop, rendering pipeline, layout, and input.

/// Lightweight CPU render-phase instrumentation.
///
/// Enable at runtime with `AURALIS_CPU_PERF=1`. Works in release builds (the
/// only meaningful place to measure tight pixel loops). Every 60 painted frames
/// it prints averaged per-phase timings for the CPU (tiny-skia) backend:
/// `gen` (build the DrawCommand list, cache-replayed), `raster`
/// (`render_damage` — clear + rasterise all commands/text), `present`
/// (buffer copy + softbuffer present). Zero overhead when disabled.
#[cfg(feature = "backend-tiny-skia")]
pub(crate) mod cpu_perf {
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;
    use std::time::Instant;

    struct Acc {
        frames: u32,
        gen_us: u64,
        raster_us: u64,
        present_us: u64,
        cmds: u64,
        text: u64,
        max_raster_us: u64,
        dmg_rects: u64,
        dmg_frac: f64,
        last_pr: usize,
        last_rp: usize,
        last_flush: Option<Instant>,
    }

    impl Default for Acc {
        fn default() -> Self {
            Self {
                frames: 0,
                gen_us: 0,
                raster_us: 0,
                present_us: 0,
                cmds: 0,
                text: 0,
                max_raster_us: 0,
                dmg_rects: 0,
                dmg_frac: 0.0,
                last_pr: 0,
                last_rp: 0,
                last_flush: None,
            }
        }
    }

    thread_local! {
        static ACC: RefCell<Acc> = RefCell::new(Acc::default());
    }

    pub fn enabled() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| {
            std::env::var("AURALIS_CPU_PERF")
                .map(|v| !v.is_empty() && v != "0")
                .unwrap_or(false)
        })
    }

    /// Print a one-time confirmation that instrumentation is active (so the
    /// absence of later output can be attributed to "no painted frames" rather
    /// than "env var not set"). Prints unconditionally once for diagnosis, on
    /// stdout (matching the existing `[dirty-bench]` output).
    pub fn announce_once() {
        static SHOWN: AtomicBool = AtomicBool::new(false);
        if SHOWN.swap(true, Ordering::Relaxed) {
            return;
        }
        match std::env::var("AURALIS_CPU_PERF") {
            Ok(ref v) if !v.is_empty() && v != "0" => {
                println!(
                    "[cpu-perf] ENABLED (AURALIS_CPU_PERF={v:?}). Per-phase timings print every ~30 painted frames (or ~1s). Idle with no animation produces no painted frames — scroll / hover / type to generate them."
                );
            }
            _ => {} // silent when disabled or not set
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        gen_us: u64,
        raster_us: u64,
        present_us: u64,
        cmds: usize,
        text: usize,
        dmg_n: usize,
        dmg_frac: f32,
        pr_n: usize,
        rp_n: usize,
        w: f32,
        h: f32,
    ) {
        if !enabled() {
            return;
        }
        ACC.with(|a| {
            let mut a = a.borrow_mut();
            a.frames += 1;
            a.gen_us += gen_us;
            a.raster_us += raster_us;
            a.present_us += present_us;
            a.cmds += cmds as u64;
            a.text += text as u64;
            a.max_raster_us = a.max_raster_us.max(raster_us);
            a.dmg_rects += dmg_n as u64;
            a.dmg_frac += dmg_frac as f64;
            a.last_pr = pr_n;
            a.last_rp = rp_n;

            let now = Instant::now();
            let due_time = match a.last_flush {
                Some(t) => now.duration_since(t).as_millis() >= 1000,
                None => {
                    a.last_flush = Some(now);
                    false
                }
            };
            if a.frames >= 30 || due_time {
                let f = a.frames as u64;
                let total = (a.gen_us + a.raster_us + a.present_us) / f;
                let fps = if total > 0 { 1_000_000 / total } else { 0 };
                let dmg_pct = (a.dmg_frac / a.frames as f64) * 100.0;
                println!(
                    "[cpu-perf] {f}f avg | gen {}us  raster {}us (max {})  present {}us  | paint-total {total}us (~{fps} fps) | damage {:.1}% ({} rects, pr={} rp={}) | cmds {}  text {}  win {w:.0}x{h:.0}",
                    a.gen_us / f,
                    a.raster_us / f,
                    a.max_raster_us,
                    a.present_us / f,
                    dmg_pct,
                    a.dmg_rects / f,
                    a.last_pr,
                    a.last_rp,
                    a.cmds / f,
                    a.text / f,
                );
                a.frames = 0;
                a.gen_us = 0;
                a.raster_us = 0;
                a.present_us = 0;
                a.cmds = 0;
                a.text = 0;
                a.max_raster_us = 0;
                a.dmg_rects = 0;
                a.dmg_frac = 0.0;
                a.last_flush = Some(now);
            }
        });
    }
}

// ── Sub-modules ──
mod app;
mod app_builder;
mod app_handler;
pub use app::{App, create_window};
pub use app_builder::AppBuilder;
pub(crate) mod scroll_physics;
pub(crate) mod frame_hook;
pub(crate) mod icon;
pub(crate) mod buttons;
pub(crate) mod handle;
pub(crate) mod config;
mod cursor_blink;
mod frame_ticks;
mod winit_map;
mod submenu;
mod finger;
mod drag;
mod action;
mod cancel_path;
mod ime;
mod window_state;
mod window_frame;
mod window_events;
mod window_idle;
// ── Re-exports from sub-modules ──
pub(crate) use cursor_blink::process_cursor_blink;
pub(crate) use frame_ticks::process_frame_ticks;

pub use icon::WindowIcon;
pub use buttons::WindowButtons;
pub use handle::WindowHandle;
pub use config::WindowConfig;
pub(crate) use window_state::WindowState;
