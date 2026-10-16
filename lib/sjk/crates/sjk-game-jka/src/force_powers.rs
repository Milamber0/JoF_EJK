//! The Force powers a player uses on itself, as `WP_ForcePowersUpdate` runs them every
//! frame (`w_force.c:4969-5480`): the power a command selects (`PmoveSingle`,
//! `bg_pmove.c:11072-11079`), the Force button and the release it waits for
//! (`WP_DoSpecificPower`), each power's start, run, duration and stop
//! (`WP_ForcePowerStart`, `WP_ForcePowerRun`, `WP_ForcePowerStop`), the sounds that go
//! with them and the looping ones a client tracks (`G_Sound` on a tracking channel,
//! `G_MuteSound`), and the pool's regeneration.
//!
//! Ported here: heal, speed, seeing, protection, absorption and rage — each held against
//! `tools/game-oracle/forceself.c` — and everything of the update that is not one power:
//! the levitation every player knows, the selection's bounds, powers stopped by
//! ysalamiri or by what `BG_CanUseFPNow` forbids, enlightenment, the Force-jump sound,
//! the speed powerup, a dead player's powers, the regeneration and the boon.
//!
//! The powers used on others run here too: push and pull through the caller
//! ([`crate::force_throw`]); grip, lightning, drain ([`crate::force_dark`]) and mind trick
//! ([`crate::force_trick`]) through the frame's [`crate::force_dark::OtherPlayers`].
//!
//! Team heal and replenish reach teammates the same way ([`crate::force_team`]).
//!
//! Not yet: the
//! levitation charge the button does with levitation selected; the holocron,
//! siege and power-duel regeneration rules (Jedi Master's are [`crate::jedi_master`]'s); and the saber's own Force restrictions
//! (`saberFlags`, `forceRestrictions`), which need the saber definitions.

use crate::entity_id::EntityId;
use crate::entity_pool::EF_SOUNDTRACKER;
/// `MAX_CLIENTS`: a tracking channel names only an entity past the clients.
const MAX_CLIENTS: u16 = 32;
use crate::event_entity::EventEntity;
use crate::knockdown::predef_sound;
use crate::pmove::MovementState;
use sjk_protocol::{PlayerState, UserCommand};

/// `NUM_FORCE_POWERS` and the powers by number (`forcePowers_t`).
pub const NUM_FORCE_POWERS: usize = 18;
pub const FP_HEAL: usize = 0;
pub const FP_LEVITATION: usize = 1;
pub const FP_SPEED: usize = 2;
pub const FP_PUSH: usize = 3;
pub const FP_PULL: usize = 4;
pub const FP_TELEPATHY: usize = 5;
pub const FP_GRIP: usize = 6;
pub const FP_LIGHTNING: usize = 7;
pub const FP_RAGE: usize = 8;
pub const FP_PROTECT: usize = 9;
pub const FP_ABSORB: usize = 10;
pub const FP_TEAM_HEAL: usize = 11;
pub const FP_TEAM_FORCE: usize = 12;
pub const FP_DRAIN: usize = 13;
pub const FP_SEE: usize = 14;
pub const FP_SABER_OFFENSE: usize = 15;
pub const FP_SABER_DEFENSE: usize = 16;
pub const FP_SABER_THROW: usize = 17;

/// `forcePowerNeeded` (`bg_pmove.c:89-174`): each power's cost by level.
pub const FORCE_POWER_NEEDED: [[i32; NUM_FORCE_POWERS]; 4] = [
    [999; NUM_FORCE_POWERS],
    [
        65, 10, 50, 20, 20, 20, 30, 1, 50, 50, 50, 50, 50, 20, 20, 0, 2, 20,
    ],
    [
        60, 10, 50, 20, 20, 20, 30, 1, 50, 25, 25, 33, 33, 20, 20, 0, 1, 20,
    ],
    [
        50, 10, 50, 20, 20, 20, 60, 1, 50, 10, 10, 25, 25, 20, 20, 0, 0, 20,
    ],
];

/// `forcePowerDarkLight` (`bg_misc.c:222-243`): the side a power belongs to, 0 for both.
pub const FORCE_POWER_SIDES: [u8; NUM_FORCE_POWERS] =
    [1, 0, 0, 0, 0, 1, 2, 2, 2, 1, 1, 1, 2, 2, 0, 0, 0, 0];

/// `FORCE_POWER_MAX`.
pub const FORCE_POWER_MAX: i32 = 100;

// The player state's fields the update reads and writes.
const PS_VELOCITY_Z: usize = 8;
const PS_LEGS_ANIM: usize = 13;
const PS_TORSO_ANIM: usize = 15;
const PS_GROUND_ENTITY: usize = 16;
const PS_EFLAGS: usize = 17;
const PS_FORCE_POWER: usize = 18;
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_MOVE: usize = 34;
const PS_WEAPON: usize = 47;
const PS_KNOWN: usize = 51;
const PS_LEVITATION_LEVEL: usize = 52;
/// `fd.forcePowerDebounce[FP_LEVITATION]`, which the wire carries.
const PS_LEVITATION_DEBOUNCE: usize = 53;
pub(crate) const PS_SELECTED: usize = 54;
const PS_JUMP_Z_START: usize = 74;
const PS_ACTIVE: usize = 82;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_ACTIVE_FORCE_PASS: usize = 72;
const PS_ROCKET_LOCK_INDEX: usize = 24;
const PS_ROCKET_LOCK_TIME: usize = 79;
const PS_ROCKET_TARGET_TIME: usize = 71;
const PS_VEHICLE: usize = 84;
const PS_SABER_IN_FLIGHT: usize = 88;
/// `ps.saberHolstered`, `ps.weaponTime`.
const PS_SABER_HOLSTERED: usize = 81;
const PS_WEAPON_TIME: usize = 10;
const PS_BROKEN_LIMBS: usize = 93;
const PS_RAGE_RECOVERY: usize = 96;
const PS_FALLING_TO_DEATH: usize = 97;
const PS_MINDTRICK: [usize; 4] = [98, 99, 101, 104];
const PS_SABER_LOCK_TIME: usize = 107;
const PS_SABER_LOCK_FRAME: usize = 108;
const PS_SEE_LEVEL: usize = 109;
const PS_GRIP_CRIPPLE: usize = 111;
const PS_FORCE_RESTRICTED: usize = 115;
const PS_TRUE_NON_JEDI: usize = 116;
const PS_DUEL_IN_PROGRESS: usize = 119;
/// The fields an update may change that the movement keeps a copy of: when one changed,
/// the movement is reseeded from the state.
const MOVEMENT_FIELDS: [usize; 21] = [
    PS_FORCE_POWER,
    PS_SABER_ANIM_LEVEL,
    PS_KNOWN,
    PS_LEVITATION_LEVEL,
    PS_SELECTED,
    PS_ACTIVE,
    PS_RAGE_RECOVERY,
    PS_SEE_LEVEL,
    PS_GRIP_CRIPPLE,
    PS_LEGS_TIMER,
    PS_TORSO_TIMER,
    PS_MINDTRICK[0],
    PS_MINDTRICK[1],
    PS_EFLAGS,
    PS_FORCE_HAND_EXTEND,
    PS_ACTIVE_FORCE_PASS,
    PS_ROCKET_LOCK_INDEX,
    PS_ROCKET_LOCK_TIME,
    PS_ROCKET_TARGET_TIME,
    // A gripped user's saber turned off (`Cmd_ToggleSaber_f`).
    PS_SABER_HOLSTERED,
    PS_WEAPON_TIME,
];
const STAT_HEALTH: usize = 0;
const STAT_MAX_HEALTH: usize = 8;
const EF_DEAD: u32 = 1 << 1;
const PW_REDFLAG: usize = 4;
const PW_BLUEFLAG: usize = 5;
const PW_DISINT_4: usize = 9;
const PW_SPEED: usize = 10;
const PW_FORCE_ENLIGHTENED_LIGHT: usize = 12;
const PW_FORCE_ENLIGHTENED_DARK: usize = 13;
const PW_FORCE_BOON: usize = 14;
const PW_YSALAMIRI: usize = 15;
const GT_CTY: i32 = 9;
const GT_SIEGE: i32 = 7;
const WP_SABER: u32 = 3;
const WP_EMPLACED_GUN: u32 = 17;
const BROKENLIMB_ARMS: u32 = (1 << 3) | (1 << 4);
const ENTITY_NUMBER_NONE: u32 = 1_023;
pub(crate) const BUTTON_FORCEPOWER: u16 = 512;
/// `BUTTON_FORCEGRIP`, `BUTTON_FORCE_LIGHTNING`, `BUTTON_FORCE_DRAIN`: the dark powers'
/// own buttons.
const BUTTON_FORCEGRIP: u16 = 64;
const BUTTON_FORCE_LIGHTNING: u16 = 1_024;
const BUTTON_FORCE_DRAIN: u16 = 2_048;
/// `CHAN_AUTO`, `CHAN_VOICE`, `CHAN_ITEM`, `CHAN_BODY`, and the tracking channels.
const CHAN_AUTO: u32 = 0;
const CHAN_VOICE: u32 = 3;
const CHAN_ITEM: u32 = 5;
const CHAN_BODY: u32 = 6;
const TRACK_CHANNEL_NONE: u32 = 50;
const TRACK_CHANNEL_2: u32 = 52;
const TRACK_CHANNEL_3: u32 = 53;
const TRACK_CHANNEL_4: u32 = 54;
const TRACK_CHANNEL_5: u32 = 55;
/// `PDSOUND_PROTECT`, `PDSOUND_ABSORB`, `PDSOUND_FORCEJUMP`, `PDSOUND_FORCEGRIP`.
const PDSOUND_PROTECT: u32 = 2;
const PDSOUND_ABSORB: u32 = 4;
const PDSOUND_FORCEJUMP: u32 = 5;
const PDSOUND_FORCEGRIP: u32 = 6;
const EV_GENERAL_SOUND: u32 = 76;
const EV_MUTE_SOUND: u32 = 74;
const ES_EFLAGS: usize = 19;
const ES_SABER_ENTITY: usize = 37;
const ES_TRICKED: usize = 58;
const ES_TRICKED2: usize = 74;

/// The Force a player holds that no client is sent: `forcedata_t`'s levels, durations,
/// debounces and bookkeeping, and the client's fields the powers keep.
#[derive(Clone, Debug, PartialEq)]
pub struct ForcePowers {
    /// `forcePowerLevel`: every power's level (levitation's and seeing's are on the wire
    /// too, and follow these).
    pub levels: [u8; NUM_FORCE_POWERS],
    /// `forcePowerBaseLevel`: the levels enlightenment raised them from.
    pub base_levels: [u8; NUM_FORCE_POWERS],
    /// `forcePowerDuration`: when a timed power runs out, 0 for none.
    pub duration: [i32; NUM_FORCE_POWERS],
    /// `forcePowerDebounce`: when a power next costs or may be used again (levitation's
    /// is on the wire, and the movement keeps it).
    pub debounce: [i32; NUM_FORCE_POWERS],
    /// `forcePowerRegenDebounceTime`: when the pool next regenerates a point.
    pub regen_debounce: i32,
    /// `forceButtonNeedRelease`: the Force button must be let go before another use.
    pub button_need_release: bool,
    /// `forceAllowDeactivateTime`: from when pressing a timed power again switches it off.
    pub allow_deactivate_time: i32,
    /// `forceRageDrainTime`: when rage next costs health.
    pub rage_drain_time: i32,
    /// `forceHealTime`, `forceHealAmount`: the held heal's pace and what it has healed.
    pub heal_time: i32,
    pub heal_amount: i32,
    /// `forcePowerMax`.
    pub max: i32,
    /// `forceUsingAdded`: enlightenment raised the levels.
    pub using_added: bool,
    /// `killSoundEntIndex`: the sound tracker on each tracking channel, if any.
    pub kill_sounds: [Option<EntityId>; 6],
    /// `forceDeactivateAll`: every power is to stop at the next update.
    pub deactivate_all: bool,
    /// The held sabers' `forceRestrictions`, one bit a power: forbidden while a blade is
    /// lit ([`crate::player_sabers::PlayerSabers::force_restrictions`]). Not `forcedata_t`;
    /// the game keeps it up to date from the sabers.
    pub saber_restrictions: u32,
    /// `forceGripBeingGripped` (a float in the game): until when someone grips this
    /// player.
    pub grip_being_gripped: f32,
    /// `forceGripSoundTime` (a float): when the gripped player's choke is next heard.
    pub grip_sound_time: f32,
    /// `forceGripEntityNum`: whom this player grips, `ENTITYNUM_NONE` for nobody.
    pub grip_entity: u16,
    /// `forceGripStarted` (a float): when this player was last gripped.
    pub grip_started: f32,
    /// `forceGripDamageDebounceTime`: 1 once the crushing blow has landed.
    pub grip_damage_debounce: i32,
    /// `forceGripUseTime`: no grip before this time.
    pub grip_use_time: i32,
    /// `forceGripMoveInterval`: when a grip next sets this player's velocity.
    pub grip_move_interval: i32,
    /// `forceGripChangeMovetype`: the movement type a grip holds this player in, 0 for
    /// none.
    pub grip_movement_type: u8,
    /// `force.lightningDebounce`, `force.drainDebounce`: when the next shot is due.
    pub lightning_debounce: i32,
    pub drain_debounce: i32,
    /// `forceDrainEntNum`, `forceDrainTime` (a float): whom this one's drain last held,
    /// and until when — `ENTITYNUM_NONE` and 0 from `WP_InitForcePowers`.
    pub drain_entity: u16,
    pub drain_time: f32,
    /// `noLightningTime`: until when lightning feeds this player's pool rather than
    /// hurting it (absorption's respite).
    pub no_lightning_time: i32,
    /// `dangerTime`: when the player last attacked (fired, swung, used a power on
    /// someone), which frees the players its mind trick holds who see it.
    pub danger_time: i32,
    /// `forcePowerSoundDebounce`: when a protected or absorbing player's hit is next
    /// heard.
    pub sound_debounce: i32,
    /// `ps.otherSoundTime`, `ps.otherSoundLen`: until when a power it began is heard, and
    /// how far (for the bots' hearing only; no sound is played by it).
    pub other_sound_time: i32,
    pub other_sound_len: f32,
    /// The holocrons carried, in Holocron FFA.
    pub holocrons: crate::holocron::Carried,
    /// `saberAnimLevelBase` as `HolocronUpdate` last reset it, for the movement (which
    /// keeps it) to take up.
    pub saber_base_reset: Option<u8>,
    /// `fd.forceJumpCharge`: a Force jump charged, which the update jumps once the jump
    /// key is up ([`crate::force_jump`]). Only an NPC's AI charges one.
    pub jump_charge: f32,
    /// `client->fjDidJump`: it Force-jumped and has not been on the ground since.
    pub jumped: bool,
    /// `ps.forceJumpFlip`: the jump's flip, for its next move to show.
    pub jump_flip: bool,
}

impl ForcePowers {
    /// A player's Force with these levels and nothing running.
    pub fn with_levels(levels: [u8; NUM_FORCE_POWERS]) -> Self {
        Self {
            levels,
            base_levels: [0; NUM_FORCE_POWERS],
            duration: [0; NUM_FORCE_POWERS],
            debounce: [0; NUM_FORCE_POWERS],
            regen_debounce: 0,
            button_need_release: false,
            allow_deactivate_time: 0,
            rage_drain_time: 0,
            heal_time: 0,
            heal_amount: 0,
            max: FORCE_POWER_MAX,
            using_added: false,
            kill_sounds: [None; 6],
            deactivate_all: false,
            saber_restrictions: 0,
            grip_being_gripped: 0.0,
            grip_sound_time: 0.0,
            grip_entity: ENTITY_NUMBER_NONE as u16,
            grip_started: 0.0,
            grip_damage_debounce: 0,
            grip_use_time: 0,
            grip_move_interval: 0,
            grip_movement_type: 0,
            lightning_debounce: 0,
            drain_debounce: 0,
            drain_entity: ENTITY_NUMBER_NONE as u16,
            drain_time: 0.0,
            no_lightning_time: 0,
            danger_time: 0,
            sound_debounce: 0,
            other_sound_time: 0,
            other_sound_len: 0.0,
            holocrons: crate::holocron::Carried::default(),
            saber_base_reset: None,
            jump_charge: 0.0,
            jumped: false,
            jump_flip: false,
        }
    }

    /// `WP_SpawnInitForcePowers` (`w_force.c:423-541`) for what no client is sent, at a
    /// spawn outside siege and holocron: the levels the player spawns with, every timer
    /// cleared, the pool regenerating from now. The sound trackers are kept
    /// (`killSoundEntIndex` survives a respawn); the spawn's state has already stopped
    /// every power and filled the pool.
    pub fn respawned(&mut self, levels: [u8; NUM_FORCE_POWERS], level_time: i32) {
        let kill_sounds = self.kill_sounds;
        *self = Self::with_levels(levels);
        self.kill_sounds = kill_sounds;
        self.regen_debounce = level_time;
    }
}

/// What an update is run in: the time, the game type, the server's regeneration pace,
/// the player's number, team, health (the game's), hand and spawn protection, and
/// everyone else — with the entities its sounds become and the sound table
/// ([`crate::force_dark::OtherPlayers`]).
pub struct ForceFrame<'a> {
    pub level_time: i32,
    pub gametype: i32,
    /// `g_forceRegenTime`: milliseconds a point takes to come back.
    pub regen_time: i32,
    /// `g_TimeSinceLastFrame`: how long the last frame was.
    pub since_last_frame: i32,
    pub client: u16,
    /// The user is an NPC (its blows are an NPC's, `G_Damage`'s NPC branches).
    pub npc: bool,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// The style the player's sabers hold it to without the saber-attack holocron
    /// ([`crate::player_sabers::PlayerSabers::base_style`]), and whether the server is
    /// saber-only (`HasSetSaberOnly`): Holocron FFA's.
    pub saber_style: u8,
    pub saber_only: bool,
    /// A power duel's lone: its own regeneration pace
    /// ([`crate::power_duel::lone_regen_time`]), added as the reference adds a double.
    pub lone_regen: Option<f64>,
    pub health: &'a mut i32,
    /// `forceHandExtendTime` (the knockdown's memory keeps it).
    pub hand_extend_time: &'a mut i32,
    /// `invulnerableTimer`: until when the spawn's protection lasts.
    pub invulnerable_until: &'a mut i32,
    /// The other players, whom grip, lightning and drain reach.
    pub others: &'a mut dyn crate::force_dark::OtherPlayers,
    /// The sounds the user's `Cmd_ToggleSaber_f` plays turning its sabers off, each hand's
    /// (`saber[n].soundOff`): a gripped user's saber is kept off.
    pub saber_off_sounds: [SaberOffSound; 2],
}

/// A hand's off-sound as `Cmd_ToggleSaber_f` plays it (`g_cmds.c:2720-2730`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaberOffSound {
    /// None plays (no index, or no saber in the second hand).
    Silent,
    /// The saber's registered index.
    Index(u16),
    /// A sound registered by name when it first plays.
    Named(&'static [u8]),
}

/// A player and its Force for the length of an update.
pub(crate) struct Forcer<'s, 'f> {
    pub(crate) state: &'s mut PlayerState,
    pub(crate) force: &'s mut ForcePowers,
    pub(crate) frame: &'s mut ForceFrame<'f>,
    /// A push or pull the button called for, which the caller carries out.
    throw: Option<bool>,
    /// A gripped user's thrown saber was turned off in the air, which the caller carries
    /// out ([`Begun::saber_knocked_down`]).
    saber_knocked: bool,
    /// The command's buttons, which keep lightning and drain going.
    pub(crate) buttons: u16,
}

/// `PmoveSingle`'s selections (`bg_pmove.c:11072-11079`): the power a command names, if
/// the player knows it, and the item, if it holds it.
pub(crate) fn select(state: &mut MovementState, command: &UserCommand) {
    let power = command.force_selection;
    if power != 0xFF && state.force_powers_known & (1u32 << (power & 31)) != 0 {
        state.force_power_selected = power;
    }
    let item = command.inventory_selection;
    if item != 0xFF && state.holdable_items & (1u32 << (item & 31)) != 0 {
        // `BG_GetItemIndexByTag(invensel, IT_HOLDABLE)`.
        if let Some(index) = crate::items::ITEMS
            .iter()
            .position(|row| row.kind == crate::items::Kind::Holdable && row.tag == i32::from(item))
        {
            state.holdable_item = index as u32;
        }
    }
}

/// The first half of `WP_ForcePowersUpdate`, through the Force button: what
/// [`finish`] carries on from, and the push or pull the button called for (`ForceThrow`,
/// which reaches other players: the caller runs it, [`crate::force_throw::throw`], before
/// finishing).
#[derive(Clone, Copy, Debug)]
pub struct Begun {
    /// `Some(pull)`: `ForceThrow(self, pull)` is due.
    pub throw: Option<bool>,
    /// The user, gripped, had its saber in flight: `Cmd_ToggleSaber_f` turns it off in the
    /// air (`saberKnockDown`), which is the caller's (`g_cmds.c:2690-2700`).
    pub saber_knocked_down: bool,
    prepower: i32,
    alive: bool,
    buttons: u16,
    before: [u32; MOVEMENT_FIELDS.len()],
    health_before: u32,
    powerups_before: [u32; 16],
}

/// `WP_ForcePowersUpdate` for a playing client, after its knockdown part
/// ([`crate::knockdown::force_update`]), up to and including the Force button:
/// `jump_sound` is the Force jump the movement began since the last update
/// (`fd.forceJumpSound`).
pub fn begin(
    state: &mut PlayerState,
    force: &mut ForcePowers,
    command: &UserCommand,
    jump_sound: bool,
    frame: &mut ForceFrame,
) -> Begun {
    let before = MOVEMENT_FIELDS.map(|field| state.raw_field(field).unwrap_or(0));
    let (health_before, powerups_before) = (state.stats[STAT_HEALTH], state.powerups);
    let mut forcer = Forcer {
        state,
        force,
        frame,
        throw: None,
        saber_knocked: false,
        buttons: command.buttons,
    };
    let (prepower, alive) = forcer.begin(command, jump_sound);
    Begun {
        throw: forcer.throw,
        saber_knocked_down: forcer.saber_knocked,
        prepower,
        alive,
        buttons: command.buttons,
        before,
        health_before,
        powerups_before,
    }
}

/// The rest of `WP_ForcePowersUpdate` after [`begin`] (and the throw it called for): the
/// powers' durations and runs, the regeneration and the boon. Returns whether the
/// movement's copy of the state changed over the whole update, so that it is reseeded.
pub fn finish(
    state: &mut PlayerState,
    force: &mut ForcePowers,
    begun: Begun,
    frame: &mut ForceFrame,
) -> bool {
    let mut forcer = Forcer {
        state,
        force,
        frame,
        throw: None,
        saber_knocked: false,
        buttons: begun.buttons,
    };
    forcer.finish(begun.prepower, begun.alive);
    let state = &*forcer.state;
    MOVEMENT_FIELDS.map(|field| state.raw_field(field).unwrap_or(0)) != begun.before
        || state.stats[STAT_HEALTH] != begun.health_before
        || state.powerups[..] != begun.powerups_before[..]
}

/// A Force power's own key (`GENCMD_FORCE_*`, `g_active.c:3135-3167`): the power's
/// function straight, with no button and no release to wait for. Returns the push or pull
/// due (`Some(pull)`, `ForceThrow`), which the caller runs; the keys of the powers not yet
/// ported (mind trick, the team powers) do nothing.
pub fn key(
    state: &mut PlayerState,
    force: &mut ForcePowers,
    power: usize,
    frame: &mut ForceFrame,
) -> Option<bool> {
    let mut forcer = Forcer {
        state,
        force,
        frame,
        throw: None,
        saber_knocked: false,
        buttons: 0,
    };
    match power {
        FP_HEAL => forcer.heal(),
        FP_SPEED | FP_SEE => forcer.timed(power),
        FP_RAGE | FP_PROTECT | FP_ABSORB => forcer.guard(power),
        FP_PUSH | FP_PULL => return Some(power == FP_PULL),
        FP_TELEPATHY => forcer.telepathy(),
        FP_TEAM_HEAL | FP_TEAM_FORCE => forcer.team_power(power),
        _ => {}
    }
    None
}

/// The levels a client is sent: levitation's and seeing's.
pub(crate) fn mirror_levels(state: &mut PlayerState, force: &ForcePowers) {
    state.set_raw_field(PS_LEVITATION_LEVEL, u32::from(force.levels[FP_LEVITATION]));
    state.set_raw_field(PS_SEE_LEVEL, u32::from(force.levels[FP_SEE]));
}

/// `ClientDisconnect`'s Force (`g_client.c:3913-3931`): every power on stopped — a grip
/// lets its victim go — and every looping sound still tracked muted.
pub fn disconnect(state: &mut PlayerState, force: &mut ForcePowers, frame: &mut ForceFrame) {
    let mut forcer = Forcer {
        state,
        force,
        frame,
        throw: None,
        saber_knocked: false,
        buttons: 0,
    };
    for power in 0..NUM_FORCE_POWERS {
        if forcer.active(power) {
            forcer.stop(power);
        }
    }
    for slot in 1..forcer.force.kill_sounds.len() {
        let tracker = forcer.force.kill_sounds[slot];
        if tracker.is_some() {
            forcer.mute(tracker, CHAN_VOICE);
        }
    }
}

impl<'s, 'f> Forcer<'s, 'f> {
    /// A client and its Force outside an update: a power its AI (an NPC's) or a key uses
    /// directly.
    pub(crate) fn new(
        state: &'s mut PlayerState,
        force: &'s mut ForcePowers,
        frame: &'s mut ForceFrame<'f>,
    ) -> Self {
        Self {
            state,
            force,
            frame,
            throw: None,
            saber_knocked: false,
            buttons: 0,
        }
    }
}

impl Forcer<'_, '_> {
    /// `Cmd_ToggleSaber_f(self)` on a gripped user's lit saber: off, with each hand's
    /// off-sound (`G_Sound(ent, CHAN_AUTO, soundOff)`), or knocked down in flight.
    fn toggle_saber_off(&mut self) {
        let mut sounds = Vec::new();
        if crate::generic_commands::toggle_saber(self.state, self.frame.level_time, &mut sounds) {
            self.saber_knocked = true;
        }
        let origin = self.origin();
        for sound in sounds.into_iter().filter(|sound| !sound.on) {
            let index = match self.frame.saber_off_sounds[usize::from(sound.hand)] {
                SaberOffSound::Silent => continue,
                SaberOffSound::Index(index) => index,
                SaberOffSound::Named(name) => self.frame.others.sound_index(name),
            };
            self.raise(crate::weapon_fire::sound_event(origin, CHAN_AUTO, index));
        }
    }

    pub(crate) fn field(&self, index: usize) -> u32 {
        self.state.raw_field(index).unwrap_or(0)
    }

    pub(crate) fn active(&self, power: usize) -> bool {
        self.field(PS_ACTIVE) & (1 << power) != 0
    }

    fn set_active(&mut self, power: usize, on: bool) {
        let active = self.field(PS_ACTIVE);
        self.state.set_raw_field(
            PS_ACTIVE,
            if on {
                active | (1 << power)
            } else {
                active & !(1 << power)
            },
        );
    }

    pub(crate) fn pool(&self) -> i32 {
        self.field(PS_FORCE_POWER) as i32
    }

    fn set_pool(&mut self, amount: i32) {
        self.state.set_raw_field(PS_FORCE_POWER, amount as u32);
    }

    pub(crate) fn origin(&self) -> [f32; 3] {
        self.state.origin()
    }

    /// Everything up to and including the button; returns the boon's starting pool and
    /// whether the player is alive (the dead stop their powers and skip the rest).
    fn begin(&mut self, command: &UserCommand, jump_sound: bool) -> (i32, bool) {
        let level_time = self.frame.level_time;
        if self.field(PS_SABER_ANIM_LEVEL) == 0 {
            self.state.set_raw_field(PS_SABER_ANIM_LEVEL, 1);
        }
        if self.frame.gametype != GT_SIEGE {
            let known = self.field(PS_KNOWN);
            self.state
                .set_raw_field(PS_KNOWN, known | (1 << FP_LEVITATION));
            if self.force.levels[FP_LEVITATION] < 1 {
                self.force.levels[FP_LEVITATION] = 1;
                self.state.set_raw_field(PS_LEVITATION_LEVEL, 1);
            }
        }
        if self.field(PS_SELECTED) as usize >= NUM_FORCE_POWERS {
            self.state.set_raw_field(PS_SELECTED, 0);
        }
        if self.frame.gametype == crate::holocron::GT_HOLOCRON {
            self.holocron_update();
        }
        if self.frame.gametype == crate::jedi_master::GT_JEDIMASTER {
            self.jedi_master_update();
        }
        let prepower = if self.state.powerups[PW_FORCE_BOON] != 0 {
            self.pool()
        } else {
            0
        };
        if self.has_ysalamiri() || self.force.deactivate_all {
            for power in 0..NUM_FORCE_POWERS {
                if self.active(power) && power != FP_LEVITATION {
                    self.stop(power);
                }
            }
            self.force.deactivate_all = false;
        } else {
            for power in 0..NUM_FORCE_POWERS {
                if self.active(power) && power != FP_LEVITATION && !self.can_use_now(power) {
                    self.stop(power);
                }
            }
        }
        self.enlightenment();
        if !self.active(FP_TELEPATHY) {
            for field in PS_MINDTRICK {
                self.state.set_raw_field(field, 0);
            }
        }
        if *self.frame.health < 1 {
            self.force.grip_being_gripped = 0.0;
        }
        // Gripped, the player's saber is kept off (`w_force.c:5256-5264`).
        let crippled = self.force.grip_being_gripped > level_time as f32;
        self.state
            .set_raw_field(PS_GRIP_CRIPPLE, u32::from(crippled));
        if crippled && self.field(PS_WEAPON) == WP_SABER && self.field(PS_SABER_HOLSTERED) == 0 {
            self.toggle_saber_off();
        }
        if jump_sound {
            self.raise(predef_sound(self.origin(), PDSOUND_FORCEJUMP));
        }
        if crippled && self.force.grip_sound_time < level_time as f32 {
            self.raise(predef_sound(self.origin(), PDSOUND_FORCEGRIP));
            self.force.grip_sound_time = (level_time + 1_000) as f32;
        }
        if self.active(FP_SPEED) {
            self.state.powerups[PW_SPEED] = (level_time + 100) as u32;
        }
        if *self.frame.health <= 0 {
            for power in 0..NUM_FORCE_POWERS {
                if self.force.duration[power] != 0 || self.active(power) {
                    self.stop(power);
                    self.force.duration[power] = 0;
                }
            }
            return (prepower, false);
        }
        self.charged_jump(command);
        self.buttons(command);
        (prepower, true)
    }

    /// The durations, the runs and the regeneration for the living, then the boon.
    fn finish(&mut self, prepower: i32, alive: bool) {
        let level_time = self.frame.level_time;
        if alive {
            for power in 0..NUM_FORCE_POWERS {
                if self.force.duration[power] != 0 && self.force.duration[power] < level_time {
                    if self.active(power) {
                        self.stop(power);
                    }
                    self.force.duration[power] = 0;
                }
                if self.active(power) {
                    self.run(power);
                }
            }
            if !self.active(FP_DRAIN) {
                self.force.drain_debounce = level_time;
            }
            if !self.active(FP_LIGHTNING) {
                self.force.lightning_debounce = level_time;
            }
            self.regenerate();
        }
        if prepower != 0 && self.pool() < prepower {
            // The boon halves what the frame cost.
            let difference = ((prepower - self.pool()) / 2).max(1);
            self.set_pool(prepower - difference);
        }
    }

    /// The dark powers' own buttons, each held or its power stopped unless the Force
    /// button holds it (`w_force.c:5338-5381`), then the Force button (`w_force.c:5383-5393`).
    fn buttons(&mut self, command: &UserCommand) {
        for (power, button) in [
            (FP_GRIP, BUTTON_FORCEGRIP),
            (FP_LIGHTNING, BUTTON_FORCE_LIGHTNING),
            (FP_DRAIN, BUTTON_FORCE_DRAIN),
        ] {
            if command.buttons & button != 0 {
                self.do_specific_power(power);
            } else if self.active(power)
                && (command.buttons & BUTTON_FORCEPOWER == 0
                    || self.field(PS_SELECTED) as usize != power)
            {
                self.stop(power);
            }
        }
        let selected = self.field(PS_SELECTED) as usize;
        if command.buttons & BUTTON_FORCEPOWER != 0 && self.can_use_now(selected) {
            if selected != FP_LEVITATION {
                self.do_specific_power(selected);
            }
        } else {
            self.force.button_need_release = false;
        }
    }

    /// The pool regenerates a point every `g_forceRegenTime` while no power but drain is
    /// on and no saber is thrown or in a special move.
    fn regenerate(&mut self) {
        let level_time = self.frame.level_time;
        let active = self.field(PS_ACTIVE);
        let special = self.field(PS_WEAPON) == WP_SABER
            && crate::saber_rules::in_special(self.field(PS_SABER_MOVE));
        if (active == 0 || active == 1 << FP_DRAIN)
            && self.field(PS_SABER_IN_FLIGHT) == 0
            && !special
        {
            while self.force.regen_debounce < level_time {
                // "jedi master regenerates 4 times as fast".
                let master = self.frame.gametype == crate::jedi_master::GT_JEDIMASTER
                    && crate::jedi_master::is_master(self.state);
                let amount = if self.state.powerups[PW_FORCE_BOON] != 0 {
                    6
                } else if master {
                    4
                } else {
                    1
                };
                self.set_pool((self.pool() + amount).min(self.force.max));
                self.force.regen_debounce = match self.frame.lone_regen {
                    Some(pace) => (f64::from(self.force.regen_debounce) + pace) as i32,
                    None => self.force.regen_debounce + self.frame.regen_time.max(1),
                };
            }
        } else {
            self.force.regen_debounce = level_time;
        }
    }

    /// Enlightenment raises every power of its side (or of none) to the top level and
    /// makes it known; once it is gone, the levels go back and a power left at nothing is
    /// stopped and forgotten.
    fn enlightenment(&mut self) {
        let enlightened = self.state.powerups[PW_FORCE_ENLIGHTENED_LIGHT] != 0
            || self.state.powerups[PW_FORCE_ENLIGHTENED_DARK] != 0;
        if enlightened && !self.force.using_added {
            let side = self.field(61) as u8;
            let mut known = self.field(PS_KNOWN);
            for power in 0..NUM_FORCE_POWERS {
                self.force.base_levels[power] = self.force.levels[power];
                if FORCE_POWER_SIDES[power] == 0 || FORCE_POWER_SIDES[power] == side {
                    self.force.levels[power] = 3;
                    known |= 1 << power;
                }
            }
            self.state.set_raw_field(PS_KNOWN, known);
            self.force.using_added = true;
            self.mirror_levels();
        } else if !enlightened && self.force.using_added {
            for power in 0..NUM_FORCE_POWERS {
                self.force.levels[power] = self.force.base_levels[power];
                if self.force.levels[power] == 0 {
                    if self.active(power) {
                        self.stop(power);
                    }
                    let known = self.field(PS_KNOWN);
                    self.state.set_raw_field(PS_KNOWN, known & !(1 << power));
                }
            }
            self.force.using_added = false;
            self.mirror_levels();
        }
    }

    fn mirror_levels(&mut self) {
        mirror_levels(self.state, self.force);
    }

    fn has_ysalamiri(&self) -> bool {
        has_ysalamiri(self.state, self.frame.gametype)
    }

    fn can_use_now(&self, power: usize) -> bool {
        can_use_now(
            self.state,
            power,
            self.frame.level_time,
            self.frame.gametype,
        )
    }

    pub(crate) fn available(&self, power: usize, override_amount: i32) -> bool {
        available(self.state, self.force, power, override_amount)
    }

    pub(crate) fn usable(&self, power: usize) -> bool {
        usable(
            self.state,
            self.force,
            *self.frame.health,
            power,
            self.frame.level_time,
            self.frame.gametype,
        )
    }

    pub(crate) fn drain(&mut self, power: usize, override_amount: i32) {
        drain(self.state, self.force, power, override_amount);
    }

    /// `WP_DoSpecificPower` for the powers a player uses on itself: once per press.
    fn do_specific_power(&mut self, power: usize) {
        if !self.available(power, 0) {
            return;
        }
        match power {
            FP_HEAL | FP_SPEED | FP_RAGE | FP_PROTECT | FP_ABSORB | FP_SEE | FP_PUSH | FP_PULL
            | FP_TELEPATHY | FP_TEAM_HEAL | FP_TEAM_FORCE => {
                if self.force.button_need_release {
                    return;
                }
                match power {
                    FP_HEAL => self.heal(),
                    FP_SPEED => self.timed(FP_SPEED),
                    FP_SEE => self.timed(FP_SEE),
                    FP_PROTECT | FP_ABSORB | FP_RAGE => self.guard(power),
                    // `ForceThrow`, which the caller carries out.
                    FP_PUSH | FP_PULL => self.throw = Some(power == FP_PULL),
                    FP_TELEPATHY => self.telepathy(),
                    FP_TEAM_HEAL | FP_TEAM_FORCE => self.team_power(power),
                    // Not ported yet: the press is spent all the same.
                    _ => {}
                }
                self.force.button_need_release = true;
            }
            FP_GRIP => self.grip_button(),
            FP_LIGHTNING => self.lightning(),
            FP_DRAIN => self.force_drain(),
            _ => {}
        }
    }

    /// `ForceHeal`: all levels heal at once — 25, 10 or 5 — up to the maximum health.
    pub(crate) fn heal(&mut self) {
        let max_health = self.state.stats[STAT_MAX_HEALTH] as i32;
        if *self.frame.health <= 0 || !self.usable(FP_HEAL) || *self.frame.health >= max_health {
            return;
        }
        let amount = match self.force.levels[FP_HEAL] {
            3 => 25,
            2 => 10,
            _ => 5,
        };
        *self.frame.health = (*self.frame.health + amount).min(max_health);
        self.drain(FP_HEAL, 0);
        let index = self
            .frame
            .others
            .sound_index(b"sound/weapons/force/heal.wav");
        self.sound(CHAN_ITEM, index);
    }

    /// `ForceSpeed(self, duration)`: a Jedi NPC's speed runs `duration` (its level's when 0).
    pub(crate) fn speed(&mut self, duration: i32) {
        self.timed_for(FP_SPEED, duration);
    }

    /// `ForceSpeed` and `ForceSeeing`: on, or — pressed again once it has run 1.5 s — off.
    fn timed(&mut self, power: usize) {
        self.timed_for(power, 0);
    }

    /// [`Self::timed`] with `ForceSpeed`'s `forceDuration` (0 for the level's).
    fn timed_for(&mut self, power: usize, duration: i32) {
        if *self.frame.health <= 0 {
            return;
        }
        if self.force.allow_deactivate_time < self.frame.level_time && self.active(power) {
            self.stop(power);
            return;
        }
        if !self.usable(power) {
            return;
        }
        self.force.allow_deactivate_time = self.frame.level_time + 1_500;
        self.start(power, duration);
        let (sound, channel, loop_sound, track): (&[u8], _, &[u8], _) = if power == FP_SPEED {
            (
                b"sound/weapons/force/speed.wav",
                CHAN_BODY,
                b"sound/weapons/force/speedloop.wav",
                TRACK_CHANNEL_2,
            )
        } else {
            (
                b"sound/weapons/force/see.wav",
                CHAN_AUTO,
                b"sound/weapons/force/seeloop.wav",
                TRACK_CHANNEL_5,
            )
        };
        let index = self.frame.others.sound_index(sound);
        self.sound(channel, index);
        let index = self.frame.others.sound_index(loop_sound);
        self.sound(track, index);
    }

    /// `ForceProtect`, `ForceAbsorb` and `ForceRage`: on, ending the other two; or — once
    /// it has run 1.5 s — off. Rage waits out its recovery and wants ten health.
    pub(crate) fn guard(&mut self, power: usize) {
        let level_time = self.frame.level_time;
        if *self.frame.health <= 0 {
            return;
        }
        if self.force.allow_deactivate_time < level_time && self.active(power) {
            self.stop(power);
            return;
        }
        if !self.usable(power) {
            return;
        }
        if power == FP_RAGE
            && (self.field(PS_RAGE_RECOVERY) as i32 >= level_time || *self.frame.health < 10)
        {
            return;
        }
        let others = match power {
            FP_PROTECT => [FP_RAGE, FP_ABSORB],
            FP_ABSORB => [FP_RAGE, FP_PROTECT],
            _ => [FP_PROTECT, FP_ABSORB],
        };
        for other in others {
            if self.active(other) {
                self.stop(other);
            }
        }
        self.force.allow_deactivate_time = level_time + 1_500;
        self.start(power, 0);
        let loop_sound: &[u8] = match power {
            FP_PROTECT => {
                self.raise(predef_sound(self.origin(), PDSOUND_PROTECT));
                b"sound/weapons/force/protectloop.wav"
            }
            FP_ABSORB => {
                self.raise(predef_sound(self.origin(), PDSOUND_ABSORB));
                b"sound/weapons/force/absorbloop.wav"
            }
            _ => {
                let index = self
                    .frame
                    .others
                    .sound_index(b"sound/weapons/force/rage.wav");
                self.sound(TRACK_CHANNEL_4, index);
                b"sound/weapons/force/rageloop.wav"
            }
        };
        let index = self.frame.others.sound_index(loop_sound);
        self.sound(TRACK_CHANNEL_3, index);
    }

    /// `WP_ForcePowerStart` for the powers ported: the power on, its duration by level, a
    /// full-body taunt cut short, the noise the bots hear, and the cost.
    pub(crate) fn start(&mut self, power: usize, override_amount: i32) {
        if !self.available(power, override_amount) {
            return;
        }
        heard(self.force, power, self.frame.level_time);
        for (animation, timer) in [
            (PS_LEGS_ANIM, PS_LEGS_TIMER),
            (PS_TORSO_ANIM, PS_TORSO_TIMER),
        ] {
            if crate::pmove_input_freeze::full_body_taunt(self.field(animation) as u16) {
                self.state.set_raw_field(timer, 0);
            }
        }
        let level = self.force.levels[power];
        let mut override_amount = override_amount;
        let duration = match power {
            FP_SPEED => [0, 10_000, 15_000, 20_000][usize::from(level.min(3))],
            FP_SEE => [0, 10_000, 20_000, 30_000][usize::from(level.min(3))],
            FP_RAGE => [0, 8_000, 14_000, 20_000][usize::from(level.min(3))],
            FP_TELEPATHY => [0, 20_000, 25_000, 30_000][usize::from(level.min(3))],
            FP_PROTECT | FP_ABSORB => 20_000,
            // Lightning and drain last what the caller asks, and cost their plain amount.
            FP_LIGHTNING | FP_DRAIN => std::mem::take(&mut override_amount),
            _ => 0,
        };
        let starts = !(matches!(power, FP_SPEED | FP_SEE | FP_RAGE | FP_TELEPATHY)
            && !(1..=3).contains(&level));
        if starts {
            self.set_active(power, true);
        }
        match power {
            FP_GRIP => self.state.powerups[PW_DISINT_4] = (self.frame.level_time + 60_000) as u32,
            FP_LIGHTNING => {
                self.state
                    .set_raw_field(PS_ACTIVE_FORCE_PASS, u32::from(level));
            }
            _ => {}
        }
        let duration = if power == FP_SPEED && override_amount != 0 && starts {
            override_amount
        } else {
            duration
        };
        self.force.duration[power] = if duration != 0 {
            self.frame.level_time + duration
        } else {
            0
        };
        self.force.debounce[power] = 0;
        if power == FP_LEVITATION {
            // Levitation's debounce is the wire's (`fd.forcePowerDebounce[FP_LEVITATION]`).
            self.state.set_raw_field(PS_LEVITATION_DEBOUNCE, 0);
        }
        if power == FP_SPEED && override_amount != 0 {
            self.drain(power, (override_amount as f32 * 0.025) as i32);
        } else if power != FP_GRIP && power != FP_DRAIN {
            self.drain(power, override_amount);
        }
    }

    /// `WP_ForcePowerStop` for the powers ported: the power off, and its looping sound
    /// muted.
    pub(crate) fn stop(&mut self, power: usize) {
        let was_active = self.active(power);
        self.set_active(power, false);
        match power {
            FP_HEAL => (self.force.heal_amount, self.force.heal_time) = (0, 0),
            FP_SPEED if was_active => self.mute(
                self.force.kill_sounds[(TRACK_CHANNEL_2 - TRACK_CHANNEL_NONE) as usize],
                CHAN_VOICE,
            ),
            FP_SEE if was_active => self.mute(
                self.force.kill_sounds[(TRACK_CHANNEL_5 - TRACK_CHANNEL_NONE) as usize],
                CHAN_VOICE,
            ),
            FP_RAGE => {
                self.state
                    .set_raw_field(PS_RAGE_RECOVERY, (self.frame.level_time + 10_000) as u32);
                if was_active {
                    self.mute(
                        self.force.kill_sounds[(TRACK_CHANNEL_3 - TRACK_CHANNEL_NONE) as usize],
                        CHAN_VOICE,
                    );
                }
            }
            FP_ABSORB | FP_PROTECT if was_active => self.mute(
                self.force.kill_sounds[(TRACK_CHANNEL_3 - TRACK_CHANNEL_NONE) as usize],
                CHAN_VOICE,
            ),
            FP_TELEPATHY => {
                if was_active {
                    let index = self
                        .frame
                        .others
                        .sound_index(b"sound/weapons/force/distractstop.wav");
                    self.sound(CHAN_AUTO, index);
                }
                for field in PS_MINDTRICK {
                    self.state.set_raw_field(field, 0);
                }
            }
            FP_GRIP => self.stop_grip(was_active),
            FP_LIGHTNING | FP_DRAIN => self.stop_stream(power),
            _ => {}
        }
    }

    /// `WP_ForcePowerRun` for the powers ported.
    fn run(&mut self, power: usize) {
        let level_time = self.frame.level_time;
        match power {
            FP_HEAL => self.run_heal(),
            FP_LEVITATION => {
                if self.field(PS_GROUND_ENTITY) != ENTITY_NUMBER_NONE
                    && self.field(PS_JUMP_Z_START) == 0
                {
                    self.stop(power);
                }
            }
            FP_RAGE => {
                if *self.frame.health < 1 {
                    self.stop(power);
                    return;
                }
                if self.force.rage_drain_time < level_time {
                    *self.frame.health -= 2;
                    self.force.rage_drain_time = level_time
                        + [400, 150, 300, 450][usize::from(self.force.levels[FP_RAGE].min(3))];
                }
                if *self.frame.health < 1 {
                    *self.frame.health = 1;
                    self.stop(power);
                }
                self.state.stats[STAT_HEALTH] = *self.frame.health as u32;
            }
            FP_GRIP => self.run_grip(),
            FP_TELEPATHY => self.run_telepathy(),
            FP_LIGHTNING => self.run_stream(power, BUTTON_FORCE_LIGHTNING),
            FP_DRAIN => self.run_stream(power, BUTTON_FORCE_DRAIN),
            FP_PROTECT | FP_ABSORB => {
                if self.force.debounce[power] < level_time {
                    self.drain(power, 1);
                    if self.pool() < 1 {
                        self.stop(power);
                    }
                    self.force.debounce[power] =
                        level_time + if power == FP_PROTECT { 300 } else { 600 };
                }
            }
            _ => {}
        }
    }

    /// A held heal, a point a second (no level starts one since all heal at once, but
    /// enlightenment or a mod may).
    fn run_heal(&mut self) {
        let level_time = self.frame.level_time;
        let level = self.force.levels[FP_HEAL];
        let velocity = self.state.velocity();
        if level == 1 && velocity != [0.0; 3] {
            self.stop(FP_HEAL);
            return;
        }
        let max_health = self.state.stats[STAT_MAX_HEALTH] as i32;
        if *self.frame.health < 1 || (self.state.stats[STAT_HEALTH] as i32) < 1 {
            self.stop(FP_HEAL);
            return;
        }
        if self.force.heal_time > level_time {
            return;
        }
        if *self.frame.health > max_health {
            self.stop(FP_HEAL);
            return;
        }
        self.force.heal_time = level_time + 1_000;
        *self.frame.health += 1;
        self.force.heal_amount += 1;
        if *self.frame.health > max_health {
            *self.frame.health = max_health;
            self.stop(FP_HEAL);
        }
        if (level == 1 && self.force.heal_amount >= 25)
            || (level == 2 && self.force.heal_amount >= 33)
        {
            self.stop(FP_HEAL);
        }
    }

    /// A temp entity raised where the game raises it.
    pub(crate) fn raise(&mut self, event: EventEntity) -> Option<EntityId> {
        self.frame
            .others
            .pool()
            .spawn_temporary(event.state(), self.frame.level_time, None)
    }

    /// `G_Sound` on the player: on a tracking channel the temp entity stays as the
    /// sound's tracker, replacing (muting) the channel's last one.
    ///
    /// The channel names its last tracker by number (`killSoundEntIndex`), and nothing clears
    /// it when an NPC's tracker is freed: whatever entity now has the number is muted and
    /// freed — the new sound's own temp entity too, when it took the number, as it does
    /// when a loop is started again after it was muted (`g_utils.c:1358-1371`); the channel
    /// then names entity 0. Of an entity that
    /// is no sound tracker and not this sound's own, only the mute is sent: freeing an
    /// unrelated entity (an NPC, a missile) through a stale number is not reproduced.
    fn sound(&mut self, channel: u32, index: u16) {
        let mut event = EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(index),
            origin: self.origin(),
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        event.extra[0] = (ES_SABER_ENTITY, channel);
        if channel <= TRACK_CHANNEL_NONE {
            self.raise(event);
            return;
        }
        event.extra[1] = (ES_TRICKED, u32::from(self.frame.client));
        event.extra[2] = (ES_EFLAGS, EF_SOUNDTRACKER);
        event.broadcast = true;
        let slot = (channel - TRACK_CHANNEL_NONE) as usize;
        let Some(tracker) = self.raise(event) else {
            return;
        };
        // `g_entities[killSoundEntIndex].inuse && killSoundEntIndex > MAX_CLIENTS`.
        let named = self.force.kill_sounds[slot]
            .map(EntityId::legacy_number)
            .filter(|number| *number > MAX_CLIENTS);
        if let Some(current) = named.and_then(|number| self.frame.others.pool().legacy_id(number)) {
            if current == tracker {
                // Not yet marked a tracker as it is muted and freed: no loop to stop. Freed,
                // its number is 0 (`G_FreeEntity`'s `memset`), and so is the channel's
                // (`killSoundEntIndex = te->s.number`); what `G_Sound` then writes on it
                // stays for the slot's next occupant.
                self.mute_event(current.legacy_number(), CHAN_VOICE);
                self.frame
                    .others
                    .pool()
                    .free(current, self.frame.level_time);
                self.frame
                    .others
                    .pool()
                    .leave_tracker_residue(current, self.frame.client);
                self.force.kill_sounds[slot] = None;
                return;
            }
            self.mute(Some(current), CHAN_VOICE);
        }
        self.force.kill_sounds[slot] = Some(tracker);
    }

    /// `G_MuteSound`: everyone told to stop the sound `entity`'s number plays on `channel`;
    /// the entity that has the number now is freed with it where it is a sound tracker.
    /// `None` is entity 0, which the reference mutes as readily (an empty channel).
    pub(crate) fn mute(&mut self, entity: Option<EntityId>, channel: u32) {
        let number = entity.map_or(0, EntityId::legacy_number);
        self.mute_event(number, channel);
        let pool = self.frame.others.pool();
        let tracker = pool.legacy_id(number).filter(|current| {
            pool.state(*current)
                .is_some_and(|state| state.raw_field(ES_EFLAGS).unwrap_or(0) & EF_SOUNDTRACKER != 0)
        });
        if let Some(tracker) = tracker {
            self.free_tracker(tracker);
        }
    }

    /// `G_MuteSound`'s event: everyone told to stop the sound entity `number` plays on
    /// `channel`.
    fn mute_event(&mut self, number: u16, channel: u32) {
        let mut event = EventEntity {
            event: EV_MUTE_SOUND,
            parameter: 0,
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        };
        event.extra[0] = (ES_TRICKED2, u32::from(number));
        event.extra[1] = (ES_TRICKED, channel);
        self.raise(event);
    }

    /// `G_FreeEntity` on a tracker: a player's channel that named it names nothing
    /// (`g_utils.c:1011-1036` clears the clients' `killSoundEntIndex` only); an NPC's goes
    /// on naming its number.
    fn free_tracker(&mut self, entity: EntityId) {
        let player = self
            .frame
            .others
            .pool()
            .state(entity)
            .map_or(0, |state| state.raw_field(ES_TRICKED).unwrap_or(0) as u16);
        self.frame.others.pool().free(entity, self.frame.level_time);
        self.frame
            .others
            .loop_stopped(player, entity.legacy_number());
        if !self.frame.npc {
            for slot in &mut self.force.kill_sounds {
                if *slot == Some(entity) {
                    *slot = None;
                }
            }
        }
    }
}

/// `ForcePowerUsableOn(attacker, other, power)` (`w_force.c:543-618`) between two
/// players.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsableOn {
    Yes,
    No,
    /// A grip at a player absorbing: refused, with the absorbing hit's sound
    /// (`PDSOUND_ABSORBHIT`, at most every 400 ms) the caller plays.
    Absorbed,
}

/// `ForcePowerUsableOn` for player `attacker` using `power` on player `other`: not on
/// one with ysalamiri, not by one who may not use it now, not in or at a duel; a grip
/// not at one absorbing or in a saber special; a push or pull not at one knocked down.
/// (NPCs, the vehicle and siege cases, are not players.)
pub fn usable_on(
    attacker: &PlayerState,
    other: &PlayerState,
    power: usize,
    level_time: i32,
    gametype: i32,
) -> UsableOn {
    if has_ysalamiri(other, gametype)
        || !can_use_now(attacker, power, level_time, gametype)
        || attacker.duel_in_progress()
        || other.duel_in_progress()
    {
        return UsableOn::No;
    }
    if power == FP_GRIP {
        if other.force_powers_active() & (1 << FP_ABSORB) != 0 {
            return UsableOn::Absorbed;
        }
        if other.weapon() == 3 && crate::saber_rules::in_special(other.saber_move()) {
            return UsableOn::No;
        }
    }
    if (power == FP_PUSH || power == FP_PULL)
        && crate::knockdown::in_knockdown(other.leg_animation())
    {
        return UsableOn::No;
    }
    UsableOn::Yes
}

/// `WP_ForcePowerStart`'s `hearable` and `hearDist`, which only the bots read: every power
/// begun but the saber's is heard for 100 ms, lightning 512 units away, the rest 256.
pub(crate) fn heard(force: &mut ForcePowers, power: usize, level_time: i32) {
    if matches!(power, FP_SABER_OFFENSE | FP_SABER_DEFENSE | FP_SABER_THROW) {
        return;
    }
    force.other_sound_len = if power == FP_LIGHTNING { 512.0 } else { 256.0 };
    force.other_sound_time = level_time + 100;
}

/// `BG_HasYsalamiri`.
pub(crate) fn has_ysalamiri(state: &PlayerState, gametype: i32) -> bool {
    (gametype == GT_CTY && (state.powerups[PW_REDFLAG] != 0 || state.powerups[PW_BLUEFLAG] != 0))
        || state.powerups[PW_YSALAMIRI] != 0
}

/// `BG_CanUseFPNow` (`bg_misc.c:1725-1788`).
pub(crate) fn can_use_now(
    state: &PlayerState,
    power: usize,
    level_time: i32,
    gametype: i32,
) -> bool {
    let field = |index: usize| state.raw_field(index).unwrap_or(0);
    if has_ysalamiri(state, gametype)
        || field(PS_FORCE_RESTRICTED) != 0
        || field(PS_TRUE_NON_JEDI) != 0
    {
        return false;
    }
    if field(PS_WEAPON) == WP_EMPLACED_GUN || field(PS_VEHICLE) != 0 {
        return false;
    }
    let lock_frame = field(PS_SABER_LOCK_FRAME);
    if field(PS_DUEL_IN_PROGRESS) != 0
        && !matches!(power, FP_SABER_OFFENSE | FP_SABER_DEFENSE | FP_LEVITATION)
        && (lock_frame == 0 || power != FP_PUSH)
    {
        return false;
    }
    if (lock_frame != 0 || field(PS_SABER_LOCK_TIME) as i32 > level_time) && power != FP_PUSH {
        return false;
    }
    if field(PS_FALLING_TO_DEATH) != 0 {
        return false;
    }
    !(field(PS_BROKEN_LIMBS) & BROKENLIMB_ARMS != 0
        && matches!(power, FP_PUSH | FP_PULL | FP_GRIP | FP_LIGHTNING | FP_DRAIN))
}

/// `WP_ForcePowerAvailable`.
pub(crate) fn available(
    state: &PlayerState,
    force: &ForcePowers,
    power: usize,
    override_amount: i32,
) -> bool {
    let drain = if override_amount != 0 {
        override_amount
    } else {
        FORCE_POWER_NEEDED[usize::from(force.levels[power])][power]
    };
    let pool = state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32;
    if state.raw_field(PS_ACTIVE).unwrap_or(0) & (1 << power) != 0
        || power == FP_LEVITATION
        || drain == 0
    {
        return true;
    }
    if matches!(power, FP_DRAIN | FP_LIGHTNING) && pool >= 25 {
        return true;
    }
    pool >= drain
}

/// `WP_ForcePowerUsable` for a player in the game (never a spectator).
pub(crate) fn usable(
    state: &PlayerState,
    force: &ForcePowers,
    health: i32,
    power: usize,
    level_time: i32,
    gametype: i32,
) -> bool {
    if has_ysalamiri(state, gametype)
        || health <= 0
        || state.stats[STAT_HEALTH] as i32 <= 0
        || state.raw_field(PS_EFLAGS).unwrap_or(0) & EF_DEAD != 0
    {
        return false;
    }
    if !can_use_now(state, power, level_time, gametype)
        || state.raw_field(PS_KNOWN).unwrap_or(0) & (1 << power) == 0
    {
        return false;
    }
    if state.raw_field(PS_ACTIVE).unwrap_or(0) & (1 << power) != 0 && power != FP_LEVITATION {
        return false;
    }
    if force.levels[power] == 0 {
        return false;
    }
    // "this power is verboten when using this saber" (`w_force.c:732-788`, with
    // `g_saberRestrictForce` 0).
    if state.saber_holstered() == 0 && force.saber_restrictions & (1 << power) != 0 {
        return false;
    }
    available(state, force, power, 0)
}

/// `BG_ForcePowerDrain`: the cost taken from the pool, levitation's by how fast the
/// player rises.
pub(crate) fn drain(
    state: &mut PlayerState,
    force: &ForcePowers,
    power: usize,
    override_amount: i32,
) {
    let cost = if override_amount != 0 {
        override_amount
    } else {
        FORCE_POWER_NEEDED[usize::from(force.levels[power])][power]
    };
    if cost == 0 {
        return;
    }
    let pool = state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32;
    let cost = if power == FP_LEVITATION {
        let rising = f32::from_bits(state.raw_field(PS_VELOCITY_Z).unwrap_or(0));
        let jump = [
            (250.0, 20),
            (200.0, 16),
            (150.0, 12),
            (100.0, 8),
            (50.0, 6),
            (0.0, 4),
        ]
        .into_iter()
        .find(|(above, _)| rising > *above)
        .map_or(0, |(_, cost)| cost);
        if force.levels[FP_LEVITATION] != 0 {
            jump / i32::from(force.levels[FP_LEVITATION])
        } else {
            jump
        }
    } else {
        cost
    };
    state.set_raw_field(PS_FORCE_POWER, (pool - cost).max(0) as u32);
}
