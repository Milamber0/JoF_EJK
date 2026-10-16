//! A player and the vehicle it drives, as `ClientThink_real` couples them
//! (`codemp/game/g_active.c:1889-1910`, `3497-3510`): the pilot's command handed to the
//! vehicle before its own move ([`hand_command`]), the vehicle's `ClientThink` run on that
//! command after it ([`NpcRoster::drive`]); and the use key's vehicle branches of `TryUse`
//! (`g_utils.c:1619-1631`, `1699-1723`): boarding, or getting off.
//!
//! The vehicle is an NPC of the roster; the player is borrowed as a [`Rider`] for as long
//! as the roster works on it. What the vehicle's think does to the players' side of the
//! game beyond the rider — a rider killed, the explosion's blast — the roster leaves as
//! [`VehicleOutcome`]s for the caller ([`NpcRoster::take_vehicle_outcomes`]).

use crate::damage::{Attacker, DamageRequest};
use crate::event_entity::EventEntity;
use crate::npc_damage::NpcBlow;
use crate::npc_roster::{Fired, NpcRoster};
use crate::npc_spawn::{NpcHost, NpcThink};
use crate::npc_world::NpcWorld;
use crate::vehicle_board::{EjectTrace, Parent};
use crate::vehicle_rider::Rider;
use crate::vehicle_update::VehicleRequest;
use sjk_protocol::UserCommand;

/// `BUTTON_TALK`.
const BUTTON_TALK: u16 = 2;
/// `CLASS_VEHICLE`.
const CLASS_VEHICLE: i32 = 53;
/// `GT_TEAM`.
const GT_TEAM: i32 = 6;
/// `EV_PLAY_EFFECT_ID`.
const EV_PLAY_EFFECT_ID: u32 = 69;
use crate::damage::DAMAGE_NO_PROTECTION;
use crate::means_of_death::MOD_SUICIDE;
/// `s.loopSound`, `ps.loopSound`; `s.origin`, `s.angles`.
const ES_LOOP_SOUND: usize = 55;
const PS_LOOP_SOUND: usize = 75;
/// `FRAMETIME`.
const FRAMETIME: i32 = 100;

/// What the use key does with an entity it reached ([`NpcRoster::vehicle_use`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleUse {
    /// It is no vehicle: the use key goes on to whatever else it is.
    NotOne,
    /// The rider's own vehicle: off it.
    GetOff,
    /// Someone else's, or nobody's: on it.
    GetOn,
    /// Another team's: nothing, but the press is spent.
    Refused,
}

/// What a vehicle's think left for the players' side of the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehicleOutcome {
    /// `G_Damage` on a player the vehicle killed with it: `damage` with `flags` by `means`
    /// (`MOD_SUICIDE` from its death, `MOD_BLASTER` from `player_die`'s kill of everyone
    /// aboard), the attacker (the vehicle, or who it died of) where `by` names one.
    KillRider {
        rider: u16,
        damage: i32,
        flags: u32,
        by: Option<u16>,
        means: u32,
    },
    /// `G_RadiusDamage(at, attacker, damage, radius, spared, NULL, MOD_SUICIDE)`: a
    /// vehicle's explosion (no attacker, nothing spared), or the blast of a fighter's
    /// surface coming off (the fighter the attacker, and spared): the caller's part — its
    /// players and its own things. The roster's NPCs were struck as the blast went off
    /// ([`crate::vehicle_blast`]).
    /// `rider_at` is a rider where the blast finds it, where that is not where it stands now
    /// (a pilot the vehicle's move has since carried on, a droid unit it has since set on
    /// its bolt).
    Blast {
        vehicle: u16,
        at: [f32; 3],
        damage: i32,
        radius: f32,
        attacker: Option<u16>,
        spared: Option<u16>,
        rider_at: Option<(u16, [f32; 3])>,
    },
    /// `PM_VehicleImpact`'s knock on what the vehicle struck (`bg_slidemove.c:506-552`):
    /// a player knocked down and thrown along (where it may be the vehicle's enemy), then
    /// `G_Damage(target, attacker, attacker, NULL, at, magnitude * 40 for a humanoid, 0,
    /// MOD_MELEE)`, at least 1, by the pilot (or the vehicle without one); from a fighter
    /// the humanoid's scale is 2000 (`humanoid_scale`).
    Ram {
        vehicle: u16,
        attacker: u16,
        target: u16,
        magnitude: f32,
        velocity: [f32; 3],
        at: [f32; 3],
        command_time: i32,
        humanoid_scale: f32,
    },
    /// A walker's weight on `target` (`g_active.c:3035-3046`): `G_Damage(target, vehicle,
    /// vehicle, {0, 0, -1}, target's origin, 100, 0, MOD_CRUSH)` where the target has
    /// health and takes damage: a player or an NPC (a breakable under it is not crushed).
    Crush { vehicle: u16, target: u16 },
    /// `G_AddEvent(pilot, event, parameter)` on a player pilot: its vehicle's weapon ran
    /// dry (`EV_NOAMMO`, [`crate::vehicle_weapons`]).
    PilotEvent {
        pilot: u16,
        event: u32,
        parameter: u32,
    },
    /// A ship through a hyperspace trigger (`g_trigger.c:1700-1716`): the vehicle already
    /// put out at the far point; `TeleportPlayer(pilot, origin, angles)` of its player
    /// pilot, then the jump's end heard on the vehicle where it now stands
    /// ([`crate::vehicle_ship_touch::jump_sound`], `sound` its index).
    Jumped {
        vehicle: u16,
        pilot: Option<u16>,
        origin: [f32; 3],
        angles: [f32; 3],
        sound: u16,
    },
}

/// `ClientThink_real`'s opening for a player on a vehicle (`g_active.c:1892-1910`): the
/// vehicle it pilots is set to the pilot's command time and takes its command — only the
/// talk button while it chats.
pub fn hand_command(
    npc: &mut crate::npc_spawn::NpcActor,
    pilot: u16,
    pilot_command_time: i32,
    command: &UserCommand,
) {
    let Some(vehicle) = npc.vehicle.as_deref_mut() else {
        return;
    };
    if vehicle.pilot != Some(pilot) {
        return;
    }
    npc.player.set_command_time(pilot_command_time);
    vehicle.ucmd = *command;
    if vehicle.ucmd.buttons & BUTTON_TALK != 0 {
        vehicle.ucmd.buttons = BUTTON_TALK;
        vehicle.ucmd.forward_move = 0;
        vehicle.ucmd.right_move = 0;
        vehicle.ucmd.up_move = 0;
    }
}

impl NpcRoster {
    /// `ClientThink(m_iVehicleNum, &m_ucmd)` at the end of the pilot's think
    /// (`g_active.c:3497-3510`): the vehicle's think on its own command, with `rider`
    /// aboard. `false` where no NPC has `number` (the caller clears its `m_iVehicleNum`).
    pub fn drive(
        &mut self,
        number: u16,
        rider: Rider<'_>,
        passengers: Vec<Rider<'_>>,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> (bool, Fired) {
        let mut fired = Fired::new();
        let Some(at) = self
            .actors
            .iter()
            .position(|npc| npc.number == number && npc.vehicle.is_some())
        else {
            return (false, fired);
        };
        let command = self.actors[at]
            .vehicle
            .as_deref()
            .map(|vehicle| vehicle.ucmd)
            .unwrap_or_default();
        {
            let mut world = self.world(level_time, host, &mut fired);
            world.rider = Some(rider);
            world.passengers = passengers;
            world.client_think(at, command);
        }
        let npc = &self.actors[at];
        host.publish(npc.number, &npc.state, npc.bounds(), npc.contents);
        (true, fired)
    }

    /// The `ps.generic1` rider `rider` of vehicle `number` has now
    /// ([`crate::vehicle_board::seat_of`]): kept by the caller after anything that moved the
    /// seats about; `None` where it is not aboard.
    pub fn seat_of(&self, number: u16, rider: u16) -> Option<u32> {
        let vehicle = self
            .actors
            .iter()
            .find(|npc| npc.number == number)?
            .vehicle
            .as_deref()?;
        crate::vehicle_board::seat_of(vehicle, rider)
    }

    /// `Board` on vehicle `number` by `rider`, as the use key or a landing asks it
    /// (`g_utils.c:1717`, `bg_pmove.c:4226`): whether it got on.
    pub fn board(
        &mut self,
        number: u16,
        rider: &mut Rider<'_>,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> bool {
        let Some(npc) = self.actors.iter_mut().find(|npc| npc.number == number) else {
            return false;
        };
        let Some(mut parent) = Parent::of(npc) else {
            return false;
        };
        crate::vehicle_board::board(&mut parent, rider, level_time, host)
    }

    /// `Eject` of `rider` from vehicle `number` between moves (`TryUse`, a death):
    /// whether it got off. `force` throws it off where no side is clear, if it is dead.
    pub fn eject(
        &mut self,
        number: u16,
        rider: &mut Rider<'_>,
        force: bool,
        level_time: i32,
        trace: &mut EjectTrace<'_>,
    ) -> bool {
        let Some(npc) = self.actors.iter_mut().find(|npc| npc.number == number) else {
            return false;
        };
        let Some(mut parent) = Parent::of(npc) else {
            return false;
        };
        crate::vehicle_board::eject(&mut parent, rider, force, level_time, trace)
    }

    /// `TryUse` for a player on a vehicle (`g_utils.c:1619-1631`): the use key gets it off
    /// unless it is still boarding. `true` where `TryUse` ends here (the vehicle is there).
    pub fn use_while_riding(
        &mut self,
        rider: &mut Rider<'_>,
        level_time: i32,
        trace: &mut EjectTrace<'_>,
    ) -> bool {
        let number = rider.vehicle();
        let Some(npc) = self.actors.iter_mut().find(|npc| npc.number == number) else {
            return false;
        };
        let Some(mut parent) = Parent::of(npc) else {
            return false;
        };
        if parent.vehicle.boarding == 0 {
            crate::vehicle_board::eject(&mut parent, rider, false, level_time, trace);
        }
        true
    }

    /// `TryUse` on entity `target` (`g_utils.c:1699-1723`): what the use key does with it
    /// — a vehicle is left by the one who rides it and boarded by anyone else (in a team
    /// game only by its allied team, if it has one). Anything but [`VehicleUse::NotOne`]
    /// spends the key's press (`pers.cmd.buttons &= ~BUTTON_USE`); the caller then runs
    /// [`Self::eject`] or [`Self::board`].
    pub fn vehicle_use(
        &self,
        target: u16,
        rider: &Rider<'_>,
        zoomed: bool,
        gametype: i32,
        team: i32,
    ) -> VehicleUse {
        let Some(npc) = self.actors.iter().find(|npc| npc.number == target) else {
            return VehicleUse::NotOne;
        };
        if npc.vehicle.is_none() || npc.definition.client_class != CLASS_VEHICLE || zoomed {
            return VehicleUse::NotOne;
        }
        if rider.body.owner == target {
            VehicleUse::GetOff
        } else if gametype < GT_TEAM || npc.allied_team == 0 || npc.allied_team == team {
            VehicleUse::GetOn
        } else {
            VehicleUse::Refused
        }
    }

    /// Whom `G_RadiusDamage` credits a blast by `attacker` to (`g_combat.c:5695-5700`):
    /// "say my pilot did it" — a vehicle with a pilot aboard names its pilot.
    pub fn splash_credit(&self, attacker: u16) -> u16 {
        self.actors
            .iter()
            .find(|npc| npc.number == attacker)
            .and_then(|npc| npc.vehicle.as_deref())
            .and_then(|vehicle| vehicle.pilot)
            .unwrap_or(attacker)
    }

    /// `G_Damage`'s shelter (`g_combat.c:4475-4492`): a player aboard a living walker or
    /// fighter (`rider`'s `m_iVehicleNum`) takes no damage from a blow with `flags`,
    /// unless it ignores protection. The DEMP2's shock, which comes before, still lands.
    pub fn shelters(&self, rider: &sjk_protocol::PlayerState, flags: u32) -> bool {
        let vehicle = rider
            .raw_field(crate::vehicle_rider::field::PS_VEHICLE)
            .unwrap_or(0);
        vehicle != 0
            && flags & crate::damage::DAMAGE_NO_PROTECTION == 0
            && self
                .actors
                .iter()
                .find(|npc| u32::from(npc.number) == vehicle)
                .is_some_and(|npc| {
                    npc.health > 0
                        && npc.vehicle.as_deref().is_some_and(|vehicle| {
                            matches!(
                                vehicle.kind(),
                                crate::vehicle_fields::kind::WALKER
                                    | crate::vehicle_fields::kind::FIGHTER
                            )
                        })
                })
    }

    /// What the vehicles' thinks left for the players' side since the last call.
    pub fn take_vehicle_outcomes(&mut self) -> Vec<VehicleOutcome> {
        std::mem::take(&mut self.vehicle_outcomes)
    }

    /// A vehicle's blast on the roster's NPCs (`G_RadiusDamage`'s NPC half): `targets`
    /// the NPCs' numbers in entity order, `hurt` what the caller's radius damage decided
    /// for each.
    pub fn blast_npcs(
        &mut self,
        blows: &[(u16, DamageRequest)],
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        for (target, request) in blows {
            if let Some((_, more)) = self.damage(
                *target,
                NpcBlow {
                    request: *request,
                    spared_by_master: false,
                    surface: None,
                },
                level_time,
                host,
            ) {
                fired.extend(more);
            }
        }
        fired
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// What a vehicle's move asked of the world, done once it is over.
    pub(crate) fn vehicle_request(&mut self, me: usize, request: VehicleRequest) {
        let level_time = self.level_time;
        let number = self.actors[me].number;
        match request {
            VehicleRequest::DieUnpiloted => {
                let npc = &self.actors[me];
                let attacker = Attacker {
                    npc: true,
                    client: number,
                    max_health: npc.max_health,
                    team: npc.session_team,
                    saber_knockback: [0.0; 4],
                };
                let blow = DamageRequest {
                    level_time,
                    attacker: Some(attacker),
                    direction: None,
                    point: Some(npc.player.origin()),
                    damage: 99_999,
                    flags: DAMAGE_NO_PROTECTION,
                    means: MOD_SUICIDE,
                };
                self.damage(
                    me,
                    NpcBlow {
                        request: blow,
                        spared_by_master: false,
                        surface: None,
                    },
                );
            }
            VehicleRequest::FireLoop => {
                let sound = self.host.sound_index(b"sound/vehicles/common/fire_lp.wav");
                let npc = &mut self.actors[me];
                npc.player.set_raw_field(PS_LOOP_SOUND, u32::from(sound));
                npc.state.set_raw_field(ES_LOOP_SOUND, u32::from(sound));
            }
            VehicleRequest::Crush { under } => self.vehicle_outcomes.push(VehicleOutcome::Crush {
                vehicle: number,
                target: under,
            }),
            VehicleRequest::Crash { at, damage } => {
                let blow = DamageRequest {
                    level_time,
                    attacker: None,
                    direction: None,
                    point: Some(at),
                    damage,
                    flags: crate::damage::DAMAGE_NO_ARMOR,
                    means: crate::means_of_death::MOD_FALLING,
                };
                self.damage(
                    me,
                    NpcBlow {
                        request: blow,
                        spared_by_master: false,
                        surface: None,
                    },
                );
            }
            VehicleRequest::Ram {
                target,
                magnitude,
                velocity,
                at,
                command_time,
                humanoid_scale,
            } => {
                let attacker = self.actors[me]
                    .vehicle
                    .as_deref()
                    .and_then(|vehicle| vehicle.pilot)
                    .unwrap_or(number);
                self.vehicle_outcomes.push(VehicleOutcome::Ram {
                    vehicle: number,
                    attacker,
                    target,
                    magnitude,
                    velocity,
                    at,
                    command_time,
                    humanoid_scale,
                });
            }
            VehicleRequest::Fire {
                alternate,
                origin,
                view_angles,
            } => self.vehicle_fire(me, alternate, origin, view_angles),
            VehicleRequest::Turrets {
                origin,
                view_angles,
                orientation,
            } => self.vehicle_turrets(me, origin, view_angles, orientation),
            VehicleRequest::TurretRecharge { server_time } => {
                self.recharge_turrets(me, server_time)
            }
            VehicleRequest::EntitySound { sound, at } => {
                let mut event = crate::knockdown::entity_sound(at, number, 0);
                event.parameter = sound as u32;
                self.host.raise(event);
            }
            VehicleRequest::TurboStart { origin, yaw } => {
                let Some(vehicle) = self.actors[me].vehicle.as_deref() else {
                    return;
                };
                let (effect, tags) = (vehicle.info.turbo_start_fx, vehicle.exhaust_tags);
                for tag in tags.into_iter().take_while(|&tag| tag != -1) {
                    let bolt =
                        self.host
                            .vehicle_tag(number, tag, [0.0, yaw, 0.0], origin, level_time);
                    // The reference reads the bolt's origin twice: the effect's angles are it.
                    self.play_effect_id(effect, bolt.origin, bolt.origin);
                }
            }
            VehicleRequest::Wrecked {
                at,
                damage,
                flags,
                means,
                fallback_self,
            } => {
                // The killer: its last attacker while it is remembered, else itself or none.
                let other = self.actors[me].mind.fight.other_killer;
                let remembered = other.number < crate::pmove::ENTITY_NUMBER_WORLD
                    && other.time > level_time
                    && self.host.in_use(other.number);
                let attacker = if remembered {
                    Some(other.number)
                } else {
                    fallback_self.then_some(number)
                };
                let attacker = attacker.map(|number| self.attacker_of(number));
                let blow = DamageRequest {
                    level_time,
                    attacker,
                    direction: None,
                    point: Some(at),
                    damage,
                    flags,
                    means,
                };
                self.damage(
                    me,
                    NpcBlow {
                        request: blow,
                        spared_by_master: false,
                        surface: None,
                    },
                );
            }
            VehicleRequest::Surfaces {
                normal,
                at,
                view,
                magnitude,
                force,
                rider_at,
            } => self.surface_destruction(me, (normal, at, view), magnitude, force, rider_at),
            VehicleRequest::TurnFighter { target, turned } => {
                let Some(other) = self.actors.iter_mut().find(|npc| npc.number == target) else {
                    return;
                };
                let mut velocity = other.player.velocity();
                for axis in 0..3 {
                    velocity[axis] += turned.push[axis];
                }
                other.player.set_velocity(velocity);
                if let Some(vehicle) = other.vehicle.as_deref_mut() {
                    if let Some(pitch) = turned.pitch {
                        vehicle.full_angle_velocity[0] = pitch;
                    }
                    if let Some(roll) = turned.roll {
                        vehicle.full_angle_velocity[2] = roll;
                    }
                }
            }
            VehicleRequest::BoxSizing => self.damage_box_sizing(me),
            VehicleRequest::EjectDroid { kill } => self.eject_droid(me, kill),
            VehicleRequest::AttachDroid { origin, yaw } => self.attach_droid(me, origin, yaw),
            VehicleRequest::KillRider {
                rider,
                damage,
                by_vehicle,
            } => {
                let flags = if by_vehicle { DAMAGE_NO_PROTECTION } else { 0 };
                self.vehicle_outcomes.push(VehicleOutcome::KillRider {
                    rider,
                    damage,
                    flags,
                    by: by_vehicle.then_some(number),
                    means: MOD_SUICIDE,
                });
            }
            VehicleRequest::Explode {
                at,
                effect,
                mark,
                blast,
            } => {
                let info = self.actors[me]
                    .vehicle
                    .as_deref()
                    .map(|vehicle| std::sync::Arc::clone(&vehicle.info));
                let Some(info) = info else { return };
                if effect {
                    self.effect_id(info.explode_fx, at);
                    if let Some(mark) = mark {
                        let scorch =
                            i32::from(self.host.effect_index(b"ships/ship_explosion_mark"));
                        self.effect_id(scorch, mark);
                    }
                }
                let npc = &mut self.actors[me];
                // "so we don't recursively damage ourselves".
                npc.takes_damage = false;
                if let Some(blast) = blast {
                    self.vehicle_blast(
                        number,
                        blast,
                        info.explosion_damage,
                        info.explosion_radius,
                        None,
                        None,
                        None,
                    );
                }
                self.actors[me].think = NpcThink::Free(level_time + FRAMETIME);
            }
        }
    }

    /// `G_PlayEffectID(fx, at, {-90, 0, 0})` (`DeathUpdate`'s explosion and scorch mark):
    /// the effect's event where it happens, its angles pitched straight down.
    fn effect_id(&mut self, effect: i32, at: [f32; 3]) {
        self.play_effect_id(effect, at, [-90.0, 0.0, 0.0]);
    }

    /// `G_PlayEffectID(fx, org, ang)` (`g_utils.c:1271-1288`): a temp entity at `org` (its
    /// `s.origin` unsnapped) with `ang` as its angles — `{0, 1, 0}` for none.
    pub(crate) fn play_effect_id(&mut self, effect: i32, origin: [f32; 3], angles: [f32; 3]) {
        let angles = if angles == [0.0; 3] {
            [0.0, 1.0, 0.0]
        } else {
            angles
        };
        let mut event = EventEntity {
            event: EV_PLAY_EFFECT_ID,
            parameter: effect as u32,
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for axis in 0..3 {
            event.extra[axis] = (crate::npc_spawn::es::ORIGIN[axis], origin[axis].to_bits());
            event.extra[3 + axis] = (crate::npc_spawn::es::ANGLES[axis], angles[axis].to_bits());
        }
        self.host.raise(event);
    }
}
