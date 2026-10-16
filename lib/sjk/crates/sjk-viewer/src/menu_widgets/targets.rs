//! Pointer targets every screen shares (the way back, the tabs) and where along
//! a row a click lands: a row's value zone starts at [`VALUE_START`] of its width,
//! a cycler's left half steps back, a slider's rail ends before its value column.

use sjk_ui::Rect;

/// Fraction of a row where the value zone (slider, toggle, cycler) starts.
pub(crate) const VALUE_START: f32 = 0.52;

/// Token of a screen's way back (its Esc cap or Back button).
pub(crate) const BACK_TOKEN: u16 = 900;

/// First tab token; tab `i` is `TAB_BASE + i`.
pub(crate) const TAB_BASE: u16 = 500;

/// Width (at scale 1) of the value text column a slider keeps to the
/// right of its rail.
pub(crate) const SLIDER_VALUE_COLUMN: f32 = 72.0;

/// Where along a row's value zone the pointer is, 0 at the zone's left
/// edge and 1 at the row's right edge.
pub(crate) fn zone_ratio(rect: Rect, x: f32) -> f32 {
    let start = rect.x + rect.width * VALUE_START;
    ((x - start) / (rect.right() - start)).clamp(0.0, 1.0)
}

/// Where along a slider's rail the pointer is, 0 at its left end and 1 at
/// its right end (the value column past it counts as 1), so the knob lands
/// under the pointer when a slider is clicked or dragged at UI `scale`.
pub(crate) fn slider_ratio(rect: Rect, x: f32, scale: f32) -> f32 {
    let start = rect.x + rect.width * VALUE_START;
    let end = rect.right() - SLIDER_VALUE_COLUMN * scale;
    ((x - start) / (end - start).max(1.0)).clamp(0.0, 1.0)
}

/// Which of `count` equal palette cells across a row's value zone the
/// pointer at `x` is in.
pub(crate) fn palette_index(rect: Rect, x: f32, count: usize) -> usize {
    ((zone_ratio(rect, x) * count as f32) as usize).min(count.saturating_sub(1))
}

/// Which way a pointer click at `x` on a cycler row `rect` turns: the left
/// half of the value zone steps back, the right half forward.
pub(crate) fn cycler_direction(rect: Rect, x: f32) -> isize {
    let start = rect.x + rect.width * VALUE_START;
    if x < (start + rect.right()) * 0.5 {
        -1
    } else {
        1
    }
}
