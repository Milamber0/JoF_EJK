//! Players on vehicles on this server (`ClientThink_real`'s vehicle parts,
//! `codemp/game/g_active.c:1889-1910`, `2809-2813`, `3052-3065`, `3497-3510`; `player_die`'s
//! eject, `g_combat.c:2257-2275`): the pilot's command handed to its vehicle before its own
//! move, its move as a rider ([`Riding`]), the vehicle's think on that command after it,
//! what a vehicle's think leaves for the players (a rider killed, an explosion's blast), and
//! a dying rider thrown off.
//!
//! The vehicle is an NPC of the roster; the rules are `sjk_game_jka::vehicle_*`. The
//! players it carries are borrowed by the roster as riders ([`riders`]).

use super::*;
use sjk_game_jka::npc_spawn::NpcHost;
use sjk_game_jka::pmove::riding::Riding;
use sjk_game_jka::vehicle_drive::VehicleOutcome;
use sjk_game_jka::vehicle_rider::CONTENTS_BODY;

/// `BUTTON_USE`.
const BUTTON_USE: u16 = 32;
/// `ps.eFlags2`; `EF2_HYPERSPACE`.
const PS_EFLAGS2: usize = 103;

impl NativeGame {
    /// Authoritative landing/standing boarding from multiplayer Pmove.
    pub(in crate::bridge) fn board_from_ground(
        &mut self,
        client: usize,
        landing: Option<u16>,
        level_time: i32,
    ) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        if !peer.playing() || peer.state.vehicle_entity_num() != 0 || peer.state.zoom_mode() != 0 {
            return;
        }
        let ground = landing.unwrap_or_else(|| peer.state.ground_entity_num());
        let gametype = self.gametype;
        let team = peer.session.team;
        if !(32..1022).contains(&ground) {
            return;
        }
        let Some(npc) = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == ground)
        else {
            return;
        };
        let Some(vehicle) = npc.vehicle.as_ref() else {
            return;
        };
        if !sjk_game_jka::vehicle_auto_board::may_board(
            vehicle.kind(),
            npc.player.vehicle_entity_num() != 0,
            npc.spawnflags & 2 != 0,
            landing.is_some(),
            gametype,
            npc.allied_team,
            team,
        ) {
            return;
        }
        let _ = self.with_rider(client, |roster, rider, host| {
            roster.board(ground, rider, level_time, host);
        });
    }

    /// `ClientThink_real`'s opening for a player on a vehicle: the command handed to the
    /// vehicle it pilots, and the use key debounced while it rides.
    pub(crate) fn riding_opening(
        &mut self,
        client: usize,
        command: &mut UserCommand,
        level_time: i32,
    ) {
        let Self {
            server,
            world,
            players,
            npcs,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        let vehicle = peer.state.vehicle_entity_num();
        if vehicle == 0 {
            return;
        }
        if let Some(npc) = npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == vehicle)
        {
            sjk_game_jka::vehicle_drive::hand_command(
                npc,
                client as u16,
                peer.state.command_time(),
                command,
            );
        }
        if peer.riding.use_delay > level_time {
            command.buttons &= !BUTTON_USE;
        }
    }

    /// The vehicle a player's move rides, handed to its movement before the move.
    pub(crate) fn riding_move(&mut self, client: usize) {
        let Self {
            server,
            world,
            players,
            npcs,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        let vehicle = peer.state.vehicle_entity_num();
        // A ship a boundary turns back turns its rider's view too (`PM_VehForcedTurning`).
        let turnaround = npcs.roster.turnaround(vehicle);
        let riding = npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == vehicle && vehicle != 0)
            .and_then(|npc| {
                Some(Riding {
                    turnaround,
                    ..Riding::of(
                        vehicle,
                        npc.vehicle.as_deref()?,
                        client as u16,
                        npc.player.speed(),
                    )
                })
            });
        peer.movement.set_riding(riding);
    }

    /// What a rider's move did to its vehicle (its command, its hyperspace time) and to
    /// itself (`solidHack`: no contents while it does not fit on it).
    pub(crate) fn ridden(&mut self, client: usize, level_time: i32) {
        let Self {
            server,
            world,
            players,
            npcs,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        if let Some(riding) = peer.movement.riding() {
            if riding.solid_hack {
                peer.riding.solid_hack = level_time + 200;
            }
            if let Some(npc) = npcs
                .roster
                .actors
                .iter_mut()
                .find(|npc| npc.number == riding.vehicle)
                && let Some(vehicle) = npc.vehicle.as_deref_mut()
                && riding.apply_to(vehicle)
            {
                let flags = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0);
                npc.player.set_raw_field(
                    PS_EFLAGS2,
                    flags | sjk_game_jka::vehicle_riders::EF2_HYPERSPACE,
                );
            }
        }
        peer.movement.set_riding(None);
        let body = &mut peer.riding;
        if body.solid_hack != 0 {
            if body.solid_hack > level_time {
                body.contents = 0;
            } else {
                body.contents = CONTENTS_BODY;
                body.solid_hack = 0;
            }
        }
    }

    /// `ClientThink(m_iVehicleNum, &m_ucmd)` at the end of any rider's think — a pilot's or
    /// a passenger's (`g_active.c:3497-3510`): the vehicle's own think on its command, its
    /// whole crew borrowed; a vehicle gone takes the player's `m_iVehicleNum` with it.
    pub(crate) fn drive_ridden(&mut self, client: usize, level_time: i32) {
        let vehicle = self
            .peer(client)
            .map_or(0, |peer| peer.state.vehicle_entity_num());
        if vehicle == 0 {
            return;
        }
        let crew = self.crew_of(vehicle, client);
        let driven = self.with_riders(&crew, |roster, riders, host| {
            let (pilot, passengers) = riders
                .split_first_mut()
                .expect("the crew has its first rider");
            let passengers = passengers
                .iter_mut()
                .map(|passenger| passenger.reborrow())
                .collect();
            roster.drive(vehicle, pilot.reborrow(), passengers, level_time, host)
        });
        let (found, fired) = driven.unwrap_or((false, Vec::new()));
        if !found && let Some(peer) = self.peer_mut(client) {
            peer.state.set_raw_field(84, 0);
            peer.movement.state_mut().vehicle_entity_num = 0;
        }
        for name in fired {
            self.fire_targets(&String::from_utf8_lossy(&name), usize::MAX, level_time);
        }
        self.vehicle_outcomes(level_time);
    }

    /// `player_die` for a player on a vehicle (`g_combat.c:2257-2268`): thrown off, where
    /// no side is clear where it is.
    pub(crate) fn eject_on_death(&mut self, client: usize, level_time: i32) {
        self.no_corpse_in_space(client);
        let vehicle = self
            .peer(client)
            .map_or(0, |peer| peer.state.vehicle_entity_num());
        self.throw_off(client, level_time, false);
        self.ship_death(client, vehicle);
    }

    /// `G_LeaveVehicle` (`g_cmds.c:2524-2544`), for a player leaving the game (`ConCheck`:
    /// thrown off as one no longer connected) or going to the intermission: forced off
    /// whatever it rides, its `m_iVehicleNum` cleared even where the vehicle is gone.
    pub(crate) fn leave_vehicle(&mut self, client: usize, level_time: i32, disconnecting: bool) {
        self.throw_off(client, level_time, disconnecting);
        if let Some(peer) = self.peer_mut(client) {
            peer.state.set_raw_field(84, 0);
            peer.movement.state_mut().vehicle_entity_num = 0;
        }
    }

    /// `Eject(pVeh, ent, qtrue)` for the player at `client` on the vehicle it rides.
    fn throw_off(&mut self, client: usize, level_time: i32, disconnecting: bool) {
        let vehicle = self
            .peer(client)
            .map_or(0, |peer| peer.state.vehicle_entity_num());
        if vehicle == 0 {
            return;
        }
        let _ = self.with_rider(client, |roster, rider, host| {
            if disconnecting {
                rider.connected = false;
            }
            let owner = rider.number;
            let bodies: Vec<BoxObstacle> = roster
                .actors
                .iter()
                .filter(|npc| {
                    npc.contents != 0 && sjk_game_jka::vehicle_board::owner_of(npc) != owner
                })
                .map(|npc| npc.body())
                .collect();
            let mut trace = |start, mins, maxs, end, mask| {
                host.trace(start, mins, maxs, end, owner, mask, &bodies)
            };
            roster.eject(vehicle, rider, true, level_time, &mut trace)
        });
    }

    /// `PM_VehicleImpact`'s knock on what a vehicle struck (`bg_slidemove.c:506-552`): a
    /// player it may treat as an enemy (`BG_KnockDownable`, `G_CanBeEnemy`) knocked down
    /// for 1100 ms, the vehicle credited for its fall, thrown along the vehicle's velocity
    /// and 200 up; then the blow — forty times the magnitude on a humanoid, at least 1.
    #[allow(clippy::too_many_arguments)]
    fn rammed(
        &mut self,
        vehicle: u16,
        attacker: u16,
        target: u16,
        (magnitude, humanoid_scale): (f32, f32),
        velocity: [f32; 3],
        at: [f32; 3],
        command_time: i32,
        level_time: i32,
    ) {
        const CLASS_VEHICLE: i32 = 53;
        let vehicle_team = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == vehicle)
            .map_or(0, |npc| npc.session_team);
        let humanoid = match self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == target)
        {
            Some(npc) => npc.definition.entity_class != CLASS_VEHICLE,
            None => self.peer(usize::from(target)).is_some(),
        };
        let (gametype, friendly_fire) = (self.gametype, self.cvars.integer(b"g_friendlyFire") != 0);
        if let Some(peer) = self.peer_mut(usize::from(target))
            && sjk_game_jka::knockdown::knockdownable(&peer.state)
            && !(peer.state.duel_in_progress() && peer.state.duel_index() != vehicle)
            && (gametype < GAMETYPE_TEAM || friendly_fire || peer.session.team != vehicle_team)
        {
            // "smash!"
            sjk_game_jka::knockdown::rammed(&mut peer.state, &mut peer.knockdown, command_time);
            peer.wounds.other_killer = sjk_game_jka::damage::OtherKiller {
                number: vehicle,
                time: command_time + 5_000,
                debounce_time: command_time + 100,
            };
            let mut thrown = peer.state.velocity();
            for axis in 0..3 {
                thrown[axis] += velocity[axis];
            }
            thrown[2] += 200.0;
            peer.state.set_velocity(thrown);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        let damage = sjk_game_jka::pmove::vehicle_impact::x86_int(
            magnitude * if humanoid { humanoid_scale } else { 1.0 },
        )
        .max(1);
        let request = DamageRequest {
            level_time,
            attacker: self.attacker_for(attacker),
            direction: None,
            point: Some(at),
            damage,
            flags: 0,
            means: sjk_game_jka::means_of_death::MOD_MELEE,
        };
        let _ = self.strike(usize::from(attacker), usize::from(target), request, false);
        self.flush_npc_blows();
    }

    /// A walker's weight on `target` (`g_active.c:3035-3046`): 100 by the walker, down onto
    /// a player or an NPC with health, `MOD_CRUSH`.
    fn crushed(&mut self, vehicle: u16, target: u16, level_time: i32) {
        let origin = match self.peer(usize::from(target)) {
            Some(peer) if peer.health != 0 => peer.state.origin(),
            Some(_) => return,
            None => match self
                .npcs
                .roster
                .actors
                .iter()
                .find(|npc| npc.number == target)
            {
                Some(npc) if npc.health != 0 && npc.takes_damage => npc.current_origin,
                _ => return,
            },
        };
        let attacker = self.attacker_for(vehicle);
        let request = DamageRequest {
            level_time,
            attacker,
            direction: Some([0.0, 0.0, -1.0]),
            point: Some(origin),
            damage: 100,
            flags: 0,
            means: sjk_game_jka::means_of_death::MOD_CRUSH,
        };
        let _ = self.strike(usize::MAX, usize::from(target), request, false);
        self.flush_npc_blows();
    }

    /// What the vehicles' thinks left for the players: a rider killed with its vehicle, an
    /// explosion's blast on the players and the NPCs (`G_RadiusDamage`, no attacker), a
    /// walker's weight, a ram, a pilot's event.
    pub(crate) fn vehicle_outcomes(&mut self, level_time: i32) {
        for outcome in self.npcs.roster.take_vehicle_outcomes() {
            match outcome {
                VehicleOutcome::KillRider {
                    rider,
                    damage,
                    flags,
                    by,
                    means,
                } => {
                    let attacker = by.and_then(|number| self.attacker_for(number));
                    let point = self
                        .peer(usize::from(rider))
                        .map(|peer| peer.state.origin());
                    let request = DamageRequest {
                        level_time,
                        attacker,
                        direction: None,
                        point,
                        damage,
                        flags,
                        means,
                    };
                    let _ = self.hurt(usize::from(rider), request);
                }
                VehicleOutcome::PilotEvent {
                    pilot,
                    event,
                    parameter,
                } => {
                    if let Some(peer) = self.peer_mut(usize::from(pilot)) {
                        sjk_game_jka::player_entity::add_event(&mut peer.state, event, parameter);
                        peer.entity.event_raised(level_time);
                    }
                }
                VehicleOutcome::Crush { vehicle, target } => {
                    self.crushed(vehicle, target, level_time)
                }
                VehicleOutcome::Jumped {
                    vehicle,
                    pilot,
                    origin,
                    angles,
                    sound,
                } => self.jumped(vehicle, pilot, origin, angles, sound, level_time),
                VehicleOutcome::Ram {
                    vehicle,
                    attacker,
                    target,
                    magnitude,
                    velocity,
                    at,
                    command_time,
                    humanoid_scale,
                } => {
                    self.rammed(
                        vehicle,
                        attacker,
                        target,
                        (magnitude, humanoid_scale),
                        velocity,
                        at,
                        command_time,
                        level_time,
                    );
                }
                VehicleOutcome::Blast {
                    at,
                    damage,
                    radius,
                    attacker,
                    spared,
                    rider_at,
                    ..
                } => {
                    // "say my pilot did it".
                    let attacker = attacker.map(|number| self.npcs.roster.splash_credit(number));
                    // The roster's NPCs were struck as the blast went off.
                    self.blast(
                        at,
                        damage,
                        radius,
                        attacker,
                        spared,
                        sjk_game_jka::means_of_death::MOD_SUICIDE,
                        level_time,
                        rider_at,
                        false,
                    );
                }
            }
        }
    }

    /// `G_RadiusDamage` of `damage` within `radius` of `at` by `attacker` (an entity
    /// number), `spared` left out: the players and the NPCs, in entity order. `rider_at`
    /// is a pilot the blast finds where its vehicle's move had not yet carried it. The
    /// roster's NPCs are left out unless `npcs` (a vehicle's blast struck them already).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn blast(
        &mut self,
        at: [f32; 3],
        damage: i32,
        radius: f32,
        attacker: Option<u16>,
        spared: Option<u16>,
        means: u32,
        level_time: i32,
        rider_at: Option<(u16, [f32; 3])>,
        npcs: bool,
    ) {
        let attacker = attacker.and_then(|number| self.attacker_for(number));
        self.gather_splash_targets();
        if !npcs {
            let roster = &self.npcs.roster;
            self.splash_targets
                .retain(|target| !roster.is_npc(target.number));
        }
        if let Some((rider, then)) = rider_at
            && let Some(target) = self
                .splash_targets
                .iter_mut()
                .find(|target| target.number == rider)
        {
            let shift: [f32; 3] = std::array::from_fn(|axis| then[axis] - target.origin[axis]);
            target.origin = then;
            target.bounds = (
                std::array::from_fn(|axis| target.bounds.0[axis] + shift[axis]),
                std::array::from_fn(|axis| target.bounds.1[axis] + shift[axis]),
            );
        }
        let targets = std::mem::take(&mut self.splash_targets);
        let map = self.map.take();
        let mut hurt = |number: u16, request: DamageRequest| {
            self.strike(usize::MAX, usize::from(number), request, false)
                .1
        };
        match &map {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                radius_damage(
                    at,
                    attacker,
                    damage as f32,
                    radius,
                    spared,
                    means,
                    level_time,
                    &targets,
                    &world,
                    &mut hurt,
                );
            }
            None => {
                radius_damage(
                    at,
                    attacker,
                    damage as f32,
                    radius,
                    spared,
                    means,
                    level_time,
                    &targets,
                    &Void,
                    &mut hurt,
                );
            }
        }
        self.map = map;
        self.flush_npc_blows();
        self.splash_targets = targets;
    }
}

#[path = "bridge_space.rs"]
mod bridge_space;
#[path = "bridge_riders.rs"]
pub(super) mod riders;
