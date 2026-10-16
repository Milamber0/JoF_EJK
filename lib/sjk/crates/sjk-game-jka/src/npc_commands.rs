//! What the rest of the game does to the roster's NPCs in a fight: a blow on one
//! (`G_Damage`, [`crate::npc_damage`]), a player's death that an NPC caused or that its
//! teammates may avenge (`player_die`'s NPC half, `G_DeathAlert`), and the `npc` command's
//! `kill`, `score` and `showbounds` (`NPC_Kill_f`, `NPC_PrintScore`, `Cmd_NPC_f`,
//! `codemp/game/NPC_spawn.c:4126-4324`).
//!
//! Held to `tools/game-oracle/npccombat.c` (`game-npccombat.txt`).

use crate::npc_damage::{NpcBlow, NpcDamaged};
use crate::npc_roster::{Fired, NpcRoster};
use crate::npc_senses::Body;
use crate::npc_spawn::NpcHost;

/// `MOD_UNKNOWN`.
const MOD_UNKNOWN: u32 = 0;
/// `STAT_HEALTH`, `PERS_SCORE`.
const STAT_HEALTH: usize = 0;
const PERS_SCORE: usize = 0;
/// `NPCTEAM_FREE`, `NPCTEAM_PLAYER`.
const NPCTEAM_FREE: i32 = 0;
const NPCTEAM_PLAYER: i32 = 2;
/// `DEATH_ALERT_RADIUS`, `DEATH_ALERT_SOUND_RADIUS`.
const DEATH_ALERT_RADIUS: f32 = 512.0;
/// `TeamNames` from `TEAM_FREE + 1` on, as the errors list them.
const TEAM_NAMES: [&str; 3] = ["player", "enemy", "neutral"];

/// `GetIDForString(TeamTable, name)` (`NPC_stats.c:34-41`): the team's number, -1 for a
/// name the table does not have.
fn team_by_name(name: &[u8]) -> i32 {
    [
        ("NPCTEAM_FREE", 0),
        ("NPCTEAM_PLAYER", 2),
        ("NPCTEAM_ENEMY", 1),
        ("NPCTEAM_NEUTRAL", 3),
    ]
    .into_iter()
    .find(|(known, _)| known.as_bytes().eq_ignore_ascii_case(name))
    .map_or(-1, |(_, team)| team)
}

/// A name as `%s` prints it: `(null)` for none.
fn named(name: Option<&[u8]>) -> String {
    name.map_or_else(
        || "(null)".to_owned(),
        |name| String::from_utf8_lossy(name).into_owned(),
    )
}

/// Who `NPC_Kill_f` kills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Victims<'a> {
    /// `kill team nonally`: everyone not on the player's team, players too, and the
    /// spawners removed.
    NotAllies,
    /// `kill team <team>`: the NPCs of the team (-1 for a name the table does not have).
    Team(i32),
    /// `kill <targetname>` or `kill all`.
    Named(&'a [u8]),
}

impl NpcRoster {
    /// `G_Damage` on the NPC numbered `target` at `level_time`, `None` for a number no NPC
    /// has; what it came to, and the names its pain or death fired.
    pub fn damage(
        &mut self,
        target: u16,
        blow: NpcBlow,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> Option<(NpcDamaged, Fired)> {
        let at = self.actors.iter().position(|npc| npc.number == target)?;
        let mut fired = Fired::new();
        let damaged = self.world(level_time, host, &mut fired).damage(at, blow);
        if let Some(attacker) = blow.request.attacker.filter(|attacker| attacker.npc) {
            self.record_hit(
                attacker.client,
                damaged.attacker_hits,
                damaged.attackee_armor,
            );
        }
        Some((damaged, fired))
    }

    /// `saberKnockOutOfHand` on NPC `npc`'s saber at `velocity` (a player's blade knocked it
    /// away, `w_saber.c`'s clash rules): its flight, as an NPC's own blade knocks it out.
    /// Whether it flew; `false` for a number no NPC has.
    pub fn knock_saber_out(
        &mut self,
        npc: u16,
        velocity: [f32; 3],
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> bool {
        let Some(at) = self.actors.iter().position(|actor| actor.number == npc) else {
            return false;
        };
        let mut fired = Fired::new();
        self.world(level_time, host, &mut fired)
            .with_npc_saber(at, |saber, world| {
                crate::saber_drop::knock_out_of_hand(saber, world, velocity, level_time)
            })
            .unwrap_or(false)
    }

    /// `saberCheckKnockdown_Smashed` on NPC `npc`'s thrown saber, struck by `striker`'s
    /// blade (`defending`: in an extra defence move) for `damage`. Whether it was knocked
    /// out of the air; `false` for a number no NPC has.
    pub fn smash_saber(
        &mut self,
        npc: u16,
        striker: u16,
        defending: bool,
        damage: i32,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> bool {
        let Some(at) = self.actors.iter().position(|actor| actor.number == npc) else {
            return false;
        };
        let mut fired = Fired::new();
        self.world(level_time, host, &mut fired)
            .with_npc_saber(at, |saber, world| {
                crate::saber_drop::smashed(saber, world, striker, defending, damage, level_time)
            })
            .unwrap_or(false)
    }

    /// NPC `npc` beaten in a lock by a player (`pmove.checkDuelLoss` after the player's
    /// move, `g_active.c:3072-3107`): `attacker` standing at `origin`, its blade's readings
    /// `storage` and its sabers' disarm `chance`. Returns what the NPC's death fired.
    #[allow(clippy::too_many_arguments)]
    pub fn lost_lock_to_player(
        &mut self,
        npc: u16,
        attacker: crate::damage::Attacker,
        origin: [f32; 3],
        storage: &crate::saber_clash::SaberStorage,
        chance: i32,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        if let Some(at) = self.actors.iter().position(|actor| actor.number == npc) {
            self.world(level_time, host, &mut fired)
                .npc_lost_lock(at, attacker, origin, storage, chance);
        }
        fired
    }

    /// A player's move touched NPC `npc` (`ClientImpacts` → `NPC_Touch`,
    /// [`crate::npc_touch`]). Returns what it fired.
    pub fn player_touch(
        &mut self,
        npc: u16,
        player: u16,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        if let Some(at) = self.actors.iter().position(|actor| actor.number == npc) {
            self.world(level_time, host, &mut fired)
                .npc_touch(at, player);
        }
        fired
    }

    /// `G_Damage`'s hit counter for an NPC attacker (`g_combat.c:4895-4905`): `PERS_HITS`
    /// by `hits` (a foe's +1, a teammate's -1) and `PERS_ATTACKEE_ARMOR` — nothing when the
    /// blow counted none.
    pub fn record_hit(&mut self, attacker: u16, hits: i32, attackee_armor: Option<u32>) {
        const PERS_HITS: usize = 1;
        const PERS_ATTACKEE_ARMOR: usize = 7;
        if hits == 0 {
            return;
        }
        if let Some(shooter) = self.actors.iter_mut().find(|npc| npc.number == attacker) {
            shooter.player.persistent[PERS_HITS] =
                (shooter.player.persistent[PERS_HITS] as i32 + hits) as u32;
            shooter.player.persistent[PERS_ATTACKEE_ARMOR] = attackee_armor.unwrap_or(0);
        }
    }

    /// `G_MissileImpact` after a missile's damage on an NPC (`g_missile.c:700-716`): a droid
    /// sparks for a split second, unless it is already.
    pub fn missile_struck(&mut self, number: u16, level_time: i32) {
        const DROIDS: [i32; 12] = [41, 32, 29, 11, 34, 35, 39, 23, 24, 16, 1, 42];
        const PS_ELECTRIFY_TIME: usize = 73;
        let Some(npc) = self.actors.iter_mut().find(|npc| npc.number == number) else {
            return;
        };
        if DROIDS.contains(&npc.definition.client_class)
            && (npc.player.raw_field(PS_ELECTRIFY_TIME).unwrap_or(0) as i32) < level_time + 100
        {
            npc.player
                .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 450) as u32);
        }
    }

    /// Whether an NPC of the roster has entity number `number`.
    pub fn is_npc(&self, number: u16) -> bool {
        self.actors.iter().any(|npc| npc.number == number)
    }

    /// `player_die`'s NPC half for a player's death (`g_combat.c:2515-2600`, `2805-2808`):
    /// an NPC killer's victory, point and awards; then — unless the player killed itself —
    /// `G_DeathAlert`: the NPCs of the player's team near it take the killer on.
    pub fn player_killed(
        &mut self,
        victim: Body,
        killer: Option<u16>,
        means: u32,
        victim_was_master: bool,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        let mut world = self.world(level_time, host, &mut fired);
        if let Some(killer) = killer {
            world.forget_victim(killer, victim.number);
        }
        if let Some(killer) = killer.filter(|killer| world.actor_at(*killer).is_some()) {
            crate::npc_death::npc_killed_player(&mut world, killer, means, victim_was_master);
        }
        if let Some(attacker) = killer
            .filter(|killer| *killer != victim.number)
            .and_then(|killer| world.body(killer))
        {
            world.alert_team(
                &victim,
                None,
                &attacker,
                DEATH_ALERT_RADIUS,
                DEATH_ALERT_RADIUS,
            );
        }
        fired
    }

    /// `NPC_Kill_f` (`NPC_spawn.c:4126-4250`) for `npc kill <name> [team]`: the errors a
    /// missing or unknown name gets, else — over the level's entities in number order —
    /// each NPC (and for `nonally` each player not on the player's team, and each spawner)
    /// named and killed (`die`, `MOD_UNKNOWN`, by itself). Returns what the deaths fired.
    pub fn kill_command(
        &mut self,
        name: &[u8],
        team: &[u8],
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        if name.is_empty() {
            for line in [
                "Error, Expected:\n",
                "NPC kill '[NPC targetname]' - kills NPCs with certain targetname\n",
                "or\n",
                "NPC kill 'all' - kills all NPCs\n",
                "or\n",
                "NPC team '[teamname]' - kills all NPCs of a certain team ('nonally' is all but your allies)\n",
            ] {
                host.print(&format!("^1{line}"));
            }
            return fired;
        }
        let victims = if name.eq_ignore_ascii_case(b"team") {
            if team.is_empty() {
                host.print("^1NPC_Kill Error: 'npc kill team' requires a team name!\n");
                list_teams(host);
                return fired;
            }
            if team.eq_ignore_ascii_case(b"nonally") {
                Victims::NotAllies
            } else {
                let number = team_by_name(team);
                if number == NPCTEAM_FREE {
                    host.print(&format!(
                        "^1NPC_Kill Error: team '{}' not recognized\n",
                        String::from_utf8_lossy(team)
                    ));
                    list_teams(host);
                    return fired;
                }
                Victims::Team(number)
            }
        } else {
            Victims::Named(name)
        };
        // Everyone the loop meets, by entity number: players, NPCs, spawners.
        let mut everyone: Vec<(u16, u8)> = host
            .players()
            .iter()
            .filter(|player| player.number >= 1)
            .map(|player| (player.number, 0))
            .collect();
        everyone.extend(self.actors.iter().map(|npc| (npc.number, 1)));
        everyone.extend(self.spawners.iter().map(|(number, _)| (*number, 2)));
        everyone.sort_unstable();
        for (number, kind) in everyone {
            match (kind, victims) {
                (0, Victims::NotAllies) => {
                    if host.players().iter().any(|player| {
                        player.number == number && player.player_team != NPCTEAM_PLAYER
                    }) {
                        host.print(&format!(
                            "^2Killing NPC {} named {}\n",
                            named(None),
                            named(None)
                        ));
                        host.kill_player(number);
                    }
                }
                (1, _) => fired.append(&mut self.kill_one(number, victims, level_time, host)),
                (2, Victims::NotAllies) => {
                    let Some(at) = self.spawners.iter().position(|(known, _)| *known == number)
                    else {
                        continue;
                    };
                    let spawner = &self.spawners[at].1;
                    if spawner.npc_type.is_some() {
                        host.print(&format!(
                            "^2Removing NPC spawner {} with NPC named {}\n",
                            named(spawner.npc_type.as_deref()),
                            named(spawner.npc_targetname.as_deref())
                        ));
                        self.spawners.remove(at);
                        host.free(number);
                    }
                }
                _ => {}
            }
        }
        fired
    }

    /// One NPC of `npc kill`, if it is among the victims: named on the console, its health
    /// (and for a name or `all` its stat) at zero, and `die`d by itself.
    fn kill_one(
        &mut self,
        number: u16,
        victims: Victims<'_>,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        let Some(at) = self.actors.iter().position(|npc| npc.number == number) else {
            return fired;
        };
        let npc = &mut self.actors[at];
        let (killed, damage) = match victims {
            Victims::NotAllies => (npc.player_team != NPCTEAM_PLAYER, npc.max_health),
            Victims::Team(team) => (npc.player_team == team, npc.max_health),
            Victims::Named(name) => (
                npc.targetname
                    .as_deref()
                    .is_some_and(|known| known.eq_ignore_ascii_case(name))
                    || name.eq_ignore_ascii_case(b"all"),
                100,
            ),
        };
        if !killed {
            return fired;
        }
        host.print(&format!(
            "^2Killing NPC {} named {}\n",
            String::from_utf8_lossy(&npc.npc_type),
            named(npc.targetname.as_deref())
        ));
        npc.health = 0;
        if matches!(victims, Victims::Named(_)) {
            npc.player.stats[STAT_HEALTH] = 0;
        }
        self.world(level_time, host, &mut fired)
            .die(at, number, damage, MOD_UNKNOWN);
        fired
    }

    /// `npc score [targetname]` (`NPC_spawn.c:4295-4320`, `NPC_PrintScore`): every client's
    /// score — the players' slots, then the NPCs — or the one named; `(null)` for a client
    /// with no `targetname`.
    pub fn score_command(&self, name: &[u8], host: &mut impl NpcHost) {
        let score = |npc: &crate::npc_spawn::NpcActor| {
            format!(
                "{}: {}\n",
                named(npc.targetname.as_deref()),
                npc.player.persistent[PERS_SCORE] as i32
            )
        };
        if name.is_empty() {
            host.print("SCORE LIST:\n");
            for client in 0..host.client_slots() {
                let line = format!("(null): {}\n", host.player_score(client));
                host.print(&line);
            }
            let mut npcs: Vec<&crate::npc_spawn::NpcActor> = self.actors.iter().collect();
            npcs.sort_by_key(|npc| npc.number);
            for npc in npcs {
                host.print(&score(npc));
            }
            return;
        }
        // `G_Find` by `targetname`: the first entity in use that has it — a spawner or
        // another of the map's entities before the NPC answers with no client.
        let npc = self
            .actors
            .iter()
            .filter(|npc| {
                npc.targetname
                    .as_deref()
                    .is_some_and(|known| known.eq_ignore_ascii_case(name))
            })
            .min_by_key(|npc| npc.number);
        let spawner = self
            .spawners
            .iter()
            .filter(|(_, spawner)| {
                spawner
                    .targetname
                    .as_deref()
                    .is_some_and(|known| known.eq_ignore_ascii_case(name))
            })
            .map(|(number, _)| *number)
            .min();
        let other = host.named_entity(name);
        match npc {
            Some(npc)
                if spawner.is_none_or(|spawner| spawner > npc.number)
                    && other.is_none_or(|other| other > npc.number) =>
            {
                host.print(&score(npc))
            }
            _ => host.print(&format!(
                "ERROR: NPC score - no such NPC {}\n",
                String::from_utf8_lossy(name)
            )),
        }
    }

    /// `npc showbounds` (`showBBoxes`): toggled. The boxes it draws are the local renderer's
    /// debug drawing (`G_Cube`, empty in multiplayer): nothing a server shows.
    pub fn toggle_bounds(&mut self) {
        self.show_bounds = !self.show_bounds;
    }
}

/// `NPC_Kill_f`'s list of team names.
fn list_teams(host: &mut impl NpcHost) {
    host.print("^1Valid team names are:\n");
    for name in TEAM_NAMES {
        host.print(&format!("^1{name}\n"));
    }
    host.print("^1nonally - kills all but your teammates\n");
}
