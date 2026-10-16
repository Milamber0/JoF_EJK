//! An NPC made from its spawner: `NPC_Spawn_Do` (`codemp/game/NPC_spawn.c:1360-1746`).
//!
//! The NPC is a native actor ([`NpcActor`]): the game's record of what the reference keeps
//! on its `gentity_t`, its `gNPC_t` and its fake `gclient_t`. Its wire state (`s`) is the
//! `ET_NPC` entity a legacy client draws; its player state (`client->ps`) is the protocol's
//! own [`PlayerState`], the state the movement code (`Pmove`, [`crate::pmove::Predictor`])
//! starts from, as a player's is. Nothing here knows how many there may be: the entity
//! numbers come from the [`NpcHost`], which owns them.
//!
//! What `NPC_Spawn_Do` does, in its order: the spawner's count, the entity and its goal
//! entity allocated (`G_Spawn` twice), the type lower-cased, the spawner's sound flags and
//! key handed on, the origin, the definition read (`NPC_ParseParms`, [`NpcParms::parse`]),
//! the spawner's names, health, angles and spawn flags handed on, the entity made an
//! `ET_NPC` that is not drawn yet (`EF_NODRAW`) and interpolated from its place, the default
//! script flags, the teams, and — for a spawner with nothing left to spawn — its target
//! fired and the spawner freed. `NPC_Begin` follows a frame later ([`crate::npc_begin`]).
//!
//! Held to `tools/game-oracle/npcspawn.c` (`game-npcspawn.txt`).

use crate::npc_parms::{NpcDefinition, NpcParms, NpcRefusalReason, NpcSpawn};
use crate::npc_spawners::NpcSpawner;
use crate::pmove::MovementTrace;
use crate::saber_definition::{SaberParms, SaberParseHost};
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

/// Wire fields of `entityState_t` the spawn writes (`msg.cpp`'s table).
pub(crate) mod es {
    pub const POS_TIME: usize = 0;
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const POS_DELTA: [usize; 3] = [6, 7, 10];
    pub const POS_TYPE: usize = 23;
    pub const POS_DURATION: usize = 20;
    pub const APOS_TIME: usize = 34;
    pub const APOS_BASE: [usize; 3] = [5, 3, 33];
    pub const APOS_TYPE: usize = 15;
    pub const TYPE: usize = 8;
    pub const ANGLES: [usize; 3] = [25, 9, 24];
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const EFLAGS: usize = 19;
    pub const WEAPON: usize = 14;
    pub const TEAM_OWNER: usize = 21;
    pub const GROUND_ENTITY: usize = 22;
    pub const G2_RADIUS: usize = 38;
    pub const OWNER: usize = 40;
    pub const MODEL_INDEX: usize = 46;
    pub const MODEL_GHOUL2: usize = 54;
    pub const SHOULD_TARGET: usize = 57;
    pub const HEALTH: usize = 69;
    pub const NPC_SABER1: usize = 72;
    pub const MAX_HEALTH: usize = 73;
    pub const SOUNDS: [usize; 4] = [80, 103, 104, 105];
    pub const NPC_CLASS: usize = 88;
    pub const BOLT_TO_PLAYER: usize = 101;
    pub const NPC_SABER2: usize = 102;
    pub const SURFACES_OFF: usize = 95;
    pub const SURFACES_ON: usize = 106;
}

/// `ET_NPC`.
pub const ET_NPC: u32 = 13;
/// `TR_STATIONARY`, `TR_INTERPOLATE`.
const TR_STATIONARY: u32 = 0;
const TR_INTERPOLATE: u32 = 1;
/// `EF_NODRAW`.
pub const EF_NODRAW: u32 = 1 << 8;
/// `ENTITYNUM_NONE`, `ENTITYNUM_WORLD`.
pub const ENTITYNUM_NONE: u16 = 1_023;
pub const ENTITYNUM_WORLD: u16 = 1_022;
/// `FL_NOTARGET`, `FL_NO_KNOCKBACK`, `FL_SHIELDED`.
pub const FL_NOTARGET: u32 = 0x20;
pub const FL_NO_KNOCKBACK: u32 = 0x800;
pub const FL_SHIELDED: u32 = 0x8_0000;
/// `SCF_CHASE_ENEMIES | SCF_LOOK_FOR_ENEMIES`: `NPC_DefaultScriptFlags`.
const DEFAULT_SCRIPT_FLAGS: u32 = 0x400 | 0x800;
/// `NPCAI_MATCHPLAYERWEAPON`.
pub const NPCAI_MATCHPLAYERWEAPON: u32 = 0x4_0000;
/// `BS_WAIT`: the `test` NPC's behaviour.
const BS_WAIT: i32 = 10;
/// `FRAMETIME`: `NPC_Begin` a frame after the spawn.
pub const FRAMETIME: i32 = 100;
/// `persistant[PERS_TEAM]`.
pub(crate) const PERS_TEAM: usize = 3;
/// `fd.forcePowerLevel[FP_LEVITATION]`, `fd.forcePowerLevel[FP_SEE]`: the levels on the wire.
const PS_LEVITATION_LEVEL: usize = 52;
const PS_SEE_LEVEL: usize = 109;

/// What an NPC's think (`ent->think`, `ent->nextthink`) is waiting to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcThink {
    /// `NPC_Begin` at this time: placed, not yet begun.
    Begin(i32),
    /// `G_FreeEntity` at this time: a blocked NPC that gave up.
    Free(i32),
    /// `NPC_Think` at this time: begun ([`crate::npc_think`]).
    Think(i32),
    /// `NPC_RemoveBody` at this time: dead, its body on its way out ([`crate::npc_dead`]).
    RemoveBody(i32),
}

/// An NPC: the reference's `gentity_t` with its `gNPC_t` and fake `gclient_t`, as far as
/// the spawn writes them.
#[derive(Clone, Debug)]
pub struct NpcActor {
    /// Its entity number, as the host gave it.
    pub number: u16,
    /// `NPC->tempGoal`: the goal entity allocated with it.
    pub goal: Option<u16>,
    /// `saberStoredIndex`: the saber entity `WP_SaberInitBladeData` gave a saber carrier.
    pub saber_entity: Option<u16>,
    /// Its saber between frames — blades, trails, wounds and the saber entity
    /// ([`crate::npc_saber`]).
    pub saber: crate::npc_saber::NpcSaber,
    /// `NPC_type`, lower-cased.
    pub npc_type: Vec<u8>,
    /// What `NPC_ParseParms` read.
    pub definition: NpcDefinition,
    /// `s`: the wire entity every client is sent.
    pub state: EntityState,
    /// `client->ps`.
    pub player: PlayerState,
    /// `ent->health`, `client->pers.maxHealth`, and `ent->maxHealth` (the health bar's).
    pub health: i32,
    pub max_health: i32,
    pub bar_max_health: i32,
    /// `ent->flags` (`FL_*`).
    pub flags: u32,
    /// `r.svFlags`.
    pub server_flags: u32,
    pub spawnflags: i32,
    /// Milliseconds before a blocked begin tries again; below zero it gives up.
    pub wait: f32,
    /// `r.contents`, `clipmask`.
    pub contents: u32,
    pub clip_mask: u32,
    /// `r.mins`, `r.maxs`, `r.currentOrigin`.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub current_origin: [f32; 3],
    /// `client->playerTeam`, `client->enemyTeam` (`npcteam_t`), `sess.sessionTeam`.
    pub player_team: i32,
    pub enemy_team: i32,
    pub session_team: i32,
    /// `NPC->behaviorState`, `NPC->defaultBehavior` (`bState_t`), `NPC->scriptFlags`,
    /// `NPC->aiFlags`, `NPC->desiredYaw`.
    pub behavior_state: i32,
    pub default_behavior: i32,
    pub script_flags: u32,
    pub ai_flags: u32,
    pub desired_yaw: f32,
    /// `ent->count` (a seeker's shots).
    pub count: i32,
    /// `fd.forcePowerLevel`.
    pub force_levels: [i32; crate::npc_parms::NPC_FORCE_POWERS],
    /// The Force data no wire field carries (`forcedata_t`): durations, debounces, the
    /// grip's and the drain's — cleared with its client, set by `WP_InitForcePowers` as it
    /// begins.
    pub force: crate::force_powers::ForcePowers,
    /// The names it goes by and fires: `targetname` (and `script_targetname`), `target`
    /// (on death), `target2`, `target3` (blocked for good), `target4`, `paintarget`,
    /// `opentarget`.
    pub targetname: Option<Vec<u8>>,
    pub target: Option<Vec<u8>>,
    pub target2: Option<Vec<u8>>,
    pub target3: Option<Vec<u8>>,
    pub target4: Option<Vec<u8>>,
    pub pain_target: Option<Vec<u8>>,
    pub open_target: Option<Vec<u8>>,
    /// `ent->message`: a key it carries. `ent->fullName`.
    pub message: Option<Vec<u8>>,
    pub full_name: Option<Vec<u8>>,
    /// `behaviorSet[BSET_SPAWN]`: the ICARUS script it would run once begun.
    pub spawn_script: Option<Vec<u8>>,
    /// `alliedTeam`, `teamnodmg`.
    pub allied_team: i32,
    pub team_no_damage: i32,
    /// `ent->think`.
    pub think: NpcThink,
    /// `level.time` at `NPC_Spawn_Do`: when `SetupGameGhoul2Model` set its server-side model
    /// going.
    pub spawn_time: i32,
    /// What its thinking keeps (`gNPC_t`, `renderInfo`, `enemy`, the timers).
    pub mind: crate::npc_mind::NpcMind,
    /// Its movement: the player-state fields `Pmove` does not network, its skeleton's
    /// animation lengths and its box and class ([`crate::npc_client_think`]).
    pub movement: crate::pmove::Predictor,
    /// `localAnimIndex == 0`: its skeleton is the humanoid one.
    pub humanoid: bool,
    /// `takedamage`: begun, it can be hurt (`NPC_Begin`), and a corpse still can.
    pub takes_damage: bool,
    /// `r.absmin`, `r.absmax`: the box as it was last linked, grown by a unit — where a blow
    /// landed on it is judged by this (`G_GetHitLocation`).
    pub link: ([f32; 3], [f32; 3]),
    /// `m_pVehicle`: the vehicle this NPC is ([`crate::vehicle`]).
    pub vehicle: Option<Box<crate::vehicle::Vehicle>>,
}

impl NpcActor {
    /// The box it is linked by (`r.mins`, `r.maxs`).
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        (self.mins, self.maxs)
    }

    /// Whether its begin has run: it stands in the world (and its body lies there once
    /// dead, until it is freed).
    pub fn begun(&self) -> bool {
        self.takes_damage
    }

    /// `trap->LinkEntity`: the box it is linked by, from where it now is.
    pub fn relink(&mut self) {
        let origin = self.current_origin;
        self.link = (
            std::array::from_fn(|axis| origin[axis] + self.mins[axis] - 1.0),
            std::array::from_fn(|axis| origin[axis] + self.maxs[axis] + 1.0),
        );
    }

    /// Its body as others' moves and traces meet it: linked by its box, solid as its
    /// contents are.
    pub fn body(&self) -> crate::entity_clip::BoxObstacle {
        crate::entity_clip::BoxObstacle {
            entity: self.number,
            origin: self.current_origin,
            bounds: (self.mins, self.maxs),
            contents: self.contents,
            model: None,
        }
    }
}

/// What the spawn needs of the world it spawns into: entity numbers, the tables clients
/// are told, the models the game data has, traces, and who is in a box.
pub trait NpcHost: SaberParseHost {
    /// `G_Spawn` for an entity every client is sent (linked).
    fn spawn_entity(&mut self) -> Option<u16>;
    /// `G_Spawn` for an entity the server alone keeps (`SVF_NOCLIENT`): a spawner, a goal
    /// entity, a saber entity.
    fn spawn_hidden(&mut self) -> Option<u16>;
    /// `G_FreeEntity`.
    fn free(&mut self, number: u16);
    /// The entity's wire state, link box and contents (`r.contents`), as they now are.
    fn publish(
        &mut self,
        number: u16,
        state: &EntityState,
        bounds: ([f32; 3], [f32; 3]),
        contents: u32,
    );
    /// `G_ModelIndex`, `G_EffectIndex`.
    fn model_index(&mut self, name: &[u8]) -> u16;
    fn effect_index(&mut self, name: &[u8]) -> u16;
    /// `RegisterItem` for the item list's row.
    fn register_item(&mut self, item: usize);
    /// Whether `models/players/<model>/model.glm` loads (`G2API_InitGhoul2Model`).
    fn model_loads(&mut self, model: &[u8]) -> bool;
    /// `trap->Trace` through the world, the players and `bodies` (the NPCs, which the
    /// caller knows), skipping `pass`.
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
        bodies: &[crate::entity_clip::BoxObstacle],
    ) -> MovementTrace;
    /// Whether a player with a body (`contents & MASK_NPCSOLID`) is linked where its box,
    /// grown by a unit, meets `mins`..`maxs` (`EntitiesInBox`).
    fn player_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3]) -> bool;
    /// `G_KillBox`'s players: every player linked in the box telefragged by `killer`.
    fn telefrag_players(&mut self, mins: [f32; 3], maxs: [f32; 3], killer: u16);
    /// [`Self::player_in_box`] but for player `owner`: `NPC_SpotWouldTelefrag` spares the
    /// NPC's owner (`r.ownerNum`, a player's seeker's). A host with one player at most may
    /// leave it to [`Self::player_in_box`].
    fn player_in_box_but(&mut self, mins: [f32; 3], maxs: [f32; 3], _owner: u16) -> bool {
        self.player_in_box(mins, maxs)
    }
    /// [`Self::telefrag_players`] but for player `owner`, whom `G_KillBox` spares.
    fn telefrag_players_but(&mut self, mins: [f32; 3], maxs: [f32; 3], killer: u16, _owner: u16) {
        self.telefrag_players(mins, maxs, killer);
    }
    /// `G_TempEntity` for `event` (a sound the NPC makes).
    fn raise(&mut self, event: crate::event_entity::EventEntity);
    /// Client 0's `s.origin` and `playerTeam` (zero and none for a slot never used), which
    /// the debugging NPC called `test` takes (`NPC_spawn.c:1591-1606`).
    fn client_zero(&self) -> ([f32; 3], i32);
    /// A line for the server's console (`Com_Printf`).
    fn print(&mut self, text: &str);
    /// `g_gravity`, `g_npcspskill`, `g_gametype`.
    fn gravity(&self) -> f32;
    fn skill(&self) -> i32;
    fn gametype(&self) -> i32;
    /// The game's generator (`Q_irand`'s), for what takes it whole.
    fn rng(&mut self) -> &mut crate::player_death::Rng;
    /// The players in the game, as an NPC's senses read them, in client order.
    fn players(&self) -> &[crate::npc_senses::Body];
    /// Whether entity `number` is in use (`g_entities[number].inuse`).
    fn in_use(&self, number: u16) -> bool;
    /// `trap->InPVS`.
    fn in_pvs(&self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// The health of a thing at `number` that takes damage and is no body (a breakable, a
    /// mine), which an NPC's shot may go through (`NPC_BSST_Attack`'s `takedamage` and
    /// `health < 40`). `None` for anything else.
    fn damageable_health(&self, _number: u16) -> Option<i32> {
        None
    }
    /// What the entity at `number` in a waypoint edge's way is to the navigator: a door
    /// (locked or not), a breakable, a removable usable (`GVM_NAV_EntIs*`).
    fn nav_obstacle(&self, _number: u16) -> crate::npc_nav_setup::NavObstacle {
        crate::npc_nav_setup::NavObstacle::Other
    }
    /// The linked place and box (`r.currentOrigin`, `r.mins`, `r.maxs`) of an entity that
    /// is no player or NPC, as an NPC's move runs into it; `None` for all zeros (the world).
    fn entity_box(&self, _number: u16) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
        None
    }
    /// The middle of the door team a `func_door` at `number` belongs to
    /// (`CalcTeamDoorCenter`); `None` for anything that is no `func_door`.
    fn door_center(&self, _number: u16) -> Option<[f32; 3]> {
        None
    }
    /// `EntIsGlass`: a breakable pane at `number`.
    fn glass(&self, _number: u16) -> bool {
        false
    }
    /// `Pmove` for an NPC (`ClientThink_real`, `g_active.c:3015`): `movement` moved by
    /// `command` through the world, the players and `bodies` (the other NPCs), skipping
    /// `pass` (the NPC itself). A host that keeps the NPC's model gives the move its feet
    /// (`pm->ghoul2`, [`crate::pmove::MoveContext::with_foot_bolts`]).
    fn move_npc(
        &mut self,
        movement: &mut crate::pmove::Predictor,
        command: sjk_protocol::UserCommand,
        context: &crate::pmove::MoveContext,
        pass: u16,
        bodies: &[crate::entity_clip::BoxObstacle],
    );
    /// Native burrowing movement: preserve world collision but pass through bodies.
    fn move_npc_buried(
        &mut self,
        movement: &mut crate::pmove::Predictor,
        command: sjk_protocol::UserCommand,
        context: &crate::pmove::MoveContext,
        pass: u16,
        bodies: &[crate::entity_clip::BoxObstacle],
    ) {
        self.move_npc(movement, command, context, pass, bodies);
    }
    /// Read-only grounded state for native vibration sensing.
    fn client_ground(&self, _number: u16) -> Option<u16> {
        None
    }

    /// [`Self::move_npc`] for an NPC in a saber lock, pushing against `partner`
    /// (`PM_SaberLocked`, [`crate::pmove::Predictor::predict_command_locked`]) with its
    /// weight `hits`, the game's generator drawn: what the lock did. A host that never
    /// locks may leave it to [`Self::move_npc`].
    #[allow(clippy::too_many_arguments)]
    fn move_npc_locked(
        &mut self,
        movement: &mut crate::pmove::Predictor,
        command: sjk_protocol::UserCommand,
        context: &crate::pmove::MoveContext,
        pass: u16,
        bodies: &[crate::entity_clip::BoxObstacle],
        partner: crate::npc_saber_lock::LockPartner<'_>,
        hits: i32,
    ) -> crate::pmove_saber_lock::LockOutcome {
        let _ = (partner, hits);
        self.move_npc(movement, command, context, pass, bodies);
        crate::pmove_saber_lock::LockOutcome::default()
    }
    /// [`Self::move_npc`] for a vehicle NPC: the move through the same world, with the
    /// game's vehicle functions (`game`) where `PmoveSingle` calls them
    /// ([`crate::pmove::Predictor::predict_vehicle_command`]); the players in `riders` —
    /// which the vehicle owns — are gone through, as `SV_ClipMoveToEntities` skips an
    /// entity's own. A host that never spawns vehicles may leave it to [`Self::move_npc`].
    #[allow(clippy::too_many_arguments)]
    fn move_vehicle(
        &mut self,
        movement: &mut crate::pmove::Predictor,
        command: sjk_protocol::UserCommand,
        context: &crate::pmove::MoveContext,
        pass: u16,
        bodies: &[crate::entity_clip::BoxObstacle],
        riders: &[u16],
        game: &mut dyn crate::pmove::vehicle::VehicleGame,
    ) {
        let _ = (game, riders);
        self.move_npc(movement, command, context, pass, bodies);
    }
    /// `G2API_AddBolt` on `models/players/<model>/model.glm`: the bolt's index, -1 where
    /// the model has none (a vehicle's exhausts, muzzles, droid unit). A host that does not
    /// read bolts answers -1.
    fn bolt(&mut self, _model: &[u8], _name: &str) -> i32 {
        -1
    }
    /// `G2API_GetBoltMatrix` on vehicle `number`'s model: bolt `tag` (from [`Self::bolt`])
    /// with the model at `origin` facing `angles`, its skeleton posed at `level_time`. A
    /// host without the model's skeleton answers the model's origin, unrotated
    /// ([`crate::vehicle_weapons::MuzzleBolt::unposed`]).
    fn vehicle_bolt(
        &mut self,
        _number: u16,
        _tag: i32,
        _angles: [f32; 3],
        origin: [f32; 3],
        _level_time: i32,
    ) -> crate::vehicle_weapons::MuzzleBolt {
        crate::vehicle_weapons::MuzzleBolt::unposed(origin)
    }
    /// `G2API_GetBoltMatrix` (the plain read, with its "90 degree offset" swap) on vehicle
    /// `number`'s model: where an exhaust's turbo flares, where a droid unit or a passenger
    /// sits. Unposed, like [`Self::vehicle_bolt`], without the model's skeleton.
    fn vehicle_tag(
        &mut self,
        _number: u16,
        _tag: i32,
        _angles: [f32; 3],
        origin: [f32; 3],
        _level_time: i32,
    ) -> crate::vehicle_weapons::MuzzleBolt {
        crate::vehicle_weapons::MuzzleBolt::unposed(origin)
    }
    /// Where vehicle `number`'s `*driver` tag is in its model's own frame (forward, left,
    /// up), which `AttachRidersGeneric` carries its pilot on: zero for a host without the
    /// model's skeleton (the oracle's fake Ghoul2 holds every bolt at the origin).
    fn vehicle_driver_offset(&mut self, _number: u16) -> [f32; 3] {
        [0.0; 3]
    }
    /// [`Self::trace`] past `pass` and what it owns (`SV_ClipMoveToEntities` skips an
    /// entity whose `r.ownerNum` is the one passed): a vehicle's `riders`.
    #[allow(clippy::too_many_arguments)]
    fn trace_past_riders(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        riders: &[u16],
        mask: u32,
        bodies: &[crate::entity_clip::BoxObstacle],
    ) -> MovementTrace {
        let _ = riders;
        self.trace(start, mins, maxs, end, pass, mask, bodies)
    }
    /// The C library's `rand()`, which a few of the game's choices draw from (its own
    /// generator, `Q_irand`, is [`Self::rng`]'s). A host without the library's sequence
    /// answers 0.
    fn crt_rand(&mut self) -> i32 {
        0
    }
    /// `g_cullDistance`: the level's `distanceCull` (6000 without one), as far as a
    /// walker's crosshair reaches.
    fn cull_distance(&self) -> f32 {
        6_000.0
    }
    /// What of the host's own entities a vehicle's move may bump into (brush movers,
    /// missiles, terrain), added to `bodies`; the roster adds the players and NPCs. None by
    /// default.
    fn impact_bodies(&mut self, bodies: &mut Vec<crate::pmove::vehicle_impact::ImpactBody>) {
        let _ = bodies;
    }
    /// Player `number` as the vehicle turret it works as a passenger reads it: its view and
    /// its command's buttons, while it lives (`VEH_TurretObeyPassengerControl`).
    fn gunner(&mut self, number: u16) -> Option<crate::vehicle_turrets::Gunner> {
        let _ = number;
        None
    }
    /// What a vehicle's turrets may aim at beyond the players and the NPCs: breakable
    /// brushes, `misc_turret`s (`VEH_TurretFindEnemies`), added to `targets`.
    fn turret_targets(&mut self, targets: &mut Vec<crate::vehicle_turrets::TurretTarget>) {
        let _ = targets;
    }
    /// `trap->SetBrushModel`'s box for inline model `name` (`*N`), where the map has it.
    fn brush_bounds(&mut self, name: &str) -> Option<([f32; 3], [f32; 3])> {
        let _ = name;
        None
    }
    /// `trap->LinkEntity` of a point spawned hidden: sent to the clients from now (the
    /// point a ship boundary turns ships toward, which a client's prediction reads).
    fn show(&mut self, number: u16) {
        let _ = number;
    }
    /// Player `client`'s `ps.stats[STAT_MAX_HEALTH]` (its handicap), which `G_Damage` scales
    /// a player attacker's blows by; 100 where the host does not know it.
    fn player_max_health(&self, client: u16) -> i32 {
        let _ = client;
        100
    }
    /// `bg_fighterAltControl` (a system-info cvar, 0 by default): player pilots fly
    /// fighters with free pitch and roll (`BG_UnrestrainedPitchRoll`).
    fn fighter_alt_control(&self) -> bool {
        false
    }
    /// A vehicle's projectile ([`crate::vehicle_weapons`]) put into the world: `G_Spawn`,
    /// then it flies with the other missiles. Its entity number, `None` where no entity is
    /// free.
    fn launch(&mut self, missile: crate::weapon_fire::Missile) -> Option<u16> {
        let _ = missile;
        None
    }
    /// The animation lengths of the skeleton `models/players/<model>/model.glm` animates
    /// with, and whether it is the humanoid one (`localAnimIndex == 0`).
    fn npc_animations(
        &mut self,
        model: &[u8],
    ) -> (
        Option<std::sync::Arc<dyn crate::pmove_anim::AnimationLengths>>,
        bool,
    );
    /// A temp entity carrying an NPC's event its entity could not (`SendPendingPredictableEvents`),
    /// not sent to the client numbered `not_for` (the NPC's own number, `SVF_NOTSINGLECLIENT`).
    fn raise_entity(&mut self, state: &EntityState, not_for: u16);
    /// A later step's function the thinking reached and did not run (`name`, for NPC
    /// `number`).
    fn stub(&mut self, _number: u16, _name: &str) {}
    /// Player `number`'s client as a Jedi NPC's AI reads another client
    /// ([`crate::npc_jedi_glue::JediClient`]); `None` when it is no playing client.
    fn jedi_player(&self, _number: u16) -> Option<crate::npc_jedi_glue::JediClient> {
        None
    }
    /// Entity `number`'s place, trajectory delta and weapon — a thrown saber or a missile a
    /// Jedi NPC watches (`r.currentOrigin`, `s.pos.trDelta`, `s.weapon`); `None` for one the
    /// host does not know.
    fn entity_motion(&self, _number: u16) -> Option<crate::npc_jedi_glue::EntityMotion> {
        None
    }
    /// `trap->EntitiesInBox(mins, maxs)` for what may come at an NPC
    /// (`WP_SaberStartMissileBlockCheck`): every missile and every player's thrown saber
    /// whose linked box meets the box, pushed onto `out`. None by default.
    fn incoming(
        &self,
        _mins: [f32; 3],
        _maxs: [f32; 3],
        _out: &mut Vec<crate::npc_missile_block::IncomingEntity>,
    ) {
    }

    /// `WP_SaberPositionUpdate`'s skeleton half for `npc` at its turn in the frame
    /// (`g_main.c:3342-3346`): a host that keeps the NPC's server-side model poses it
    /// ([`crate::npc_skeleton::NpcSkeleton::pose`]), reading its blades where
    /// [`crate::npc_saber::reads_blades`] says. `look_target` is where the entity the NPC
    /// looks at stands (`ps.hasLookTarget`). Animations are installed separately by
    /// [`Self::update_npc_anims`] after the blade sweeps. No blades by default.
    fn pose_npc(
        &mut self,
        _npc: &NpcActor,
        _look_target: Option<[f32; 3]>,
        _level_time: i32,
    ) -> crate::npc_skeleton::NpcBlades {
        [[None; crate::server_skeleton::MAX_BLADES]; 2]
    }
    /// `G_BoneIndex(name)` (`g_utils.c:122-124`): the bone's index among the level's
    /// `CS_G2BONES` configstrings, registered the first time. A host that names no bones
    /// answers 0 (no bone).
    fn bone_index(&mut self, _name: &[u8]) -> u16 {
        0
    }
    /// Whether `models/players/<model>/model.glm` (Kyle's where it is missing, as an NPC's
    /// instance is) has a bolt of this name — a surface bolt (`*r_hand`) or a bone
    /// (`jaw_bone`) — as `G2API_AddBolt` finds one. A host that keeps no models answers no.
    fn model_has_bolt(&mut self, _model: &[u8], _name: &str) -> bool {
        false
    }
    /// `G_GetBoltPosition(npc, index, pos, 0)` (`NPC_utils.c:1723-1756`): where bolt `bolt`
    /// (its instance's `index`) of `npc`'s posed model is, at its `r.currentOrigin` facing
    /// its view's yaw, the skeleton at the Ghoul2 clock. `None`, and a host that keeps no
    /// model, answer the origin, as `G2API_GetBoltMatrix` does for a bad bolt.
    fn npc_bolt(
        &mut self,
        npc: &NpcActor,
        _index: i32,
        _bolt: Option<&str>,
        _level_time: i32,
    ) -> [f32; 3] {
        npc.current_origin
    }
    /// `G2API_GetBoltMatrix(npc->ghoul2, 0, index, &matrix, angles, origin, level.time, NULL,
    /// modelScale)`: bolt `bolt` (its instance's `index`) of `npc`'s posed model placed at
    /// `origin` turned by all of `angles` — a machine's muzzle
    /// ([`crate::npc_skeleton::NpcSkeleton::bolt_matrix_turned`]). A host that keeps no model,
    /// and a bolt the model lacks, answer as the reference does for a bolt it lacks
    /// ([`crate::npc_machine_parts::unbolted_matrix`]).
    fn npc_bolt_matrix(
        &mut self,
        _npc: &NpcActor,
        _index: i32,
        _bolt: Option<&str>,
        angles: [f32; 3],
        origin: [f32; 3],
        _level_time: i32,
    ) -> [[f32; 4]; 3] {
        crate::npc_machine_parts::unbolted_matrix(angles, origin)
    }
    /// `gPainHitLoc` for a blow on `npc` (`g_combat.c:5417-5432`): when a blade or missile
    /// struck its model this frame (`g2LastSurfaceTime`), the machine part of that surface
    /// ([`crate::npc_machine_parts::surface_part`]). `None` where none was struck, for a
    /// class with no parts, and for a host that keeps no models.
    fn npc_struck_part(&mut self, _npc: &NpcActor, _level_time: i32) -> Option<i32> {
        None
    }
    /// `G_G2NPCAngles`' AT-ST head (`w_saber.c:869-874`): `yaw`, its view's, less its legs'
    /// trailing yaw — a local the reference never sets (`trailingLegsAngles`), so whatever
    /// its stack held. A server takes it as none: `yaw` itself.
    fn atst_head_yaw(&mut self, _npc: &NpcActor, yaw: f32) -> f32 {
        yaw
    }
    /// `CreateMissile` by NPC `shooter` (`g_missile.c:297-325`) with the class AI's own
    /// weapon, damage and means: the missile spawned and run from the next frame on — the
    /// host's, which owns the missiles. Reported as a stub where the host has none.
    fn launch_missile(&mut self, shooter: u16, _missile: crate::weapon_fire::Missile) {
        self.stub(shooter, "CreateMissile");
    }
    /// `G2API_GetSurfaceRenderStatus(ent->ghoul2, 0, name)` on client `number`'s instance (a
    /// player's or an NPC's): the surface's flags — zero where it is drawn — or -1 where its
    /// model lacks it. A host that keeps no models answers zero.
    fn surface_status(&mut self, _number: u16, _name: &str) -> i32 {
        0
    }
    /// `NPC_SetBoneAngles`' turn of NPC `number`'s server-side model (`NPC_utils.c:1010`): the
    /// bone `bone` turned by `angles` at `level_time` ([`crate::npc_skeleton::NpcSkeleton::set_bone_angles`]).
    fn set_npc_bone_angles(
        &mut self,
        _number: u16,
        _bone: &str,
        _angles: [f32; 3],
        _level_time: i32,
    ) {
    }
    /// An entity freed in its own think is bounced by `G_RunItem` all the same
    /// (`g_items.c:3259-3278`): the freed slot `number` is left `s.groundEntityNum` `ground`,
    /// which its next occupant keeps ([`crate::entity_pool::EntityPool::leave_ground_residue`]).
    fn freed_ground_residue(&mut self, _number: u16, _ground: u16) {}
    /// `trap->PointContents(point, -1)`: the contents of the world at `point`. A host with
    /// no world answers none.
    fn world_point_contents(&mut self, _point: [f32; 3]) -> u32 {
        0
    }
    /// `G2API_SetSurfaceOnOff(ent->ghoul2, name, flags)` on NPC `number`'s instance: a
    /// dismembered limb's surface hidden, its cap shown.
    fn set_npc_surface(&mut self, _number: u16, _name: &str, _flags: u32) {}
    /// `g_dismember`: the chance in a hundred that a killing or corpse-cutting blade blow
    /// takes a limb off ([`crate::npc_dismember_check`]). 0, a retail server's, cuts nothing.
    fn dismember_setting(&self) -> i32 {
        0
    }
    /// `G_HeavyMelee(attacker)` (`g_combat.c:52-63`): in a siege, the attacker plays a class
    /// with `CFL_HEAVYMELEE`, whose melee cuts as a blade does. None by default.
    fn heavy_melee(&self, _attacker: u16) -> bool {
        false
    }
    /// `G_UpdateClientAnims(npc, 1.0f)` (`g_client.c:2833`): `npc`'s server-side model given
    /// its current animations at `level_time`, after the blade sweeps and as `player_die`
    /// does before it reads a limb's bone. A host that keeps no models does nothing.
    fn update_npc_anims(&mut self, _npc: &NpcActor, _level_time: i32) {}
    /// `G2API_GetBoltMatrix(ent->ghoul2, 0, G2API_AddBolt(ent->ghoul2, 0, bone), ...)` of
    /// client `number`'s (a player's or an NPC's) posed model at `origin` turned by `angles`,
    /// at the Ghoul2 clock (`G_GetDismemberBolt`). `None` where the host keeps no model or
    /// the model lacks the bone.
    fn client_bolt_matrix(
        &mut self,
        _number: u16,
        _bone: &str,
        _angles: [f32; 3],
        _origin: [f32; 3],
    ) -> Option<[[f32; 4]; 3]> {
        None
    }
    /// The negated second axis of client `number`'s first hilt's first blade bolt
    /// (`G2API_GetBoltMatrix(ent->ghoul2, 1, 0, ...)`) at `origin` turned by `angles`: the
    /// blade's direction. `None` where it holds no hilt, or the host keeps no model.
    fn client_hilt_direction(
        &mut self,
        _number: u16,
        _angles: [f32; 3],
        _origin: [f32; 3],
    ) -> Option<[f32; 3]> {
        None
    }
    /// Player `player`'s blade readings (`lastSaberBase_Always`, `olderSaberBase`,
    /// `olderIsValid`, `lastSaberStorageTime`), which throw a limb it cuts along its sweep.
    /// `None` by default.
    fn player_saber_storage(&self, _player: u16) -> Option<crate::saber_clash::SaberStorage> {
        None
    }
    /// `G_GetHitLocFromSurfName` for player `player`: when a blade struck its model this
    /// frame, the body part of that surface; `None` leaves it to the box.
    fn player_surface_location(
        &mut self,
        _player: u16,
        _spot: [f32; 3],
        _level_time: i32,
    ) -> Option<crate::damage::HitLocation> {
        None
    }
    /// `BG_AttachToRancor` for client `victim` held by `rancor` (in its jaw when `in_mouth`,
    /// else its right hand): where the victim goes and how it faces, from the rancor's posed
    /// model at its `r.currentOrigin` facing its `r.currentAngles`' yaw
    /// ([`crate::npc_creature::attach_to_rancor`]). `None` where the host keeps no model.
    fn rancor_attach(
        &mut self,
        _rancor: &NpcActor,
        _victim: u16,
        _in_mouth: bool,
        _level_time: i32,
    ) -> Option<([f32; 3], [f32; 3])> {
        None
    }
    /// `G_G2TraceCollide` on `npc`'s posed model for a blade's trace from `start` to `end`
    /// with a box of `radius` ([`crate::npc_skeleton::NpcSkeleton::collide`]); its box
    /// alone where the host keeps no model.
    fn collide_npc(
        &mut self,
        _npc: &NpcActor,
        _start: [f32; 3],
        _end: [f32; 3],
        _radius: f32,
        _level_time: i32,
    ) -> crate::saber_damage::Ghoul2Answer {
        crate::saber_damage::Ghoul2Answer::NoModel
    }
    /// `g_debugSaberLocks`: any two blades that meet lock ([`crate::saber_lock::forced_lock`]).
    fn debug_saber_locks(&self) -> bool {
        false
    }
    /// Entity `number`'s wire state as the host keeps it (an NPC's saber entity's, which its
    /// flight writes). None by default.
    fn entity_state(&self, _number: u16) -> Option<&EntityState> {
        None
    }
    /// Player `number`'s `s.legsAnim`, which a move reads of whoever it meets
    /// (`PM_BGEntForNum`). None by default.
    fn client_legs(&self, _number: u16) -> Option<u16> {
        None
    }
    /// `d_projectileGhoul2Collision`: missiles are swept through the clients' posed models,
    /// and any blow on an NPC whose model was struck this frame is placed by the surface
    /// (`G_LocationBasedDamageModifier`, `g_combat.c:4306-4317`). Off by default, for the
    /// replays of drivers that set it 0.
    fn projectile_ghoul2_collision(&self) -> bool {
        false
    }
    /// `G_LocationBasedDamageModifier`'s surface half for a blade's blow on `npc`
    /// ([`crate::npc_skeleton::NpcSkeleton::surface_location`]).
    fn npc_surface_location(
        &mut self,
        _npc: &NpcActor,
        _flags: u32,
        _spot: [f32; 3],
        _level_time: i32,
    ) -> Option<crate::damage::HitLocation> {
        None
    }
    /// `G_G2TraceCollide` on player `player`'s posed model for an NPC's blade.
    fn collide_player(
        &mut self,
        _player: u16,
        _start: [f32; 3],
        _end: [f32; 3],
        _radius: f32,
        _level_time: i32,
    ) -> crate::saber_damage::Ghoul2Answer {
        crate::saber_damage::Ghoul2Answer::NoModel
    }
    /// What entity `player` — a player or a breakable, not an NPC — is to the blade of the
    /// NPC `swinger` (`CheckSaberDamage`,
    /// `w_saber.c:4530-4570`): whether it can be hurt, a power duel's teammate, duelling
    /// someone else, lying knocked down, its `playerTeam`.
    fn player_saber_victim(
        &self,
        _player: u16,
        _swinger: &crate::npc_senses::Body,
    ) -> Option<crate::saber_damage::SaberVictim> {
        None
    }
    /// The player whose lit saber entity is `saber_entity`, as a clash reads it.
    fn player_saber(&self, _saber_entity: u16) -> Option<crate::saber_clash::Fighter> {
        None
    }
    /// A clash's change to a player's saber (its move and its block), made by an NPC's
    /// blade.
    fn set_player_saber(&mut self, _owner: &crate::saber_clash::Fighter) {}
    /// The players' linked, lit saber entities, added to `out` for an NPC's blade to meet.
    fn player_saber_boxes(&self, _out: &mut Vec<crate::entity_clip::BoxObstacle>) {}
    /// `WP_SaberApplyDamage`'s `G_Damage` on player `player` by an NPC's blade, the blow
    /// placed by the surface struck. Returns what the NPC's hit counter gains
    /// (`PERS_HITS`, `PERS_ATTACKEE_ARMOR`).
    fn saber_blow_on_player(
        &mut self,
        _player: u16,
        _request: crate::damage::DamageRequest,
    ) -> (i32, Option<u32>) {
        (0, None)
    }
    /// `WP_SaberApplyDamage`'s `G_Damage` on an entity that is no client (a breakable), by
    /// an NPC's blade.
    fn saber_blow_on_entity(&mut self, _entity: u16, _request: crate::damage::DamageRequest) {}
    /// Entity `number`'s wire state taken out of the host's keeping (an NPC's saber entity,
    /// for its flight, [`crate::npc_saber_throw`]); `None` where the host keeps none.
    fn take_entity_state(&mut self, _number: u16) -> Option<EntityState> {
        None
    }
    /// Entity `number`'s wire state given back, sent to the clients or not (`SVF_NOCLIENT`).
    fn put_entity_state(&mut self, _number: u16, _state: EntityState, _shown: bool) {}
    /// Entity `number` raised an event of its own at `event_time` (a knocked saber's bounce),
    /// which the host clears it of in time. Nothing by default.
    fn entity_event(&mut self, _number: u16, _event_time: i32) {}
    /// A thrown NPC saber reaching player `player` at `point`: `WP_SaberCanBlock(ent, point,
    /// 0, MOD_SABER, qfalse, 999)` and `WP_SaberBlockNonRandom` — whether it blocked. None by
    /// default.
    fn player_blocks_thrown(&mut self, _player: u16, _point: [f32; 3]) -> bool {
        false
    }
    /// Player `player`'s `FP_SABER_DEFENSE` level, for a thrown NPC saber it blocks
    /// (`saberCheckKnockdown_Thrown`). None by default.
    fn player_saber_defense(&self, _player: u16) -> Option<u8> {
        None
    }
    /// `saberKnockOutOfHand` for player `player`'s saber, knocked by an NPC's blade at
    /// `velocity`: whether it flew. Never by default.
    fn knock_player_saber(&mut self, _player: u16, _velocity: [f32; 3]) -> bool {
        false
    }
    /// `WP_Explode`'s `G_RadiusDamage` for NPC `npc` (`g_weapon.c:383-386`): `damage`
    /// within `radius` of `at` (`MOD_UNKNOWN`, the attacker `attacker` not spared) on the
    /// players and the NPCs. Returns what the attacker's hit counter gains of the players
    /// struck at once (`PERS_HITS`, `PERS_ATTACKEE_ARMOR`). Nothing by default.
    fn explode(
        &mut self,
        _npc: u16,
        _at: [f32; 3],
        _damage: i32,
        _radius: f32,
        _attacker: crate::damage::Attacker,
    ) -> (i32, Option<u32>) {
        (0, None)
    }
    /// Player `number` as a saber's wall-bounce splash reads it (`WP_SaberRadiusDamage`):
    /// whether a monster holds it (`EF2_HELD_BY_MONSTER`) and whether it stands on
    /// something. `None` by default: the splash then neither spares nor knocks it down.
    fn player_splash_state(&self, _number: u16) -> Option<(bool, bool)> {
        None
    }
    /// The breakable entities (`G_EntIsBreakable`) whose linked boxes meet `mins`..`maxs`,
    /// appended to `out`. None by default.
    fn breakables_in_box(&self, _mins: [f32; 3], _maxs: [f32; 3], _out: &mut Vec<u16>) {}
    /// Whether entity `number` is a breakable, and whether it takes damage; `None` for no
    /// breakable.
    fn breakable_takes_damage(&self, _number: u16) -> Option<bool> {
        None
    }
    /// `G_Damage(brush, npc, npc, vec3_origin, origin, damage, 0, MOD_MELEE)` on breakable
    /// `number` by NPC `attacker`. Nothing by default.
    fn hurt_breakable(&mut self, _number: u16, _damage: i32, _attacker: u16) {}
    /// `saberCheckKnockdown_Smashed` on player `player`'s thrown saber, struck by NPC
    /// `striker`'s blade (`defending`: in an extra defence move) for `damage`: whether it
    /// was knocked out of the air. Nothing by default.
    fn smash_player_saber(
        &mut self,
        _player: u16,
        _striker: u16,
        _defending: bool,
        _damage: i32,
    ) -> bool {
        false
    }
    /// `G_TouchTriggers` of NPC `npc` against the map's triggers the host keeps (its hurt,
    /// push, teleport, `trigger_multiple` and door brushes, [`crate::npc_triggers`]), as it
    /// stands after its move; run only for a living NPC not in noclip. Nothing by default.
    fn touch_map_triggers(&mut self, _npc: &NpcActor) {}
    /// A lock an NPC won pushed player `player` down (`forceHandExtendTime`) or credited the
    /// NPC with it (`otherKiller`, its time and debounce). Nothing by default.
    fn player_locked_down(
        &mut self,
        _player: u16,
        _until: Option<i32>,
        _other_killer: Option<(u16, i32, i32)>,
    ) {
    }
    /// `pmove.checkDuelLoss` for player `player`, who lost a lock to an NPC (`attacker`,
    /// standing at `origin`, its blades' readings `storage`, its disarm chance `chance`):
    /// finished outright on a weak draw, else its saber may fly
    /// (`saberCheckKnockdown_DuelLoss`). Nothing by default.
    fn player_lost_lock(
        &mut self,
        _player: u16,
        _attacker: crate::damage::Attacker,
        _origin: [f32; 3],
        _storage: &crate::saber_clash::SaberStorage,
        _chance: i32,
        _level_time: i32,
    ) {
    }
    /// `G_TouchTriggers` of an NPC's box (`origin`, `bounds`) against the players' knocked
    /// sabers: each one touched stands upright (`SaberBounceSound`). Nothing by default.
    fn touch_player_sabers(&mut self, _origin: [f32; 3], _bounds: ([f32; 3], [f32; 3])) {}
    /// `WP_SabersCheckLock` between `npc` and player `player` (`w_saber.c:1459`), `npc`
    /// first where `npc_first`: whether they locked, both begun
    /// ([`crate::npc_saber_lock::npc_lock_fighter`], [`crate::npc_saber_lock::npc_locked`]).
    /// `bodies` holds the NPCs the traces meet. Never by default.
    fn player_lock(
        &mut self,
        _npc: &mut NpcActor,
        _player: u16,
        _npc_first: bool,
        _bodies: &mut Vec<crate::entity_clip::BoxObstacle>,
        _level_time: i32,
    ) -> bool {
        false
    }
    /// `NPC_Touch` ran for NPC `npc`, touched by `toucher`: for a host that records it.
    fn noting_touch(&mut self, _npc: u16, _toucher: u16) {}
    /// A `G_Damage` the roster deals an NPC itself (a blade's blow, a telefrag), `inflictor`
    /// dealing it: for a host that records what `G_Damage` is asked. Nothing by default.
    fn noting_damage(
        &mut self,
        _target: u16,
        _inflictor: u16,
        _request: &crate::damage::DamageRequest,
    ) {
    }
    /// The players as the Force powers used in the NPCs' world reach them, and the entity
    /// pool the powers' sounds and events become ([`crate::npc_force_update::PlayerForce`]).
    /// `None` (the default) for a host that runs no NPC's Force: its NPCs use none.
    fn player_force(&mut self) -> Option<&mut dyn crate::npc_force_update::PlayerForce> {
        None
    }
    /// `ClientEvents`' `FireWeapon(npc, alternate)` (`g_weapon.c:4490-4640`): the NPC's
    /// weapon fired from `entity` as its move converted it, its missiles spawned — the
    /// host's, which owns the missiles. Reported as a stub where the host has none.
    fn fire_weapon(
        &mut self,
        shooter: u16,
        _player: &mut PlayerState,
        _entity: &EntityState,
        _alternate: bool,
    ) {
        self.stub(shooter, "FireWeapon");
    }
    /// `G_FreeEntity` on an entity with a Ghoul2 model: freed, and named for the clients
    /// to drop its model (`G_KillG2Queue`).
    fn free_model(&mut self, number: u16) {
        self.free(number);
    }
    /// `player_die`'s static counter: which of `EV_DEATH1..3` the next death raises,
    /// shared with the players' deaths ([`crate::player_death::Deaths`]).
    fn death_counter(&mut self) -> &mut u8;
    /// A client's `pers.netname`, for the log.
    fn client_name(&self, _number: u16) -> Vec<u8> {
        Vec::new()
    }
    /// `G_LogPrintf`: a line of the game log (a dedicated server's console too).
    fn log(&mut self, text: &str) {
        self.print(text);
    }
    /// `AddScore` on a player (its team's in a team game, the ranks).
    fn add_player_score(&mut self, _player: u16, _points: i32) {}
    /// A player killed an NPC (`player_die`, `g_combat.c:2600-2622`): the stun baton's count,
    /// the excellent award by its last kill's time, and the time of this one.
    fn credit_kill(&mut self, _player: u16, _means: u32) {}
    /// `AddScore` on an NPC, beyond its own score: its team's in a team game (by the
    /// player team it counts as), and the ranks.
    fn npc_scored(&mut self, _team: i32, _points: i32) {}
    /// In a Jedi Master game, the master's client number, if anyone is the master.
    fn jedi_master(&self) -> Option<u16> {
        None
    }
    /// `level.warmupTime`: no scoring during the warm-up.
    fn warmup(&self) -> bool {
        false
    }
    /// `level.intermissiontime`: nobody dies at the intermission.
    fn intermission(&self) -> bool {
        false
    }
    /// `G_ClearEnemy(player)` and `player->enemy = enemy` (`NPC_CheckAttacker`): whom a
    /// player is taken to be fighting.
    fn set_player_enemy(&mut self, _player: u16, _enemy: Option<u16>) {}
    /// `other->flags &= ~FL_NOTARGET` for a player an ally turns on.
    fn clear_player_notarget(&mut self, _player: u16) {}
    /// The humanoid skeleton's animation lengths (`bgHumanoidAnimations`), whose frame
    /// times a pain animation's length is taken at.
    fn humanoid_animations(
        &mut self,
    ) -> Option<std::sync::Arc<dyn crate::pmove_anim::AnimationLengths>> {
        None
    }
    /// `npc kill team nonally` on a player: its health to zero and `player_die` by itself
    /// (`MOD_UNKNOWN`).
    fn kill_player(&mut self, _player: u16) {}
    /// `level.maxclients`: the client slots `npc score` lists.
    fn client_slots(&self) -> u16 {
        0
    }
    /// A client slot's `PERS_SCORE` (zero for an empty one).
    fn player_score(&self, _client: u16) -> i32 {
        0
    }
    /// The lowest-numbered entity in use that is neither an NPC nor a spawner and has the
    /// `targetname` `name` (`G_Find`).
    fn named_entity(&self, _name: &[u8]) -> Option<u16> {
        None
    }
    /// The `index`-th item entity in use, in entity-number order, as an NPC's weapon search
    /// and touch read it ([`crate::npc_weapon_pickup::NpcItem`]); `None` past the last. A
    /// host without items answers `None`.
    fn npc_item(&self, _index: usize) -> Option<crate::npc_weapon_pickup::NpcItem> {
        None
    }
    /// `Touch_Item`'s own refusal of NPC `toucher` (`g_items.c:2401-2424`): a tossed weapon
    /// its thrower cannot take back yet (its thrower forgotten once the time is up).
    fn npc_item_refuses(&mut self, _number: u16, _toucher: u16, _level_time: i32) -> bool {
        false
    }
    /// `Touch_Item`'s end for item `number` taken by NPC `toucher` (`g_items.c:2617-2681`):
    /// its targets fired, and the item gone — a dropped one freed, a placed one back in
    /// `respawn` seconds (or never).
    fn npc_took_item(&mut self, _number: u16, _toucher: u16, _respawn: i32, _level_time: i32) {}
    /// Player `number`'s `client->buttons` (the command it last thought with), which an NPC
    /// ducking its fire reads; none by default.
    fn client_buttons(&self, _number: u16) -> u16 {
        0
    }
    /// `Drop_Item`'s entity (`LaunchItem`, `g_items.c:2693-2768`) for an item NPC `dropper`
    /// lets fall (`TossClientItems` on the rancor, [`crate::dropped_items`]): `G_Spawn`, then
    /// it falls and lies with the host's other items. Its entity number, `None` where the
    /// host keeps no items (reported as a stub) or no entity is free.
    fn drop_item(&mut self, dropper: u16, _pickup: crate::items::Pickup) -> Option<u16> {
        self.stub(dropper, "LaunchItem");
        None
    }
}

/// Why a spawner made no NPC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoNpc {
    /// No entity was left (`G_Spawn` failed): the spawner stays.
    OutOfEntities,
    /// The definition was refused: the entity and the spawner are freed.
    Refused(NpcRefusalReason),
}

/// What `NPC_Spawn_Do` did.
#[derive(Debug)]
pub struct Spawned {
    /// The NPC, placed and waiting for its begin.
    pub npc: Result<NpcActor, NoNpc>,
    /// Whether the spawner is freed (its count spent, or its NPC refused).
    pub spawner_freed: bool,
    /// The spawner's `target`, fired as it went (`G_UseTargets`).
    pub fired: Option<Vec<u8>>,
}

/// `NPC_Spawn_Do(spawner)` at `level_time`. The spawner's count is spent and its type
/// lower-cased in place, as the reference writes them on the spawner. An `NPC_Vehicle`
/// spawner's NPC is a vehicle of the level's table `vehicles` ([`crate::vehicle_spawn`]).
/// The caller links what is published and frees the spawner when told.
pub fn spawn_do(
    spawner: &mut NpcSpawner,
    parms: &NpcParms,
    sabers: &SaberParms,
    vehicles: Option<&mut crate::vehicle_parms::VehicleTable>,
    level_time: i32,
    host: &mut impl NpcHost,
) -> Spawned {
    // `NSF_DROP_TO_FLOOR` moves the spawner's `r.currentOrigin` for the spawn and back
    // after it (`NPC_spawn.c:1374-1387`, `1738-1742`); the NPC copies `s.origin`, which
    // `G_SetOrigin` never touches, so the drop changes nothing. It is not traced here.
    if spawner.count != -1 {
        spawner.count -= 1;
        if spawner.count <= 0 {
            spawner.usable = false;
        }
    }
    let Some(number) = host.spawn_entity() else {
        host.print("^1ERROR: NPC G_Spawn failed\n");
        return Spawned {
            npc: Err(NoNpc::OutOfEntities),
            spawner_freed: false,
            fired: None,
        };
    };
    let Some(goal) = host.spawn_hidden() else {
        // The reference leaves the entity with no NPC at all; here it is given back.
        host.free(number);
        host.print("^1ERROR: NPC G_Spawn failed\n");
        return Spawned {
            npc: Err(NoNpc::OutOfEntities),
            spawner_freed: false,
            fired: None,
        };
    };
    let npc_type = match &spawner.npc_type {
        None => b"random".to_vec(),
        Some(name) => crate::npc_parms::new_string(name).to_ascii_lowercase(),
    };
    if spawner.npc_type.is_some() {
        spawner.npc_type = Some(npc_type.clone());
    }
    // An `NPC_Vehicle` spawner's vehicle, made before the parse (`NPC_spawn.c:1478-1520`);
    // one not defined frees the NPC and the spawner.
    let vehicle = if spawner.vehicle {
        let Some(vehicle) =
            vehicles.and_then(|table| crate::vehicle_spawn::create(&npc_type, table, host))
        else {
            host.free(number);
            return Spawned {
                npc: Err(NoNpc::Refused(NpcRefusalReason::NotFound)),
                spawner_freed: true,
                fired: None,
            };
        };
        Some(vehicle)
    } else {
        None
    };
    let spawn = NpcSpawn {
        vehicle: vehicle.as_ref().map(crate::vehicle_spawn::parse_kind),
        origin: spawner.origin,
        sound_flags: spawner.sound_flags,
    };
    let definition = match parms.parse(&npc_type, &spawn, sabers, host) {
        Ok(definition) => definition,
        Err(refusal) => {
            for model in &refusal.registered_models {
                host.model_index(model);
            }
            match refusal.reason {
                NpcRefusalReason::Random => host.print("RANDOM NPC NOT SUPPORTED IN MP\n"),
                NpcRefusalReason::VehicleWithoutVehicle => host.print(&format!(
                    "^1ERROR: Tried to spawn a vehicle NPC ({}) without using NPC_Vehicle or 'NPC spawn vehicle <vehiclename>'!!!  Bad, bad, bad!  Shame on you!\n",
                    String::from_utf8_lossy(&npc_type)
                )),
                NpcRefusalReason::Md3Model => host.print("MD3 MODEL NPC'S ARE NOT SUPPORTED IN MP!\n"),
                _ => {}
            }
            host.print(&format!(
                "^1ERROR: Couldn't spawn NPC {}\n",
                String::from_utf8_lossy(&npc_type)
            ));
            // The goal entity stays allocated, as the reference leaves it
            // (`NPC_spawn.c:1573-1580` frees only the NPC and the spawner).
            host.free(number);
            return Spawned {
                npc: Err(NoNpc::Refused(refusal.reason)),
                spawner_freed: true,
                fired: None,
            };
        }
    };
    // A vehicle's Ghoul2 model is its definition's (`BG_GetVehicleModelName`).
    let model = vehicle.as_ref().map_or_else(
        || definition.player_model.clone(),
        |vehicle| vehicle.info.model.clone().unwrap_or_default(),
    );
    let state = spawned_state(spawner, &definition, &model, host, level_time);
    let test = npc_type.eq_ignore_ascii_case(b"test");
    let mut npc = actor(
        spawner, number, goal, npc_type, definition, state, level_time,
    );
    // `SetupGameGhoul2Model` gives the spawn its skeleton (`localAnimIndex`), which what runs
    // before the NPC begins already reads (a creature's look target, `w_saber.c:5780`).
    npc.humanoid = host.npc_animations(&model).1;
    if let Some(vehicle) = vehicle {
        crate::vehicle_spawn::initialize(&mut npc, vehicle, spawner, host);
    }
    if test {
        // Client 0's place and team, and nothing but waiting.
        let (origin, team) = host.client_zero();
        for axis in 0..3 {
            npc.state
                .set_raw_field(es::ORIGIN[axis], origin[axis].to_bits());
        }
        npc.player_team = team;
        (npc.default_behavior, npc.behavior_state) = (BS_WAIT, BS_WAIT);
    }
    // `newent->s.teamowner = ent->s.teamowner`: the spawner's, over the definition's.
    npc.state
        .set_raw_field(es::TEAM_OWNER, spawner.team_owner as u32);
    npc.session_team = session_team(spawner);
    npc.player.persistent[PERS_TEAM] = npc.session_team as u32;
    host.publish(number, &npc.state, npc.bounds(), npc.contents);
    let mut fired = None;
    if !spawner.usable {
        // The last NPC: the spawner's target fired, its `closetarget` the NPC's death
        // target, and the spawner gone.
        fired = spawner.target.clone();
        if spawner.close_target.is_some() {
            npc.target = spawner.close_target.clone();
        }
    }
    Spawned {
        npc: Ok(npc),
        spawner_freed: !spawner.usable,
        fired,
    }
}

/// `sess.sessionTeam` (`NPC_spawn.c:1709-1729`): the spawner's `team` key, else the first
/// of its team owner, its allied team and the team it takes no damage from that is set.
fn session_team(spawner: &NpcSpawner) -> i32 {
    match &spawner.team {
        Some(team) if !team.is_empty() => crate::userinfo::atoi(team),
        _ => [
            spawner.team_owner,
            spawner.allied_team,
            spawner.team_no_damage,
        ]
        .into_iter()
        .find(|&team| team != 0)
        .unwrap_or(0),
    }
}

/// The wire state the spawn leaves: the definition's model, sabers, class and sound sets
/// registered and written, the origin and angles interpolated from the spawn time, an
/// `ET_NPC` not drawn yet.
fn spawned_state(
    spawner: &NpcSpawner,
    definition: &NpcDefinition,
    model: &[u8],
    host: &mut impl NpcHost,
    level_time: i32,
) -> EntityState {
    let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    let mut set = |index: usize, value: u32| {
        state.set_raw_field(index, value);
    };
    // The models in the parse's order; the Ghoul2 setup's only where the model loads
    // (`SetupGameGhoul2Model`, `g_client.c:1740-1767`), the default one's first.
    let loads = (definition.rigid_model || host.model_loads(b"kyle")) && host.model_loads(model);
    for (at, model) in definition.registered_models.iter().enumerate() {
        if at != definition.setup_model_at || loads {
            host.model_index(model);
        }
    }
    if loads {
        set(
            es::MODEL_INDEX,
            u32::from(host.model_index(&definition.model)),
        );
        set(es::MODEL_GHOUL2, u32::from(!definition.rigid_model));
    }
    for (index, saber) in [es::NPC_SABER1, es::NPC_SABER2]
        .into_iter()
        .zip(&definition.saber_models)
    {
        if let Some(name) = saber {
            set(index, u32::from(host.model_index(name)));
        }
    }
    for name in &definition.registered_sounds {
        host.sound_index(name);
    }
    for weapon in &definition.registered_weapons {
        if let Some(item) = crate::npc_spawners::Registration::Weapon(*weapon).item() {
            host.register_item(item);
        }
    }
    let sounds = &definition.sounds;
    for (index, name) in es::SOUNDS.into_iter().zip([
        &sounds.standard,
        &sounds.combat,
        &sounds.extra,
        &sounds.jedi,
    ]) {
        if let Some(name) = name {
            set(index, u32::from(host.sound_index(name)));
        }
    }
    set(es::BOLT_TO_PLAYER, definition.bolt_to_player as u32);
    set(es::NPC_CLASS, definition.entity_class as u32);
    set(es::TYPE, ET_NPC);
    set(es::EFLAGS, EF_NODRAW);
    set(es::SHOULD_TARGET, u32::from(spawner.shows_health));
    for axis in 0..3 {
        set(es::ORIGIN[axis], definition.origin[axis].to_bits());
        set(es::ANGLES[axis], spawner.angles[axis].to_bits());
        set(es::POS_BASE[axis], definition.origin[axis].to_bits());
        set(es::APOS_BASE[axis], spawner.angles[axis].to_bits());
    }
    set(es::POS_TYPE, TR_INTERPOLATE);
    set(es::POS_TIME, level_time as u32);
    set(es::APOS_TYPE, TR_INTERPOLATE);
    set(es::APOS_TIME, level_time as u32);
    state
}

/// The actor as the spawn leaves it.
fn actor(
    spawner: &NpcSpawner,
    number: u16,
    goal: u16,
    npc_type: Vec<u8>,
    definition: NpcDefinition,
    state: EntityState,
    level_time: i32,
) -> NpcActor {
    let mut player = PlayerState::zero();
    player.set_origin(definition.origin);
    player.set_view_angles(spawner.angles);
    if let Some(weapon) = definition.weapon {
        player.set_raw_field(crate::npc_begin::ps::WEAPON, weapon as u32);
    }
    player.stats[crate::npc_begin::STAT_WEAPONS] = definition.weapons;
    for index in 0..crate::weapon_data::LEGACY_WEAPON_COUNT {
        if definition.ammo_filled & (1 << index) != 0 {
            player.ammo[index] = 100;
        }
    }
    // The rest of what the parse wrote on the player state.
    use crate::npc_begin::ps;
    if definition.flying {
        player.set_raw_field(ps::EFLAGS2, 1 << 4);
    }
    for (index, value) in ps::CUSTOM_RGBA.into_iter().zip(definition.custom_rgba) {
        player.set_raw_field(index, value as u32);
    }
    if let Some(scale) = definition.scale {
        player.set_raw_field(ps::MODEL_SCALE, scale.percent as u32);
    }
    player.set_raw_field(ps::STAND_HEIGHT, definition.stand_height as u32);
    player.set_raw_field(ps::CROUCH_HEIGHT, definition.crouch_height as u32);
    if let Some(style) = definition.saber_style {
        player.set_raw_field(ps::SABER_ANIM_LEVEL, style as u32);
    }
    let force_levels: [i32; crate::npc_parms::NPC_FORCE_POWERS] =
        std::array::from_fn(|power| definition.force_levels[power].unwrap_or(0));
    player.set_raw_field(
        crate::npc_begin::ps::FORCE_KNOWN,
        definition.force_powers_known(0),
    );
    // The two levels the wire carries (`fd.forcePowerLevel[FP_LEVITATION]`,
    // `[FP_SEE]`), as `NPC_ParseParms` sets every level.
    player.set_raw_field(
        PS_LEVITATION_LEVEL,
        force_levels[crate::force_powers::FP_LEVITATION] as u32,
    );
    player.set_raw_field(
        PS_SEE_LEVEL,
        force_levels[crate::force_powers::FP_SEE] as u32,
    );
    let mut flags = FL_NOTARGET;
    if spawner.message.is_some() {
        flags |= FL_NO_KNOCKBACK;
    }
    let ai_flags = if npc_type.eq_ignore_ascii_case(b"kyle") {
        NPCAI_MATCHPLAYERWEAPON
    } else {
        0
    };
    NpcActor {
        number,
        goal: Some(goal),
        saber_entity: None,
        saber: crate::npc_saber::NpcSaber::default(),
        takes_damage: false,
        link: (definition.origin, definition.origin),
        mins: definition.mins,
        maxs: definition.maxs,
        current_origin: definition.origin,
        player_team: definition.player_team.unwrap_or(0),
        enemy_team: definition.enemy_team.unwrap_or(0),
        session_team: 0,
        behavior_state: 0,
        default_behavior: definition.default_behavior,
        script_flags: DEFAULT_SCRIPT_FLAGS,
        ai_flags,
        desired_yaw: spawner.angles[1],
        count: 0,
        force_levels,
        force: crate::npc_force::cleared(&force_levels, definition.force_power_max.unwrap_or(0)),
        health: spawner.health,
        max_health: 0,
        bar_max_health: 0,
        flags,
        server_flags: spawner.sound_flags,
        spawnflags: spawner.spawnflags,
        wait: spawner.wait,
        contents: 0,
        clip_mask: 0,
        targetname: spawner.npc_targetname.clone(),
        target: spawner.npc_target.clone(),
        target2: spawner.target2.clone(),
        target3: spawner.target3.clone(),
        target4: spawner.target4.clone(),
        pain_target: spawner.pain_target.clone(),
        open_target: spawner.open_target.clone(),
        message: spawner.message.clone(),
        full_name: (!spawner.full_name.is_empty()).then(|| spawner.full_name.clone()),
        spawn_script: spawner.spawn_script.clone(),
        allied_team: spawner.allied_team,
        team_no_damage: spawner.team_no_damage,
        think: NpcThink::Begin(level_time + FRAMETIME),
        spawn_time: level_time,
        // `G_SetAngles`: the spawner's (`r.currentAngles`).
        mind: crate::npc_mind::NpcMind {
            current_angles: spawner.angles,
            ..Default::default()
        },
        movement: crate::npc_client_think::fresh_movement(&player),
        humanoid: true,
        vehicle: None,
        npc_type,
        definition,
        state,
        player,
    }
}

/// `G_SetOrigin`: the entity stands at `origin` (`TR_STATIONARY`).
pub(crate) fn set_origin(npc: &mut NpcActor, origin: [f32; 3]) {
    for axis in 0..3 {
        npc.state
            .set_raw_field(es::POS_BASE[axis], origin[axis].to_bits());
        npc.state.set_raw_field(es::POS_DELTA[axis], 0);
    }
    npc.state.set_raw_field(es::POS_TYPE, TR_STATIONARY);
    npc.state.set_raw_field(es::POS_TIME, 0);
    npc.state.set_raw_field(es::POS_DURATION, 0);
    npc.current_origin = origin;
}

/// `NPC_SpawnType`'s placement for `npc spawn` (`NPC_spawn.c:3985-3999`): 64 units ahead of
/// the player's origin along its view (`MASK_SOLID`, a point), 24 down from there, and 24
/// back up; facing the player's own yaw.
pub fn command_place(
    origin: [f32; 3],
    view: [f32; 3],
    trace: &mut impl FnMut([f32; 3], [f32; 3]) -> MovementTrace,
) -> ([f32; 3], f32) {
    let mut forward = crate::pmove::flight::flight_axes(view).0.to_array();
    crate::player_angle_math::normalize(&mut forward);
    let ahead: [f32; 3] = std::array::from_fn(|axis| origin[axis] + 64.0 * forward[axis]);
    let first = trace(origin, ahead);
    let mut end = first.end_position;
    end[2] -= 24.0;
    let second = trace(first.end_position, end);
    let mut end = second.end_position;
    end[2] += 24.0;
    (end, view[1])
}
