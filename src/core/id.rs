use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// A unique identifier for an Element in the widget tree.
///
/// Internally encodes an arena index (high 32 bits) plus a generation counter
/// (low 32 bits) to prevent ABA problems when arena slots are reused.
///
/// ## Encoding
/// - `index` (bits 63:32): slot position in the ElementArena's `Vec`.
/// - `generation` (bits 31:0): incremented each time the slot is reused.
///
/// A standalone (non-arena) ID uses `index = 0` and a monotonic counter as
/// its raw value; `index()` and `generation()` are not meaningful for
/// standalone IDs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "devtools", derive(serde::Serialize, serde::Deserialize))]
pub struct ElementId(pub(crate) u64);

/// Monotonic generator for standalone (test-only / non-arena) IDs.
/// Starts at `1` so `0` is reserved for the sentinel's generation slot.
static STABLE_COUNTER: AtomicU64 = AtomicU64::new(1);

impl ElementId {
    pub const SENTINEL: Self = Self(u64::MAX);

    /// Construct an arena-backed `ElementId` from a slot index and generation.
    #[inline]
    pub(crate) fn from_parts(index: u32, generation: u32) -> Self {
        Self(((index as u64) << 32) | (generation as u64))
    }

    /// Arena slot index (bits 63:32).
    #[inline]
    pub fn index(&self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Generation counter for this slot (bits 31:0).
    #[inline]
    pub fn generation(&self) -> u32 {
        self.0 as u32
    }

    /// Raw 64-bit representation.
    #[inline]
    pub fn to_u64(self) -> u64 {
        self.0
    }

    /// Construct from a raw u64 (used for serde, debugging, and external
    /// interoperability).
    #[inline]
    pub(crate) fn from_u64(raw: u64) -> Self {
        Self(raw)
    }

    /// Allocate a **standalone** (non-arena) `ElementId` for test harnesses
    /// and cross-thread signalling where no real arena slot exists.
    /// The returned ID uses `index == 0`; `index()` and `generation()` are
    /// **not meaningful** for standalone IDs.
    pub fn allocate() -> Self {
        Self(STABLE_COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for ElementId {
    fn default() -> Self {
        Self::SENTINEL
    }
}

impl fmt::Debug for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::SENTINEL {
            write!(f, "ElementId(SENTINEL)")
        } else {
            write!(
                f,
                "ElementId(idx={},gen={})",
                self.index(),
                self.generation()
            )
        }
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::SENTINEL {
            write!(f, "#SENTINEL")
        } else {
            write!(f, "#{}:{}", self.index(), self.generation())
        }
    }
}
