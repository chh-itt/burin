//! Golden / snapshot testing for `TestHarness`.
//!
//! Renders the most recent frame to pixels (via `testing::pixel`, tiny-skia,
//! fully headless), then compares against a committed baseline PNG under the
//! consuming crate's `tests/snapshots/<name>.png`.
//!
//! Default comparison is **per-pixel absolute tolerance** — the industry
//! consensus (egui/dify, Masonry, Slint) for catching real visual regressions,
//! not a perceptual hash (which misses small but real diffs). auralis' tiny-skia
//! CPU backend is highly deterministic, so a tight tolerance is safe.
//!
//! Bless via `AURALIS_UPDATE_SNAPSHOTS=1` (update failing) or `=force`
//! (update all). Failures write `<name>.new.png` + `<name>.diff.png`.
//!
//! Behind the `backend-tiny-skia` feature gate.

use crate::testing::pixel::PixelBuffer;
use std::path::PathBuf;

/// Maximum encoded baseline size — guards against committing large PNGs that
/// bloat the repo (mirrors Masonry's size cap).
const MAX_SNAPSHOT_BYTES: usize = 512 * 1024;

/// Comparison mode. Default is per-pixel; perceptual/SSIM are opt-in for the
/// rare cross-GPU fuzzy-match case.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapshotMode {
    /// Per-channel absolute tolerance (default; catches small regressions).
    PerPixel,
}

/// Snapshot comparison options (builder-style).
#[derive(Clone, Copy, Debug)]
pub struct SnapshotOptions {
    /// Per-channel absolute tolerance (0.0–1.0). Absorbs AA / backend micro-diffs.
    pub tolerance: f32,
    /// Number of differing pixels allowed before the snapshot fails.
    pub failed_pixel_count_threshold: usize,
    /// AA-aware: ignore an isolated differing pixel when a neighbour in the
    /// baseline matches the new pixel within tolerance (sub-pixel shift / AA jitter).
    pub ignore_antialiasing: bool,
    pub mode: SnapshotMode,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        SnapshotOptions {
            tolerance: 3.0 / 255.0,
            failed_pixel_count_threshold: 0,
            ignore_antialiasing: false,
            mode: SnapshotMode::PerPixel,
        }
    }
}

impl SnapshotOptions {
    pub fn tolerance(mut self, t: f32) -> Self {
        self.tolerance = t;
        self
    }
    pub fn failed_pixel_count_threshold(mut self, n: usize) -> Self {
        self.failed_pixel_count_threshold = n;
        self
    }
    pub fn ignore_antialiasing(mut self, v: bool) -> Self {
        self.ignore_antialiasing = v;
        self
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Bless {
    Off,
    Failing,
    Force,
}

fn bless_mode() -> Bless {
    match std::env::var("AURALIS_UPDATE_SNAPSHOTS").ok().as_deref() {
        Some("force") => Bless::Force,
        Some(v) if v == "1" || v.eq_ignore_ascii_case("true") => Bless::Failing,
        _ => Bless::Off,
    }
}

fn snapshot_dir(crate_dir: &str) -> PathBuf {
    PathBuf::from(crate_dir).join("tests").join("snapshots")
}

/// Compare `buf` against the committed baseline `<crate_dir>/tests/snapshots/<name>.png`.
/// Returns `Ok(())` on match (or bless), `Err(msg)` on mismatch / missing baseline.
pub fn check_snapshot(
    buf: &PixelBuffer,
    crate_dir: &str,
    name: &str,
    opts: &SnapshotOptions,
) -> Result<(), String> {
    let dir = snapshot_dir(crate_dir);
    let base_path = dir.join(format!("{name}.png"));
    let new_path = dir.join(format!("{name}.new.png"));
    let diff_path = dir.join(format!("{name}.diff.png"));
    let bless = bless_mode();

    // Size guard for anything we might write as a baseline.
    let encoded_len = buf.encode_png().map(|b| b.len()).unwrap_or(0);
    let size_ok = encoded_len <= MAX_SNAPSHOT_BYTES;

    // ── Missing baseline ──
    if !base_path.exists() {
        if bless != Bless::Off {
            if !size_ok {
                return Err(format!(
                    "snapshot '{name}' encodes to {encoded_len} bytes > cap {MAX_SNAPSHOT_BYTES}; \
                     reduce resolution before blessing"
                ));
            }
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            buf.save_png(&base_path)?;
            return Ok(());
        }
        std::fs::create_dir_all(&dir).ok();
        let _ = buf.save_png(&new_path);
        return Err(format!(
            "no baseline for snapshot '{name}'. Wrote {}. \
             Run with AURALIS_UPDATE_SNAPSHOTS=1 to create it.",
            new_path.display()
        ));
    }

    // ── Load + compare ──
    let baseline = PixelBuffer::load_png(&base_path)?;
    if baseline.width() != buf.width() || baseline.height() != buf.height() {
        if bless == Bless::Force || bless == Bless::Failing {
            buf.save_png(&base_path)?;
            return Ok(());
        }
        std::fs::create_dir_all(&dir).ok();
        let _ = buf.save_png(&new_path);
        return Err(format!(
            "snapshot '{name}' size mismatch: baseline {}x{}, got {}x{}",
            baseline.width(),
            baseline.height(),
            buf.width(),
            buf.height()
        ));
    }

    let (diff_count, diff_buf) = diff(&baseline, buf, opts);

    if diff_count > opts.failed_pixel_count_threshold {
        if bless == Bless::Failing || bless == Bless::Force {
            if !size_ok {
                return Err(format!(
                    "snapshot '{name}' encodes to {encoded_len} bytes > cap {MAX_SNAPSHOT_BYTES}"
                ));
            }
            buf.save_png(&base_path)?;
            return Ok(());
        }
        std::fs::create_dir_all(&dir).ok();
        let _ = buf.save_png(&new_path);
        if let Some(d) = diff_buf {
            let _ = d.save_png(&diff_path);
        }
        return Err(format!(
            "snapshot '{name}' mismatch: {diff_count} pixels differ (> {} allowed). \
             Wrote {} and {}. Bless with AURALIS_UPDATE_SNAPSHOTS=1.",
            opts.failed_pixel_count_threshold,
            new_path.display(),
            diff_path.display()
        ));
    }

    // Force-bless updates even a passing snapshot (e.g. within-tolerance drift).
    if bless == Bless::Force && size_ok {
        let _ = buf.save_png(&base_path);
    }
    Ok(())
}

/// Per-pixel diff. Returns (differing-pixel count, optional diff image).
fn diff(
    base: &PixelBuffer,
    new: &PixelBuffer,
    opts: &SnapshotOptions,
) -> (usize, Option<PixelBuffer>) {
    let w = base.width();
    let h = base.height();
    let ba = base.rgba_premul();
    let na = new.rgba_premul();
    let tol = (opts.tolerance * 255.0).round() as i32;

    let px = |data: &[u8], x: u32, y: u32| -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    };
    let within = |a: [u8; 4], b: [u8; 4]| -> bool {
        (0..4).all(|c| (a[c] as i32 - b[c] as i32).abs() <= tol)
    };

    let mut diff_data = vec![0u8; (w * h * 4) as usize];
    let mut count = 0usize;

    for y in 0..h {
        for x in 0..w {
            let a = px(ba, x, y);
            let b = px(na, x, y);
            let mut differs = !within(a, b);

            if differs && opts.ignore_antialiasing {
                // AA relaxation: if any 8-neighbour in the baseline matches the
                // new pixel (content shifted < 1px), treat as anti-aliasing.
                'nb: for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        if within(px(ba, nx as u32, ny as u32), b) {
                            differs = false;
                            break 'nb;
                        }
                    }
                }
            }

            let o = ((y * w + x) * 4) as usize;
            if differs {
                count += 1;
                // Bright magenta marks a differing pixel (premultiplied, opaque).
                diff_data[o] = 255;
                diff_data[o + 1] = 0;
                diff_data[o + 2] = 255;
                diff_data[o + 3] = 255;
            } else {
                // Dim the matching pixel so diffs stand out.
                diff_data[o] = a[0] / 4;
                diff_data[o + 1] = a[1] / 4;
                diff_data[o + 2] = a[2] / 4;
                diff_data[o + 3] = 255;
            }
        }
    }

    let diff_buf = PixelBuffer::new_rgba(w, h, &diff_data);
    (count, diff_buf)
}
