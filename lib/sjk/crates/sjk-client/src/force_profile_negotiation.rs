//! Keep the player's Force profile legal for the current server.
//!
//! The user's preferred `forcepowers` is a client setting; what goes on the
//! wire is a per-server legalized copy. `WP_InitForcePowers`
//! (`codemp/game/w_force.c:240-251`) strips disabled or over-budget powers,
//! and (`w_force.c:368-392`) parks any client whose profile needed stripping
//! — or whose session has not yet confirmed its powers (`!sess.setForce`) —
//! in spectator mode with `spc` + `nfr <rank> 1 <team>`: the stock "Force menu
//! on first join". [`enter_play`] sends the legalized value and then the
//! stock Force menu's own accept command, `forcechanged "<TEAM>"`, while the
//! client is still the spectator it connects as; `Cmd_ForceChanged_f`
//! (`codemp/game/g_cmds.c:1276-1313`) runs `WP_InitForcePowers` immediately,
//! marks the session and joins the team, so play starts without a bounce
//! (verified against JA+ 2.4 on the EFF server,
//! `target/parity-reports/playtest-japlus5`).
//!
//! When a server still sends `nfr <rank> 1 <team>`, stock JKA opens the Force
//! menu; closing it flushes the legalized userinfo, sends `forcechanged`, and
//! the player clicks Join again (`assets1.pk3:ui/jamp/ingame_playerforce.menu:
//! 1556-1562`). [`ForceProfileNegotiator`] performs those steps itself. Because
//! `Cmd_Team_f` refuses a second team change within five seconds of the first
//! (`codemp/game/g_cmds.c:998-1001`, `1031`), the join is retried a bounded
//! number of times, except in duel and power duel, where waiting as a
//! spectator is the queue: there `SetTeam` keeps a queued player spectating
//! but announces and respawns them on every request (`g_cmds.c:829-841`,
//! `906`, `922`), and `Cmd_Team_f` refuses any change in power duel
//! (`g_cmds.c:1016-1022`).
//!
//! The value sent is always the player's own `forcepowers` fitted to the
//! server's current rules ([`ForceLegalizeRules::for_sent_profile`]), worked
//! out each time the userinfo is built, so a profile applied in play is the
//! one the server receives. A server reads a changed profile only when told
//! to: `Cmd_ForceChanged_f` re-reads it at once for a spectator and at the
//! next respawn in play (`forceDoInit`, `g_cmds.c:1276-1296`,
//! `g_client.c:3174-3178`), so an applied profile is followed by
//! `forcechanged` ([`ForceProfileNegotiator::profile_applied`]), as stock
//! `UI_UpdateClientForcePowers` does (`codemp/ui/ui_force.c`).

use crate::force_profile::{ForceLegalizeRules, legalize_force_powers};
use crate::force_rank_reply::{
    ForceRankReply, force_rank_reply, force_rules_from_serverinfo, server_force_rules,
};
use crate::team_commands::{LegacyTeamChoice, legacy_team_command};
use crate::{ClientError, ClientSession, UserinfoUpdateStatus};
use sjk_network::LegacyUserInfo;
use sjk_protocol::{GameState, InfoString};
use std::time::{Duration, Instant};

/// `switchTeamTime` window (`g_cmds.c:1031`) plus transport slack.
const REJOIN_INTERVAL: Duration = Duration::from_millis(5_500);
/// Bounded so a server that keeps refusing never produces a join/spectate loop.
const REJOIN_ATTEMPTS: u8 = 3;

/// Fit `preferred` to the rules the server advertises in `CS_SERVERINFO`,
/// using its `g_maxForceRank` as the rank ceiling, as it is sent
/// ([`ForceLegalizeRules::for_sent_profile`]: disabled powers are kept).
pub fn server_legal_forcepowers(game_state: &GameState, preferred: &str) -> String {
    let rules = server_force_rules(game_state).for_sent_profile();
    legalize_force_powers(preferred, rules).allocation.encode()
}

/// `gametype_t` values (`bg_public.h:235-244`) for which `Cmd_ForceChanged_f`
/// ignores its team argument (`g_cmds.c:1297-1300`).
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_SIEGE: i32 = 7;
/// Ordinary `team` → first snapshot in play on a loaded server.
/// How long to wait for the server to confirm the Force profile and spawn
/// us. Reaching it is no longer a failure -- the caller enters the game
/// anyway -- so this only decides how long a join sits still on a server
/// that never answers `nfr`, which a modified game module need not do. Ten
/// seconds of that was the "takes longer to join" the owner reported.
const JOIN_TIMEOUT: Duration = Duration::from_secs(3);
/// Upper bound on the userinfo coalescing wait before joining regardless.
const USERINFO_TIMEOUT: Duration = Duration::from_secs(2);

/// What [`ForceProfileNegotiator::profile_applied`] did about a new profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileApplied {
    /// `forcechanged` is queued behind the userinfo.
    Queued,
    /// A reply that makes the server read the profile is already waiting for
    /// the userinfo, which carries this one.
    AlreadyQueued,
    /// No server's rules are known (not connected, or before its game state),
    /// so nothing was queued; the join sends the profile.
    NotOnServer,
}

/// How the client entered play.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnterPlayOutcome {
    /// The `forcepowers` value now on the wire (also written to `userinfo`).
    pub forcepowers: String,
    /// Whether the server confirmed the profile and a snapshot in play
    /// arrived before the timeout.
    ///
    /// This is not a verdict on the connection. Plenty of servers hold a
    /// fresh client in spectator until it picks a team, and a mod's game
    /// module need not answer with `nfr` at all; a stock client is simply
    /// connected, watching, with the join menu up. Treat a false here as
    /// "still spectating", never as a failed join.
    pub active: bool,
    /// The server answered the force profile (`nfr`), or the gametype never
    /// sends one.
    pub confirmed: bool,
    /// The newest snapshot still shows this client spectating.
    pub spectating: bool,
}

/// Join `team` with a server-legal Force profile, without the stock
/// spectator bounce.
///
/// Legalizes `userinfo.forcepowers` against `CS_SERVERINFO`, pushes it when
/// it changed, then sends `forcechanged "<TEAM>"`: as a spectator that runs
/// `WP_InitForcePowers` immediately (marking the session so the join does not
/// bounce) and hands the team to `Cmd_Team_f` (`g_cmds.c:1276-1313`). Duel
/// gametypes ignore that argument, so a plain `team` command follows there.
/// The `nfr` notices the exchange provokes are consumed. Must be called
/// while the client is still the spectator it connects as.
pub fn enter_play(
    session: &mut ClientSession,
    userinfo: &mut LegacyUserInfo,
    team: LegacyTeamChoice,
) -> Result<EnterPlayOutcome, ClientError> {
    let forcepowers = server_legal_forcepowers(session.game_state(), &userinfo.forcepowers);
    if forcepowers != userinfo.forcepowers {
        userinfo.forcepowers = forcepowers.clone();
        // The tracker coalesces updates to one per second from the connect
        // userinfo on; wait that out so the legal value precedes the
        // `forcechanged` instead of being skipped.
        let deadline = Instant::now() + USERINFO_TIMEOUT;
        while session.update_userinfo(userinfo, Instant::now())?
            == UserinfoUpdateStatus::RateLimited
            && Instant::now() < deadline
        {
            receive_quietly(session)?;
        }
    }
    let rules = force_rules_from_serverinfo(session.game_state(), 0, 0);
    let reply = force_rank_reply(&forcepowers, rules, Some(team));
    session.send_reliable_command(&reply.command)?;
    if matches!(
        serverinfo_i32(session.game_state(), "g_gametype"),
        GT_DUEL | GT_POWERDUEL
    ) {
        session.send_reliable_command(legacy_team_command(team))?;
    }
    // The first snapshots can still show the pre-park in-game state, so wait
    // for the server's own confirmation: `WP_InitForcePowers` answers a
    // legal, confirmed profile with `nfr <rank> 0 <team>`
    // (`w_force.c:394`). Siege never sends it (`scl` instead).
    let siege = serverinfo_i32(session.game_state(), "g_gametype") == GT_SIEGE;
    let deadline = Instant::now() + JOIN_TIMEOUT;
    let mut confirmed = siege;
    let mut active = false;
    while Instant::now() < deadline {
        receive_quietly(session)?;
        while let Some(update) = session.take_force_rank_event() {
            confirmed |= !update.open_profile;
        }
        active = confirmed && !session.latest_snapshot().player.is_spectator();
        if active {
            break;
        }
    }
    let spectating = session.latest_snapshot().player.is_spectator();
    Ok(EnterPlayOutcome {
        forcepowers,
        active,
        confirmed,
        spectating,
    })
}

/// One receive step that also releases paced commands; timeouts are normal.
fn receive_quietly(session: &mut ClientSession) -> Result<(), ClientError> {
    match session.receive_snapshot(Duration::from_millis(100)) {
        Ok(_) => Ok(()),
        Err(error) if error.is_timeout() => Ok(()),
        Err(error) => Err(error),
    }
}

fn serverinfo_i32(game_state: &GameState, key: &str) -> i32 {
    game_state
        .config_string(0)
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|text| InfoString::parse(text).ok())
        .and_then(|info| info.get_i32(key))
        .unwrap_or(0)
}

/// Reliable commands the shell must send this frame, in order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RejoinOutput {
    /// Byte-exact reliable commands, in send order.
    pub commands: Vec<Vec<u8>>,
    /// Console notice once the bounded retries are exhausted.
    pub notice: Option<String>,
}

#[derive(Debug)]
struct Retry {
    due: Instant,
    attempts_left: u8,
}

/// `forcechanged` without a team: the server re-reads the profile and the
/// player stays where they are (`setForce "none"`, `ui_main.c:6335-6345`).
const FORCE_CHANGED: &[u8] = b"forcechanged";

/// Per-connection Force-profile state: the server's rules the sent profile is
/// fitted to and the `nfr` reply / rejoin machine.
#[derive(Debug)]
pub struct ForceProfileNegotiator {
    /// The server's Force rules, `g_forcePowerDisable` included; `None` off a
    /// server, where the profile goes out as the player wrote it.
    rules: Option<ForceLegalizeRules>,
    desired_team: LegacyTeamChoice,
    pending: Option<Vec<u8>>,
    retry: Option<Retry>,
}

impl Default for ForceProfileNegotiator {
    fn default() -> Self {
        Self {
            rules: None,
            desired_team: LegacyTeamChoice::Free,
            pending: None,
            retry: None,
        }
    }
}

impl ForceProfileNegotiator {
    /// The Force rules of the server this connection plays on, its
    /// `g_forcePowerDisable` included.
    pub fn server_rules(&self) -> Option<ForceLegalizeRules> {
        self.rules
    }

    /// Adopt the server's rules (`None` once off a server).
    pub fn set_server_rules(&mut self, rules: Option<ForceLegalizeRules>) {
        self.rules = rules;
    }

    /// Adopt the rules `CS_SERVERINFO` advertises now, as it changes during a
    /// connection. The side an `nfr` notice held the player to under
    /// `g_forceBasedTeams` is kept: `CS_SERVERINFO` does not carry it.
    pub fn refresh_server_rules(&mut self, game_state: &GameState) {
        let mut rules = server_force_rules(game_state);
        if rules.team_side.is_none() && serverinfo_i32(game_state, "g_forceBasedTeams") != 0 {
            rules.team_side = self.rules.and_then(|held| held.team_side);
        }
        self.rules = Some(rules);
    }

    /// The `forcepowers` value sent for the player's `preferred` one: fitted
    /// to the server's rules while on one, keeping the powers it disables.
    pub fn sent_forcepowers(&self, preferred: &str) -> String {
        match self.rules {
            Some(rules) => legalize_force_powers(preferred, rules.for_sent_profile())
                .allocation
                .encode(),
            None => preferred.to_owned(),
        }
    }

    /// Replace the preferred profile in `userinfo` with the value sent for it.
    pub fn apply_to_userinfo(&self, userinfo: &mut LegacyUserInfo) {
        if self.rules.is_some() {
            userinfo.forcepowers = self.sent_forcepowers(&userinfo.forcepowers);
        }
    }

    /// The player applied a new profile: once its userinfo has gone out, send
    /// `forcechanged` so the server reads it, at once for a spectator and at
    /// the next respawn in play. Nothing to do off a server; a queued `nfr`
    /// reply already does it.
    pub fn profile_applied(&mut self) -> ProfileApplied {
        if self.rules.is_none() {
            ProfileApplied::NotOnServer
        } else if self.pending.is_some() {
            ProfileApplied::AlreadyQueued
        } else {
            self.pending = Some(FORCE_CHANGED.to_vec());
            ProfileApplied::Queued
        }
    }

    /// Remember the team the player last asked for; spectator disables rejoin.
    pub fn note_team_choice(&mut self, choice: LegacyTeamChoice) {
        self.desired_team = choice;
        if choice == LegacyTeamChoice::Spectator {
            self.pending = None;
            self.retry = None;
        }
    }

    /// The team a `forcechanged` reply should return the player to.
    pub fn rejoin_team(&self) -> Option<LegacyTeamChoice> {
        (self.desired_team != LegacyTeamChoice::Spectator).then_some(self.desired_team)
    }

    /// Queue the reply; it is sent once the userinfo has reached the server.
    /// The team is asked for again later while the player stays parked,
    /// except where spectating is the duel queue.
    pub fn queue(&mut self, reply: &ForceRankReply, now: Instant) {
        self.pending = Some(reply.command.clone());
        let duel = self
            .rules
            .is_some_and(|rules| matches!(rules.gametype, GT_DUEL | GT_POWERDUEL));
        self.retry = self.rejoin_team().filter(|_| !duel).map(|_| Retry {
            due: now + REJOIN_INTERVAL,
            attempts_left: REJOIN_ATTEMPTS,
        });
    }

    /// Drop all per-connection state, e.g. on disconnect or a new server.
    pub fn reset(&mut self) {
        self.rules = None;
        self.pending = None;
        self.retry = None;
    }

    /// Advance the state machine for one frame; call it every frame, as a
    /// retry falls due seconds after the userinfo before it went out.
    ///
    /// `userinfo_settled` is true when no userinfo change is still waiting to be
    /// sent; `spectator` reflects the latest snapshot.
    pub fn poll(&mut self, now: Instant, userinfo_settled: bool, spectator: bool) -> RejoinOutput {
        let mut output = RejoinOutput::default();
        if userinfo_settled {
            if let Some(command) = self.pending.take() {
                output.commands.push(command);
            }
        }
        let Some(retry) = &mut self.retry else {
            return output;
        };
        if self.pending.is_some() || now < retry.due {
            return output;
        }
        if !spectator {
            self.retry = None;
            return output;
        }
        if retry.attempts_left == 0 {
            self.retry = None;
            output.notice = Some(
                "^3Server kept you in spectator mode after the Force profile update; \
                 use the join menu."
                    .to_owned(),
            );
            return output;
        }
        retry.attempts_left -= 1;
        retry.due = now + REJOIN_INTERVAL;
        output
            .commands
            .push(legacy_team_command(self.desired_team).to_vec());
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::force_profile::ForceAllocation;

    /// A server at Jedi Adept (rank 4, 30 points) with Heal (bit 0) and
    /// Grip (bit 6) disabled, in a free-for-all.
    fn adept_server() -> GameState {
        let mut game = GameState::empty_local(0);
        game.replace_config_string(
            0,
            b"\\g_gametype\\0\\g_maxForceRank\\4\\g_forcePowerDisable\\65".to_vec(),
        )
        .unwrap();
        game
    }

    fn levels(value: &str) -> [u8; 18] {
        ForceAllocation::parse(value).unwrap().levels
    }

    #[test]
    fn the_sent_profile_keeps_disabled_powers_within_the_servers_points() {
        let game = adept_server();
        // Light side: Heal 3 (12 points), Jump 1, Push 2 (4), Saber Attack 2 (6).
        let preferred = "7-1-310200000000000200";
        let sent = server_legal_forcepowers(&game, preferred);
        assert_eq!(sent, "4-1-310200000000000200");
        let rules = server_force_rules(&game);
        assert_eq!(rules.max_rank, 4);
        assert_eq!(rules.disabled_mask, 65);
        // What the server grants drops Heal, as `WP_InitForcePowers` does,
        // without calling the profile illegal.
        let granted = legalize_force_powers(&sent, rules);
        assert!(granted.was_legal);
        assert_eq!(granted.allocation.levels[0], 0);
        // Over the 30 points, the sent profile is cut down to fit, disabled
        // powers counted, so the server never parks the player for it.
        let greedy = server_legal_forcepowers(&game, "7-1-333330000000003333");
        let fitted = ForceAllocation::parse(&greedy).unwrap();
        assert!(fitted.used_points(false) <= 30, "{greedy}");
        assert!(legalize_force_powers(&greedy, rules.for_sent_profile()).was_legal);
    }

    #[test]
    fn a_serverinfo_refresh_keeps_the_side_an_nfr_notice_set() {
        use crate::force_profile::ForceSide;
        let mut game = adept_server();
        let mut negotiator = ForceProfileNegotiator::default();
        let held = ForceLegalizeRules {
            team_side: Some(ForceSide::Dark),
            ..server_force_rules(&game)
        };
        negotiator.set_server_rules(Some(held));
        // Force-based teams off: the refresh has no side to keep.
        negotiator.refresh_server_rules(&game);
        assert_eq!(negotiator.server_rules().unwrap().team_side, None);
        // On: the side from the notice survives a serverinfo change.
        game.replace_config_string(
            0,
            b"\\g_gametype\\0\\g_maxForceRank\\4\\g_forceBasedTeams\\1".to_vec(),
        )
        .unwrap();
        negotiator.set_server_rules(Some(held));
        negotiator.refresh_server_rules(&game);
        assert_eq!(
            negotiator.server_rules().unwrap().team_side,
            Some(ForceSide::Dark)
        );
    }

    #[test]
    fn the_applied_profile_is_the_one_sent_fitted_to_the_server() {
        let game = adept_server();
        let mut negotiator = ForceProfileNegotiator::default();
        let first = "7-1-310200000000000200";
        assert_eq!(negotiator.sent_forcepowers(first), first, "off a server");
        negotiator.set_server_rules(Some(server_force_rules(&game)));
        assert_eq!(negotiator.sent_forcepowers(first), "4-1-310200000000000200");
        // A profile applied later goes out as applied, not as the join's.
        let later = negotiator.sent_forcepowers("7-2-011110300000000300");
        assert_eq!(levels(&later)[6], 3, "Grip kept");
        assert_eq!(later, "4-2-011110300000000300");
        negotiator.reset();
        assert_eq!(negotiator.server_rules(), None);
        assert_eq!(negotiator.sent_forcepowers(first), first);
    }

    #[test]
    fn an_applied_profile_is_followed_by_forcechanged_once_its_userinfo_is_out() {
        let now = Instant::now();
        let mut negotiator = ForceProfileNegotiator::default();
        assert_eq!(negotiator.profile_applied(), ProfileApplied::NotOnServer);
        assert!(
            negotiator.poll(now, true, false).commands.is_empty(),
            "off a server"
        );
        negotiator.set_server_rules(Some(ForceLegalizeRules::default()));
        assert_eq!(negotiator.profile_applied(), ProfileApplied::Queued);
        assert_eq!(negotiator.profile_applied(), ProfileApplied::AlreadyQueued);
        assert!(negotiator.poll(now, false, false).commands.is_empty());
        let output = negotiator.poll(now, true, false);
        assert_eq!(output.commands, vec![b"forcechanged".to_vec()]);
        assert!(negotiator.poll(now, true, false).commands.is_empty());
        // A queued `nfr` reply, which rejoins the team, is not replaced.
        let reply = force_rank_reply(
            "7-1-032330000000001333",
            ForceLegalizeRules::default(),
            Some(LegacyTeamChoice::Free),
        );
        negotiator.queue(&reply, now);
        assert_eq!(negotiator.profile_applied(), ProfileApplied::AlreadyQueued);
        let output = negotiator.poll(now, true, false);
        assert_eq!(output.commands, vec![b"forcechanged \"FREE\"".to_vec()]);
    }

    #[test]
    fn a_parked_duel_player_waits_in_the_queue() {
        let now = Instant::now();
        for gametype in [GT_DUEL, GT_POWERDUEL] {
            let mut negotiator = ForceProfileNegotiator::default();
            negotiator.set_server_rules(Some(ForceLegalizeRules {
                gametype,
                ..ForceLegalizeRules::default()
            }));
            let reply = force_rank_reply(
                "7-1-032330000000001333",
                ForceLegalizeRules::default(),
                Some(LegacyTeamChoice::Free),
            );
            negotiator.queue(&reply, now);
            assert_eq!(negotiator.poll(now, true, true).commands.len(), 1);
            for step in 1..=REJOIN_ATTEMPTS + 1 {
                let at = now + REJOIN_INTERVAL * u32::from(step);
                assert_eq!(negotiator.poll(at, true, true), RejoinOutput::default());
            }
        }
    }

    #[test]
    fn a_parked_player_is_sent_back_to_the_team_until_the_retries_run_out() {
        let now = Instant::now();
        let mut negotiator = ForceProfileNegotiator::default();
        let reply = force_rank_reply(
            "7-1-032330000000001333",
            ForceLegalizeRules::default(),
            Some(LegacyTeamChoice::Red),
        );
        negotiator.note_team_choice(LegacyTeamChoice::Red);
        negotiator.queue(&reply, now);
        assert_eq!(negotiator.poll(now, true, true).commands.len(), 1);
        let mut at = now;
        for _ in 0..REJOIN_ATTEMPTS {
            at += REJOIN_INTERVAL;
            let output = negotiator.poll(at, true, true);
            assert_eq!(output.commands, vec![b"team red".to_vec()]);
        }
        at += REJOIN_INTERVAL;
        assert!(negotiator.poll(at, true, true).notice.is_some());
        assert_eq!(
            negotiator.poll(at + REJOIN_INTERVAL, true, true),
            RejoinOutput::default()
        );
    }
}
