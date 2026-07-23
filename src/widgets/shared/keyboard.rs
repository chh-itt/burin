use crate::event::action::ActionKind;

/// Result of computing the next row index from a keyboard action.
#[derive(Debug, PartialEq, Eq)]
pub enum RowNavOutcome {
    /// Navigate to `new_idx` (may equal `focused` for Home/End).
    Navigate(usize),
    /// Activate the currently focused row (e.g. Enter / Space).
    Activate,
    /// Action was not handled.
    Unhandled,
}

/// Compute the target row index for a keyboard navigation action.
///
/// * `cur_len`  — total number of rows (0 = empty).
/// * `focused`  — currently focused row index (0-based).
/// * `skip`     — predicate that returns `true` for rows that should be
///                skipped (disabled, hidden, etc.).
pub fn row_nav(
    kind: ActionKind,
    cur_len: usize,
    focused: usize,
    skip: impl Fn(usize) -> bool,
) -> RowNavOutcome {
    if cur_len == 0 {
        return RowNavOutcome::Unhandled;
    }

    let find_enabled = |start: usize, dir: isize| -> Option<usize> {
        for off in 0..cur_len {
            let idx = if dir > 0 {
                (start + off) % cur_len
            } else {
                (start + cur_len - off) % cur_len
            };
            if !skip(idx) {
                return Some(idx);
            }
        }
        None
    };

    match kind {
        ActionKind::MoveDown => find_enabled((focused + 1) % cur_len, 1)
            .map(RowNavOutcome::Navigate)
            .unwrap_or(RowNavOutcome::Unhandled),
        ActionKind::MoveUp => find_enabled((focused + cur_len - 1) % cur_len, -1)
            .map(RowNavOutcome::Navigate)
            .unwrap_or(RowNavOutcome::Unhandled),
        ActionKind::MoveHome => find_enabled(0, 1)
            .map(RowNavOutcome::Navigate)
            .unwrap_or(RowNavOutcome::Unhandled),
        ActionKind::MoveEnd => find_enabled(cur_len - 1, -1)
            .map(RowNavOutcome::Navigate)
            .unwrap_or(RowNavOutcome::Unhandled),
        ActionKind::MovePageDown => find_enabled((focused + 10).min(cur_len - 1), 1)
            .map(RowNavOutcome::Navigate)
            .unwrap_or(RowNavOutcome::Unhandled),
        ActionKind::MovePageUp => find_enabled(focused.saturating_sub(10), -1)
            .map(RowNavOutcome::Navigate)
            .unwrap_or(RowNavOutcome::Unhandled),
        ActionKind::Activate => RowNavOutcome::Activate,
        _ => RowNavOutcome::Unhandled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: fn(usize) -> bool = |_| false;

    #[test]
    fn empty_list_is_unhandled() {
        assert_eq!(
            row_nav(ActionKind::MoveDown, 0, 0, NONE),
            RowNavOutcome::Unhandled
        );
    }

    #[test]
    fn move_down_basic_and_wraps() {
        assert_eq!(
            row_nav(ActionKind::MoveDown, 3, 0, NONE),
            RowNavOutcome::Navigate(1)
        );
        // From the last row, wraps back to the first.
        assert_eq!(
            row_nav(ActionKind::MoveDown, 3, 2, NONE),
            RowNavOutcome::Navigate(0)
        );
    }

    #[test]
    fn move_up_basic_and_wraps() {
        assert_eq!(
            row_nav(ActionKind::MoveUp, 3, 2, NONE),
            RowNavOutcome::Navigate(1)
        );
        // From the first row, wraps to the last.
        assert_eq!(
            row_nav(ActionKind::MoveUp, 3, 0, NONE),
            RowNavOutcome::Navigate(2)
        );
    }

    #[test]
    fn navigation_skips_disabled_rows() {
        // Row 1 disabled: Down from 0 lands on 2.
        assert_eq!(
            row_nav(ActionKind::MoveDown, 3, 0, |i| i == 1),
            RowNavOutcome::Navigate(2)
        );
        // Row 1 disabled: Up from 2 lands on 0.
        assert_eq!(
            row_nav(ActionKind::MoveUp, 3, 2, |i| i == 1),
            RowNavOutcome::Navigate(0)
        );
    }

    #[test]
    fn home_and_end_respect_disabled() {
        assert_eq!(
            row_nav(ActionKind::MoveHome, 3, 2, NONE),
            RowNavOutcome::Navigate(0)
        );
        assert_eq!(
            row_nav(ActionKind::MoveEnd, 3, 0, NONE),
            RowNavOutcome::Navigate(2)
        );
        // Home with row 0 disabled lands on the first enabled row.
        assert_eq!(
            row_nav(ActionKind::MoveHome, 3, 2, |i| i == 0),
            RowNavOutcome::Navigate(1)
        );
        // End with last row disabled lands on the last enabled row.
        assert_eq!(
            row_nav(ActionKind::MoveEnd, 3, 0, |i| i == 2),
            RowNavOutcome::Navigate(1)
        );
    }

    #[test]
    fn activate_and_unrelated_kinds() {
        assert_eq!(
            row_nav(ActionKind::Activate, 3, 0, NONE),
            RowNavOutcome::Activate
        );
        assert_eq!(
            row_nav(ActionKind::Copy, 3, 0, NONE),
            RowNavOutcome::Unhandled
        );
    }

    #[test]
    fn all_disabled_is_unhandled() {
        assert_eq!(
            row_nav(ActionKind::MoveDown, 3, 0, |_| true),
            RowNavOutcome::Unhandled
        );
    }
}
