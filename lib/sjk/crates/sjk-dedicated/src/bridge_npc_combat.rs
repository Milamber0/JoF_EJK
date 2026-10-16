//! NPCs in the server's fights: a blow's target and attacker may be an NPC as well as a
//! player. A missile, a splash, a punch or any other `G_Damage` this server deals reaches
//! an NPC through the roster ([`sjk_game_jka::npc_commands`], `G_Damage`'s NPC branches);
//! an NPC's missiles and blows reach players as any attacker's do, the NPC named as their
//! attacker (`Attacker::npc`); a player's death feeds the NPCs (an NPC killer's point and
//! victory, `G_DeathAlert`); and what the roster did to the players — scores, awards, a
//! player taken for an enemy, the log — is applied here ([`NpcOutcome`]).

use super::super::*;
use super::Npcs;
use super::host::NpcOutcome;
use sjk_game_jka::npc_damage::NpcBlow;

/// `MOD_STUN_BATON`; `CARNAGE_REWARD_TIME`; `PERS_EXCELLENT_COUNT`,
/// `PERS_GAUNTLET_FRAG_COUNT`.
const MOD_STUN_BATON: u32 = 1;
const CARNAGE_REWARD_TIME: i32 = 3_000;
const PERS_EXCELLENT: usize = 10;
const PERS_GAUNTLET: usize = 13;
/// `MOD_UNKNOWN`.
const MOD_UNKNOWN: u32 = 0;

impl NativeGame {
    /// The attacker `G_Damage` is given for entity `number`: a player in the game, or an
    /// NPC (which has no handicap, and a session team of its own); `None` for anything else.
    pub(crate) fn attacker_for(&self, number: u16) -> Option<Attacker> {
        if let Some(shooter) = self.peer(usize::from(number)) {
            return Some(Attacker {
                npc: false,
                client: number,
                max_health: shooter.state.max_health(),
                team: shooter.session.team,
                saber_knockback: [0.0; 4],
            });
        }
        let Some(npc) = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)
        else {
            return self.turret_attacker(number);
        };
        Some(Attacker {
            npc: true,
            client: number,
            max_health: npc.player.stats[8] as i32,
            team: npc.session_team,
            saber_knockback: [0.0; 4],
        })
    }

    /// `G_MissileImpact`'s damage on the player or NPC it struck: `G_Damage` with the
    /// missile's velocity and position, the shooter's hit counter, and the death when the
    /// blow kills. Whether the hit counted for the shooter's accuracy.
    pub(crate) fn missile_hit(
        &mut self,
        target: usize,
        missile: &Missile,
        level_time: i32,
    ) -> bool {
        if missile.activator.is_some() {
            return self.gun_missile_hit(target, missile, level_time);
        }
        if self.duel_spares(target, missile.owner) {
            return false;
        }
        let Some(attacker) = self.attacker_for(missile.owner) else {
            return false;
        };
        let request = DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: Some(missile.impact_velocity),
            point: Some(missile.impact_point),
            damage: missile.damage,
            flags: missile.damage_flags,
            means: missile.method_of_death,
        };
        let counted = self
            .strike(usize::from(missile.owner), target, request, true)
            .1;
        // A droid struck sparks for a split second (`g_missile.c:700-716`).
        self.npcs.roster.missile_struck(target as u16, level_time);
        counted
    }

    /// A blow by `owner` on `target`: `LogAccuracyHit` (a living foe — teammates only
    /// exist in team games), `G_Damage`, and the shooter's hit counters. Returns what the
    /// damage came to.
    pub(crate) fn strike(
        &mut self,
        owner: usize,
        target: usize,
        request: DamageRequest,
        count: bool,
    ) -> (Damaged, bool) {
        self.strike_at(owner, target, request, None, count)
    }

    /// [`Self::strike`], placed where the caller says (see [`damage_at`]). An NPC target
    /// takes the blow through the roster; an NPC shooter keeps its own hit counter.
    pub(crate) fn strike_at(
        &mut self,
        owner: usize,
        target: usize,
        request: DamageRequest,
        location: Option<HitLocation>,
        count: bool,
    ) -> (Damaged, bool) {
        // An emplaced gun a blow reaches is no client: `G_Damage`'s thing (`bridge_emplaced`).
        if self.hurt_gun(
            target as u16,
            request.damage,
            request.attacker.map(|attacker| attacker.client),
            request.level_time,
        ) {
            return (Damaged::default(), false);
        }
        if self.npcs.roster.is_npc(target as u16) {
            return self.strike_npc(owner, target as u16, request, location, count);
        }
        if self.turret_spares(target, owner)
            || u16::try_from(target).is_ok_and(|target| {
                self.strike_turret(target, request)
                    || self.strike_siege_item(target, request)
                    || self.strike_shield(target, request)
            })
        {
            return (Damaged::default(), false);
        }
        let Some(victim) = self.peer_mut(target) else {
            return (Damaged::default(), false);
        };
        let team = request.attacker.map_or(0, |attacker| attacker.team);
        let by_npc = request.attacker.is_some_and(|attacker| attacker.npc);
        let counts = victim.health > 0
            && owner != target
            && !(team == victim.session.team && team != 0 && !by_npc);
        let damaged = self.hurt_at(target, request, location);
        self.count_hit(owner, counts && count, &damaged);
        (damaged, counts)
    }

    /// The shooter's counters after a blow: a player's accuracy and hits, an NPC's hits.
    fn count_hit(&mut self, owner: usize, counted: bool, damaged: &Damaged) {
        if let Some(shooter) = self.peer_mut(owner) {
            shooter.accuracy.0 += i32::from(counted);
            if damaged.attacker_hits != 0 {
                shooter.state.persistent[PERS_HITS] =
                    (shooter.state.persistent[PERS_HITS] as i32 + damaged.attacker_hits) as u32;
                shooter.state.persistent[PERS_ATTACKEE_ARMOR] = damaged.attackee_armor.unwrap_or(0);
            }
        } else {
            self.npcs.roster.record_hit(
                owner as u16,
                damaged.attacker_hits,
                damaged.attackee_armor,
            );
        }
    }

    /// `G_Damage` on NPC `target` (`LogAccuracyHit`: a living NPC is a client, never on a
    /// player's team), and the player shooter's counters. A blast's blow, dealt while the
    /// map is set aside for its lines (`G_RadiusDamage`), waits for the blast to be over
    /// ([`Self::flush_npc_blows`]): the NPCs come after the players in its order anyway.
    fn strike_npc(
        &mut self,
        owner: usize,
        target: u16,
        request: DamageRequest,
        surface: Option<HitLocation>,
        count: bool,
    ) -> (Damaged, bool) {
        let counts = owner != usize::from(target)
            && self
                .npcs
                .roster
                .actors
                .iter()
                .any(|npc| npc.number == target && npc.player.stats[0] as i32 > 0);
        if self.map.is_none() {
            self.npcs
                .deferred_blows
                .push((owner, target, request, count && counts));
            return (Damaged::default(), counts);
        }
        (
            self.hurt_npc(owner, target, request, surface, count && counts),
            counts,
        )
    }

    /// The blows a blast dealt NPCs while the map was set aside, dealt now in their order.
    pub(crate) fn flush_npc_blows(&mut self) {
        let blows = std::mem::take(&mut self.npcs.deferred_blows);
        for (owner, target, request, counted) in blows {
            let _ = self.hurt_npc(owner, target, request, None, counted);
        }
        self.flush_turret_blows();
    }

    /// `G_Damage` on NPC `target` through the roster, placed by the surface a blade struck
    /// when there is one; the player shooter's counters.
    fn hurt_npc(
        &mut self,
        owner: usize,
        target: u16,
        request: DamageRequest,
        surface: Option<HitLocation>,
        counted: bool,
    ) -> Damaged {
        // The Jedi Master's rule for a player's blow (an NPC is never the master).
        let spared_by_master = self.jedi_master_spares(
            request.attacker.map(|attacker| attacker.client),
            usize::from(target),
        );
        let level_time = request.level_time;
        let blow = NpcBlow {
            request,
            spared_by_master,
            surface,
        };
        let outcome = self
            .with_roster(|roster, _, host| roster.damage(target, blow, level_time, host))
            .flatten();
        let Some((hurt, fired)) = outcome else {
            return Damaged::default();
        };
        self.fire_targets_from_npcs(fired, level_time);
        let damaged = Damaged {
            take: hurt.take,
            attacker_hits: hurt.attacker_hits,
            attackee_armor: hurt.attackee_armor,
            landed: hurt.landed,
            ..Damaged::default()
        };
        if self.peer(owner).is_some() {
            self.count_hit(owner, counted, &damaged);
        }
        damaged
    }

    /// The names an NPC's pain or death fired, used as any name is.
    fn fire_targets_from_npcs(&mut self, fired: sjk_game_jka::npc_roster::Fired, level_time: i32) {
        for name in fired {
            self.fire_targets(&String::from_utf8_lossy(&name), usize::MAX, level_time);
        }
    }

    /// `player_die`'s NPC half for player `client` (`g_combat.c:2515-2600`, `2805-2808`):
    /// an NPC killer's victory, point and awards, and the NPCs of the player's team near it
    /// taking the killer on (`G_DeathAlert`).
    pub(crate) fn npc_player_death(
        &mut self,
        client: usize,
        attacker: Option<u16>,
        means: u32,
        was_master: bool,
    ) {
        if self.npcs.roster.actors.is_empty() {
            return;
        }
        let Some(victim) = self.player_sight(client) else {
            return;
        };
        let level_time = self.last_frame_time;
        let fired = self
            .with_roster(|roster, _, host| {
                roster.player_killed(victim, attacker, means, was_master, level_time, host)
            })
            .unwrap_or_default();
        self.fire_targets_from_npcs(fired, level_time);
    }

    /// What the roster did beyond itself ([`NpcOutcome`]), applied in its order.
    pub(super) fn apply_npc_outcomes(&mut self, outcomes: Vec<NpcOutcome>) {
        let level_time = self.last_frame_time;
        for outcome in outcomes {
            match outcome {
                NpcOutcome::Log(line) => self.log(&line),
                // `AddScore` is held off during the warmup.
                NpcOutcome::PlayerScore(..) | NpcOutcome::NpcScored(..) if !self.scoring() => {}
                NpcOutcome::PlayerScore(player, points) => {
                    let Some(peer) = self.peer_mut(usize::from(player)) else {
                        continue;
                    };
                    peer.state.persistent[0] = (peer.state.persistent[0] as i32 + points) as u32;
                    let team = peer.session.team;
                    self.add_team_score(team, points);
                    self.calculate_ranks();
                }
                NpcOutcome::NpcScored(team, points) => {
                    // `teamScores[PERS_TEAM]`: the NPC's player team read as a team number.
                    self.add_team_score(team, points);
                    self.calculate_ranks();
                }
                NpcOutcome::CreditKill(player, means) => {
                    let Some(peer) = self.peer_mut(usize::from(player)) else {
                        continue;
                    };
                    if means == MOD_STUN_BATON {
                        peer.state.persistent[PERS_GAUNTLET] =
                            peer.state.persistent[PERS_GAUNTLET].wrapping_add(1);
                    }
                    if level_time - peer.mortality.last_kill_time < CARNAGE_REWARD_TIME {
                        peer.state.persistent[PERS_EXCELLENT] =
                            peer.state.persistent[PERS_EXCELLENT].wrapping_add(1);
                    }
                    peer.mortality.last_kill_time = level_time;
                }
                NpcOutcome::PlayerEnemy(player, enemy) => {
                    if let Some(peer) = self.peer_mut(usize::from(player)) {
                        peer.npc_enemy = enemy;
                    }
                }
                NpcOutcome::KillPlayer(player) => {
                    self.npc_kill_player(usize::from(player), level_time)
                }
                NpcOutcome::SaberBlow(target, request) => self.npc_saber_blow(target, request),
                NpcOutcome::Unported(number, name) => self.npcs.tell_unported_call(number, name),
                NpcOutcome::KnockSaber(player, velocity) => {
                    let _ = self.knock_out(usize::from(player), velocity, level_time);
                }
                NpcOutcome::TouchSaber(player) => self.bounce_saber(usize::from(player)),
                NpcOutcome::Explode(at, damage, radius, attacker) => self.blast(
                    at,
                    damage,
                    radius,
                    Some(attacker),
                    None,
                    sjk_game_jka::means_of_death::MOD_UNKNOWN,
                    level_time,
                    None,
                    true,
                ),
                NpcOutcome::SmashSaber(player, striker, defending, damage) => {
                    let _ = self.with_saber(usize::from(player), |saber, world| {
                        sjk_game_jka::saber_drop::smashed(
                            saber, world, striker, defending, damage, level_time,
                        )
                    });
                }
                NpcOutcome::HurtBrush(number, damage, attacker) => {
                    let _ = self.hurt_brush(
                        number,
                        damage,
                        sjk_game_jka::means_of_death::MOD_MELEE,
                        attacker,
                        level_time,
                    );
                }
                NpcOutcome::TouchTriggers(npc) => self.npc_touch_triggers(npc, level_time),
                NpcOutcome::LostLock(player, attacker, origin, storage, chance) => self
                    .lost_lock_to_npc(
                        usize::from(player),
                        attacker,
                        origin,
                        &storage,
                        chance,
                        level_time,
                    ),
                NpcOutcome::ForceBlow(target, request) => {
                    // `G_Damage` by the NPC, as a player's Force blows are dealt.
                    let owner = request
                        .attacker
                        .map_or(usize::MAX, |attacker| usize::from(attacker.client));
                    let _ = self.strike(owner, usize::from(target), request, false);
                }
            }
        }
    }

    /// `WP_SaberApplyDamage`'s `G_Damage` by an NPC's blade on a player — placed by the
    /// surface its model was struck on — or on a breakable brush; the NPC's hit counter.
    fn npc_saber_blow(&mut self, target: u16, request: DamageRequest) {
        let Some(attacker) = request.attacker else {
            return;
        };
        let owner = usize::from(attacker.client);
        if self.peer(usize::from(target)).is_some() {
            let point = request.point.unwrap_or_default();
            let location = self.surface_location(
                usize::from(target),
                request.flags,
                point,
                request.level_time,
            );
            let _ = self.strike_at(owner, usize::from(target), request, location, false);
        } else {
            let _ = self.hurt_brush(
                target,
                request.damage,
                request.means,
                attacker.client,
                request.level_time,
            );
        }
    }

    /// `npc kill team nonally` on a player (`NPC_spawn.c:4203-4213`): its health to zero and
    /// `player_die` by itself, `MOD_UNKNOWN`, with its full health for the damage.
    fn npc_kill_player(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if !peer.playing() {
            return;
        }
        peer.health = 0;
        let (origin, sounds, max_health) = (
            peer.state.origin(),
            peer.saber_off_sounds(),
            peer.state.max_health(),
        );
        let request = DeathRequest {
            means: MOD_UNKNOWN,
            damage: max_health,
            ..DeathRequest::suicide(level_time, client as u16, origin, sounds, 0)
        };
        self.die(client, request);
    }

    /// The begun NPCs a blast may reach, with their linked boxes (`G_RadiusDamage`'s
    /// `EntitiesInBox`, every one of which takes damage).
    pub(in crate::bridge) fn add_npc_splash_targets(npcs: &Npcs, targets: &mut Vec<SplashTarget>) {
        targets.extend(npcs.roster.begun().map(|npc| SplashTarget {
            number: npc.number,
            bounds: npc.link,
            origin: npc.current_origin,
            takes_damage: npc.takes_damage,
        }));
    }

    /// Where entity `number` stands: a player's origin, or an NPC's.
    pub(crate) fn origin_of(&self, number: u16) -> Option<[f32; 3]> {
        self.peer(usize::from(number))
            .map(|peer| peer.state.origin())
            .or_else(|| {
                self.npcs
                    .roster
                    .actors
                    .iter()
                    .find(|npc| npc.number == number)
                    .map(|npc| npc.current_origin)
            })
    }

    /// A melee blow's target when it is an NPC (`WP_FireMelee`, `WP_FireStunBaton`: a client,
    /// never in a duel).
    pub(crate) fn npc_struck(&self, number: u16) -> Option<sjk_game_jka::melee::Struck> {
        let npc = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number && npc.begun())?;
        Some(sjk_game_jka::melee::Struck {
            origin: npc.current_origin,
            player: true,
            duelling: None,
        })
    }

    /// The stun baton's shock on an NPC: its `electrifyTime`.
    pub(crate) fn electrify_npc(&mut self, number: u16, until: i32) {
        if let Some(npc) = self
            .npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == number)
        {
            npc.player
                .set_raw_field(sjk_game_jka::melee::ELECTRIFY_TIME, until as u32);
        }
    }
}
