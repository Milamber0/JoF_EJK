//! Fixed-storage bone-animation overrides with captured-pose cross-fades.
//!
//! The machinery is format- and game-agnostic: callers provide bone indices,
//! frame ranges, clocks, speeds, and end behavior. An override is inherited by
//! descendants until a child supplies its own override.

use super::bone_angles::{BoneAngleOverride, compile};
use super::bone_override_timing::{advance_override, timing};
use super::{BoneAngleCommand, Gla, ModelError, interpolate_3x4, multiply_3x4};

/// Behavior after an animation override reaches its terminal frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverrideEndBehavior {
    /// Wrap to the start of the supplied frame range.
    Loop,
    /// Hold the terminal frame indefinitely.
    Freeze,
    /// Remove the override and resume the inherited parent track.
    Stop,
}

/// One command that installs or replaces a bone-animation override.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneAnimationCommand {
    /// Adapter-owned identity used for change detection and diagnostics.
    pub clip: usize,
    /// Inclusive first clock frame. May be greater than `end_frame` in reverse playback.
    pub start_frame: i32,
    /// Exclusive forward endpoint or reverse terminal boundary.
    pub end_frame: i32,
    /// Frames advanced per 50 milliseconds; negative values play backwards.
    pub speed: f32,
    /// Command timestamp in the caller's presentation clock.
    pub time_millis: i64,
    /// Optional fractional frame at which the new override begins.
    pub set_frame: Option<f32>,
    /// Endpoint behavior selected by the adapter.
    pub end_behavior: OverrideEndBehavior,
    /// Whether to capture and cross-fade from the replaced override.
    pub blend: bool,
    /// Duration of the captured-pose cross-fade.
    pub blend_millis: u16,
}

/// Evaluated frame pair and current-to-next interpolation fraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneFrameSample {
    pub current_frame: usize,
    pub next_frame: usize,
    pub fraction: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CapturedBlend {
    frame: f32,
    following_frame: i32,
    started_at_millis: i64,
    duration_millis: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoneOverride {
    pub(super) clip: usize,
    pub(super) start_frame: i32,
    pub(super) end_frame: i32,
    pub(super) speed: f32,
    pub(super) start_time_millis: i64,
    pub(super) end_behavior: OverrideEndBehavior,
    blend: Option<CapturedBlend>,
}

#[derive(Clone, Copy, Debug)]
struct EvaluatedOverride {
    sample: BoneFrameSample,
    blend: Option<(f32, i32, f32)>,
}

/// Per-actor override state, inherited timing cache, and pose scratch.
#[derive(Clone, Debug)]
pub struct BoneOverridePose {
    overrides: Vec<Option<BoneOverride>>,
    angle_overrides: Vec<Option<BoneAngleOverride>>,
    inherited: Vec<Option<EvaluatedOverride>>,
    local_matrices: Vec<[[f32; 4]; 3]>,
    matrices: Vec<[[f32; 4]; 3]>,
    states: Vec<u8>,
    /// Whether a looping override's start is moved up as it wraps (`G2_Animate_Bone_List`).
    rebase_loops: bool,
}

impl BoneOverridePose {
    /// Allocate all bone state once for an actor skeleton.
    pub fn new(bone_count: usize) -> Self {
        Self {
            overrides: vec![None; bone_count],
            angle_overrides: vec![None; bone_count],
            inherited: vec![None; bone_count],
            local_matrices: vec![[[0.0; 4]; 3]; bone_count],
            matrices: vec![[[0.0; 4]; 3]; bone_count],
            states: vec![0; bone_count],
            rebase_loops: true,
        }
    }

    /// The pose as the dedicated server's Ghoul2 keeps it: a looping override keeps the
    /// start it was set with (`G2_TimingModel` wraps the frame without moving it; only
    /// `G2API_AnimateG2Models`, a ragdoll's, rebases it, `G2_bones.cpp:1080-1150`). So a
    /// skeleton read at an earlier time after a later one (a bolt at the Ghoul2 clock
    /// after a collision at the frame's time) finds the frames that time has.
    pub fn without_loop_rebasing(mut self) -> Self {
        self.rebase_loops = false;
        self
    }

    /// Install or replace an angle override on one bone.
    pub fn set_bone_angles(
        &mut self,
        animation: &Gla,
        bone: usize,
        command: BoneAngleCommand,
    ) -> Result<(), ModelError> {
        let Some(definition) = animation.bones.get(bone) else {
            return Err(ModelError::invalid(bone, "bone angle index is invalid"));
        };
        if !command.angles_degrees.into_iter().all(f32::is_finite) {
            return Err(ModelError::invalid(bone, "bone angles are not finite"));
        }
        self.angle_overrides[bone] = Some(compile(definition, command));
        Ok(())
    }

    /// The matrix an angle override applies to one bone (Ghoul2's `boneInfo_t::matrix`),
    /// if the bone has one: for evidence against the dedicated server's.
    pub fn bone_angle_matrix(&self, bone: usize) -> Option<[[f32; 4]; 3]> {
        self.angle_overrides
            .get(bone)
            .copied()
            .flatten()
            .map(|angle| angle.matrix)
    }

    /// Remove an angle override from one bone.
    pub fn clear_bone_angles(&mut self, bone: usize) -> bool {
        self.angle_overrides
            .get_mut(bone)
            .is_some_and(|slot| slot.take().is_some())
    }

    /// Install a bone override, capturing the replaced override at command time.
    pub fn set_bone_animation(
        &mut self,
        animation: &Gla,
        bone: usize,
        command: BoneAnimationCommand,
    ) -> Result<(), ModelError> {
        validate_command(animation, bone, command)?;
        let old = self.overrides[bone];
        let captured = if command.blend {
            if let Some(mut pending) = old
                .and_then(|old| old.blend)
                .filter(|blend| blend.started_at_millis == command.time_millis)
            {
                pending.duration_millis = command.blend_millis;
                Some(pending)
            } else {
                old.map(|old| capture(old, command.time_millis, animation.frames.len()))
                    .transpose()?
                    .map(|(frame, following_frame)| CapturedBlend {
                        frame,
                        following_frame,
                        started_at_millis: command.time_millis,
                        duration_millis: command.blend_millis,
                    })
            }
        } else {
            None
        };
        let start_time_millis = command.set_frame.map_or(command.time_millis, |frame| {
            command.time_millis
                - (((frame - command.start_frame as f32) * 50.0 / command.speed) as i64)
        });
        self.overrides[bone] = Some(BoneOverride {
            clip: command.clip,
            start_frame: command.start_frame,
            end_frame: command.end_frame,
            speed: command.speed,
            start_time_millis,
            end_behavior: command.end_behavior,
            blend: captured,
        });
        Ok(())
    }

    /// Evaluate a command without installing it, useful when deriving a set frame.
    pub fn sample_command(
        animation: &Gla,
        command: BoneAnimationCommand,
        time_millis: i64,
    ) -> Result<BoneFrameSample, ModelError> {
        validate_command(animation, 0, command)?;
        let start_time_millis = command.set_frame.map_or(command.time_millis, |frame| {
            command.time_millis
                - (((frame - command.start_frame as f32) * 50.0 / command.speed) as i64)
        });
        timing(
            BoneOverride {
                clip: command.clip,
                start_frame: command.start_frame,
                end_frame: command.end_frame,
                speed: command.speed,
                start_time_millis,
                end_behavior: command.end_behavior,
                blend: None,
            },
            time_millis,
            animation.frames.len(),
        )
    }

    /// Evaluate every local bone and compose the inherited hierarchy once.
    pub fn evaluate(
        &mut self,
        animation: &Gla,
        time_millis: i64,
    ) -> Result<&mut [[[f32; 4]; 3]], ModelError> {
        let bone_count = animation.bones.len();
        if self.overrides.len() != bone_count {
            return Err(ModelError::invalid(
                self.overrides.len(),
                "override storage differs from GLA bone count",
            ));
        }
        if self.rebase_loops {
            for state in &mut self.overrides {
                advance_override(state, time_millis);
            }
        }
        self.inherited.fill(None);
        self.states.fill(0);
        for bone in 0..bone_count {
            evaluate_bone(
                animation,
                &self.overrides,
                &self.angle_overrides,
                time_millis,
                bone,
                &mut self.inherited,
                &mut self.local_matrices,
                &mut self.matrices,
                &mut self.states,
            )?;
        }
        Ok(&mut self.matrices)
    }

    /// Evaluate one joint and its ancestors, returning its model-space attachment matrix.
    /// Uses the existing fixed scratch; unrelated joints are not decompressed.
    pub fn sample_joint(
        &mut self,
        animation: &Gla,
        bone: usize,
        time_millis: i64,
    ) -> Result<[[f32; 4]; 3], ModelError> {
        if self.overrides.len() != animation.bones.len() || bone >= animation.bones.len() {
            return Err(ModelError::invalid(
                bone,
                "joint differs from skeleton storage",
            ));
        }
        if self.rebase_loops {
            for state in &mut self.overrides {
                advance_override(state, time_millis);
            }
        }
        self.inherited.fill(None);
        self.states.fill(0);
        evaluate_bone(
            animation,
            &self.overrides,
            &self.angle_overrides,
            time_millis,
            bone,
            &mut self.inherited,
            &mut self.local_matrices,
            &mut self.matrices,
            &mut self.states,
        )?;
        Ok(multiply_3x4(
            self.matrices[bone],
            animation.bones[bone].base_pose,
        ))
    }

    /// Composed skinning matrices from the most recent successful evaluation.
    pub fn matrices(&self) -> &[[[f32; 4]; 3]] {
        &self.matrices
    }

    /// Local matrices from the most recent evaluation, before parent composition.
    pub fn local_matrices(&self) -> &[[[f32; 4]; 3]] {
        &self.local_matrices
    }

    /// Backing addresses used to prove that evaluation does not reallocate.
    pub fn storage_addresses(&self) -> [usize; 6] {
        [
            self.overrides.as_ptr() as usize,
            self.angle_overrides.as_ptr() as usize,
            self.inherited.as_ptr() as usize,
            self.local_matrices.as_ptr() as usize,
            self.matrices.as_ptr() as usize,
            self.states.as_ptr() as usize,
        ]
    }
}

fn validate_command(
    animation: &Gla,
    bone: usize,
    command: BoneAnimationCommand,
) -> Result<(), ModelError> {
    if bone >= animation.bones.len() || command.start_frame < 0 || command.end_frame < 0 {
        return Err(ModelError::invalid(
            bone,
            "bone override index/range is invalid",
        ));
    }
    if command.start_frame as usize > animation.frames.len()
        || command.end_frame as usize > animation.frames.len()
        || !command.speed.is_finite()
        || command.speed == 0.0
    {
        return Err(ModelError::invalid(
            bone,
            "bone override command is out of range",
        ));
    }
    Ok(())
}

fn capture(
    old: BoneOverride,
    time_millis: i64,
    frame_count: usize,
) -> Result<(f32, i32), ModelError> {
    let sample = timing(old, time_millis, frame_count)?;
    let current = sample.current_frame as f32 + sample.fraction;
    if old.speed < 0.0 {
        let frame = current.floor();
        return Ok((frame, frame as i32));
    }
    let mut frame = current;
    let mut following = current + 1.0;
    if frame >= old.end_frame as f32 {
        frame = if old.end_behavior == OverrideEndBehavior::Loop {
            old.start_frame as f32
        } else {
            (old.end_frame - 1).max(0) as f32
        };
    }
    if following >= old.end_frame as f32 {
        following = if old.end_behavior == OverrideEndBehavior::Loop {
            old.start_frame as f32
        } else {
            (old.end_frame - 1).max(0) as f32
        };
    }
    Ok((frame, following as i32))
}

fn evaluate_bone(
    animation: &Gla,
    overrides: &[Option<BoneOverride>],
    angle_overrides: &[Option<BoneAngleOverride>],
    time_millis: i64,
    bone: usize,
    inherited: &mut [Option<EvaluatedOverride>],
    local_matrices: &mut [[[f32; 4]; 3]],
    matrices: &mut [[[f32; 4]; 3]],
    states: &mut [u8],
) -> Result<EvaluatedOverride, ModelError> {
    if states[bone] == 2 {
        return inherited[bone]
            .ok_or_else(|| ModelError::invalid(bone, "bone has no inherited animation"));
    }
    if states[bone] == 1 {
        return Err(ModelError::invalid(bone, "cyclic GLA bone hierarchy"));
    }
    states[bone] = 1;
    let parent = animation.bones[bone]
        .parent
        .map(|parent| {
            evaluate_bone(
                animation,
                overrides,
                angle_overrides,
                time_millis,
                parent,
                inherited,
                local_matrices,
                matrices,
                states,
            )
        })
        .transpose()?;
    let evaluated = if let Some(state) = overrides[bone] {
        let sample = timing(state, time_millis, animation.frames.len())?;
        let blend = state.blend.and_then(|blend| {
            let elapsed = time_millis - blend.started_at_millis;
            (elapsed >= 0 && elapsed < i64::from(blend.duration_millis)).then(|| {
                let fraction = if blend.duration_millis == 0 {
                    1.0
                } else {
                    elapsed as f32 / f32::from(blend.duration_millis)
                };
                (blend.frame, blend.following_frame, fraction)
            })
        });
        EvaluatedOverride { sample, blend }
    } else {
        // A root with no animation holds the first frame, unlerped and unblended
        // (`G2_TransformGhoulBones`' root `SBoneCalc`, `tr_ghoul2.cpp:1993-2000`).
        parent.unwrap_or(EvaluatedOverride {
            sample: BoneFrameSample {
                current_frame: 0,
                next_frame: 0,
                fraction: 0.0,
            },
            blend: None,
        })
    };
    let first = animation
        .bone_matrix(evaluated.sample.current_frame, bone)
        .ok_or_else(|| ModelError::invalid(bone, "missing current compressed bone matrix"))?;
    let second = animation
        .bone_matrix(evaluated.sample.next_frame, bone)
        .ok_or_else(|| ModelError::invalid(bone, "missing next compressed bone matrix"))?;
    let mut local = interpolate_3x4(first, second, evaluated.sample.fraction);
    if let Some((frame, following, blend_fraction)) = evaluated.blend {
        let captured_frame = frame as usize;
        let captured_following = usize::try_from(following)
            .map_err(|_| ModelError::invalid(bone, "negative captured animation frame"))?;
        let captured_first = animation
            .bone_matrix(captured_frame, bone)
            .ok_or_else(|| ModelError::invalid(bone, "missing captured bone matrix"))?;
        let captured_second = animation
            .bone_matrix(captured_following, bone)
            .ok_or_else(|| ModelError::invalid(bone, "missing captured lerp bone matrix"))?;
        let captured = interpolate_3x4(captured_first, captured_second, 1.0 - frame.fract());
        local = interpolate_3x4(captured, local, blend_fraction);
    }
    let angle = angle_overrides[bone];
    let posed_local = match angle {
        Some(angle) => multiply_3x4(local, angle.matrix),
        None => local,
    };
    local_matrices[bone] = posed_local;
    matrices[bone] = if let Some(parent_index) = animation.bones[bone].parent {
        multiply_3x4(matrices[parent_index], posed_local)
    } else {
        posed_local
    };
    inherited[bone] = Some(evaluated);
    states[bone] = 2;
    Ok(evaluated)
}
