//! Portable shields in the native server: placement, callbacks, collision and damage.
use crate::bridge::*;
use sjk_game_jka::holdable_shield::{self as rules, Placer, Shield, Sounds};
use sjk_game_jka::pmove::MovementTrace;
use sjk_game_jka::saber_block::MissilePaths;

/// The level's deployed shields. Allocation happens when an item is placed, not in
/// the ordinary frame loop. Native entity identities belong to the existing pool.
#[derive(Debug, Default)]
pub(in crate::bridge) struct Shields {
    pub(in crate::bridge) placed: Vec<(EntityId, Shield)>,
    sounds: Option<Sounds>,
}

impl Shields {
    /// Their current linked collision boxes, including nonsolid placement triggers.
    pub(in crate::bridge) fn obstacles(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.placed
            .iter()
            .filter(|(_, shield)| !shield.freed)
            .map(|(id, shield)| BoxObstacle {
                entity: id.legacy_number(),
                origin: shield.origin,
                bounds: (shield.mins, shield.maxs),
                contents: shield.contents,
                model: None,
            })
    }

    /// Damageable fields as `G_RadiusDamage` sees them; linked bounds include a unit.
    pub(in crate::bridge) fn splash_targets(&self) -> impl Iterator<Item = SplashTarget> + '_ {
        self.placed
            .iter()
            .filter(|(_, shield)| !shield.freed)
            .map(|(id, shield)| SplashTarget {
                number: id.legacy_number(),
                origin: shield.origin,
                takes_damage: shield.takes_damage,
                bounds: (
                    std::array::from_fn(|axis| shield.origin[axis] + shield.mins[axis] - 1.0),
                    std::array::from_fn(|axis| shield.origin[axis] + shield.maxs[axis] + 1.0),
                ),
            })
    }
}

/// The normal server trace, with the shield or placer passed through.
struct World<'a> {
    map: Option<&'a LoadedMap>,
    obstacles: &'a [BoxObstacle],
}
impl rules::World for World<'_> {
    fn trace(
        &self,
        skip: u16,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        match self.map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: self.obstacles,
            }
            .trace_from(skip, start, mins, maxs, end, mask),
            None => WithPlayers {
                world: Void,
                players: self.obstacles,
            }
            .trace_from(skip, start, mins, maxs, end, mask),
        }
    }
}

impl NativeGame {
    /// `ItemUse_Shield`. A failed second placement trace still consumes the item, as
    /// the reference does after `PM_ItemUsable`/`G_ItemUsable` has accepted it.
    pub(in crate::bridge) fn place_shield(&mut self, client: usize, now: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let placer = Placer {
            number: client as u16,
            team: peer.session.team,
            origin: peer.state.origin(),
            angles: peer.state.view_angles(),
        };
        let sounds = match self.stock.shields.sounds {
            Some(sounds) => sounds,
            None => {
                let mut sound = |name: &[u8]| {
                    self.sounds.index(name, &mut |index, value| {
                        self.told.push(Told::ConfigString {
                            index,
                            previous: Vec::new(),
                            value: value.to_vec(),
                        })
                    })
                };
                let sounds = Sounds {
                    looping: sound(b"sound/movers/doors/forcefield_lp.wav"),
                    attach: sound(b"sound/weapons/detpack/stick.wav"),
                    activate: sound(b"sound/movers/doors/forcefield_on.wav"),
                    deactivate: sound(b"sound/movers/doors/forcefield_off.wav"),
                    damage: sound(b"sound/effects/bumpfield.wav"),
                };
                self.stock.shields.sounds = Some(sounds);
                sounds
            }
        };
        self.gather_obstacles(client);
        let world = World {
            map: self.map.as_ref(),
            obstacles: &self.obstacles,
        };
        let Some(mut shield) = Shield::place(placer, self.gametype, now, sounds, &world) else {
            return;
        };
        let Some(id) = self.pool.spawn_entity(shield.state.clone(), now) else {
            eprintln!("shield placement refused: entity budget exhausted");
            return;
        };
        shield
            .state
            .set_number(id.legacy_number())
            .expect("pool assigned a valid entity number");
        self.stock.shields.placed.push((id, shield));
        self.publish_shield(self.stock.shields.placed.len() - 1);
    }

    /// Publish the game's fields and `SV_LinkEntity`'s solid box. The shield's linked
    /// center differs from its unchanged trajectory base; compensate only for PVS bounds.
    fn publish_shield(&mut self, index: usize) {
        let (id, shield) = &self.stock.shields.placed[index];
        self.pool.set_state(*id, &shield.state);
        let byte = |value: f32| (value as i32).clamp(1, 255) as u32;
        let solid = if shield.contents & 0x101 != 0 {
            byte(shield.maxs[2] + 32.0) << 16 | byte(-shield.mins[2]) << 8 | byte(shield.maxs[0])
        } else {
            0
        };
        if let Some(state) = self.pool.state_mut(*id) {
            state.set_raw_field(26, solid);
        }
        let offset: [f32; 3] = std::array::from_fn(|axis| {
            shield.origin[axis]
                - f32::from_bits(shield.state.raw_field([2, 1, 4][axis]).unwrap_or(0))
        });
        self.pool.set_bounds(
            *id,
            (
                std::array::from_fn(|axis| shield.mins[axis] + offset[axis]),
                std::array::from_fn(|axis| shield.maxs[axis] + offset[axis]),
            ),
        );
        self.pool.event_raised(*id, shield.event_time);
    }

    /// Every due shield callback. Ordinary frames only expire events and inspect clocks.
    pub(in crate::bridge) fn run_shields(&mut self, now: i32) {
        for index in 0..self.stock.shields.placed.len() {
            let shield = &mut self.stock.shields.placed[index].1;
            let mut changed = shield.expire_event(now);
            if shield.next_think > 0 && shield.next_think <= now && !shield.freed {
                // Gather only when a callback actually needs the world, reusing the
                // server's collision scratch and updating it between callbacks.
                if matches!(shield.phase, rules::Phase::Create | rules::Phase::GoSolid) {
                    self.gather_obstacles(usize::MAX);
                }
                let world = World {
                    map: self.map.as_ref(),
                    obstacles: &self.obstacles,
                };
                let (id, shield) = &mut self.stock.shields.placed[index];
                changed |= shield.run(id.legacy_number(), self.gametype, now, &world);
            }
            let (id, shield) = &self.stock.shields.placed[index];
            if shield.freed {
                self.pool.free(*id, now);
            } else if changed {
                self.publish_shield(index);
            }
        }
        self.stock
            .shields
            .placed
            .retain(|(_, shield)| !shield.freed);
    }

    /// `ClientImpacts` invokes `ShieldTouch` for shields hit by the player's move.
    pub(in crate::bridge) fn touch_shields(&mut self, client: usize, touched: &[u16], now: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let team = peer.session.team;
        for index in 0..self.stock.shields.placed.len() {
            let (id, shield) = &self.stock.shields.placed[index];
            if !touched.contains(&id.legacy_number()) {
                continue;
            }
            let owner_team = self
                .peer(usize::from(shield.owner))
                .map(|peer| peer.session.team);
            if self.stock.shields.placed[index].1.touch(
                client as u16,
                Some(team),
                owner_team,
                self.gametype,
                now,
            ) {
                self.publish_shield(index);
            }
        }
    }

    /// `G_Damage` on a shield, after the existing object-damage policy (handicap and
    /// siege's round gate). Shields have no `teamnodmg`: allies can damage them too.
    pub(in crate::bridge) fn strike_shield(&mut self, number: u16, request: DamageRequest) -> bool {
        let Some(index) = self
            .stock
            .shields
            .placed
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
        else {
            return false;
        };
        let attacker = request
            .attacker
            .and_then(|attacker| self.blow_attacker(attacker.client));
        let blow = sjk_game_jka::map_turret_world::ObjectBlow {
            attacker,
            damage: request.damage,
            flags: request.flags,
            means: request.means,
            siege: self.gametype == GAMETYPE_SIEGE,
            siege_round_begun: self.siege.as_ref().is_some_and(|siege| siege.round.begun),
            friendly_fire_objectives: self.integer_cvar(b"g_ff_objectives", 0) != 0,
        };
        let shield = &mut self.stock.shields.placed[index].1;
        if let Some(take) =
            sjk_game_jka::map_turret_world::object_take(number, shield.takes_damage, 0, &blow)
        {
            shield.damage(take, request.level_time);
            self.publish_shield(index);
        }
        true
    }
}
