//! What the other clients are shown of a player: `BG_PlayerStateToEntityState` and
//! `BG_PlayerStateToEntityStateExtraPolate` (OpenJK `codemp/game/bg_misc.c:2762-3050`),
//! which the game runs after every command of a player and at the end of every frame.
//! Held against `tools/game-oracle/entity.c`: 400 drawn player states, converted up to
//! three times in a row, every entity field the functions write and none they do not.
//!
//! Both states are protocol-26 wire states, so the port is a table of wire indices — a
//! JKA table, which is why it lives in this crate.

use sjk_protocol::{EntityState, PlayerState};

/// How the entity's position is to be read between snapshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerEntityMotion {
    /// `TR_INTERPOLATE`: the plain function, which `ClientSpawn` uses (snapping).
    Interpolated,
    /// `TR_LINEAR_STOP` from the player's command time, for at most 50 ms: the
    /// reference's `g_smoothClients 1`, its default.
    Extrapolated {
        /// `ps.commandTime`.
        time: i32,
    },
}

/// Player wire field → entity wire field, copied bit for bit.
const COPIED: [(usize, usize); 49] = [
    (2, 2),
    (1, 1),
    (5, 4), // origin → pos.trBase
    (6, 6),
    (7, 7),
    (8, 10), // velocity → pos.trDelta
    (4, 5),
    (3, 3),
    (50, 33), // viewangles → apos.trBase
    (98, 58),
    (99, 74),
    (101, 92),
    (104, 94), // mind trick targets → trickedentindex 1..4
    (108, 85), // saberLockFrame → forceFrame
    (73, 47),  // electrifyTime → emplacedOwner
    (12, 31),  // speed
    (26, 18),  // genericEnemyIndex
    (72, 68),  // activeForcePass
    (13, 16),
    (15, 17), // legsAnim, torsoAnim
    (69, 62),
    (55, 50),  // legsFlip, torsoFlip
    (43, 32),  // clientNum
    (103, 96), // eFlags2
    (88, 81),
    (31, 37),
    (34, 43),  // saberInFlight, saberEntityNum, saberMove
    (82, 75),  // forcePowersActive
    (112, 39), // emplacedIndex → otherEntityNum2
    (81, 71),  // saberHolstered
    (47, 14),
    (16, 22), // weapon, groundEntityNum
    (75, 55),
    (85, 86), // loopSound, generic1
    (33, 41), // weaponstate → modelindex2
    (68, 64), // weaponChargeTime → constantLight
    (102, 56),
    (105, 60),
    (100, 53), // lastHitLoc → origin2
    (114, 97), // isJediMaster
    (113, 61), // holocronBits → time2
    (23, 27),  // saberAnimLevel → fireflag
    (121, 99),
    (122, 100), // heldByClient, ragAttach
    (123, 76),
    (93, 79), // iModelScale, brokenLimbs
    (76, 66),
    (66, 52), // hasLookTarget, lookTarget
    (84, 93), // m_iVehicleNum
];
/// customRGBA, whose four bytes the two tables order differently.
const COLOUR: [(usize, usize); 4] = [(29, 30), (42, 35), (45, 36), (32, 29)];
/// The snapped fields: pos.trBase, then apos.trBase.
const SNAPPED: [usize; 6] = [2, 1, 4, 5, 3, 33];

const PS_PM_TYPE: usize = 63;
const PS_GENERIC_ENEMY: usize = 26;
const PS_MOVEMENT_DIR: usize = 30;
const PS_EFLAGS: usize = 17;
const PS_DUEL_IN_PROGRESS: usize = 119;
const PS_EXTERNAL_EVENT: usize = 56;
const PS_EXTERNAL_EVENT_PARM: usize = 64;
const PS_EVENT_SEQUENCE: usize = 19;
const PS_EVENTS: [usize; 2] = [27, 28];
const PS_EVENT_PARMS: [usize; 2] = [65, 60];
const PS_CLIENT_NUM: usize = 43;
const ES_TYPE: usize = 8;
const ES_POS_TYPE: usize = 23;
const ES_POS_TIME: usize = 0;
const ES_POS_DURATION: usize = 20;
const ES_APOS_TYPE: usize = 15;
const ES_ANGLES2_YAW: usize = 51;
const ES_EFLAGS: usize = 19;
const ES_BOLT1: usize = 91;
const ES_EVENT: usize = 28;
const ES_EVENT_PARM: usize = 42;
const ES_POWERUPS: usize = 77;

const PM_SPECTATOR: u32 = 4;
const PM_INTERMISSION: u32 = 7;
const ET_PLAYER: u32 = 1;
const ET_INVISIBLE: u32 = 12;
const TR_INTERPOLATE: u32 = 1;
const TR_LINEAR_STOP: u32 = 3;
const GIB_HEALTH: i32 = -40;
const EF_DEAD: u32 = 1 << 1;
const EF_SEEKERDRONE: u32 = 1 << 21;
const MAX_PS_EVENTS: i32 = 2;
const STAT_HEALTH: usize = 0;
/// `1000 / sv_fps` at the reference's default of 20, as the reference hard-codes it.
const EXTRAPOLATION_MS: u32 = 50;

/// `ClientEndFrame`'s first act for a player in the game (`g_active.c:3724-3729`): every
/// powerup whose time is past is gone.
pub fn expire_powerups(player: &mut PlayerState, level_time: i32) {
    for powerup in &mut player.powerups {
        if (*powerup as i32) < level_time {
            *powerup = 0;
        }
    }
}

/// `s.powerups` as `ClientEndFrame` leaves it (`BG_PlayerStateToEntityState`): a bit for
/// every powerup the state holds — what the team overlay reads after the frame's end.
pub fn powerup_bits(player: &PlayerState) -> u32 {
    player
        .powerups
        .iter()
        .enumerate()
        .filter(|(_, until)| **until != 0)
        .fold(0, |bits, (index, _)| bits | 1 << index)
}

/// `G_AddEvent` on a player (`g_utils.c:1195-1210`): the event goes into its state's
/// external event, the sequence bits stepped; the entity's `eventTime` is the caller's
/// ([`PlayerEntity::event_raised`]).
pub fn add_event(state: &mut PlayerState, event: u32, parameter: u32) {
    let bits =
        (state.raw_field(PS_EXTERNAL_EVENT).unwrap_or(0) & 0x300).wrapping_add(0x100) & 0x300;
    state.set_raw_field(PS_EXTERNAL_EVENT, event | bits);
    state.set_raw_field(PS_EXTERNAL_EVENT_PARM, parameter);
}

/// Write `entity` from `player`; what the reference leaves alone is left alone.
/// `entity_event_sequence` is `ps.entityEventSequence`, which is not on the wire: how many
/// of the player's predictable events its entity has shown. At most one more is shown per
/// call, and an entity more than two behind skips ahead.
pub fn player_entity_state(
    player: &PlayerState,
    entity_event_sequence: &mut i32,
    motion: PlayerEntityMotion,
    snap: bool,
    entity: &mut EntityState,
) {
    convert(player, entity_event_sequence, motion, snap, entity, true);
}

/// The conversion; `external` is whether the player's external event counts, which
/// `SendPendingPredictableEvents` switches off around its own call.
fn convert(
    player: &PlayerState,
    entity_event_sequence: &mut i32,
    motion: PlayerEntityMotion,
    snap: bool,
    entity: &mut EntityState,
    external: bool,
) {
    let read = |index: usize| player.raw_field(index).unwrap_or(0);
    let health = player.stats[STAT_HEALTH] as i32;
    let hidden = matches!(read(PS_PM_TYPE), PM_INTERMISSION | PM_SPECTATOR) || health <= GIB_HEALTH;
    entity.set_raw_field(ES_TYPE, if hidden { ET_INVISIBLE } else { ET_PLAYER });
    // A client number is below 32; an entity state refuses only numbers past the table.
    let _ = entity.set_number(read(PS_CLIENT_NUM) as u16);
    for (from, to) in COPIED.into_iter().chain(COLOUR) {
        entity.set_raw_field(to, read(from));
    }
    match motion {
        PlayerEntityMotion::Interpolated => {
            entity.set_raw_field(ES_POS_TYPE, TR_INTERPOLATE);
        }
        PlayerEntityMotion::Extrapolated { time } => {
            entity.set_raw_field(ES_POS_TYPE, TR_LINEAR_STOP);
            entity.set_raw_field(ES_POS_TIME, time as u32);
            entity.set_raw_field(ES_POS_DURATION, EXTRAPOLATION_MS);
        }
    }
    entity.set_raw_field(ES_APOS_TYPE, TR_INTERPOLATE);
    if snap {
        for index in SNAPPED {
            let value = f32::from_bits(entity.raw_field(index).unwrap_or(0));
            // The game module's own `SnapVector` (`q_math.c:1240`), not the engine's that
            // movement calls: an `(int)` cast on every build but 32-bit MSVC, so it
            // truncates where the engine's rounds.
            entity.set_raw_field(index, (value as i32 as f32).to_bits());
        }
    }
    entity.set_raw_field(
        ES_ANGLES2_YAW,
        (read(PS_MOVEMENT_DIR) as i32 as f32).to_bits(),
    );
    entity.set_raw_field(ES_BOLT1, u32::from(read(PS_DUEL_IN_PROGRESS) != 0));

    let mut flags = read(PS_EFLAGS);
    if read(PS_GENERIC_ENEMY) as i32 != -1 {
        flags |= EF_SEEKERDRONE;
    }
    flags = if health <= 0 {
        flags | EF_DEAD
    } else {
        flags & !EF_DEAD
    };
    entity.set_raw_field(ES_EFLAGS, flags);

    let event_sequence = read(PS_EVENT_SEQUENCE) as i32;
    if external && read(PS_EXTERNAL_EVENT) != 0 {
        entity.set_raw_field(ES_EVENT, read(PS_EXTERNAL_EVENT));
        entity.set_raw_field(ES_EVENT_PARM, read(PS_EXTERNAL_EVENT_PARM));
    } else if *entity_event_sequence < event_sequence {
        *entity_event_sequence = (*entity_event_sequence).max(event_sequence - MAX_PS_EVENTS);
        let slot = (*entity_event_sequence & (MAX_PS_EVENTS - 1)) as usize;
        entity.set_raw_field(
            ES_EVENT,
            read(PS_EVENTS[slot]) | ((*entity_event_sequence & 3) as u32) << 8,
        );
        entity.set_raw_field(ES_EVENT_PARM, read(PS_EVENT_PARMS[slot]));
        *entity_event_sequence += 1;
    }

    let carried = player
        .powerups
        .iter()
        .enumerate()
        .filter(|(_, held)| **held != 0)
        .fold(0, |mask, (index, _)| mask | 1 << index);
    entity.set_raw_field(ES_POWERUPS, carried);
}

const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_TEAM_OWNER: usize = 21;
const PS_ORIGIN: [usize; 3] = [2, 1, 5];
const PS_COMMAND_TIME: usize = 0;
const NPCTEAM_PLAYER: u32 = 2;
const EVENT_VALID_MS: i32 = 300;
const ET_EVENTS: u32 = 18;
const EF_PLAYER_EVENT: u32 = 1 << 5;
const ES_OTHER_ENTITY_NUM: usize = 59;
const ES_SOLID: usize = 26;

/// A player's entity as the game keeps it between snapshots: the wire state the other
/// clients are sent, and how many of the player's events it has shown.
#[derive(Clone, Debug)]
pub struct PlayerEntity {
    state: EntityState,
    /// The temp entity of the last conversion that left an event over; kept to be reused.
    overflow: EntityState,
    shown_events: i32,
    /// `gentity_t::eventTime`: when the player last raised an event in a command.
    event_time: i32,
}

impl PlayerEntity {
    /// The entity of a client who has not begun yet.
    pub fn new(client: u16) -> Self {
        let mut state = EntityState::zero(client, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        let _ = state.set_number(client);
        let overflow = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        Self {
            state,
            overflow,
            shown_events: 0,
            event_time: 0,
        }
    }

    /// An entity as it was left: its state, how many of its player's events it has
    /// shown, and when the player last raised one.
    pub fn from_parts(state: EntityState, shown_events: i32, event_time: i32) -> Self {
        let overflow = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        Self {
            state,
            overflow,
            shown_events,
            event_time,
        }
    }

    /// How many of its player's events the entity has shown (`ps.entityEventSequence`).
    pub fn shown_events(&self) -> i32 {
        self.shown_events
    }

    /// What a snapshot lists.
    pub fn state(&self) -> &EntityState {
        &self.state
    }

    /// The game writing its own fields of the entity, as `player_die` does.
    pub fn state_mut(&mut self) -> &mut EntityState {
        &mut self.state
    }

    /// `ClientSpawn` before its think: the player's state was cleared, and with it the
    /// count of shown events; `SetClientViewAngle` writes the entity's angles; outside
    /// siege every player is on the team NPCs take for the player's.
    pub fn spawned(&mut self, angles: [f32; 3]) {
        self.shown_events = 0;
        for (index, angle) in ES_ANGLES.into_iter().zip(angles) {
            self.state.set_raw_field(index, angle.to_bits());
        }
        self.state.set_raw_field(ES_TEAM_OWNER, NPCTEAM_PLAYER);
    }

    /// `SV_LinkEntity` for a box that is no brush model: its size goes into `solid`, which
    /// is all a client's prediction knows of it — half its width, its depth below the
    /// origin, its height above it plus 32, a byte each (`sv_world.cpp:263-286`). An
    /// entity made of nothing solid has none.
    pub fn linked(&mut self, bounds: ([f32; 3], [f32; 3]), solid: bool) {
        let byte = |value: f32| (value as i32).clamp(1, 255) as u32;
        let packed = byte(bounds.1[2] + 32.0) << 16 | byte(-bounds.0[2]) << 8 | byte(bounds.1[0]);
        self.state
            .set_raw_field(ES_SOLID, if solid { packed } else { 0 });
    }

    /// `G_AddEvent` on this player at `level_time`: the event itself went into the player's
    /// state (its external event); the entity remembers when, so that the frames clear
    /// it `EVENT_VALID_MSEC` on.
    pub fn event_raised(&mut self, level_time: i32) {
        self.event_time = level_time;
    }

    /// The start of a server frame (`G_RunFrame`, `g_main.c:3080-3090`): an event shown
    /// for longer than `EVENT_VALID_MSEC` is taken off the entity, and the player's
    /// external event with it.
    pub fn frame_began(&mut self, player: &mut PlayerState, level_time: i32) {
        if level_time - self.event_time > EVENT_VALID_MS
            && self.state.raw_field(ES_EVENT) != Some(0)
        {
            self.state.set_raw_field(ES_EVENT, 0);
            player.set_raw_field(PS_EXTERNAL_EVENT, 0);
        }
    }

    /// The end of a command that ran (`ClientThink_real`, `g_active.c:3322-3339`), at
    /// the server frame's `level_time`; `events_before` is the player's `eventSequence`
    /// as the command began. A spectator's entity is only given its origin
    /// (`SpectatorThink`, `:735`) — a field nothing reads for a player, which keeps it
    /// from then on.
    ///
    /// Returns the temp entity for an event the entity could not carry, see
    /// [`Self::frame_ended`].
    pub fn thought(
        &mut self,
        player: &PlayerState,
        spectator: bool,
        level_time: i32,
        events_before: u32,
    ) -> Option<&EntityState> {
        if player.raw_field(PS_EVENT_SEQUENCE) != Some(events_before) && !spectator {
            self.event_time = level_time;
        }
        if spectator {
            for (to, from) in ES_ORIGIN.into_iter().zip(PS_ORIGIN) {
                self.state
                    .set_raw_field(to, player.raw_field(from).unwrap_or(0));
            }
            return None;
        }
        self.extrapolated(player)
    }

    /// `ClientEndFrame`, once per server frame: a player is converted again; a spectator
    /// is not touched.
    ///
    /// An entity carries one event per conversion. If the player has raised another,
    /// `SendPendingPredictableEvents` (`g_active.c:1079-1106`) sends it at once in a temp
    /// entity — the player converted once more, snapped, into an event entity that names
    /// the player in `otherEntityNum` — for everyone but the player, who predicted it.
    /// That entity is returned, without a number: whoever allocates it numbers it.
    pub fn frame_ended(&mut self, player: &PlayerState, spectator: bool) -> Option<&EntityState> {
        if spectator {
            return None;
        }
        self.extrapolated(player)
    }

    /// The last act of `ClientSpawn` (`g_client.c:3837`): the plain conversion, snapped.
    pub fn spawn_finished(&mut self, player: &PlayerState) {
        player_entity_state(
            player,
            &mut self.shown_events,
            PlayerEntityMotion::Interpolated,
            true,
            &mut self.state,
        );
    }

    fn extrapolated(&mut self, player: &PlayerState) -> Option<&EntityState> {
        extrapolate(
            player,
            &mut self.shown_events,
            &mut self.state,
            &mut self.overflow,
        )
        .then_some(&self.overflow)
    }
}

/// `BG_PlayerStateToEntityStateExtraPolate` from the command time, then
/// `SendPendingPredictableEvents` (`g_active.c:1079-1106`): `entity` converted, and — if the
/// player raised an event the entity could not carry — `overflow` made the temp entity that
/// carries it. Returns whether it did. What [`PlayerEntity`] does for a player, and what an
/// NPC's think does for its `ET_NPC` entity (which its caller types back).
pub fn extrapolate(
    player: &PlayerState,
    shown_events: &mut i32,
    entity: &mut EntityState,
    overflow: &mut EntityState,
) -> bool {
    let time = player.raw_field(PS_COMMAND_TIME).unwrap_or(0) as i32;
    player_entity_state(
        player,
        shown_events,
        PlayerEntityMotion::Extrapolated { time },
        false,
        entity,
    );
    if *shown_events >= player.raw_field(PS_EVENT_SEQUENCE).unwrap_or(0) as i32 {
        return false;
    }
    let read = |index: usize| player.raw_field(index).unwrap_or(0);
    let slot = (*shown_events & (MAX_PS_EVENTS - 1)) as usize;
    let event = read(PS_EVENTS[slot]) | ((*shown_events & 3) as u32) << 8;
    // `G_TempEntity` spawns a cleared entity; all it sets itself is written over.
    for index in 0..sjk_protocol::LEGACY_ENTITY_FIELDS.len() {
        overflow.set_raw_field(index, 0);
    }
    convert(
        player,
        shown_events,
        PlayerEntityMotion::Interpolated,
        true,
        overflow,
        false,
    );
    overflow.set_raw_field(ES_TYPE, ET_EVENTS + event);
    let flags = overflow.raw_field(ES_EFLAGS).unwrap_or(0);
    overflow.set_raw_field(ES_EFLAGS, flags | EF_PLAYER_EVENT);
    overflow.set_raw_field(ES_OTHER_ENTITY_NUM, read(PS_CLIENT_NUM));
    true
}
