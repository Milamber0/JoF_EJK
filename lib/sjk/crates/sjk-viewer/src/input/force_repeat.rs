//! A held Force key uses its power again as soon as it can (`cl_forceRepeat`).
//!
//! In JoF EJK, as in every OpenJK client, a key bound to a plain command runs that
//! command again on each of the system's key repeats (`CL_KeyDownEvent` calls
//! `CL_ParseBinding` for every press, `cl_keys.cpp`); only `+` commands ignore them. A
//! held push key therefore pushes again once push is ready. SJK drops system repeats for
//! binds (`key_events.rs`), so it does this itself: while a key whose bind runs one of
//! [`REPEATED`] is held, every user command carries that command's `generic_cmd` again.
//! The server decides when it fires, on the first command after the power's cooldown,
//! so the repeat is never faster than pressing the key at the right moment.
//!
//! The console fills it from every bound key, keyboard and mouse alike
//! (`ViewerConsole::queue_bound_script`). A release clears its key even when a menu or
//! the chat took the key up (`GpuState::keyboard`, `pointer_button`), and losing the
//! window clears every key.

use super::generic_commands::generic_command;

/// The setting: 1 repeats a held push or pull key, 0 sends it once per press.
pub(crate) const CVAR: &str = "cl_forceRepeat";

/// The bind commands a held key repeats, with why repeating each is safe.
///
/// Only push and pull: codemp's `ClientThink_real` (`g_active.c`, `lastGenCmd`) takes
/// the same `generic_cmd` again only 300 ms later, except these two, "the only two where
/// you wouldn't care about a delay between", which it takes on every command
/// (`sjk_game_jka::generic_commands::take`); `ForceThrow` then refuses while the hand is still out (1 s after a
/// push) or the power is not usable (`force_throw.rs`), so it fires on the first command
/// after the cooldown. Not the others: speed, rage, protect, absorb and seeing are
/// toggles a second command switches off, and heal, mind trick and the team powers would
/// spend the pool every 300 ms without being asked.
pub(crate) const REPEATED: [(&str, &str); 2] = [
    (
        "force_throw",
        "push: taken on every command, fires once its cooldown is over",
    ),
    (
        "force_pull",
        "pull: taken on every command, fires once its cooldown is over",
    ),
];

/// The `generic_cmd` a held key repeats for a bind command, if it is one of [`REPEATED`].
pub(crate) fn repeated(command: &str) -> Option<u8> {
    let name = command
        .split_ascii_whitespace()
        .next()?
        .to_ascii_lowercase();
    REPEATED
        .iter()
        .any(|(repeated, _)| *repeated == name)
        .then(|| generic_command(&name))
        .flatten()
}

/// The held keys whose bind repeats a power, the last pressed last.
#[derive(Debug, Default)]
pub(crate) struct ForceRepeat {
    held: Vec<(String, u8)>,
}

impl ForceRepeat {
    /// The bind key `key` was pressed and runs `command`.
    pub(crate) fn press(&mut self, key: &str, command: u8) {
        self.release(key);
        self.held.push((key.to_owned(), command));
    }

    /// The bind key `key` was let go of.
    pub(crate) fn release(&mut self, key: &str) {
        self.held
            .retain(|(held, _)| !held.eq_ignore_ascii_case(key));
    }

    /// Every key was let go of (the window lost the keyboard).
    pub(crate) fn clear(&mut self) {
        self.held.clear();
    }

    /// The `generic_cmd` the next user command repeats: the last held key's.
    pub(crate) fn command(&self) -> Option<u8> {
        self.held.last().map(|(_, command)| *command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_push_and_pull_repeat() {
        assert_eq!(repeated("force_throw"), Some(5));
        assert_eq!(repeated("FORCE_PULL"), Some(6));
        for command in [
            "force_protect",
            "force_speed",
            "force_heal",
            "+useforce",
            "taunt",
        ] {
            assert_eq!(repeated(command), None, "{command}");
        }
    }

    #[test]
    fn the_last_held_key_repeats_until_every_key_is_let_go() {
        let mut repeat = ForceRepeat::default();
        assert_eq!(repeat.command(), None);
        repeat.press("MOUSE4", 5);
        repeat.press("f", 6);
        assert_eq!(repeat.command(), Some(6));
        repeat.release("F");
        assert_eq!(repeat.command(), Some(5));
        repeat.release("mouse4");
        assert_eq!(repeat.command(), None);
        repeat.press("MOUSE4", 5);
        repeat.clear();
        assert_eq!(repeat.command(), None);
    }

    #[test]
    fn a_second_press_of_a_held_key_is_not_counted_twice() {
        let mut repeat = ForceRepeat::default();
        repeat.press("MOUSE4", 5);
        repeat.press("MOUSE4", 5);
        repeat.release("MOUSE4");
        assert_eq!(repeat.command(), None);
    }
}
