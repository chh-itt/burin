//! 10,000-Row Stress Test — GPU & CPU Backend
//!
//! Brute-force mount of real elements in a ScrollView (no virtualization).
//! Each row: 28px tall, alternating background, border, CJK+Latin text.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example stress_10k                           # GPU, 10k rows
//! cargo run --example stress_10k -- cpu                    # CPU, 5k rows (auto)
//! cargo run --example stress_10k -- cpu 8000               # CPU, 8k rows
//! cargo run --example stress_10k -- gpu 20000              # GPU, 20k rows
//! AURALIS_CPU_PERF=1 cargo run --example stress_10k -- cpu # CPU perf counters
//! ```
//!
//! ## Expected behavior
//!
//! | Backend | Count | Mount | First frame | Scroll |
//! |---------|-------|-------|-------------|--------|
//! | GPU     | 10k   | ~9ms  | ~50ms       | smooth |
//! | CPU     | 5k    | ~5ms  | ~200ms      | usable |
//! | CPU     | 10k   | ~9ms  | slow/frozen | avoid  |
//!
//! CPU backend first-frame freeze is expected: taffy O(N) layout + CPU glyph
//! rasterisation + sequential pixel blit.  Env `AURALIS_CPU_PERF=1` prints
//! per-phase timing (gen/raster/present) in the console for diagnosis.

use std::env;
use std::time::Instant;

use burin::core::Compositor;
use burin::platform::{App, WindowConfig};
use burin::render::RendererChoice;
use burin::style::{Color, Padding, Styled};
use burin::theme::M3Theme;
use burin::widgets::display::Text;
use burin::widgets::layout::{ScrollDirection, ScrollView, VStack};

const ROW_HEIGHT: f32 = 28.0;
const ROW_WIDTH: f32 = 940.0;

const ALT_BG_A: Color = Color::rgba8(0x1a, 0x1a, 0x24, 0xFF);
const ALT_BG_B: Color = Color::rgba8(0x20, 0x20, 0x2c, 0xFF);
const BORDER: Color = Color::rgba8(0x30, 0x30, 0x40, 0xFF);

fn build_rows(count: usize) -> VStack {
    eprintln!(
        "[stress_10k] Mounting {count} rows ({} elements total)...",
        count
    );
    let t0 = Instant::now();

    let mut stack = VStack::new();
    for i in 0..count {
        let bg = if i & 1 == 0 { ALT_BG_A } else { ALT_BG_B };
        let row = Text::new(format!(
            "Row #{i:05}  │  The quick brown fox jumps over the lazy dog.  ··  敏捷的棕色狐狸跳过了懒狗。"
        ))
        .width(ROW_WIDTH)
        .height(ROW_HEIGHT)
        .background(bg)
        .border_width(1.0)
        .border_color(BORDER)
        .font_size(13.0)
        .padding(Padding::symmetric(12.0, 0.0));
        stack = stack.push(row);
    }

    let elapsed = t0.elapsed();
    eprintln!(
        "[stress_10k] Mount complete in {:.2?} ({:.2} µs/row, {:.0} rows/s)",
        elapsed,
        elapsed.as_micros() as f64 / count as f64,
        count as f64 / elapsed.as_secs_f64()
    );
    stack
}

fn app(row_count: usize) -> impl burin::core::Widget {
    let count = row_count;
    Compositor::new(move |_scope| {
        eprintln!(
            "[stress_10k] First frame incoming — {} rows on {} backend.  \
             CPU may appear frozen on first frame (taffy layout + glyph raster).",
            count,
            if cfg!(feature = "backend-tiny-skia") && !cfg!(feature = "backend-wgpu") {
                "CPU"
            } else {
                "GPU"
            }
        );
        ScrollView::new()
            .scroll_direction(ScrollDirection::Vertical)
            .scrollbar_width(8.0)
            .child(build_rows(count))
    })
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let is_cpu = args.iter().any(|a| a == "cpu");
    let (backend, default_count) = if is_cpu {
        (RendererChoice::Cpu, 5_000usize)
    } else {
        (RendererChoice::Auto, 10_000usize)
    };

    let row_count: usize = args
        .iter()
        .filter_map(|a| a.parse::<usize>().ok())
        .next()
        .unwrap_or(default_count);

    eprintln!(
        "[stress_10k] Backend: {backend:?}, rows: {row_count}  \
         (GPU default=10k, CPU default=5k; pass count as arg to override)"
    );

    if is_cpu {
        eprintln!(
            "[stress_10k] Tip: set AURALIS_CPU_PERF=1 for per-phase timing \
             (gen/raster/present) printed every 60 frames"
        );
    }

    let seed = Color::rgba8(0x67, 0x79, 0xE8, 0xFF);

    App::new()
        .window(
            WindowConfig {
                title: format!(
                    "Auralis Stress — {row_count} rows ({}) ",
                    if is_cpu { "CPU" } else { "GPU" }
                ),
                width: 960.0,
                height: 900.0,
                theme: M3Theme::from_seed(seed)
                    .preset(burin::theme::PresetTheme::neo_minimal_slate()),
                backend,
                ..Default::default()
            },
            app(row_count),
        )
        .run()
        .expect("run");
}
