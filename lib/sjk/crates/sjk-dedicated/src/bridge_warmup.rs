//! The pre-match warmup on this server (`g_doWarmup`, `g_warmup`): `level.warmupTime` is
//! `limits.warmup_time`, which the exit rules already read. The rules are
//! `sjk_game_jka::warmup`; this is where they meet the server's players, strings, log and
//! restart.

use super::NativeGame;
use sjk_game_jka::warmup::{self, Players, Step};
use sjk_game_jka::worldspawn::CS_WARMUP;

impl NativeGame {
    /// `SP_worldspawn`'s warmup (`g_spawn.c:1480-1488`), as a level begins: none after the
    /// warmup's own restart (`g_restarted`, cleared here); otherwise `g_doWarmup` has the
    /// level wait for players.
    pub(in crate::bridge) fn start_warmup(&mut self) {
        let restarted = self.cvars.integer(b"g_restarted") != 0;
        if restarted {
            self.set_cvar("g_restarted", "0");
        }
        let do_warmup = self.cvars.integer(b"g_doWarmup") != 0;
        self.limits.warmup_time =
            warmup::at_level_start(do_warmup, restarted, self.gametype).unwrap_or(0);
        // `SP_worldspawn` clears `CS_WARMUP` first, whatever follows.
        if self.limits.warmup_time == -1 {
            self.set_config_string(CS_WARMUP, b"-1");
            self.log("Warmup:");
        } else {
            self.set_config_string(CS_WARMUP, b"");
        }
    }

    /// `CheckTournament` for a game type that is not a duel (`g_main.c:2486-2544`), once a
    /// frame: waiting, the countdown, and the restart that ends it.
    pub(in crate::bridge) fn check_warmup(&mut self, server_time: i32) {
        if self.limits.warmup_time == 0 {
            return;
        }
        let team = |team: i32| {
            (0..self.players.places())
                .filter(|&client| {
                    self.peer(client)
                        .is_some_and(|peer| peer.session.team == team)
                })
                .count() as i32
        };
        let players = Players {
            playing: self.sorted().1 as i32,
            red: team(1),
            blue: team(2),
        };
        let g_warmup = self.cvars.integer(b"g_warmup");
        match warmup::check(
            &mut self.limits.warmup_time,
            self.gametype,
            players,
            g_warmup,
            server_time,
        ) {
            Step::Unchanged => {}
            Step::Waiting => {
                self.set_config_string(CS_WARMUP, b"-1");
                self.log("Warmup:");
            }
            Step::Countdown(until) => {
                self.set_config_string(CS_WARMUP, until.to_string().as_bytes())
            }
            Step::Restart => {
                // `g_restarted 1` and `map_restart 0`, run before the next frame.
                self.set_cvar("g_restarted", "1");
                self.queue_restart(server_time);
            }
        }
    }

    /// `CalculateRanks`' end of a warmup (`g_main.c:1122-1123`): `g_warmup 0` or siege.
    pub(in crate::bridge) fn warmup_after_ranks(&mut self) {
        self.limits.warmup_time = warmup::after_ranks(
            self.limits.warmup_time,
            self.cvars.integer(b"g_warmup"),
            self.gametype,
        );
    }

    /// `AddScore`'s first gate (`g_combat.c:469`): nobody scores during the warmup.
    pub(in crate::bridge) fn scoring(&self) -> bool {
        self.limits.warmup_time == 0
    }
}
