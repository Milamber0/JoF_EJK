//! Display mode of the client window and the monitor facts the settings
//! screen offers choices from.
//!
//! Stock `r_fullscreen` keeps its meaning, fullscreen on or off, and Alt+Enter
//! still toggles it. SJK adds `r_exclusiveFullscreen` to say which kind of
//! fullscreen that is: 0 (the default) is borderless at the desktop size, 1 is
//! an exclusive video mode at `r_resolution`. Stock JA's fullscreen is always
//! the exclusive kind; SJK keeps borderless as its default because it does
//! not change the monitor's mode and switches away instantly.

use crate::console::ViewerConsole;
use std::time::Duration;
use winit::window::Window;

/// Cvar choosing the kind of fullscreen `r_fullscreen 1` gives.
pub(crate) const EXCLUSIVE_CVAR: &str = "r_exclusiveFullscreen";

/// How the client window covers the screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayMode {
    /// A decorated (or `r_noborder`) window of `r_resolution`.
    Windowed,
    /// A borderless window covering the monitor at the desktop size.
    Borderless,
    /// The monitor switched to the `r_resolution` video mode.
    Exclusive,
}

impl DisplayMode {
    /// Every mode, in the order the settings row steps through them.
    pub(crate) const ALL: [Self; 3] = [Self::Windowed, Self::Borderless, Self::Exclusive];

    /// The mode the two cvars describe.
    pub(crate) fn from_cvars(fullscreen: bool, exclusive: bool) -> Self {
        match (fullscreen, exclusive) {
            (false, _) => Self::Windowed,
            (true, false) => Self::Borderless,
            (true, true) => Self::Exclusive,
        }
    }

    /// The mode the console's cvars ask for.
    pub(crate) fn requested(console: &ViewerConsole) -> Self {
        Self::from_cvars(
            console.bool_cvar("r_fullscreen").unwrap_or(false),
            console.bool_cvar(EXCLUSIVE_CVAR).unwrap_or(false),
        )
    }

    /// The mode that is applied when exclusive fullscreen is unavailable:
    /// it falls back to borderless.
    pub(crate) fn effective(self, exclusive_available: bool) -> Self {
        if self == Self::Exclusive && !exclusive_available {
            Self::Borderless
        } else {
            self
        }
    }

    /// The mode to apply while the window is `suspended` (see
    /// [`suspended_after_focus`]): exclusive fullscreen becomes windowed, which
    /// restores the desktop's video mode and drops the topmost window level
    /// winit gives an exclusive window; the other modes are unaffected.
    pub(crate) fn while_suspended(self, suspended: bool) -> Self {
        if suspended && self == Self::Exclusive {
            Self::Windowed
        } else {
            self
        }
    }

    /// The name the settings screen shows.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Windowed => "Windowed",
            Self::Borderless => "Borderless fullscreen",
            Self::Exclusive => "Exclusive fullscreen",
        }
    }

    /// The next mode `direction` steps away, wrapping, skipping exclusive
    /// fullscreen when it is unavailable.
    pub(crate) fn step(self, direction: i32, exclusive_available: bool) -> Self {
        let modes: &[Self] = if exclusive_available {
            &Self::ALL
        } else {
            &Self::ALL[..2]
        };
        let index = modes
            .iter()
            .position(|mode| *mode == self.effective(exclusive_available))
            .unwrap_or(0) as i32;
        modes[(index + direction.signum()).rem_euclid(modes.len() as i32) as usize]
    }

    /// Write this mode into the cvars. Windowed leaves the fullscreen kind
    /// alone, so Alt+Enter returns to whichever kind was last chosen.
    pub(crate) fn store(self, console: &mut ViewerConsole) {
        match self {
            Self::Windowed => {
                console.set_cvar("r_fullscreen", "0");
            }
            Self::Borderless | Self::Exclusive => {
                let exclusive = if self == Self::Exclusive { "1" } else { "0" };
                console.set_cvar(EXCLUSIVE_CVAR, exclusive);
                console.set_cvar("r_fullscreen", "1");
            }
        }
    }
}

/// How soon after entering a display mode a focus loss is taken for a side
/// effect of the switch itself, not the player leaving (Alt+Tab, Win+D).
pub(crate) const FOCUS_LOSS_GRACE: Duration = Duration::from_millis(750);

/// Whether exclusive fullscreen is suspended after a focus change. An exclusive
/// window is topmost and owns the video mode, so it would stay over the desktop
/// after Alt+Tab or Win+D: losing focus suspends it, and gaining focus (the
/// window restored from the taskbar) resumes it. A loss within
/// [`FOCUS_LOSS_GRACE`] of the last display change is ignored, so a focus
/// flicker caused by the switch cannot loop suspend and resume.
pub(crate) fn suspended_after_focus(
    suspended: bool,
    exclusive_active: bool,
    since_display_change: Duration,
    focused: bool,
) -> bool {
    if focused {
        false
    } else {
        suspended || (exclusive_active && since_display_change >= FOCUS_LOSS_GRACE)
    }
}

/// What the window's monitor offers, gathered when the settings screen opens.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MonitorModes {
    /// Distinct sizes of the monitor's video modes, largest first.
    pub(crate) sizes: Vec<[u32; 2]>,
    /// The monitor's current (desktop) size.
    pub(crate) desktop: Option<[u32; 2]>,
    /// Whether exclusive fullscreen can be used on this window.
    pub(crate) exclusive: bool,
}

impl MonitorModes {
    /// Ask the window's current monitor for its modes.
    pub(crate) fn query(window: &Window) -> Self {
        let Some(monitor) = window.current_monitor() else {
            return Self::default();
        };
        let mut sizes: Vec<[u32; 2]> = monitor
            .video_modes()
            .map(|mode| [mode.size().width, mode.size().height])
            .filter(|[width, height]| *width > 0 && *height > 0)
            .collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes.dedup();
        let desktop = Some([monitor.size().width, monitor.size().height])
            .filter(|[width, height]| *width > 0 && *height > 0);
        let exclusive = exclusive_supported(window) && !sizes.is_empty();
        Self {
            sizes,
            desktop,
            exclusive,
        }
    }
}

/// Whether the windowing system honours exclusive fullscreen: Wayland
/// ignores the request (winit `Window::set_fullscreen`).
pub(crate) fn exclusive_supported(window: &Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    window
        .window_handle()
        .is_ok_and(|handle| !matches!(handle.as_raw(), RawWindowHandle::Wayland(_)))
}

/// The monitor's video mode of exactly `size`, at its highest refresh rate
/// and colour depth, for exclusive fullscreen.
pub(crate) fn exclusive_video_mode(
    window: &Window,
    size: [u32; 2],
) -> Option<winit::monitor::VideoModeHandle> {
    window
        .current_monitor()?
        .video_modes()
        .filter(|mode| [mode.size().width, mode.size().height] == size)
        .max_by_key(|mode| (mode.refresh_rate_millihertz(), mode.bit_depth()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cvars_map_to_modes() {
        assert_eq!(DisplayMode::from_cvars(false, false), DisplayMode::Windowed);
        assert_eq!(DisplayMode::from_cvars(false, true), DisplayMode::Windowed);
        assert_eq!(
            DisplayMode::from_cvars(true, false),
            DisplayMode::Borderless
        );
        assert_eq!(DisplayMode::from_cvars(true, true), DisplayMode::Exclusive);
    }

    #[test]
    fn exclusive_falls_back_to_borderless() {
        assert_eq!(
            DisplayMode::Exclusive.effective(false),
            DisplayMode::Borderless
        );
        assert_eq!(
            DisplayMode::Exclusive.effective(true),
            DisplayMode::Exclusive
        );
        assert_eq!(
            DisplayMode::Windowed.effective(false),
            DisplayMode::Windowed
        );
    }

    #[test]
    fn suspended_exclusive_is_windowed() {
        use DisplayMode::*;
        assert_eq!(Exclusive.while_suspended(true), Windowed);
        assert_eq!(Exclusive.while_suspended(false), Exclusive);
        assert_eq!(Borderless.while_suspended(true), Borderless);
        assert_eq!(Windowed.while_suspended(true), Windowed);
    }

    #[test]
    fn focus_loss_suspends_exclusive_and_gain_resumes() {
        let settled = FOCUS_LOSS_GRACE;
        assert!(suspended_after_focus(false, true, settled, false));
        assert!(!suspended_after_focus(true, false, Duration::ZERO, true));
        assert!(!suspended_after_focus(false, true, settled, true));
    }

    #[test]
    fn focus_loss_leaves_other_modes_alone() {
        // Borderless, windowed and the exclusive request that fell back to
        // borderless are not exclusive on the window.
        assert!(!suspended_after_focus(
            false,
            false,
            FOCUS_LOSS_GRACE,
            false
        ));
    }

    #[test]
    fn focus_loss_right_after_a_switch_is_ignored() {
        let early = FOCUS_LOSS_GRACE - Duration::from_millis(1);
        assert!(!suspended_after_focus(false, true, early, false));
    }

    #[test]
    fn repeated_focus_loss_stays_suspended() {
        assert!(suspended_after_focus(true, false, Duration::ZERO, false));
    }

    #[test]
    fn stepping_wraps_and_skips_unavailable_modes() {
        use DisplayMode::*;
        assert_eq!(Windowed.step(1, true), Borderless);
        assert_eq!(Borderless.step(1, true), Exclusive);
        assert_eq!(Exclusive.step(1, true), Windowed);
        assert_eq!(Windowed.step(-1, true), Exclusive);
        assert_eq!(Borderless.step(1, false), Windowed);
        assert_eq!(Windowed.step(-1, false), Borderless);
        // A stored exclusive request steps from what is applied.
        assert_eq!(Exclusive.step(1, false), Windowed);
    }
}
