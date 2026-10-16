//! The game's entities beyond its players: allocated as `G_Spawn` allocates them
//! (`g_utils.c:730-790`) — the lowest free slot above the players, but not one freed less
//! than a second ago unless nothing else is left — and, for the ones that exist only to
//! carry an event, freed as `G_RunFrame` frees them (`g_main.c:3080-3100`), `EVENT_VALID_MSEC`
//! after the event was raised. Held against the whole-game oracle's `temp` and `freed`
//! lines.
//!
//! The pool stores its entities by native identity ([`EntityId`]): an ordinal in
//! `G_Spawn`'s order and a generation, under a budget the caller chooses. Wire entity
//! numbers are the protocol-26 adapter's ([`sjk_protocol::LegacyEntityNumbers`]): each
//! stored wire state is stamped with its entity's projection, and an entity beyond what
//! a legacy client can represent is stamped `ENTITYNUM_NONE` and flagged
//! ([`PoolEntity::legacy_number`]) for the snapshot adapter to withhold. The pool knows
//! nothing of what a slot holds beyond its wire state and its timing.

use crate::entity_id::EntityId;
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, LEGACY_GAME_ENTITIES, LegacyEntityNumbers};

/// The JKA profile's budget: `G_Spawn` fails ("no free entities") once every slot below
/// `ENTITYNUM_MAX_NORMAL` above the players is in use. A match that must stay within
/// what every legacy client can be sent keeps it; the pool itself takes any budget
/// ([`EntityPool::with_budget`]).
pub const JKA_PROFILE_BUDGET: usize = LEGACY_GAME_ENTITIES;
/// `EVENT_VALID_MSEC`.
const EVENT_VALID_MS: i32 = 300;
/// `BODY_QUEUE_SIZE`: how many bodies lie around before the oldest is reused.
const BODIES: usize = 8;
/// `BODY_SINK_TIME`: a body fades this long after it fell, and is unlinked 18 s later.
const BODY_SINK_MS: i32 = 30_000;
const BODY_FADE_THINK_MS: i32 = 18_000;
/// `EV_BODYFADE`, with `G_AddEvent`'s stepped sequence bits.
const EV_BODYFADE: u32 = 125;
const ES_EVENT: usize = 28;
/// `s.eFlags`, `s.eventParm`, `s.eType`.
const ES_EFLAGS: usize = 19;
const ES_EVENT_PARM: usize = 42;
const ES_TYPE: usize = 8;
/// `s.groundEntityNum`.
const ES_GROUND_ENTITY: usize = 22;
/// `s.trickedentindex`.
const ES_TRICKED: usize = 58;
/// `EF_SOUNDTRACKER`: a temp entity a player's looping sound is tracked by (`G_Sound` on a
/// tracking channel).
pub const EF_SOUNDTRACKER: u32 = 1 << 24;
const EVENT_BITS: u32 = 0x300;
const EVENT_BIT1: u32 = 0x100;
/// A freed slot is left alone for this long, unless nothing else is free.
const REST_MS: i32 = 1_000;
/// `ET_EVENTS`: a temp entity's type is this plus its event.
const ET_EVENTS: u32 = 18;
/// `EF_PERMANENT`: an entity the client builds for itself and is never sent.
const EF_PERMANENT: u32 = 128;
/// The events whose temp entities every site that makes them flags `SVF_BROADCAST`
/// (`EV_CLIENTJOIN`, `EV_GLOBAL_DUEL`, `EV_GLOBAL_ITEM_PICKUP`, `EV_MUTE_SOUND`,
/// `EV_VOICECMD_SOUND`, `EV_GLOBAL_SOUND`, `EV_GLOBAL_TEAM_SOUND`, `EV_OBITUARY`,
/// `EV_CTFMESSAGE`, `EV_SIEGE_ROUNDOVER`, `EV_SIEGE_OBJECTIVECOMPLETE`,
/// `EV_DESTROY_GHOUL2_INSTANCE`, `EV_DESTROY_WEAPON_MODEL`, `EV_SET_FREE_SABER`,
/// `EV_SET_FORCE_DISABLE`), numbered as `bg_public.h` numbers them. `EV_GLOBAL_ITEM_PICKUP`
/// for an item with a `speed` goes to its toucher alone instead, which no stock map uses.
const BROADCAST_EVENTS: [u32; 15] = [
    1, 14, 23, 74, 75, 77, 78, 93, 99, 101, 102, 103, 104, 106, 107,
];

/// One slot of the pool.
struct Slot {
    state: EntityState,
    /// How many times the slot has been handed out: the generation of its [`EntityId`].
    generation: u32,
    in_use: bool,
    /// `freetime`: when the slot was last freed.
    freed_at: i32,
    /// `eventTime`: when its event was raised, for the ones that carry one.
    event_time: i32,
    /// `freeAfterEvent`.
    free_after_event: bool,
    /// `SVF_NOTSINGLECLIENT`'s `singleClient`: everyone but this wire client is sent it.
    not_for: Option<u16>,
    linked: bool,
    /// `timestamp` of a body: when it fell; `None` for anything else.
    body_since: Option<i32>,
    /// `r.mins`, `r.maxs`: the box `SV_LinkEntity` links around it (a point for most
    /// temp entities).
    bounds: ([f32; 3], [f32; 3]),
    /// `SVF_BROADCAST` set by the game for this entity itself (a body, an objective).
    broadcast: bool,
    /// A freed entity's `s.groundEntityNum` written after its `G_FreeEntity` cleared it —
    /// `G_RunItem` bouncing an item its own think freed (`g_items.c:3259-3278`) — which
    /// `G_Spawn` does not clear: the slot's next occupant keeps it until it sets its own.
    ground_residue: Option<u32>,
    /// A freed sound tracker's `s.eFlags` (`EF_SOUNDTRACKER`) and `s.trickedentindex`
    /// written after its `G_FreeEntity` — `G_Sound` freeing its own new temp entity through
    /// a stale `killSoundEntIndex` (`g_utils.c:1358-1376`) — which `G_Spawn` does not clear:
    /// the slot's next occupant carries them where it sets none of its own.
    tracker_residue: Option<u32>,
}

/// The game's entities beyond its players, by native identity.
pub struct EntityPool {
    slots: Vec<Slot>,
    /// How many slots the pool may ever open: `G_Spawn` fails beyond it.
    budget: usize,
    /// Counts every change to what the pool sends, so a caller can tell when the links it
    /// worked out from the pool are stale.
    generation: u64,
    /// `level.startTime`: the first two seconds relax the rest a freed slot gets.
    start_time: i32,
    /// `level.bodyQueIndex`: the next body slot to reuse.
    next_body: usize,
    /// How far this frame's upkeep has gone ([`Self::run_through`]): the slots before it are
    /// done.
    upkept: usize,
}

/// An entity of the pool as a snapshot lists it.
#[derive(Clone, Copy)]
pub struct PoolEntity<'a> {
    /// Its native identity.
    pub id: EntityId,
    /// Its wire state, numbered as the legacy adapter numbers it.
    pub state: &'a EntityState,
    /// The one wire client that is not sent it, if any.
    pub not_for: Option<u16>,
    /// `r.mins`, `r.maxs`.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `SVF_BROADCAST`: sent to everyone wherever they are — set for the entity, or for
    /// its event ([`broadcast_event`]), or a sound tracker's (`G_Sound`, `g_utils.c:1376`).
    pub broadcast: bool,
    /// `EF_PERMANENT`: never sent (`SV_AddEntitiesVisibleFromPoint`).
    pub permanent: bool,
}

impl PoolEntity<'_> {
    /// The number a legacy client knows it by, or `None` when it lies beyond what a
    /// legacy client can represent: the snapshot adapter withholds it and counts it.
    pub fn legacy_number(&self) -> Option<u16> {
        LegacyEntityNumbers::game_entity(self.id.ordinal())
    }
}

/// Whether a temp entity carries one of the events the reference always broadcasts.
pub fn broadcast_event(state: &EntityState) -> bool {
    let kind = state.raw_field(ES_TYPE).unwrap_or(0);
    kind > ET_EVENTS && BROADCAST_EVENTS.contains(&(kind - ET_EVENTS))
}

impl EntityPool {
    /// A pool for a game that started at `start_time`, under the JKA profile's budget
    /// ([`JKA_PROFILE_BUDGET`]): `InitBodyQue` has taken the first eight slots, never
    /// freed, unlinked until a body lies in them.
    pub fn new(start_time: i32) -> Self {
        Self::with_budget(start_time, JKA_PROFILE_BUDGET)
    }

    /// A pool that opens at most `budget` slots (never fewer than the eight bodies').
    /// Slots are opened as they are needed, not preallocated.
    pub fn with_budget(start_time: i32, budget: usize) -> Self {
        let mut pool = Self {
            slots: Vec::new(),
            budget: budget.max(BODIES),
            generation: 0,
            start_time,
            next_body: 0,
            upkept: 0,
        };
        pool.reserve(BODIES);
        pool
    }

    /// `CopyToBodyQue`'s slot: the next of the eight, round robin, linked from now and
    /// sinking `BODY_SINK_TIME` on; returns its identity.
    pub fn spawn_body(&mut self, state: EntityState, level_time: i32) -> EntityId {
        let slot = self.next_body;
        self.next_body = (self.next_body + 1) % BODIES;
        let id = self.occupy(
            slot,
            state,
            Occupant {
                event_time: level_time,
                free_after_event: false,
                not_for: None,
                linked: true,
            },
        );
        let entry = &mut self.slots[slot];
        (entry.body_since, entry.broadcast) = (Some(level_time), true);
        id
    }

    /// Slots taken before the pool hands any out: the map's entities and what the game
    /// spawns for itself at start (`G_SpawnEntities`, the saber, never-freed helpers).
    /// Until those live here, they are counted and skipped.
    pub fn reserve(&mut self, count: usize) {
        self.generation += 1;
        for _ in 0..count {
            self.slots.push(Slot::closed(true));
        }
    }

    /// `G_Spawn` for an entity that is never freed and never sent — a player's saber
    /// entity (`WP_SaberInitBladeData`: `SVF_NOCLIENT`, `neverFree`), which exists on the
    /// server alone. Its legacy number is what the player's state names in
    /// `saberEntityNum`.
    pub fn spawn_hidden(&mut self, level_time: i32) -> Option<EntityId> {
        let slot = self.take(level_time)?;
        let state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
        Some(self.occupy(
            slot,
            state,
            Occupant {
                event_time: 0,
                free_after_event: false,
                not_for: None,
                linked: false,
            },
        ))
    }

    /// `G_Spawn` for an entity that stays until something frees it — a missile — linked
    /// from now; `None` when the pool's budget is spent.
    pub fn spawn_entity(&mut self, state: EntityState, level_time: i32) -> Option<EntityId> {
        let slot = self.take(level_time)?;
        Some(self.occupy(
            slot,
            state,
            Occupant {
                event_time: 0,
                free_after_event: false,
                not_for: None,
                linked: true,
            },
        ))
    }

    /// `G_Spawn` + `G_TempEntity`: a slot for an entity that carries one event and is freed
    /// after it. `state` is what it holds, already numbered or not — the legacy adapter's
    /// number for it is written over it. `None` when the pool's budget is spent (`G_Spawn`
    /// drops the server there).
    pub fn spawn_temporary(
        &mut self,
        state: EntityState,
        level_time: i32,
        not_for: Option<u16>,
    ) -> Option<EntityId> {
        let slot = self.take(level_time)?;
        Some(self.occupy(
            slot,
            state,
            Occupant {
                event_time: level_time,
                free_after_event: true,
                not_for,
                linked: true,
            },
        ))
    }

    /// Hands `slot` out again: a new generation, `state` stamped with the legacy
    /// adapter's number for it, in use from now.
    fn occupy(&mut self, slot: usize, mut state: EntityState, occupant: Occupant) -> EntityId {
        self.generation += 1;
        let entry = &mut self.slots[slot];
        let id = EntityId::new(slot as u32, entry.generation.wrapping_add(1));
        let _ = state.set_number(id.legacy_number());
        let Occupant {
            event_time,
            free_after_event,
            not_for,
            linked,
        } = occupant;
        if let Some(ground) = entry.ground_residue
            && state.raw_field(ES_GROUND_ENTITY) == Some(0)
        {
            state.set_raw_field(ES_GROUND_ENTITY, ground);
        }
        if let Some(tricked) = entry.tracker_residue {
            if state.raw_field(ES_EFLAGS) == Some(0) {
                state.set_raw_field(ES_EFLAGS, EF_SOUNDTRACKER);
            }
            if state.raw_field(ES_TRICKED) == Some(0) {
                state.set_raw_field(ES_TRICKED, tricked);
            }
        }
        *entry = Slot {
            state,
            generation: id.generation(),
            in_use: true,
            freed_at: 0,
            event_time,
            free_after_event,
            not_for,
            linked,
            body_since: None,
            bounds: ([0.0; 3], [0.0; 3]),
            broadcast: false,
            ground_residue: None,
            tracker_residue: None,
        };
        id
    }

    /// [`Self::leave_ground_residue`] for the freed slot the legacy adapter numbers
    /// `number`, whatever generation last held it.
    pub fn leave_freed_ground_residue(&mut self, number: u16, ground: u16) {
        let Some(ordinal) = LegacyEntityNumbers::ordinal(number) else {
            return;
        };
        if let Some(slot) = self.slots.get(ordinal) {
            let id = EntityId::new(ordinal as u32, slot.generation);
            self.leave_ground_residue(id, ground);
        }
    }

    /// A freed slot's `s.groundEntityNum` written after the entity was freed (see
    /// [`Slot::ground_residue`]): its next occupant starts with it. Nothing for a slot in
    /// use.
    pub fn leave_ground_residue(&mut self, id: EntityId, ground: u16) {
        self.generation += 1;
        if let Some(slot) = self
            .slots
            .get_mut(id.ordinal())
            .filter(|slot| !slot.in_use && slot.generation == id.generation())
        {
            slot.ground_residue = Some(u32::from(ground));
        }
    }

    /// A freed sound tracker's residue (see [`Slot::tracker_residue`]): `tricked` is the
    /// client its sound was on. Nothing for a slot in use.
    pub fn leave_tracker_residue(&mut self, id: EntityId, tricked: u16) {
        self.generation += 1;
        if let Some(slot) = self
            .slots
            .get_mut(id.ordinal())
            .filter(|slot| !slot.in_use && slot.generation == id.generation())
        {
            slot.tracker_residue = Some(u32::from(tricked));
        }
    }

    /// An entity in use sent to the clients from now (`SVF_NOCLIENT` cleared and linked),
    /// or hidden again: a player's saber entity while it flies.
    pub fn show(&mut self, id: EntityId, shown: bool) {
        if let Some(slot) = self.slot_mut(id) {
            slot.linked = shown;
        }
    }

    /// `G_AddEvent`'s `eventTime` for an entity in use whose state the caller changed: its
    /// event is cleared `EVENT_VALID_MSEC` on (a knocked saber's bounce).
    pub fn event_raised(&mut self, id: EntityId, level_time: i32) {
        if let Some(slot) = self.slot_mut(id) {
            slot.event_time = level_time;
        }
    }

    /// The wire state of an entity in use, to be changed in place (a missile run on).
    pub fn state_mut(&mut self, id: EntityId) -> Option<&mut EntityState> {
        self.slot_mut(id).map(|slot| &mut slot.state)
    }

    /// An entity in use given `state` as its wire state (a missile's run published),
    /// stamped with the legacy adapter's number for it.
    pub fn set_state(&mut self, id: EntityId, state: &EntityState) {
        if let Some(slot) = self.slot_mut(id) {
            slot.state.copy_from(state);
            let _ = slot.state.set_number(id.legacy_number());
        }
    }

    /// Whether `id` is in use and linked: sent, and in the world's traces.
    pub fn is_linked(&self, id: EntityId) -> bool {
        self.slot(id).is_some_and(|slot| slot.in_use && slot.linked)
    }

    /// Whether `id` is an entity of the pool's in use (`inuse`).
    pub fn in_use(&self, id: EntityId) -> bool {
        self.state(id).is_some()
    }

    /// The wire state of an entity in use.
    pub fn state(&self, id: EntityId) -> Option<&EntityState> {
        self.slot(id)
            .filter(|slot| slot.in_use)
            .map(|slot| &slot.state)
    }

    /// The entity in use a legacy client knows by wire number `number`, if any: the way
    /// back from what a trace or a wire field names to the pool's identity.
    pub fn legacy_id(&self, number: u16) -> Option<EntityId> {
        let ordinal = LegacyEntityNumbers::ordinal(number)?;
        let slot = self.slots.get(ordinal).filter(|slot| slot.in_use)?;
        Some(EntityId::new(ordinal as u32, slot.generation))
    }

    /// `G_AddEvent` with `freeAfterEvent`: from now the entity exists for the event it
    /// carries and is freed `EVENT_VALID_MSEC` on.
    pub fn free_after_event(&mut self, id: EntityId, level_time: i32) {
        if let Some(slot) = self.slot_mut(id) {
            (slot.free_after_event, slot.event_time) = (true, level_time);
        }
    }

    /// `G_FreeEntity`: the slot is unlinked and rests.
    pub fn free(&mut self, id: EntityId, level_time: i32) {
        if let Some(slot) = self.slot_mut(id) {
            (slot.in_use, slot.linked, slot.freed_at) = (false, false, level_time);
        }
    }

    /// The lowest usable slot: a free one that has rested, else a free one, else a new one
    /// while the budget allows.
    fn take(&mut self, level_time: i32) -> Option<usize> {
        for force in [false, true] {
            let rested = |slot: &Slot| {
                force
                    || !(slot.freed_at > self.start_time + 2_000
                        && level_time - slot.freed_at < REST_MS)
            };
            if let Some(index) = self
                .slots
                .iter()
                .position(|slot| !slot.in_use && rested(slot))
            {
                return Some(index);
            }
            if self.slots.len() < self.budget {
                break;
            }
        }
        if self.slots.len() >= self.budget {
            return None;
        }
        self.slots.push(Slot::closed(false));
        Some(self.slots.len() - 1)
    }

    /// The start of `G_RunFrame` for the pool: an event older than `EVENT_VALID_MSEC` is
    /// cleared, and an entity that existed for it is freed.
    pub fn run_frame(&mut self, level_time: i32) {
        self.begin_frame();
        self.run_rest(level_time);
    }

    /// A frame begun: no slot's upkeep done yet ([`Self::run_through`]).
    pub fn begin_frame(&mut self) {
        self.generation += 1;
        self.upkept = 0;
    }

    /// `G_RunFrame`'s upkeep of each slot (`g_main.c:3080-3100`) in its turn, for the slots
    /// up to and including the one at `ordinal` ([`EntityId::ordinal`]) not yet kept this
    /// frame — for a caller that runs other entities (thinks, missiles) between them in
    /// run order, as the reference does: an entity freed at its turn in the frame is still
    /// in use for the entities before it.
    pub fn run_through(&mut self, level_time: i32, ordinal: usize) {
        self.run_until(level_time, ordinal.saturating_add(1));
    }

    /// The upkeep of every slot not yet kept this frame.
    pub fn run_rest(&mut self, level_time: i32) {
        self.run_until(level_time, usize::MAX);
    }

    fn run_until(&mut self, level_time: i32, end: usize) {
        let end = end.min(self.slots.len());
        let start = self.upkept.min(end);
        self.upkept = self.upkept.max(end);
        for slot in &mut self.slots[start..end] {
            if !slot.in_use {
                continue;
            }
            // `BodySink`: a body's think, `BODY_SINK_TIME` after it fell, fades it
            // (`EV_BODYFADE`) and thinks again 18 s later, when it is unlinked.
            if let Some(since) = slot.body_since
                && slot.linked
            {
                if level_time - since > BODY_SINK_MS + BODY_FADE_THINK_MS {
                    slot.linked = false;
                } else if level_time - since >= BODY_SINK_MS
                    && slot.event_time < since + BODY_SINK_MS
                {
                    let bits = (slot.state.raw_field(ES_EVENT).unwrap_or(0) & EVENT_BITS)
                        .wrapping_add(EVENT_BIT1)
                        & EVENT_BITS;
                    slot.state.set_raw_field(ES_EVENT, EV_BODYFADE | bits);
                    slot.event_time = level_time;
                }
                continue;
            }
            if level_time - slot.event_time <= EVENT_VALID_MS {
                continue;
            }
            if slot.free_after_event
                && slot.state.raw_field(ES_EFLAGS).unwrap_or(0) & EF_SOUNDTRACKER != 0
            {
                // A sound tracker stays, its event spent, until the sound it tracks is
                // muted (`g_main.c:3093-3099`): it is what a client follows the sound by.
                for field in [ES_EVENT, ES_EVENT_PARM, ES_TYPE] {
                    slot.state.set_raw_field(field, 0);
                }
                slot.event_time = 0;
            } else if slot.free_after_event {
                (slot.in_use, slot.linked, slot.freed_at) = (false, false, level_time);
            } else if slot.state.raw_field(ES_EVENT) != Some(0) {
                // `G_RunFrame` (`g_main.c:3080-3090`): an event shown long enough is
                // taken off the entity.
                slot.state.set_raw_field(ES_EVENT, 0);
            }
        }
    }

    /// `G_AddEvent` on an entity that stays: its event is shown for `EVENT_VALID_MSEC`
    /// from now.
    pub fn raise_event(&mut self, id: EntityId, level_time: i32) {
        self.generation += 1;
        if let Some(slot) = self
            .slots
            .get_mut(id.ordinal())
            .filter(|slot| slot.generation == id.generation())
        {
            slot.event_time = level_time;
        }
    }

    /// Every slot the pool has opened, in run order: the identity of what it holds or last
    /// held, whether it is in use, its wire state, and — for an entity that exists for its
    /// event (`freeAfterEvent`) — when the event was raised.
    pub fn entities(&self) -> impl Iterator<Item = (EntityId, bool, &EntityState, Option<i32>)> {
        self.slots.iter().enumerate().map(|(slot, entry)| {
            (
                EntityId::new(slot as u32, entry.generation),
                entry.in_use,
                &entry.state,
                entry.free_after_event.then_some(entry.event_time),
            )
        })
    }

    /// The client an entity is not sent to (`SVF_NOTSINGLECLIENT`, `r.singleClient`), if
    /// any.
    pub fn not_for(&self, id: EntityId) -> Option<u16> {
        self.slot(id)?.not_for
    }

    /// Every linked entity, in run order, as `SV_BuildClientSnapshot` walks them.
    pub fn linked(&self) -> impl Iterator<Item = PoolEntity<'_>> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.in_use && slot.linked)
            .map(|(ordinal, slot)| {
                let flags = slot.state.raw_field(ES_EFLAGS).unwrap_or(0);
                PoolEntity {
                    id: EntityId::new(ordinal as u32, slot.generation),
                    state: &slot.state,
                    not_for: slot.not_for,
                    bounds: slot.bounds,
                    broadcast: slot.broadcast
                        || broadcast_event(&slot.state)
                        || (slot.free_after_event && flags & EF_SOUNDTRACKER != 0),
                    permanent: flags & EF_PERMANENT != 0,
                }
            })
    }

    /// `r.mins`/`r.maxs` of an entity in use: the box it is linked by.
    pub fn set_bounds(&mut self, id: EntityId, bounds: ([f32; 3], [f32; 3])) {
        if let Some(slot) = self.slot_mut(id) {
            slot.bounds = bounds;
        }
    }

    /// `SVF_BROADCAST` on an entity in use, or off.
    pub fn set_broadcast(&mut self, id: EntityId, broadcast: bool) {
        if let Some(slot) = self.slot_mut(id) {
            slot.broadcast = broadcast;
        }
    }

    /// A number that changes whenever anything the pool sends may have: a caller keeps
    /// what it worked out from the pool until this moves.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The slot `id` names, in whatever use, if it is still that entity's.
    fn slot(&self, id: EntityId) -> Option<&Slot> {
        self.slots
            .get(id.ordinal())
            .filter(|slot| slot.generation == id.generation())
    }

    /// The slot of an entity in use, counted as a change.
    fn slot_mut(&mut self, id: EntityId) -> Option<&mut Slot> {
        self.generation += 1;
        self.slots
            .get_mut(id.ordinal())
            .filter(|slot| slot.in_use && slot.generation == id.generation())
    }

    /// How many slots the pool has ever opened (`level.num_entities - MAX_CLIENTS`).
    pub fn opened(&self) -> usize {
        self.slots.len()
    }

    /// How many slots it may open.
    pub fn budget(&self) -> usize {
        self.budget
    }
}

/// How a slot handed out is used, as the spawn that took it says.
struct Occupant {
    event_time: i32,
    free_after_event: bool,
    not_for: Option<u16>,
    linked: bool,
}

impl Slot {
    /// A slot opened and never handed out (`in_use` for one reserved).
    fn closed(in_use: bool) -> Self {
        Self {
            state: EntityState::zero(0, &LEGACY_ENTITY_FIELDS),
            generation: 0,
            in_use,
            freed_at: 0,
            event_time: 0,
            free_after_event: false,
            not_for: None,
            linked: false,
            body_since: None,
            bounds: ([0.0; 3], [0.0; 3]),
            broadcast: false,
            ground_residue: None,
            tracker_residue: None,
        }
    }
}
