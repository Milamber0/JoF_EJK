//! `cg_cameraStyle`: how the third-person camera follows the player.
//!
//! `ejk`, the default, holds the camera and its target at their ideal places
//! every frame, as JoF EJK draws them with its strafe helper on: EternalJK skips
//! both dampings for `cg_strafeHelper` bits 0-3 and 13 (`cg_view.c`
//! `CG_UpdateThirdPersonTargetDamp` and `CG_UpdateThirdPersonCameraDamp`), which
//! is how Sol's JoF EJK profile plays (`cg_strafeHelper 2242`). `sjk` is SJK's
//! first camera: EternalJK's `CG_OffsetThirdPersonView` easing towards its ideal
//! place behind the player with `cg_thirdPersonCameraDamp` and
//! `cg_thirdPersonTargetDamp`, so it trails a moving or turning player a little.
//! Range, height, angles, collision and vehicle framing are the same in both
//! styles. A profile saved with the earlier default `sjk` moves to `ejk` once
//! (`cg_cameraStyleDefaultVersion`, in the console's start); a style chosen
//! after that stays.

use crate::console::ViewerConsole;

/// Archived cvar naming the camera style.
pub(crate) const CVAR: &str = "cg_cameraStyle";

/// How the third-person camera follows the player.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Style {
    /// Locked at its ideal place behind the player: no camera or target
    /// damping. SJK's default.
    #[default]
    Ejk,
    /// Eases after the player with the damping cvars.
    Sjk,
}

impl Style {
    /// Values the settings offer, in [`Style`] order.
    pub(crate) const NAMES: [&'static str; 2] = ["ejk", "sjk"];
    /// The `cg_cameraStyle` value of the default style.
    pub(crate) const DEFAULT_NAME: &'static str = Self::NAMES[0];
    /// The default `cg_cameraStyle` before `ejk` became it: a saved value moves
    /// from it once.
    pub(crate) const OLD_DEFAULT_NAME: &'static str = Self::NAMES[1];

    /// Read the cvar value: `sjk` (any case) selects SJK's eased camera;
    /// anything else, including a missing or mistyped value, the default
    /// locked one.
    pub(crate) fn from_cvar(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(text) if text.eq_ignore_ascii_case("sjk") => Self::Sjk,
            _ => Self::Ejk,
        }
    }

    /// The player's current style.
    pub(crate) fn from_console(console: Option<&ViewerConsole>) -> Self {
        Self::from_cvar(console.and_then(|console| console.text_value(CVAR)))
    }

    /// The camera and target damping the style uses, from the values of
    /// `cg_thirdPersonCameraDamp` and `cg_thirdPersonTargetDamp`. 1 puts the
    /// point at its ideal place every frame, as EternalJK's damping factor 1 does.
    pub(crate) fn damping(self, camera: f32, target: f32) -> (f32, f32) {
        match self {
            Self::Sjk => (camera, target),
            Self::Ejk => (1.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sjk_needs_an_explicit_value() {
        assert_eq!(Style::from_cvar(None), Style::Ejk);
        assert_eq!(Style::from_cvar(Some("")), Style::Ejk);
        assert_eq!(Style::from_cvar(Some("jof")), Style::Ejk);
        assert_eq!(Style::from_cvar(Some(" EJK ")), Style::Ejk);
        assert_eq!(Style::from_cvar(Some(" SJK ")), Style::Sjk);
    }

    #[test]
    fn offered_names_parse_in_order_and_the_default_is_ejk() {
        let parsed = Style::NAMES.map(|name| Style::from_cvar(Some(name)));
        assert_eq!(parsed, [Style::Ejk, Style::Sjk]);
        assert_eq!(
            Style::from_cvar(Some(Style::DEFAULT_NAME)),
            Style::default()
        );
        assert_eq!(Style::default(), Style::Ejk);
        assert_eq!(Style::from_cvar(Some(Style::OLD_DEFAULT_NAME)), Style::Sjk);
    }

    #[test]
    fn sjk_keeps_the_damping_cvars_and_ejk_snaps_both() {
        assert_eq!(Style::Sjk.damping(0.3, 0.5), (0.3, 0.5));
        assert_eq!(Style::Sjk.damping(0.0, 1.0), (0.0, 1.0));
        assert_eq!(Style::Ejk.damping(0.3, 0.5), (1.0, 1.0));
        assert_eq!(Style::Ejk.damping(0.0, 0.0), (1.0, 1.0));
    }
}
