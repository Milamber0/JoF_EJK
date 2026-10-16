//! Bridge Camera control's actions to the existing presentation director and
//! cursor policy.
use super::*;
use crate::console::{ViewerConsole, director::Request};
use crate::{GpuState, ingame_menu::Page};
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

fn camera_defaults(console: &ViewerConsole) -> [f32; 4] {
    [
        ("cg_thirdPersonAngle", 0.),
        ("cg_thirdPersonPitchOffset", 0.),
        ("cg_thirdPersonRange", 80.),
        ("cg_thirdPersonVertOffset", 16.),
    ]
    .map(|(name, fallback)| {
        console
            .float_cvar(name)
            .filter(|v| v.is_finite() && (*v as f32).is_finite())
            .map_or(fallback, |v| v as f32)
    })
}
fn angles(sun: glam::Vec3) -> [f32; 2] {
    [
        sun.y.atan2(sun.x).to_degrees().rem_euclid(360.),
        sun.z.clamp(-1., 1.).asin().to_degrees(),
    ]
}

impl GpuState {
    /// Open a live-world panel without changing the current camera or sunlight.
    pub(crate) fn open_shot_panel(&mut self) {
        let Some(console) = &mut self.console else {
            return;
        };
        let fallback = camera_defaults(console);
        let mut camera = console.director.camera(fallback).unwrap_or(fallback);
        camera[0] = (camera[0] + 180.).rem_euclid(360.) - 180.;
        let sun = self.world_materials.shot_sun(false);
        let panel = &mut self.in_game_menu.shot;
        panel.camera_fallback = fallback;
        panel.values[..4].copy_from_slice(&camera);
        panel.sun_available = sun.is_some();
        panel.sun_mode = console.director.sun_mode();
        if let Some(sun) = sun {
            panel.values[4..6].copy_from_slice(&angles(sun));
        }
        panel.hud = console.bool_cvar("cg_draw2D").unwrap_or(true);
        panel.dirty = [false; 2];
        panel.numeric = None;
        self.game_menu = true;
        self.game_menu_page = Page::Shot;
        self.release_pointer();
    }

    /// Update live readouts during an orbit without overwriting staged targets.
    pub(crate) fn refresh_shot_preview(&mut self) {
        if !self.game_menu || self.game_menu_page != Page::Shot {
            return;
        }
        let panel = &mut self.in_game_menu.shot;
        if let Some(console) = &self.console {
            panel.sun_mode = console.director.sun_mode();
        }
        if !panel.live {
            return;
        }
        if !panel.dirty[0]
            && let Some(console) = &mut self.console
        {
            let mut pose = console
                .director
                .camera(panel.camera_fallback)
                .unwrap_or(panel.camera_fallback);
            pose[0] = (pose[0] + 180.).rem_euclid(360.) - 180.;
            panel.values[..4].copy_from_slice(&pose);
        }
        if !panel.dirty[1]
            && let Some(sun) = self.world_materials.shot_sun(false)
        {
            panel.values[4..6].copy_from_slice(&angles(sun));
        }
    }

    /// F8 opens the panel if unbound; the panel owns its navigation while visible.
    pub(crate) fn shot_key(&mut self, event: &KeyEvent) -> bool {
        let PhysicalKey::Code(key) = event.physical_key else {
            return false;
        };
        let open = self.game_menu && self.game_menu_page == Page::Shot;
        if !open {
            if key == KeyCode::F8
                && event.state == ElementState::Pressed
                && !event.repeat
                && (self.live_session.is_some() || self.demo_session.is_some())
                && self
                    .console
                    .as_ref()
                    .is_some_and(|c| !c.has_key_binding("F8"))
            {
                self.open_shot_panel();
                return true;
            }
            return false;
        }
        if event.state != ElementState::Pressed {
            return true;
        }
        let panel = &mut self.in_game_menu.shot;
        let action = if panel.numeric.is_some() && key != KeyCode::F8 {
            panel.edit_numeric(key, event.text.as_deref(), event.repeat)
        } else if !event.repeat
            && event.text.as_deref().is_some_and(|text| {
                text.starts_with(|c: char| c.is_ascii_digit() || ".,-".contains(c))
            })
            && panel.begin_typed(event.text.as_deref().unwrap_or_default())
        {
            // SJK: a number typed on a selected slider opens entry with it.
            None
        } else {
            match key {
                KeyCode::F8 | KeyCode::Escape if !event.repeat => Some(Action::Hide),
                KeyCode::ArrowLeft => panel.adjust(-1.),
                KeyCode::ArrowRight => panel.adjust(1.),
                KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Tab => {
                    panel.step(&mut self.in_game_menu.canvas, key != KeyCode::ArrowUp);
                    None
                }
                // SJK: Space keeps stepping a slider; Enter opens its entry.
                KeyCode::Space if !event.repeat && panel.selects_slider() => panel.adjust(1.),
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if !event.repeat => {
                    panel.activate(panel.selected)
                }
                _ => None,
            }
        };
        if let Some(action) = action {
            self.apply_shot_action(action);
        }
        true
    }

    /// Consume pointer events entirely inside the shot overlay; never send clicks to combat.
    pub(crate) fn shot_pointer(&mut self, event: InputEvent) {
        if let Some(action) = self
            .in_game_menu
            .shot
            .pointer(&mut self.in_game_menu.canvas, event)
        {
            self.apply_shot_action(action);
        }
    }

    fn apply_shot_action(&mut self, action: Action) {
        let panel = &mut self.in_game_menu.shot;
        let Some(console) = &mut self.console else {
            return;
        };
        let seconds = f64::from(panel.values[6]);
        let sun_tab = panel.sun_tab;
        match action {
            Action::Preview | Action::ApplyHide => {
                if action == Action::Preview || !panel.live {
                    let duration = if action == Action::Preview {
                        0.18
                    } else {
                        seconds
                    };
                    if panel.dirty[0] {
                        console.director.set_camera(Request::Target(
                            panel.values[..4].try_into().unwrap(),
                            duration,
                        ));
                        self.third_person_choice = true;
                    }
                    if panel.dirty[1] && panel.sun_available {
                        console.director.set_sun(Request::Target(
                            panel.values[4..6].try_into().unwrap(),
                            duration,
                        ));
                    }
                }
            }
            Action::Orbit => {
                if sun_tab {
                    console.director.set_sun(Request::Orbit(panel.values[7]));
                } else {
                    self.third_person_choice = true;
                    console.director.set_camera(Request::Orbit(panel.values[7]));
                }
                panel.dirty[usize::from(sun_tab)] = false;
            }
            Action::Stop => {
                if sun_tab {
                    console.director.set_sun(Request::Stop);
                } else {
                    console.director.set_camera(Request::Stop);
                }
                panel.dirty[usize::from(sun_tab)] = false;
            }
            Action::Reset => {
                if sun_tab {
                    console.director.set_sun(Request::Auto(seconds));
                    if let Some(sun) = self.world_materials.shot_sun(true) {
                        panel.values[4..6].copy_from_slice(&angles(sun));
                    }
                } else {
                    console.director.set_camera(Request::Auto(seconds));
                    panel.values[..4].copy_from_slice(&camera_defaults(console));
                }
                panel.dirty[usize::from(sun_tab)] = false;
            }
            Action::ResetSun => {
                panel.reset_sun(&mut console.director);
                if let Some(sun) = self.world_materials.shot_sun(true) {
                    panel.values[4..6].copy_from_slice(&angles(sun));
                }
            }
            Action::Hud => {
                panel.hud = !panel.hud;
                console.set_cvar("cg_draw2D", if panel.hud { "1" } else { "0" });
            }
            Action::Hide => {}
        }
        if matches!(action, Action::Hide | Action::ApplyHide) {
            self.game_menu = false;
            self.capture_pointer();
        }
    }
}
