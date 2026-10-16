//! `G_Damage` on a turret, and a turret's blows on the others: a missile that struck one,
//! a blast that reached one (dealt once the blast's lines are traced, as the NPCs' are),
//! the attacker a turret's own shots and splash are credited to, and the rule that keeps a
//! turret from hurting its allied team in a team game.

use super::*;

impl NativeGame {
    /// `G_MissileImpact`'s damage on a thing that is no client: a breakable brush, else a
    /// turret, else an emplaced gun.
    pub(in crate::bridge) fn hurt_thing(
        &mut self,
        number: u16,
        missile: &Missile,
        level_time: i32,
    ) {
        if self.hurt_brush(
            number,
            missile.damage,
            missile.method_of_death,
            missile.owner,
            level_time,
        ) || missile.damage == 0
        {
            return;
        }
        let attacker = self.attacker_for(missile.owner);
        let request = DamageRequest {
            level_time,
            attacker,
            direction: Some(missile.impact_velocity),
            point: Some(missile.impact_point),
            damage: missile.damage,
            flags: missile.damage_flags,
            means: missile.method_of_death,
        };
        if self.strike_siege_item(number, request) || self.strike_shield(number, request) {
            return;
        }
        if !self.map_turrets.owns(number) {
            self.hurt_gun(number, missile.damage, Some(missile.owner), level_time);
            return;
        }
        let attacker = self.attacker_for(missile.owner);
        let request = DamageRequest {
            level_time,
            attacker,
            direction: Some(missile.impact_velocity),
            point: Some(missile.impact_point),
            damage: missile.damage,
            flags: missile.damage_flags,
            means: missile.method_of_death,
        };
        self.damage_turret(number, request);
    }

    /// A blow on entity `target` when it is a turret — kept for later while the map is set
    /// aside for a blast's lines. Whether it was a turret.
    pub(in crate::bridge) fn strike_turret(&mut self, target: u16, request: DamageRequest) -> bool {
        if !self.map_turrets.owns(target) {
            return false;
        }
        if self.map.is_none() {
            self.map_turrets.deferred.push((target, request));
        } else {
            self.damage_turret(target, request);
        }
        true
    }

    /// The blows a blast dealt the turrets while the map was set aside, dealt now.
    pub(in crate::bridge) fn flush_turret_blows(&mut self) {
        for (target, request) in std::mem::take(&mut self.map_turrets.deferred) {
            self.damage_turret(target, request);
        }
    }

    /// The attacker `G_Damage` is given for turret `number` (no client: no handicap, no
    /// team of a player's).
    pub(in crate::bridge) fn turret_attacker(&self, number: u16) -> Option<Attacker> {
        self.map_turrets.owns(number).then_some(Attacker {
            npc: false,
            client: number,
            max_health: 100,
            team: 0,
            saber_knockback: [0.0; 4],
        })
    }

    /// "things allied with my team shouldn't hurt me" (`g_combat.c:4834-4842`): in a team
    /// game, a turret's blow on a player of its allied team does nothing.
    pub(in crate::bridge) fn turret_spares(&self, target: usize, owner: usize) -> bool {
        if self.gametype < GT_TEAM || self.integer_cvar(b"g_friendlyFire", 0) != 0 {
            return false;
        }
        let Ok(owner) = u16::try_from(owner) else {
            return false;
        };
        let allied = self
            .map_turrets
            .g2
            .iter()
            .find(|(id, _)| id.legacy_number() == owner)
            .map(|(_, turret)| turret.allied_team);
        let allied = allied.or_else(|| {
            self.map_turrets
                .misc
                .iter()
                .find(|(base, top, _)| {
                    base.legacy_number() == owner || top.legacy_number() == owner
                })
                .map(|(_, _, turret)| turret.base.allied_team)
        });
        allied.is_some_and(|team| {
            team != 0
                && self
                    .peer(target)
                    .is_some_and(|peer| peer.session.team == team)
        })
    }

    /// What `G_Damage` reads of the attacker numbered `number` when it strikes a turret.
    pub(in crate::bridge) fn blow_attacker(&self, number: u16) -> Option<BlowAttacker> {
        if let Some(peer) = self.peer(usize::from(number)) {
            return Some(BlowAttacker {
                number,
                client: true,
                player: true,
                max_health: peer.state.max_health(),
                team: peer.session.team,
                weapon: u32::from(peer.state.weapon()),
                activator_team: None,
            });
        }
        if let Some(npc) = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)
        {
            return Some(BlowAttacker {
                number,
                client: true,
                player: false,
                max_health: npc.player.stats[8] as i32,
                team: npc.session_team,
                weapon: u32::from(npc.player.weapon()),
                activator_team: None,
            });
        }
        let team = self
            .map_turrets
            .g2
            .iter()
            .find(|(id, _)| id.legacy_number() == number)
            .map(|(_, turret)| turret.team_no_damage);
        let team = team.or_else(|| {
            self.map_turrets
                .misc
                .iter()
                .find(|(base, top, _)| {
                    base.legacy_number() == number || top.legacy_number() == number
                })
                .map(|(_, _, turret)| turret.base.team_no_damage)
        });
        Some(BlowAttacker {
            number,
            client: false,
            player: false,
            max_health: 0,
            team: team.unwrap_or(0),
            weapon: 0,
            activator_team: None,
        })
    }

    /// `G_Damage` on turret `target`: its health, its pain or its death, its wire state; then
    /// what its death asked for (its splash, its targets, its model gone).
    fn damage_turret(&mut self, target: u16, request: DamageRequest) {
        let level_time = request.level_time;
        let attacker = request
            .attacker
            .map(|attacker| attacker.client)
            .filter(|number| *number != ENTITYNUM_WORLD)
            .and_then(|number| self.blow_attacker(number));
        let siege = self.gametype == GT_SIEGE;
        let blow = ObjectBlow {
            attacker,
            damage: request.damage,
            flags: request.flags,
            means: request.means,
            siege,
            siege_round_begun: self.siege.as_ref().is_some_and(|siege| siege.round.begun),
            friendly_fire_objectives: self.integer_cvar(b"g_ff_objectives", 0) != 0,
        };
        self.gather_turret_sights();
        let targets = std::mem::take(&mut self.map_turrets.sighted);
        let known = attacker
            .and_then(|attacker| targets.iter().find(|entry| entry.number == attacker.number))
            .copied();
        let map = self.map.take();
        if let Some(index) = self
            .map_turrets
            .g2
            .iter()
            .position(|(id, _)| id.legacy_number() == target)
        {
            let (id, mut turret) = self.map_turrets.g2.remove(index);
            self.with_turret_host(map.as_ref(), usize::MAX, |host| {
                sjk_game_jka::map_turret_g2::damage(
                    &mut turret,
                    target,
                    &blow,
                    known.as_ref(),
                    level_time,
                    host,
                )
            });
            self.map_turrets.g2.insert(index, (id, turret));
        } else if let Some(index) = self.map_turrets.misc.iter().position(|(base, top, _)| {
            base.legacy_number() == target || top.legacy_number() == target
        }) {
            let (base, top, mut turret) = self.map_turrets.misc.remove(index);
            let part = if base.legacy_number() == target {
                Part::Base
            } else {
                Part::Top
            };
            self.with_turret_host(map.as_ref(), usize::MAX, |host| {
                sjk_game_jka::map_turret::damage(
                    &mut turret,
                    part,
                    target,
                    &blow,
                    known.as_ref(),
                    level_time,
                    host,
                )
            });
            self.map_turrets.misc.insert(index, (base, top, turret));
        }
        self.map = map;
        self.map_turrets.sighted = targets;
        self.publish_turret(target);
        self.carry_out_turret_work(level_time);
    }
}
