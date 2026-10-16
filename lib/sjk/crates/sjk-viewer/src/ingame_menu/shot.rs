//! Camera control (called Shot controls until 08/10/2026): the camera and
//! sunlight panel for framing shots and recordings, sharing the console's
//! presentation director (`demo_camera`, `demo_sun`). The game menu's Camera
//! control entry or F8 (while unbound) opens it. It is drawn in the SJK UI's look
//! ([`sjk_view`]) in every menu style.
mod numeric;
mod runtime;

mod sjk_view;
use crate::menu_widgets::MenuCanvas;
use sjk_ui::{AbstractAction, InputEvent, UiEventKind};

pub(crate) const CAMERA: u16 = 10;
pub(crate) const SUN: u16 = 11;
/// The first of the four view presets (Back, Front, Left, Right).
pub(crate) const PRESET: u16 = 12;
pub(crate) const LIVE: u16 = 16;
pub(crate) const ORBIT: u16 = 17;
pub(crate) const STOP: u16 = 18;
pub(crate) const RESET: u16 = 19;
pub(crate) const APPLY_HIDE: u16 = 20;
pub(crate) const HIDE: u16 = 21;
pub(crate) const HUD: u16 = 22;
pub(crate) const RESET_SUN: u16 = 23;

/// One UI action; no formatted commands or per-drag allocation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Action {
    Preview,
    Orbit,
    Stop,
    Reset,
    ResetSun,
    ApplyHide,
    Hide,
    Hud,
}

/// A transient target draft; actual motion remains owned by the director.
pub(crate) struct Panel {
    pub(crate) values: [f32; 8],
    pub(super) camera_fallback: [f32; 4],
    pub(crate) sun_tab: bool,
    pub(crate) live: bool,
    pub(crate) dirty: [bool; 2],
    pub(crate) sun_available: bool,
    pub(crate) sun_mode: crate::console::director::SunMode,
    pub(crate) hud: bool,
    pub(crate) selected: u16,
    numeric: Option<crate::menu_widgets::numeric::NumericEdit>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            values: [0., 0., 80., 16., 0., 45., 3., 10.],
            camera_fallback: [0., 0., 80., 16.],
            sun_tab: false,
            live: true,
            dirty: [false; 2],
            sun_available: false,
            sun_mode: Default::default(),
            hud: true,
            selected: CAMERA,
            numeric: None,
        }
    }
}

pub(super) const SLIDERS: [(&str, f32, f32, f32); 8] = [
    ("Angle", -180., 180., 1.),
    ("Pitch", -80., 80., 1.),
    ("Distance", 16., 512., 1.),
    ("Height", -64., 160., 1.),
    ("Sun direction", 0., 360., 1.),
    ("Sun elevation", -20., 90., 1.),
    ("Move duration", 0.2, 20., 0.1),
    ("Orbit speed", -30., 30., 1.),
];

impl Panel {
    /// Reset the controller and discard staged sunlight so Preview/Move cannot reapply it.
    pub(crate) fn reset_sun(&mut self, director: &mut crate::console::director::Director) {
        director.reset_sun();
        self.dirty[1] = false;
        self.sun_mode = director.sun_mode();
    }

    /// Lay the panel out in the SJK UI's look.
    pub(crate) fn build_sjk(&self, canvas: &mut MenuCanvas, viewport: [f32; 2]) {
        sjk_view::build(self, canvas, viewport);
    }

    /// Up, Down or Tab: the next control in the drawn order, `forward` or
    /// back. A slider's number and its track are one stop.
    pub(crate) fn step(&mut self, canvas: &mut MenuCanvas, forward: bool) {
        let direction = if forward {
            AbstractAction::Next
        } else {
            AbstractAction::Previous
        };
        let stop = |token: u16| {
            crate::menu_widgets::numeric::value_row(token).unwrap_or(usize::from(token))
        };
        let from = stop(self.selected);
        for _ in 0..2 {
            let Some(token) = canvas.action(direction) else {
                return;
            };
            self.selected = token;
            if stop(token) != from {
                return;
            }
        }
    }

    pub(crate) fn pointer(&mut self, canvas: &mut MenuCanvas, event: InputEvent) -> Option<Action> {
        let event = canvas.pointer(event)?;
        let token = event.token?;
        // SJK: pressing anything but the draft's own value field applies it.
        if event.kind == UiEventKind::Press
            && crate::menu_widgets::numeric::value_row(token)
                != self.numeric.as_ref().map(|edit| edit.row)
            && let Some(action) = self.settle_numeric()
        {
            return Some(action);
        }
        if event.kind == UiEventKind::Activate {
            if let Some(row) = crate::menu_widgets::numeric::value_row(token) {
                self.begin_numeric(row);
                return None;
            }
            self.numeric = None;
        } else if self.numeric.is_some() {
            return None;
        }
        if matches!(event.kind, UiEventKind::Hover | UiEventKind::HoverEnter) {
            self.selected = token;
        }
        if token < 8 && matches!(event.kind, UiEventKind::Activate | UiEventKind::Drag) {
            self.selected = token;
            let rect = canvas.rect_for(token)?;
            let ratio = ((event.position?.x - rect.x) / rect.width).clamp(0., 1.);
            let (_, min, max, step) = SLIDERS[token as usize];
            return self.set_value(
                token as usize,
                ((min + ratio * (max - min)) / step).round() * step,
            );
        }
        (event.kind == UiEventKind::Activate)
            .then(|| self.activate(token))
            .flatten()
    }

    pub(crate) fn adjust(&mut self, direction: f32) -> Option<Action> {
        let row = crate::menu_widgets::numeric::value_row(self.selected)
            .unwrap_or(self.selected as usize);
        if row >= 8 {
            return None;
        }
        self.set_value(row, self.values[row] + direction * SLIDERS[row].3)
    }

    fn set_value(&mut self, row: usize, value: f32) -> Option<Action> {
        if self.sun_tab && !self.sun_available && row != 6 {
            return None;
        }
        self.values[row] = value.clamp(SLIDERS[row].1, SLIDERS[row].2);
        if row < 6 {
            self.dirty[usize::from(row >= 4)] = true;
            return self.live.then_some(Action::Preview);
        }
        None
    }

    pub(crate) fn activate(&mut self, token: u16) -> Option<Action> {
        if let Some(row) = crate::menu_widgets::numeric::value_row(token)
            .or_else(|| (token < 8).then_some(token as usize))
        {
            self.begin_numeric(row);
            return None;
        }
        self.numeric = None;
        match token {
            CAMERA | SUN => {
                self.sun_tab = token == SUN;
                self.selected = token;
                None
            }
            PRESET..=15 => {
                self.values[0] = [0., 180., -90., 90.][(token - PRESET) as usize];
                self.dirty[0] = true;
                self.live.then_some(Action::Preview)
            }
            LIVE => {
                self.live = !self.live;
                self.live.then_some(Action::Preview)
            }
            ORBIT if !self.sun_tab || self.sun_available => Some(Action::Orbit),
            STOP => Some(Action::Stop),
            RESET => Some(Action::Reset),
            RESET_SUN => Some(Action::ResetSun),
            APPLY_HIDE => Some(Action::ApplyHide),
            HIDE => Some(Action::Hide),
            HUD => Some(Action::Hud),
            _ => None,
        }
    }
}
