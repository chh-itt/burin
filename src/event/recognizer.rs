use crate::core::element::ElementId;
use crate::event::GesturePhase;
use crate::style::Point;

// ── Core trait ──
//
// Implementations:
// - TapRecognizer        — immediate tap
// - DragRecognizer       — drag past threshold
// - LongPressRecognizer  — hold 500ms
// - DoubleTapRecognizer  — two taps within 300ms (with optional hold mode)

pub trait Recognizer: std::any::Any {
    /// Process a pointer event. Called by the arena for each member.
    fn handle_event(&mut self, phase: GesturePhase, position: Point) -> RecognizerResult;
    /// Called from the per-frame timeout pass to check for time-based
    /// acceptance (e.g. long-press duration elapsed). Default: Possible.
    fn handle_timeout(&mut self) -> RecognizerResult {
        RecognizerResult::Possible
    }
    /// The instant at which this recognizer wants a timeout check, if any.
    /// The arena aggregates these into a discrete scheduler deadline so a
    /// motionless hold fires even from a sleeping event loop.
    fn timeout_deadline(&self) -> Option<web_time::Instant> {
        None
    }
    /// Reset to initial state (e.g. on pointer cancel).
    fn reset(&mut self);
    /// Clone into box for arena storage.
    fn clone_box(&self) -> Box<dyn Recognizer>;
}

pub enum RecognizerResult {
    /// This gesture cannot happen anymore — remove from arena.
    Impossible,
    /// Still possible, haven't decided yet.
    Possible,
    /// I'm certain — claim the arena.
    Accepted,
}

// ── Shared constants ──
//
// SINGLE SOURCE OF TRUTH for gesture thresholds (audit 2026-07-19: these
// were split three ways — translator 6px, ClickCounter 6px/400ms,
// recognizer 8px/300ms — so the same physical motion could be a drag in
// one system and a tap in another). translator.rs and click.rs re-use
// these constants.

pub const TAP_TIMEOUT_MS: u64 = 300;
pub const TAP_DRAG_THRESHOLD: f32 = 6.0;
pub const LONG_PRESS_DURATION_MS: u64 = 500;
pub const DOUBLE_TAP_INTERVAL_MS: u64 = 400;

// ── TapRecognizer ──

#[derive(Clone)]
pub struct TapRecognizer {
    start_position: Option<Point>,
    start_time: Option<web_time::Instant>,
    /// Distance threshold before we reject.
    threshold: f32,
    /// Max time before we reject.
    timeout_ms: u64,
}

impl TapRecognizer {
    pub fn new() -> Self {
        Self {
            start_position: None,
            start_time: None,
            threshold: TAP_DRAG_THRESHOLD,
            timeout_ms: TAP_TIMEOUT_MS,
        }
    }
}

impl Recognizer for TapRecognizer {
    fn handle_event(&mut self, phase: GesturePhase, position: Point) -> RecognizerResult {
        match phase {
            GesturePhase::Started => {
                self.start_position = Some(position);
                self.start_time = Some(crate::core::clock::now());
                RecognizerResult::Possible
            }
            GesturePhase::Moved => {
                let d = self.start_position.map_or(0.0, |s| position.distance(&s));
                if d > self.threshold {
                    RecognizerResult::Impossible // became a drag
                } else {
                    // Check timeout
                    if let Some(t) = self.start_time {
                        if crate::core::clock::now()
                            .saturating_duration_since(t)
                            .as_millis() as u64
                            > self.timeout_ms
                        {
                            return RecognizerResult::Impossible;
                        }
                    }
                    RecognizerResult::Possible
                }
            }
            GesturePhase::Ended => {
                let d = self
                    .start_position
                    .map_or(f32::MAX, |s| position.distance(&s));
                if d <= self.threshold {
                    RecognizerResult::Accepted // clean tap!
                } else {
                    RecognizerResult::Impossible
                }
            }
            GesturePhase::Cancelled => {
                self.reset();
                RecognizerResult::Impossible
            }
        }
    }

    fn reset(&mut self) {
        self.start_position = None;
        self.start_time = None;
    }

    fn clone_box(&self) -> Box<dyn Recognizer> {
        Box::new(self.clone())
    }
}

// ── DragRecognizer ──

#[derive(Clone)]
pub struct DragRecognizer {
    start_position: Option<Point>,
    threshold: f32,
    has_accepted: bool,
}

impl DragRecognizer {
    pub fn new() -> Self {
        Self {
            start_position: None,
            threshold: TAP_DRAG_THRESHOLD,
            has_accepted: false,
        }
    }
}

impl Recognizer for DragRecognizer {
    fn handle_event(&mut self, phase: GesturePhase, position: Point) -> RecognizerResult {
        match phase {
            GesturePhase::Started => {
                self.start_position = Some(position);
                self.has_accepted = false;
                RecognizerResult::Possible
            }
            GesturePhase::Moved => {
                if self.has_accepted {
                    return RecognizerResult::Accepted;
                }
                let d = self.start_position.map_or(0.0, |s| position.distance(&s));
                if d > self.threshold {
                    self.has_accepted = true;
                    RecognizerResult::Accepted
                } else {
                    RecognizerResult::Possible
                }
            }
            GesturePhase::Ended | GesturePhase::Cancelled => {
                self.reset();
                if self.has_accepted {
                    RecognizerResult::Accepted
                } else {
                    RecognizerResult::Impossible
                }
            }
        }
    }

    fn reset(&mut self) {
        self.start_position = None;
        self.has_accepted = false;
    }

    fn clone_box(&self) -> Box<dyn Recognizer> {
        Box::new(self.clone())
    }
}

// ── LongPressRecognizer ──

#[derive(Clone)]
pub struct LongPressRecognizer {
    start_position: Option<Point>,
    start_time: Option<web_time::Instant>,
    threshold: f32,
    duration_ms: u64,
}

impl LongPressRecognizer {
    pub fn new() -> Self {
        Self {
            start_position: None,
            start_time: None,
            threshold: TAP_DRAG_THRESHOLD,
            duration_ms: LONG_PRESS_DURATION_MS,
        }
    }
}

impl Recognizer for LongPressRecognizer {
    fn handle_event(&mut self, phase: GesturePhase, position: Point) -> RecognizerResult {
        match phase {
            GesturePhase::Started => {
                self.start_position = Some(position);
                self.start_time = Some(crate::core::clock::now());
                RecognizerResult::Possible
            }
            GesturePhase::Moved => {
                let d = self.start_position.map_or(0.0, |s| position.distance(&s));
                if d > self.threshold {
                    RecognizerResult::Impossible // moved too much
                } else {
                    // Check if duration elapsed
                    if let Some(t) = self.start_time {
                        if crate::core::clock::now()
                            .saturating_duration_since(t)
                            .as_millis() as u64
                            >= self.duration_ms
                        {
                            return RecognizerResult::Accepted;
                        }
                    }
                    RecognizerResult::Possible
                }
            }
            GesturePhase::Ended => {
                // If user released before long-press duration, reject
                let elapsed = self.start_time.map_or(0, |t| {
                    crate::core::clock::now()
                        .saturating_duration_since(t)
                        .as_millis() as u64
                });
                if elapsed >= self.duration_ms {
                    RecognizerResult::Accepted
                } else {
                    RecognizerResult::Impossible
                }
            }
            GesturePhase::Cancelled => {
                self.reset();
                RecognizerResult::Impossible
            }
        }
    }

    fn handle_timeout(&mut self) -> RecognizerResult {
        if let Some(t) = self.start_time {
            if crate::core::clock::now()
                .saturating_duration_since(t)
                .as_millis() as u64
                >= self.duration_ms
            {
                return RecognizerResult::Accepted;
            }
        }
        RecognizerResult::Possible
    }

    fn timeout_deadline(&self) -> Option<web_time::Instant> {
        self.start_time
            .map(|t| t + std::time::Duration::from_millis(self.duration_ms))
    }

    fn reset(&mut self) {
        self.start_position = None;
        self.start_time = None;
    }

    fn clone_box(&self) -> Box<dyn Recognizer> {
        Box::new(self.clone())
    }
}

// ── DoubleTapRecognizer ──
//
// Flutter's approach: delay single-tap by 300ms before dispatching.
// Our approach (simpler): fire single-tap immediately, then cancel it
// if second tap arrives within 300ms. This avoids the 300ms delay
// on single taps at the cost of "single → double" correction.
//
// If you need the Flutter behavior (no correction, always correct):
// use DoubleTapRecognizer in "delayed" mode by setting hold_single = true.
// This delays single-tap by 300ms.

#[derive(Clone)]
pub struct DoubleTapRecognizer {
    /// State of first tap attempt
    first_tap: Option<(Point, web_time::Instant)>,
    /// Whether we've already accepted (first tap already dispatched as single)
    fired_single: bool,
    /// Distance threshold for each tap
    threshold: f32,
    /// Max interval between taps
    interval_ms: u64,
    /// If true: hold first tap for interval_ms before deciding single vs double
    /// If false: fire single immediately, cancel if double arrives
    hold_single: bool,
    /// Pending: is there a second tap received while waiting?
    second_tap: Option<Point>,
}

impl DoubleTapRecognizer {
    pub fn new(hold_single: bool) -> Self {
        Self {
            first_tap: None,
            fired_single: false,
            threshold: TAP_DRAG_THRESHOLD,
            interval_ms: DOUBLE_TAP_INTERVAL_MS,
            hold_single,
            second_tap: None,
        }
    }
}

impl Recognizer for DoubleTapRecognizer {
    fn handle_event(&mut self, phase: GesturePhase, position: Point) -> RecognizerResult {
        match phase {
            GesturePhase::Started => {
                let now = crate::core::clock::now();

                // If we have a first tap and this is within interval → second tap!
                if let Some((pos, time)) = self.first_tap {
                    if position.distance(&pos) <= self.threshold
                        && crate::core::clock::now()
                            .saturating_duration_since(time)
                            .as_millis() as u64
                            <= self.interval_ms
                    {
                        self.second_tap = Some(position);
                        return RecognizerResult::Possible;
                    }
                }

                // New first tap
                self.first_tap = Some((position, now));
                self.fired_single = false;
                self.second_tap = None;
                RecognizerResult::Possible
            }
            GesturePhase::Moved => {
                // Check if we're in second-tap mode
                if self.second_tap.is_some() {
                    let d = self.second_tap.map_or(f32::MAX, |s| position.distance(&s));
                    if d > self.threshold {
                        self.second_tap = None;
                        self.reset();
                        return RecognizerResult::Impossible;
                    }
                    // Check timeout — if too much time passed since first tap
                    if let Some((_, time)) = self.first_tap {
                        if crate::core::clock::now()
                            .saturating_duration_since(time)
                            .as_millis() as u64
                            > self.interval_ms * 2
                        {
                            self.reset();
                            return RecognizerResult::Impossible;
                        }
                    }
                    return RecognizerResult::Possible;
                }

                // Normal mode: check if first tap is still valid
                if let Some((pos, _)) = self.first_tap {
                    if position.distance(&pos) > self.threshold {
                        self.reset();
                        return RecognizerResult::Impossible;
                    }
                    // Check timeout
                    if let Some((_, time)) = self.first_tap {
                        if crate::core::clock::now()
                            .saturating_duration_since(time)
                            .as_millis() as u64
                            > self.interval_ms * 2
                        {
                            self.reset();
                            return RecognizerResult::Impossible;
                        }
                    }
                }
                RecognizerResult::Possible
            }
            GesturePhase::Ended => {
                if let Some(pos) = self.second_tap.take() {
                    // Second tap completed → double-tap!
                    if position.distance(&pos) <= self.threshold {
                        self.reset();
                        return RecognizerResult::Accepted;
                    }
                    self.reset();
                    return RecognizerResult::Impossible;
                }

                // First tap just ended
                if let Some((pos, _)) = self.first_tap {
                    if position.distance(&pos) <= self.threshold {
                        if self.hold_single {
                            // Delayed: don't accept yet, wait for interval
                            // The caller will check timeouts via process_timeouts
                            RecognizerResult::Possible
                        } else {
                            // Immediate: accept single, can be corrected later
                            self.fired_single = true;
                            RecognizerResult::Accepted
                        }
                    } else {
                        self.reset();
                        RecognizerResult::Impossible
                    }
                } else {
                    RecognizerResult::Impossible
                }
            }
            GesturePhase::Cancelled => {
                self.reset();
                RecognizerResult::Impossible
            }
        }
    }

    fn reset(&mut self) {
        self.first_tap = None;
        self.fired_single = false;
        self.second_tap = None;
    }

    fn clone_box(&self) -> Box<dyn Recognizer> {
        Box::new(self.clone())
    }
}

// ── EagerDragRecognizer ──
//
// Zero-threshold drag: accepts at PointerDown. This is the historical
// "press = grab" contract that sliders, text selection, color pickers and
// split panes depend on — they trade tap-vs-drag disambiguation for
// instant response (DragArbitration::Eager, the default).

#[derive(Clone)]
pub struct EagerDragRecognizer;

impl EagerDragRecognizer {
    pub fn new() -> Self {
        Self
    }
}

impl Recognizer for EagerDragRecognizer {
    fn handle_event(&mut self, phase: GesturePhase, _position: Point) -> RecognizerResult {
        match phase {
            GesturePhase::Started | GesturePhase::Moved => RecognizerResult::Accepted,
            GesturePhase::Ended | GesturePhase::Cancelled => RecognizerResult::Impossible,
        }
    }

    fn reset(&mut self) {}

    fn clone_box(&self) -> Box<dyn Recognizer> {
        Box::new(self.clone())
    }
}

// ── ScrollRecognizer ──
//
// Touch-first scrolling: on touch, a single finger dragging a scroll
// container IS the scroll gesture. Mouse pointers never activate this
// (desktop scrolls via wheel/scrollbar) — the arena filters Scroll-kind
// members to touch pointers at PointerDown.
//
// Direction verdict at TOUCH_SLOP: the dominant axis of the first
// movement past the slop must be scrollable by the container, otherwise
// the recognizer bows out (a horizontal swipe on a vertical list belongs
// to someone else).

/// Android ViewConfiguration-style touch slop. Deliberately larger than
/// the 6px mouse threshold — fingers jitter. Tune on-device in W3.
pub const TOUCH_SLOP: f32 = 8.0;

#[derive(Clone)]
pub struct ScrollRecognizer {
    start: Option<Point>,
    can_vertical: bool,
    can_horizontal: bool,
    slop: f32,
}

impl ScrollRecognizer {
    pub fn new(can_horizontal: bool, can_vertical: bool) -> Self {
        Self {
            start: None,
            can_vertical,
            can_horizontal,
            slop: TOUCH_SLOP,
        }
    }
}

impl Recognizer for ScrollRecognizer {
    fn handle_event(&mut self, phase: GesturePhase, position: Point) -> RecognizerResult {
        match phase {
            GesturePhase::Started => {
                self.start = Some(position);
                RecognizerResult::Possible
            }
            GesturePhase::Moved => {
                let Some(s) = self.start else {
                    return RecognizerResult::Impossible;
                };
                let dx = position.x - s.x;
                let dy = position.y - s.y;
                if dx.hypot(dy) <= self.slop {
                    return RecognizerResult::Possible;
                }
                let vertical_intent = dy.abs() >= dx.abs();
                if (vertical_intent && self.can_vertical)
                    || (!vertical_intent && self.can_horizontal)
                {
                    RecognizerResult::Accepted
                } else {
                    RecognizerResult::Impossible
                }
            }
            GesturePhase::Ended | GesturePhase::Cancelled => {
                self.reset();
                RecognizerResult::Impossible
            }
        }
    }

    fn reset(&mut self) {
        self.start = None;
    }

    fn clone_box(&self) -> Box<dyn Recognizer> {
        Box::new(self.clone())
    }
}

// ── Arena ──
//
// Semantics (audit 2026-07-19 G1 rewrite — Flutter-arena-style):
// - Every element may register MULTIPLE recognizers (drag + long-press +
//   tap coexist; the old single-slot registry silently overwrote).
// - PointerDown opens an arena for the pointer and feeds every member.
//   There is NO single-member fast path: a lone LongPressRecognizer must
//   still wait its 500ms (the old fast path fired long-press ON PRESS).
// - Moved feeds members: Accepted → win (highest priority among this
//   round's acceptors), Impossible → eliminated. No last-man-standing on
//   Moved — a lone survivor still has to earn acceptance (or win the
//   sweep on release).
// - Ended feeds members, then SWEEPS: acceptors win by priority; if none
//   accepted, nobody wins (a 200ms motionless release is neither tap nor
//   long-press for gesture members — the Click pipeline handles it).
// - Timeouts: recognizers expose `timeout_deadline()`; the arena folds
//   the earliest into a discrete scheduler wake so a motionless hold
//   fires from a sleeping event loop. `process_timeouts` runs in the
//   frame prepass.
// - Cancelled resets ONLY the cancelled pointer's members (the old code
//   reset every recognizer in the registry across all pointers).

use rustc_hash::FxHashMap;

/// What family a registration belongs to — the arena reports the winning
/// kind so event synthesis (drag capture, tap → Click) can react without
/// downcasting recognizers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecognizerKind {
    Tap,
    Drag,
    LongPress,
    DoubleTap,
    /// Touch-only container scrolling (single-finger drag). Never joins
    /// mouse-pointer arenas.
    Scroll,
    Custom,
}

/// An arena resolution: which element won, and with what gesture family.
#[derive(Clone, Copy, Debug)]
pub struct GestureWin {
    pub element_id: ElementId,
    pub kind: RecognizerKind,
}

/// Per-element recognizer registration.
pub struct RecognizerRegistration {
    pub recognizer: Box<dyn Recognizer>,
    pub priority: u16,
    pub kind: RecognizerKind,
    /// Called when this recognizer wins the arena.
    /// First argument: the winning element ID.
    /// Second argument: the GesturePhase that triggered the win.
    pub on_accept: Option<Box<dyn FnMut(ElementId, GesturePhase)>>,
    generation: u64,
}

/// A pointer's live scroll capture: the winning container plus velocity
/// tracking samples for the release fling.
pub(crate) struct ScrollCaptureState {
    pub eid: ElementId,
    pub last_pos: Point,
    /// (instant, position) ring — the release velocity is computed from
    /// the samples inside the trailing 100ms window.
    pub samples: Vec<(web_time::Instant, Point)>,
}

/// Gesture arena state: per-element recognizer registrations, active arenas,
/// pending long-press wins, drag captures, and a monotonic generation
/// counter. Owned by `AppContext` — no thread_local in this module.
#[derive(Default)]
pub struct GestureDomain {
    pub(crate) registry: FxHashMap<ElementId, Vec<RecognizerRegistration>>,
    pub(crate) arenas: FxHashMap<u64, ArenaState>,
    pub(crate) pending_wins: Vec<ElementId>,
    /// Per-pointer drag capture: set when a Drag-kind registration wins,
    /// cleared on PointerUp/Cancel. Drag updates route here regardless of
    /// the hit path (mid-drag wandering must not lose the gesture).
    pub(crate) drag_captures: FxHashMap<u64, ElementId>,
    /// Per-pointer scroll capture (Scroll-kind win): moves apply offsets,
    /// release feeds tracked velocity into the fling.
    pub(crate) scroll_captures: FxHashMap<u64, ScrollCaptureState>,
    /// Pointer sequences whose click synthesis is suppressed (a non-Tap
    /// gesture won — scrolling over a button must not press it).
    pub(crate) click_suppressed: rustc_hash::FxHashSet<u64>,
    next_generation: u64,
}

impl GestureDomain {
    fn next_gen(&mut self) -> u64 {
        let v = self.next_generation;
        self.next_generation = v.wrapping_add(1);
        v
    }
}

fn with_gesture_domain_mut<R>(f: impl FnOnce(&mut GestureDomain) -> R) -> R {
    let app = crate::core::app_context::current_app();
    let mut guard = app.gesture.borrow_mut();
    f(&mut guard)
}

/// Scheduler key for gesture timeout wakes (long-press from sleep).
const GESTURE_TIMEOUT_KEY: u64 = 0x6E57_0000;

pub(crate) fn fire_on_accept(eid: ElementId, reg_idx: usize, phase: GesturePhase) {
    // Take the callback out so USER code runs with no registry borrow held —
    // it may register/unregister recognizers re-entrantly (mount/teardown
    // from a gesture win) without panicking.
    let (cb, gen) = with_gesture_domain_mut(|gd| {
        if let Some(entry) = gd.registry.get_mut(&eid).and_then(|v| v.get_mut(reg_idx)) {
            let cb = entry.on_accept.take();
            (cb, entry.generation)
        } else {
            (None, 0)
        }
    });
    if let Some(mut cb) = cb {
        cb(eid, phase);
        // Restore the callback only if the registration was NOT replaced
        // during the callback (same generation). A re-registered recognizer
        // gets a fresh generation and its on_accept is authoritative.
        with_gesture_domain_mut(|gd| {
            if let Some(r) = gd.registry.get_mut(&eid).and_then(|v| v.get_mut(reg_idx)) {
                if r.generation == gen && r.on_accept.is_none() {
                    r.on_accept = Some(cb);
                }
            }
        });
    }
}

pub fn push_long_press_win(eid: ElementId) {
    with_gesture_domain_mut(|gd| gd.pending_wins.push(eid));
}

pub fn drain_long_press_wins() -> Vec<ElementId> {
    with_gesture_domain_mut(|gd| gd.pending_wins.drain(..).collect())
}

/// Register a recognizer for `eid`. Multiple recognizers per element
/// coexist and compete in the arena (drag + long-press + tap).
pub fn register_recognizer(
    eid: ElementId,
    priority: u16,
    kind: RecognizerKind,
    recognizer: Box<dyn Recognizer>,
    on_accept: Option<Box<dyn FnMut(ElementId, GesturePhase)>>,
) {
    with_gesture_domain_mut(|gd| {
        let gen = gd.next_gen();
        gd.registry
            .entry(eid)
            .or_default()
            .push(RecognizerRegistration {
                recognizer,
                priority,
                kind,
                on_accept,
                generation: gen,
            });
    });
}

/// Remove ALL recognizers registered for `eid` (element teardown).
pub fn unregister_recognizer(eid: ElementId) {
    with_gesture_domain_mut(|gd| {
        gd.registry.remove(&eid);
    });
}

/// Remove only the recognizers of `kind` for `eid` (e.g. swapping drag
/// arbitration re-registers the Drag recognizer without disturbing an
/// element's long-press registration).
pub fn unregister_recognizer_kind(eid: ElementId, kind: RecognizerKind) {
    with_gesture_domain_mut(|gd| {
        if let Some(v) = gd.registry.get_mut(&eid) {
            v.retain(|r| r.kind != kind);
            if v.is_empty() {
                gd.registry.remove(&eid);
            }
        }
    });
}

/// Whether `eid` has a recognizer of `kind` registered.
pub fn has_recognizer_kind(eid: ElementId, kind: RecognizerKind) -> bool {
    with_gesture_domain_mut(|gd| {
        gd.registry
            .get(&eid)
            .is_some_and(|v| v.iter().any(|r| r.kind == kind))
    })
}

/// The element currently holding the drag capture for `pointer_id`, if a
/// Drag-kind registration has won this pointer's arena.
pub fn drag_capture(pointer_id: u64) -> Option<ElementId> {
    with_gesture_domain_mut(|gd| gd.drag_captures.get(&pointer_id).copied())
}

/// Clear the drag capture for `pointer_id` (PointerUp / Cancel).
pub fn clear_drag_capture(pointer_id: u64) {
    with_gesture_domain_mut(|gd| {
        gd.drag_captures.remove(&pointer_id);
    });
}

/// The scroll container captured by `pointer_id`, if a Scroll-kind
/// registration won this pointer's arena.
pub fn scroll_capture(pointer_id: u64) -> Option<ElementId> {
    with_gesture_domain_mut(|gd| gd.scroll_captures.get(&pointer_id).map(|c| c.eid))
}

/// Advance the scroll capture: returns `(container, delta since last)`
/// and records a velocity sample. `None` when the pointer holds no
/// scroll capture.
pub(crate) fn scroll_capture_advance(pointer_id: u64, pos: Point) -> Option<(ElementId, f32, f32)> {
    with_gesture_domain_mut(|gd| {
        let cap = gd.scroll_captures.get_mut(&pointer_id)?;
        let dx = pos.x - cap.last_pos.x;
        let dy = pos.y - cap.last_pos.y;
        cap.last_pos = pos;
        cap.samples.push((crate::core::clock::now(), pos));
        if cap.samples.len() > 32 {
            cap.samples.remove(0);
        }
        Some((cap.eid, dx, dy))
    })
}

/// Finish the scroll capture: returns `(container, release_velocity)`
/// in FINGER space (px/s). Fling runs in offset space — negate before
/// feeding `ScrollBundle::fling`.
pub(crate) fn scroll_capture_release(
    pointer_id: u64,
    pos: Point,
) -> Option<(ElementId, crate::style::Vec2)> {
    with_gesture_domain_mut(|gd| {
        let mut cap = gd.scroll_captures.remove(&pointer_id)?;
        let now = crate::core::clock::now();
        cap.samples.push((now, pos));
        // Velocity over the trailing 100ms window.
        let window_start = cap
            .samples
            .iter()
            .find(|(t, _)| now.saturating_duration_since(*t).as_millis() <= 100)
            .copied();
        let v = match window_start {
            Some((t0, p0)) if t0 < now => {
                let dt = now.saturating_duration_since(t0).as_secs_f32();
                if dt > 0.0005 {
                    crate::style::Vec2::new((pos.x - p0.x) / dt, (pos.y - p0.y) / dt)
                } else {
                    crate::style::Vec2::ZERO
                }
            }
            _ => crate::style::Vec2::ZERO,
        };
        Some((cap.eid, v))
    })
}

/// Whether click synthesis is suppressed for this pointer sequence
/// (a non-Tap gesture won). Consuming read — the flag resets.
pub fn take_click_suppressed(pointer_id: u64) -> bool {
    with_gesture_domain_mut(|gd| gd.click_suppressed.remove(&pointer_id))
}

#[derive(Clone, Copy)]
struct ArenaMember {
    element_id: ElementId,
    reg_idx: usize,
}

pub(crate) struct ArenaState {
    members: Vec<ArenaMember>,
}

/// Feed one member's recognizer. Missing registration (torn down
/// mid-gesture) counts as Impossible.
fn feed_member(
    registry: &mut FxHashMap<ElementId, Vec<RecognizerRegistration>>,
    m: ArenaMember,
    phase: GesturePhase,
    position: Point,
) -> RecognizerResult {
    if let Some(reg) = registry
        .get_mut(&m.element_id)
        .and_then(|v| v.get_mut(m.reg_idx))
    {
        reg.recognizer.handle_event(phase, position)
    } else {
        RecognizerResult::Impossible
    }
}

fn member_priority(
    registry: &FxHashMap<ElementId, Vec<RecognizerRegistration>>,
    m: ArenaMember,
) -> u16 {
    registry
        .get(&m.element_id)
        .and_then(|v| v.get(m.reg_idx))
        .map_or(0, |r| r.priority)
}

/// Fold the earliest member timeout deadline into a discrete scheduler
/// wake, so time-based acceptance (long-press) fires from a sleeping loop.
fn schedule_earliest_timeout(gd: &GestureDomain) {
    let mut earliest: Option<web_time::Instant> = None;
    for arena in gd.arenas.values() {
        for m in &arena.members {
            if let Some(d) = gd
                .registry
                .get(&m.element_id)
                .and_then(|v| v.get(m.reg_idx))
                .and_then(|r| r.recognizer.timeout_deadline())
            {
                earliest = Some(earliest.map_or(d, |e| e.min(d)));
            }
        }
    }
    match earliest {
        Some(d) => crate::core::scheduler::schedule_at(d, GESTURE_TIMEOUT_KEY),
        None => crate::core::scheduler::cancel(GESTURE_TIMEOUT_KEY),
    }
}

/// Main entry: call from event dispatch on PointerDown/Move/Up.
/// Returns the win (element + gesture kind) if the arena resolved on this
/// event. Drag/Scroll wins additionally set the pointer's capture.
/// `is_touch` filters Scroll-kind members (mouse never drag-scrolls).
pub fn process_pointer_event(
    hit_path: &[ElementId],
    phase: GesturePhase,
    position: Point,
    pointer_id: u64,
    is_touch: bool,
) -> Option<GestureWin> {
    match phase {
        GesturePhase::Started => {
            let winner = with_gesture_domain_mut(|gd| {
                // New pointer sequence: reset the click-suppression flag
                // (a leftover from a sequence that never synthesized a
                // click must not eat the next tap).
                gd.click_suppressed.remove(&pointer_id);
                let GestureDomain {
                    ref mut registry,
                    ref mut arenas,
                    ..
                } = *gd;
                let mut members: Vec<ArenaMember> = Vec::new();
                for eid in hit_path {
                    if let Some(regs) = registry.get(eid) {
                        for (idx, reg) in regs.iter().enumerate() {
                            if reg.kind == RecognizerKind::Scroll && !is_touch {
                                continue; // mouse never drag-scrolls
                            }
                            members.push(ArenaMember {
                                element_id: *eid,
                                reg_idx: idx,
                            });
                        }
                    }
                }
                if members.is_empty() {
                    return None;
                }

                // Feed everyone. Eager recognizers (zero-threshold drag)
                // may accept immediately — highest priority acceptor wins.
                let mut best: Option<(u16, ArenaMember)> = None;
                let mut i = 0;
                while i < members.len() {
                    match feed_member(registry, members[i], phase, position) {
                        RecognizerResult::Accepted => {
                            let p = member_priority(registry, members[i]);
                            if best.is_none_or(|(bp, _)| p > bp) {
                                best = Some((p, members[i]));
                            }
                            i += 1;
                        }
                        RecognizerResult::Impossible => {
                            members.swap_remove(i);
                        }
                        RecognizerResult::Possible => {
                            i += 1;
                        }
                    }
                }

                if let Some((_, m)) = best {
                    // Immediate win — no arena persists for this pointer.
                    return Some(m);
                }
                if !members.is_empty() {
                    arenas.insert(pointer_id, ArenaState { members });
                }
                None
            });
            let win = resolve_win(winner, phase, pointer_id, position);
            with_gesture_domain_mut(|gd| schedule_earliest_timeout(gd));
            win
        }
        GesturePhase::Moved | GesturePhase::Ended => {
            let is_end = phase == GesturePhase::Ended;
            let winner = with_gesture_domain_mut(|gd| {
                let GestureDomain {
                    ref mut registry,
                    ref mut arenas,
                    ..
                } = *gd;
                let Some(arena) = arenas.get_mut(&pointer_id) else {
                    return None;
                };

                let mut best: Option<(u16, ArenaMember)> = None;
                let mut i = 0;
                while i < arena.members.len() {
                    match feed_member(registry, arena.members[i], phase, position) {
                        RecognizerResult::Accepted => {
                            let p = member_priority(registry, arena.members[i]);
                            if best.is_none_or(|(bp, _)| p > bp) {
                                best = Some((p, arena.members[i]));
                            }
                            i += 1;
                        }
                        RecognizerResult::Impossible => {
                            arena.members.swap_remove(i);
                        }
                        RecognizerResult::Possible => {
                            i += 1;
                        }
                    }
                }
                // NOTE: no last-man-standing on Moved. A lone survivor must
                // earn acceptance (threshold / duration / release sweep) —
                // the old rule declared it winner on the first Move, firing
                // long-press on a 1-pixel jitter.

                let resolved = best.is_some() || is_end || arena.members.is_empty();
                if resolved {
                    arenas.remove(&pointer_id);
                }
                best.map(|(_, m)| m)
            });
            let win = resolve_win(winner, phase, pointer_id, position);
            with_gesture_domain_mut(|gd| schedule_earliest_timeout(gd));
            win
        }
        GesturePhase::Cancelled => {
            clear_drag_capture(pointer_id);
            with_gesture_domain_mut(|gd| {
                gd.scroll_captures.remove(&pointer_id);
                gd.click_suppressed.remove(&pointer_id);
                let GestureDomain {
                    ref mut registry,
                    ref mut arenas,
                    ..
                } = *gd;
                // Reset ONLY this pointer's members — other pointers'
                // in-flight gestures are untouched.
                if let Some(arena) = arenas.remove(&pointer_id) {
                    for m in arena.members {
                        if let Some(reg) = registry
                            .get_mut(&m.element_id)
                            .and_then(|v| v.get_mut(m.reg_idx))
                        {
                            reg.recognizer.reset();
                        }
                    }
                }
                schedule_earliest_timeout(gd);
            });
            None
        }
    }
}

/// Turn an arena member win into a `GestureWin`: fire its on_accept,
/// record the drag/scroll capture, and suppress click synthesis for
/// non-Tap wins.
fn resolve_win(
    winner: Option<ArenaMember>,
    phase: GesturePhase,
    pointer_id: u64,
    position: Point,
) -> Option<GestureWin> {
    let m = winner?;
    let kind = with_gesture_domain_mut(|gd| {
        gd.registry
            .get(&m.element_id)
            .and_then(|v| v.get(m.reg_idx))
            .map_or(RecognizerKind::Custom, |r| r.kind)
    });
    with_gesture_domain_mut(|gd| {
        if kind != RecognizerKind::Tap && kind != RecognizerKind::DoubleTap {
            gd.click_suppressed.insert(pointer_id);
        }
        match kind {
            RecognizerKind::Drag if phase != GesturePhase::Ended => {
                gd.drag_captures.insert(pointer_id, m.element_id);
            }
            RecognizerKind::Scroll if phase != GesturePhase::Ended => {
                gd.scroll_captures.insert(
                    pointer_id,
                    ScrollCaptureState {
                        eid: m.element_id,
                        last_pos: position,
                        samples: vec![(crate::core::clock::now(), position)],
                    },
                );
            }
            _ => {}
        }
    });
    fire_on_accept(m.element_id, m.reg_idx, phase);
    Some(GestureWin {
        element_id: m.element_id,
        kind,
    })
}

/// Per-frame timeout pass (wired into `run_pre_passes`): lets time-based
/// recognizers (long-press; delayed single-tap) accept without pointer
/// motion. The discrete scheduler wake from `schedule_earliest_timeout`
/// guarantees a frame arrives at the deadline even if the loop sleeps.
pub fn process_timeouts() {
    let resolved: Vec<ArenaMember> = with_gesture_domain_mut(|gd| {
        if gd.arenas.is_empty() {
            return Vec::new();
        }
        let GestureDomain {
            ref mut registry,
            ref mut arenas,
            ..
        } = *gd;
        let mut resolved = Vec::new();
        let mut finished_pointers = Vec::new();
        for (pid, arena) in arenas.iter_mut() {
            let mut best: Option<(u16, ArenaMember)> = None;
            for m in &arena.members {
                let accepted = registry
                    .get_mut(&m.element_id)
                    .and_then(|v| v.get_mut(m.reg_idx))
                    .is_some_and(|r| {
                        matches!(r.recognizer.handle_timeout(), RecognizerResult::Accepted)
                    });
                if accepted {
                    let p = member_priority(registry, *m);
                    if best.is_none_or(|(bp, _)| p > bp) {
                        best = Some((p, *m));
                    }
                }
            }
            if let Some((_, m)) = best {
                resolved.push(m);
                finished_pointers.push(*pid);
            }
        }
        for pid in finished_pointers {
            arenas.remove(&pid);
        }
        resolved
    });

    // fire_on_accept runs the registration's own callback (for long-press
    // registrations that callback IS push_long_press_win — the old code
    // additionally pushed here, double-firing every timeout win).
    for m in &resolved {
        fire_on_accept(m.element_id, m.reg_idx, GesturePhase::Moved);
    }
    if !resolved.is_empty() {
        with_gesture_domain_mut(|gd| schedule_earliest_timeout(gd));
    }
}

/// Reset all recognizers and drop all arenas (window-level cancel, e.g.
/// focus loss — NOT per-pointer cancel, use GesturePhase::Cancelled there).
pub fn reset_all() {
    with_gesture_domain_mut(|gd| {
        for regs in gd.registry.values_mut() {
            for r in regs.iter_mut() {
                r.recognizer.reset();
            }
        }
        gd.arenas.clear();
        gd.drag_captures.clear();
        gd.scroll_captures.clear();
        gd.click_suppressed.clear();
        schedule_earliest_timeout(gd);
    });
}
