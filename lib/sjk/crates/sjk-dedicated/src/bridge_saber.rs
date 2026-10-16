//! The saber as the server holds it: `WP_SaberPositionUpdate`'s skeleton half on this
//! server (`sjk_game_jka::server_skeleton`, `sjk_game_jka::g2_player_angles`).
//!
//! Every playing client's skeleton is posed once a frame, as `G_RunFrame` does before the
//! client's own run: the frame's spine angles (reading the `Motion` bolt first when the
//! spine corrects for it), the blade from the hilt at the Ghoul2 clock — the previous
//! frame's time, as the engine sets it after each game frame — and then the frame's
//! animations, which the next frame's pose will show.

use super::NativeGame;
use sjk_game_jka::g2_player_angles::{
    AngleInputs, AngleMemory, corrects_for_motion, player_angles,
};
use sjk_game_jka::server_skeleton::{
    AnimationInputs, HiltSpec, MAX_BLADES, ServerSkeleton, SkeletonModels, proper_origin,
};
use std::collections::HashMap;
use std::sync::Arc;

/// `DEFAULT_MODEL` (`bg_public.h`): the model a player whose own is missing gets.
const DEFAULT_MODEL: &str = "kyle";
/// `DEFAULT_SABER_MODEL`: the hilt of a saber whose own cannot be read.
const DEFAULT_HILT: &str = "models/weapons2/saber/saber_w.glm";
/// `sv_fps`: this server's frame rate, which the blade's lead is measured by.
const SERVER_FPS: f32 = 20.0;
/// `WP_SABER`; `WEAPON_RAISING`, `WEAPON_DROPPING`.
const WP_SABER: u8 = 3;
const WEAPON_RAISING: u8 = 1;
const WEAPON_DROPPING: u8 = 2;
/// Player-state fields read here without an accessor: `fd.saberAnimLevel`,
/// `saberLockFrame`, `hasLookTarget`, `lookTarget`.
pub(super) const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_LOCK_FRAME: usize = 108;
const PS_HAS_LOOK_TARGET: usize = 76;
const PS_LOOK_TARGET: usize = 66;
/// A `Motion` bolt with no offset, for the one frame a new skeleton has none.
const LEVEL_MOTION: [[f32; 4]; 3] = [
    [0.0, -1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];
/// `FP_RAGE` in `fd.forcePowersActive`: animations play at twice the speed.
const FP_RAGE: u32 = 8;

/// A player's Ghoul2 instance on this server: its models, its pose, and what
/// `G_G2PlayerAngles` remembers between frames.
pub(crate) struct PlayerSkeleton {
    pub(super) models: Arc<SkeletonModels>,
    pub(super) skeleton: ServerSkeleton,
    memory: AngleMemory,
    /// The player model and hilts it was built for, so a change of any builds a new one.
    model: Vec<u8>,
    hilts: [HiltKey; 2],
}

/// A hand's hilt as the skeleton was built with it: the model (empty for none), its
/// blades, whether it is bolted to the wrist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HiltKey {
    model: Vec<u8>,
    blades: usize,
    wrist: bool,
}

/// `SFL_BOLT_TO_WRIST`.
const SFL_BOLT_TO_WRIST: u32 = 1 << 9;

impl HiltKey {
    /// The key of `saber`'s hilt: its model, its blades, whether it is bolted to the wrist.
    pub(super) fn of(saber: &sjk_game_jka::saber_definition::SaberDefinition) -> Self {
        Self {
            model: saber.model.clone(),
            blades: saber.num_blades.max(0) as usize,
            wrist: saber.flags & SFL_BOLT_TO_WRIST != 0,
        }
    }

    fn matches(&self, saber: &sjk_game_jka::saber_definition::SaberDefinition) -> bool {
        self.model == saber.model
            && self.blades == saber.num_blades.max(0) as usize
            && self.wrist == (saber.flags & SFL_BOLT_TO_WRIST != 0)
    }
}

/// `G_UpdateClientAnims`' inputs for a player: its halves' animations and flips, weapon,
/// style, broken limbs, lock frame, rage's double speed and its hilts' speed scales.
pub(super) fn animation_inputs(peer: &crate::peer::Peer) -> AnimationInputs {
    let state = &peer.state;
    AnimationInputs {
        legs: state.leg_animation(),
        torso: state.torso_animation(),
        legs_flip: state.leg_flip(),
        torso_flip: state.torso_flip(),
        weapon: state.weapon(),
        saber_style: state.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) as i32,
        broken_limbs: i32::from(state.broken_limbs()),
        saber_lock_frame: state.raw_field(PS_SABER_LOCK_FRAME).unwrap_or(0) as i32,
        speed_scale: if state.force_powers_active() & (1 << FP_RAGE) != 0 {
            2.0
        } else {
            1.0
        },
        hilt_speed_scales: peer.sabers.speed_scales().0,
    }
}

impl PlayerSkeleton {
    /// `PM_FootSlopeTrace`'s feet on the player's model at `origin` facing `yaw`, at the
    /// Ghoul2 clock ([`ServerSkeleton::foot_points`]); `None` without a humanoid model.
    pub(crate) fn foot_points(
        &mut self,
        yaw: f32,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Option<([f32; 3], [f32; 3])> {
        self.skeleton
            .foot_points(&self.models, yaw, origin, ghoul2_time)
            .ok()
            .flatten()
    }

    /// See [`ServerSkeleton::forget_animations`].
    pub(crate) fn forget_animations(&mut self) {
        self.skeleton.forget_animations();
    }
}

/// The skeleton models for a model (`"kyle/default"`: the model, its skin) holding
/// `hilts`, loaded once into `cache`. A model that is not there is `kyle`'s, as
/// `SetupGameGhoul2Model` falls back to its precached Kyle; for a player (`hilts` given)
/// the first hand always holds a hilt, the default one if its own cannot be read, and an
/// NPC without a saber (`None`) holds none. `None` for a model that is not the humanoid's
/// and has no fallback.
pub(super) fn load_skeleton_models(
    cache: &mut HashMap<String, Arc<SkeletonModels>>,
    map: &crate::map::LoadedMap,
    model: &str,
    hilts: Option<&[HiltKey; 2]>,
) -> Option<Arc<SkeletonModels>> {
    let name = model
        .split('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(DEFAULT_MODEL)
        .to_ascii_lowercase();
    let key = match hilts {
        Some(hilts) => format!(
            "{name}|{}|{}|{}|{}|{}|{}",
            String::from_utf8_lossy(&hilts[0].model),
            hilts[0].blades,
            hilts[0].wrist,
            String::from_utf8_lossy(&hilts[1].model),
            hilts[1].blades,
            hilts[1].wrist
        ),
        None => format!("{name}|unarmed"),
    };
    if let Some(models) = cache.get(&key) {
        return Some(models.clone());
    }
    let files = &map.files;
    let read = |path: &[u8]| {
        files
            .read(&String::from_utf8_lossy(path))
            .ok()
            .flatten()
            .map(|asset| asset.bytes)
    };
    let (first, second) = match hilts {
        Some(hilts) => (
            read(&hilts[0].model).or_else(|| read(DEFAULT_HILT.as_bytes())),
            (!hilts[1].model.is_empty())
                .then(|| read(&hilts[1].model))
                .flatten(),
        ),
        None => (None, None),
    };
    let keys = hilts.cloned().unwrap_or_default();
    let load = |name: &str| -> Option<SkeletonModels> {
        let body = files
            .read(&format!("models/players/{name}/model.glm"))
            .ok()
            .flatten()?
            .bytes;
        let skeleton_name = sjk_model::Glm::parse(&body).ok()?.animation_name;
        let gla = files
            .read(&format!("{skeleton_name}.gla"))
            .ok()
            .flatten()?
            .bytes;
        let directory = skeleton_name
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory);
        let config = files
            .read(&format!("{directory}/animation.cfg"))
            .ok()
            .flatten()?
            .bytes;
        fn spec<'m>(model: &'m Option<Vec<u8>>, key: &HiltKey) -> Option<HiltSpec<'m>> {
            model.as_deref().map(|model| HiltSpec {
                model,
                blades: key.blades,
                wrist: key.wrist,
            })
        }
        SkeletonModels::with_hilts(
            &gla,
            &config,
            &body,
            [spec(&first, &keys[0]), spec(&second, &keys[1])],
        )
        .map_err(|error| eprintln!("skeleton for {name}: {error}"))
        .ok()
    };
    let models = Arc::new(load(&name).or_else(|| {
        (name != DEFAULT_MODEL)
            .then(|| load(DEFAULT_MODEL))
            .flatten()
    })?);
    cache.insert(key, models.clone());
    Some(models)
}

impl NativeGame {
    /// [`load_skeleton_models`] for a player; `None` on a server without game data.
    fn skeleton_models(
        &mut self,
        model: &str,
        hilts: &[HiltKey; 2],
    ) -> Option<Arc<SkeletonModels>> {
        let Self { skeletons, map, .. } = self;
        load_skeleton_models(skeletons, map.as_ref()?, model, Some(hilts))
    }

    /// `WP_SaberPositionUpdate`'s skeleton for one client, this frame.
    pub(super) fn pose_saber(&mut self, client: usize, server_time: i32) {
        let ghoul2_time = if self.previous_frame_time == 0 {
            server_time
        } else {
            self.previous_frame_time
        };
        // The userinfo's `model`, as `ClientUserinfoChanged` reads it; compared in place,
        // so a frame allocates nothing unless the model changed. Team skins and a siege
        // class's forced model are not applied yet.
        let Some(peer) = self.peer(client) else {
            return;
        };
        let current = sjk_protocol::info_value(&peer.userinfo, b"model").unwrap_or_default();
        let hands = &peer.sabers.hands;
        let stale = |skeleton: &PlayerSkeleton| {
            skeleton.model != current
                || !skeleton
                    .hilts
                    .iter()
                    .zip(hands)
                    .all(|(key, saber)| key.matches(saber))
        };
        if peer.skeleton.as_ref().is_none_or(stale) {
            let (model, hilts) = (
                current.to_vec(),
                [HiltKey::of(&hands[0]), HiltKey::of(&hands[1])],
            );
            let Some(models) = self.skeleton_models(&String::from_utf8_lossy(&model), &hilts)
            else {
                return;
            };
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            peer.skeleton = Some(PlayerSkeleton {
                skeleton: ServerSkeleton::new(&models),
                models,
                memory: AngleMemory::default(),
                model,
                hilts,
            });
        }
        let look_target = self.look_target_of(client);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        // `UpdateClientRenderinfo`'s cheap muzzle (`w_saber.c:7468-7470`), once a frame.
        peer.muzzle = (peer.state.origin(), peer.muzzle.0);
        let state = &peer.state;
        let inputs = AngleInputs {
            client_animations: None,
            origin: state.origin(),
            view: state.view_angles(),
            velocity: state.velocity(),
            legs: state.leg_animation(),
            torso: state.torso_animation(),
            weapon: state.weapon(),
            eflags: state.entity_flags(),
            movement_dir: i32::from(state.movement_direction()),
            ground: state.ground_entity_num(),
            saber_move: state.saber_move(),
            held: state.raw_field(PS_SABER_LOCK_FRAME).unwrap_or(0) != 0
                || state.vehicle_entity_num() != 0,
            look_target,
        };
        let animation = animation_inputs(peer);
        // `returnAfterUpdate`: the angles and the animations are kept up to date, but no
        // blade is read while the saber is not held out, or for a corpse.
        let holds_saber = inputs.weapon == WP_SABER
            && ![WEAPON_RAISING, WEAPON_DROPPING].contains(&state.weapon_state())
            && peer.health > 0;
        let origin = proper_origin(inputs.origin, inputs.velocity, SERVER_FPS);
        let Some(player) = peer.skeleton.as_mut() else {
            return;
        };
        let PlayerSkeleton {
            models,
            skeleton,
            memory,
            ..
        } = player;
        let motion = if corrects_for_motion(&inputs) {
            skeleton
                .motion_bolt(models, inputs.origin, ghoul2_time)
                .ok()
                .flatten()
        } else {
            None
        };
        // A skeleton not yet animated has no `Motion` to correct for: it corrects for none.
        let angles = player_angles(&inputs, memory, server_time, || {
            motion.unwrap_or(LEVEL_MOTION)
        });
        // Every blade of every hilt, as the damage loop reads them (`G2API_GetBoltMatrix(
        // ghoul2, saber + 1, blade, ...)`, all at the same clock).
        let mut blades = [[None; MAX_BLADES]; 2];
        let posed = skeleton.set_angles(models, &angles).and_then(|()| {
            if !holds_saber {
                return Ok(());
            }
            for (saber, row) in blades.iter_mut().enumerate() {
                for (blade, slot) in row.iter_mut().enumerate().take(models.blade_count(saber)) {
                    *slot = skeleton.blade_of(
                        models,
                        saber,
                        blade,
                        angles.legs,
                        origin,
                        ghoul2_time,
                    )?;
                }
            }
            Ok(())
        });
        let updated = skeleton.update_animations(models, &animation, server_time);
        match (posed, updated) {
            (Ok(()), Ok(())) => {
                if let Some(first) = blades[0][0] {
                    peer.blade = Some((first, server_time));
                    peer.blades_old = peer.blades;
                    peer.blades = (blades, server_time);
                }
            }
            (Err(error), _) | (_, Err(error)) => eprintln!("client {client}'s skeleton: {error}"),
        }
    }

    /// Where the client looks, if it looks at another player (`ps.hasLookTarget`): that
    /// player's origin. A look target that is not a player is not followed yet.
    fn look_target_of(&self, client: usize) -> Option<[f32; 3]> {
        let state = &self.peer(client)?.state;
        if state.raw_field(PS_HAS_LOOK_TARGET).unwrap_or(0) == 0 {
            return None;
        }
        let target = state.raw_field(PS_LOOK_TARGET).unwrap_or(0) as usize;
        self.peer(target).map(|other| other.state.origin())
    }
}
