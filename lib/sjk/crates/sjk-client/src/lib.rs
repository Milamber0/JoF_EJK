//! Stateful legacy multiplayer client composition.

mod actor_color;
mod ambient_sets;
mod ambient_world;
/// Model-authored multiplayer animation sound events.
pub mod animation_events;
mod animation_selection;
mod asset_catalog;
mod base_server_commands;
mod body_animation;
mod shader_remaps;
pub use shader_remaps::{
    SHADER_STATE_CONFIG, ShaderRemapTable, ShaderRemaps, next_remap_order, shader_name,
};
mod catalog_tokens;
mod character_catalog;
mod chat;
mod client_commands;
mod client_info;
pub mod command_history;
pub mod command_rate;
mod compat_profile;
mod crosshair_target;
mod demo_playback;
mod demo_recorder;
pub mod download;
mod download_session;
mod duel_isolation;
pub use duel_isolation::duel_passes_through;
mod entity_models;
mod force_overlay_events;
mod force_overlays;
mod force_profile;
mod force_profile_negotiation;
mod force_rank_reply;
mod freeze_probe;
mod gamestate_probe;
mod ghoul2_pose;
mod impact_events;
mod information;
mod jof_cosmetics;
use sjk_game_jka::intermission;
mod join_bootstrap;
mod join_session;
mod lagometer;
use sjk_game_jka::legacy_animation;
mod legacy_text;
mod local_sounds;
mod loop_sounds;
mod maintained_sounds;
mod map_effects;
mod match_info;
mod mind_trick;
mod missile_effects;
mod muzzle_effects;
mod npc_identity;
mod obituary;
mod permanent_entities;
mod player_angle_rules;
mod player_angles;
mod player_identity;
mod player_lookup;
mod vehicle_missile_effects;
pub use body_animation::{legacy_body_frame, legacy_body_queue_command};
pub use client_info::LegacyClientInfo;
mod player_profile;
mod player_spheres;
mod player_sprites;
pub use sjk_game_jka::pmove;
use sjk_game_jka::pmove_anim;
use sjk_game_jka::pmove_roll;
use sjk_game_jka::pmove_saber;
pub use sjk_game_jka::predicted_events;
mod prediction_error;
pub use sjk_game_jka::prediction_items;
mod presentation;
mod pure_checksums;
pub mod referenced_paks;
pub mod string_table;
pub use permanent_entities::{legacy_permanent_visible, legacy_scene_entities};
mod presentation_equipment;
mod reliable_pacing;
mod retail_assets;
mod saber_clash_flare;
mod saber_command;
mod saber_definitions;
mod saber_move;
use sjk_game_jka::saber_move_data;
pub mod force_wheel;
pub mod selection;
mod server_address;
mod server_clock;
mod session_transition;
mod sound_events;
mod taystjk_cosmetics;
mod team_commands;
mod team_info;
mod thrown_sabers;
mod userinfo_update;
mod view_bob;
mod view_weapon;
mod visual_effect_events;
mod vote_state;
use sjk_game_jka::weapon_data;
mod weapon_selection;

pub use actor_color::{TeamColorPolicy, legacy_body_color, legacy_player_color};
pub use ambient_sets::{AmbientSet, AmbientSetKind, AmbientSets};
pub use ambient_world::{CS_GLOBAL_AMBIENT_SET, LegacyAmbientShot, LegacyRandom};
pub use animation_selection::legacy_predicted_animation_inputs;
/// EternalJK's player animation fixes for the viewer's own predicted player
/// ([`animation_selection::ejk_animation_fixes`]); returns `(legs, torso)`.
pub fn legacy_ejk_animation_fixes(
    legs: u16,
    torso: u16,
    weapon: u8,
    saber_in_flight: bool,
) -> (u16, u16) {
    animation_selection::ejk_animation_fixes(legs, torso, weapon, saber_in_flight, false)
}

pub use entity_models::{LegacyMoverModel, legacy_limb, legacy_mover_model2};

/// `BG_InDeathAnim`: a death or dead animation.
pub fn legacy_death_animation(clip: usize) -> bool {
    animation_selection::death_animation(clip)
}
pub use asset_catalog::{
    LEGACY_SABER_COLORS, LegacyAssetCatalog, LegacyAssetCatalogLoader, LegacyCatalogStatus,
    legacy_asset_catalog,
};
pub use base_server_commands::{
    BaseServerCommandEvent, BodyIdentity, ForceRankUpdate, Ghoul2Restore, ImmediateBodyCopy,
    parse_new_force_rank, parse_restore_client_ghoul,
};
pub use character_catalog::{
    LegacyCharacter, LegacyCharacterCatalog, LegacySpecies, LegacySpeciesColor,
    legacy_character_catalog,
};
pub use chat::{
    CHAT_INPUT_BYTES, ChatDestination, ChatRoster, ChatTarget, chat_body, chat_command,
    chat_display_text, chat_input_fits, chat_name_key, chat_plain_text, chat_unescape,
};
pub use client_commands::{CompatConsoleCommand, console_commands as compat_console_commands};
pub use compat_profile::{CompatProfile, PLUGIN_DISABLE_DEFAULT, PLUGIN_IDENTITY_KEY};
pub use crosshair_target::{CrosshairCandidate, CrosshairName, crosshair_name};
pub use demo_playback::{DemoAdvance, DemoPlayback, DemoPlaybackError, legacy_presentation_times};
pub use demo_recorder::{
    DemoMessageKind, DemoRecorder, DemoRecorderError, DemoRecordingStats, legacy_demo_filename,
};
pub use force_overlay_events::{LegacyForceOverlayTracker, LegacyShieldHit};
pub use force_overlays::{
    LegacyActorOverlayState, LegacyForceOverlayContext, LegacyOverlayList, LegacyOverlayRandom,
    LegacyOverlayRequest, LegacyOverlayTint, LegacyTeamPowerEffect, legacy_force_overlays,
};
pub use force_profile::{
    FORCE_POWER_COUNT, ForceAllocation, ForceLegalizeRules, ForcePower, ForceProfileError,
    ForceSide, LegalizedForcePowers, legalize_force_powers, mastery_points,
};
pub use force_profile_negotiation::{
    EnterPlayOutcome, ForceProfileNegotiator, ProfileApplied, RejoinOutput, enter_play,
    server_legal_forcepowers,
};
pub use force_rank_reply::{
    ForceRankReply, force_rank_reply, force_rules_from_serverinfo, server_force_rules,
};
pub use ghoul2_pose::{LegacyGhoul2Animator, LegacyGhoul2PosePolicy};
pub use impact_events::{
    EV_DISRUPTOR_HIT, EV_DISRUPTOR_MAIN_SHOT, EV_DISRUPTOR_SNIPER_MISS, EV_DISRUPTOR_SNIPER_SHOT,
    EV_MISSILE_HIT, EV_MISSILE_MISS, EV_MISSILE_MISS_METAL, EV_SABER_BLOCK, EV_SABER_CLASHFLARE,
    EV_SABER_HIT, LegacyImpactEvent, LegacyImpactKind, LegacyImpactTracker,
    kind as legacy_impact_kind, legacy_byte_to_direction, legacy_direction_to_byte,
    legacy_impact_latches_saber_flare,
};
pub use information::{
    HudDataSource, LegacyHudValues, legacy_hud_data, legacy_hud_values, legacy_predicted_hud_data,
    legacy_saber_style_name,
};
pub use intermission::{IntermissionView, PM_INTERMISSION, suppresses_movement};
pub use jof_cosmetics::{
    CosmeticSlot, MAX_COSMETIC_NAME, join_color_value, split_color_value, valid_cosmetic_name,
    worn_cosmetic,
};
pub use lagometer::{LAG_SAMPLES, LagometerSamples, SnapshotSample, connection_interrupted};
pub use legacy_text::decode_legacy;
pub use loop_sounds::{
    LegacyLoopDecision, LegacyLoopKind, legacy_doppler_config, legacy_doppler_scale,
};
pub use maintained_sounds::{
    LegacyMaintainedAction, LegacyMaintainedActionKind, LegacyMaintainedEvent,
    LegacyMaintainedLoopDecision,
};
pub use map_effects::{LegacyMapEffectMetrics, LegacyMapEffectRequest, LegacyMapEffects};
pub use match_info::{MatchClock, WarmupText, legacy_match_clock, legacy_warmup_text};
pub use mind_trick::{
    LegacyTrickFade, LegacyTrickFades, legacy_entity_trick_targets, legacy_mind_tricked,
    legacy_player_trick_targets,
};
pub use missile_effects::{
    LegacyMissileEffectMetrics, LegacyMissileEffectRequest, LegacyMissileEffects,
    LegacyMissileLight, legacy_missile_mode,
};
pub use muzzle_effects::{
    LEGACY_MUZZLE_EFFECTS, LegacyMuzzleEffectPair, LegacyMuzzleEffectRequest, LegacyMuzzleEffects,
    legacy_muzzle_effect, legacy_muzzle_window,
};
pub use npc_identity::{
    legacy_npc_appearance, legacy_npc_body_model, legacy_npc_saber_names,
    legacy_npc_saber_names_borrowed, legacy_npc_state, legacy_vehicle_name,
};
pub use obituary::{
    EV_OBITUARY, Gender, KillFeed, ObituaryEvent, ObituaryTracker, legacy_obituary, obituary_name,
};
pub use player_angle_rules::{PredictedPoseFields, legacy_predicted_pose};
pub use player_angles::{
    LegacyPlayerAngleController, LegacyPlayerAngleInputs, LegacyPlayerAngleSample,
};
pub use player_identity::{
    legacy_client_appearance, legacy_client_appearance_forced, legacy_client_saber_name,
    legacy_client_saber_names,
};
pub use player_lookup::{PlayerLookup, lookup_player};
pub use player_profile::{
    PlayerProfile, PlayerProfileError, SaberColor, pack_saber_rgb, unpack_saber_rgb,
};
pub use player_spheres::{
    LEGACY_PLAYER_SPHERE_MODEL, LEGACY_PLAYER_SPHERE_SHADERS, LegacyJetpack, LegacyPlayerSphere,
    legacy_player_spheres,
};
pub use player_sprites::{
    LEGACY_PLAYER_SPRITE_HEIGHT, LEGACY_PLAYER_SPRITE_RADIUS, LegacyPlayerSprite,
    legacy_player_sprite,
};
pub use pmove_anim::{AnimationLengthTable, AnimationLengths, AnimationTiming};
pub use pmove_roll::{PMF_ROLLING, RollRules};
pub use pmove_saber::move_is_predicted as legacy_saber_move_is_predicted;
pub use prediction_error::{DEFAULT_ERROR_DECAY_MILLIS, EF_TELEPORT_BIT, PredictionErrorDecay};
pub use presentation::{
    LegacyMoverPresentation, legacy_angles_to_quaternion, legacy_evaluate_trajectory,
    legacy_evaluate_trajectory_angles, legacy_evaluate_trajectory_delta, legacy_item_appearance,
    legacy_model_appearance, legacy_present_mover,
};
pub use presentation::{LegacyWorldAdapter, legacy_predicted_equipment};
pub use pure_checksums::{legacy_pure_checksum_command, legacy_server_is_pure};
pub use retail_assets::{RETAIL_ARCHIVES, RETAIL_DIRECTORY, is_retail_mount, retail_view};
pub use saber_clash_flare::{
    LegacySaberClashFlare, LegacySaberClashSample, LegacySaberClashVisibility,
};
pub use saber_command::{SaberChangeNotifier, profile_accepts_saber_command, saber_command};
pub use saber_definitions::{
    LegacySaberColor, LegacySaberDefinition, LegacySaberType, legacy_saber_definitions,
    legacy_saber_hands, legacy_saber_movement, legacy_sabers_forbid_rolls,
    shared_saber_definitions,
};
pub use saber_move::legacy_saber_trail_length;
pub use server_address::{LegacyAddressError, LegacyServerAddress};
pub use server_clock::ServerClock;
pub use session_transition::{SessionTransition, SessionTransitionKind};
pub use sound_events::{
    LegacyMusicAction, LegacySaberView, LegacySoundAdapter, LegacySoundDecision, LegacySoundEvent,
    LegacySoundLedger, RegisteredLegacySound, SABER_SOUND_CLIENTS, SaberSoundSet, TOGGLE_REACH,
    normal_attenuation as legacy_sound_attenuation,
};
pub use taystjk_cosmetics::{
    CosmeticUnlock, CosmeticUnlockTable, MAX_COSMETIC_UNLOCKS, apply_taystjk_cosmetics,
};
pub use team_commands::{LegacyTeamChoice, legacy_team_command};
pub use team_info::{
    MAX_TEAM_CLIENTS, TeamInfo, TeamInfoError, TeamInfoTable, legacy_team_location,
};
pub use userinfo_update::UserinfoUpdateStatus;
pub use view_bob::{
    LegacyFirstPersonOffset, LegacyFirstPersonView, LegacyViewBobConfig, LegacyViewBobSample,
    LegacyViewPlayer,
};
pub use view_weapon::{
    LegacyViewModel, LegacyViewWeaponAnimations, LegacyViewWeaponFrames, LegacyViewWeaponPose,
    legacy_view_model, legacy_view_weapon_frames, legacy_view_weapon_pose,
    legacy_view_weapon_visible, legacy_view_weapon_visible_as, legacy_view_weapons,
    map_torso_to_weapon_frame,
};
pub use visual_effect_events::legacy_visual_effect;
pub use vote_state::{
    LegacyCallVote, LegacyCallVoteError, LegacyVoteScope, VoteState, legacy_global_vote,
    legacy_team_vote, write_legacy_callvote,
};
pub use weapon_data::{
    LEGACY_WEAPON_COUNT, LEGACY_WEAPON_DATA, LegacyWeaponData, legacy_weapon_data,
};
pub use weapon_selection::{
    LegacyWeaponInventory, legacy_cycle_weapon, legacy_direct_weapon, legacy_melee_weapon,
    legacy_weapon_selectable,
};

pub use sjk_game_jka::{legacy_animation_count, legacy_animation_name};

use sjk_network::{
    ConnectPhase as NetworkConnectPhase, LegacyConnection, LegacyUserInfo, NetworkError,
    connect_legacy_with_userinfo_extensions_observed, legacy_userinfo_payload_with_extensions,
    query_server_info,
};
use sjk_protocol::{
    GameState, GameStateError, InfoString, InfoStringError, MessageError, MessageReader,
    ServiceCommand, Snapshot, SnapshotError, UserCommand, decode_initial_gamestate,
    decode_snapshot,
};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

type PureCommandBuilder = Box<dyn Fn(&GameState) -> Option<Vec<u8>> + Send>;

const SNAPSHOT_HISTORY: usize = 32;
mod config_string_commands;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerEventKind {
    Print,
    Chat,
    TeamChat,
    CenterPrint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerEvent {
    pub kind: ServerEventKind,
    pub text: String,
    /// Sender slot emitted by codemp's G_SayTo, absent for system/older-mod text.
    pub sender: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreEntry {
    pub client_num: u8,
    pub score: i32,
    pub ping: i32,
    pub time_minutes: i32,
    pub flags: u32,
    /// Powerup bits (`powerUps`), which carry the flag a player holds.
    pub powerups: u32,
    /// CTF defends (`defendCount`).
    pub defends: i32,
    /// CTF assists (`assistCount`).
    pub assists: i32,
    /// CTF flag captures (`captures`).
    pub captures: i32,
}

mod local_session;
pub use local_session::LocalSimulation;

pub struct ClientSession {
    local: Option<Box<dyn LocalSimulation>>,
    origin_server: SocketAddr,
    retired_world: Option<(GameState, Snapshot)>,
    download_storage: Option<Box<dyn download::DownloadStorage>>,
    pending_download: Option<sjk_network::ServerMessage>,
    downloaded_message: Option<sjk_network::ServerMessage>,
    connection: Option<LegacyConnection>,
    game_state: GameState,
    config_string_dirty: sjk_protocol::ConfigStringDirty,
    shader_remaps: ShaderRemaps,
    server_id: i32,
    latest_snapshot: Snapshot,
    history: VecDeque<Snapshot>,
    reliable_sequence: i32,
    client_reliable_sequence: i32,
    client_reliable_acknowledge: i32,
    pending_client_commands: VecDeque<(i32, Vec<u8>)>,
    command_history: command_history::CommandHistory,
    gamestate_probe: gamestate_probe::GamestateProbe,
    highest_server_command: i32,
    pending_big_config_string: Option<(usize, Vec<u8>)>,
    events: VecDeque<ServerEvent>,
    transitions: VecDeque<SessionTransition>,
    restart_received_at: Option<Instant>,
    pure_command_builder: PureCommandBuilder,
    request_full_snapshot: bool,
    disconnected: bool,
    scores: Vec<ScoreEntry>,
    team_scores: [i32; 2],
    team_info: TeamInfoTable,
    base_command_events: VecDeque<BaseServerCommandEvent>,
    unknown_command_names: Vec<Vec<u8>>,
    compat_profile: CompatProfile,
    cosmetic_unlocks: CosmeticUnlockTable,
    userinfo_updates: userinfo_update::UserinfoUpdateTracker,
    command_pacer: reliable_pacing::ReliableCommandPacer,
    demo_recorder: DemoRecorder,
}

/// Observable milestones and diagnostic replies during protocol-26 bootstrap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoinPhase<'a> {
    /// A validated challenge response was received.
    Challenge,
    /// The server accepted the connect request.
    Connected,
    /// The initial gamestate was decoded.
    Gamestate,
    /// The first usable authoritative snapshot was decoded.
    FirstSnapshot,
    /// An unrecognized connectionless command from the server during handshake.
    UnknownReply(&'a [u8]),
}

impl ClientSession {
    pub fn join(server: SocketAddr, name: &str, timeout: Duration) -> Result<Self, ClientError> {
        Self::join_with_pure(server, name, timeout, |_| None)
    }

    pub fn join_with_pure(
        server: SocketAddr,
        name: &str,
        timeout: Duration,
        build_pure_command: impl Fn(&GameState) -> Option<Vec<u8>> + Send + 'static,
    ) -> Result<Self, ClientError> {
        Self::join_with_pure_userinfo(
            server,
            &LegacyUserInfo::with_name(name),
            timeout,
            build_pure_command,
        )
    }

    /// Join using shell-supplied legacy userinfo fields while preserving the
    /// protocol-26 connection and pure-handshake behavior.
    pub fn join_with_pure_userinfo(
        server: SocketAddr,
        userinfo: &LegacyUserInfo,
        timeout: Duration,
        build_pure_command: impl Fn(&GameState) -> Option<Vec<u8>> + Send + 'static,
    ) -> Result<Self, ClientError> {
        let profile = query_server_info(server, timeout.min(Duration::from_millis(650)))
            .map(|info| CompatProfile::from_server_info(&info))
            .unwrap_or(CompatProfile::BaseJka);
        Self::join_with_pure_userinfo_profile(
            server,
            userinfo,
            profile,
            timeout,
            build_pure_command,
        )
    }

    /// Join with userinfo extensions owned by an explicitly detected profile.
    pub fn join_with_pure_userinfo_profile(
        server: SocketAddr,
        userinfo: &LegacyUserInfo,
        profile: CompatProfile,
        timeout: Duration,
        build_pure_command: impl Fn(&GameState) -> Option<Vec<u8>> + Send + 'static,
    ) -> Result<Self, ClientError> {
        Self::join_with_pure_userinfo_profile_observed(
            server,
            userinfo,
            profile,
            timeout,
            build_pure_command,
            |_| {},
        )
    }

    /// Join while reporting the major protocol-26 bootstrap milestones.
    pub fn join_with_pure_userinfo_profile_observed(
        server: SocketAddr,
        userinfo: &LegacyUserInfo,
        profile: CompatProfile,
        timeout: Duration,
        build_pure_command: impl Fn(&GameState) -> Option<Vec<u8>> + Send + 'static,
        observe: impl FnMut(JoinPhase<'_>),
    ) -> Result<Self, ClientError> {
        Self::join_with_downloads(
            server,
            userinfo,
            profile,
            timeout,
            build_pure_command,
            observe,
            None,
        )
    }

    /// Socket counters, for telling a quiet server apart from packets that
    /// arrive and are dropped before they become snapshots.
    pub fn connection_traffic(&self) -> sjk_network::ConnectionTraffic {
        self.connection
            .as_ref()
            .map_or_else(Default::default, |c| c.traffic())
    }

    /// Source and opening bytes of the most recently discarded packet.
    pub fn last_rejected_packet(&self) -> Option<(SocketAddr, &[u8])> {
        self.connection
            .as_ref()
            .and_then(|c| c.last_rejected_packet())
    }

    pub fn server(&self) -> SocketAddr {
        self.origin_server
    }

    pub fn game_state(&self) -> &GameState {
        &self.game_state
    }

    /// `sv_serverid` of the gamestate currently in force; changes on every
    /// replacement gamestate (map change or restart).
    pub fn server_id(&self) -> i32 {
        self.server_id
    }

    /// Pending notifications for snapshot consumers; the frame owner drains them later.
    pub fn config_string_changes(&self) -> &sjk_protocol::ConfigStringDirty {
        &self.config_string_dirty
    }

    /// Consume changed configstring indices once, without allocation.
    pub fn drain_config_string_changes(&mut self, visit: impl FnMut(usize)) {
        self.config_string_dirty.drain(visit);
    }

    /// Compatibility policy detected from the current gamestate serverinfo.
    pub fn compat_profile(&self) -> &CompatProfile {
        &self.compat_profile
    }

    /// TaystJK cosmetic unlock requirements from the latest reliable command.
    pub fn cosmetic_unlocks(&self) -> &CosmeticUnlockTable {
        &self.cosmetic_unlocks
    }

    pub fn latest_snapshot(&self) -> &Snapshot {
        &self.latest_snapshot
    }

    /// Begin a protocol-26 demo from the current active gamestate.
    pub fn start_demo_recording(
        &mut self,
        config_directory: &Path,
        name: Option<&str>,
        now: SystemTime,
    ) -> Result<&Path, DemoRecorderError> {
        if self.is_local() {
            return Err(DemoRecorderError::LocalContinuation);
        }
        let result = self.demo_recorder.start(
            !self.disconnected,
            config_directory,
            name,
            now,
            self.latest_snapshot.message_sequence.wrapping_sub(1),
            self.client_reliable_sequence,
            &self.game_state,
        );
        if result.is_ok() {
            // `CL_WritePacket` disables delta requests while `demowaiting`
            // (`codemp/client/cl_input.cpp:1554-1559`), forcing the full
            // snapshot that begins a self-contained demo stream.
            self.request_full_snapshot = true;
        }
        result
    }

    /// Finish the active demo recording, if any.
    pub fn stop_demo_recording(&mut self) -> Result<Option<PathBuf>, DemoRecorderError> {
        self.demo_recorder.stop()
    }

    /// Whether this session currently owns an open demo file.
    pub fn is_recording_demo(&self) -> bool {
        self.demo_recorder.is_recording()
    }

    /// Counters for the current or most recently stopped recording.
    pub fn demo_recording_stats(&self) -> &DemoRecordingStats {
        self.demo_recorder.stats()
    }

    /// Return the newest retained snapshot at or before a cgame presentation
    /// time, mirroring codemp's `cg.snap` choice while `cg.nextSnap` is newer.
    /// No decoding or wire state is changed by this read-only lookup.
    pub fn snapshot_at_or_before(&self, server_time: i32) -> &Snapshot {
        if self.local.is_some() {
            return &self.latest_snapshot;
        }
        if self.latest_snapshot.server_time <= server_time {
            return &self.latest_snapshot;
        }
        self.history
            .iter()
            .rev()
            .find(|snapshot| snapshot.server_time <= server_time)
            .or_else(|| self.history.front())
            .unwrap_or(&self.latest_snapshot)
    }

    /// The next retained snapshot after a presentation time, for interpolation.
    /// This is a read-only presentation lookup; it never changes wire history.
    pub fn snapshot_after(&self, server_time: i32) -> Option<&Snapshot> {
        self.history
            .iter()
            .find(|snapshot| snapshot.server_time > server_time)
            .or_else(|| {
                (self.latest_snapshot.server_time > server_time).then_some(&self.latest_snapshot)
            })
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = ServerEvent> + '_ {
        self.events.drain(..)
    }

    /// Drain typed session lifecycle changes discovered while parsing server
    /// messages. A replacement gamestate event is completed with the first
    /// snapshot sequence before `receive_snapshot` returns it.
    pub fn drain_transitions(&mut self) -> impl Iterator<Item = SessionTransition> + '_ {
        self.transitions.drain(..)
    }

    pub fn scores(&self) -> &[ScoreEntry] {
        &self.scores
    }

    /// Discard cached rows when a legacy client first opens a stale scoreboard.
    /// Team totals remain available until the next authoritative `scores` command.
    pub fn clear_scores(&mut self) {
        self.scores.clear();
    }

    pub fn team_scores(&self) -> [i32; 2] {
        self.team_scores
    }

    /// Latest fixed-capacity team-overlay state supplied by `tinfo`.
    pub fn team_info(&self) -> &TeamInfoTable {
        &self.team_info
    }

    /// Pop one typed BaseJKA action while preserving reliable order.
    pub fn pop_base_command_event(&mut self) -> Option<BaseServerCommandEvent> {
        self.base_command_events.pop_front()
    }

    /// Remove the oldest pending `nfr` notice, leaving other actions queued.
    pub fn take_force_rank_event(&mut self) -> Option<ForceRankUpdate> {
        let index = self
            .base_command_events
            .iter()
            .position(|event| matches!(event, BaseServerCommandEvent::ForceRank(_)))?;
        match self.base_command_events.remove(index) {
            Some(BaseServerCommandEvent::ForceRank(update)) => Some(update),
            _ => None,
        }
    }

    /// Send `command` in a packet of its own (with the `cl_packetdup` repeats).
    pub fn send_command(&mut self, command: &UserCommand) -> Result<(), ClientError> {
        self.queue_command(command);
        self.send_queued_commands()
    }

    /// Hold `command` for the next move packet (see [`command_history`]).
    pub fn queue_command(&mut self, command: &UserCommand) {
        if let Some(local) = &mut self.local {
            local.command(command);
            return;
        }
        if self.pending_download.is_some() {
            return; // Stay CS_PRIMED until donedl; never enter while downloading.
        }
        self.command_history.queue(*command);
    }

    /// Commands waiting for a move packet.
    pub fn queued_commands(&self) -> usize {
        self.command_history.unsent()
    }

    /// Send one move packet with every queued command; nothing when none waits.
    pub fn send_queued_commands(&mut self) -> Result<(), ClientError> {
        if self.local.is_some()
            || self.pending_download.is_some()
            || self.command_history.unsent() == 0
        {
            return Ok(());
        }
        self.pump_reliable_commands(Instant::now())?;
        let pending = self.pending_client_commands.iter().collect::<Vec<_>>();
        let pending = pending
            .iter()
            .map(|(sequence, command)| (*sequence, command.as_slice()))
            .collect::<Vec<_>>();
        let batch = self.command_history.take_packet();
        self.gamestate_probe.sample(
            Instant::now(),
            gamestate_probe::MovePacketHead {
                server_id: self.server_id,
                message_acknowledge: self.latest_snapshot.message_sequence,
                server_command_sequence: self.reliable_sequence,
                command_stamp: batch.last().expect("nonempty command batch").server_time,
                snapshot_time: self.latest_snapshot.server_time,
                command_time: self.latest_snapshot.player.command_time(),
                highest_server_command: self.highest_server_command,
            },
        );
        self.connection
            .as_mut()
            .expect("network session")
            .send_user_commands_with_reliables(
                self.server_id,
                self.latest_snapshot.message_sequence,
                self.reliable_sequence,
                self.game_state.checksum_feed,
                // CL_WritePacket keeps clc_moveNoDelta active for all of demowaiting,
                // even when an in-flight delta cleared the general recovery flag.
                !self.request_full_snapshot && !self.demo_recorder.waiting_for_full_snapshot(),
                &pending,
                batch,
            )?;
        Ok(())
    }

    /// Set how many earlier packets' commands each move packet repeats
    /// (`cl_packetdup`, clamped to the stock 0..=5).
    pub fn set_packet_dup(&mut self, packet_dup: usize) {
        self.command_history.set_packet_dup(packet_dup);
    }

    /// Current command redundancy (`cl_packetdup`).
    pub fn packet_dup(&self) -> usize {
        self.command_history.packet_dup()
    }

    /// Queue a reliable string command for the server.
    ///
    /// Commands leave in order, at most one per `sv_floodProtect` window
    /// (see [`reliable_pacing`]); a command that cannot go out immediately is
    /// released by a later `send_command`, `receive_snapshot` or
    /// [`Self::pump_reliable_commands`] call.
    ///
    /// A command that is UTF-8 text leaves as legacy clients read it: in
    /// Windows-1252 when every character has a byte there, as UTF-8 otherwise
    /// ([`sjk_protocol::encode_legacy_text`]). Other bytes pass unchanged.
    pub fn send_reliable_command(&mut self, command: &[u8]) -> Result<(), ClientError> {
        let command = legacy_text::legacy_command(command);
        if let Some(local) = &mut self.local {
            local.reliable(&command);
            return Ok(());
        }
        if !self.command_pacer.push(command.into_owned()) {
            return Err(ClientError::ReliableCommandOverflow);
        }
        self.pump_reliable_commands(Instant::now())
    }

    /// Release the next paced reliable command if its window has opened.
    pub fn pump_reliable_commands(&mut self, now: Instant) -> Result<(), ClientError> {
        if let Some(command) = self.command_pacer.pop_due(now) {
            self.queue_reliable_command(command)?;
            self.send_pending_reliable_commands()?;
        }
        Ok(())
    }

    /// Reliable commands accepted but not yet released to the server.
    pub fn queued_reliable_commands(&self) -> usize {
        self.command_pacer.queued()
    }

    /// Send a changed userinfo payload through the stock reliable-command path.
    ///
    /// Calls are coalesced to at most one effective update per second. The
    /// payload omits connect-only keys, matching `CL_CheckUserinfo` in OpenJK
    /// (`codemp/client/cl_main.cpp:2157-2169`).
    pub fn update_userinfo(
        &mut self,
        userinfo: &LegacyUserInfo,
        now: Instant,
    ) -> Result<UserinfoUpdateStatus, ClientError> {
        self.update_userinfo_options(userinfo, None, now)
    }

    fn send_pending_reliable_commands(&mut self) -> Result<(), ClientError> {
        let pending = self.pending_client_commands.iter().collect::<Vec<_>>();
        let pending = pending
            .iter()
            .map(|(sequence, command)| (*sequence, command.as_slice()))
            .collect::<Vec<_>>();
        self.connection
            .as_mut()
            .expect("network session")
            .send_reliable_commands(
                self.server_id,
                self.latest_snapshot.message_sequence,
                self.reliable_sequence,
                &pending,
            )?;
        Ok(())
    }

    fn acknowledge_client_commands(&mut self, acknowledge: i32) {
        let acknowledge = acknowledge.min(self.client_reliable_sequence);
        if acknowledge <= self.client_reliable_acknowledge {
            return;
        }
        self.client_reliable_acknowledge = acknowledge;
        while self
            .pending_client_commands
            .front()
            .is_some_and(|(sequence, _)| *sequence <= acknowledge)
        {
            self.pending_client_commands.pop_front();
        }
    }

    /// Receive the next snapshot; zero timeout drains only immediately queued network data.
    pub fn receive_snapshot(&mut self, timeout: Duration) -> Result<&Snapshot, ClientError> {
        if let Some(local) = &mut self.local {
            return if local.receive(
                &mut self.game_state,
                &mut self.latest_snapshot,
                &mut self.config_string_dirty,
            ) {
                let mut commands = std::mem::take(&mut self.latest_snapshot.server_commands);
                for command in &commands {
                    self.apply_server_command(&command.command)?;
                }
                commands.clear();
                self.latest_snapshot.server_commands = commands;
                Ok(&self.latest_snapshot)
            } else {
                Err(NetworkError::TimedOut("local snapshot pending").into())
            };
        }
        if self.pending_download.is_some() {
            return Err(NetworkError::TimedOut("content download pending").into());
        }
        self.pump_reliable_commands(Instant::now())?;
        loop {
            let message = match self.downloaded_message.take() {
                Some(message) => message,
                None => self
                    .connection
                    .as_mut()
                    .expect("network session")
                    .receive_server_message(timeout)?,
            };
            // LegacyConnection has already removed/reassembled the netchan
            // sequence and fragment headers and reversed the command-key XOR.
            // This is therefore the same decoded `msg->data + headerBytes`
            // range saved at `codemp/client/cl_main.cpp:2052-2104`.
            let requested_delta = snapshot_delta_sequence(&message.payload, message.sequence).ok();
            let message_kind = match requested_delta {
                Some(-1) => DemoMessageKind::FullSnapshot,
                Some(_) => DemoMessageKind::DeltaSnapshot,
                None => DemoMessageKind::Other,
            };
            let delta_base = requested_delta.and_then(|sequence| {
                if self.latest_snapshot.message_sequence == sequence {
                    Some(&self.latest_snapshot)
                } else {
                    self.history
                        .iter()
                        .find(|snapshot| snapshot.message_sequence == sequence)
                }
            });
            let snapshot = match decode_snapshot(
                &message.payload,
                message.sequence,
                &self.game_state,
                delta_base,
            ) {
                Ok(snapshot) => snapshot,
                Err(SnapshotError::UnexpectedCommand(command))
                    if is_replacement_gamestate(command) =>
                {
                    self.demo_recorder.stop()?;
                    let initial = decode_initial_gamestate(&message.payload)?;
                    if let Some(storage) = &self.download_storage
                        && !storage
                            .missing(&initial.game_state)
                            .map_err(ClientError::Download)?
                            .is_empty()
                    {
                        self.pending_download = Some(message);
                        return Err(NetworkError::TimedOut("content download pending").into());
                    }
                    let old_map = legacy_map_name(&self.game_state);
                    let new_map = legacy_map_name(&initial.game_state);
                    let system_info = initial
                        .game_state
                        .config_string(1)
                        .ok_or(ClientError::MissingSystemInfo)?;
                    let system_info = std::str::from_utf8(system_info)
                        .map_err(|_| ClientError::NonUtf8SystemInfo)?;
                    let system_info_text = system_info;
                    let system_info = InfoString::parse(system_info)?;
                    let previous_server_id = self.server_id;
                    self.server_id = system_info
                        .get_i32("sv_serverid")
                        .ok_or(ClientError::MissingServerId)?;
                    eprintln!(
                        "received replacement gamestate: serverId={} sequence={} map={:?}",
                        self.server_id, message.sequence, new_map
                    );
                    self.gamestate_probe.arm(
                        Instant::now(),
                        previous_server_id,
                        self.server_id,
                        system_info_text,
                    );
                    // The commands the server flushed ahead of this
                    // gamestate carry the text both sides key packets with
                    // (`sv_client.cpp:429-433`), so they must be recorded
                    // before the sequence jumps past them, and applied for
                    // their own sake.
                    for command in &initial.server_commands {
                        self.gamestate_probe
                            .command(command.sequence, &command.command);
                        self.connection
                            .as_mut()
                            .expect("network session")
                            .record_server_command(command.sequence, &command.command);
                        self.highest_server_command =
                            self.highest_server_command.max(command.sequence);
                    }
                    for command in &initial.server_commands {
                        if command.sequence > self.reliable_sequence {
                            self.apply_server_command(&command.command)?;
                        }
                    }
                    self.reliable_sequence = initial.game_state.server_command_sequence;
                    self.acknowledge_client_commands(initial.reliable_acknowledge);
                    self.command_pacer.clear();
                    self.retired_world.get_or_insert_with(|| {
                        (self.game_state.clone(), self.latest_snapshot.clone())
                    });
                    self.shader_remaps.reset(&initial.game_state);
                    self.game_state = initial.game_state;
                    self.config_string_dirty.mark_all();
                    self.pending_big_config_string = None;
                    self.compat_profile = CompatProfile::from_game_state(&self.game_state);
                    self.cosmetic_unlocks = CosmeticUnlockTable::default();
                    self.team_info = TeamInfoTable::default();
                    self.base_command_events.clear();
                    self.history.clear();
                    self.command_history.begin_gamestate();
                    self.request_full_snapshot = true;
                    self.transitions.push_back(SessionTransition {
                        kind: if old_map == new_map {
                            SessionTransitionKind::SameMapGamestate
                        } else {
                            SessionTransitionKind::NewMap
                        },
                        old_map,
                        new_map,
                        server_id: self.server_id,
                        checksum_feed: self.game_state.checksum_feed,
                        gamestate_message_sequence: Some(message.sequence),
                        first_snapshot_message_sequence: None,
                        gamestate_to_first_snapshot_micros: None,
                        reason: None,
                    });
                    self.restart_received_at = Some(Instant::now());
                    // `CL_DownloadsComplete` sends `cp <pure-checksums>` after
                    // every no-download gamestate (`codemp/client/cl_main.cpp:
                    // 1518-1526`) before the first usercmd. `donedl` is only
                    // used after an actual filesystem download/restart
                    // (`cl_main.cpp:1473-1491`); `clientinfo` is not a wire
                    // command in this sequence.
                    if let Some(command) = (self.pure_command_builder)(&self.game_state) {
                        self.queue_reliable_command(command)?;
                    }
                    self.enter_restarted_world(message.sequence)?;
                    continue;
                }
                Err(SnapshotError::UnexpectedCommand(_)) => {
                    self.demo_recorder.observe_message(
                        message.sequence,
                        &message.payload,
                        message_kind,
                    )?;
                    continue;
                }
                Err(SnapshotError::DeltaBaseRequired { .. }) => {
                    self.demo_recorder.observe_message(
                        message.sequence,
                        &message.payload,
                        message_kind,
                    )?;
                    self.request_full_snapshot = true;
                    continue;
                }
                Err(SnapshotError::Message(MessageError::UnexpectedEnd { .. }))
                | Err(SnapshotError::AreaMaskTooLarge(_))
                | Err(SnapshotError::InvalidPlayerFieldCount(_)) => {
                    self.demo_recorder.observe_message(
                        message.sequence,
                        &message.payload,
                        message_kind,
                    )?;
                    self.request_full_snapshot = true;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            self.demo_recorder
                .observe_message(message.sequence, &message.payload, message_kind)?;
            self.request_full_snapshot = false;
            self.acknowledge_client_commands(snapshot.reliable_acknowledge);
            for command in &snapshot.server_commands {
                self.gamestate_probe
                    .command(command.sequence, &command.command);
                // Every parsed command goes into the retransmit ring, executed
                // or not (`CL_ParseCommandString`): a packet is keyed by the
                // command its acknowledgement names, not by the newest one.
                self.connection
                    .as_mut()
                    .expect("network session")
                    .record_server_command(command.sequence, &command.command);
                self.highest_server_command = self.highest_server_command.max(command.sequence);
                if command.sequence <= self.reliable_sequence {
                    continue;
                }
                self.apply_server_command(&command.command)?;
                self.reliable_sequence = command.sequence;
            }
            let previous = std::mem::replace(&mut self.latest_snapshot, snapshot);
            self.command_history.received_gamestate_snapshot();
            self.gamestate_probe.snapshot(
                Instant::now(),
                self.latest_snapshot.server_time,
                self.latest_snapshot.player.command_time(),
                self.latest_snapshot.message_sequence,
                self.server_id,
                self.reliable_sequence,
                self.highest_server_command,
            );
            if let Some(restart) = self.transitions.iter_mut().rev().find(|transition| {
                matches!(
                    transition.kind,
                    SessionTransitionKind::NewMap | SessionTransitionKind::SameMapGamestate
                ) && transition.first_snapshot_message_sequence.is_none()
            }) {
                restart.first_snapshot_message_sequence =
                    Some(self.latest_snapshot.message_sequence);
                restart.gamestate_to_first_snapshot_micros = self
                    .restart_received_at
                    .take()
                    .map(|started| started.elapsed().as_micros() as u64);
            }
            self.history.push_back(previous);
            if self.history.len() > SNAPSHOT_HISTORY {
                self.history.pop_front();
            }
            return Ok(&self.latest_snapshot);
        }
    }

    pub fn disconnect(&mut self) -> Result<(), ClientError> {
        if self.connection.is_none() {
            self.local = None;
            self.disconnected = true;
            return Ok(());
        }
        if self.disconnected {
            return Ok(());
        }
        self.demo_recorder.stop()?;
        // Bypass the pacer: the process is about to go away, and the engine
        // handles `disconnect` before the game-command flood check.
        self.queue_reliable_command(b"disconnect".to_vec())?;
        self.send_pending_reliable_commands()?;
        // The reference client transmits its final packet three times because
        // UDP has no delivery guarantee and the process is about to disappear.
        // Reuse the same reliable sequence so the server executes it once.
        for _ in 0..2 {
            self.send_pending_reliable_commands()?;
        }
        self.disconnected = true;
        Ok(())
    }

    fn apply_server_command(&mut self, command: &[u8]) -> Result<(), ClientError> {
        let arguments = tokenize_command(command);
        let Some(name) = arguments.first().map(Vec::as_slice) else {
            return Ok(());
        };
        if name == b"disconnect" {
            self.demo_recorder.stop()?;
            let reason = arguments
                .get(1..)
                .filter(|arguments| !arguments.is_empty())
                .map(|arguments| {
                    arguments
                        .iter()
                        .map(|argument| String::from_utf8_lossy(argument))
                        .collect::<Vec<_>>()
                        .join(" ")
                });
            self.transitions.push_back(SessionTransition::disconnected(
                reason,
                self.server_id,
                self.game_state.checksum_feed,
            ));
            self.disconnected = true;
            return Ok(());
        }
        if name == b"map_restart" {
            self.retired_world
                .get_or_insert_with(|| (self.game_state.clone(), self.latest_snapshot.clone()));
            self.demo_recorder.stop()?;
            let map = legacy_map_name(&self.game_state);
            self.history.clear();
            self.request_full_snapshot = true;
            self.transitions.push_back(SessionTransition {
                kind: SessionTransitionKind::MapRestart,
                old_map: map.clone(),
                new_map: map,
                server_id: self.server_id,
                checksum_feed: self.game_state.checksum_feed,
                gamestate_message_sequence: None,
                first_snapshot_message_sequence: None,
                gamestate_to_first_snapshot_micros: None,
                reason: None,
            });
            return Ok(());
        }
        if let Some(event) = chat::server_chat_event(&arguments) {
            self.events.push_back(event);
            return Ok(());
        }
        let event_kind = match name {
            b"print" => Some(ServerEventKind::Print),
            b"cp" => Some(ServerEventKind::CenterPrint),
            _ => None,
        };
        if let Some(kind) = event_kind {
            let text = server_text(arguments.get(1..).unwrap_or_default());
            self.events.push_back(ServerEvent {
                kind,
                text,
                sender: None,
            });
            return Ok(());
        }
        if self.shader_remaps.command(&arguments) {
            return Ok(());
        }
        if name == b"scores" {
            self.apply_scores(&arguments);
            return Ok(());
        }
        if name == b"tinfo" {
            self.team_info.apply_command(&arguments)?;
            return Ok(());
        }
        if name == b"nfr" {
            if let Some(update) = parse_new_force_rank(&arguments) {
                self.base_command_events
                    .push_back(BaseServerCommandEvent::ForceRank(update));
            }
            return Ok(());
        }
        if matches!(name, b"ircg" | b"rcg") {
            if let Some(restore) = parse_restore_client_ghoul(name, &arguments) {
                if let Some(body) = BodyIdentity::capture(&self.game_state, restore) {
                    self.base_command_events
                        .push_back(BaseServerCommandEvent::CopyBody(body));
                }
                self.base_command_events
                    .push_back(BaseServerCommandEvent::RestoreGhoul2(restore));
            }
            return Ok(());
        }
        if name == b"kg2" {
            for value in arguments.iter().skip(1) {
                if let Some(number) = base_server_commands::killed_entity(value) {
                    self.base_command_events
                        .push_back(BaseServerCommandEvent::KillGhoul2(number));
                }
            }
            return Ok(());
        }
        if let Some(event) = base_server_commands::siege_menu_command(name) {
            self.base_command_events.push_back(event);
            return Ok(());
        }
        if name == b"cosmetics" {
            apply_taystjk_cosmetics(
                &self.compat_profile,
                arguments.get(1).map_or(&[], Vec::as_slice),
                &mut self.cosmetic_unlocks,
            );
            return Ok(());
        }
        if let Some(index) = config_string_commands::apply(
            &arguments,
            &mut self.game_state,
            &mut self.config_string_dirty,
            &mut self.pending_big_config_string,
        )? {
            if index == SHADER_STATE_CONFIG {
                self.shader_remaps
                    .apply_config(self.game_state.config_string(index).unwrap_or_default());
            }
            if index == 1 {
                self.refresh_server_id()?;
            }
            if index == 0 {
                self.compat_profile = CompatProfile::from_game_state(&self.game_state);
            }
        } else if !matches!(name, b"cs" | b"bcs0" | b"bcs1" | b"bcs2")
            && self.unknown_command_names.len() < 64
            && name.len() <= 64
            && !self.unknown_command_names.iter().any(|known| known == name)
        {
            self.unknown_command_names.push(name.to_vec());
            self.base_command_events
                .push_back(BaseServerCommandEvent::UnknownCommand(
                    String::from_utf8_lossy(name).into_owned(),
                ));
        }
        Ok(())
    }

    fn queue_reliable_command(&mut self, command: Vec<u8>) -> Result<(), ClientError> {
        if self.pending_client_commands.len() >= 128 {
            return Err(ClientError::ReliableCommandOverflow);
        }
        self.client_reliable_sequence = self.client_reliable_sequence.wrapping_add(1);
        self.pending_client_commands
            .push_back((self.client_reliable_sequence, command));
        Ok(())
    }

    fn enter_restarted_world(&mut self, gamestate_sequence: i32) -> Result<(), ClientError> {
        let pending = self.pending_client_commands.iter().collect::<Vec<_>>();
        let pending = pending
            .iter()
            .map(|(sequence, command)| (*sequence, command.as_slice()))
            .collect::<Vec<_>>();
        // Stock stamps these 0: `CL_ClearState` zeroes `cl.serverTime` with
        // the gamestate and only `CL_FirstSnapshot` anchors it again, so the
        // packets that follow cgame initialization carry 0, not a made-up
        // time from no timeline at all.
        let command = UserCommand::default();
        // `CL_DownloadsComplete` calls CL_WritePacket three times after cgame
        // initialization (`codemp/client/cl_main.cpp:1522-1526`). Each packet
        // carries every still-unacknowledged reliable command before its
        // usercmd (`client/cl_input.cpp:1590-1595`).
        for _ in 0..3 {
            self.connection
                .as_mut()
                .expect("network session")
                .send_user_command_with_reliables(
                    self.server_id,
                    gamestate_sequence,
                    self.reliable_sequence,
                    self.game_state.checksum_feed,
                    false,
                    &pending,
                    &command,
                )?;
        }
        Ok(())
    }

    fn refresh_server_id(&mut self) -> Result<(), ClientError> {
        let bytes = self
            .game_state
            .config_string(1)
            .ok_or(ClientError::MissingSystemInfo)?;
        let text = std::str::from_utf8(bytes).map_err(|_| ClientError::NonUtf8SystemInfo)?;
        self.server_id = InfoString::parse(text)?
            .get_i32("sv_serverid")
            .ok_or(ClientError::MissingServerId)?;
        Ok(())
    }

    fn apply_scores(&mut self, arguments: &[Vec<u8>]) {
        let Some((team_scores, scores)) = parse_scores(arguments) else {
            return;
        };
        self.team_scores = team_scores;
        self.scores = scores;
    }
}

/// Parse a `scores` command: count, two team scores, then one row per client.
///
/// Stock rows have 14 fields (`CG_ParseScores`, `codemp/cgame/cg_servercmds.c`).
/// JA+ and jaPRO servers append a 15th, the deaths count, for clients that
/// identify as a client plugin (`cjp_client`): EternalJK reads 15 there
/// (`cg_servercmds.c:61-64`), and a JA+ 2.4B7 server sent
/// `scores 1 0 0 1 0 999 0 0 0 0 0 0 0 0 0 1 0 0` to such a client.
///
/// The count is the server's connected-client count, not the number of rows: the
/// game sends at most 20 rows (`MAX_CLIENT_SCORE_SEND`, `DeathmatchScoreboardMessage`
/// in `codemp/game/g_cmds.c`) and fewer when the command would pass 1023 bytes. So
/// the row width comes from the rows actually sent; when both widths divide them,
/// the one whose rows read as real players wins. Reading 15-field rows as 14 (as
/// SJK did on any server with more than 20 players) shifted every later row, and
/// the zeros it then read as client numbers repeated client 0's name down the board.
fn parse_scores(arguments: &[Vec<u8>]) -> Option<([i32; 2], Vec<ScoreEntry>)> {
    const STOCK_FIELDS: usize = 14;
    const WITH_DEATHS_FIELDS: usize = 15;
    let integer = |index: usize| {
        arguments
            .get(index)
            .and_then(|value| std::str::from_utf8(value).ok())
            .and_then(|value| value.parse::<i32>().ok())
    };
    let count = integer(1).and_then(|value| usize::try_from(value).ok())?;
    let team_scores = [integer(2).unwrap_or(0), integer(3).unwrap_or(0)];
    let sent = arguments.len().saturating_sub(4);
    // Rows as players: client numbers below 32 and unique, a ping of -1 (connecting)
    // to 999, a non-negative connected time.
    let plausible = |width: usize| {
        let mut seen = 0_u32;
        (0..sent / width).all(|row| {
            let base = 4 + row * width;
            let fields = (integer(base), integer(base + 2), integer(base + 3));
            let (Some(client), Some(ping), Some(time)) = fields else {
                return false;
            };
            let fresh = (0..32).contains(&client) && seen & (1 << client) == 0;
            seen |= 1_u32.checked_shl(client as u32).unwrap_or(0);
            fresh && (-1..=999).contains(&ping) && time >= 0
        })
    };
    let fits = |width: usize| sent.is_multiple_of(width) && sent / width <= count.max(1);
    let row_fields = match (fits(STOCK_FIELDS), fits(WITH_DEATHS_FIELDS)) {
        (true, true) if !plausible(STOCK_FIELDS) && plausible(WITH_DEATHS_FIELDS) => {
            WITH_DEATHS_FIELDS
        }
        (false, true) => WITH_DEATHS_FIELDS,
        (false, false) if plausible(WITH_DEATHS_FIELDS) && !plausible(STOCK_FIELDS) => {
            WITH_DEATHS_FIELDS
        }
        _ => STOCK_FIELDS,
    };
    let rows = (sent / row_fields).min(count).min(32);
    let mut scores = Vec::with_capacity(rows);
    for row in 0..rows {
        let base = 4 + row * row_fields;
        let Some(client_num) = integer(base).and_then(|value| u8::try_from(value).ok()) else {
            break;
        };
        if client_num >= 32 {
            continue;
        }
        scores.push(ScoreEntry {
            client_num,
            score: integer(base + 1).unwrap_or(0),
            ping: integer(base + 2).unwrap_or(-1),
            time_minutes: integer(base + 3).unwrap_or(0),
            flags: integer(base + 4).unwrap_or(0) as u32,
            // `CG_ParseScores` (`cg_servercmds.c`): powerUps, accuracy, impressive,
            // excellent, gauntlet, defend, assist, perfect, captures.
            powerups: integer(base + 5).unwrap_or(0) as u32,
            defends: integer(base + 10).unwrap_or(0),
            assists: integer(base + 11).unwrap_or(0),
            captures: integer(base + 13).unwrap_or(0),
        });
    }
    Some((team_scores, scores))
}

impl Drop for ClientSession {
    fn drop(&mut self) {
        let _ = self.disconnect();
    }
}

fn snapshot_delta_sequence(payload: &[u8], sequence: i32) -> Result<i32, ClientError> {
    let mut reader = MessageReader::new(payload);
    reader.read_i32()?;
    loop {
        match reader.read_service_command()? {
            ServiceCommand::Nop => {}
            ServiceCommand::ServerCommand => {
                reader.read_i32()?;
                reader.read_c_string(16_383)?;
            }
            ServiceCommand::Snapshot => {
                reader.read_i32()?;
                let distance = reader.read_u8()?;
                return Ok(if distance == 0 {
                    -1
                } else {
                    sequence - i32::from(distance)
                });
            }
            command => return Err(ClientError::UnexpectedCommand(command)),
        }
    }
}

fn tokenize_command(command: &[u8]) -> Vec<Vec<u8>> {
    let mut arguments = Vec::new();
    let mut position = 0;
    while position < command.len() {
        while command.get(position).is_some_and(u8::is_ascii_whitespace) {
            position += 1;
        }
        if position == command.len() {
            break;
        }
        let quoted = command[position] == b'"';
        if quoted {
            position += 1;
        }
        let start = position;
        while position < command.len()
            && if quoted {
                command[position] != b'"'
            } else {
                !command[position].is_ascii_whitespace()
            }
        {
            position += 1;
        }
        arguments.push(command[start..position].to_vec());
        if quoted && position < command.len() {
            position += 1;
        }
    }
    arguments
}

fn legacy_map_name(game_state: &GameState) -> Option<String> {
    game_state
        .config_string(0)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|text| InfoString::parse(text).ok())
        .and_then(|info| info.get("mapname").map(str::to_owned))
}

const fn is_replacement_gamestate(command: ServiceCommand) -> bool {
    // Protocol 26 names svc_gamestate byte 2 `GameState`. `SetGame` byte 8 is
    // an unrelated Raven extension; confusing the two caused the original
    // map-change soft lock.
    matches!(command, ServiceCommand::GameState)
}

#[derive(Debug)]
pub enum ClientError {
    /// A refused, unsafe, incomplete or checksum-invalid content transfer.
    Download(String),
    Network(NetworkError),
    GameState(GameStateError),
    Snapshot(SnapshotError),
    Message(MessageError),
    Info(InfoStringError),
    TeamInfo(TeamInfoError),
    DemoRecorder(DemoRecorderError),
    MissingSystemInfo,
    NonUtf8SystemInfo,
    MissingServerId,
    MalformedConfigStringCommand,
    BigConfigStringTooLarge,
    ReliableCommandOverflow,
    UnexpectedCommand(ServiceCommand),
}

impl ClientError {
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Network(NetworkError::TimedOut(_)))
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Download(reason) => write!(formatter, "content download: {reason}"),
            Self::Network(error) => error.fmt(formatter),
            Self::GameState(error) => error.fmt(formatter),
            Self::Snapshot(error) => error.fmt(formatter),
            Self::Message(error) => error.fmt(formatter),
            Self::Info(error) => error.fmt(formatter),
            Self::TeamInfo(error) => error.fmt(formatter),
            Self::DemoRecorder(error) => error.fmt(formatter),
            Self::MissingSystemInfo => formatter.write_str("gamestate has no CS_SYSTEMINFO"),
            Self::NonUtf8SystemInfo => formatter.write_str("CS_SYSTEMINFO is not UTF-8"),
            Self::MissingServerId => formatter.write_str("CS_SYSTEMINFO has no sv_serverid"),
            Self::MalformedConfigStringCommand => {
                formatter.write_str("malformed configstring server command")
            }
            Self::BigConfigStringTooLarge => {
                formatter.write_str("fragmented configstring exceeds the legacy size limit")
            }
            Self::ReliableCommandOverflow => {
                formatter.write_str("client reliable command overflow")
            }
            Self::UnexpectedCommand(command) => write!(formatter, "unexpected {command:?}"),
        }
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Network(error) => Some(error),
            Self::GameState(error) => Some(error),
            Self::Snapshot(error) => Some(error),
            Self::Message(error) => Some(error),
            Self::Info(error) => Some(error),
            Self::TeamInfo(error) => Some(error),
            Self::DemoRecorder(error) => Some(error),
            _ => None,
        }
    }
}

impl From<NetworkError> for ClientError {
    fn from(value: NetworkError) -> Self {
        Self::Network(value)
    }
}
impl From<GameStateError> for ClientError {
    fn from(value: GameStateError) -> Self {
        Self::GameState(value)
    }
}
impl From<SnapshotError> for ClientError {
    fn from(value: SnapshotError) -> Self {
        Self::Snapshot(value)
    }
}
impl From<MessageError> for ClientError {
    fn from(value: MessageError) -> Self {
        Self::Message(value)
    }
}
impl From<InfoStringError> for ClientError {
    fn from(value: InfoStringError) -> Self {
        Self::Info(value)
    }
}
impl From<TeamInfoError> for ClientError {
    fn from(value: TeamInfoError) -> Self {
        Self::TeamInfo(value)
    }
}

impl From<DemoRecorderError> for ClientError {
    fn from(value: DemoRecorderError) -> Self {
        Self::DemoRecorder(value)
    }
}

/// Snapshot-indexed primary saber presentation.
pub use thrown_sabers::{LegacyThrownSaber, LegacyThrownSabers, legacy_thrown_saber_owner};

#[cfg(test)]
mod score_tests {
    use super::{parse_scores, tokenize_command};

    fn rows(command: &str) -> Vec<(u8, i32, i32)> {
        parse_scores(&tokenize_command(command.as_bytes()))
            .unwrap()
            .1
            .iter()
            .map(|row| (row.client_num, row.score, row.ping))
            .collect()
    }

    #[test]
    fn stock_rows_have_fourteen_fields() {
        let command = "scores 2 0 0 \
            0 5 40 1 0 0 0 0 0 0 0 0 0 0 \
            1 3 60 2 0 0 0 0 0 0 0 0 0 0";
        assert_eq!(rows(command), [(0, 5, 40), (1, 3, 60)]);
    }

    #[test]
    fn plugin_rows_with_deaths_have_fifteen_fields() {
        // As a JA+ 2.4B7 server sends them to a client-plugin user.
        assert_eq!(
            rows("scores 1 0 0 1 0 999 0 0 0 0 0 0 0 0 0 1 0 0"),
            [(1, 0, 999)]
        );
        let command = "scores 2 0 0 \
            0 5 40 1 0 0 0 0 0 0 0 0 0 0 7 \
            1 3 60 2 0 0 0 0 0 0 0 0 0 0 9";
        assert_eq!(rows(command), [(0, 5, 40), (1, 3, 60)]);
    }

    /// A full server: the count is every connected client, but only 20 rows follow.
    fn full_server(fields: usize) -> String {
        let mut command = String::from("scores 28 0 0");
        for row in 0..20 {
            let client = 27 - row;
            command.push_str(&format!(
                " {client} {} {} 3 0 0 0 0 0 0 0 0 0 0",
                40 - row,
                50 + row
            ));
            if fields == 15 {
                command.push_str(" 2");
            }
        }
        command
    }

    #[test]
    fn full_japro_server_rows_are_read_whole() {
        let read = rows(&full_server(15));
        assert_eq!(read.len(), 20);
        assert_eq!(read[0], (27, 40, 50));
        assert_eq!(read[19], (8, 21, 69));
        let mut clients = read.iter().map(|row| row.0).collect::<Vec<_>>();
        clients.dedup();
        assert_eq!(clients.len(), 20, "every row is a different player");
    }

    #[test]
    fn full_stock_server_rows_are_read_whole() {
        let read = rows(&full_server(14));
        assert_eq!(read.len(), 20);
        assert_eq!(read[0], (27, 40, 50));
        assert_eq!(read[19], (8, 21, 69));
    }

    /// 210 values are 15 stock rows or 14 rows with deaths; only one reads as players.
    #[test]
    fn ambiguous_length_picks_the_width_that_reads_as_players() {
        let mut command = String::from("scores 20 0 0");
        for client in 0..14 {
            command.push_str(&format!(" {client} 9 30 4 0 0 0 0 0 0 0 0 0 0 5"));
        }
        let read = rows(&command);
        assert_eq!(read.len(), 14);
        assert!(read.iter().all(|row| row.1 == 9 && row.2 == 30));
    }
}

impl ClientSession {
    /// Persistent shader aliases supplied by this server and gamestate.
    pub fn shader_remaps(&self) -> &ShaderRemaps {
        &self.shader_remaps
    }
    /// Local `clearRemaps`; nothing is sent to the server.
    pub fn clear_shader_remaps(&mut self) {
        self.shader_remaps.clear();
    }
}

/// Join the arguments of a `print` or `cp` command as display text.
///
/// Servers embed player names in these as legacy bytes, so a name with `×` or `é` is
/// decoded as [`decode_legacy`] decodes it in chat and the scoreboard; reading it as
/// UTF-8 replaced each such byte with U+FFFD, drawn as `?`.
fn server_text(arguments: &[Vec<u8>]) -> String {
    arguments
        .iter()
        .map(|argument| decode_legacy(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod server_text_tests {
    use super::server_text;

    #[test]
    fn server_text_reads_legacy_name_bytes_as_latin_1() {
        let arguments = [b"You have challenged \xd7jof.jk.belyash\xd7\n".to_vec()];
        assert_eq!(
            server_text(&arguments),
            "You have challenged ×jof.jk.belyash×\n"
        );
    }

    #[test]
    fn server_text_keeps_utf8_and_joins_arguments() {
        let arguments = [b"gg".to_vec(), "ø".as_bytes().to_vec()];
        assert_eq!(server_text(&arguments), "gg ø");
    }
}
