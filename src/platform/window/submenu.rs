/// Compute a submenu's x and the direction it actually opened.
/// `prefer_left` is the parent menu's open direction, inherited so a deep
/// cascade keeps going the same way instead of zig-zagging back onto an
/// ancestor menu. When neither side fits, clamp into the screen — the submenu
/// stays fully visible; it may overlap the parent (an unavoidable physical
/// limit, as on Windows).
pub(crate) fn submenu_x(parent_x: f32, parent_w: f32, screen_w: f32, prefer_left: bool) -> (f32, bool) {
    let w = 220.0_f32;
    let gap = 4.0_f32;
    let right_x = parent_x + parent_w + gap;
    let left_x = parent_x - w - gap;
    let fits_right = right_x + w <= screen_w;
    let fits_left = left_x >= 0.0;
    let max_x = (screen_w - w).max(0.0);
    if prefer_left {
        if fits_left {
            (left_x, true)
        } else if fits_right {
            (right_x, false)
        } else {
            (left_x.clamp(0.0, max_x), true)
        }
    } else {
        if fits_right {
            (right_x, false)
        } else if fits_left {
            (left_x, true)
        } else {
            (right_x.clamp(0.0, max_x), false)
        }
    }
}

pub(crate) fn submenu_y(parent_y: f32, parent_h: f32, sub_h: f32, screen_h: f32) -> f32 {
    if parent_y + sub_h > screen_h {
        // Not enough room below: open upward, aligning the submenu's bottom with
        // the parent row's bottom so the two menus stay visually connected.
        // (Uses the real submenu height, not a fixed estimate — otherwise a
        // shorter submenu leaves a gap when flipped.)
        (parent_y + parent_h - sub_h).max(0.0)
    } else {
        parent_y
    }
}
