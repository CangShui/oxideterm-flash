use gpui::{Pixels, Point, ScrollHandle, ScrollWheelEvent, px};

/// Clears a stale horizontal offset once the previously measured content no
/// longer overflows the container, so a path that shrank cannot stay
/// translated out of view. Returns true when the offset changed.
pub(in crate::workspace) fn reset_breadcrumb_scroll_if_content_fits(
    scroll_handle: &ScrollHandle,
) -> bool {
    let offset = scroll_handle.offset();
    breadcrumb_scroll_reset_offset(
        f32::from(scroll_handle.max_offset().x),
        f32::from(offset.x),
        f32::from(offset.y),
    )
    .map(|next| {
        scroll_handle.set_offset(next);
        true
    })
    .unwrap_or(false)
}

/// Returns the corrective offset when previously measured content fits the
/// container while the scroll offset is still non-zero.
fn breadcrumb_scroll_reset_offset(max_scroll_x: f32, offset_x: f32, offset_y: f32) -> Option<Point<Pixels>> {
    if max_scroll_x > 0.0 {
        return None;
    }
    if offset_x == 0.0 && offset_y == 0.0 {
        return None;
    }
    Some(Point::default())
}

/// Applies dominant wheel movement to a measured horizontal breadcrumb viewport.
pub(super) fn scroll_breadcrumb_by_wheel(
    scroll_handle: &ScrollHandle,
    event: &ScrollWheelEvent,
    line_height: Pixels,
) -> Option<bool> {
    let max_scroll = f32::from(scroll_handle.max_offset().x);
    if max_scroll <= 0.0 {
        return None;
    }
    let delta = event.delta.pixel_delta(line_height);
    let wheel_delta = dominant_wheel_delta(f32::from(delta.x), f32::from(delta.y));
    if wheel_delta == 0.0 {
        return None;
    }
    let current_scroll = f32::from(-scroll_handle.offset().x).clamp(0.0, max_scroll);
    let next_scroll = breadcrumb_scroll_after_wheel(current_scroll, wheel_delta, max_scroll);
    if (next_scroll - current_scroll).abs() < 0.01 {
        // Consume boundary wheel events so the file list behind the path bar does not scroll.
        return Some(false);
    }
    // GPUI represents rightward content movement as a negative x offset.
    scroll_handle.set_offset(Point::new(px(-next_scroll), px(0.0)));
    Some(true)
}

fn dominant_wheel_delta(delta_x: f32, delta_y: f32) -> f32 {
    if delta_x.abs() > delta_y.abs() {
        delta_x
    } else {
        delta_y
    }
}

fn breadcrumb_scroll_after_wheel(current_scroll: f32, wheel_delta: f32, max_scroll: f32) -> f32 {
    (current_scroll - wheel_delta).clamp(0.0, max_scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumb_wheel_prefers_the_dominant_axis() {
        assert_eq!(dominant_wheel_delta(-12.0, -4.0), -12.0);
        assert_eq!(dominant_wheel_delta(-4.0, -12.0), -12.0);
    }

    #[test]
    fn breadcrumb_wheel_clamps_to_scroll_bounds() {
        assert_eq!(breadcrumb_scroll_after_wheel(0.0, -24.0, 100.0), 24.0);
        assert_eq!(breadcrumb_scroll_after_wheel(90.0, -24.0, 100.0), 100.0);
        assert_eq!(breadcrumb_scroll_after_wheel(10.0, 24.0, 100.0), 0.0);
    }

    #[test]
    fn breadcrumb_scroll_resets_only_when_content_fits_with_a_stale_offset() {
        // Previously measured content no longer overflows while the bar is
        // still translated: the stale offset must be cleared.
        assert_eq!(
            breadcrumb_scroll_reset_offset(0.0, -120.0, 0.0),
            Some(Point::default())
        );
        // Overflowing content keeps any scroll offset.
        assert_eq!(breadcrumb_scroll_reset_offset(80.0, -40.0, 0.0), None);
        // An already-centered bar needs no correction.
        assert_eq!(breadcrumb_scroll_reset_offset(0.0, 0.0, 0.0), None);
    }
}
