//! Shared menu widgets built on renderer-neutral `sjk-ui` primitives.

mod input;
mod text;
mod vote;
pub(crate) use vote::VoteLayout;
mod controls;
pub(crate) mod numeric;
mod targets;

pub(crate) use targets::{BACK_TOKEN, TAB_BASE, cycler_direction, palette_index};
pub(crate) use text::TextFamily;

use sjk_ui::{Color, DrawCommand, DrawList, InputRouter, Rect, Theme, WidgetId, WidgetTree};

/// Text runs a canvas keeps a frame: the SJK UI's server browser draws about
/// 170 (six to a server row).
const MAX_TEXT: usize = 224;
/// Pointer areas a canvas keeps a frame.
pub(crate) const MAX_WIDGETS: usize = 96;
const MAX_DRAW: usize = 512;

/// Semantic action attached to one focusable screen widget.
pub(crate) type MenuToken = u16;

/// Reusable fixed-storage screen canvas and input router.
pub(crate) struct MenuCanvas {
    theme: Theme,
    draw: DrawList,
    tree: WidgetTree,
    rects: Vec<Rect>,
    tokens: Vec<MenuToken>,
    text: Vec<String>,
    /// The family of each stored text run, for screens drawn in the SJK UI's
    /// type ([`TextFamily`]); others draw every run in one font.
    families: Vec<TextFamily>,
    /// The family runs stored from now on get.
    family: TextFamily,
    text_len: usize,
    input: InputRouter,
    hovered_token: Option<MenuToken>,
    pressed_token: Option<MenuToken>,
    viewport: [f32; 2],
    /// Pointer areas and text runs this frame had no room for.
    dropped: u32,
    /// Whether a release build has logged an overflow of this canvas yet.
    overflow_logged: bool,
    /// A card laid over the page this frame ([`Self::cover`]): where it lies (window
    /// pixels) and the id of its first text run.
    cover: Option<(Rect, u32)>,
}

impl MenuCanvas {
    /// Create process-lifetime storage for one menu layer.
    pub(crate) fn new() -> Self {
        Self::with_text_capacity(128)
    }

    /// [`Self::new`] with room for `draws` draw commands, for a screen that draws
    /// more shapes than an ordinary one (the Saber page's blade swatches).
    pub(crate) fn with_draw_capacity(draws: usize) -> Self {
        Self::with_capacities(MAX_TEXT, 128, draws)
    }

    /// Reserve screen-specific text storage before entering the frame loop.
    pub(crate) fn with_text_capacity(text_bytes: usize) -> Self {
        Self::with_capacities(MAX_TEXT, text_bytes, MAX_DRAW)
    }

    /// Reserve `text_slots` text runs of `text_bytes` each and `draws` draw
    /// commands, for layers that draw more than an ordinary screen (such as a
    /// full scoreboard), before entering the frame loop.
    pub(crate) fn with_capacities(text_slots: usize, text_bytes: usize, draws: usize) -> Self {
        Self {
            theme: Theme::default(),
            draw: DrawList::new(draws),
            tree: WidgetTree::new(MAX_WIDGETS),
            rects: Vec::with_capacity(MAX_WIDGETS),
            tokens: Vec::with_capacity(MAX_WIDGETS),
            text: (0..text_slots)
                .map(|_| String::with_capacity(text_bytes))
                .collect(),
            families: vec![TextFamily::Body; text_slots],
            family: TextFamily::Body,
            text_len: 0,
            input: InputRouter::new(MAX_WIDGETS),
            hovered_token: None,
            pressed_token: None,
            viewport: [1.0, 1.0],
            dropped: 0,
            overflow_logged: false,
            cover: None,
        }
    }

    /// Where along a slider row's rail the pointer at `x` is, at the UI
    /// scale of the viewport this canvas was last begun with.
    pub(crate) fn slider_ratio(&self, rect: Rect, x: f32) -> f32 {
        targets::slider_ratio(rect, x, crate::ui_scale::height_scale(self.viewport[1]))
    }

    /// Reset retained scratch without drawing a full-screen background.
    pub(crate) fn begin_transparent(&mut self, viewport: [f32; 2]) {
        self.hovered_token = self
            .input
            .hovered()
            .and_then(|id| self.tokens.get(id.0 as usize).copied());
        self.pressed_token = self
            .input
            .pressed()
            .and_then(|id| self.tokens.get(id.0 as usize).copied());
        self.viewport = viewport;
        self.draw.clear();
        self.dropped = 0;
        self.tree.clear();
        self.rects.clear();
        self.tokens.clear();
        self.text_len = 0;
        self.family = TextFamily::Body;
        self.cover = None;
    }

    /// Lay a card over the page from here on (a hover card): text draws over every
    /// shape, so the page's glyphs under `rect` (window pixels) are left out and do not
    /// show through it; the text runs added from now on are the card's and draw in full.
    pub(crate) fn cover(&mut self, rect: Rect) {
        self.cover = Some((rect, self.text_len as u32));
    }

    /// The token the pointer rests on, as this frame began.
    pub(crate) fn hovered(&self) -> Option<MenuToken> {
        self.hovered_token
    }

    /// Draw a draggable scrollbar and register its entire track for pointer input.
    pub(crate) fn scrollbar(
        &mut self,
        token: MenuToken,
        track: Rect,
        first: usize,
        visible: usize,
        total: usize,
    ) {
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: track,
            radius: track.width * 0.5,
            color: Color::new(1.0, 1.0, 1.0, 0.08),
        });
        let fraction = (visible as f32 / total.max(visible) as f32).clamp(0.08, 1.0);
        let thumb_height = track.height * fraction;
        let travel = track.height - thumb_height;
        let denominator = total.saturating_sub(visible).max(1) as f32;
        let thumb_y = track.y + travel * (first as f32 / denominator).clamp(0.0, 1.0);
        let active = self.token_hovered(token) || self.token_pressed(token);
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(track.x, thumb_y, track.width, thumb_height),
            radius: track.width * 0.5,
            color: if active {
                self.theme.accent
            } else {
                Color::new(0.42, 0.56, 0.66, 0.72)
            },
        });
        self.interactive(token, track, false, true);
    }

    /// Draw a semantic accent bar supplied by the calling screen.
    pub(crate) fn accent_bar(&mut self, rect: Rect, color: Color) {
        let _ = self.draw.push(DrawCommand::SolidRect { rect, color });
    }

    /// Finish focus ordering and restore semantic selection.
    pub(crate) fn finish(&mut self, selected_token: MenuToken) {
        self.check_storage();
        self.input.begin_frame(&self.tree);
        if let Some(index) = self
            .tokens
            .iter()
            .position(|token| *token == selected_token)
        {
            let _ = self.input.focus(WidgetId(index as u32));
        }
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        &self.draw
    }

    /// Whether this frame lost pointer areas, text runs or draw commands to
    /// the fixed storage ([`Self::check_storage`]), for a screen's tests.
    #[cfg(test)]
    pub(crate) fn overflowed(&self) -> bool {
        self.dropped > 0 || self.draw.len() >= self.draw.limit()
    }

    /// A frame past the fixed storage loses pointer areas, text or draw
    /// commands without a trace (a row that cannot be clicked, a missing label
    /// or picture). Debug builds stop on it, so a screen's tests catch it;
    /// release builds log it once per canvas.
    fn check_storage(&mut self) {
        let full = self.draw.len() >= self.draw.limit();
        if self.dropped == 0 && !full {
            return;
        }
        let message = format!(
            "menu canvas overflow: {} pointer areas or text runs dropped              (limits {MAX_WIDGETS} areas, {} text runs), {} of {} draw commands",
            self.dropped,
            self.text.len(),
            self.draw.len(),
            self.draw.limit()
        );
        if cfg!(debug_assertions) {
            panic!("{message}");
        }
        if !std::mem::replace(&mut self.overflow_logged, true) {
            crate::log::progress(format_args!("{message}"));
        }
    }

    /// The frame's command list, for screens that push commands the widget
    /// vocabulary has no word for (textured tiles, say).
    pub(crate) fn draw_list_mut(&mut self) -> &mut DrawList {
        &mut self.draw
    }

    /// Multiply the opacity of everything drawn until [`Self::pop_opacity`].
    pub(crate) fn push_opacity(&mut self, opacity: f32) {
        let _ = self.draw.push(DrawCommand::PushOpacity(opacity));
    }

    pub(crate) fn pop_opacity(&mut self) {
        let _ = self.draw.push(DrawCommand::PopOpacity);
    }

    pub(crate) fn theme(&self) -> Theme {
        self.theme
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[cfg(debug_assertions)]
    use sjk_ui::FontWeight;

    /// A frame past the canvas's fixed storage stops a debug build instead of
    /// dropping a row's pointer area (or a label) without a trace.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "menu canvas overflow")]
    fn running_out_of_pointer_areas_is_not_silent() {
        let mut canvas = MenuCanvas::new();
        canvas.begin_transparent([640.0, 480.0]);
        for token in 0..=MAX_WIDGETS as MenuToken {
            canvas.hit_region(token, Rect::new(0.0, 0.0, 1.0, 1.0));
        }
        canvas.finish(0);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "menu canvas overflow")]
    fn running_out_of_text_runs_is_not_silent() {
        let mut canvas = MenuCanvas::with_capacities(2, 16, 64);
        canvas.begin_transparent([640.0, 480.0]);
        for _ in 0..3 {
            canvas.text(
                "label",
                Rect::new(0.0, 0.0, 10.0, 10.0),
                12.0,
                Color::new(1.0, 1.0, 1.0, 1.0),
                FontWeight::Regular,
                0.0,
            );
        }
        canvas.finish(0);
    }

    #[test]
    fn a_frame_within_its_storage_finishes_quietly() {
        let mut canvas = MenuCanvas::new();
        canvas.begin_transparent([640.0, 480.0]);
        for token in 0..MAX_WIDGETS as MenuToken {
            canvas.hit_region(token, Rect::new(0.0, 0.0, 1.0, 1.0));
        }
        canvas.finish(0);
        assert_eq!(canvas.widget_count(), MAX_WIDGETS);
    }
}
