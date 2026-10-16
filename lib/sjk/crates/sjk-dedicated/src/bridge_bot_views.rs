//! What a bot sees of this server as it thinks (`StandardBotAI`'s reads of
//! `g_entities`, `level.clients`, `trap->Trace` and `trap->InPVS`): the clients, the
//! entities that may be dangers or targets, the world with the players standing in it,
//! and the bot's own state. Gathered once a bot frame into buffers kept to be reused;
//! see [`sjk_game_jka::bot_standard`].
//!
//! The dropped flags and the Jedi Master's saber are read as a bot thinks.
//!
//! The siege objectives are the chain ends the routes were tied to as the level
//! started ([`NativeGame::siege_things`]).
//!
//! A player's noises are heard as the reference keeps them: its last step
//! (`footstepTime`) and the last Force power it began (`otherSoundTime`,
//! `otherSoundLen`).
//!
//! Not yet seen here: movers and shields to the lift and shield checks (their brushes
//! are in no trace of this server's yet, the players' own included); and sentries (not
//! spawned).

use super::*;
use crate::collision::{Void, WithPlayers, WorldCollision};
use sjk_game_jka::bot_ctf::{CtfClient, LyingFlag};
use sjk_game_jka::bot_moves::BotMoveWorld;
use sjk_game_jka::bot_objectives::{GoalThing, ObjectiveClient};
use sjk_game_jka::bot_senses::{
    BotSenses, EventTracker, SensedClient, SensedThing, update_event_tracker,
};
use sjk_game_jka::bot_squad::SquadClient;
use sjk_game_jka::bot_standard::BotSelf;
use sjk_game_jka::ctf::{PW_BLUEFLAG, PW_REDFLAG};
use sjk_game_jka::mines::Kind;
use sjk_game_jka::pmove::MovementTrace;

/// `CON_CONNECTING`, `CON_CONNECTED`.
const CON_CONNECTING: i32 = 1;
const CON_CONNECTED: i32 = 2;
/// `fd.forceMindtrickTargetIndex` 1 to 4 on the wire.
const PS_MINDTRICK: [usize; 4] = [98, 99, 101, 104];
/// `PMF_JUMP_HELD`.
const PMF_JUMP_HELD: u16 = 2;
/// `ENTITYNUM_NONE`.
const ENTITY_NONE: u16 = 1_023;
/// `MASK_SOLID`.
const MASK_SOLID: u32 = 0x1;

/// The bot frame's views, kept to be reused.
#[derive(Clone, Debug, Default)]
pub(super) struct BotViews {
    /// Every player's place as the senses read it (`None` for an empty one).
    pub(super) clients: Vec<Option<SensedClient>>,
    /// The entities that are no clients: missiles and charges.
    pub(super) things: Vec<SensedThing>,
    /// The clients as the squads, the objectives and the flag game read them.
    pub(super) squad: Vec<Option<SquadClient>>,
    pub(super) objective: Vec<Option<ObjectiveClient>>,
    pub(super) ctf: Vec<Option<CtfClient>>,
    /// Each bot's `jumpTime`, which another bot's jump check reads.
    pub(super) jump_times: Vec<Option<f32>>,
    /// `gBotEventTracker`: each client's last events.
    pub(super) trackers: Vec<EventTracker>,
    /// The siege objectives' chain ends (`shootGoal`, `touchGoal`, the objectives' points).
    pub(super) siege_things: Vec<GoalThing>,
}

/// One client as the senses read it, written over the slot's last reading (its name's
/// storage reused).
fn sense_client(peer: &Peer, out: &mut SensedClient) {
    let state = &peer.state;
    out.in_use = true;
    out.bot = peer.bot;
    out.connected = if peer.begun {
        CON_CONNECTED
    } else {
        CON_CONNECTING
    };
    out.team = peer.session.team;
    out.duel_team = peer.session.duel_team;
    out.health = peer.health;
    out.takes_damage = peer.body_active();
    out.pm_type = i32::from(peer.movement.state().movement_type);
    // `SV_LinkEntity` packs a box only for a solid or a body.
    out.solid = peer.begun && peer.body_active() && peer.corpse.is_none();
    out.origin = state.origin();
    out.duel_in_progress = state.duel_in_progress();
    out.duel_index = i32::from(state.duel_index());
    out.is_jedi_master = state.is_jedi_master();
    out.mind_tricked = PS_MINDTRICK.map(|field| state.raw_field(field).unwrap_or(0) as i32);
    out.danger_time = peer.force.danger_time;
    out.other_sound_time = peer.force.other_sound_time;
    out.other_sound_len = peer.force.other_sound_len;
    out.footstep_time = peer.movement.state().footstep_time;
    out.netname.clone_from(&peer.name);
    out.weapon = i32::from(state.weapon());
    out.red_flag = state.powerups[PW_REDFLAG] != 0;
    out.blue_flag = state.powerups[PW_BLUEFLAG] != 0;
    out.viewheight = state.view_height();
    out.velocity = state.velocity();
    // A player's entity moves as its state does (`BG_PlayerStateToEntityState`).
    out.moving = out.velocity != [0.0; 3];
    out.force_power = i32::from(state.force_power());
    out.force_side = i32::from(state.force_side());
    out.force_powers_active = state.force_powers_active() as i32;
    out.saber_holstered = state.saber_holstered() != 0;
    out.duel_time = state.duel_time();
    out.on_ground = state.ground_entity_num() != ENTITY_NONE;
    out.saber_move = state.saber_move();
}

/// A missile or a charge as a danger.
fn sense_missile(number: u16, missile: &Missile, health: i32, takes_damage: bool) -> SensedThing {
    let base = ES_POS_BASE.map(|field| f32::from_bits(missile.state.raw_field(field).unwrap_or(0)));
    SensedThing {
        number: i32::from(number),
        in_use: true,
        damage: missile.damage,
        splash_damage: missile.splash_damage,
        weapon: missile.state.raw_field(ES_WEAPON).unwrap_or(0) as i32,
        generic5: 0,
        generic3: 0,
        health,
        owner: i32::from(missile.owner),
        current_origin: missile.current,
        base,
        origin: base,
        takes_damage,
    }
}

impl NativeGame {
    /// The views a bot frame reads, gathered before any bot thinks (the bots of one frame
    /// all see the world as it was).
    pub(super) fn gather_bot_views(&mut self) {
        let Self {
            server,
            world,
            players,
            missiles,
            charges,
            bots,
            ..
        } = self;
        let views = &mut bots.views;
        let count = players.places();
        views.clients.resize(count, None);
        views.squad.resize(count, None);
        views.objective.resize(count, None);
        views.ctf.resize(count, None);
        let world = server.world(*world);
        for (slot, handle) in players.holders().enumerate() {
            let peer = handle.and_then(|handle| world.and_then(|world| world.entity(handle)));
            let Some(peer) = peer else {
                views.clients[slot] = None;
                views.squad[slot] = None;
                views.objective[slot] = None;
                views.ctf[slot] = None;
                continue;
            };
            let sensed = views.clients[slot].get_or_insert_with(SensedClient::default);
            sense_client(peer, sensed);
            let squad = views.squad[slot].get_or_insert_with(SquadClient::default);
            squad.team = sensed.team;
            squad.duel_team = sensed.duel_team;
            squad.bot = sensed.bot;
            squad.health = sensed.health;
            squad.red_flag = sensed.red_flag;
            squad.blue_flag = sensed.blue_flag;
            squad.netname.clone_from(&sensed.netname);
            let role = bots.minds.get(slot).and_then(Option::as_ref);
            views.objective[slot] = Some(ObjectiveClient {
                in_use: true,
                team: sensed.team,
                health: sensed.health,
                origin: sensed.origin,
                is_jedi_master: sensed.is_jedi_master,
                bot_role: role.map(|mind| mind.siege_state),
            });
            views.ctf[slot] = Some(CtfClient {
                team: sensed.team,
                red_flag: sensed.red_flag,
                blue_flag: sensed.blue_flag,
                origin: sensed.origin,
                bot_role: role.map(|mind| mind.ctf_state),
            });
        }
        views.things.clear();
        views.things.extend(
            missiles
                .iter()
                .map(|(number, missile)| sense_missile(number.legacy_number(), missile, 0, false)),
        );
        views.things.extend(charges.iter().map(|(number, charge)| {
            sense_missile(
                number.legacy_number(),
                &charge.missile,
                charge.health,
                charge.takes_damage,
            )
        }));
        views.jump_times.clear();
        views.jump_times.extend(
            bots.minds
                .iter()
                .map(|mind| mind.as_ref().map(|mind| mind.jump_time)),
        );
    }

    /// `UpdateEventTracker`, which `BotAIStartFrame` runs before the bots think: each
    /// client whose events moved on is heard afresh.
    pub(super) fn update_bot_trackers(&mut self, level_time: i32) {
        let Self {
            server,
            world,
            players,
            bots,
            ..
        } = self;
        let world = server.world(*world);
        let events = |slot: usize| {
            let state = handle_state(world, players, slot);
            state.map_or((0, [0; 2]), |state| {
                (
                    state.event_sequence(),
                    [0, 1].map(|index| i32::from(state.event(index).unwrap_or(0))),
                )
            })
        };
        bots.views
            .trackers
            .resize(players.places(), EventTracker::default());
        update_event_tracker(&mut bots.views.trackers, &events, level_time);
    }

    /// Bot `me`'s own state as it thinks (`cur_ps`), its ammunition and powerups written
    /// into `ammo` and `powerups`.
    pub(super) fn bot_self<'a>(
        &self,
        me: usize,
        ammo: &'a mut [i32; 16],
        powerups: &'a mut [i32; 16],
    ) -> Option<BotSelf<'a>> {
        let peer = self.peer(me)?;
        let state = &peer.state;
        let movement = peer.movement.state();
        *ammo = movement.ammo;
        for (slot, value) in powerups.iter_mut().zip(state.powerups) {
            *slot = value as i32;
        }
        let det_pack = self
            .charges
            .iter()
            .find(|(_, charge)| {
                charge.kind == Kind::DetPack && usize::from(charge.missile.owner) == me
            })
            .map(|(_, charge)| {
                ES_POS_BASE
                    .map(|field| f32::from_bits(charge.missile.state.raw_field(field).unwrap_or(0)))
            });
        let velocity = state.velocity();
        Some(BotSelf {
            health: peer.health,
            weapon: i32::from(movement.weapon),
            weapon_state: i32::from(movement.weapon_state),
            weapon_charge_time: movement.weapon_charge_time,
            rocket_lock_time: movement.rocket_lock_time,
            rocket_last_valid_time: movement.rocket_last_valid_time,
            ammo: &ammo[..],
            weapons: state.stats[STAT_WEAPONS] as i32,
            holdables: movement.holdable_items as i32,
            powerups: &powerups[..],
            force_power: i32::from(movement.force_power),
            force_side: i32::from(state.force_side()),
            powers_known: movement.force_powers_known as i32,
            powers_active: movement.force_powers_active as i32,
            force_levels: peer.force.levels.map(i32::from),
            grip_being_gripped: peer.force.grip_being_gripped,
            grip_cripple: i32::from(movement.force_grip_cripple),
            electrify_time: state.electrify_time(),
            force_jump_charge: 0.0,
            saber_in_flight: state.saber_in_flight(),
            saber_entity_num: i32::from(state.saber_entity_num()),
            saber_holstered: i32::from(state.saber_holstered()),
            saber_lock_time: state.saber_lock_time(),
            saber_anim_level: i32::from(state.saber_style()),
            has_det_pack_planted: state.has_detpack_planted(),
            duel_in_progress: state.duel_in_progress(),
            duel_index: i32::from(state.duel_index()),
            is_jedi_master: state.is_jedi_master(),
            on_ground: state.ground_entity_num() != ENTITY_NONE,
            jump_held: movement.movement_flags & PMF_JUMP_HELD != 0,
            last_upmove: i32::from(peer.last_command.up_move),
            moving: velocity != [0.0; 3],
            det_pack,
        })
    }
}

impl NativeGame {
    /// `droppedRedFlag`, `droppedBlueFlag`: each team's flag while it lies dropped
    /// (`FL_DROPPED_ITEM`), where it lies (`s.pos.trBase`). The reference keeps the last
    /// one launched, whose entity is freed (its flags cleared) once it is taken back.
    pub(super) fn dropped_flags(&self) -> [Option<LyingFlag>; 2] {
        [PW_REDFLAG, PW_BLUEFLAG].map(|tag| {
            self.items
                .iter()
                .find(|(_, pickup)| {
                    let row = &sjk_game_jka::items::ITEMS[pickup.item];
                    row.kind == sjk_game_jka::items::Kind::Team
                        && row.tag == tag as i32
                        && pickup.dropped.is_some_and(|dropped| !dropped.taken)
                })
                .map(|(number, pickup)| LyingFlag {
                    entity: i32::from(number.legacy_number()),
                    dropped: true,
                    base: ES_POS_BASE
                        .map(|field| f32::from_bits(pickup.state.raw_field(field).unwrap_or(0))),
                })
        })
    }

    /// The siege objectives' chain ends as the bots' objectives read them: a breakable as
    /// it stands (gone once broken), a usable brush, a trigger until it has fired for good
    /// (`trigger_once`), and the rest — relays, counters, objectives — always there. Every
    /// one of them has a `use` function.
    pub(super) fn siege_things(&self, things: &mut Vec<GoalThing>) {
        things.clear();
        for link in &self.bots.siege_links {
            let point = (link.centre, link.centre);
            let (in_use, takes_damage, health, bounds) = match link.model {
                Some(model) => {
                    if let Some((_, brush)) = self
                        .breakables
                        .iter()
                        .find(|(_, brush)| brush.model == model)
                    {
                        (true, brush.takes_damage, brush.health, brush.bounds)
                    } else if let Some((_, usable)) = self
                        .usable_entities
                        .iter()
                        .find(|(_, usable)| usable.model == model)
                    {
                        (true, false, 0, usable.bounds)
                    } else if let Some(trigger) =
                        self.multiples.iter().find(|trigger| trigger.model == model)
                    {
                        (
                            trigger.contents & sjk_game_jka::triggers::CONTENTS_TRIGGER != 0,
                            false,
                            0,
                            trigger.bounds,
                        )
                    } else {
                        // A broken breakable is gone from the game.
                        (false, false, 0, point)
                    }
                }
                None => (true, false, 0, point),
            };
            things.push(GoalThing {
                entity: link.entity,
                in_use,
                usable: true,
                takes_damage,
                health,
                absmin: bounds.0,
                absmax: bounds.1,
            });
        }
    }

    /// `gJMSaberEnt`: the Jedi Master's saber, as an entity in use and where it is.
    pub(super) fn jedi_master_saber(&self) -> Option<(i32, bool, [f32; 3])> {
        self.jedi_master.saber.as_ref().map(|saber| {
            (
                i32::from(saber.id.legacy_number()),
                true,
                saber.missile.current,
            )
        })
    }
}

/// A client's wire state, where it has one.
fn handle_state<'a>(
    world: Option<&'a sjk_server::ServerWorld<(), Peer>>,
    players: &crate::players::PlayerRoster,
    slot: usize,
) -> Option<&'a PlayerState> {
    let handle = players.at(slot)?;
    world?.entity(handle).map(|peer| &peer.state)
}

/// The map with the players standing in it, as a bot's traces meet it (`trap->Trace`,
/// `trap->InPVS`); the players are the frame's obstacles, the thinking bot left out.
pub(super) struct BotTraces<'a> {
    pub(super) map: Option<&'a LoadedMap>,
    pub(super) obstacles: &'a [BoxObstacle],
}

impl BotTraces<'_> {
    fn trace(
        &self,
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
            .trace(start, mins, maxs, end, mask),
            None => WithPlayers {
                world: Void,
                players: self.obstacles,
            }
            .trace(start, mins, maxs, end, mask),
        }
    }

    fn in_pvs(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.map
            .is_none_or(|map| Eye::new(&map.bsp, &map.areas, from).sees_point(&map.bsp, to))
    }
}

/// The senses' side of the server: the gathered clients and things, and point traces.
pub(super) struct ServerSenses<'a> {
    pub(super) traces: BotTraces<'a>,
    pub(super) views: &'a BotViews,
}

impl BotSenses for ServerSenses<'_> {
    fn client(&self, number: i32) -> Option<&SensedClient> {
        usize::try_from(number)
            .ok()
            .and_then(|number| self.views.clients.get(number))
            .and_then(Option::as_ref)
    }

    fn things(&self) -> &[SensedThing] {
        &self.views.things
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.traces.in_pvs(from, to)
    }

    fn trace(&mut self, from: [f32; 3], to: [f32; 3], _pass: i32) -> (f32, i32) {
        let trace = self.traces.trace(from, [0.0; 3], [0.0; 3], to, MASK_SOLID);
        (trace.fraction, i32::from(trace.entity_number))
    }
}

/// The movement checks' side of the server.
pub(super) struct ServerMoves<'a> {
    pub(super) traces: BotTraces<'a>,
    pub(super) views: &'a BotViews,
    pub(super) gametype: i32,
}

impl BotMoveWorld for ServerMoves<'_> {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        _pass: i32,
        mask: u32,
    ) -> MovementTrace {
        self.traces.trace(start, mins, maxs, end, mask)
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.traces.in_pvs(from, to)
    }

    /// Every brush among the obstacles is a `func_` entity: doors, plats, buttons and
    /// breakables.
    fn is_func(&self, entity: i32) -> bool {
        self.traces
            .obstacles
            .iter()
            .any(|obstacle| i32::from(obstacle.entity) == entity && obstacle.model.is_some())
    }

    fn special_owner(&self, _entity: i32) -> Option<Option<i32>> {
        None
    }

    fn same_team(&self, one: i32, other: i32) -> bool {
        let client = |number: i32| {
            usize::try_from(number)
                .ok()
                .and_then(|number| self.views.clients.get(number))
                .and_then(Option::as_ref)
        };
        match (client(one), client(other)) {
            (Some(one), Some(other)) => {
                sjk_game_jka::bot_senses::on_same_team(one, other, self.gametype)
            }
            _ => false,
        }
    }

    fn bot_jump_time(&self, client: i32) -> Option<f32> {
        usize::try_from(client)
            .ok()
            .and_then(|client| self.views.jump_times.get(client).copied().flatten())
    }
}
