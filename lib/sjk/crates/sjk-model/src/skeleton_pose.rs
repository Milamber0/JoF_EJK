//! Allocation-free split-track skeletal pose evaluation.
//!
//! A [`BoneTrackPartition`] is built when a skeleton is loaded.  Each render
//! sample then reuses [`PoseScratch`] while interpolating source/target local
//! matrices and composing the hierarchy.  The caller owns game-specific bone
//! names and transition sampling policy.

use super::{Gla, ModelError, SplitAnimationSample, interpolate_3x4, multiply_3x4};

/// Animation track inherited by a bone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoneTrack {
    /// Lower-body/root animation track.
    Lower,
    /// Upper-body animation track.
    Upper,
}

/// Cached per-bone track selection for one skeleton hierarchy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoneTrackPartition {
    tracks: Vec<BoneTrack>,
}

impl BoneTrackPartition {
    /// Build a partition from an upper-body subtree plus explicit overrides.
    ///
    /// This is generic hierarchy policy: callers resolve format/game-specific
    /// bone names to indices before constructing it.
    pub fn from_upper_subtree(
        animation: &Gla,
        upper_root: usize,
        direct_upper: &[usize],
    ) -> Result<Self, ModelError> {
        if upper_root >= animation.bones.len()
            || direct_upper
                .iter()
                .any(|&index| index >= animation.bones.len())
        {
            return Err(ModelError::invalid(
                upper_root,
                "bone-track partition index is out of range",
            ));
        }
        let mut tracks = vec![BoneTrack::Lower; animation.bones.len()];
        for (candidate, track) in tracks.iter_mut().enumerate() {
            let mut current = Some(candidate);
            while let Some(index) = current {
                if index == upper_root {
                    *track = BoneTrack::Upper;
                    break;
                }
                current = animation.bones[index].parent;
            }
        }
        for &index in direct_upper {
            tracks[index] = BoneTrack::Upper;
        }
        Ok(Self { tracks })
    }

    /// Track assigned to `bone`, if it belongs to this partition.
    pub fn track(&self, bone: usize) -> Option<BoneTrack> {
        self.tracks.get(bone).copied()
    }

    /// Number of bones described by this partition.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Whether the partition contains no bones.
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}

/// Reusable matrices and traversal markers for one skeleton pose.
#[derive(Clone, Debug)]
pub struct PoseScratch {
    local_matrices: Vec<[[f32; 4]; 3]>,
    matrices: Vec<[[f32; 4]; 3]>,
    states: Vec<u8>,
}

impl PoseScratch {
    /// Allocate fixed storage once for `bone_count` bones.
    pub fn new(bone_count: usize) -> Self {
        Self {
            local_matrices: vec![[[0.0; 4]; 3]; bone_count],
            matrices: vec![[[0.0; 4]; 3]; bone_count],
            states: vec![0; bone_count],
        }
    }

    /// Evaluate a split lower/upper pose with a caller-supplied cross-fade.
    ///
    /// Frame interpolation and transition interpolation are component-wise on
    /// each local 3x4 matrix before hierarchy composition. Game adapters own
    /// their animation clocks, transition durations, and capture policy.
    pub fn evaluate<'a>(
        &'a mut self,
        animation: &Gla,
        partition: &BoneTrackPartition,
        previous: SplitAnimationSample,
        current: SplitAnimationSample,
        lower_blend_fraction: f32,
        upper_blend_fraction: f32,
    ) -> Result<&'a mut [[[f32; 4]; 3]], ModelError> {
        let bone_count = animation.bones.len();
        if partition.len() != bone_count
            || self.matrices.len() != bone_count
            || self.states.len() != bone_count
        {
            return Err(ModelError::invalid(
                partition.len(),
                "pose storage differs from GLA bone count",
            ));
        }
        let highest_frame = [
            previous.lower_frames.0,
            previous.lower_frames.1,
            previous.upper_frames.0,
            previous.upper_frames.1,
            current.lower_frames.0,
            current.lower_frames.1,
            current.upper_frames.0,
            current.upper_frames.1,
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        if highest_frame >= animation.frames.len() {
            return Err(ModelError::invalid(
                highest_frame,
                "GLA frame is out of range",
            ));
        }
        self.states.fill(0);
        for bone in 0..bone_count {
            evaluate_bone(
                animation,
                partition,
                previous,
                current,
                lower_blend_fraction.clamp(0.0, 1.0),
                upper_blend_fraction.clamp(0.0, 1.0),
                bone,
                &mut self.local_matrices,
                &mut self.matrices,
                &mut self.states,
            )?;
        }
        Ok(&mut self.matrices)
    }

    /// Local matrices from the most recent evaluation, before parent composition.
    pub fn local_matrices(&self) -> &[[[f32; 4]; 3]] {
        &self.local_matrices
    }

    /// Backing addresses used by allocation-regression tests.
    pub fn storage_addresses(&self) -> (usize, usize, usize) {
        (
            self.local_matrices.as_ptr() as usize,
            self.matrices.as_ptr() as usize,
            self.states.as_ptr() as usize,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_bone(
    animation: &Gla,
    partition: &BoneTrackPartition,
    previous: SplitAnimationSample,
    current: SplitAnimationSample,
    lower_blend_fraction: f32,
    upper_blend_fraction: f32,
    bone: usize,
    local_matrices: &mut [[[f32; 4]; 3]],
    matrices: &mut [[[f32; 4]; 3]],
    states: &mut [u8],
) -> Result<[[f32; 4]; 3], ModelError> {
    if states[bone] == 2 {
        return Ok(matrices[bone]);
    }
    if states[bone] == 1 {
        return Err(ModelError::invalid(bone, "cyclic GLA bone hierarchy"));
    }
    states[bone] = 1;
    let (previous_frames, previous_fraction, current_frames, current_fraction, blend_fraction) =
        match partition.track(bone) {
            Some(BoneTrack::Upper) => (
                previous.upper_frames,
                previous.upper_fraction,
                current.upper_frames,
                current.upper_fraction,
                upper_blend_fraction,
            ),
            Some(BoneTrack::Lower) => (
                previous.lower_frames,
                previous.lower_fraction,
                current.lower_frames,
                current.lower_fraction,
                lower_blend_fraction,
            ),
            None => {
                return Err(ModelError::invalid(
                    bone,
                    "bone is absent from track partition",
                ));
            }
        };
    let sample = |frames: (usize, usize), fraction: f32| {
        let first = animation
            .bone_matrix(frames.0, bone)
            .ok_or_else(|| ModelError::invalid(bone, "missing compressed bone matrix"))?;
        let second = animation
            .bone_matrix(frames.1, bone)
            .ok_or_else(|| ModelError::invalid(bone, "missing compressed bone matrix"))?;
        Ok::<_, ModelError>(interpolate_3x4(first, second, fraction.clamp(0.0, 1.0)))
    };
    let outgoing = sample(previous_frames, previous_fraction)?;
    let incoming = sample(current_frames, current_fraction)?;
    let local = interpolate_3x4(outgoing, incoming, blend_fraction);
    local_matrices[bone] = local;
    matrices[bone] = if let Some(parent) = animation.bones[bone].parent {
        multiply_3x4(
            evaluate_bone(
                animation,
                partition,
                previous,
                current,
                lower_blend_fraction,
                upper_blend_fraction,
                parent,
                local_matrices,
                matrices,
                states,
            )?,
            local,
        )
    } else {
        local
    };
    states[bone] = 2;
    Ok(matrices[bone])
}
