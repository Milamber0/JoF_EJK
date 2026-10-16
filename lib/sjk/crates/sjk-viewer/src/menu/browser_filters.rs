//! The browser's filter toggles: their pointer tokens and the archived cvars
//! they flip.
use super::*;

/// Independent hit tokens, outside row/header/modal namespaces.
pub(super) const BASE: u16 = 50;
const NAMES: [&str; 4] = [
    "ui_browserShowEmpty",
    "ui_browserShowFull",
    "ui_browserShowPasswordProtected",
    "ui_browserFilterInvalidInfo",
];

impl ClientMenu {
    /// Sample cvars into the browser policy without rebuilding unchanged rows.
    pub(crate) fn configure_browser(&mut self, console: &mut ViewerConsole) {
        self.browser.configure(console);
    }

    /// Apply a filter-strip activation, persisting through the ordinary archived cvar path.
    pub(super) fn activate_filter(&mut self, token: u16, console: &mut ViewerConsole) -> bool {
        if !(BASE..BASE + 5).contains(&token) {
            return false;
        }
        let index = usize::from(token - BASE);
        if let Some(name) = NAMES.get(index) {
            let value = console.integer_cvar(name).unwrap_or(1) == 0;
            console.set_cvar(name, if value { "1" } else { "0" });
        } else {
            self.step_browser_mode(1, console);
        }
        self.configure_browser(console);
        true
    }

    /// Show the next game type (`step` 1) or the one before (-1): all of them
    /// (-1), then each of 0 to 9, round again.
    pub(super) fn step_browser_mode(&mut self, step: i32, console: &mut ViewerConsole) {
        let mode = next_mode(self.browser.filters().mode, step);
        console.set_cvar("ui_actualNetGametype", &mode.to_string());
        self.configure_browser(console);
    }
}

/// The game type `step` after `mode` in -1 (all), 0 to 9.
fn next_mode(mode: i32, step: i32) -> i32 {
    (mode + 1 + step).rem_euclid(11) - 1
}

#[cfg(test)]
mod tests {
    #[test]
    fn game_types_step_round_both_ways() {
        assert_eq!(super::next_mode(-1, 1), 0);
        assert_eq!(super::next_mode(9, 1), -1);
        assert_eq!(super::next_mode(-1, -1), 9);
        assert_eq!(super::next_mode(0, -1), -1);
        assert_eq!(super::next_mode(4, 1), 5);
    }
}
