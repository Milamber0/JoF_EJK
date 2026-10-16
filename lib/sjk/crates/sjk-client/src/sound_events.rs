//! Protocol-26/codemp sound-event compatibility adapter.
//!
//! This module owns all JKA event, channel, configstring and asset-table
//! knowledge. It mirrors the sound-bearing `CG_EntityEvent` cases
//! (`codemp/cgame/cg_event.c:1380-1825,1836-2033,2409-2584,2699-2760,
//! 3089-3192,3311-3418`) and emits
//! engine-generic [`sjk_audio::PlayRequest`] values.  Every asset is resolved
//! and registered at gamestate installation or on a table change. Snapshot
//! observation only touches fixed-capacity vectors and arrays.

use crate::ambient_world::LegacyAmbientShot;
use crate::local_sounds::{LegacyLocalSounds, LocalSoundMedia};
use crate::loop_sounds::{LegacyLoopAdapter, LegacyLoopDecision};
use crate::maintained_sounds::{
    LegacyMaintainedAction, LegacyMaintainedEvent, LegacyMaintainedLoopDecision, MaintainedSounds,
};
use crate::{AmbientSetKind, AmbientSets};
use sjk_audio::{Attenuation, ChannelId, PlayRequest, SoundHandle, SourceId};
use sjk_protocol::{EntityState, GameState, Snapshot};
use sjk_vfs::VirtualFileSystem;
#[path = "sound_feedback.rs"]
mod feedback;
#[path = "saber_sound_overrides.rs"]
mod saber_overrides;
#[path = "sound_saber_switch.rs"]
mod saber_switch;
#[path = "sound_taunts.rs"]
mod taunts;
use saber_overrides::SaberSoundOverrides;
pub use saber_overrides::{SABER_SOUND_CLIENTS, SaberSoundSet, TOGGLE_REACH};
pub use saber_switch::LegacySaberView;

const CS_SOUNDS: usize = 811; // codemp/game/bg_public.h:132
const MAX_SOUNDS: usize = 256;
const MAX_ENTITIES: usize = 1_024;
const MAX_CLIENTS: usize = 32;
const ET_EVENTS: u8 = 18; // codemp/game/bg_public.h:1270
const SOLID_BMODEL: u32 = 0x00ff_ffff; // codemp/qcommon/q_shared.h

const CHAN_AUTO: u32 = 0;
const CHAN_WEAPON: u32 = 2;
const CHAN_VOICE: u32 = 3;
const CHAN_BODY: u32 = 6;
const CHAN_MENU1: u32 = 11;
const CHAN_LOCAL: u32 = 1;
pub(crate) const CHAN_ANNOUNCER: u32 = 9;
const CS_AMBIENT_SET: usize = 37;
const EVENT_KIND_COUNT: usize = 59;
const EV_PLAYDOORLOOPSOUND: u16 = 72;
const EV_MUTE_SOUND: u16 = 74;
const EV_GENERAL_SOUND: u16 = 76;
const EV_STOPLOOPINGSOUND: u16 = 113;
const EV_STARTLOOPINGSOUND: u16 = 114;
const TRACKED_LOOP_CHANNELS: [u32; 3] = [52, 53, 55];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MaintainedRule {
    Door,
    Mute,
    TrackedGeneral,
    Stop,
    Start,
}

fn maintained_rule(event: u16, channel: u32) -> Option<MaintainedRule> {
    match event {
        EV_PLAYDOORLOOPSOUND => Some(MaintainedRule::Door),
        EV_MUTE_SOUND => Some(MaintainedRule::Mute),
        EV_GENERAL_SOUND if TRACKED_LOOP_CHANNELS.contains(&channel) => {
            Some(MaintainedRule::TrackedGeneral)
        }
        EV_STOPLOOPINGSOUND => Some(MaintainedRule::Stop),
        EV_STARTLOOPINGSOUND => Some(MaintainedRule::Start),
        _ => None,
    }
}

// `CG_RegisterWeapon` flash sounds, codemp/cgame/cg_weaponinit.c:136-607,
// indexed by `weapon_t` (codemp/game/bg_weapons.h:31-50). Only the stun baton
// registers a flash sound in the baton/melee case; melee swings come from the
// player's animevents.cfg instead.
const WEAPON_PATHS: [[Option<&str>; 2]; 19] = [
    [None, None],                              // WP_NONE
    [Some("sound/weapons/baton/fire.mp3"); 2], // WP_STUN_BATON
    [None, None],                              // WP_MELEE
    [None, None],                              // WP_SABER
    [
        Some("sound/weapons/bryar/fire.wav"),
        Some("sound/weapons/bryar/alt_fire.wav"),
    ],
    [
        Some("sound/weapons/blaster/fire.wav"),
        Some("sound/weapons/blaster/alt_fire.wav"),
    ],
    [
        Some("sound/weapons/disruptor/fire.wav"),
        Some("sound/weapons/disruptor/alt_fire.wav"),
    ],
    [Some("sound/weapons/bowcaster/fire.wav"); 2],
    [
        Some("sound/weapons/repeater/fire.wav"),
        Some("sound/weapons/repeater/alt_fire.wav"),
    ],
    [
        Some("sound/weapons/demp2/fire.wav"),
        Some("sound/weapons/demp2/altfire.wav"),
    ],
    [
        Some("sound/weapons/flechette/fire.wav"),
        Some("sound/weapons/flechette/alt_fire.wav"),
    ],
    [
        Some("sound/weapons/rocket/fire.wav"),
        Some("sound/weapons/rocket/alt_fire.wav"),
    ],
    [Some("sound/weapons/thermal/fire.wav"); 2],
    [Some("sound/weapons/laser_trap/fire.wav"); 2],
    [Some("sound/weapons/detpack/fire.wav"); 2],
    [None, None],
    [
        Some("sound/weapons/bryar/fire.wav"),
        Some("sound/weapons/bryar/alt_fire.wav"),
    ],
    [
        Some("sound/weapons/blaster/fire.wav"),
        Some("sound/weapons/blaster/alt_fire.wav"),
    ],
    [None, None],
];
const WEAPON_SELECT_PATHS: [Option<&str>; 19] = [
    None,
    None,
    None,
    None,
    Some("sound/weapons/bryar/select.wav"),
    Some("sound/weapons/blaster/select.wav"),
    Some("sound/weapons/disruptor/select.wav"),
    Some("sound/weapons/bowcaster/select.wav"),
    Some("sound/weapons/repeater/select.wav"),
    Some("sound/weapons/demp2/select.wav"),
    Some("sound/weapons/flechette/select.wav"),
    Some("sound/weapons/rocket/select.wav"),
    Some("sound/weapons/thermal/select.wav"),
    Some("sound/weapons/detpack/select.wav"),
    Some("sound/weapons/detpack/select.wav"),
    Some("sound/weapons/concussion/select.wav"),
    Some("sound/weapons/bryar/select.wav"),
    Some("sound/weapons/blaster/select.wav"),
    None,
];
const WEAPON_CHARGE_PATHS: [[Option<&str>; 2]; 19] = [
    [None, None],
    [None, None],
    [None, None],
    [None, None],
    [None, Some("sound/weapons/bryar/altcharge.wav")],
    [None, None],
    [
        Some("sound/weapons/disruptor/zoomloop.wav"),
        Some("sound/weapons/disruptor/altCharge.wav"),
    ],
    [Some("sound/weapons/bowcaster/altcharge.wav"), None],
    [None, None],
    [None, Some("sound/weapons/demp2/altCharge.wav")],
    [None, None],
    [None, None],
    [
        Some("sound/weapons/thermal/charge.wav"),
        Some("sound/weapons/thermal/charge.wav"),
    ],
    [None, None],
    [None, None],
    [None, Some("sound/weapons/bryar/altcharge.wav")],
    [None, Some("sound/weapons/bryar/altcharge.wav")],
    [None, None],
    [None, None],
];

/// Supported codemp event categories recorded by the parity ledger.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LegacySoundEvent {
    Footstep,
    FootstepMetal,
    Footsplash,
    Footwade,
    Swim,
    Fall,
    GlobalDuel,
    PrivateDuel,
    Jump,
    Roll,
    WaterTouch,
    WaterLeave,
    WaterUnder,
    WaterClear,
    ItemPickup,
    GlobalItemPickup,
    NoAmmo,
    ChangeWeapon,
    FireWeapon,
    AltFire,
    SaberAttack,
    SaberHit,
    SaberBlock,
    SaberUnholster,
    BecomeJediMaster,
    DisruptorZoom,
    Predefined,
    TeamPower,
    ItemRespawn,
    ItemPop,
    TeleportIn,
    TeleportOut,
    /// `EV_PLAYDOORSOUND`: a mover's soundset start or end sound (`CG_PlayDoorSound`).
    PlayDoorSound,
    Bmodel,
    VoiceCommand,
    General,
    Global,
    Entity,
    Pain,
    Death,
    WeaponCharge,
    WeaponChargeAlt,
    Taunt,
    ForceDrained,
    /// `chat`/`lchat` server command beep (`CG_ServerCommand`).
    ChatBeep,
    /// `tchat`/`ltchat` server command beep.
    TeamChatBeep,
    /// `CG_CheckLocalSounds` timelimit warnings.
    OneMinuteWarning,
    FiveMinuteWarning,
    /// `CG_CheckLocalSounds` fraglimit warnings, played via the buffered ring.
    OneFragWarning,
    TwoFragWarning,
    ThreeFragWarning,
    /// `CG_DrawWarmup` 3/2/1 countdown.
    WarmupCount,
    /// `CG_MapRestart` "fight" line.
    RestartFight,
    /// Buffered, listener-local red/blue team announcement.
    GlobalTeam,
    /// Push reaction voice (`cg_event.c:1684-1688`).
    Pushed,
    /// Grip/choke reaction voice (`cg_event.c:1691-1695`).
    Choke,
    /// Failed push reaction voice (`cg_event.c:1821-1823`).
    PushFail,
    /// cgame's ignition as a player draws the saber from another weapon
    /// (`CG_CheckPlayerG2Weapons`, `CG_Player`).
    SaberSwitchOn,
    /// cgame's retraction as a player puts a lit saber away for another weapon.
    SaberSwitchOff,
}

impl LegacySoundEvent {
    /// True for the families `cg_footsteps 0` silences: `EV_FOOTSTEP`,
    /// `EV_FOOTSTEP_METAL`, `EV_FOOTSPLASH`, `EV_FOOTWADE` and `EV_SWIM` are
    /// the only cases guarded by the cvar in `codemp/cgame/cg_event.c:1380-1455`.
    pub const fn is_footstep(self) -> bool {
        matches!(
            self,
            Self::Footstep | Self::FootstepMetal | Self::Footsplash | Self::Footwade | Self::Swim
        )
    }

    /// Stable report label.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pushed => "EV_PUSHED",
            Self::Choke => "EV_CHOKE",
            Self::PushFail => "EV_PUSHFAIL",
            Self::SaberSwitchOn => "CG_SABER_SWITCH_ON",
            Self::SaberSwitchOff => "CG_SABER_SWITCH_OFF",
            Self::GlobalTeam => "EV_GLOBAL_TEAM_SOUND",
            Self::Footstep => "EV_FOOTSTEP",
            Self::FootstepMetal => "EV_FOOTSTEP_METAL",
            Self::Footsplash => "EV_FOOTSPLASH",
            Self::Footwade => "EV_FOOTWADE",
            Self::Swim => "EV_SWIM",
            Self::Fall => "EV_FALL",
            Self::GlobalDuel => "EV_GLOBAL_DUEL",
            Self::PrivateDuel => "EV_PRIVATE_DUEL",
            Self::Jump => "EV_JUMP",
            Self::Roll => "EV_ROLL",
            Self::WaterTouch => "EV_WATER_TOUCH",
            Self::WaterLeave => "EV_WATER_LEAVE",
            Self::WaterUnder => "EV_WATER_UNDER",
            Self::WaterClear => "EV_WATER_CLEAR",
            Self::ItemPickup => "EV_ITEM_PICKUP",
            Self::GlobalItemPickup => "EV_GLOBAL_ITEM_PICKUP",
            Self::NoAmmo => "EV_NOAMMO",
            Self::ChangeWeapon => "EV_CHANGE_WEAPON",
            Self::FireWeapon => "EV_FIRE_WEAPON",
            Self::AltFire => "EV_ALT_FIRE",
            Self::SaberAttack => "EV_SABER_ATTACK",
            Self::SaberHit => "EV_SABER_HIT",
            Self::SaberBlock => "EV_SABER_BLOCK",
            Self::SaberUnholster => "EV_SABER_UNHOLSTER",
            Self::BecomeJediMaster => "EV_BECOME_JEDIMASTER",
            Self::DisruptorZoom => "EV_DISRUPTOR_ZOOMSOUND",
            Self::Predefined => "EV_PREDEFSOUND",
            Self::TeamPower => "EV_TEAM_POWER",
            Self::ItemRespawn => "EV_ITEM_RESPAWN",
            Self::ItemPop => "EV_ITEM_POP",
            Self::TeleportIn => "EV_PLAYER_TELEPORT_IN",
            Self::TeleportOut => "EV_PLAYER_TELEPORT_OUT",
            Self::PlayDoorSound => "EV_PLAYDOORSOUND",
            Self::Bmodel => "EV_BMODEL_SOUND",
            Self::VoiceCommand => "EV_VOICECMD_SOUND",
            Self::General => "EV_GENERAL_SOUND",
            Self::Global => "EV_GLOBAL_SOUND",
            Self::Entity => "EV_ENTITY_SOUND",
            Self::Pain => "EV_PAIN",
            Self::Death => "EV_DEATH",
            Self::WeaponCharge => "EV_WEAPON_CHARGE",
            Self::WeaponChargeAlt => "EV_WEAPON_CHARGE_ALT",
            Self::Taunt => "EV_TAUNT",
            Self::ForceDrained => "EV_FORCE_DRAINED",
            Self::ChatBeep => "CG_CHAT_BEEP",
            Self::TeamChatBeep => "CG_TEAM_CHAT_BEEP",
            Self::OneMinuteWarning => "CG_ONE_MINUTE_WARNING",
            Self::FiveMinuteWarning => "CG_FIVE_MINUTE_WARNING",
            Self::OneFragWarning => "CG_ONE_FRAG_WARNING",
            Self::TwoFragWarning => "CG_TWO_FRAG_WARNING",
            Self::ThreeFragWarning => "CG_THREE_FRAG_WARNING",
            Self::WarmupCount => "CG_WARMUP_COUNT",
            Self::RestartFight => "CG_RESTART_FIGHT",
        }
    }
}

/// One sound registered at gamestate time.
#[derive(Clone, Debug)]
pub struct RegisteredLegacySound {
    /// Resolved case-insensitive VFS path, including extension fallback.
    pub path: Box<str>,
    /// The path asked for, which differs from `path` when an extension fallback
    /// resolved it (`.wav` asked, `.mp3` found): a later request for it finds this
    /// entry instead of reading the file and adding another.
    pub requested: Box<str>,
    /// Decoded engine-bank handle; absent when the source asset is unavailable.
    pub handle: Option<SoundHandle>,
}

/// One allocation-free decision emitted while traversing a snapshot.
#[derive(Clone, Copy, Debug)]
pub struct LegacySoundDecision {
    /// Compatibility event family that produced this request.
    pub event: LegacySoundEvent,
    /// Adapter-local registered-sound index.
    pub sound: Option<u16>,
    /// Decoded engine-bank handle, if registration succeeded.
    pub handle: Option<SoundHandle>,
    /// True for a second voice emitted by one event (roll/fall/voice radio).
    pub additional: bool,
    /// Engine-generic playback request.
    pub request: PlayRequest,
    /// The client the event names as having caused this sound, when that is not its
    /// source: the attacker of a saber hit or block (`otherEntityNum2`), the speaker
    /// of a voice command (`groundEntityNum`), the sender of a chat message. A
    /// presentation filter can silence a player by it (SJK's local mute).
    pub cause: Option<u16>,
}

/// Background-track transition requested by `EV_PRIVATE_DUEL`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyMusicAction {
    /// Start BaseJKA's fixed multiplayer duel track.
    StartPrivateDuel,
    /// Restore the current map's `CS_MUSIC` intro/loop pair.
    RestoreMap,
}

/// Aggregate counters suitable for a headless parity report.
#[derive(Clone, Debug)]
pub struct LegacySoundLedger {
    /// Supported source events observed.
    pub supported_events: usize,
    /// Resolved one-shot or background-track requests.
    pub play_requests: usize,
    /// Requests whose source path was absent or undecodable.
    pub unresolved_paths: usize,
    /// Fixed event-family counters in [`LegacySoundEvent`] order.
    pub by_event: [usize; EVENT_KIND_COUNT],
}

impl Default for LegacySoundLedger {
    fn default() -> Self {
        Self {
            supported_events: 0,
            play_requests: 0,
            unresolved_paths: 0,
            by_event: [0; EVENT_KIND_COUNT],
        }
    }
}

impl LegacySoundLedger {
    fn record(&mut self, event: LegacySoundEvent, resolved: bool) {
        self.supported_events += 1;
        self.by_event[event as usize] += 1;
        if resolved {
            self.play_requests += 1;
        } else {
            self.unresolved_paths += 1;
        }
    }

    fn record_additional(&mut self, resolved: bool) {
        if resolved {
            self.play_requests += 1;
        } else {
            self.unresolved_paths += 1;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct CustomSet {
    pushed: [Option<u16>; 3],
    choke: [Option<u16>; 3],
    push_fail: Option<u16>,
    jump: Option<u16>,
    pain: [Option<u16>; 4],
    death: [Option<u16>; 3],
    land: Option<u16>,
    gasp: Option<u16>,
    taunt: Option<u16>,
    anger: [Option<u16>; 3],
    victory: [Option<u16>; 3],
    taunt_numbered: [Option<u16>; 3],
    deflect: [Option<u16>; 3],
    gloat: [Option<u16>; 3],
}

#[derive(Clone, Copy)]
enum CustomKind {
    Pushed(usize),
    Choke(usize),
    PushFail,
    Jump,
    Pain(usize),
    Death(usize),
    Gasp,
    Taunt,
    Anger(usize),
    Victory(usize),
    TauntNumbered(usize),
    Deflect(usize),
    Gloat(usize),
}

impl CustomSet {
    fn get(self, kind: CustomKind) -> Option<u16> {
        match kind {
            CustomKind::Pushed(index) => self.pushed[index],
            CustomKind::Choke(index) => self.choke[index],
            CustomKind::PushFail => self.push_fail,
            CustomKind::Jump => self.jump,
            CustomKind::Pain(index) => self.pain[index],
            CustomKind::Death(index) => self.death[index],
            CustomKind::Gasp => self.gasp,
            CustomKind::Taunt => self.taunt,
            CustomKind::Anger(index) => self.anger[index],
            CustomKind::Victory(index) => self.victory[index],
            CustomKind::TauntNumbered(index) => self.taunt_numbered[index],
            CustomKind::Deflect(index) => self.deflect[index],
            CustomKind::Gloat(index) => self.gloat[index],
        }
    }
}

#[derive(Clone, Copy)]
struct EventSubject {
    predicted_zoom: Option<u8>,
    number: u16,
    client_num: u16,
    weapon: u8,
    origin: [f32; 3],
    general_channel: u32,
    entity_channel: u32,
    e_flags: u32,
    owner: u16,
    other: u16,
    other2: u16,
    ground: u16,
    soundset: u8,
    client_bits: [u16; 4],
    fixed_origin: [f32; 3],
    tracked_entity: u16,
    mute_entity: u16,
    /// A vehicle (`ET_NPC`, `CLASS_VEHICLE`): `EV_FIRE_WEAPON` and `EV_ALT_FIRE` on it
    /// play nothing ([`is_vehicle`]).
    vehicle: bool,
}

/// `CG_EntityEvent` (`cg_event.c:2751-2760`, `:2779-2784`): an `ET_NPC` vehicle with a
/// `Vehicle_t` "does nothing for clientside weapon fire events". Its guns sound through
/// the `muzzleFX` effect `EV_VEH_FIRE` plays, never through the ordinary weapon table,
/// whose entry for the vehicle's `weapon` is a different gun's flash.
fn is_vehicle(entity_type: u8, npc_class: u8) -> bool {
    const ET_NPC: u8 = 13;
    const CLASS_VEHICLE: u8 = 53;
    entity_type == ET_NPC && npc_class == CLASS_VEHICLE
}

impl EventSubject {
    fn entity(state: &EntityState) -> Self {
        Self {
            predicted_zoom: None,
            number: state.number(),
            client_num: state.client_num(),
            weapon: state.weapon(),
            origin: state.trajectory_base(),
            // `saberEntityNum` and `trickedentindex`, respectively. These are
            // netfields 37 and 58 in codemp/qcommon/msg.cpp:884,920.
            general_channel: u32::from(state.event_sound_channel()),
            entity_channel: u32::from(state.tracked_entity_num()),
            e_flags: state.e_flags(),
            owner: state.raw_field(40).unwrap_or(0) as u16,
            other: state.other_entity_num(),
            other2: state.other_entity_num2(),
            ground: state.ground_entity_num(),
            soundset: state.sound_set_index(),
            client_bits: [58, 74, 93, 95].map(|field| state.raw_field(field).unwrap_or(0) as u16),
            fixed_origin: state.event_origin(),
            tracked_entity: state.tracked_entity_num(),
            mute_entity: state.mute_entity_num(),
            vehicle: is_vehicle(state.entity_type(), state.npc_class()),
        }
    }

    fn player(snapshot: &Snapshot) -> Self {
        Self {
            predicted_zoom: None,
            number: snapshot.player.client_num(),
            client_num: snapshot.player.client_num(),
            weapon: snapshot.player.weapon(),
            origin: snapshot.player.origin(),
            general_channel: CHAN_AUTO,
            entity_channel: CHAN_AUTO,
            e_flags: snapshot.player.entity_flags(),
            owner: snapshot.player.client_num(),
            other: 0,
            other2: 0,
            ground: snapshot.player.ground_entity_num(),
            soundset: 0,
            client_bits: [0; 4],
            fixed_origin: snapshot.player.origin(),
            tracked_entity: 0,
            mute_entity: 0,
            vehicle: false,
        }
    }
}

/// The client an event names as having caused its sounds when its source (a
/// temporary entity) is not theirs: `EV_SABER_HIT` and `EV_SABER_BLOCK` carry the
/// saber's owner in `otherEntityNum2` (`w_saber.c`), `EV_VOICECMD_SOUND` the speaker
/// in `groundEntityNum` (`g_cmds.c`). `None` for the rest, whose source says whose
/// they are.
fn event_cause(event: u16, entity: &EventSubject) -> Option<u16> {
    let client = match event {
        30 => entity.other2,
        // Stock `w_saber.c` sends a thrown saber's or a missile's block with no owner
        // (`otherEntityNum2` stays 0), which is no sign of client 0.
        31 if entity.other2 != 0 => entity.other2,
        75 => entity.ground,
        _ => return None,
    };
    (usize::from(client) < MAX_CLIENTS).then_some(client)
}

/// Stateful codemp sound-event resolver and duplicate-event latch.
pub struct LegacySoundAdapter {
    predicted_events: crate::predicted_events::PredictedEventLedger,
    sounds: Vec<RegisteredLegacySound>,
    cs_sounds: [Option<u16>; MAX_SOUNDS],
    cs_custom: [Option<CustomKind>; MAX_SOUNDS],
    weapons: [[Option<u16>; 2]; 19],
    footsteps: [[Option<u16>; 4]; 11],
    water_steps: [[Option<u16>; 4]; 3],
    custom: [CustomSet; MAX_CLIENTS],
    client_teams: [u8; MAX_CLIENTS],
    taunt_random: crate::animation_events::Random,
    saber_attack: [Option<u16>; 8],
    saber_hit: [Option<u16>; 3],
    saber_block: [Option<u16>; 9],
    saber_on: Option<u16>,
    /// cgame's per-player saber ignition on a weapon switch.
    saber_switch: saber_switch::SaberSwitchSounds,
    fall: Option<u16>,
    land: Option<u16>,
    object_hit: Option<u16>,
    item_sounds: [Option<u16>; 6],
    weapon_select: [Option<u16>; 19],
    weapon_charge: [[Option<u16>; 2]; 19],
    predefined: [Option<u16>; 6],
    ambient_stages: [[Option<u16>; 3]; MAX_SOUNDS],
    ambient_catalog: AmbientSets,
    roll: Option<u16>,
    water: [Option<u16>; 3],
    teleport: [Option<u16>; 2],
    respawn: Option<u16>,
    no_ammo: Option<u16>,
    count_fight: Option<u16>,
    happy_music: Option<u16>,
    team_power: [Option<u16>; 2],
    drain: Option<u16>,
    zoom: [Option<u16>; 2],
    select_fallback: Option<u16>,
    dramatic_failure: Option<u16>,
    /// Announcer and chat sounds derived on the client (`local_sounds.rs`).
    local: LegacyLocalSounds,
    decisions: Vec<LegacySoundDecision>,
    music_actions: Vec<LegacyMusicAction>,
    previous_entity_event: Box<[u16]>,
    seen_entity: Box<[u32]>,
    last_entity_time: Box<[i32]>,
    epoch: u32,
    player_event_sequence: i32,
    player_events: [u16; 2],
    player_external_event: u16,
    pain_deadline: Box<[i32]>,
    local_health_pain: bool,
    /// The client the event being resolved names as its cause ([`event_cause`]).
    cause: Option<u16>,
    client_config_hash: [u64; MAX_CLIENTS],
    ledger: LegacySoundLedger,
    loops: LegacyLoopAdapter,
    maintained: MaintainedSounds,
    /// Blade skins' sounds in place of the stock saber's, per client.
    saber_overrides: SaberSoundOverrides,
    /// `cgs.inlineModelMidpoints`, indexed by BSP model number.
    inline_model_midpoints: Box<[[f32; 3]]>,
}

impl LegacySoundAdapter {
    /// Select TaystJK chat notification variants (cg_servercmds.c:1715-1761).
    pub fn set_chat_sound_mode(&mut self, mode: i64, clean: bool) {
        self.local.chat_mode = mode;
        self.local.chat_filter.enabled = clean;
    }

    /// Resolve and decode all static tables, CS_SOUNDS, and model custom sounds.
    ///
    /// `register` is called at most once per distinct resolved asset and is
    /// never called by [`Self::observe_snapshot`].
    pub fn new(
        game_state: &GameState,
        vfs: &VirtualFileSystem,
        mut register: impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) -> Self {
        let mut sounds = Vec::with_capacity(512);
        let mut intern = |path: &str| intern_sound(&mut sounds, vfs, path, &mut register);
        let mut cs_sounds = [None; MAX_SOUNDS];
        let mut cs_custom = [None; MAX_SOUNDS];
        for (index, slot) in cs_sounds.iter_mut().enumerate() {
            let Some(bytes) = game_state.config_string(CS_SOUNDS + index) else {
                continue;
            };
            let Ok(path) = std::str::from_utf8(bytes) else {
                continue;
            };
            if let Some(kind) = custom_kind(path) {
                cs_custom[index] = Some(kind);
            } else if !path.is_empty() {
                *slot = Some(intern(path));
            }
        }
        let weapons = WEAPON_PATHS.map(|pair| pair.map(|path| path.map(&mut intern)));
        let footstep_prefix = [
            "stone_step",
            "mud_walk",
            "dirt_step",
            "sand_walk",
            "snow_step",
            "grass_step",
            "metal_step",
            "pipe_step",
            "gravel_walk",
            "rug_step",
            "wood_walk",
        ];
        let footsteps = footstep_prefix.map(|prefix| {
            std::array::from_fn(|variant| {
                Some(intern(&format!(
                    "sound/player/footsteps/{prefix}{}.mp3",
                    variant + 1
                )))
            })
        });
        let water_steps = ["water_run", "water_walk", "water_wade_0"].map(|prefix| {
            std::array::from_fn(|variant| {
                Some(intern(&format!(
                    "sound/player/footsteps/{prefix}{}.wav",
                    variant + 1
                )))
            })
        });
        let saber_attack = std::array::from_fn(|i| {
            Some(intern(&format!(
                "sound/weapons/saber/saberhup{}.mp3",
                i + 1
            )))
        });
        let saber_hit = std::array::from_fn(|i| {
            Some(intern(&format!(
                "sound/weapons/saber/saberhit{}.mp3",
                i + 1
            )))
        });
        let saber_block = std::array::from_fn(|i| {
            Some(intern(&format!(
                "sound/weapons/saber/saberblock{}.mp3",
                i + 1
            )))
        });
        let saber_on = Some(intern(saber_switch::STOCK_SABER_ON));
        let fall = Some(intern("sound/player/fallsplat.wav"));
        let land = Some(intern("sound/player/land1.wav"));
        let object_hit = Some(intern("sound/movers/objects/objectHit.wav"));
        let item_sounds = [
            "sound/player/pickupshield.wav",
            "sound/player/pickuphealth.wav",
            "sound/weapons/w_pkup.wav",
            "sound/player/enlightenment.wav",
            "sound/player/boon.wav",
            "sound/player/pickupenergy.wav",
        ]
        .map(|path| Some(intern(path)));
        let weapon_select = WEAPON_SELECT_PATHS.map(|path| path.map(&mut intern));
        let weapon_charge = WEAPON_CHARGE_PATHS.map(|pair| pair.map(|path| path.map(&mut intern)));
        let predefined = [
            "sound/weapons/force/protecthit.mp3",
            "sound/weapons/force/protect.mp3",
            "sound/weapons/force/absorbhit.mp3",
            "sound/weapons/force/absorb.mp3",
            "sound/weapons/force/jump.mp3",
            "sound/weapons/force/grip.mp3",
        ]
        .map(|path| Some(intern(path)));
        let ambient = vfs
            .read("sound/sound.txt")
            .ok()
            .flatten()
            .map(|asset| AmbientSets::parse(&String::from_utf8_lossy(&asset.bytes)))
            .unwrap_or_default();
        let mut ambient_stages = [[None; 3]; MAX_SOUNDS];
        for (index, stages) in ambient_stages.iter_mut().enumerate() {
            let Some(bytes) = game_state.config_string(CS_AMBIENT_SET + index) else {
                continue;
            };
            let Ok(name) = std::str::from_utf8(bytes) else {
                continue;
            };
            let Some(set) = ambient
                .get(name)
                .filter(|set| set.kind == AmbientSetKind::Bmodel)
            else {
                continue;
            };
            for (stage, path) in set.sub_waves.iter().take(3).enumerate() {
                stages[stage] = Some(intern(path));
            }
        }
        let roll = Some(intern("sound/player/roll1.wav"));
        let water = [
            "sound/player/watr_in.wav",
            "sound/player/watr_out.wav",
            "sound/player/watr_un.wav",
        ]
        .map(|path| Some(intern(path)));
        let teleport =
            ["sound/player/telein.wav", "sound/player/teleout.wav"].map(|path| Some(intern(path)));
        let respawn = Some(intern("sound/items/respawn1.wav"));
        let no_ammo = Some(intern("sound/weapons/noammo.wav"));
        let count_fight = Some(intern("sound/chars/protocol/misc/40MOM038"));
        let happy_music = Some(intern("music/goodsmall.mp3"));
        let team_power = [
            "sound/weapons/force/teamheal.wav",
            "sound/weapons/force/teamforce.wav",
        ]
        .map(|path| Some(intern(path)));
        let drain = Some(intern("sound/weapons/force/drained.mp3"));
        let zoom = [
            "sound/weapons/disruptor/zoomend.wav",
            "sound/weapons/disruptor/zoomstart.wav",
        ]
        .map(|path| Some(intern(path)));
        let select_fallback = Some(intern("sound/weapons/change.wav"));
        let dramatic_failure = Some(intern("music/badsmall.mp3"));
        let mut local_media = LocalSoundMedia::register(&mut intern);
        let preferred_chat = vfs
            .contains("sound/interface/commlink_off.mp3")
            .unwrap_or(false)
            .then(|| intern("sound/interface/commlink_off.mp3"));
        let custom =
            std::array::from_fn(|client| custom_set(game_state, vfs, client as u16, &mut intern));
        let client_teams = std::array::from_fn(|client| client_team(game_state, client));
        let saber_definitions = crate::legacy_saber_definitions(vfs).unwrap_or_default();
        let saber_switch =
            saber_switch::SaberSwitchSounds::new(game_state, &saber_definitions, &mut intern);

        drop(intern);
        if let Some(id) = preferred_chat.filter(|id| sounds[*id as usize].handle.is_some()) {
            local_media.private_chat = Some(id);
        }
        let local = LegacyLocalSounds::new(game_state, local_media);
        let client_config_hash =
            std::array::from_fn(|client| player_config_hash(game_state, client));
        let loops = LegacyLoopAdapter::new(game_state, vfs, &mut register);
        let maintained = MaintainedSounds::new();
        Self {
            sounds,
            cs_sounds,
            cs_custom,
            weapons,
            footsteps,
            water_steps,
            custom,
            client_teams,
            predicted_events: Default::default(),
            taunt_random: Default::default(),
            saber_attack,
            saber_hit,
            saber_block,
            saber_on,
            saber_switch,
            fall,
            land,
            object_hit,
            item_sounds,
            weapon_select,
            weapon_charge,
            predefined,
            ambient_stages,
            roll,
            water,
            teleport,
            respawn,
            no_ammo,
            count_fight,
            happy_music,
            team_power,
            drain,
            zoom,
            select_fallback,
            dramatic_failure,
            local,
            ambient_catalog: ambient,
            decisions: Vec::with_capacity(2_048),
            music_actions: Vec::with_capacity(2),
            previous_entity_event: vec![0; MAX_ENTITIES].into_boxed_slice(),
            seen_entity: vec![0; MAX_ENTITIES].into_boxed_slice(),
            last_entity_time: vec![i32::MIN; MAX_ENTITIES].into_boxed_slice(),
            epoch: 0,
            player_event_sequence: 0,
            player_events: [0; 2],
            player_external_event: 0,
            pain_deadline: vec![i32::MIN; MAX_ENTITIES].into_boxed_slice(),
            local_health_pain: false,
            cause: None,
            client_config_hash,
            ledger: LegacySoundLedger::default(),
            loops,
            maintained,
            saber_overrides: SaberSoundOverrides::default(),
            inline_model_midpoints: Box::default(),
        }
    }

    /// Install the map's inline-model midpoints (`cgs.inlineModelMidpoints`,
    /// `CG_RegisterGraphics`), indexed by BSP model number, model 0 unused.
    pub fn set_inline_model_midpoints(&mut self, midpoints: Box<[[f32; 3]]>) {
        self.inline_model_midpoints = midpoints;
    }

    /// Where an entity's sounds come from (`CG_SetEntitySoundPosition`,
    /// `cg_ents.c:118-132`): a brush model's origin is usually the world origin,
    /// so its sounds come from the middle of its model instead.
    pub fn sound_origin(&self, state: &EntityState) -> [f32; 3] {
        let origin = state.trajectory_base();
        if state.solid() != SOLID_BMODEL {
            return origin;
        }
        usize::try_from(state.model_index())
            .ok()
            .and_then(|index| self.inline_model_midpoints.get(index))
            .map_or(origin, |midpoint| {
                std::array::from_fn(|axis| origin[axis] + midpoint[axis])
            })
    }

    /// `S_StartSound` without an origin plays at the source entity's sound
    /// position; `fallback` when the snapshot does not hold the source.
    fn origin_for_source(&self, snapshot: &Snapshot, source: u16, fallback: [f32; 3]) -> [f32; 3] {
        if source == snapshot.player.client_num() {
            return snapshot.player.origin();
        }
        snapshot
            .entities
            .iter()
            .find(|state| state.number() == source)
            .map_or(fallback, |state| self.sound_origin(state))
    }

    /// Re-resolve a player sound set when its CS_PLAYERS configstring changes.
    /// Registration and allocation happen only on that lifecycle edge, never
    /// for steady-state event playback (`CG_NewClientInfo`/`CG_LoadCISounds`).
    pub fn refresh_clients(
        &mut self,
        game_state: &GameState,
        vfs: &VirtualFileSystem,
        mut register: impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) {
        let mut saber_definitions = None;
        for client in 0..MAX_CLIENTS {
            let hash = player_config_hash(game_state, client);
            if hash == self.client_config_hash[client] {
                continue;
            }
            self.client_config_hash[client] = hash;
            let sounds = &mut self.sounds;
            let mut intern = |path: &str| intern_sound(sounds, vfs, path, &mut register);
            self.custom[client] = custom_set(game_state, vfs, client as u16, &mut intern);
            self.client_teams[client] = client_team(game_state, client);
            let definitions = saber_definitions
                .get_or_insert_with(|| crate::shared_saber_definitions(vfs).unwrap_or_default());
            self.saber_switch
                .refresh_client(game_state, client, definitions, &mut intern);
        }

        self.loops.refresh_clients(game_state, vfs, &mut register);
        self.local.observe_config(game_state);
    }

    /// Synchronize event latches without producing sound (initial snapshot).
    pub fn synchronize(&mut self, snapshot: &Snapshot) {
        self.predicted_events = Default::default();
        self.player_event_sequence = snapshot.player.event_sequence();
        self.player_events = [0, 1].map(|i| snapshot.player.event(i).unwrap_or(0));
        self.player_external_event = snapshot.player.external_event();
        for entity in &snapshot.entities {
            if let Some(slot) = self
                .previous_entity_event
                .get_mut(usize::from(entity.number()))
            {
                *slot = if entity.entity_type() > ET_EVENTS {
                    1
                } else {
                    entity.event()
                };
                self.last_entity_time[usize::from(entity.number())] = snapshot.server_time;
                self.seen_entity[usize::from(entity.number())] = self.epoch;
            }
        }
    }

    /// Resolve all newly observed supported events without allocation.
    pub fn observe_snapshot(&mut self, snapshot: &Snapshot) -> usize {
        self.predicted_events
            .identity(snapshot.player.client_num(), snapshot.player.entity_flags());
        self.decisions.clear();
        self.music_actions.clear();
        self.maintained.begin_snapshot();
        self.observe_entity_events(snapshot);
        self.observe_player_events(snapshot);
        self.local.observe_snapshot(snapshot);
        for index in 0..self.local.pending().len() {
            let local = self.local.pending()[index];
            let client = snapshot.player.client_num();
            let (event, sound, channel) = (local.event, local.sound, local.channel);
            self.cause = local.cause;
            self.emit(
                event, sound, client, channel, true, false, [0.0; 3], snapshot,
            );
        }
        self.cause = None;
        self.decisions.len()
    }

    pub fn decisions(&self) -> &[LegacySoundDecision] {
        &self.decisions
    }
    /// Rare background-track actions emitted by the latest snapshot.
    pub fn music_actions(&self) -> &[LegacyMusicAction] {
        &self.music_actions
    }
    pub fn sounds(&self) -> &[RegisteredLegacySound] {
        &self.sounds
    }
    pub fn sound(&self, index: u16) -> Option<&RegisteredLegacySound> {
        self.sounds.get(index as usize)
    }
    pub fn ledger(&self) -> &LegacySoundLedger {
        &self.ledger
    }

    /// Build the current rendered frame's fixed-capacity loop requests.
    pub fn observe_loops(
        &mut self,
        snapshot: &Snapshot,
        presented_time: i32,
        listener_origin: [f32; 3],
        presented_origin: impl FnMut(&EntityState) -> [f32; 3],
    ) -> usize {
        self.loops
            .observe(snapshot, presented_time, listener_origin, presented_origin)
    }

    /// Loop requests selected for the most recent presented frame.
    pub fn loop_decisions(&self) -> &[LegacyLoopDecision] {
        self.loops.decisions()
    }

    /// Resolve a loop decision's adapter-local registered sound.
    pub fn loop_sound(&self, index: u16) -> Option<&RegisteredLegacySound> {
        self.loops.sound(index)
    }

    /// Soundset loop frames whose catalog/stage did not resolve.
    pub fn unresolved_loop_soundsets(&self) -> usize {
        self.loops.unresolved_soundsets()
    }

    /// Soundset-backed loop requests observed in the current frame.
    pub fn referenced_loop_soundsets(&self) -> usize {
        self.loops.referenced_soundsets()
    }

    /// Number of retail BMODEL records parsed from `sound/sound.txt`.
    pub fn parsed_bmodel_soundsets(&self) -> usize {
        self.loops.parsed_bmodel_sets()
    }

    /// Requests dropped by the exact cgame/backend fixed loop ceilings.
    pub fn dropped_loop_capacity(&self) -> usize {
        self.loops.dropped_capacity()
    }

    /// Ambient-set one-shots (`S_StartAmbientSound`) started this frame.
    pub fn ambient_shots(&self) -> &[LegacyAmbientShot] {
        self.loops.ambient_shots()
    }

    /// Whether the current frame resolved a global ambient set (0 or 1).
    pub fn ambient_global_frames(&self) -> usize {
        self.loops.ambient_global_frames()
    }

    /// Entities whose local ambient set resolved this frame.
    pub fn ambient_local_frames(&self) -> usize {
        self.loops.ambient_local_frames()
    }

    /// Event-loop starts/stops and mute/channel-kill actions from the latest
    /// snapshot traversal.
    pub fn maintained_actions(&self) -> &[LegacyMaintainedAction] {
        self.maintained.actions()
    }

    /// Rebuild event-maintained cgame loops into backend loop requests.
    pub fn observe_maintained_loops(
        &mut self,
        snapshot: &Snapshot,
        presented_origin: impl FnMut(&EntityState) -> [f32; 3],
    ) -> usize {
        self.maintained
            .build_frame(snapshot, presented_origin)
            .len()
    }

    /// Event-maintained loops selected for the current presented frame.
    pub fn maintained_loop_decisions(&self) -> &[LegacyMaintainedLoopDecision] {
        self.maintained.frame()
    }

    /// Number of persistent cgame loop slots currently alive.
    pub fn active_maintained_loop_slots(&self) -> usize {
        self.maintained.active_slots()
    }

    fn resolve(
        &mut self,
        event: u16,
        raw: u16,
        entity: EventSubject,
        snapshot: &Snapshot,
        time: i32,
    ) {
        let parameter = raw as u8;
        if event == 78 {
            self.local.team_event(parameter);
            return;
        }
        self.cause = event_cause(event, &entity);
        let source = entity.number;
        let client = usize::from(entity.client_num).min(MAX_CLIENTS - 1);
        let variant = deterministic_variant(source, event, parameter);
        // Event-maintained loops live in centity state and are re-added to the
        // backend every frame (`cg_ents.c:109-110,141-192,244-285`).
        match maintained_rule(event, entity.general_channel) {
            Some(MaintainedRule::Door) => {
                let sound = self.ambient_stages[usize::from(entity.soundset)][1];
                self.start_maintained(
                    LegacyMaintainedEvent::PlayDoorLoopSound,
                    source,
                    sound,
                    entity.origin,
                    None,
                );
                return;
            }
            Some(MaintainedRule::Mute) => {
                self.maintained.mute(
                    entity.mute_entity,
                    normalize_voice_channel(entity.tracked_entity.into()),
                );
                return;
            }
            Some(MaintainedRule::TrackedGeneral) => {
                // Unlike the ordinary branch at cg_event.c:3184-3191, the
                // tracked-loop branch at :3176-3183 does not custom-sound
                // fallback when cgs.gameSounds[eventParm] is absent.
                let sound = self.cs_sounds[usize::from(parameter)];
                let tracker = (entity.e_flags & (1 << 24) != 0).then_some(entity.tracked_entity);
                self.start_maintained(
                    LegacyMaintainedEvent::TrackedGeneralSound,
                    source,
                    sound,
                    entity.origin,
                    tracker,
                );
                return;
            }
            Some(MaintainedRule::Stop) => {
                self.maintained
                    .stop(LegacyMaintainedEvent::StopLoopingSound, source);
                return;
            }
            Some(MaintainedRule::Start) => {
                let sound = self.configured_sound(parameter, client);
                self.start_maintained(
                    LegacyMaintainedEvent::StartLoopingSound,
                    source,
                    sound,
                    entity.origin,
                    None,
                );
                return;
            }
            None => {}
        }
        // A blade skin's sound for the event, played over the stock one.
        let mut layered = None;
        let simple = match event {
            125..=127 => Some((
                LegacySoundEvent::Pushed,
                self.custom[client].pushed[usize::from(event - 125)],
                source,
                CHAN_VOICE,
                false,
            )),
            128..=130 => Some((
                LegacySoundEvent::Choke,
                self.custom[client].choke[usize::from(event - 128)],
                source,
                CHAN_VOICE,
                false,
            )),
            190 => Some((
                LegacySoundEvent::PushFail,
                self.custom[client].push_fail,
                source,
                CHAN_VOICE,
                false,
            )),
            2 => Some((
                LegacySoundEvent::Footstep,
                self.footsteps[footstep_kind(parameter)][variant % 4],
                source,
                CHAN_BODY,
                false,
            )),
            3 => Some((
                LegacySoundEvent::FootstepMetal,
                self.footsteps[6][variant % 4],
                source,
                CHAN_BODY,
                false,
            )),
            4..=6 => {
                let kind = [
                    LegacySoundEvent::Footsplash,
                    LegacySoundEvent::Footwade,
                    LegacySoundEvent::Swim,
                ][usize::from(event - 4)];
                // Despite registering separate WADE/SWIM banks, codemp's
                // CG_EntityEvent uses FOOTSTEP_SPLASH for all three cases
                // (`cg_event.c:1438-1457`).
                Some((
                    kind,
                    self.water_steps[0][variant % 4],
                    source,
                    CHAN_BODY,
                    false,
                ))
            }
            11 => {
                let sound = if entity.e_flags & (1 << 1) != 0 {
                    if parameter > 25 {
                        self.fall
                    } else {
                        self.object_hit
                    }
                } else if parameter > 44 {
                    self.fall
                } else {
                    self.land
                };
                Some((LegacySoundEvent::Fall, sound, source, CHAN_AUTO, false))
            }
            14 if [entity.other, entity.other2, entity.ground]
                .contains(&snapshot.player.client_num()) =>
            {
                Some((
                    LegacySoundEvent::GlobalDuel,
                    self.count_fight,
                    snapshot.player.client_num(),
                    CHAN_ANNOUNCER,
                    true,
                ))
            }
            15 if source == snapshot.player.client_num() && parameter == 2 => Some((
                LegacySoundEvent::PrivateDuel,
                self.count_fight,
                source,
                CHAN_ANNOUNCER,
                true,
            )),
            16 => Some((
                LegacySoundEvent::Jump,
                self.custom[client].jump,
                source,
                CHAN_VOICE,
                false,
            )),
            18 => Some((
                LegacySoundEvent::WaterTouch,
                self.water[0],
                source,
                CHAN_AUTO,
                false,
            )),
            19 => Some((
                LegacySoundEvent::WaterLeave,
                self.water[1],
                source,
                CHAN_AUTO,
                false,
            )),
            20 => Some((
                LegacySoundEvent::WaterUnder,
                self.water[2],
                source,
                CHAN_AUTO,
                false,
            )),
            21 => Some((
                LegacySoundEvent::WaterClear,
                self.custom[client].gasp,
                source,
                CHAN_AUTO,
                false,
            )),
            22 => {
                // EV_ITEM_PICKUP carries an entity number, not an eight-bit item index.
                let item = snapshot
                    .entities
                    .iter()
                    .find(|state| state.number() == raw)
                    .map_or(0, EntityState::model_index);
                Some((
                    LegacySoundEvent::ItemPickup,
                    self.item_sounds[item_sound_kind(item)],
                    source,
                    CHAN_AUTO,
                    false,
                ))
            }
            23 => Some((
                LegacySoundEvent::GlobalItemPickup,
                self.item_sounds[item_sound_kind(i16::from(parameter))],
                snapshot.player.client_num(),
                CHAN_AUTO,
                true,
            )),
            25 if source == snapshot.player.client_num()
                && (snapshot.player.vehicle_entity_num() != 0 || snapshot.player.weapon() == 0) =>
            {
                Some((
                    LegacySoundEvent::NoAmmo,
                    self.no_ammo,
                    source,
                    CHAN_AUTO,
                    true,
                ))
            }
            26 => {
                let weapon = usize::from(parameter);
                let sound = self
                    .weapon_select
                    .get(weapon)
                    .copied()
                    .flatten()
                    .or((weapon != 3).then_some(self.select_fallback).flatten());
                sound.map(|sound| {
                    (
                        LegacySoundEvent::ChangeWeapon,
                        Some(sound),
                        source,
                        CHAN_AUTO,
                        false,
                    )
                })
            }
            27 | 28 if entity.vehicle => None,
            27 | 28 => {
                let alt = usize::from(event == 28);
                Some((
                    if alt == 0 {
                        LegacySoundEvent::FireWeapon
                    } else {
                        LegacySoundEvent::AltFire
                    },
                    self.weapons
                        .get(entity.weapon as usize)
                        .and_then(|paths| paths[alt]),
                    source,
                    CHAN_WEAPON,
                    false,
                ))
            }
            29 => {
                layered = self.saber_overrides.swing(source, variant);
                Some((
                    LegacySoundEvent::SaberAttack,
                    self.saber_attack[variant % 8],
                    source,
                    CHAN_WEAPON,
                    false,
                ))
            }
            30 => Some((
                LegacySoundEvent::SaberHit,
                self.saber_hit[variant % 3],
                source,
                CHAN_AUTO,
                false,
            )),
            31 if parameter != 0 => Some((
                LegacySoundEvent::SaberBlock,
                self.saber_block[variant % 9],
                source,
                CHAN_AUTO,
                false,
            )),
            // A client's blade skin or own hilts are voiced below; other entities
            // keep the stock hilt.
            33 if usize::from(source) >= MAX_CLIENTS => Some((
                LegacySoundEvent::SaberUnholster,
                self.saber_on,
                source,
                CHAN_AUTO,
                false,
            )),
            39 if source == snapshot.player.client_num() => Some((
                LegacySoundEvent::DisruptorZoom,
                self.zoom[usize::from(
                    entity.predicted_zoom.unwrap_or(snapshot.player.zoom_mode()) != 0,
                )],
                source,
                CHAN_AUTO,
                true,
            )),
            62 => Some((
                LegacySoundEvent::ItemRespawn,
                self.respawn,
                source,
                CHAN_AUTO,
                false,
            )),
            63 => Some((
                LegacySoundEvent::ItemPop,
                self.respawn,
                source,
                CHAN_AUTO,
                false,
            )),
            64 => Some((
                LegacySoundEvent::TeleportIn,
                self.teleport[0],
                source,
                CHAN_AUTO,
                false,
            )),
            65 => Some((
                LegacySoundEvent::TeleportOut,
                self.teleport[1],
                source,
                CHAN_AUTO,
                false,
            )),
            // `CG_PlayDoorSound` (`cg_ents.c:2796-2821`) for `EV_PLAYDOORSOUND`, which
            // `G_PlayDoorSound` raises on doors, plats and buttons with a soundset, and
            // `EV_BMODEL_SOUND` (`cg_event.c`): the soundset's sound for the stage.
            71 if entity.soundset == 0 => None,
            71 | 73 => self.ambient_stages[usize::from(entity.soundset)]
                .get(usize::from(parameter))
                .copied()
                .flatten()
                .map(|sound| {
                    (
                        if event == 71 {
                            LegacySoundEvent::PlayDoorSound
                        } else {
                            LegacySoundEvent::Bmodel
                        },
                        Some(sound),
                        source,
                        CHAN_AUTO,
                        false,
                    )
                }),
            76 => {
                let sound = self.configured_sound(parameter, client);
                layered =
                    self.saber_overrides
                        .general(sound, &self.sounds, entity.origin, snapshot);
                Some((
                    LegacySoundEvent::General,
                    sound,
                    source,
                    entity.general_channel,
                    false,
                ))
            }
            77 => Some((
                LegacySoundEvent::Global,
                self.configured_sound(parameter, client),
                snapshot.player.client_num(),
                CHAN_MENU1,
                true,
            )),
            79 => Some((
                LegacySoundEvent::Entity,
                self.configured_sound(parameter, client),
                entity.client_num,
                entity.entity_channel,
                false,
            )),
            89 => {
                if self.local_health_pain && source == snapshot.player.client_num() {
                    return;
                }
                if self.pain_deadline[usize::from(source)] > time {
                    return;
                }
                self.pain_deadline[usize::from(source)] = time.saturating_add(500);
                let band = match parameter {
                    0..=24 => 0,
                    25..=49 => 1,
                    50..=74 => 2,
                    _ => 3,
                };
                Some((
                    LegacySoundEvent::Pain,
                    self.custom[client].pain[band],
                    source,
                    CHAN_VOICE,
                    false,
                ))
            }
            90..=92 => Some((
                LegacySoundEvent::Death,
                self.custom[client].death[(event - 90) as usize],
                source,
                CHAN_VOICE,
                false,
            )),
            96 => Some((
                LegacySoundEvent::ForceDrained,
                self.drain,
                entity.owner,
                CHAN_AUTO,
                false,
            )),
            108 | 109 => {
                let alt = usize::from(event == 109);
                Some((
                    if alt == 0 {
                        LegacySoundEvent::WeaponCharge
                    } else {
                        LegacySoundEvent::WeaponChargeAlt
                    },
                    self.weapon_charge
                        .get(usize::from(parameter))
                        .and_then(|pair| pair[alt]),
                    source,
                    CHAN_WEAPON,
                    false,
                ))
            }
            _ => None,
        };
        if let Some((kind, sound, sound_source, channel, listener_relative)) = simple {
            self.emit(
                kind,
                sound,
                sound_source,
                channel,
                listener_relative,
                false,
                entity.origin,
                snapshot,
            );
            // A blade skin's sound plays over the stock one, on a channel of its own
            // that the stock sound's does not cut.
            if let Some(skin) = layered {
                self.emit(
                    kind,
                    Some(skin),
                    sound_source,
                    CHAN_AUTO,
                    listener_relative,
                    true,
                    entity.origin,
                    snapshot,
                );
            }
        } else {
            match event {
                15 if source == snapshot.player.client_num() => {
                    self.music_actions.push(if parameter == 0 {
                        LegacyMusicAction::RestoreMap
                    } else {
                        LegacyMusicAction::StartPrivateDuel
                    });
                    // `cg_event.c:1531-1554` changes the background track
                    // instead of creating an S_StartSound voice.
                    self.ledger.record(LegacySoundEvent::PrivateDuel, true);
                }
                17 => {
                    if parameter != 0 {
                        self.resolve(11, u16::from(parameter), entity, snapshot, time);
                    }
                    self.emit(
                        LegacySoundEvent::Roll,
                        self.custom[client].jump,
                        source,
                        CHAN_VOICE,
                        false,
                        false,
                        entity.origin,
                        snapshot,
                    );
                    self.emit(
                        LegacySoundEvent::Roll,
                        self.roll,
                        source,
                        CHAN_BODY,
                        false,
                        true,
                        entity.origin,
                        snapshot,
                    );
                }
                34 => {
                    self.emit(
                        LegacySoundEvent::BecomeJediMaster,
                        self.saber_on,
                        source,
                        CHAN_AUTO,
                        false,
                        false,
                        entity.origin,
                        snapshot,
                    );
                    if source == snapshot.player.client_num() {
                        self.emit(
                            LegacySoundEvent::BecomeJediMaster,
                            self.happy_music,
                            source,
                            CHAN_LOCAL,
                            true,
                            true,
                            entity.origin,
                            snapshot,
                        );
                    }
                }
                33 => self.emit_unholster(source, entity.origin, snapshot),
                40 if (1..=6).contains(&parameter) => {
                    self.emit(
                        LegacySoundEvent::Predefined,
                        self.predefined[usize::from(parameter - 1)],
                        source,
                        CHAN_AUTO,
                        false,
                        false,
                        entity.fixed_origin,
                        snapshot,
                    );
                }
                41 => {
                    let sound = self.team_power[usize::from(parameter != 1)];
                    for target in 0..MAX_CLIENTS {
                        if client_bit(entity.client_bits, target) {
                            let target = target as u16;
                            self.emit(
                                LegacySoundEvent::TeamPower,
                                sound,
                                target,
                                CHAN_AUTO,
                                false,
                                target != source,
                                entity.origin,
                                snapshot,
                            );
                        }
                    }
                }
                75 => self.emit_voice_command(parameter, entity, snapshot),
                115 => {
                    let sound = self.taunt_sound(client, parameter);
                    self.emit(
                        LegacySoundEvent::Taunt,
                        sound,
                        source,
                        CHAN_VOICE,
                        false,
                        false,
                        entity.origin,
                        snapshot,
                    );
                }
                _ => return,
            }
        }
        if event == 11 && parameter > 44 && entity.e_flags & (1 << 1) == 0 {
            let sound = self.custom[client].land;
            self.pain_deadline[usize::from(source)] = time.saturating_add(500);
            self.emit(
                LegacySoundEvent::Fall,
                sound,
                source,
                CHAN_VOICE,
                false,
                true,
                entity.origin,
                snapshot,
            );
        }
        if (90..=92).contains(&event) && parameter != 0 && source == snapshot.player.client_num() {
            self.emit(
                LegacySoundEvent::Death,
                self.dramatic_failure,
                source,
                CHAN_LOCAL,
                true,
                true,
                entity.origin,
                snapshot,
            );
        }
    }

    fn emit(
        &mut self,
        kind: LegacySoundEvent,
        sound: Option<u16>,
        source: u16,
        channel: u32,
        listener_relative: bool,
        additional: bool,
        origin: [f32; 3],
        snapshot: &Snapshot,
    ) {
        let handle = sound
            .and_then(|index| self.sounds.get(index as usize))
            .and_then(|sound| sound.handle);
        if additional {
            self.ledger.record_additional(handle.is_some());
        } else {
            self.ledger.record(kind, handle.is_some());
        }
        let local = source == snapshot.player.client_num();
        let origin = (!listener_relative && !local)
            .then(|| self.origin_for_source(snapshot, source, origin));
        self.decisions.push(LegacySoundDecision {
            event: kind,
            sound,
            handle,
            additional,
            cause: self.cause,
            request: PlayRequest {
                origin,
                source: SourceId(u32::from(source)),
                channel: ChannelId(normalize_voice_channel(channel)),
                volume: 1.0,
                attenuation: if listener_relative || local {
                    Attenuation::None
                } else {
                    normal_attenuation(channel)
                },
            },
        });
    }

    fn start_maintained(
        &mut self,
        event: LegacyMaintainedEvent,
        source: u16,
        sound: Option<u16>,
        origin: [f32; 3],
        tracker_target: Option<u16>,
    ) {
        let handle = sound
            .and_then(|index| self.sounds.get(usize::from(index)))
            .and_then(|sound| sound.handle);
        self.maintained
            .start(event, source, sound, handle, origin, tracker_target);
    }

    fn emit_voice_command(&mut self, parameter: u8, entity: EventSubject, snapshot: &Snapshot) {
        let client = usize::from(entity.ground);
        if client >= MAX_CLIENTS {
            return;
        }
        let sound = self.configured_sound(parameter, client);
        let same_team = self.client_teams[client] == snapshot.player.team();
        if entity.ground != snapshot.player.client_num() && same_team {
            self.emit(
                LegacySoundEvent::VoiceCommand,
                sound,
                snapshot.player.client_num(),
                CHAN_MENU1,
                true,
                false,
                entity.origin,
                snapshot,
            );
        }
        self.emit(
            LegacySoundEvent::VoiceCommand,
            sound,
            entity.ground,
            CHAN_VOICE,
            false,
            entity.ground != snapshot.player.client_num() && same_team,
            entity.origin,
            snapshot,
        );
    }

    fn configured_sound(&self, parameter: u8, client: usize) -> Option<u16> {
        self.cs_sounds[usize::from(parameter)].or_else(|| {
            self.cs_custom[usize::from(parameter)].and_then(|kind| self.custom[client].get(kind))
        })
    }
}

fn custom_kind(path: &str) -> Option<CustomKind> {
    let name = path
        .trim_start_matches('*')
        .split('.')
        .next()?
        .to_ascii_lowercase();
    match name.as_str() {
        "pushed1" => Some(CustomKind::Pushed(0)),
        "pushed2" => Some(CustomKind::Pushed(1)),
        "pushed3" => Some(CustomKind::Pushed(2)),
        "choke1" => Some(CustomKind::Choke(0)),
        "choke2" => Some(CustomKind::Choke(1)),
        "choke3" => Some(CustomKind::Choke(2)),
        "pushfail" => Some(CustomKind::PushFail),
        "jump1" => Some(CustomKind::Jump),
        "pain25" => Some(CustomKind::Pain(0)),
        "pain50" => Some(CustomKind::Pain(1)),
        "pain75" => Some(CustomKind::Pain(2)),
        "pain100" => Some(CustomKind::Pain(3)),
        "death1" => Some(CustomKind::Death(0)),
        "death2" => Some(CustomKind::Death(1)),
        "death3" => Some(CustomKind::Death(2)),
        "gasp" => Some(CustomKind::Gasp),
        "taunt" => Some(CustomKind::Taunt),
        "anger1" => Some(CustomKind::Anger(0)),
        "anger2" => Some(CustomKind::Anger(1)),
        "anger3" => Some(CustomKind::Anger(2)),
        "victory1" => Some(CustomKind::Victory(0)),
        "victory2" => Some(CustomKind::Victory(1)),
        "victory3" => Some(CustomKind::Victory(2)),
        "taunt1" => Some(CustomKind::TauntNumbered(0)),
        "taunt2" => Some(CustomKind::TauntNumbered(1)),
        "taunt3" => Some(CustomKind::TauntNumbered(2)),
        "deflect1" => Some(CustomKind::Deflect(0)),
        "deflect2" => Some(CustomKind::Deflect(1)),
        "deflect3" => Some(CustomKind::Deflect(2)),
        "gloat1" => Some(CustomKind::Gloat(0)),
        "gloat2" => Some(CustomKind::Gloat(1)),
        "gloat3" => Some(CustomKind::Gloat(2)),
        _ => None,
    }
}

#[path = "sound_event_latches.rs"]
mod event_latches;

/// codemp's per-channel attenuation (`snd_dma.cpp:145-148`, `:1368-1390`):
/// `SOUND_FULLVOLUME` 256 and `SOUND_ATTENUATE` 0.0008 for ordinary channels,
/// the voice-channel offsets/`VOICE_ATTENUATE`, and no falloff for local
/// channels. Pass `CHAN_AUTO` (0) for the default falloff.
pub fn normal_attenuation(channel: u32) -> Attenuation {
    match channel {
        3 => Attenuation::Linear {
            full_volume_distance: 768.0,
            falloff_per_unit: 0.0008,
        },
        4 => Attenuation::Linear {
            full_volume_distance: 345.6,
            falloff_per_unit: 0.004,
        },
        10 => Attenuation::Linear {
            full_volume_distance: 2048.0,
            falloff_per_unit: 0.0008,
        },
        12 => Attenuation::None,
        _ => Attenuation::Linear {
            full_volume_distance: 256.0,
            falloff_per_unit: 0.0008,
        },
    }
}

pub(crate) fn normalize_voice_channel(channel: u32) -> u32 {
    // S_CheckChannelStomp (`snd_dma.cpp:1070-1085`) treats all voice variants
    // as one replacement group; this keeps the generic mixer free of CHAN_*.
    if matches!(channel, 3 | 4 | 12) {
        CHAN_VOICE
    } else {
        channel
    }
}

fn deterministic_variant(entity: u16, event: u16, parameter: u8) -> usize {
    // codemp uses the client rand() stream. Per-entity hashing is deliberately
    // replay deterministic; only variant choice differs, never table/content.
    usize::from(entity).wrapping_mul(1_103_515_245)
        ^ usize::from(event).wrapping_mul(12_345)
        ^ usize::from(parameter)
}

fn footstep_kind(material: u8) -> usize {
    // shared/surfaceflags.h material_t values consumed by cg_event.c:1386-1430.
    match material {
        17 => 1,
        7 => 2,
        8 => 3,
        14 => 4,
        5 | 6 => 5,
        3 => 6,
        4 => 7,
        9 => 8,
        21 | 22 | 24 | 25 | 27 => 9,
        1 | 2 => 10,
        _ => 0,
    }
}

fn item_sound_kind(item: i16) -> usize {
    match item {
        1 | 2 => 0,
        3 => 1,
        15 => 3,
        16 => 4,
        19..=31 | 35..=39 => 2,
        _ => 5,
    }
}

fn custom_set(
    game_state: &GameState,
    vfs: &VirtualFileSystem,
    client: u16,
    intern: &mut impl FnMut(&str) -> u16,
) -> CustomSet {
    let appearance = crate::legacy_client_appearance(game_state, client);
    let model = appearance
        .as_ref()
        .and_then(|appearance| appearance.model.rsplit('/').next())
        .unwrap_or("mp_generic_male");
    let skin = appearance
        .as_ref()
        .map_or("default", |appearance| appearance.variant.as_str());
    let candidates = if skin.eq_ignore_ascii_case("default") {
        [
            format!("models/players/{model}/sounds.cfg"),
            format!("models/players/{model}/sounds_default.cfg"),
        ]
    } else {
        [
            format!("models/players/{model}/sounds_{skin}.cfg"),
            format!("models/players/{model}/sounds.cfg"),
        ]
    };
    let configured = candidates.iter().find_map(|path| {
        let bytes = vfs.read(path).ok().flatten()?.bytes;
        let text = String::from_utf8_lossy(&bytes);
        let directory = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('/') && !line.starts_with('#'))?
            .to_owned();
        let female = text
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            == Some("f");
        Some((directory, female))
    });
    let (directory, female) = configured.unwrap_or_else(|| (model.to_owned(), false));
    let mut sound = |name: &str| {
        let requested = format!("sound/chars/{directory}/misc/{name}.wav");
        let path = if sound_exists(vfs, &requested) {
            requested
        } else {
            let gender = if female { "female" } else { "male" };
            format!("sound/chars/mp_generic_{gender}/misc/{name}.wav")
        };
        Some(intern(&path))
    };
    CustomSet {
        pushed: ["pushed1", "pushed2", "pushed3"].map(&mut sound),
        choke: ["choke1", "choke2", "choke3"].map(&mut sound),
        push_fail: sound("pushfail"),
        jump: sound("jump1"),
        pain: ["pain25", "pain50", "pain75", "pain100"].map(&mut sound),
        death: ["death1", "death2", "death3"].map(&mut sound),
        land: sound("land1"),
        gasp: sound("gasp"),
        taunt: sound("taunt"),
        anger: ["anger1", "anger2", "anger3"].map(&mut sound),
        victory: ["victory1", "victory2", "victory3"].map(&mut sound),
        taunt_numbered: ["taunt1", "taunt2", "taunt3"].map(&mut sound),
        deflect: ["deflect1", "deflect2", "deflect3"].map(&mut sound),
        gloat: ["gloat1", "gloat2", "gloat3"].map(&mut sound),
    }
}

fn client_bit(bits: [u16; 4], client: usize) -> bool {
    client < 64 && bits[client / 16] & (1 << (client % 16)) != 0
}

fn client_team(game_state: &GameState, client: usize) -> u8 {
    config_info_value(
        game_state.config_string(1_131 + client).unwrap_or_default(),
        "t",
    )
    .and_then(|value| value.parse().ok())
    .unwrap_or(0)
}

pub(crate) fn config_info_value<'a>(bytes: &'a [u8], key: &str) -> Option<&'a str> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut fields = text.split('\\').filter(|field| !field.is_empty());
    while let (Some(candidate), Some(value)) = (fields.next(), fields.next()) {
        if candidate.eq_ignore_ascii_case(key) {
            return Some(value);
        }
    }
    None
}

fn sound_exists(vfs: &VirtualFileSystem, requested: &str) -> bool {
    if vfs.contains(requested).unwrap_or(false) {
        return true;
    }
    if let Some(base) = requested.strip_suffix(".wav") {
        return vfs.contains(&format!("{base}.mp3")).unwrap_or(false);
    }
    if let Some(base) = requested.strip_suffix(".mp3") {
        return vfs.contains(&format!("{base}.wav")).unwrap_or(false);
    }
    [".wav", ".mp3"].iter().any(|extension| {
        vfs.contains(&format!("{requested}{extension}"))
            .unwrap_or(false)
    })
}

fn player_config_hash(game_state: &GameState, client: usize) -> u64 {
    game_state
        .config_string(1_131 + client)
        .unwrap_or_default()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
}

pub(crate) fn intern_sound(
    sounds: &mut Vec<RegisteredLegacySound>,
    vfs: &VirtualFileSystem,
    requested: &str,
    register: &mut impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
) -> u16 {
    if let Some(index) = sounds.iter().position(|sound| {
        sound.path.eq_ignore_ascii_case(requested)
            || sound.requested.eq_ignore_ascii_case(requested)
    }) {
        return index as u16;
    }
    let mut resolved = requested.to_owned();
    let mut asset = vfs.read(requested).ok().flatten();
    if asset.is_none() {
        let candidates = if let Some(base) = requested.strip_suffix(".wav") {
            [format!("{base}.mp3"), String::new()]
        } else if let Some(base) = requested.strip_suffix(".mp3") {
            [format!("{base}.wav"), String::new()]
        } else {
            [format!("{requested}.wav"), format!("{requested}.mp3")]
        };
        for candidate in candidates {
            if candidate.is_empty() {
                continue;
            }
            if let Some(found) = vfs.read(&candidate).ok().flatten() {
                resolved = candidate;
                asset = Some(found);
                break;
            }
        }
    }
    let handle = asset
        .as_ref()
        .and_then(|asset| register(&resolved, &asset.bytes));
    if handle.is_none() {
        eprintln!("missing or undecodable legacy sound: {resolved}");
    }
    let index = sounds.len() as u16;
    sounds.push(RegisteredLegacySound {
        path: resolved.into_boxed_str(),
        requested: requested.into(),
        handle,
    });
    index
}

#[path = "sound_config_strings.rs"]
mod config_strings;

#[cfg(test)]
#[path = "sound_origin_tests.rs"]
mod origin_tests;

#[cfg(test)]
#[path = "sound_duplicate_tests.rs"]
mod duplicate_tests;

#[cfg(test)]
mod tests {
    use super::{EventSubject, WEAPON_PATHS, event_cause, is_vehicle};
    use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

    #[test]
    fn saber_hits_blocks_and_voice_commands_name_who_caused_them() {
        let mut event = EventSubject::entity(&EntityState::zero(300, &LEGACY_ENTITY_FIELDS));
        event.other2 = 4;
        event.ground = 7;
        // EV_SABER_HIT and EV_SABER_BLOCK: the saber's owner; EV_VOICECMD_SOUND: the
        // speaker. Their temporary entity is the source, not the player.
        assert_eq!(event_cause(30, &event), Some(4));
        assert_eq!(event_cause(31, &event), Some(4));
        assert_eq!(event_cause(75, &event), Some(7));
        // A block with no owner is not client 0's.
        event.other2 = 0;
        assert_eq!(event_cause(31, &event), None);
        assert_eq!(event_cause(30, &event), Some(0));
        event.other2 = 4;
        // A footstep's source is the player already; an entity past the clients is
        // nobody.
        assert_eq!(event_cause(2, &event), None);
        event.other2 = 1_023;
        assert_eq!(event_cause(30, &event), None);
    }

    #[test]
    fn only_a_vehicle_npc_skips_the_ordinary_weapon_flash() {
        assert!(is_vehicle(13, 53));
        // Players (a rider included), other NPCs and non-NPC entities fire normally.
        assert!(!is_vehicle(1, 53));
        assert!(!is_vehicle(13, 0));
        assert!(!is_vehicle(13, 52));
        assert!(!EventSubject::entity(&EntityState::zero(300, &LEGACY_ENTITY_FIELDS)).vehicle);
    }

    const WP_STUN_BATON: usize = 1; // codemp/game/bg_weapons.h:33
    const WP_MELEE: usize = 2; // codemp/game/bg_weapons.h:34

    #[test]
    fn only_the_stun_baton_has_baton_flash_sounds() {
        let baton = Some("sound/weapons/baton/fire.mp3");
        assert_eq!(WEAPON_PATHS[WP_STUN_BATON], [baton, baton]);
        assert_eq!(WEAPON_PATHS[WP_MELEE], [None, None]);
    }
}
