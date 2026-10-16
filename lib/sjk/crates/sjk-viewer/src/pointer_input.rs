//! Platform pointer conversion, overlay routing, and cursor-grab policy.

use super::*;
use sjk_ui::{InputEvent, PointerButton, UiEventKind, Vec2};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::window::CursorGrabMode;

/// Mouse-look settings latched from `sensitivity` and `m_invert` each frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MouseLook {
    /// Scales both axes (`cl_sensitivity`, `cl_input.cpp:1155-1159`).
    pub(crate) sensitivity: f32,
    /// Degrees of yaw per mouse count before sensitivity (`m_yaw`).
    pub(crate) yaw_scale: f32,
    /// Degrees of pitch per mouse count before sensitivity (`m_pitch`).
    pub(crate) pitch_scale: f32,
    /// Pull down to look up.
    pub(crate) invert: bool,
}

impl Default for MouseLook {
    fn default() -> Self {
        Self {
            sensitivity: 5.0,
            yaw_scale: 0.022,
            pitch_scale: 0.022,
            invert: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CursorMode {
    Free,
    Captured,
}

/// Cursor policy updates the OS cursor only when its desired state changes.
pub(crate) struct CursorPolicy {
    focused: bool,
    applied: Option<CursorMode>,
    ignore_next_motion: bool,
}

impl CursorPolicy {
    pub(crate) const fn new() -> Self {
        Self {
            focused: true,
            applied: None,
            ignore_next_motion: false,
        }
    }

    pub(crate) const fn desired(&self, live: bool, overlay: bool) -> CursorMode {
        if self.focused && live && !overlay {
            CursorMode::Captured
        } else {
            CursorMode::Free
        }
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn take_ignored_motion(&mut self) -> bool {
        std::mem::take(&mut self.ignore_next_motion)
    }

    fn finish_transition(&mut self, desired: CursorMode, was_captured: bool, captured: bool) {
        if was_captured && !captured {
            self.ignore_next_motion = true;
        }
        self.applied = Some(desired);
    }
}

impl GpuState {
    pub(crate) fn pointer_moved(&mut self, position: PhysicalPosition<f64>) {
        self.cursor_position = [position.x as f32, position.y as f32];
        self.route_pointer(InputEvent::PointerMove(Vec2::new(
            self.cursor_position[0],
            self.cursor_position[1],
        )));
    }

    pub(crate) fn pointer_left(&mut self) {
        self.route_pointer(InputEvent::PointerLeave);
    }

    pub(crate) fn pointer_button(&mut self, button: MouseButton, state: ElementState) {
        // Whoever takes the release, a held push or pull button stops repeating.
        if state == ElementState::Released
            && let Some(key) = input::keys::name(input::keys::Source::Mouse(button))
            && let Some(console) = &mut self.console
        {
            console.force_repeat_button_up(key);
        }
        // An open quick wheel takes the left and right buttons: they change its
        // page, and neither attacks.
        let side = match button {
            MouseButton::Left => Some(crate::quick_wheel::Button::Left),
            MouseButton::Right => Some(crate::quick_wheel::Button::Right),
            _ => None,
        };
        if let Some(side) = side
            && self.quick_wheel.button(
                side,
                state == ElementState::Pressed,
                std::time::Instant::now(),
            )
        {
            return;
        }
        if state == ElementState::Pressed
            && self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible())
            && match (&mut self.client_menu, &mut self.console) {
                (Some(menu), Some(console)) => menu.handle_mouse_binding(button, console),
                _ => false,
            }
        {
            return;
        }
        let Some(event) = normalize_button_event(button, state, self.cursor_position) else {
            if self.pointer_captured {
                self.route_stock_mouse_button(button, state == ElementState::Pressed);
            }
            return;
        };
        let button = normalize_button(button).expect("normalized event has a supported button");
        if self.route_pointer(event) {
            return;
        }
        if button == PointerButton::Primary && state == ElementState::Pressed {
            if !self.pointer_captured {
                self.capture_pointer();
                return;
            }
        }
        self.route_stock_mouse_button(
            match button {
                PointerButton::Primary => MouseButton::Left,
                PointerButton::Secondary => MouseButton::Right,
                PointerButton::Middle => MouseButton::Middle,
            },
            state == ElementState::Pressed,
        );
    }

    pub(crate) fn pointer_wheel(&mut self, delta: MouseScrollDelta) {
        let delta = normalize_wheel(delta);
        // An open quick wheel takes the scroll: it changes page, not weapon.
        if self.quick_wheel.is_open() {
            self.quick_wheel
                .scrolled(delta.y, std::time::Instant::now());
            return;
        }
        if let Some(up) = wheel_binding_direction(delta.y)
            && self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible())
            && match (&mut self.client_menu, &mut self.console) {
                (Some(menu), Some(console)) => menu.handle_wheel_binding(up, console),
                _ => false,
            }
        {
            return;
        }
        let event = InputEvent::PointerWheel {
            position: Vec2::new(self.cursor_position[0], self.cursor_position[1]),
            delta,
        };
        if self.route_pointer(event) {
            return;
        }
        self.route_gameplay_wheel(delta.y);
    }

    pub(crate) fn pointer_focus(&mut self, focused: bool) {
        self.cursor_policy.set_focus(focused);
        self.gameplay_input.focus(focused);
        if !focused {
            self.pending_generic_command = 0;
            if let Some(console) = &mut self.console {
                console.clear_force_repeat();
            }
            // The released keys reach the server with the next command, at once.
            self.packet_pacer.reset();
        }
        self.sync_cursor_policy();
    }

    pub(crate) fn pointer_motion(&mut self, delta: (f64, f64)) {
        if !self.pointer_captured || self.cursor_policy.take_ignored_motion() {
            return;
        }
        // An open quick wheel takes the mouse: it moves the wheel's pointer, not the view.
        if self.quick_wheel.is_open() {
            self.quick_wheel.moved([delta.0 as f32, delta.1 as f32]);
            return;
        }
        // Accumulate raw counts; filtering and acceleration run once per command frame.
        self.gameplay_input.motion.raw[0] += delta.0 as f32;
        self.gameplay_input.motion.raw[1] += delta.1 as f32;
    }

    pub(crate) fn capture_pointer(&mut self) {
        self.sync_cursor_policy();
    }

    pub(crate) fn release_pointer(&mut self) {
        self.apply_cursor_mode(CursorMode::Free);
        self.gameplay_input.release_keys();
    }

    /// Keyboard catchers also advertise BUTTON_TALK, as CL_CmdButtons does.
    pub(crate) fn key_catcher_active(&self) -> bool {
        self.game_menu
            || self.text_dialog.is_open()
            || self.chat.is_typing()
            || self
                .console
                .as_ref()
                .is_some_and(|console| console.is_open())
            || self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible())
    }

    /// Commands carry `BUTTON_TALK`: a key catcher is open, or the window is away
    /// (alt-tabbed or minimised, see [`crate::console::ViewerConsole::window_talk`]).
    pub(crate) fn talk_button(&self) -> bool {
        self.key_catcher_active()
            || self
                .console
                .as_ref()
                .is_some_and(|console| console.window_talk())
    }

    pub(crate) fn sync_cursor_policy(&mut self) {
        let overlay = self.key_catcher_active();
        let desired = self.cursor_policy.desired(
            self.live_session.is_some() || self.resident.exploring(),
            overlay,
        );
        self.apply_cursor_mode(desired);
    }

    fn apply_cursor_mode(&mut self, desired: CursorMode) {
        if self.cursor_policy.applied == Some(desired) {
            return;
        }
        let Some(window) = &self.window else {
            self.pointer_captured = false;
            self.cursor_policy.finish_transition(desired, false, false);
            return;
        };
        let was_captured = self.pointer_captured;
        self.pointer_captured = match desired {
            CursorMode::Free => {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
                false
            }
            CursorMode::Captured => {
                let captured = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                    .is_ok();
                window.set_cursor_visible(!captured);
                captured
            }
        };
        self.cursor_policy
            .finish_transition(desired, was_captured, self.pointer_captured);
    }

    fn route_pointer(&mut self, event: InputEvent) -> bool {
        // The Profile screen's tabs, over whichever of its screens shows.
        if !self.text_dialog.is_open() && self.profile_hub_pointer(event) {
            return true;
        }
        if self
            .console
            .as_ref()
            .is_some_and(|console| console.is_open())
        {
            if let Some(console) = &mut self.console {
                console.handle_pointer(event);
            }
            return true;
        }
        // The dialog is drawn over everything and takes the keys first; the pointer too.
        if self.text_dialog.is_open() {
            let action = self.text_dialog.handle_pointer(event);
            self.apply_dialog_action(action);
            return true;
        }
        if self.medal_popup.is_open() {
            self.medal_popup.handle_pointer(event);
            return true;
        }
        if self.holocron_popup.is_open() {
            self.holocron_popup.handle_pointer(event);
            return true;
        }
        if self.graphics_reload.card.is_open() {
            let choice = self.graphics_reload.card.handle_pointer(event);
            self.reload_card_choice(choice);
            return true;
        }
        if self.fps_card.is_open() {
            let choice = self.fps_card.handle_pointer(event);
            self.fps_card_choice(choice);
            return true;
        }
        if self.log_request_card.is_open() {
            let choice = self.log_request_card.handle_pointer(event);
            self.log_request_card_choice(choice);
            return true;
        }
        if self.whats_new_card.is_open() {
            let choice = self.whats_new_card.handle_pointer(event);
            self.whats_new_card_choice(choice);
            return true;
        }
        if self
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible())
        {
            let action = match (&mut self.client_menu, &mut self.console) {
                (Some(menu), Some(console)) => menu.handle_pointer(event, console),
                _ => menu::MenuAction::None,
            };
            self.apply_client_menu_action(action);
            return true;
        }
        if self.game_menu {
            // The SJK UI has no Report a bug button: it is on its SJK page.
            if self.game_menu_page != GameMenuPage::Shot
                && !self.in_game_menu.is_sjk()
                && self.text_dialog.launcher_pointer(event)
            {
                self.open_bug_report();
                return true;
            }
            if self.game_menu_page == GameMenuPage::Shot {
                self.shot_pointer(event);
                return true;
            }
            if let Some((kind, row)) = self.in_game_menu.pointer(event) {
                // The SJK UI's profile card opens the Profile screen on its Profile tab.
                if row == usize::from(crate::ingame_menu::sjk_view::CARD_TOKEN)
                    && self.in_game_menu.is_sjk()
                {
                    if kind == UiEventKind::Activate {
                        self.open_profile_hub_from_game(Some(crate::profile_hub::Tab::Profile));
                    }
                    return true;
                }
                if self.sjk_main_pointer(kind, row) {
                    return true;
                }
                if let Some(tab) = crate::ingame_menu::classic::bar_tab(row) {
                    self.classic_bar_pointer(kind, tab);
                    return true;
                }
                if row < self.game_menu_row_count()
                    && matches!(kind, UiEventKind::HoverEnter | UiEventKind::Hover)
                {
                    self.game_menu_row = row;
                    // Hovering an entry gives the list the keyboard.
                    self.in_game_menu.focus = crate::ingame_menu::sjk_focus::Focus::List;
                }
                if row < self.game_menu_row_count()
                    && kind == UiEventKind::Activate
                    && self.in_game_menu.activation_allowed(row)
                {
                    self.game_menu_row = row;
                    self.activate_game_menu_row();
                }
            }
            return true;
        }
        if self.chat.is_typing() {
            self.chat.pointer(event);
            return true;
        }
        false
    }

    fn route_stock_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        let Some(key) = input::keys::name(input::keys::Source::Mouse(button)) else {
            return;
        };
        if let Some(console) = &mut self.console {
            console.queue_bound_script(key, pressed);
        }
    }

    fn route_gameplay_wheel(&mut self, vertical: f32) {
        if self.live_session.is_none() || self.chat.is_typing() || self.game_menu {
            return;
        }
        if vertical == 0.0 {
            return;
        }
        let key = input::keys::name(input::keys::Source::Wheel(vertical > 0.0)).unwrap();
        if let Some(console) = &mut self.console {
            console.queue_bound_script(key, true);
            console.queue_bound_script(key, false);
        }
    }
}

fn normalize_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

pub(crate) fn normalize_button_event(
    button: MouseButton,
    state: ElementState,
    position: [f32; 2],
) -> Option<InputEvent> {
    let button = normalize_button(button)?;
    let position = Vec2::new(position[0], position[1]);
    Some(match state {
        ElementState::Pressed => InputEvent::PointerPress { position, button },
        ElementState::Released => InputEvent::PointerRelease { position, button },
    })
}

/// Smallest normalized vertical wheel delta that binds a waiting key slot: half
/// a notch (a [`MouseScrollDelta::LineDelta`] notch normalizes to 40), so the
/// small pixel deltas of a trackpad do not bind by accident.
const WHEEL_BIND_MIN_DELTA: f32 = 20.0;

/// The direction a normalized wheel delta binds while a slot waits for a key:
/// `Some(true)` for `MWHEELUP` (positive y, away from the player, as
/// `route_gameplay_wheel` maps it), `Some(false)` for `MWHEELDOWN`, `None` below
/// [`WHEEL_BIND_MIN_DELTA`].
fn wheel_binding_direction(vertical: f32) -> Option<bool> {
    (vertical.abs() >= WHEEL_BIND_MIN_DELTA).then_some(vertical > 0.0)
}

fn normalize_wheel(delta: MouseScrollDelta) -> Vec2 {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => Vec2::new(x * 40.0, y * 40.0),
        MouseScrollDelta::PixelDelta(position) => Vec2::new(position.x as f32, position.y as f32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notch_binds_but_a_tiny_trackpad_delta_does_not() {
        let vertical = |delta| normalize_wheel(delta).y;
        assert_eq!(
            wheel_binding_direction(vertical(MouseScrollDelta::LineDelta(0.0, 1.0))),
            Some(true)
        );
        assert_eq!(
            wheel_binding_direction(vertical(MouseScrollDelta::LineDelta(0.0, -1.0))),
            Some(false)
        );
        for pixels in [0.0, 1.0, -3.5, 19.0, -19.9] {
            let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, pixels));
            assert_eq!(wheel_binding_direction(vertical(delta)), None, "{pixels}");
        }
        let swipe = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 60.0));
        assert_eq!(wheel_binding_direction(vertical(swipe)), Some(true));
    }
}
