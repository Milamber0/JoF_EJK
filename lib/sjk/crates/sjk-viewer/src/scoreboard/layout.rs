//! The chat column kept beside the scoreboard, so match results and conversation
//! stay apart at any viewport size.
pub(crate) struct Layout {
    pub(crate) scale: f32,
    pub(crate) chat_left: f32,
    pub(crate) chat_width: f32,
}

impl Layout {
    pub(crate) fn new(viewport: [f32; 2]) -> Self {
        let scale = (viewport[1] / crate::ui_scale::REFERENCE_HEIGHT)
            .min(viewport[0] / 1400.0)
            .min(crate::ui_scale::MAX);
        Self {
            scale,
            chat_left: 40.0 * scale,
            chat_width: (viewport[0] * 0.32).min(600.0 * scale),
        }
    }
}
