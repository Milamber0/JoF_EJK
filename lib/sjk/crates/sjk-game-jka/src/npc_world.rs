//! What an NPC's think works on: every NPC of the level (it may change others — an alert
//! wakes its team), the level's alerts, and the host that owns the players, the entity
//! numbers and the map. The reference reaches all of these through globals (`g_entities`,
//! `level`, `NPCS`); here they are handed in, one [`NpcWorld`] per frame's thinks.

use crate::npc_senses::{AlertEvents, Body, SenseWorld};
use crate::npc_spawn::{NpcActor, NpcHost, es};
use crate::pmove::MovementTrace;

/// `PMF_DUCKED`; `ps.saberHolstered`, `ps.saberInFlight`.
const PMF_DUCKED: u16 = 1;
const PS_SABER_HOLSTERED: usize = 81;
const PS_SABER_IN_FLIGHT: usize = 88;
/// `SCF_FORCED_MARCH`.
pub(crate) const SCF_FORCED_MARCH: u32 = 0x1_0000;

/// The level as an NPC's think sees it.
pub struct NpcWorld<'a, H: NpcHost> {
    /// Every NPC, the thinking one among them.
    pub actors: &'a mut [NpcActor],
    /// The indices of `actors` in entity-number order: the order the reference's loops over
    /// `g_entities` meet them in.
    pub order: &'a [usize],
    /// `level.alertEvents`.
    pub alerts: &'a mut AlertEvents,
    /// The players, the entity numbers, the map.
    pub host: &'a mut H,
    /// `level.time`.
    pub level_time: i32,
    /// Scratch for the bodies a move meets.
    pub bodies: &'a mut Vec<crate::entity_clip::BoxObstacle>,
    /// Scratch for the clients' legs a move reads (`PM_BGEntForNum(n)->s.legsAnim`: a stab
    /// down at one lying), by number.
    pub body_legs: &'a mut Vec<(u16, u16)>,
    /// The level's space-ship triggers ([`crate::vehicle_triggers`]).
    pub ship_triggers: &'a mut crate::vehicle_triggers::ShipTriggers,
    /// Scratch for what a vehicle's move may bump into
    /// ([`crate::pmove::vehicle_impact::ImpactBody`]).
    pub impact_bodies: &'a mut Vec<crate::pmove::vehicle_impact::ImpactBody>,
    /// Scratch for a temp entity an NPC's event overflows into.
    pub overflow: &'a mut sjk_protocol::EntityState,
    /// The names the thinks fired (`G_UseTargets`: a death's `target`, a pain's
    /// `paintarget`), for the roster's caller to use.
    pub fired: &'a mut Vec<Vec<u8>>,
    /// What the level keeps for the NPCs' tactics: groups, combat points, the last move.
    pub level: &'a mut crate::npc_groups::NpcLevel,
    /// The player driving the vehicle whose think this is, if any ([`crate::vehicle_drive`]).
    pub rider: Option<crate::vehicle_rider::Rider<'a>>,
    /// That vehicle's passengers, borrowed with it.
    pub passengers: Vec<crate::vehicle_rider::Rider<'a>>,
    /// What the vehicles' thinks left for the players' side of the game.
    pub vehicle_outcomes: &'a mut Vec<crate::vehicle_drive::VehicleOutcome>,
    /// Scratch for a Force power's user's state while its powers run
    /// ([`NpcWorld::with_force_frame`]).
    pub spare_state: &'a mut Option<sjk_protocol::PlayerState>,
    /// The level's vehicle weapons (`g_vehWeaponInfo`), which a vehicle's guns fire.
    pub vehicle_weapons: &'a [crate::vehicle_fields::VehicleWeaponInfo],
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The actor with entity number `number`, if an NPC has it.
    pub fn actor_at(&self, number: u16) -> Option<usize> {
        self.actors.iter().position(|npc| npc.number == number)
    }

    /// Whatever has a client at `number`, as the senses read it: a player or an NPC.
    pub fn body(&self, number: u16) -> Option<Body> {
        if let Some(player) = self
            .host
            .players()
            .iter()
            .find(|player| player.number == number)
        {
            return Some(*player);
        }
        self.actor_at(number)
            .map(|at| npc_body(&self.actors[at], self.level_time))
    }

    /// The body of the NPC at `at`.
    pub fn npc(&self, at: usize) -> Body {
        npc_body(&self.actors[at], self.level_time)
    }

    /// Every client body in entity-number order — the players, then the NPCs — calling
    /// `each` with it. No allocation.
    pub fn each_body(&self, mut each: impl FnMut(Body)) {
        for player in self.host.players() {
            each(*player);
        }
        for &at in self.order {
            each(npc_body(&self.actors[at], self.level_time));
        }
    }

    /// `trap->Trace` through the world, the players and every NPC's body (the host skips
    /// `pass`), the bodies gathered into the reused scratch.
    pub fn trace_bodies(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> MovementTrace {
        let Self {
            actors,
            bodies,
            host,
            ..
        } = self;
        bodies.clear();
        bodies.extend(
            actors
                .iter()
                .filter(|npc| {
                    npc.contents != 0
                        && npc.number != pass
                        && !crate::vehicle_droid::owned_by(npc, pass)
                })
                .map(NpcActor::body),
        );
        host.trace(start, mins, maxs, end, pass, mask, bodies)
    }

    /// The senses' view of the world through the host.
    pub fn senses(&mut self) -> HostSenses<'_, H> {
        HostSenses(self.host)
    }
}

/// An NPC as the senses of another read it.
pub fn npc_body(npc: &NpcActor, level_time: i32) -> Body {
    let field = |index: usize| npc.state.raw_field(index).unwrap_or(0);
    Body {
        number: npc.number,
        npc: true,
        origin: npc.current_origin,
        mins: npc.mins,
        maxs: npc.maxs,
        view_height: npc.player.view_height(),
        view_angles: npc.player.view_angles(),
        eye_point: npc.mind.eye_point,
        eye_angles: npc.mind.eye_angles,
        health: npc.health,
        flags: npc.flags,
        entity_flags: field(es::EFLAGS),
        player_team: npc.player_team,
        enemy_team: npc.enemy_team,
        session_team: npc.session_team,
        class: npc.definition.client_class,
        weapon: field(es::WEAPON) as i32,
        enemy: npc.mind.enemy,
        spectating: false,
        surrendering: npc.mind.surrender_time > level_time
            || npc.script_flags & SCF_FORCED_MARCH != 0,
        velocity: npc.player.velocity(),
        ducked: npc.player.movement_flags() & PMF_DUCKED != 0,
        saber_holstered: npc.player.raw_field(PS_SABER_HOLSTERED).unwrap_or(0) != 0,
        saber_in_flight: npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0,
    }
}

/// The host's traces as the senses ask for them: points, through the world and whatever
/// the host links, never the NPCs' own boxes (no sight mask stops at a body).
pub struct HostSenses<'h, H: NpcHost>(pub &'h mut H);

impl<H: NpcHost> SenseWorld for HostSenses<'_, H> {
    fn line(&mut self, start: [f32; 3], end: [f32; 3], pass: u16, mask: u32) -> MovementTrace {
        self.0
            .trace(start, [0.0; 3], [0.0; 3], end, pass, mask, &[])
    }

    fn in_pvs(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.0.in_pvs(from, to)
    }

    fn glass(&self, number: u16) -> bool {
        self.0.glass(number)
    }
}
