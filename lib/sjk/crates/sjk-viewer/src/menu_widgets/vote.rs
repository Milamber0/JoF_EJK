//! Shared floating vote geometry for the HUD and headless evidence.

use sjk_ui::Rect;

/// Backplate-free vote block, sized in hero design pixels.
pub(crate) struct VoteLayout {
    /// Narrow context accent, never a text background.
    pub(crate) accent: Rect,
    /// Vote kind and countdown.
    pub(crate) heading: Rect,
    /// Proposal and yes/no totals.
    pub(crate) body: Rect,
    /// Quiet binding hints.
    pub(crate) hint: Rect,
}

impl VoteLayout {
    /// Lay out a vote at the HUD anchor with viewport-scaled spacing.
    pub(crate) fn new(rect: Rect, scale: f32) -> Self {
        let x = rect.x + 16.0 * scale;
        let width = rect.width - 16.0 * scale;
        Self {
            accent: Rect::new(rect.x, rect.y + 3.0 * scale, 4.0 * scale, 56.0 * scale),
            heading: Rect::new(x, rect.y, width, 22.0 * scale),
            body: Rect::new(x, rect.y + 28.0 * scale, width, 30.0 * scale),
            hint: Rect::new(x, rect.y + 66.0 * scale, width, 18.0 * scale),
        }
    }
}
