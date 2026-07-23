use std::cell::Cell;
use std::rc::Rc;

/// Pre-allocated slot pool with inactive tracking.
///
/// A fixed number of slots are created at mount time.  When the data length
/// changes, slots beyond the visible count are marked inactive so the
/// renderer skips them — avoiding frequent arena allocate/deallocate.
pub struct SlotPool {
    inactive: Vec<Rc<Cell<bool>>>,
}

impl SlotPool {
    pub fn new(size: usize) -> Self {
        Self {
            inactive: (0..size).map(|_| Rc::new(Cell::new(false))).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.inactive.len()
    }

    pub fn cell(&self, idx: usize) -> &Rc<Cell<bool>> {
        &self.inactive[idx]
    }

    pub fn inactive_cells(&self) -> &[Rc<Cell<bool>>] {
        &self.inactive
    }

    pub fn is_active(&self, idx: usize) -> bool {
        !self.inactive[idx].get()
    }

    pub fn set_active(&self, idx: usize, active: bool) {
        self.inactive[idx].set(!active);
    }

    pub fn set_inactive(&self, idx: usize, inactive: bool) {
        self.inactive[idx].set(inactive);
    }

    /// Sync slot visibility so the first `visible` slots are active and
    /// the rest are inactive.  Returns true if any slot changed state.
    pub fn sync_visible(&self, visible: usize) -> bool {
        let pool = self.inactive.len();
        let changed = (0..pool).any(|i| self.inactive[i].get() != (i >= visible));
        for i in 0..pool {
            self.inactive[i].set(i >= visible);
        }
        changed
    }
}
