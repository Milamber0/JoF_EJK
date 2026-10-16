//! Apply a changed saber at once on servers that accept a `saber` command.
//!
//! The `saber1`/`saber2` userinfo keys are read by stock game modules only
//! when the player spawns, so a saber picked in the menu or set from the
//! console takes effect after the next respawn. JA+ and jaPRO-lineage servers
//! also accept a `saber <saber1> [<saber2>]` client command that swaps the
//! saber in play (jaPRO `Cmd_Saber_f`, `codemp/game/g_cmds.c`: one or two names,
//! a single saber meaning `saber2` "none"; it may refuse while the player moves
//! or in some game modes, which the server answers with a `print`). JoF EJK's
//! `CG_Saber_f` (`codemp/cgame/cg_consolecmds.c`) sends it when
//! `cgs.serverMod >= SVMOD_JAPLUS` -- JA+ and jaPRO -- after setting the cvars,
//! which is the policy [`CompatProfile::JaPlus`] and [`CompatProfile::TaystJk`]
//! express here. Stock, OpenJK-alt and unknown modules never get the command.
//!
//! [`SaberChangeNotifier`] watches the two cvars each frame, so the menu and a
//! console `set saber1 ...` take the same path, and sends the command once per
//! changed selection, no faster than [`MIN_INTERVAL`].

use crate::CompatProfile;
use std::time::{Duration, Instant};

/// Shortest gap between two `saber` commands: a menu list scrolled quickly
/// settles on its last pick instead of flooding the server.
pub const MIN_INTERVAL: Duration = Duration::from_millis(1_000);

/// Whether servers of `profile` accept the `saber` client command.
pub fn profile_accepts_saber_command(profile: &CompatProfile) -> bool {
    matches!(
        profile,
        CompatProfile::JaPlus { .. } | CompatProfile::TaystJk
    )
}

/// The `saber` command for these cvar values: `saber <saber1>` for a single
/// saber (`saber2` empty or `none`), `saber <saber1> <saber2>` for dual or
/// staff. `None` when a name is empty or could not travel as one command
/// argument (whitespace, quote, `;` or `\`).
pub fn saber_command(saber1: &str, saber2: &str) -> Option<String> {
    let usable = |name: &str| {
        !name.is_empty()
            && !name
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '"' | ';' | '\\'))
    };
    let saber1 = saber1.trim();
    let saber2 = saber2.trim();
    if !usable(saber1) {
        return None;
    }
    if saber2.is_empty() || saber2.eq_ignore_ascii_case("none") {
        return Some(format!("saber {saber1}"));
    }
    usable(saber2).then(|| format!("saber {saber1} {saber2}"))
}

/// Per-connection record of the saber the server last knew.
#[derive(Debug, Default)]
pub struct SaberChangeNotifier {
    /// The selection the connection started with, or the last one sent.
    known: Option<(String, String)>,
    last_sent: Option<Instant>,
}

impl SaberChangeNotifier {
    /// Forget the connection (disconnect, new server): the next [`poll`]
    /// adopts the then-current selection, which the connect userinfo carried.
    ///
    /// [`poll`]: Self::poll
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The `saber` command due now, if any. Call it every frame while
    /// connected; `userinfo_settled` is true when no userinfo change is still
    /// waiting to be sent, so the command follows the keys it announces.
    pub fn poll(
        &mut self,
        profile: &CompatProfile,
        saber1: &str,
        saber2: &str,
        userinfo_settled: bool,
        now: Instant,
    ) -> Option<Vec<u8>> {
        if !profile_accepts_saber_command(profile) {
            return None;
        }
        let current = (saber1.to_owned(), saber2.to_owned());
        let Some(known) = &self.known else {
            self.known = Some(current);
            return None;
        };
        if *known == current || !userinfo_settled {
            return None;
        }
        if self
            .last_sent
            .is_some_and(|sent| now.duration_since(sent) < MIN_INTERVAL)
        {
            return None;
        }
        self.known = Some(current);
        let command = saber_command(saber1, saber2)?;
        self.last_sent = Some(now);
        Some(command.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn japlus() -> CompatProfile {
        CompatProfile::JaPlus { version: None }
    }

    #[test]
    fn a_single_saber_sends_one_name_and_a_pair_two() {
        assert_eq!(saber_command("single_1", "none").unwrap(), "saber single_1");
        assert_eq!(saber_command("single_1", "").unwrap(), "saber single_1");
        assert_eq!(saber_command("single_1", "NONE").unwrap(), "saber single_1");
        assert_eq!(
            saber_command("single_1", "single_2").unwrap(),
            "saber single_1 single_2"
        );
        assert_eq!(
            saber_command("dual_1", "dual_1").unwrap(),
            "saber dual_1 dual_1"
        );
    }

    #[test]
    fn names_that_cannot_be_one_argument_send_nothing() {
        assert_eq!(saber_command("", "none"), None);
        assert_eq!(saber_command("a b", "none"), None);
        assert_eq!(saber_command("a;quit", "none"), None);
        assert_eq!(saber_command("a", "b\"c"), None);
    }

    #[test]
    fn only_japlus_and_japro_lineage_servers_get_the_command() {
        assert!(profile_accepts_saber_command(&japlus()));
        assert!(profile_accepts_saber_command(&CompatProfile::TaystJk));
        assert!(!profile_accepts_saber_command(&CompatProfile::BaseJka));
        assert!(!profile_accepts_saber_command(&CompatProfile::Unknown(
            "x".to_owned()
        )));
        let mut notifier = SaberChangeNotifier::default();
        let now = Instant::now();
        let stock = CompatProfile::BaseJka;
        assert_eq!(notifier.poll(&stock, "a", "none", true, now), None);
        assert_eq!(notifier.poll(&stock, "b", "none", true, now), None);
    }

    #[test]
    fn the_connect_selection_is_not_announced_and_a_change_is_once() {
        let mut notifier = SaberChangeNotifier::default();
        let t0 = Instant::now();
        let p = japlus();
        assert_eq!(notifier.poll(&p, "single_1", "none", true, t0), None);
        assert_eq!(notifier.poll(&p, "single_1", "none", true, t0), None);
        assert_eq!(
            notifier.poll(&p, "single_2", "none", true, t0).unwrap(),
            b"saber single_2"
        );
        // The same selection again, even after the interval, sends nothing.
        let later = t0 + MIN_INTERVAL * 2;
        assert_eq!(notifier.poll(&p, "single_2", "none", true, later), None);
    }

    #[test]
    fn the_command_waits_for_the_userinfo_and_for_the_interval() {
        let mut notifier = SaberChangeNotifier::default();
        let t0 = Instant::now();
        let p = japlus();
        notifier.poll(&p, "a", "none", true, t0);
        // Userinfo still queued: hold the command.
        assert_eq!(notifier.poll(&p, "b", "none", false, t0), None);
        assert!(notifier.poll(&p, "b", "none", true, t0).is_some());
        // A quick second pick waits out the interval, then sends the latest.
        let soon = t0 + MIN_INTERVAL / 2;
        assert_eq!(notifier.poll(&p, "c", "none", true, soon), None);
        assert_eq!(notifier.poll(&p, "d", "none", true, soon), None);
        assert_eq!(
            notifier
                .poll(&p, "d", "none", true, t0 + MIN_INTERVAL)
                .unwrap(),
            b"saber d"
        );
    }

    #[test]
    fn picking_a_second_saber_sends_the_pair_and_a_new_server_starts_over() {
        let mut notifier = SaberChangeNotifier::default();
        let t0 = Instant::now();
        let p = japlus();
        notifier.poll(&p, "a", "none", true, t0);
        assert_eq!(notifier.poll(&p, "a", "b", true, t0).unwrap(), b"saber a b");
        notifier.reset();
        // The next connection adopts whatever is set, without a command.
        assert_eq!(notifier.poll(&p, "x", "none", true, t0), None);
    }
}
