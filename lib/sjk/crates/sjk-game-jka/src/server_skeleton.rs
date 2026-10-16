//! The skeleton the server poses for each player, and the saber blade read from it.
//!
//! A retail dedicated server keeps a Ghoul2 instance per player: the player model, and
//! the saber hilt as a second model bolted to its right hand. `WP_SaberPositionUpdate`
//! (`codemp/game/w_saber.c:8091`) poses it every frame and reads the blade from the
//! hilt's `*blade1` bolt; saber contact is decided against that blade. This module is
//! that instance's behaviour, on `sjk-model`'s skeleton evaluator:
//!
//! - [`ServerSkeleton::update_animations`] is `G_UpdateClientAnims` (`g_client.c:2833`):
//!   the legs' animation on `model_root`, the torso's on `lower_lumbar` and `Motion`,
//!   each replaced only when it changes, blended over 150 ms from the pose it replaces.
//! - [`ServerSkeleton::set_angles`] installs `G_G2PlayerAngles`' five spine commands
//!   ([`crate::g2_player_angles`]).
//! - [`ServerSkeleton::blade`] is `G2API_GetBoltMatrix(ghoul2, 1, 0, ...)` on the hilt:
//!   the skeleton at the Ghoul2 clock, the hand bolt, the hilt's bolt on it, the rows
//!   normalized, the world matrix, and the multiplayer 90° column swap.
//!
//! The Ghoul2 clock is the **previous** frame's time: the engine sets it after each
//! game frame (`server/sv_main.cpp:1225-1236`). Callers pass it as such.

use crate::g2_player_angles::PlayerAngles;
use crate::legacy_animation::NAMES;
use sjk_model::{
    AnimationConfig, BoneAngleCommand, BoneAngleMode, BoneAnimationCommand, BoneAxis,
    BoneOverridePose, Gla, Glm, ModelError, OverrideEndBehavior,
};

/// Ghoul2's root "identity" (`tr_ghoul2.cpp:128-135`): a quarter turn about Z, which the
/// generic evaluator leaves out and every raw Ghoul2 matrix carries.
pub(crate) const GHOUL2_ROOT: [[f32; 4]; 3] = [
    [0.0, -1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];
/// The blend `G_UpdateClientAnims` asks for on every change.
const BLEND_MILLIS: u16 = 150;
/// The player's bolts a hilt is bolted to (`G_SetG2PlayerModel`'s bolts 0, 1, 3 and 4):
/// the right and left hands, or their wrists for `boltToWrist`.
const HAND_BOLTS: [&str; 2] = ["*r_hand", "*l_hand"];
const WRIST_BOLTS: [&str; 2] = ["*r_hand_cap_r_arm", "*l_hand_cap_l_arm"];
/// The most blades a hilt has (`MAX_BLADES`).
pub const MAX_BLADES: usize = 8;

/// A saber hilt to load onto the skeleton: its model, how many blades its definition
/// gives it, and whether it is bolted to the wrist (`SFL_BOLT_TO_WRIST`).
#[derive(Clone, Copy, Debug)]
pub struct HiltSpec<'a> {
    pub model: &'a [u8],
    pub blades: usize,
    pub wrist: bool,
}

/// A hilt on the skeleton (`weaponGhoul2[n]` copied in at model `n + 1`).
struct Hilt {
    glm: Glm,
    /// Its single bone at rest: the hilt's own skeleton is the default one, which never
    /// moves.
    bones: Vec<[[f32; 4]; 3]>,
    /// Each blade's bolt at rest, in blade order (`G_SaberModelSetup`: `*blade1` on, or
    /// `*flash` alone for a hilt without them).
    sockets: Vec<[[f32; 4]; 3]>,
    /// The player's bolt it hangs on.
    hand: &'static str,
}

impl Hilt {
    fn load(spec: &HiltSpec<'_>, hand: usize) -> Result<Self, ModelError> {
        let glm = Glm::parse(spec.model)?;
        let bones = vec![IDENTITY; glm.bone_count];
        let mut sockets = Vec::with_capacity(spec.blades.clamp(1, MAX_BLADES));
        for blade in 0..spec.blades.clamp(1, MAX_BLADES) {
            match glm.surface_bolt_matrix(&format!("*blade{}", blade + 1), 0, &bones)? {
                Some(socket) => sockets.push(socket),
                None => {
                    // "guess this is an 0ldsk3wl saber"
                    if blade == 0
                        && let Some(flash) = glm.surface_bolt_matrix("*flash", 0, &bones)?
                    {
                        sockets.push(flash);
                    }
                    break;
                }
            }
        }
        Ok(Self {
            glm,
            bones,
            sockets,
            hand: if spec.wrist {
                WRIST_BOLTS[hand]
            } else {
                HAND_BOLTS[hand]
            },
        })
    }
}

/// One `animation.cfg` row as `BG_ParseAnimationFile` keeps it (`bg_panimate.c:2416,
/// 2446-2484`): unlisted animations are empty with a 100 ms frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sequence {
    first: i32,
    count: i32,
    loops: bool,
    frame_lerp: i32,
}

impl Sequence {
    /// Every animation's row of `config`, by animation number.
    pub(crate) fn table(config: &AnimationConfig) -> Vec<Self> {
        NAMES
            .iter()
            .map(|name| {
                config.get(name).map_or_else(Self::default, |row| {
                    // `fps = atof(...)`, then `frameLerp = floor/ceil(1000.0f / fps)`.
                    let fps = if row.frames_per_second == 0.0 {
                        1.0
                    } else {
                        row.frames_per_second
                    };
                    let frame_lerp = if fps < 0.0 {
                        (1000.0 / fps).floor()
                    } else {
                        (1000.0 / fps).ceil()
                    } as i32;
                    Self {
                        first: row.first_frame as i32,
                        count: row.frame_count as i32,
                        loops: row.loop_frame != -1,
                        frame_lerp,
                    }
                })
            })
            .collect()
    }

    /// An animation the model's `animation.cfg` does not list (no frames from frame 0),
    /// which `G_UpdateClientAnims` does not install on a skeleton of its own.
    pub(crate) fn is_empty(&self) -> bool {
        self.first == 0 && self.count == 0
    }
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            first: 0,
            count: 0,
            loops: false,
            frame_lerp: 100,
        }
    }
}

/// What a skeleton needs of the game's data: its skeleton (the humanoid one, or a
/// creature's own), its animations, the model and the sabers' hilts.
pub struct SkeletonModels {
    gla: Gla,
    body: Glm,
    /// The sabers' hilts, bolted to the hands, which traces meet too.
    hilts: [Option<Hilt>; 2],
    sequences: Vec<Sequence>,
    root: usize,
    /// `lower_lumbar`, which a creature's skeleton may lack (`noLumbar`, `g_client.c:1931-1937`).
    lower_lumbar: Option<usize>,
    /// `Motion`: the humanoid skeleton's alone is animated.
    motion: Option<usize>,
    /// The five spine bones `G_G2PlayerAngles` turns: the humanoid skeleton's.
    spine: Option<[usize; 5]>,
    /// The humanoid skeleton (`localAnimIndex <= 1`: `players/_humanoid/` or
    /// `players/rockettrooper/`), or a creature's own (`SetupGameGhoul2Model`,
    /// `g_client.c:1795-1830`).
    humanoid: bool,
}

impl SkeletonModels {
    /// Build from the files' bytes: `_humanoid.gla`, its `animation.cfg`, the player's
    /// `model.glm` and the saber's hilt model.
    pub fn new(
        gla: &[u8],
        animation_cfg: &[u8],
        body: &[u8],
        hilt: &[u8],
    ) -> Result<Self, ModelError> {
        Self::with_hilts(
            gla,
            animation_cfg,
            body,
            [
                Some(HiltSpec {
                    model: hilt,
                    blades: 1,
                    wrist: false,
                }),
                None,
            ],
        )
    }

    /// The skeleton with a hilt in each hand that holds one. The skeleton is the humanoid
    /// one when its name says so (its five spine bones then required), otherwise a
    /// creature's: its root required, its `lower_lumbar` where it has one.
    pub fn with_hilts(
        gla: &[u8],
        animation_cfg: &[u8],
        body: &[u8],
        hilts: [Option<HiltSpec<'_>>; 2],
    ) -> Result<Self, ModelError> {
        let gla = Gla::parse(gla)?;
        let lower_name = gla.name.to_ascii_lowercase();
        let humanoid = lower_name.contains("players/_humanoid/")
            || lower_name.contains("players/rockettrooper/");
        let config = AnimationConfig::parse(animation_cfg)?;
        let sequences = Sequence::table(&config);
        let position = |name: &str| {
            gla.bones
                .iter()
                .position(|bone| bone.name.eq_ignore_ascii_case(name))
        };
        let find = |name: &str| {
            position(name).ok_or_else(|| ModelError::invalid(0, "a skeleton bone is missing"))
        };
        let root = find("model_root")?;
        let (lower_lumbar, motion, spine) = if humanoid {
            let lower_lumbar = find("lower_lumbar")?;
            let spine = [
                lower_lumbar,
                find("upper_lumbar")?,
                find("thoracic")?,
                find("cervical")?,
                find("cranium")?,
            ];
            (Some(lower_lumbar), Some(find("Motion")?), Some(spine))
        } else {
            (position("lower_lumbar"), None, None)
        };
        let [first, second] = hilts;
        let hilts = [
            first.map(|spec| Hilt::load(&spec, 0)).transpose()?,
            second.map(|spec| Hilt::load(&spec, 1)).transpose()?,
        ];
        Ok(Self {
            body: Glm::parse(body)?,
            hilts,
            gla,
            sequences,
            root,
            lower_lumbar,
            motion,
            spine,
            humanoid,
        })
    }

    /// `G2_IsSurfaceRendered` for the model with `overrides` (an instance's
    /// `G2API_SetSurfaceOnOff`s): see [`ServerSkeleton::surface_status`].
    pub fn surface_status(&self, name: &str, overrides: &[(usize, u32)]) -> i32 {
        let hierarchy = &self.body.hierarchy;
        let Some(index) = hierarchy
            .iter()
            .position(|surface| surface.name.eq_ignore_ascii_case(name))
        else {
            return -1;
        };
        let overridden = |index: usize| {
            overrides
                .iter()
                .find(|(known, _)| *known == index)
                .map(|(_, flags)| *flags)
        };
        let mut flags = hierarchy[index].flags;
        let mut parent = hierarchy[index].parent;
        while let Some(at) = parent {
            if overridden(at).unwrap_or(hierarchy[at].flags) & SURFACE_NO_DESCENDANTS != 0 {
                flags |= SURFACE_OFF;
                break;
            }
            parent = hierarchy[at].parent;
        }
        if flags == 0 {
            flags = overridden(index).unwrap_or(flags);
        }
        flags as i32
    }

    /// Whether this is the humanoid skeleton (`localAnimIndex <= 1`).
    pub fn humanoid(&self) -> bool {
        self.humanoid
    }

    /// Whether the model has a bolt of this name: a surface bolt (`*l_hand`) or a bone
    /// (`jaw_bone`), as `G2API_AddBolt` finds one.
    pub fn has_bolt(&self, name: &str) -> bool {
        if name.starts_with('*') {
            self.body
                .hierarchy
                .iter()
                .any(|surface| surface.name.eq_ignore_ascii_case(name))
        } else {
            self.gla
                .bones
                .iter()
                .any(|bone| bone.name.eq_ignore_ascii_case(name))
        }
    }

    /// How many blades the hilt in `saber`'s hand has bolts for.
    pub fn blade_count(&self, saber: usize) -> usize {
        self.hilts
            .get(saber)
            .and_then(Option::as_ref)
            .map_or(0, |hilt| hilt.sockets.len())
    }

    fn sequence(&self, animation: u16) -> Sequence {
        self.sequences
            .get(usize::from(animation))
            .copied()
            .unwrap_or_default()
    }

    /// `BG_AnimLength` (`bg_panimate.c:1592`): how long an animation plays, in
    /// milliseconds — its frames times its frame time, truncated.
    pub fn animation_length(&self, animation: u16) -> i32 {
        let sequence = self.sequence(animation);
        (sequence.count as f32 * (sequence.frame_lerp as f32).abs()) as i32
    }

    /// `G2API_GetSurfaceName(ghoul2, surface, 0, ...)`: the name of the player model's
    /// surface with this number, as a collision record names it. A record from the hilt
    /// is looked up here all the same, as `G_LocationBasedDamageModifier` does.
    pub fn surface_name(&self, surface: usize) -> Option<&str> {
        let hierarchy = self
            .body
            .lods
            .first()?
            .surfaces
            .get(surface)?
            .hierarchy_index;
        self.body
            .hierarchy
            .get(hierarchy)
            .map(|surface| surface.name.as_str())
    }
}

/// What `G_UpdateClientAnims` reads of the player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AnimationInputs {
    /// `ps.legsAnim`, `ps.torsoAnim`.
    pub legs: u16,
    /// The torso's.
    pub torso: u16,
    /// `ps.legsFlip`, `ps.torsoFlip`: the same animation started again.
    pub legs_flip: bool,
    /// The torso's.
    pub torso_flip: bool,
    /// `ps.weapon`.
    pub weapon: u8,
    /// `ps.fd.saberAnimLevel`.
    pub saber_style: i32,
    /// `ps.brokenLimbs`.
    pub broken_limbs: i32,
    /// `ps.saberLockFrame`: while locked, both halves hold this frame.
    pub saber_lock_frame: i32,
    /// `animSpeedScale`: 2 under Force rage, otherwise 1.
    pub speed_scale: f32,
    /// The hilts' `animSpeedScale` (`saberInfo_t`), each 1 unless its definition says.
    pub hilt_speed_scales: [f32; 2],
}

/// A blade as the server holds it: `lastSaberBase_Always` and `lastSaberDir_Always`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blade {
    /// Where the blade leaves the hilt.
    pub base: [f32; 3],
    /// Which way it points (the bolt's negative Y).
    pub direction: [f32; 3],
}

/// One player's server-side skeleton.
pub struct ServerSkeleton {
    pose: BoneOverridePose,
    /// `legsAnimExecute`/`legsLastFlip` and the torso's: what was last installed.
    legs_installed: Option<(u16, bool)>,
    torso_installed: Option<(u16, bool)>,
    /// Ghoul2's skeleton cache: when the matrices were last built (`None` before the
    /// first build), and `mSkelFrameNum`, the time a bolt read last stamped them with.
    /// A bolt read rebuilds only when the stamp is not its clock; any angle or animation
    /// change clears the stamp; a collision rebuilds at its own time without stamping.
    /// So a read after a collision, with the stamp still its clock, sees the collision's
    /// skeleton — as the dedicated server's does (`G2_NeedsRecalc`, `tr_ghoul2.cpp:2861`;
    /// `G2API_CollisionDetect`, `G2_API.cpp`).
    built_at: Option<i32>,
    stamp: i32,
    /// The overrides changed since the last build: a build at the same time with nothing
    /// changed would be the same, and is skipped.
    changed: bool,
    /// The entity's `modelScale`: every bolt's position and every vertex a collision
    /// tests are scaled by it (zero on an axis: unscaled). A player's is zero.
    scale: [f32; 3],
    /// A vehicle's: `G_UpdateClientAnims` sets its root's animation alone
    /// (`g_client.c:2901-2905`).
    root_only: bool,
    /// The surfaces the game turned on or off (`G2API_SetSurfaceOnOff`, the instance's
    /// `mSlist`): each hierarchy index with its flags, in the order they were first set.
    surface_overrides: Vec<(usize, u32)>,
    /// Angle overrides set with a blend time (`G2API_SetBoneAngles(..., blendTime, time)`,
    /// `NPC_SetBoneAngles`' 100 ms): each bone, its command, when it was set and the blend.
    /// The dedicated server's evaluator honours such an override only until its blend time
    /// has passed (`tr_ghoul2.cpp:1619-1680`: past it, "blending" keeps the animation's
    /// pose), so it lapses unless set again.
    timed_angles: Vec<(usize, BoneAngleCommand, i32, i32)>,
}

impl ServerSkeleton {
    /// A skeleton with nothing installed yet.
    pub fn new(models: &SkeletonModels) -> Self {
        Self {
            pose: BoneOverridePose::new(models.gla.bones.len()).without_loop_rebasing(),
            legs_installed: None,
            torso_installed: None,
            built_at: None,
            stamp: 0,
            changed: true,
            scale: [0.0; 3],
            root_only: false,
            surface_overrides: Vec::new(),
            timed_angles: Vec::new(),
        }
    }

    /// `G2API_SetSurfaceOnOff(ghoul2, name, flags)` (`G2_SetSurfaceOnOff`,
    /// `G2_surfaces.cpp:156-206`): the surface's off and no-descendants bits set from `flags`
    /// — an override kept where it differs from the model's own flags. `false` for a surface
    /// the model lacks.
    pub fn set_surface_on_off(&mut self, models: &SkeletonModels, name: &str, flags: u32) -> bool {
        const BITS: u32 = SURFACE_OFF | SURFACE_NO_DESCENDANTS;
        let Some(index) = models
            .body
            .hierarchy
            .iter()
            .position(|surface| surface.name.eq_ignore_ascii_case(name))
        else {
            return false;
        };
        if let Some((_, known)) = self
            .surface_overrides
            .iter_mut()
            .find(|(known, _)| *known == index)
        {
            *known = (*known & !BITS) | (flags & BITS);
            return true;
        }
        let own = models.body.hierarchy[index].flags;
        let wanted = (own & !BITS) | (flags & BITS);
        if wanted != own {
            self.surface_overrides.push((index, wanted));
        }
        true
    }

    /// `G2API_GetSurfaceRenderStatus(ghoul2, 0, name)` (`G2_IsSurfaceRendered`,
    /// `G2_surfaces.cpp:617-683`): the surface's flags — off where an ancestor hides its
    /// descendants — or -1 for a surface the model lacks. Zero is drawn.
    pub fn surface_status(&self, models: &SkeletonModels, name: &str) -> i32 {
        models.surface_status(name, &self.surface_overrides)
    }

    /// `G2API_SetBoneAngles(ghoul2, 0, bone, angles, BONE_ANGLES_POSTMULT, POSITIVE_X,
    /// NEGATIVE_Y, NEGATIVE_Z, NULL, blend, time)` on a bone by name: nothing for a bone the
    /// skeleton lacks. See [`Self::timed_angles`] for what the blend does.
    pub fn set_bone_angles_named(
        &mut self,
        models: &SkeletonModels,
        bone: &str,
        angles: [f32; 3],
        blend: i32,
        time: i32,
    ) {
        let Some(index) = models
            .gla
            .bones
            .iter()
            .position(|known| known.name.eq_ignore_ascii_case(bone))
        else {
            return;
        };
        self.touch();
        let command = BoneAngleCommand {
            angles_degrees: angles,
            mode: BoneAngleMode::PostMultiply,
            up: BoneAxis::PositiveX,
            left: BoneAxis::NegativeY,
            forward: BoneAxis::NegativeZ,
        };
        self.timed_angles.retain(|(known, ..)| *known != index);
        self.timed_angles.push((index, command, time, blend));
    }

    /// Makes this a vehicle's skeleton, whose root alone `G_UpdateClientAnims` animates.
    pub fn set_root_only(&mut self) {
        self.root_only = true;
    }

    /// Sets the entity's `modelScale` (an NPC's `scale`), which every `G2API` call on it
    /// passes: bolts' positions are scaled (`G2API_GetBoltMatrix`), and so is the mesh a
    /// collision tests (`G2_TransformModel`'s `correctScale`).
    pub fn set_scale(&mut self, scale: [f32; 3]) {
        self.scale = scale;
    }

    /// `G2API_GetBoltMatrix`' "scale the bolt position by the scale factor for this model
    /// since at this point it's still in model space": each axis of the translation a
    /// nonzero scale names.
    fn scaled(&self, mut bolt: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
        for (row, scale) in bolt.iter_mut().zip(self.scale) {
            if scale != 0.0 {
                row[3] *= scale;
            }
        }
        bolt
    }

    /// `G_UpdateClientAnims`, at `level_time`. A creature's skeleton skips an animation its
    /// own `animation.cfg` does not list (`g_client.c:2854-2900`), has its torso's animation
    /// only where it has a `lower_lumbar` bone (`noLumbar`), and no `Motion` bone animated;
    /// a vehicle's has its root's alone.
    pub fn update_animations(
        &mut self,
        models: &SkeletonModels,
        inputs: &AnimationInputs,
        level_time: i32,
    ) -> Result<(), ModelError> {
        self.touch();
        let time = i64::from(level_time);
        if inputs.saber_lock_frame != 0 {
            let frame = inputs.saber_lock_frame;
            let command = BoneAnimationCommand {
                clip: usize::MAX,
                start_frame: frame,
                end_frame: frame + 1,
                speed: inputs.speed_scale,
                time_millis: time,
                set_frame: None,
                end_behavior: OverrideEndBehavior::Freeze,
                blend: true,
                blend_millis: BLEND_MILLIS,
            };
            // `G2API_SetBoneAnim` on a bone the skeleton lacks does nothing.
            for bone in [Some(models.root), models.lower_lumbar, models.motion]
                .into_iter()
                .flatten()
            {
                self.pose.set_bone_animation(&models.gla, bone, command)?;
            }
            return Ok(());
        }
        // "We'll allow this for non-humanoids."
        let unlisted = |animation: u16| {
            !models.humanoid
                && models.sequence(animation).first == 0
                && models.sequence(animation).count == 0
        };
        if !unlisted(inputs.legs) && self.legs_installed != Some((inputs.legs, inputs.legs_flip)) {
            let command = command(
                models.sequence(inputs.legs),
                inputs.legs,
                inputs.speed_scale,
                time,
            );
            self.pose
                .set_bone_animation(&models.gla, models.root, command)?;
            self.legs_installed = Some((inputs.legs, inputs.legs_flip));
        }
        // "If this fails as well just return"; "we only want to set the root bone for vehicles".
        if unlisted(inputs.torso) || self.root_only {
            return Ok(());
        }
        let Some(lower_lumbar) = models.lower_lumbar else {
            return Ok(());
        };
        if self.torso_installed != Some((inputs.torso, inputs.torso_flip)) {
            let scale = transition_speed(inputs, inputs.speed_scale);
            let command = command(models.sequence(inputs.torso), inputs.torso, scale, time);
            self.pose
                .set_bone_animation(&models.gla, lower_lumbar, command)?;
            // "only set the motion bone for humanoids"
            if let Some(motion) = models.motion {
                self.pose.set_bone_animation(&models.gla, motion, command)?;
            }
            self.torso_installed = Some((inputs.torso, inputs.torso_flip));
        }
        Ok(())
    }

    /// `ClientSpawn` clears what `G_UpdateClientAnims` remembers installing, so the next
    /// update installs both halves again — blending from the pose the skeleton still has.
    pub fn forget_animations(&mut self) {
        self.legs_installed = None;
        self.torso_installed = None;
    }

    /// `SetupGameGhoul2Model`'s first animation for a saber user (`g_client.c:1905`): the
    /// root looping frames 0 to 12 at full speed from `time`, unblended.
    pub fn play_setup_loop(
        &mut self,
        models: &SkeletonModels,
        time: i32,
    ) -> Result<(), ModelError> {
        self.touch();
        let command = BoneAnimationCommand {
            clip: usize::MAX,
            start_frame: 0,
            end_frame: 12,
            speed: 1.0,
            time_millis: i64::from(time),
            set_frame: None,
            end_behavior: OverrideEndBehavior::Loop,
            blend: false,
            blend_millis: 0,
        };
        self.pose
            .set_bone_animation(&models.gla, models.root, command)
    }

    /// What a zeroed client remembers installing (an NPC's, `NPC_Spawn_Do`'s `memset`):
    /// `legsAnimExecute` and `torsoAnimExecute` 0, unflipped — so animation 0 is never
    /// installed, and the skeleton keeps its bind pose until another is played.
    pub fn assume_zeroed_client(&mut self) {
        self.legs_installed = Some((0, false));
        self.torso_installed = Some((0, false));
    }

    /// `G_G2PlayerAngles`' `G2API_SetBoneAngles` calls.
    pub fn set_angles(
        &mut self,
        models: &SkeletonModels,
        angles: &PlayerAngles,
    ) -> Result<(), ModelError> {
        self.touch();
        let Some(spine) = models.spine else {
            return Ok(());
        };
        for (bone, command) in spine.into_iter().zip(angles.bones) {
            let Some(degrees) = command else { continue };
            self.pose.set_bone_angles(
                &models.gla,
                bone,
                BoneAngleCommand {
                    angles_degrees: degrees,
                    mode: BoneAngleMode::PostMultiply,
                    up: BoneAxis::PositiveX,
                    left: BoneAxis::NegativeY,
                    forward: BoneAxis::NegativeZ,
                },
            )?;
        }
        Ok(())
    }

    /// The angle overrides `set_angles` left on the five spine bones, lower lumbar to
    /// cranium, as Ghoul2 holds them (`boneInfo_t::matrix`).
    pub fn spine_overrides(&self, models: &SkeletonModels) -> [Option<[[f32; 4]; 3]>; 5] {
        models.spine.map_or([None; 5], |spine| {
            spine.map(|bone| self.pose.bone_angle_matrix(bone))
        })
    }

    /// `G2API_GetBoltMatrix_NoRecNoRot(ghoul2, 0, motionBolt, ...)` at the origin with no
    /// rotation: the `Motion` bone as `BG_G2ClientSpineAngles` reads it. `None` before the
    /// first animations are installed: a skeleton that was never animated has no pose.
    pub fn motion_bolt(
        &mut self,
        models: &SkeletonModels,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
        let Some(motion) = models.motion.filter(|_| self.legs_installed.is_some()) else {
            return Ok(None);
        };
        let matrices = self.read_at_clock(models, ghoul2_time)?;
        let joint = multiply(matrices[motion], models.gla.bones[motion].base_pose);
        let raw = self.scaled(multiply(GHOUL2_ROOT, joint));
        let mut bolt = normalize_rows(raw);
        for (axis, row) in bolt.iter_mut().enumerate() {
            row[3] += origin[axis];
        }
        Ok(Some(bolt))
    }

    /// The first saber's first blade: see [`Self::blade_of`].
    pub fn blade(
        &mut self,
        models: &SkeletonModels,
        angles: [f32; 3],
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<Blade>, ModelError> {
        self.blade_of(models, 0, 0, angles, origin, ghoul2_time)
    }

    /// A blade: `G2API_GetBoltMatrix(ghoul2, saber + 1, blade, ...)`, the bolt of `saber`'s
    /// hilt in the world, posed at `ghoul2_time`, the player placed at `origin` facing
    /// `angles` (`properOrigin`, `properAngles`). `None` for a hilt or bolt that is not
    /// there.
    pub fn blade_of(
        &mut self,
        models: &SkeletonModels,
        saber: usize,
        blade: usize,
        angles: [f32; 3],
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<Blade>, ModelError> {
        let Some(hilt) = models.hilts.get(saber).and_then(Option::as_ref) else {
            return Ok(None);
        };
        let Some(&socket) = hilt.sockets.get(blade) else {
            return Ok(None);
        };
        // Nothing to pose before the first animations: the blade is read from the next frame.
        if self.legs_installed.is_none() {
            return Ok(None);
        }
        let matrices = self.read_at_clock(models, ghoul2_time)?;
        let Some(hand) = models.body.surface_bolt_matrix(hilt.hand, 0, matrices)? else {
            return Ok(None);
        };
        // The hilt's root is the hand's raw bolt.
        let bolt = normalize_rows(self.scaled(multiply(multiply(GHOUL2_ROOT, hand), socket)));
        let world = multiply(world_matrix(angles, origin), bolt);
        // "this is horribly stupid and I hate it. But lots of game code is written to
        // assume this 90 degree offset thing." Column 0 becomes the negated column 1.
        let swapped: [[f32; 4]; 3] = std::array::from_fn(|row| {
            [-world[row][1], world[row][0], world[row][2], world[row][3]]
        });
        Ok(Some(Blade {
            base: [swapped[0][3], swapped[1][3], swapped[2][3]],
            direction: [-swapped[0][1], -swapped[1][1], -swapped[2][1]],
        }))
    }
}

impl ServerSkeleton {
    /// Where a bolt of the player model is in the world: `G2API_GetBoltMatrix(ghoul2, 0,
    /// bolt, ...)`'s origin, the skeleton at `ghoul2_time`, the player at `origin` facing
    /// `yaw`. A name starting with `*` is a surface bolt (`*l_hand`), any other a bone
    /// (`thoracic`). `None` before the first animations, or for a bolt the model lacks.
    pub fn bolt_point(
        &mut self,
        models: &SkeletonModels,
        bolt: &str,
        yaw: f32,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<[f32; 3]>, ModelError> {
        // The rows' normalization and the 90° swap leave the origin where it is.
        Ok(self
            .bolt_matrix(models, bolt, yaw, origin, ghoul2_time)?
            .map(|world| [world[0][3], world[1][3], world[2][3]]))
    }

    /// `G2API_GetBoltMatrix(ghoul2, 0, bolt, ...)` whole, as [`Self::bolt_point`] places it:
    /// the rows normalized, the world matrix and the multiplayer 90° column swap — what
    /// `BG_GiveMeVectorFromMatrix` reads its axes from.
    pub fn bolt_matrix(
        &mut self,
        models: &SkeletonModels,
        bolt: &str,
        yaw: f32,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
        self.bolt_matrix_turned(models, bolt, [0.0, yaw, 0.0], origin, ghoul2_time)
    }

    /// [`Self::bolt_matrix`] with the model turned by all of `angles` (`G2_GenerateWorldMatrix`
    /// reads pitch and roll too): a machine's muzzle at its `r.currentAngles`.
    pub fn bolt_matrix_turned(
        &mut self,
        models: &SkeletonModels,
        bolt: &str,
        angles: [f32; 3],
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
        if self.legs_installed.is_none() {
            return Ok(None);
        }
        let matrices = self.read_at_clock(models, ghoul2_time)?;
        let raw = crate::ghoul2_bolt::model_bolt(&models.body, &models.gla, bolt, matrices)?;
        Ok(raw.map(|raw| crate::ghoul2_bolt::world_bolt(raw, angles, origin, self.scale)))
    }
}

/// `properOrigin` (`w_saber.c:8470-8520`): the origin led along the velocity by a share
/// of its speed, "so it's more like what the client is seeing" — the sum of the
/// velocity's absolute components times `1.6 / sv_fps`, held within ±70.
pub fn proper_origin(origin: [f32; 3], velocity: [f32; 3], sv_fps: f32) -> [f32; 3] {
    let mut direction = velocity;
    crate::player_angle_math::normalize(&mut direction);
    let mut lead = velocity[0].abs() + velocity[1].abs() + velocity[2].abs();
    lead *= 1.6 / sv_fps;
    let lead = lead.clamp(-70.0, 70.0);
    std::array::from_fn(|axis| origin[axis] + direction[axis] * lead)
}

impl ServerSkeleton {
    /// `PM_FootSlopeTrace`'s foot bolt reads, which a saber carrier standing still makes
    /// every command: only their stamp on the cache matters until slope poses are ported.
    pub fn read_foot_bolts(
        &mut self,
        models: &SkeletonModels,
        ghoul2_time: i32,
    ) -> Result<(), ModelError> {
        if self.legs_installed.is_some() {
            self.read_at_clock(models, ghoul2_time)?;
        }
        Ok(())
    }

    /// `PM_FootSlopeTrace`'s two reads (`bg_pmove.c:4692-4700`): where `*l_leg_foot` and
    /// `*r_leg_foot` are with the model at `origin` facing `yaw`, posed at the Ghoul2 clock.
    /// `None` where the model is no humanoid (`pm->ghoul2` unset, `g_active.c:2852-2864`),
    /// lacks either bolt, or has no animation installed yet.
    pub fn foot_points(
        &mut self,
        models: &SkeletonModels,
        yaw: f32,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<([f32; 3], [f32; 3])>, ModelError> {
        if !models.humanoid() {
            return Ok(None);
        }
        let point = |matrix: [[f32; 4]; 3]| [matrix[0][3], matrix[1][3], matrix[2][3]];
        let Some(left) = self.bolt_matrix(models, "*l_leg_foot", yaw, origin, ghoul2_time)? else {
            return Ok(None);
        };
        let Some(right) = self.bolt_matrix(models, "*r_leg_foot", yaw, origin, ghoul2_time)? else {
            return Ok(None);
        };
        Ok(Some((point(left), point(right))))
    }

    /// An angle or animation change: `mSkelFrameNum = 0`.
    fn touch(&mut self) {
        self.stamp = 0;
        self.changed = true;
    }

    /// A bolt read at the Ghoul2 clock: rebuilt only if the stamp is not the clock.
    fn read_at_clock(
        &mut self,
        models: &SkeletonModels,
        clock: i32,
    ) -> Result<&[[[f32; 4]; 3]], ModelError> {
        if self.stamp != clock || self.built_at.is_none() {
            self.stamp = clock;
            self.build(models, clock)
        } else {
            Ok(self.pose.matrices())
        }
    }

    /// `G2_ConstructGhoulSkeleton` at `time`.
    fn build(
        &mut self,
        models: &SkeletonModels,
        time: i32,
    ) -> Result<&[[[f32; 4]; 3]], ModelError> {
        if self.changed || self.built_at != Some(time) {
            for &(bone, command, start, blend) in &self.timed_angles {
                if blend != 0 && start + blend < time {
                    self.pose.clear_bone_angles(bone);
                } else {
                    self.pose.set_bone_angles(&models.gla, bone, command)?;
                }
            }
            self.pose.evaluate(&models.gla, i64::from(time))?;
            self.built_at = Some(time);
            self.changed = false;
        }
        Ok(self.pose.matrices())
    }
}

/// Where a trace meets a posed player: `G2API_CollisionDetect` on its instance.
#[derive(Clone, Copy, Debug)]
pub struct CollisionQuery {
    /// Where the player stands and which way it faces (`ps.origin`, the view's yaw).
    pub origin: [f32; 3],
    /// Its yaw.
    pub yaw: f32,
    /// The time its skeleton is posed at: the frame being run, not the Ghoul2 clock —
    /// `G2API_CollisionDetect` builds its skeleton at the frame number it is given.
    pub time: i32,
    /// The trace's start and end.
    pub start: [f32; 3],
    /// The trace's end.
    pub end: [f32; 3],
    /// The mesh detail it is tested at (`g_g2TraceLod`, 3 by default).
    pub lod: usize,
    /// Half the trace box's width, or zero for a point trace.
    pub radius: f32,
}

impl ServerSkeleton {
    /// `G2API_CollisionDetect` on this player: the body and the hilts on its hands, each
    /// record in the order Ghoul2 fills them. `None` before the first animations.
    pub fn collide(
        &mut self,
        models: &SkeletonModels,
        query: &CollisionQuery,
        scratch: &mut sjk_model::g2_collision::CollisionScratch,
        records: &mut Vec<sjk_model::g2_collision::CollisionRecord>,
    ) -> Result<bool, ModelError> {
        records.clear();
        if self.legs_installed.is_none() {
            return Ok(false);
        }
        let scale = self.scale.map(|axis| if axis == 0.0 { 1.0 } else { axis });
        self.build(models, query.time)?;
        let matrices = self.pose.matrices();
        let body = sjk_model::g2_collision::PosedModel {
            glm: &models.body,
            bones: matrices,
            placement: GHOUL2_ROOT,
            overrides: &self.surface_overrides,
            scale,
        };
        let mut posed = [body, body, body];
        let mut count = 1;
        for hilt in models.hilts.iter().flatten() {
            if let Some(hand) = models.body.surface_bolt_matrix(hilt.hand, 0, matrices)? {
                posed[count] = sjk_model::g2_collision::PosedModel {
                    glm: &hilt.glm,
                    bones: &hilt.bones,
                    placement: multiply(GHOUL2_ROOT, hand),
                    overrides: &[],
                    scale,
                };
                count += 1;
            }
        }
        let axes = crate::pmove::flight::angles_to_axis([0.0, query.yaw, 0.0]);
        sjk_model::g2_collision::collision_detect(
            &posed[..count],
            query.origin,
            query.start,
            query.end,
            query.lod,
            query.radius,
            axes,
            scratch,
            records,
        )?;
        Ok(true)
    }
}

/// `G2SURFACEFLAG_OFF`, `G2SURFACEFLAG_NODESCENDANTS` (`mdx_format.h:63-71`).
const SURFACE_OFF: u32 = 0x2;
const SURFACE_NO_DESCENDANTS: u32 = 0x100;

const IDENTITY: [[f32; 4]; 3] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

/// `G_UpdateClientAnims`' command for one half: `50 / frameLerp` scaled, the range the
/// right way round, looping or holding, blended.
pub(crate) fn command(
    sequence: Sequence,
    animation: u16,
    scale: f32,
    time: i64,
) -> BoneAnimationCommand {
    let speed = 50.0 / sequence.frame_lerp as f32 * scale;
    let (start, end) = if speed < 0.0 {
        (sequence.first + sequence.count, sequence.first)
    } else {
        (sequence.first, sequence.first + sequence.count)
    };
    BoneAnimationCommand {
        clip: usize::from(animation),
        start_frame: start,
        end_frame: end,
        speed,
        time_millis: time,
        set_frame: None,
        end_behavior: if sequence.loops {
            OverrideEndBehavior::Loop
        } else {
            OverrideEndBehavior::Freeze
        },
        blend: true,
        blend_millis: BLEND_MILLIS,
    }
}

/// `BG_SaberStartTransAnim` for the torso's animation
/// ([`crate::pmove_anim::saber_start_trans_anim`]).
fn transition_speed(inputs: &AnimationInputs, speed: f32) -> f32 {
    crate::pmove_anim::saber_start_trans_anim(
        speed,
        inputs.torso,
        inputs.weapon,
        inputs.saber_style,
        inputs.broken_limbs as u8,
        inputs.hilt_speed_scales,
    )
}

/// `Create_Matrix` with the origin (`G2_GenerateWorldMatrix`, `G2_misc.cpp:1609-1665`):
/// `AnglesToAxis` as columns.
pub(crate) fn world_matrix(angles: [f32; 3], origin: [f32; 3]) -> [[f32; 4]; 3] {
    let [forward, left, up] = crate::pmove::flight::angles_to_axis(angles);
    std::array::from_fn(|row| [forward[row], left[row], up[row], origin[row]])
}

/// `VectorNormalize` on each row's first three entries (`G2_API.cpp`, after the scale).
pub(crate) fn normalize_rows(mut matrix: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
    for row in &mut matrix {
        let mut basis = [row[0], row[1], row[2]];
        crate::player_angle_math::normalize(&mut basis);
        row[..3].copy_from_slice(&basis);
    }
    matrix
}

/// `Multiply_3x4Matrix`.
pub(crate) fn multiply(left: [[f32; 4]; 3], right: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            let rotated = left[row][0] * right[0][column]
                + left[row][1] * right[1][column]
                + left[row][2] * right[2][column];
            if column == 3 {
                rotated + left[row][3]
            } else {
                rotated
            }
        })
    })
}
