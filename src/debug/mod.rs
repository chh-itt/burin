//! Debug diagnostics: frame metrics, element inspector.

#[cfg(feature = "devtools")]
pub mod devtools;

use std::cell::RefCell;
use std::collections::VecDeque;

/// Per-frame performance metrics.
#[derive(Clone, Debug)]
pub struct FrameMetrics {
    pub frame_id: u64,
    pub element_count: usize,
    pub dirty_measure_count: usize,
    pub dirty_reposition_count: usize,
    pub dirty_repaint_count: usize,
    pub layout_time_us: u64,
    pub paint_time_us: u64,
    pub total_time_us: u64,
    pub fps: f32,
}

/// A ring buffer of recent frame metrics for diagnostics.
pub struct MetricsHistory {
    frames: VecDeque<FrameMetrics>,
    capacity: usize,
}

impl MetricsHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, metrics: FrameMetrics) {
        if self.frames.len() >= self.capacity {
            self.frames.pop_front();
        }
        self.frames.push_back(metrics);
    }

    /// Average FPS over the recorded frames.
    pub fn avg_fps(&self) -> f32 {
        if self.frames.is_empty() {
            return 0.0;
        }
        self.frames.iter().map(|f| f.fps).sum::<f32>() / self.frames.len() as f32
    }

    /// Average frame time in microseconds.
    pub fn avg_frame_time_us(&self) -> u64 {
        if self.frames.is_empty() {
            return 0;
        }
        self.frames.iter().map(|f| f.total_time_us).sum::<u64>() / self.frames.len() as u64
    }

    /// Most recent frame metrics.
    pub fn latest(&self) -> Option<&FrameMetrics> {
        self.frames.back()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }
}

impl Default for MetricsHistory {
    fn default() -> Self {
        Self::new(600)
    }
}

/// Over-render detector: warns when elements are repainted without visual change.
///
/// Hook into `paint_element_tree`: when an element enters the re-record path
/// (cache miss) but its `surface_gen`/`decor_gen` counters are unchanged since
/// the last cache update, the `REPAINT` dirty flag was set unnecessarily.
pub struct OverRenderDetector {
    /// (consecutive_count, last_surface_gen, last_decor_gen, context_desc)
    tracking: std::collections::HashMap<crate::core::ElementId, (u32, u64, u64, String)>,
    threshold: u32,
    warnings: Vec<String>,
}

impl OverRenderDetector {
    pub fn new(threshold: u32) -> Self {
        Self {
            tracking: std::collections::HashMap::new(),
            threshold,
            warnings: Vec::new(),
        }
    }

    pub fn check(
        &mut self,
        id: crate::core::ElementId,
        surface_gen: u64,
        decor_gen: u64,
        context: &str,
    ) {
        let entry = self
            .tracking
            .entry(id)
            .or_insert((0, surface_gen, decor_gen, String::new()));
        entry.3 = context.to_string();
        if entry.1 == surface_gen && entry.2 == decor_gen {
            entry.0 += 1;
            if entry.0 == self.threshold {
                self.warnings.push(format!(
                    "[over-render] {} ({}) repainted {} times without change (s={}, d={})",
                    id, entry.3, self.threshold, surface_gen, decor_gen,
                ));
            }
        } else {
            entry.0 = 0;
            entry.1 = surface_gen;
            entry.2 = decor_gen;
        }
    }

    pub fn drain_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

impl Default for OverRenderDetector {
    fn default() -> Self {
        Self::new(30)
    }
}

thread_local! {
    /// Thread-local over-render detector, accessible from paint_element_tree
    /// without threading a parameter through every paint call.
    pub static OVER_RENDER: RefCell<OverRenderDetector> = RefCell::new(OverRenderDetector::default());
}

/// Check whether a repaint is redundant (generation counters unchanged).
/// Hook called from paint_element_tree when entering the re-record path with
/// valid cache entries.
pub fn check_over_render(
    id: crate::core::ElementId,
    surface_gen: u64,
    decor_gen: u64,
    context: &str,
) {
    OVER_RENDER.with(|d| d.borrow_mut().check(id, surface_gen, decor_gen, context));
}

/// Drain and return accumulated over-render warnings, and reset the detector.
pub fn drain_over_render_warnings() -> Vec<String> {
    OVER_RENDER.with(|d| d.borrow_mut().drain_warnings())
}
