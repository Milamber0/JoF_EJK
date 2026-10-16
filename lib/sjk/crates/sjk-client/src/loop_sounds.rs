//! Codemp looping-sound compatibility adapter.
//!
//! Plain `entityState.loopSound` follows `CG_EntityEffects`
//! (`codemp/cgame/cg_ents.c:295-340`). Per-entity de-duplication and the
//! eight-loop ceiling mirror `CG_S_AddLoopingSound` (`cg_ents.c:136-180`,
//! `cg_local.h:319`); the final 32-loop frame ceiling mirrors
//! `S_AddLoopingSound` (`codemp/client/snd_dma.cpp:1924-1974`,
//! `snd_dma.cpp:213`). Weapon held loops follow `CG_AddPlayerWeapon`
//! (`cg_weapons.c:470-480`). Saber hum selection follows the active-blade,
//! death, holster, throw and dual-saber branches in `cg_players.c:9948-10023`.
//! Ambient-set loops (`S_AddLocalSet` per entity, `S_UpdateAmbientSet` after
//! the packet entities) come from [`crate::ambient_world`] and join the same
//! 32-entry backend frame list without cgame's per-entity ceiling. A burning
//! jetpack's hover and fire loops follow `CG_Player` (`cg_players.c:11843-11935`,
//! `cg_jetpackHoverSound 1`).

use crate::ambient_world::{CS_GLOBAL_AMBIENT_SET, LegacyAmbientShot, LegacyAmbientWorld};
use crate::mind_trick::{legacy_entity_trick_targets, legacy_mind_tricked};
use crate::player_identity::legacy_client_saber_names;
use crate::player_spheres::LegacyJetpack;
use crate::presentation::legacy_evaluate_trajectory_delta;
use crate::saber_definitions::legacy_saber_definitions;
use crate::sound_events::{RegisteredLegacySound, intern_sound, normal_attenuation};
use crate::{AmbientSetKind, AmbientSets};
use sjk_audio::{ChannelId, DopplerConfig, PlayRequest, SoundHandle, SourceId};
use sjk_protocol::{EntityState, GameState, Snapshot};
use sjk_vfs::VirtualFileSystem;

const CS_SOUNDS: usize = 811; // codemp/game/bg_public.h:135 after expanded limits
const CS_AMBIENT_SET: usize = 37; // codemp/game/bg_public.h:125
const MAX_SOUNDS: usize = 256;
const MAX_CLIENTS: usize = 32; // codemp/qcommon/q_shared.h:890
const MAX_ENTITIES: usize = 1_024;
const MAX_CG_LOOPSOUNDS: u8 = 8; // codemp/cgame/cg_local.h:319
const MAX_LOOP_SOUNDS: usize = 32; // codemp/client/snd_dma.cpp:213
const ET_PLAYER: u8 = 1;
const ET_MISSILE: u8 = 3;
const ET_MOVER: u8 = 6;
const ET_NPC: u8 = 13;
const WP_SABER: u8 = 3;
const EF_DEAD: u32 = 1 << 1;
const EF_FIRING: u32 = 1 << 9;

// `CG_RegisterWeapon`, codemp/cgame/cg_weaponinit.c:136-607. Only WP_SABER
// registers a firing loop in BaseJKA MP and no weapon registers readySound.
const WEAPON_FIRING_LOOPS: [Option<&str>; 19] = [
    None,
    None,
    None,
    Some("sound/weapons/saber/saberhum1.wav"),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];
const WEAPON_READY_LOOPS: [Option<&str>; 19] = [None; 19];
/// `cgs.media.jetpackHoverSound` and the jet fire loop (`cg_main.c`, `cg_players.c`).
const JETPACK_LOOPS: [&str; 2] = ["sound/chars/boba/jethover.wav", "sound/effects/fire_lp.wav"];

/// Why a loop was selected by the legacy adapter.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LegacyLoopKind {
    Entity,
    SoundSet,
    Missile,
    WeaponFiring,
    WeaponReady,
    SaberHumPrimary,
    SaberHumSecondary,
    /// A blade skin's hum, over the saber's own.
    SaberHumSkin,
    AmbientGlobal,
    AmbientLocal,
    /// A burning jetpack's hover.
    JetpackHover,
    /// A flaming jetpack's fire.
    JetpackFire,
}

impl LegacyLoopKind {
    /// Stable parity-report label.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Entity => "entity_loopSound",
            Self::SoundSet => "ambient_bmodel_soundset",
            Self::Missile => "missile_loopSound",
            Self::WeaponFiring => "weapon_firingSound",
            Self::WeaponReady => "weapon_readySound",
            Self::SaberHumPrimary => "saber_primary_hum",
            Self::SaberHumSecondary => "saber_secondary_hum",
            Self::SaberHumSkin => "saber_skin_hum",
            Self::AmbientGlobal => "ambient_global_set",
            Self::AmbientLocal => "ambient_local_set",
            Self::JetpackHover => "jetpack_hover",
            Self::JetpackFire => "jetpack_fire",
        }
    }
}

/// One allocation-free loop request for the generic mixer.
#[derive(Clone, Copy, Debug)]
pub struct LegacyLoopDecision {
    /// Compatibility rule that selected the loop.
    pub kind: LegacyLoopKind,
    /// Adapter-local registered-sound index, even when decoding failed.
    pub sound: Option<u16>,
    /// Engine sound-bank handle when the asset decoded successfully.
    pub handle: Option<SoundHandle>,
    /// Original `CS_SOUNDS` index for generic entity loops.
    pub configured_index: Option<u8>,
    /// `CS_AMBIENT_SET` index for a BMODEL stage, when applicable.
    pub soundset_index: Option<u8>,
    /// Engine-generic spatial playback parameters.
    pub request: PlayRequest,
    /// Snapshot trajectory velocity used by the generic mixer's Doppler path.
    pub velocity: [f32; 3],
    /// Codemp's gated/clamped per-frame loop pitch ratio.
    pub doppler_scale: f32,
}

#[derive(Clone, Copy, Default)]
struct SaberLoops {
    primary: Option<u16>,
    secondary: Option<u16>,
}

#[derive(Clone, Copy, Default)]
struct SoundsetStages {
    stages: [Option<u16>; 3],
    present: bool,
}

/// Fixed-capacity state that compiles snapshot loop fields into mixer work.
pub(crate) struct LegacyLoopAdapter {
    sounds: Vec<RegisteredLegacySound>,
    cs_sounds: [Option<u16>; MAX_SOUNDS],
    soundsets: [SoundsetStages; MAX_SOUNDS],
    weapon_firing: [Option<u16>; 19],
    weapon_ready: [Option<u16>; 19],
    jetpack: [u16; 2],
    sabers: [SaberLoops; MAX_CLIENTS],
    /// Registered hums of the replacement saber sound sets, by set.
    saber_hums: Vec<u16>,
    /// The hum each client's blade skin plays over its sabers' own.
    hum_overrides: [Option<u16>; MAX_CLIENTS],
    client_config_hash: [u64; MAX_CLIENTS],
    decisions: Vec<LegacyLoopDecision>,
    loops_per_entity: Box<[u8]>,
    unresolved_soundsets: usize,
    referenced_soundsets: usize,
    parsed_bmodel_sets: usize,
    dropped_capacity: usize,
    listener_origin: [f32; 3],
    listener_velocity: [f32; 3],
    ambient: LegacyAmbientWorld,
    ambient_catalog: AmbientSets,
}

impl LegacyLoopAdapter {
    pub(crate) fn new(
        game_state: &GameState,
        vfs: &VirtualFileSystem,
        register: &mut impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) -> Self {
        let mut sounds = Vec::with_capacity(384);
        let mut cs_sounds = [None; MAX_SOUNDS];
        for (index, slot) in cs_sounds.iter_mut().enumerate() {
            let Some(bytes) = game_state.config_string(CS_SOUNDS + index) else {
                continue;
            };
            let Ok(path) = std::str::from_utf8(bytes) else {
                continue;
            };
            if !path.is_empty() && !path.starts_with('*') {
                *slot = Some(intern_sound(&mut sounds, vfs, path, register));
            }
        }
        let mut intern = |path: &str| intern_sound(&mut sounds, vfs, path, register);
        let weapon_firing = WEAPON_FIRING_LOOPS.map(|path| path.map(&mut intern));
        let weapon_ready = WEAPON_READY_LOOPS.map(|path| path.map(&mut intern));
        let jetpack = JETPACK_LOOPS.map(&mut intern);
        let definitions = legacy_saber_definitions(vfs).unwrap_or_default();
        let sabers = std::array::from_fn(|client| {
            saber_loops_for_client(game_state, client as u16, &definitions, &mut intern)
        });
        let ambient = vfs
            .read("sound/sound.txt")
            .ok()
            .flatten()
            .map(|asset| AmbientSets::parse(&String::from_utf8_lossy(&asset.bytes)))
            .unwrap_or_default();
        let parsed_bmodel_sets = ambient.declaration_count(AmbientSetKind::Bmodel);
        let mut soundsets = [SoundsetStages::default(); MAX_SOUNDS];
        for (index, stages) in soundsets.iter_mut().enumerate() {
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
            stages.present = true;
            for (stage, path) in set.sub_waves.iter().take(3).enumerate() {
                stages.stages[stage] = Some(intern(path));
            }
        }
        let ambient_catalog = ambient;
        let ambient = LegacyAmbientWorld::new(game_state, &ambient_catalog, &mut intern);
        drop(intern);
        Self {
            sounds,
            cs_sounds,
            soundsets,
            weapon_firing,
            weapon_ready,
            jetpack,
            sabers,
            saber_hums: Vec::new(),
            hum_overrides: [None; MAX_CLIENTS],
            client_config_hash: std::array::from_fn(|client| {
                player_config_hash(game_state, client)
            }),
            decisions: Vec::with_capacity(MAX_LOOP_SOUNDS),
            loops_per_entity: vec![0; MAX_ENTITIES].into_boxed_slice(),
            unresolved_soundsets: 0,
            referenced_soundsets: 0,
            parsed_bmodel_sets,
            dropped_capacity: 0,
            listener_origin: [0.0; 3],
            listener_velocity: [0.0; 3],
            ambient,
            ambient_catalog,
        }
    }

    pub(crate) fn refresh_clients(
        &mut self,
        game_state: &GameState,
        vfs: &VirtualFileSystem,
        register: &mut impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) {
        self.ambient
            .select_global(game_state.config_string(CS_GLOBAL_AMBIENT_SET));
        let hashes = std::array::from_fn(|client| player_config_hash(game_state, client));
        if hashes == self.client_config_hash {
            return;
        }
        let definitions = crate::shared_saber_definitions(vfs).unwrap_or_default();
        for client in 0..MAX_CLIENTS {
            let hash = hashes[client];
            if hash == self.client_config_hash[client] {
                continue;
            }
            self.client_config_hash[client] = hash;
            let sounds = &mut self.sounds;
            let mut intern = |path: &str| intern_sound(sounds, vfs, path, register);
            self.sabers[client] =
                saber_loops_for_client(game_state, client as u16, &definitions, &mut intern);
        }
    }

    pub(crate) fn observe(
        &mut self,
        snapshot: &Snapshot,
        presented_time: i32,
        listener_origin: [f32; 3],
        mut presented_origin: impl FnMut(&EntityState) -> [f32; 3],
    ) -> usize {
        self.decisions.clear();
        self.loops_per_entity.fill(0);
        self.unresolved_soundsets = 0;
        self.referenced_soundsets = 0;
        self.dropped_capacity = 0;
        self.listener_origin = listener_origin;
        self.listener_velocity = snapshot.player.velocity();
        self.ambient.begin_frame();

        for state in &snapshot.entities {
            let local_set = state.sound_set_index() != 0 && state.entity_type() != ET_MOVER;
            let configured_loop = state.loop_sound() != 0
                || (state.loop_is_soundset()
                    && usize::from(state.number()) >= MAX_CLIENTS
                    && usize::from(state.loop_sound()) < MAX_SOUNDS);
            let weapon = usize::from(state.weapon());
            let weapon_loop = matches!(state.entity_type(), ET_PLAYER | ET_NPC)
                && state.weapon() != WP_SABER
                && if state.e_flags() & EF_FIRING != 0 {
                    self.weapon_firing.get(weapon).copied().flatten().is_some()
                } else {
                    self.weapon_ready.get(weapon).copied().flatten().is_some()
                };
            let saber_loop = state.number() != snapshot.player.client_num()
                && matches!(state.entity_type(), ET_PLAYER | ET_NPC)
                && state.weapon() == WP_SABER
                && state.saber_holstered() < 2
                && state.e_flags() & EF_DEAD == 0;
            let jetpack = (state.number() != snapshot.player.client_num()
                && matches!(state.entity_type(), ET_PLAYER | ET_NPC))
            .then(|| LegacyJetpack::from_flags(state.e_flags()))
            .filter(|jetpack| {
                // `jetpackVisible`: no jet sounds from a player who tricks the viewer.
                jetpack.active
                    && !legacy_mind_tricked(
                        legacy_entity_trick_targets(state),
                        snapshot.player.client_num(),
                        snapshot.player.force_powers_active(),
                    )
            });
            if !configured_loop && !weapon_loop && !saber_loop && !local_set && jetpack.is_none() {
                continue;
            }
            let origin = presented_origin(state);
            self.add_entity_loop(state, presented_time, origin);
            if let Some(jetpack) = jetpack {
                self.add_jetpack_loops(jetpack, state.number(), origin);
            }
            if local_set {
                self.add_local_set(state, snapshot.server_time, presented_time, origin);
            }
            if state.number() != snapshot.player.client_num() {
                self.add_weapon_loop(state, origin);
                self.add_entity_saber_loops(state, origin);
            }
        }
        let own = LegacyJetpack::from_flags(snapshot.player.entity_flags());
        if own.active && !snapshot.player.is_spectator() {
            self.add_jetpack_loops(own, snapshot.player.client_num(), listener_origin);
        }
        self.add_local_saber_loops(snapshot, listener_origin);
        self.add_global_set(listener_origin, presented_time);
        self.decisions.len()
    }

    /// Ambient one-shots started while building the current frame.
    pub(crate) fn ambient_shots(&self) -> &[LegacyAmbientShot] {
        self.ambient.shots()
    }

    /// Whether a global set resolved this frame (0 or 1).
    pub(crate) const fn ambient_global_frames(&self) -> usize {
        self.ambient.global_frames()
    }

    /// Entities whose local set resolved this frame.
    pub(crate) const fn ambient_local_frames(&self) -> usize {
        self.ambient.local_frames()
    }

    /// `cg_ents.c:3326-3334`: local sets bypass `CG_S_AddLoopingSound`'s
    /// per-entity table and go straight to the backend frame list.
    fn add_local_set(
        &mut self,
        state: &EntityState,
        server_time: i32,
        presented_time: i32,
        origin: [f32; 3],
    ) {
        let decision = self.ambient.local_set(
            state.sound_set_index(),
            state.number(),
            origin,
            self.listener_origin,
            server_time,
            presented_time,
            &self.sounds,
        );
        if let Some(decision) = decision {
            self.push_backend_loop(decision);
        }
    }

    /// `cg_view.c:2694-2698`: the global set is added after every entity.
    fn add_global_set(&mut self, listener_origin: [f32; 3], presented_time: i32) {
        let Self {
            ambient,
            sounds,
            decisions,
            dropped_capacity,
            ..
        } = self;
        ambient.update_global(listener_origin, presented_time, sounds, |decision| {
            if decisions.len() >= MAX_LOOP_SOUNDS {
                *dropped_capacity += 1;
            } else {
                decisions.push(decision);
            }
        });
    }

    /// `S_AddAmbientLoopingSound` (`snd_dma.cpp:1982-2023`): only the backend
    /// frame ceiling applies.
    fn push_backend_loop(&mut self, decision: LegacyLoopDecision) {
        if self.decisions.len() >= MAX_LOOP_SOUNDS {
            self.dropped_capacity += 1;
            return;
        }
        self.decisions.push(decision);
    }

    pub(crate) fn decisions(&self) -> &[LegacyLoopDecision] {
        &self.decisions
    }

    pub(crate) fn sound(&self, index: u16) -> Option<&RegisteredLegacySound> {
        self.sounds.get(usize::from(index))
    }

    pub(crate) const fn unresolved_soundsets(&self) -> usize {
        self.unresolved_soundsets
    }

    pub(crate) const fn referenced_soundsets(&self) -> usize {
        self.referenced_soundsets
    }

    pub(crate) const fn parsed_bmodel_sets(&self) -> usize {
        self.parsed_bmodel_sets
    }

    pub(crate) const fn dropped_capacity(&self) -> usize {
        self.dropped_capacity
    }

    fn add_entity_loop(&mut self, state: &EntityState, at_time: i32, origin: [f32; 3]) {
        let configured = state.loop_sound();
        let deferred_soundset = state.loop_is_soundset()
            && usize::from(state.number()) >= MAX_CLIENTS
            && usize::from(configured) < MAX_SOUNDS;
        if configured == 0 && !deferred_soundset {
            return;
        }
        if deferred_soundset {
            self.referenced_soundsets += 1;
            let set_index = state.sound_set_index();
            let stage = usize::from(configured);
            let sound = self.soundsets[usize::from(set_index)]
                .stages
                .get(stage)
                .copied()
                .flatten();
            if !self.soundsets[usize::from(set_index)].present || sound.is_none() {
                self.unresolved_soundsets += 1;
            }
            self.push_soundset(sound, set_index, state.number(), origin);
            return;
        }
        let sound = self.cs_sounds[usize::from(configured)];
        let velocity = if state.entity_type() == ET_MISSILE {
            legacy_evaluate_trajectory_delta(
                state.trajectory_delta(),
                state.trajectory_type(),
                state.trajectory_time(),
                state.trajectory_duration(),
                at_time,
            )
        } else {
            [0.0; 3]
        };
        self.push(
            if state.entity_type() == ET_MISSILE {
                LegacyLoopKind::Missile
            } else {
                LegacyLoopKind::Entity
            },
            sound,
            Some(configured),
            state.number(),
            origin,
            velocity,
        );
        // A thrown saber of a blade skin's wearer hums with the skin as well.
        if let Some(hum) = self.thrown_saber_hum(state) {
            self.push(
                LegacyLoopKind::SaberHumSkin,
                Some(hum),
                None,
                state.number(),
                origin,
                velocity,
            );
        }
    }

    /// `CG_Player`: the fire loop of a flaming jetpack (`trap->S_RegisterSound(
    /// "sound/effects/fire_lp")` inside the jet loop), then the hover.
    fn add_jetpack_loops(&mut self, jetpack: LegacyJetpack, source: u16, origin: [f32; 3]) {
        let [hover, fire] = self.jetpack;
        if jetpack.flaming {
            self.push(
                LegacyLoopKind::JetpackFire,
                Some(fire),
                None,
                source,
                origin,
                [0.0; 3],
            );
        }
        self.push(
            LegacyLoopKind::JetpackHover,
            Some(hover),
            None,
            source,
            origin,
            [0.0; 3],
        );
    }

    fn add_weapon_loop(&mut self, state: &EntityState, origin: [f32; 3]) {
        if !matches!(state.entity_type(), ET_PLAYER | ET_NPC) || state.weapon() == WP_SABER {
            return;
        }
        let weapon = usize::from(state.weapon());
        let (kind, sound) = if state.e_flags() & EF_FIRING != 0 {
            (
                LegacyLoopKind::WeaponFiring,
                self.weapon_firing.get(weapon).copied().flatten(),
            )
        } else {
            (
                LegacyLoopKind::WeaponReady,
                self.weapon_ready.get(weapon).copied().flatten(),
            )
        };
        if sound.is_some() {
            self.push(kind, sound, None, state.number(), origin, [0.0; 3]);
        }
    }

    fn add_entity_saber_loops(&mut self, state: &EntityState, origin: [f32; 3]) {
        if !matches!(state.entity_type(), ET_PLAYER | ET_NPC)
            || state.weapon() != WP_SABER
            || state.saber_holstered() >= 2
            || state.e_flags() & EF_DEAD != 0
        {
            return;
        }
        let client = usize::from(state.client_num());
        if client >= MAX_CLIENTS {
            return;
        }
        let loops = self.saber_loops(client);
        let primary_active = !state.saber_in_flight();
        let secondary_active = state.saber_holstered() == 0 && loops.secondary.is_some();
        if primary_active {
            self.push(
                LegacyLoopKind::SaberHumPrimary,
                loops.primary,
                None,
                state.number(),
                origin,
                [0.0; 3],
            );
        }
        if secondary_active {
            self.push(
                LegacyLoopKind::SaberHumSecondary,
                loops.secondary,
                None,
                state.number(),
                origin,
                [0.0; 3],
            );
        }
        if primary_active || secondary_active {
            self.push_skin_hum(client, state.number(), origin);
        }
    }

    fn add_local_saber_loops(&mut self, snapshot: &Snapshot, listener_origin: [f32; 3]) {
        let player = &snapshot.player;
        if player.is_spectator()
            || player.health() <= 0
            || player.weapon() != WP_SABER
            || player.saber_holstered() >= 2
        {
            return;
        }
        let client = usize::from(player.client_num());
        if client >= MAX_CLIENTS {
            return;
        }
        let loops = self.saber_loops(client);
        let primary_active = !player.saber_in_flight();
        let secondary_active = player.saber_holstered() == 0 && loops.secondary.is_some();
        if primary_active {
            self.push(
                LegacyLoopKind::SaberHumPrimary,
                loops.primary,
                None,
                player.client_num(),
                listener_origin,
                [0.0; 3], // CG_Player passes vec3_origin for saber hum.
            );
        }
        if secondary_active {
            self.push(
                LegacyLoopKind::SaberHumSecondary,
                loops.secondary,
                None,
                player.client_num(),
                listener_origin,
                [0.0; 3],
            );
        }
        if primary_active || secondary_active {
            self.push_skin_hum(client, player.client_num(), listener_origin);
        }
    }

    /// Register the replacement sets' hums, in set order
    /// (`LegacySoundAdapter::register_saber_sound_sets`).
    pub(crate) fn register_saber_hums(
        &mut self,
        hums: &[&str],
        vfs: &VirtualFileSystem,
        register: &mut impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) {
        self.saber_hums = hums
            .iter()
            .map(|path| intern_sound(&mut self.sounds, vfs, path, register))
            .collect();
    }

    /// Which set's hum each client plays (`None`: its sabers' own).
    pub(crate) fn set_saber_hum_overrides(&mut self, sets: &[Option<u8>; MAX_CLIENTS]) {
        self.hum_overrides =
            sets.map(|set| set.and_then(|set| self.saber_hums.get(usize::from(set)).copied()));
    }

    /// A thrown saber's hum when its owner wears a blade skin: the skin's, to play over
    /// the `loopSound` the game gives the saber entity (`w_saber.c`'s `saberHumSound`
    /// or the hilt's `soundLoop`). `None` for any other entity or a stock owner.
    fn thrown_saber_hum(&self, state: &EntityState) -> Option<u16> {
        let owner = crate::thrown_sabers::legacy_thrown_saber_owner(state)?;
        self.hum_overrides
            .get(usize::from(owner))
            .copied()
            .flatten()
    }

    /// A client's sabers' own hums, which a blade skin's hum plays over
    /// ([`Self::push_skin_hum`]).
    fn saber_loops(&self, client: usize) -> SaberLoops {
        self.sabers[client]
    }

    /// The blade skin's hum of a client wearing one, once for both sabers, over theirs.
    fn push_skin_hum(&mut self, client: usize, source: u16, origin: [f32; 3]) {
        if let Some(hum) = self.hum_overrides[client] {
            self.push(
                LegacyLoopKind::SaberHumSkin,
                Some(hum),
                None,
                source,
                origin,
                [0.0; 3],
            );
        }
    }

    fn push(
        &mut self,
        kind: LegacyLoopKind,
        sound: Option<u16>,
        configured_index: Option<u8>,
        source: u16,
        origin: [f32; 3],
        velocity: [f32; 3],
    ) {
        let entity = usize::from(source);
        if entity >= self.loops_per_entity.len()
            || self.loops_per_entity[entity] >= MAX_CG_LOOPSOUNDS
            || self.decisions.len() >= MAX_LOOP_SOUNDS
        {
            self.dropped_capacity += 1;
            return;
        }
        let handle = sound
            .and_then(|index| self.sounds.get(usize::from(index)))
            .and_then(|registered| registered.handle);
        if self.decisions.iter().any(|decision| {
            decision.request.source == SourceId(u32::from(source)) && decision.sound == sound
        }) {
            return;
        }
        self.loops_per_entity[entity] += 1;
        self.decisions.push(LegacyLoopDecision {
            kind,
            sound,
            handle,
            configured_index,
            soundset_index: None,
            request: PlayRequest {
                origin: Some(origin),
                source: SourceId(u32::from(source)),
                channel: ChannelId(0),
                volume: 1.0,
                attenuation: normal_attenuation(0),
            },
            velocity,
            doppler_scale: legacy_doppler_scale(
                self.listener_origin,
                self.listener_velocity,
                origin,
                velocity,
                true,
            ),
        });
    }

    fn push_soundset(
        &mut self,
        sound: Option<u16>,
        soundset_index: u8,
        source: u16,
        origin: [f32; 3],
    ) {
        let before = self.decisions.len();
        self.push(
            LegacyLoopKind::SoundSet,
            sound,
            None,
            source,
            origin,
            [0.0; 3],
        );
        if self.decisions.len() > before {
            self.decisions.last_mut().unwrap().soundset_index = Some(soundset_index);
        }
    }
}

/// Exact `S_AddLoopingSound` Doppler scale from
/// `codemp/client/snd_dma.cpp:1956-1970`. Listener velocity is deliberately
/// ignored: codemp gates and computes this effect using source velocity only.
pub fn legacy_doppler_scale(
    listener_origin: [f32; 3],
    _listener_velocity: [f32; 3],
    source_origin: [f32; 3],
    source_velocity: [f32; 3],
    enabled: bool,
) -> f32 {
    if !enabled {
        return 1.0;
    }
    let velocity = source_velocity;
    let speed_squared = velocity.iter().map(|value| value * value).sum::<f32>();
    if speed_squared <= 0.0 {
        return 1.0;
    }
    let before: [f32; 3] = std::array::from_fn(|axis| source_origin[axis] - listener_origin[axis]);
    let after: [f32; 3] =
        std::array::from_fn(|axis| source_origin[axis] + velocity[axis] - listener_origin[axis]);
    let lena = before.iter().map(|value| value * value).sum::<f32>();
    if lena <= f32::EPSILON {
        return 50.0;
    }
    let lenb = after.iter().map(|value| value * value).sum::<f32>();
    let scale = lenb / (lena * 100.0);
    if scale <= 1.0 { 1.0 } else { scale.min(50.0) }
}

/// Codemp's archived `s_doppler` policy (`snd_dma.cpp:452,1914,1956-1970`)
/// expressed as generic mixer parameters.
pub const fn legacy_doppler_config(enabled: bool) -> DopplerConfig {
    DopplerConfig {
        enabled,
        distance_ratio_divisor: 100.0,
        maximum_scale: 50.0,
    }
}

fn saber_loops_for_client(
    game_state: &GameState,
    client: u16,
    definitions: &std::collections::BTreeMap<String, crate::LegacySaberDefinition>,
    intern: &mut impl FnMut(&str) -> u16,
) -> SaberLoops {
    let names = legacy_client_saber_names(game_state, client);
    let resolve = |name: Option<&str>, fallback: Option<&str>| {
        let name = name.or(fallback)?;
        if name.eq_ignore_ascii_case("none") {
            return None;
        }
        definitions
            .get(&name.to_ascii_lowercase())
            .map(|definition| definition.sound_loop.as_str())
            .or(Some("sound/weapons/saber/saberhum1.wav"))
    };
    SaberLoops {
        primary: resolve(names[0].as_deref(), Some("Kyle")).map(&mut *intern),
        secondary: resolve(names[1].as_deref(), None).map(&mut *intern),
    }
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

#[path = "loop_config_strings.rs"]
mod config_strings;
