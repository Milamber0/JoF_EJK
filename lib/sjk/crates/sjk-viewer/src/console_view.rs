//! The console's slide: how far the drop-down is open. The console itself is
//! drawn on its own layer ([`super::classic`]); command behavior remains in
//! `console.rs`.

use crate::menu_widgets::MenuCanvas;
use sjk_ui::DrawList;

/// The open fraction and the empty draw list the overlay pass reads while the
/// console layer draws.
pub(crate) struct ConsolePresentation {
    ui: MenuCanvas,
    fraction: f32,
    tick: std::time::Instant,
}

impl ConsolePresentation {
    pub(crate) fn new() -> Self {
        Self {
            ui: MenuCanvas::new(),
            fraction: 0.0,
            tick: std::time::Instant::now(),
        }
    }
    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    /// Move the open fraction towards `target` by `speed` screens per second
    /// since the last frame (`Con_RunConsole`), and return it.
    pub(super) fn slide(&mut self, target: f32, speed: f32) -> f32 {
        let delta = self.tick.elapsed().as_secs_f32();
        self.tick = std::time::Instant::now();
        let step = speed * delta;
        self.fraction += (target - self.fraction).clamp(-step, step);
        self.fraction
    }

    /// Set the open fraction at once, as a full-screen console is drawn.
    pub(super) fn snap(&mut self, fraction: f32) -> f32 {
        self.tick = std::time::Instant::now();
        self.fraction = fraction;
        fraction
    }

    /// Leave no shapes or text for this frame: the console layer draws the
    /// console.
    pub(super) fn clear(&mut self, viewport: [f32; 2]) {
        self.ui.begin_transparent(viewport);
        self.ui.finish(u16::MAX);
    }
}
