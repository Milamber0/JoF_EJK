//! Safe, owned loading for legacy Jedi Academy render models.

mod bone_angles;
mod bone_override;
mod bone_override_timing;
pub mod g2_collision;
pub mod gltf;
mod md3_lerp;
pub mod posed_trace;
mod reskin;
mod skeleton_pose;
mod skin;

#[cfg(test)]
mod animation_config_tests;
#[cfg(test)]
mod glm_tolerance_tests;

pub use bone_angles::{BoneAngleCommand, BoneAngleMode, BoneAxis};
pub use bone_override::{
    BoneAnimationCommand, BoneFrameSample, BoneOverridePose, OverrideEndBehavior,
};
pub use skeleton_pose::{BoneTrack, BoneTrackPartition, PoseScratch};
pub use skin::{SKIN_SHADER_OFF, Skin};

use std::collections::HashMap;
use std::error::Error;
use std::f32::consts::TAU;
use std::fmt;

const MD3_HEADER_BYTES: usize = 108;
const MD3_FRAME_BYTES: usize = 56;
const MD3_TAG_BYTES: usize = 112;
const MD3_SURFACE_HEADER_BYTES: usize = 108;
const MD3_SHADER_BYTES: usize = 68;
const MD3_TRIANGLE_BYTES: usize = 12;
const MD3_TEXCOORD_BYTES: usize = 8;
const MD3_VERTEX_BYTES: usize = 8;
const MD3_MAX_TRIANGLES: usize = 8192;
const MD3_MAX_VERTICES: usize = 4096;
const MD3_MAX_SHADERS: usize = 256;
const MD3_MAX_FRAMES: usize = 1024;
const MD3_MAX_SURFACES: usize = 64;
const MD3_MAX_TAGS: usize = 16;
const GLM_HEADER_BYTES: usize = 164;
const GLM_HIERARCHY_BYTES: usize = 144;
const GLM_SURFACE_BYTES: usize = 40;
const GLM_VERTEX_BYTES: usize = 32;
const GLM_MAX_BONES: usize = 1024;
const GLM_MAX_LODS: usize = 32;
const GLM_MAX_SURFACES: usize = 1024;
const GLM_MAX_VERTICES: usize = 65_536;
const GLM_MAX_TRIANGLES: usize = 1_048_576;
const GLA_HEADER_BYTES: usize = 100;
const GLA_SKELETON_BYTES: usize = 172;
const GLA_COMPRESSED_BONE_BYTES: usize = 14;
const GLA_MAX_FRAMES: usize = 1_000_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationConfig {
    sequences: HashMap<String, AnimationSequence>,
    ordered_sequences: Vec<AnimationSequence>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationSequence {
    pub name: String,
    pub first_frame: usize,
    pub frame_count: usize,
    pub loop_frame: i32,
    pub frames_per_second: f32,
}

impl AnimationSequence {
    /// Frame 0 held, for a model whose table names no animation at all:
    /// `G2_TransformBone` (`tr_ghoul2.cpp`) poses a bone that no animation was
    /// set on at frame 0.
    pub const REST: Self = Self {
        name: String::new(),
        first_frame: 0,
        frame_count: 1,
        loop_frame: -1,
        frames_per_second: 20.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitAnimationSample {
    pub lower_frames: (usize, usize),
    pub lower_fraction: f32,
    pub upper_frames: (usize, usize),
    pub upper_fraction: f32,
}

impl AnimationConfig {
    pub fn parse(bytes: &[u8]) -> Result<Self, ModelError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| ModelError::invalid(0, "animation config is not UTF-8"))?;
        let mut sequences = HashMap::new();
        let mut ordered_sequences = Vec::new();
        for (line_index, raw_line) in text.lines().enumerate() {
            let line = raw_line
                .split_once("//")
                .map_or(raw_line, |(content, _)| content)
                .trim();
            if line.is_empty() {
                continue;
            }
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            // BG_ParseAnimationFile skips every token that names no animation, so a
            // line without a name ("0 11 0 30", left by vehicle packs) adds nothing.
            if fields[0].parse::<f64>().is_ok() {
                continue;
            }
            if fields.len() != 5 {
                return Err(ModelError::invalid(
                    line_index,
                    "animation line must have five fields",
                ));
            }
            let parse_integer = |field: &str| {
                field
                    .parse::<i32>()
                    .map_err(|_| ModelError::invalid(line_index, "invalid animation integer field"))
            };
            let first_frame = usize::try_from(parse_integer(fields[1])?)
                .map_err(|_| ModelError::invalid(line_index, "negative first animation frame"))?;
            let frame_count = usize::try_from(parse_integer(fields[2])?)
                .map_err(|_| ModelError::invalid(line_index, "negative animation frame count"))?;
            // BG_ParseAnimationFile (bg_panimate.c) keeps such a line and
            // BG_HasAnimation then reports the animation as absent; creature
            // packs (bomabeast, nexu) list unused sequences this way.
            if frame_count == 0 {
                continue;
            }
            let loop_frame = parse_integer(fields[3])?;
            let frames_per_second = fields[4]
                .parse::<f32>()
                .map_err(|_| ModelError::invalid(line_index, "invalid animation frame speed"))?;
            if !frames_per_second.is_finite() || frames_per_second == 0.0 {
                return Err(ModelError::invalid(
                    line_index,
                    "zero/non-finite animation speed",
                ));
            }
            let name = fields[0].to_ascii_uppercase();
            let sequence = AnimationSequence {
                name,
                first_frame,
                frame_count,
                loop_frame,
                frames_per_second,
            };
            ordered_sequences.push(sequence.clone());
            sequences.insert(sequence.name.clone(), sequence);
        }
        Ok(Self {
            sequences,
            ordered_sequences,
        })
    }

    pub fn get(&self, name: &str) -> Option<&AnimationSequence> {
        self.sequences.get(&name.to_ascii_uppercase())
    }

    /// Look up an already-uppercase animation name without allocating.
    pub fn get_exact(&self, name: &str) -> Option<&AnimationSequence> {
        self.sequences.get(name)
    }

    pub fn get_by_index(&self, index: usize) -> Option<&AnimationSequence> {
        self.ordered_sequences.get(index)
    }

    pub fn len(&self) -> usize {
        self.sequences.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gla {
    pub name: String,
    pub scale: f32,
    pub bones: Vec<GlaBone>,
    /// Compressed-bone pool index for every bone in every frame.
    pub frames: Vec<Vec<u32>>,
    pub compressed_bones: Vec<[u8; GLA_COMPRESSED_BONE_BYTES]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlaBone {
    pub name: String,
    pub flags: u32,
    pub parent: Option<usize>,
    pub base_pose: [[f32; 4]; 3],
    pub inverse_base_pose: [[f32; 4]; 3],
    pub children: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinnedSurface {
    pub name: String,
    pub shader: Option<String>,
    pub vertices: Vec<SkinnedVertex>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub texture_coordinates: [f32; 2],
}

impl Gla {
    pub fn parse(data: &[u8]) -> Result<Self, ModelError> {
        let reader = Reader { data };
        if reader.bytes(0, 4, "GLA identifier")? != b"2LGA" {
            return Err(ModelError::invalid(0, "expected GLA identifier 2LGA"));
        }
        if reader.i32(4, "GLA version")? != 6 {
            return Err(ModelError::invalid(4, "unsupported GLA version"));
        }
        let scale = reader.f32(72, "GLA scale")?;
        let frame_count = reader.count(76, GLA_MAX_FRAMES, "GLA frame count")?;
        let frames_offset = reader.offset(80, "GLA frames offset")?;
        let bone_count = reader.count(84, GLM_MAX_BONES, "GLA bone count")?;
        let pool_offset = reader.offset(88, "GLA compressed pool offset")?;
        let skeleton_offset = reader.offset(92, "GLA skeleton offset")?;
        let end = reader.offset(96, "GLA end")?;
        if frame_count == 0 {
            return Err(ModelError::invalid(76, "GLA has no frames"));
        }
        if !(GLA_HEADER_BYTES..=data.len()).contains(&end) {
            return Err(ModelError::invalid(96, "GLA end is outside the file"));
        }
        reader.region(GLA_HEADER_BYTES, bone_count, 4, end, "GLA skeleton offsets")?;
        let frame_indices = frame_count
            .checked_mul(bone_count)
            .ok_or_else(|| ModelError::invalid(76, "GLA frame index count overflow"))?;
        reader.region(frames_offset, frame_indices, 3, end, "GLA frame indices")?;

        let mut bones = Vec::with_capacity(bone_count);
        for bone_index in 0..bone_count {
            let relative_offset = reader.offset(
                GLA_HEADER_BYTES + bone_index * 4,
                "GLA skeleton relative offset",
            )?;
            let offset = GLA_HEADER_BYTES
                .checked_add(relative_offset)
                .ok_or_else(|| ModelError::invalid(GLA_HEADER_BYTES, "skeleton offset overflow"))?;
            if offset < skeleton_offset {
                return Err(ModelError::invalid(
                    offset,
                    "skeleton precedes skeleton region",
                ));
            }
            let child_count = reader.count(offset + 168, bone_count, "bone child count")?;
            reader.region(
                offset + GLA_SKELETON_BYTES,
                child_count,
                4,
                end,
                "bone children",
            )?;
            let children = (0..child_count)
                .map(|child| {
                    reader
                        .index(
                            offset + GLA_SKELETON_BYTES + child * 4,
                            bone_count,
                            "bone child",
                        )
                        .map(|value| usize::try_from(value).expect("u32 fits usize"))
                })
                .collect::<Result<_, _>>()?;
            bones.push(GlaBone {
                name: reader.name(offset, 64, "bone name")?,
                flags: reader.u32(offset + 64, "bone flags")?,
                parent: optional_index(
                    reader.i32(offset + 68, "bone parent")?,
                    bone_count,
                    offset + 68,
                )?,
                base_pose: reader.matrix3x4(offset + 72, "bone base pose")?,
                inverse_base_pose: reader.matrix3x4(offset + 120, "bone inverse base pose")?,
                children,
            });
        }

        let mut maximum_pool_index = 0_u32;
        let frames = (0..frame_count)
            .map(|frame| {
                (0..bone_count)
                    .map(|bone| {
                        let offset = frames_offset + (frame * bone_count + bone) * 3;
                        let bytes = reader.bytes(offset, 3, "GLA frame index")?;
                        let index = u32::from(bytes[0])
                            | (u32::from(bytes[1]) << 8)
                            | (u32::from(bytes[2]) << 16);
                        maximum_pool_index = maximum_pool_index.max(index);
                        Ok(index)
                    })
                    .collect()
            })
            .collect::<Result<_, _>>()?;
        let pool_count = if frame_indices == 0 {
            0
        } else {
            usize::try_from(maximum_pool_index)
                .expect("24-bit pool index fits usize")
                .checked_add(1)
                .ok_or_else(|| ModelError::invalid(pool_offset, "GLA pool count overflow"))?
        };
        reader.region(
            pool_offset,
            pool_count,
            GLA_COMPRESSED_BONE_BYTES,
            end,
            "GLA compressed bone pool",
        )?;
        let compressed_bones = (0..pool_count)
            .map(|index| {
                reader
                    .bytes(
                        pool_offset + index * GLA_COMPRESSED_BONE_BYTES,
                        GLA_COMPRESSED_BONE_BYTES,
                        "compressed bone",
                    )?
                    .try_into()
                    .map_err(|_| ModelError::invalid(pool_offset, "compressed bone size"))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            name: reader.name(8, 64, "GLA name")?,
            scale,
            bones,
            frames,
            compressed_bones,
        })
    }

    pub fn bone_matrix(&self, frame: usize, bone: usize) -> Option<[[f32; 4]; 3]> {
        let pool_index = usize::try_from(*self.frames.get(frame)?.get(bone)?).ok()?;
        self.compressed_bones
            .get(pool_index)
            .map(decompress_quaternion_bone)
    }

    pub fn frame_matrices(&self, frame: usize) -> Result<Vec<[[f32; 4]; 3]>, ModelError> {
        if frame >= self.frames.len() {
            return Err(ModelError::invalid(frame, "GLA frame is out of range"));
        }
        let mut matrices = vec![None; self.bones.len()];
        let mut states = vec![0_u8; self.bones.len()];
        for bone in 0..self.bones.len() {
            evaluate_bone_matrix(self, frame, bone, &mut matrices, &mut states)?;
        }
        Ok(matrices
            .into_iter()
            .map(|matrix| matrix.expect("every requested bone was evaluated"))
            .collect())
    }

    /// Applies the humanoid view-pitch overrides used by JKA's
    /// `BG_G2ClientSpineAngles`. Ghoul2 post-multiplies a base-pose-conjugated
    /// angle matrix and then evaluates every descendant from that result.
    pub fn apply_humanoid_spine_pitch(
        &self,
        matrices: &mut [[[f32; 4]; 3]],
        view_pitch_degrees: f32,
    ) -> Result<(), ModelError> {
        self.apply_humanoid_spine_angles(matrices, view_pitch_degrees, 0.0, 0.0)
    }

    /// Applies JKA's lower-lumbar, upper-lumbar and thoracic view-angle split.
    pub fn apply_humanoid_spine_angles(
        &self,
        matrices: &mut [[[f32; 4]; 3]],
        view_pitch_degrees: f32,
        spine_yaw_degrees: f32,
        view_roll_degrees: f32,
    ) -> Result<(), ModelError> {
        if matrices.len() != self.bones.len() {
            return Err(ModelError::invalid(
                matrices.len(),
                "pose matrix count differs from GLA bone count",
            ));
        }
        let pitch = (view_pitch_degrees + 180.0).rem_euclid(360.0) - 180.0;
        let yaw = (spine_yaw_degrees + 180.0).rem_euclid(360.0) - 180.0;
        let roll = (view_roll_degrees + 180.0).rem_euclid(360.0) - 180.0;
        // BG_G2PlayerAngles halves view pitch before distributing 40/40/20.
        for (name, pitch_fraction, yaw_roll_fraction) in [
            ("lower_lumbar", 0.20_f32, 0.45_f32),
            ("upper_lumbar", 0.20_f32, 0.35_f32),
            ("thoracic", 0.10_f32, 0.20_f32),
        ] {
            let Some(bone) = self
                .bones
                .iter()
                .position(|candidate| candidate.name.eq_ignore_ascii_case(name))
            else {
                continue;
            };
            let rotation = quake_angles_matrix([
                pitch * pitch_fraction,
                yaw * yaw_roll_fraction,
                roll * yaw_roll_fraction,
            ]);
            let override_matrix = multiply_3x4(
                self.bones[bone].base_pose,
                multiply_3x4(rotation, self.bones[bone].inverse_base_pose),
            );
            let old_target = matrices[bone];
            let new_target = multiply_3x4(old_target, override_matrix);
            let delta = multiply_3x4(new_target, inverse_rigid_3x4(old_target));
            for (candidate, matrix) in matrices.iter_mut().enumerate() {
                if is_bone_or_descendant(&self.bones, candidate, bone) {
                    *matrix = multiply_3x4(delta, *matrix);
                }
            }
        }
        Ok(())
    }

    pub fn split_frame_matrices(
        &self,
        lower_frame: usize,
        upper_frame: usize,
        upper_root_name: &str,
    ) -> Result<Vec<[[f32; 4]; 3]>, ModelError> {
        if lower_frame >= self.frames.len() || upper_frame >= self.frames.len() {
            return Err(ModelError::invalid(
                lower_frame.max(upper_frame),
                "GLA frame is out of range",
            ));
        }
        let upper_root = self
            .bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(upper_root_name))
            .ok_or_else(|| ModelError::invalid(0, "GLA upper-body root bone was not found"))?;
        let upper_bones = (0..self.bones.len())
            .map(|bone| {
                let mut current = Some(bone);
                while let Some(index) = current {
                    if index == upper_root {
                        return true;
                    }
                    current = self.bones[index].parent;
                }
                false
            })
            .collect::<Vec<_>>();
        let mut matrices = vec![None; self.bones.len()];
        let mut states = vec![0_u8; self.bones.len()];
        for bone in 0..self.bones.len() {
            evaluate_split_bone_matrix(
                self,
                lower_frame,
                upper_frame,
                &upper_bones,
                bone,
                &mut matrices,
                &mut states,
            )?;
        }
        Ok(matrices
            .into_iter()
            .map(|matrix| matrix.expect("every requested bone was evaluated"))
            .collect())
    }

    /// Builds a split lower/upper-body pose while blending between animation
    /// frames. Legacy GLA clips commonly run at 20 Hz; evaluating the pose at
    /// render time avoids visibly stepping those clips on modern displays.
    pub fn split_interpolated_frame_matrices(
        &self,
        lower_frames: (usize, usize),
        lower_fraction: f32,
        upper_frames: (usize, usize),
        upper_fraction: f32,
        upper_root_name: &str,
    ) -> Result<Vec<[[f32; 4]; 3]>, ModelError> {
        let highest_frame = lower_frames
            .0
            .max(lower_frames.1)
            .max(upper_frames.0)
            .max(upper_frames.1);
        if highest_frame >= self.frames.len() {
            return Err(ModelError::invalid(
                highest_frame,
                "GLA frame is out of range",
            ));
        }
        let upper_root = self
            .bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(upper_root_name))
            .ok_or_else(|| ModelError::invalid(0, "GLA upper-body root bone was not found"))?;
        let upper_bones = (0..self.bones.len())
            .map(|bone| {
                let mut current = Some(bone);
                while let Some(index) = current {
                    if index == upper_root {
                        return true;
                    }
                    current = self.bones[index].parent;
                }
                false
            })
            .collect::<Vec<_>>();
        let mut matrices = vec![None; self.bones.len()];
        let mut states = vec![0_u8; self.bones.len()];
        for bone in 0..self.bones.len() {
            evaluate_interpolated_split_bone_matrix(
                self,
                lower_frames,
                lower_fraction.clamp(0.0, 1.0),
                upper_frames,
                upper_fraction.clamp(0.0, 1.0),
                &upper_bones,
                bone,
                &mut matrices,
                &mut states,
            )?;
        }
        Ok(matrices
            .into_iter()
            .map(|matrix| matrix.expect("every requested bone was evaluated"))
            .collect())
    }

    /// Blends two split-body poses in local bone space before composing the
    /// hierarchy. Ghoul2 performs animation transitions on each bone's local
    /// frame matrix; blending already-composed world matrices makes hands and
    /// attached weapons take an incorrect path during clip changes.
    pub fn split_interpolated_blended_frame_matrices(
        &self,
        previous: SplitAnimationSample,
        current: SplitAnimationSample,
        lower_blend_fraction: f32,
        upper_blend_fraction: f32,
        upper_root_name: &str,
    ) -> Result<Vec<[[f32; 4]; 3]>, ModelError> {
        let upper_root = self
            .bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(upper_root_name))
            .ok_or_else(|| ModelError::invalid(0, "GLA upper-body root bone was not found"))?;
        let partition = BoneTrackPartition::from_upper_subtree(self, upper_root, &[])?;
        let mut scratch = PoseScratch::new(self.bones.len());
        Ok(scratch
            .evaluate(
                self,
                &partition,
                previous,
                current,
                lower_blend_fraction,
                upper_blend_fraction,
            )?
            .to_vec())
    }

    pub fn blend_frame_matrices(
        &self,
        first: &[[[f32; 4]; 3]],
        second: &[[[f32; 4]; 3]],
        fraction: f32,
    ) -> Result<Vec<[[f32; 4]; 3]>, ModelError> {
        if first.len() != self.bones.len() || second.len() != self.bones.len() {
            return Err(ModelError::invalid(
                first.len().max(second.len()),
                "pose matrix count differs from GLA bone count",
            ));
        }
        let fraction = fraction.clamp(0.0, 1.0);
        Ok(first
            .iter()
            .zip(second)
            .map(|(first, second)| interpolate_3x4(*first, *second, fraction))
            .collect())
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_interpolated_split_bone_matrix(
    animation: &Gla,
    lower_frames: (usize, usize),
    lower_fraction: f32,
    upper_frames: (usize, usize),
    upper_fraction: f32,
    upper_bones: &[bool],
    bone: usize,
    matrices: &mut [Option<[[f32; 4]; 3]>],
    states: &mut [u8],
) -> Result<[[f32; 4]; 3], ModelError> {
    if let Some(matrix) = matrices[bone] {
        return Ok(matrix);
    }
    if states[bone] == 1 {
        return Err(ModelError::invalid(bone, "cyclic GLA bone hierarchy"));
    }
    states[bone] = 1;
    let (frames, fraction) = if upper_bones[bone] {
        (upper_frames, upper_fraction)
    } else {
        (lower_frames, lower_fraction)
    };
    let first = animation
        .bone_matrix(frames.0, bone)
        .ok_or_else(|| ModelError::invalid(bone, "missing compressed bone matrix"))?;
    let second = animation
        .bone_matrix(frames.1, bone)
        .ok_or_else(|| ModelError::invalid(bone, "missing compressed bone matrix"))?;
    let local = interpolate_3x4(first, second, fraction);
    let matrix = if let Some(parent) = animation.bones[bone].parent {
        let parent = evaluate_interpolated_split_bone_matrix(
            animation,
            lower_frames,
            lower_fraction,
            upper_frames,
            upper_fraction,
            upper_bones,
            parent,
            matrices,
            states,
        )?;
        multiply_3x4(parent, local)
    } else {
        local
    };
    states[bone] = 2;
    matrices[bone] = Some(matrix);
    Ok(matrix)
}

fn interpolate_3x4(first: [[f32; 4]; 3], second: [[f32; 4]; 3], fraction: f32) -> [[f32; 4]; 3] {
    let mut output = [[0.0; 4]; 3];
    for row in 0..3 {
        for column in 0..4 {
            output[row][column] =
                first[row][column] + (second[row][column] - first[row][column]) * fraction;
        }
    }
    // Ghoul2 interpolates all twelve mdxaBone_t components directly in
    // G2_TransformBone, including during BONE_ANIM_BLEND transitions.
    output
}

fn dot_3(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn normalize_3(vector: &mut [f32; 3]) {
    let length = dot_3(*vector, *vector).sqrt();
    if length > f32::EPSILON {
        for component in vector {
            *component /= length;
        }
    }
}

fn evaluate_split_bone_matrix(
    animation: &Gla,
    lower_frame: usize,
    upper_frame: usize,
    upper_bones: &[bool],
    bone: usize,
    matrices: &mut [Option<[[f32; 4]; 3]>],
    states: &mut [u8],
) -> Result<[[f32; 4]; 3], ModelError> {
    if let Some(matrix) = matrices[bone] {
        return Ok(matrix);
    }
    if states[bone] == 1 {
        return Err(ModelError::invalid(bone, "cyclic GLA bone hierarchy"));
    }
    states[bone] = 1;
    let frame = if upper_bones[bone] {
        upper_frame
    } else {
        lower_frame
    };
    let local = animation
        .bone_matrix(frame, bone)
        .ok_or_else(|| ModelError::invalid(bone, "missing compressed bone matrix"))?;
    let matrix = if let Some(parent) = animation.bones[bone].parent {
        let parent = evaluate_split_bone_matrix(
            animation,
            lower_frame,
            upper_frame,
            upper_bones,
            parent,
            matrices,
            states,
        )?;
        multiply_3x4(parent, local)
    } else {
        local
    };
    states[bone] = 2;
    matrices[bone] = Some(matrix);
    Ok(matrix)
}

fn evaluate_bone_matrix(
    animation: &Gla,
    frame: usize,
    bone: usize,
    matrices: &mut [Option<[[f32; 4]; 3]>],
    states: &mut [u8],
) -> Result<[[f32; 4]; 3], ModelError> {
    if let Some(matrix) = matrices[bone] {
        return Ok(matrix);
    }
    if states[bone] == 1 {
        return Err(ModelError::invalid(bone, "cyclic GLA bone hierarchy"));
    }
    states[bone] = 1;
    let local = animation
        .bone_matrix(frame, bone)
        .ok_or_else(|| ModelError::invalid(bone, "missing compressed bone matrix"))?;
    let matrix = if let Some(parent) = animation.bones[bone].parent {
        let parent = evaluate_bone_matrix(animation, frame, parent, matrices, states)?;
        multiply_3x4(parent, local)
    } else {
        local
    };
    states[bone] = 2;
    matrices[bone] = Some(matrix);
    Ok(matrix)
}

fn multiply_3x4(left: [[f32; 4]; 3], right: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let mut output = [[0.0; 4]; 3];
    for row in 0..3 {
        for column in 0..3 {
            output[row][column] = (0..3)
                .map(|inner| left[row][inner] * right[inner][column])
                .sum();
        }
        output[row][3] = left[row][3]
            + (0..3)
                .map(|inner| left[row][inner] * right[inner][3])
                .sum::<f32>();
    }
    output
}

fn is_bone_or_descendant(bones: &[GlaBone], candidate: usize, ancestor: usize) -> bool {
    let mut current = Some(candidate);
    while let Some(index) = current {
        if index == ancestor {
            return true;
        }
        current = bones[index].parent;
    }
    false
}

fn inverse_rigid_3x4(matrix: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let mut inverse = [[0.0; 4]; 3];
    for row in 0..3 {
        for column in 0..3 {
            inverse[row][column] = matrix[column][row];
        }
        inverse[row][3] = -(0..3)
            .map(|column| inverse[row][column] * matrix[column][3])
            .sum::<f32>();
    }
    inverse
}

fn quake_angles_matrix(angles: [f32; 3]) -> [[f32; 4]; 3] {
    let [pitch, yaw, roll] = angles.map(f32::to_radians);
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();
    let forward = [cp * cy, cp * sy, -sp];
    let left = [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    [
        [forward[0], left[0], up[0], 0.0],
        [forward[1], left[1], up[1], 0.0],
        [forward[2], left[2], up[2], 0.0],
    ]
}

fn decompress_quaternion_bone(compressed: &[u8; GLA_COMPRESSED_BONE_BYTES]) -> [[f32; 4]; 3] {
    let component =
        |index: usize| u16::from_le_bytes([compressed[index * 2], compressed[index * 2 + 1]]);
    let quaternion = [
        f32::from(component(0)) / 16_383.0 - 2.0,
        f32::from(component(1)) / 16_383.0 - 2.0,
        f32::from(component(2)) / 16_383.0 - 2.0,
        f32::from(component(3)) / 16_383.0 - 2.0,
    ];
    let [w, x, y, z] = quaternion;
    let tx = 2.0 * x;
    let ty = 2.0 * y;
    let tz = 2.0 * z;
    let mut matrix = [
        [
            1.0 - (ty * y + tz * z),
            ty * x - tz * w,
            tz * x + ty * w,
            0.0,
        ],
        [
            ty * x + tz * w,
            1.0 - (tx * x + tz * z),
            tz * y - tx * w,
            0.0,
        ],
        [
            tz * x - ty * w,
            tz * y + tx * w,
            1.0 - (tx * x + ty * y),
            0.0,
        ],
    ];
    for (axis, row) in matrix.iter_mut().enumerate() {
        row[3] = f32::from(component(4 + axis)) / 64.0 - 512.0;
    }
    matrix
}

/// Bone count of the original `_humanoid` skeleton (see [`Glm::remap_old_humanoid`]).
const OLD_HUMANOID_BONES: usize = 72;
/// Bone count of the shipped `_humanoid` skeleton.
const NEW_HUMANOID_BONES: usize = 53;
/// rd-vanilla `OldToNewRemapTable` (`codemp/rd-vanilla/tr_ghoul2.cpp`): the
/// 53-bone index for each bone of the original 72-bone `_humanoid` skeleton.
#[rustfmt::skip]
const OLD_TO_NEW_HUMANOID: [usize; OLD_HUMANOID_BONES] = [
    0, 1, 2, 3, 4, 5, 6, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
    22, 23, 24, 25, 26, 27, 28, 29, 29, 34, 35, 35, 30, 31, 31, 32, 33, 33, 32, 33, 33, 34, 35, 35,
    36, 37, 38, 39, 40, 41, 42, 42, 43, 44, 44, 43, 44, 44, 45, 46, 46, 45, 46, 46, 47, 48, 48, 52,
];

#[derive(Clone, Debug, PartialEq)]
pub struct Glm {
    pub name: String,
    pub animation_name: String,
    pub bone_count: usize,
    pub hierarchy: Vec<GlmSurfaceHierarchy>,
    pub lods: Vec<GlmLod>,
}

/// A GLM surface's bolt flag (`G2SURFACEFLAG_ISBOLT`): a tag that places attachments,
/// never drawn.
pub const GLM_SURFACE_BOLT: u32 = 0x1;
/// A GLM surface's off flag (`G2SURFACEFLAG_OFF`): hidden until switched on.
pub const GLM_SURFACE_OFF: u32 = 0x2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlmSurfaceHierarchy {
    pub name: String,
    pub flags: u32,
    pub shader: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlmLod {
    pub surfaces: Vec<GlmSurface>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlmSurface {
    pub hierarchy_index: usize,
    pub vertices: Vec<GlmVertex>,
    pub triangles: Vec<[u32; 3]>,
    pub bone_references: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlmVertex {
    pub normal: [f32; 3],
    pub position: [f32; 3],
    pub texture_coordinates: [f32; 2],
    pub weights: Vec<GlmWeight>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlmWeight {
    /// Index into [`GlmSurface::bone_references`].
    pub bone_reference: usize,
    pub weight: f32,
}

impl Glm {
    pub fn parse(data: &[u8]) -> Result<Self, ModelError> {
        let reader = Reader { data };
        if reader.bytes(0, 4, "GLM identifier")? != b"2LGM" {
            return Err(ModelError::invalid(0, "expected GLM identifier 2LGM"));
        }
        if reader.i32(4, "GLM version")? != 6 {
            return Err(ModelError::invalid(4, "unsupported GLM version"));
        }
        let bone_count = reader.count(140, GLM_MAX_BONES, "GLM bone count")?;
        let lod_count = reader.count(144, GLM_MAX_LODS, "GLM LOD count")?;
        let surface_count = reader.count(152, GLM_MAX_SURFACES, "GLM surface count")?;
        let lods_offset = reader.offset(148, "GLM LOD offset")?;
        let hierarchy_offset = reader.offset(156, "GLM hierarchy offset")?;
        let end = reader.offset(160, "GLM end")?;
        if !(GLM_HEADER_BYTES..=data.len()).contains(&end) {
            return Err(ModelError::invalid(160, "GLM end is outside the file"));
        }
        reader.region(
            GLM_HEADER_BYTES,
            surface_count,
            4,
            end,
            "GLM hierarchy offsets",
        )?;

        let mut hierarchy = Vec::with_capacity(surface_count);
        let mut current_hierarchy = hierarchy_offset;
        for surface_index in 0..surface_count {
            let listed = reader.offset(
                GLM_HEADER_BYTES + surface_index * 4,
                "GLM hierarchy relative offset",
            )?;
            let listed = GLM_HEADER_BYTES.checked_add(listed).ok_or_else(|| {
                ModelError::invalid(GLM_HEADER_BYTES, "hierarchy offset overflow")
            })?;
            if listed != current_hierarchy {
                return Err(ModelError::invalid(
                    GLM_HEADER_BYTES + surface_index * 4,
                    "GLM hierarchy offset disagrees with hierarchy chain",
                ));
            }
            let child_count = reader.count(
                current_hierarchy + 140,
                surface_count,
                "surface child count",
            )?;
            reader.region(
                current_hierarchy + GLM_HIERARCHY_BYTES,
                child_count,
                4,
                end,
                "surface children",
            )?;
            let parent_raw = reader.i32(current_hierarchy + 136, "surface parent")?;
            let parent = optional_index(parent_raw, surface_count, current_hierarchy + 136)?;
            let children = (0..child_count)
                .map(|child| {
                    reader
                        .index(
                            current_hierarchy + GLM_HIERARCHY_BYTES + child * 4,
                            surface_count,
                            "surface child",
                        )
                        .map(|value| usize::try_from(value).expect("u32 fits usize"))
                })
                .collect::<Result<_, _>>()?;
            hierarchy.push(GlmSurfaceHierarchy {
                name: reader.name(current_hierarchy, 64, "surface hierarchy name")?,
                flags: reader.u32(current_hierarchy + 64, "surface flags")?,
                shader: reader.name(current_hierarchy + 68, 64, "surface shader")?,
                parent,
                children,
            });
            current_hierarchy = current_hierarchy
                .checked_add(GLM_HIERARCHY_BYTES + child_count * 4)
                .ok_or_else(|| {
                    ModelError::invalid(current_hierarchy, "hierarchy chain overflow")
                })?;
        }

        let mut lods = Vec::with_capacity(lod_count);
        let mut lod_offset = lods_offset;
        for _ in 0..lod_count {
            let lod_end = relative_end(reader, lod_offset, lod_offset, end, "LOD end")?;
            let offsets_base = lod_offset + 4;
            reader.region(
                offsets_base,
                surface_count,
                4,
                lod_end,
                "LOD surface offsets",
            )?;
            let mut surfaces = Vec::with_capacity(surface_count);
            for hierarchy_index in 0..surface_count {
                let relative_offset = reader.offset(
                    offsets_base + hierarchy_index * 4,
                    "LOD surface relative offset",
                )?;
                let surface_offset =
                    offsets_base.checked_add(relative_offset).ok_or_else(|| {
                        ModelError::invalid(offsets_base, "LOD surface offset overflow")
                    })?;
                surfaces.push(parse_glm_surface(
                    reader,
                    surface_offset,
                    lod_end,
                    hierarchy_index,
                    surface_count,
                    bone_count,
                )?);
            }
            lods.push(GlmLod { surfaces });
            lod_offset = lod_end;
        }

        let mut mesh = Self {
            name: reader.name(8, 64, "GLM name")?,
            animation_name: skeleton_path(reader.name(72, 64, "GLM animation name")?),
            bone_count,
            hierarchy,
            lods,
        };
        mesh.remap_old_humanoid();
        Ok(mesh)
    }

    /// Meshes built for the original 72-bone `_humanoid` skeleton still ship in
    /// many player packs. rd-vanilla's `R_LoadMDXM` recognises them by exactly that
    /// bone count and skeleton name and moves every surface bone reference onto
    /// the 53-bone skeleton through `OldToNewRemapTable` (`tr_ghoul2.cpp`); a
    /// reference outside the old range becomes bone 0. The mesh then skins
    /// against the current `_humanoid.gla` like any other.
    fn remap_old_humanoid(&mut self) {
        if self.bone_count != OLD_HUMANOID_BONES || !self.animation_name.contains("_humanoid") {
            return;
        }
        for surface in self.lods.iter_mut().flat_map(|lod| lod.surfaces.iter_mut()) {
            for bone in &mut surface.bone_references {
                *bone = OLD_TO_NEW_HUMANOID.get(*bone).copied().unwrap_or(0);
            }
        }
        self.bone_count = NEW_HUMANOID_BONES;
    }

    pub fn skin(
        &self,
        animation: &Gla,
        skin: &Skin,
        frame: usize,
        lod: usize,
    ) -> Result<Vec<SkinnedSurface>, ModelError> {
        if self.bone_count != animation.bones.len() {
            return Err(ModelError::invalid(
                140,
                "GLM and GLA bone counts do not match",
            ));
        }
        let matrices = animation.frame_matrices(frame)?;
        self.skin_with_matrices(skin, lod, &matrices)
    }

    pub fn skin_split(
        &self,
        animation: &Gla,
        skin: &Skin,
        lower_frame: usize,
        upper_frame: usize,
        lod: usize,
    ) -> Result<Vec<SkinnedSurface>, ModelError> {
        if self.bone_count != animation.bones.len() {
            return Err(ModelError::invalid(
                140,
                "GLM and GLA bone counts do not match",
            ));
        }
        let matrices = animation.split_frame_matrices(lower_frame, upper_frame, "lower_lumbar")?;
        self.skin_with_matrices(skin, lod, &matrices)
    }

    pub fn skin_split_interpolated(
        &self,
        animation: &Gla,
        skin: &Skin,
        lower_sample: ((usize, usize), f32),
        upper_sample: ((usize, usize), f32),
        lod: usize,
    ) -> Result<Vec<SkinnedSurface>, ModelError> {
        if self.bone_count != animation.bones.len() {
            return Err(ModelError::invalid(
                140,
                "GLM and GLA bone counts do not match",
            ));
        }
        let matrices = animation.split_interpolated_frame_matrices(
            lower_sample.0,
            lower_sample.1,
            upper_sample.0,
            upper_sample.1,
            "lower_lumbar",
        )?;
        self.skin_with_matrices(skin, lod, &matrices)
    }

    pub fn skin_pose_matrices(
        &self,
        skin: &Skin,
        lod: usize,
        matrices: &[[[f32; 4]; 3]],
    ) -> Result<Vec<SkinnedSurface>, ModelError> {
        if matrices.len() != self.bone_count {
            return Err(ModelError::invalid(
                matrices.len(),
                "pose matrix count differs from GLM bone count",
            ));
        }
        self.skin_with_matrices(skin, lod, matrices)
    }

    /// Evaluates a normal three-vertex Ghoul2 bolt surface in model space.
    /// This is the attachment transform used by player `*r_hand`/`*l_hand`
    /// surfaces and weapon `*blade1` surfaces.
    pub fn surface_bolt_matrix(
        &self,
        name: &str,
        lod: usize,
        matrices: &[[[f32; 4]; 3]],
    ) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
        if matrices.len() != self.bone_count {
            return Err(ModelError::invalid(
                matrices.len(),
                "pose matrix count differs from GLM bone count",
            ));
        }
        let Some(surface_index) = self
            .hierarchy
            .iter()
            .position(|surface| surface.name.eq_ignore_ascii_case(name))
        else {
            return Ok(None);
        };
        let source = self
            .lods
            .get(lod)
            .and_then(|lod| lod.surfaces.get(surface_index))
            .ok_or_else(|| ModelError::invalid(surface_index, "bolt surface is absent from LOD"))?;
        if source.vertices.len() < 3 {
            return Err(ModelError::invalid(
                surface_index,
                "bolt surface has fewer than three vertices",
            ));
        }
        let skinned_position = |vertex: &GlmVertex| {
            let mut position = [0.0; 3];
            for weight in &vertex.weights {
                let global_bone = source.bone_references[weight.bone_reference];
                let matrix = matrices[global_bone];
                for axis in 0..3 {
                    position[axis] += weight.weight
                        * (matrix[axis][3]
                            + (0..3)
                                .map(|component| {
                                    matrix[axis][component] * vertex.position[component]
                                })
                                .sum::<f32>());
                }
            }
            position
        };
        let points = [
            skinned_position(&source.vertices[0]),
            skinned_position(&source.vertices[1]),
            skinned_position(&source.vertices[2]),
        ];
        let mut longest = subtract_3(points[1], points[0]);
        let mut shortest = subtract_3(points[0], points[2]);
        normalize_3(&mut longest);
        normalize_3(&mut shortest);
        let projection = dot_3(longest, shortest);
        for axis in 0..3 {
            longest[axis] -= projection * shortest[axis];
        }
        normalize_3(&mut longest);
        let mut normal = cross_3(
            subtract_3(points[1], points[0]),
            subtract_3(points[0], points[2]),
        );
        normalize_3(&mut normal);
        if dot_3(longest, longest) < 0.5
            || dot_3(shortest, shortest) < 0.5
            || dot_3(normal, normal) < 0.5
        {
            return Err(ModelError::invalid(
                surface_index,
                "bolt surface triangle is degenerate",
            ));
        }
        Ok(Some([
            [shortest[0], longest[0], -normal[0], points[2][0]],
            [shortest[1], longest[1], -normal[1], points[2][1]],
            [shortest[2], longest[2], -normal[2], points[2][2]],
        ]))
    }

    /// The bone that carries the origin of the bolt surface `name` (the heaviest
    /// weight on its origin vertex), as an index into the animation's bones.
    pub fn surface_bolt_bone(&self, name: &str, lod: usize) -> Option<usize> {
        let surface_index = self
            .hierarchy
            .iter()
            .position(|surface| surface.name.eq_ignore_ascii_case(name))?;
        let source = self.lods.get(lod)?.surfaces.get(surface_index)?;
        let origin = source.vertices.get(2)?;
        let weight = origin
            .weights
            .iter()
            .max_by(|a, b| a.weight.total_cmp(&b.weight))?;
        source.bone_references.get(weight.bone_reference).copied()
    }

    fn skin_with_matrices(
        &self,
        skin: &Skin,
        lod: usize,
        matrices: &[[[f32; 4]; 3]],
    ) -> Result<Vec<SkinnedSurface>, ModelError> {
        let source_lod = self
            .lods
            .get(lod)
            .ok_or_else(|| ModelError::invalid(lod, "GLM LOD is out of range"))?;
        let mut output = Vec::new();
        for (surface, hierarchy) in source_lod.surfaces.iter().zip(&self.hierarchy) {
            if hierarchy.flags & GLM_SURFACE_OFF != 0 {
                continue;
            }
            let vertices = surface
                .vertices
                .iter()
                .map(|vertex| reskin::vertex(surface, vertex, matrices))
                .collect();
            // A skin entry of `*off` hides the surface (species variants
            // switch heads, torsos and legs this way); no entry keeps the
            // model's own shader. A bolt surface (a tag: `*r_hand`, `*hip_l`)
            // is never drawn, whatever the skin says: Ghoul2 draws a surface
            // only while all its flags are clear (`RenderSurfaces`, rd-vanilla
            // tr_ghoul2.cpp), and a tag carries the bolt flag. Its shader is
            // often `[nomaterial]`, which would draw as a missing image.
            let shader = if hierarchy.flags & GLM_SURFACE_BOLT != 0 {
                None
            } else {
                match skin.shader(&hierarchy.name) {
                    Some(SKIN_SHADER_OFF) => None,
                    Some(shader) => Some(shader.to_owned()),
                    None => (!hierarchy.shader.is_empty()).then(|| hierarchy.shader.clone()),
                }
            };
            output.push(SkinnedSurface {
                name: hierarchy.name.clone(),
                shader,
                vertices,
                triangles: surface.triangles.clone(),
            });
        }
        Ok(output)
    }
}

fn subtract_3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross_3(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn parse_glm_surface(
    reader: Reader<'_>,
    base: usize,
    lod_end: usize,
    expected_hierarchy: usize,
    surface_count: usize,
    bone_count: usize,
) -> Result<GlmSurface, ModelError> {
    reader.bytes(base, GLM_SURFACE_BYTES, "GLM surface header")?;
    let hierarchy_index =
        usize::try_from(reader.index(base + 4, surface_count, "GLM surface hierarchy index")?)
            .expect("u32 fits usize");
    if hierarchy_index != expected_hierarchy {
        return Err(ModelError::invalid(
            base + 4,
            "LOD surface offset points to a different hierarchy index",
        ));
    }
    let vertex_count = reader.count(base + 12, GLM_MAX_VERTICES, "GLM vertex count")?;
    let triangle_count = reader.count(base + 20, GLM_MAX_TRIANGLES, "GLM triangle count")?;
    let bone_reference_count = reader.count(base + 28, bone_count, "GLM bone reference count")?;
    let surface_end = relative_end(reader, base, base + 36, lod_end, "GLM surface end")?;
    let vertices_offset = relative(reader, base, base + 16, surface_end, "GLM vertices")?;
    let triangles_offset = relative(reader, base, base + 24, surface_end, "GLM triangles")?;
    let bone_references_offset =
        relative(reader, base, base + 32, surface_end, "GLM bone references")?;
    reader.region(
        vertices_offset,
        vertex_count,
        GLM_VERTEX_BYTES,
        surface_end,
        "GLM vertices",
    )?;
    let texcoords_offset = vertices_offset
        .checked_add(vertex_count * GLM_VERTEX_BYTES)
        .ok_or_else(|| ModelError::invalid(vertices_offset, "GLM texcoord offset overflow"))?;
    reader.region(
        texcoords_offset,
        vertex_count,
        8,
        surface_end,
        "GLM texture coordinates",
    )?;
    reader.region(
        triangles_offset,
        triangle_count,
        12,
        surface_end,
        "GLM triangles",
    )?;
    reader.region(
        bone_references_offset,
        bone_reference_count,
        4,
        surface_end,
        "GLM bone references",
    )?;

    let bone_references = (0..bone_reference_count)
        .map(|index| {
            reader
                .index(
                    bone_references_offset + index * 4,
                    bone_count,
                    "GLM global bone reference",
                )
                .map(|value| usize::try_from(value).expect("u32 fits usize"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let triangles = (0..triangle_count)
        .map(|index| {
            let offset = triangles_offset + index * 12;
            Ok([
                reader.index(offset, vertex_count, "GLM triangle vertex")?,
                reader.index(offset + 4, vertex_count, "GLM triangle vertex")?,
                reader.index(offset + 8, vertex_count, "GLM triangle vertex")?,
            ])
        })
        .collect::<Result<_, _>>()?;
    let vertices = (0..vertex_count)
        .map(|index| {
            let offset = vertices_offset + index * GLM_VERTEX_BYTES;
            let packed = reader.u32(offset + 24, "GLM packed weights")?;
            let weight_count = usize::try_from((packed >> 30) + 1).expect("two bits fit usize");
            let mut weights = Vec::with_capacity(weight_count);
            let mut accumulated = 0.0;
            for weight_index in 0..weight_count {
                let bone_reference = usize::try_from((packed >> (weight_index * 5)) & 31)
                    .expect("five bits fit usize");
                if bone_reference >= bone_reference_count {
                    return Err(ModelError::invalid(
                        offset + 24,
                        "vertex weight references absent surface bone",
                    ));
                }
                // The last weight is whatever the others leave, below zero when
                // they add up to more than one; G2_GetVertBoneWeight uses it as is.
                let weight = if weight_index + 1 == weight_count {
                    1.0 - accumulated
                } else {
                    let low =
                        u32::from(reader.bytes(offset + 28 + weight_index, 1, "GLM weight")?[0]);
                    let high = (packed >> (12 + weight_index * 2)) & 0x300;
                    let weight = (low | high) as f32 / 1023.0;
                    accumulated += weight;
                    weight
                };
                weights.push(GlmWeight {
                    bone_reference,
                    weight,
                });
            }
            Ok(GlmVertex {
                normal: reader.f32x3(offset, "GLM vertex normal")?,
                position: reader.f32x3(offset + 12, "GLM vertex position")?,
                texture_coordinates: reader
                    .f32x2(texcoords_offset + index * 8, "GLM texture coordinate")?,
                weights,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(GlmSurface {
        hierarchy_index,
        vertices,
        triangles,
        bone_references,
    })
}

/// A GLM's skeleton name as a file path. `R_LoadMDXM` registers `<name>.gla`
/// through the filesystem, which drops one leading slash (`FS_FOpenFileRead`,
/// `files.cpp`): several vehicle packs name `/models/players/<x>/<x>`.
fn skeleton_path(name: String) -> String {
    match name.strip_prefix(['/', '\\']) {
        Some(relative) => relative.to_owned(),
        None => name,
    }
}

fn optional_index(raw: i32, count: usize, offset: usize) -> Result<Option<usize>, ModelError> {
    if raw == -1 {
        return Ok(None);
    }
    let index = usize::try_from(raw)
        .map_err(|_| ModelError::invalid(offset, "negative hierarchy index"))?;
    if index < count {
        Ok(Some(index))
    } else {
        Err(ModelError::invalid(offset, "hierarchy index out of range"))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Md3 {
    pub name: String,
    pub frames: Vec<Md3Frame>,
    pub tags: Vec<Vec<Md3Tag>>,
    pub surfaces: Vec<Md3Surface>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Md3Frame {
    pub minimums: [f32; 3],
    pub maximums: [f32; 3],
    pub local_origin: [f32; 3],
    pub radius: f32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Md3Tag {
    pub name: String,
    pub origin: [f32; 3],
    pub axes: [[f32; 3]; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Md3Surface {
    pub name: String,
    pub shaders: Vec<String>,
    pub texture_coordinates: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
    pub frames: Vec<Vec<Md3Vertex>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Md3Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl Md3 {
    pub fn parse(data: &[u8]) -> Result<Self, ModelError> {
        let reader = Reader { data };
        if reader.bytes(0, 4, "MD3 identifier")? != b"IDP3" {
            return Err(ModelError::invalid(0, "expected MD3 identifier IDP3"));
        }
        if reader.i32(4, "MD3 version")? != 15 {
            return Err(ModelError::invalid(4, "unsupported MD3 version"));
        }
        let frame_count = reader.count(76, MD3_MAX_FRAMES, "frame count")?;
        let tag_count = reader.count(80, MD3_MAX_TAGS, "tag count")?;
        let surface_count = reader.count(84, MD3_MAX_SURFACES, "surface count")?;
        if frame_count == 0 {
            return Err(ModelError::invalid(76, "MD3 has no frames"));
        }
        let end = reader.offset(104, "model end")?;
        if !(MD3_HEADER_BYTES..=data.len()).contains(&end) {
            return Err(ModelError::invalid(104, "model end is outside the file"));
        }
        let frames_offset = reader.offset(92, "frames offset")?;
        let tags_offset = reader.offset(96, "tags offset")?;
        let surfaces_offset = reader.offset(100, "surfaces offset")?;
        reader.region(frames_offset, frame_count, MD3_FRAME_BYTES, end, "frames")?;
        reader.region(
            tags_offset,
            frame_count
                .checked_mul(tag_count)
                .ok_or_else(|| ModelError::invalid(80, "tag count overflow"))?,
            MD3_TAG_BYTES,
            end,
            "tags",
        )?;

        let frames = (0..frame_count)
            .map(|index| parse_frame(reader, frames_offset + index * MD3_FRAME_BYTES))
            .collect::<Result<_, _>>()?;
        let tags = (0..frame_count)
            .map(|frame| {
                (0..tag_count)
                    .map(|tag| {
                        parse_tag(
                            reader,
                            tags_offset + (frame * tag_count + tag) * MD3_TAG_BYTES,
                        )
                    })
                    .collect()
            })
            .collect::<Result<_, _>>()?;

        let mut surfaces = Vec::with_capacity(surface_count);
        let mut surface_offset = surfaces_offset;
        for _ in 0..surface_count {
            let (surface, next) = parse_surface(reader, surface_offset, end, frame_count)?;
            surfaces.push(surface);
            surface_offset = next;
        }
        if surface_offset > end {
            return Err(ModelError::invalid(
                surface_offset,
                "surface chain exceeds model",
            ));
        }

        Ok(Self {
            name: reader.name(8, 64, "model name")?,
            frames,
            tags,
            surfaces,
        })
    }
}

fn parse_frame(reader: Reader<'_>, offset: usize) -> Result<Md3Frame, ModelError> {
    let minimums = reader.f32x3(offset, "frame minimums")?;
    let maximums = reader.f32x3(offset + 12, "frame maximums")?;
    let local_origin = reader.f32x3(offset + 24, "frame origin")?;
    let radius = reader.f32(offset + 36, "frame radius")?;
    if radius < 0.0 {
        return Err(ModelError::invalid(offset + 36, "negative frame radius"));
    }
    Ok(Md3Frame {
        minimums,
        maximums,
        local_origin,
        radius,
        name: reader.name(offset + 40, 16, "frame name")?,
    })
}

fn parse_tag(reader: Reader<'_>, offset: usize) -> Result<Md3Tag, ModelError> {
    Ok(Md3Tag {
        name: reader.name(offset, 64, "tag name")?,
        origin: reader.f32x3(offset + 64, "tag origin")?,
        axes: [
            reader.f32x3(offset + 76, "tag axis")?,
            reader.f32x3(offset + 88, "tag axis")?,
            reader.f32x3(offset + 100, "tag axis")?,
        ],
    })
}

fn parse_surface(
    reader: Reader<'_>,
    base: usize,
    model_end: usize,
    model_frame_count: usize,
) -> Result<(Md3Surface, usize), ModelError> {
    if reader.bytes(base, 4, "surface identifier")? != b"IDP3" {
        return Err(ModelError::invalid(base, "invalid MD3 surface identifier"));
    }
    let frame_count = reader.count(base + 72, MD3_MAX_FRAMES, "surface frame count")?;
    if frame_count != model_frame_count {
        return Err(ModelError::invalid(
            base + 72,
            "surface frame count differs from model",
        ));
    }
    let shader_count = reader.count(base + 76, MD3_MAX_SHADERS, "shader count")?;
    let vertex_count = reader.count(base + 80, MD3_MAX_VERTICES, "vertex count")?;
    let triangle_count = reader.count(base + 84, MD3_MAX_TRIANGLES, "triangle count")?;
    let surface_end = relative_end(reader, base, base + 104, model_end, "surface end")?;
    if surface_end < base + MD3_SURFACE_HEADER_BYTES {
        return Err(ModelError::invalid(
            base + 104,
            "surface is smaller than its header",
        ));
    }

    let triangles_offset = relative(reader, base, base + 88, surface_end, "triangles offset")?;
    let shaders_offset = relative(reader, base, base + 92, surface_end, "shaders offset")?;
    let texcoords_offset = relative(reader, base, base + 96, surface_end, "texcoords offset")?;
    let vertices_offset = relative(reader, base, base + 100, surface_end, "vertices offset")?;
    reader.region(
        triangles_offset,
        triangle_count,
        MD3_TRIANGLE_BYTES,
        surface_end,
        "triangles",
    )?;
    reader.region(
        shaders_offset,
        shader_count,
        MD3_SHADER_BYTES,
        surface_end,
        "shaders",
    )?;
    reader.region(
        texcoords_offset,
        vertex_count,
        MD3_TEXCOORD_BYTES,
        surface_end,
        "texture coordinates",
    )?;
    reader.region(
        vertices_offset,
        frame_count
            .checked_mul(vertex_count)
            .ok_or_else(|| ModelError::invalid(base + 80, "animated vertex count overflow"))?,
        MD3_VERTEX_BYTES,
        surface_end,
        "animated vertices",
    )?;

    let shaders = (0..shader_count)
        .map(|index| reader.name(shaders_offset + index * MD3_SHADER_BYTES, 64, "shader name"))
        .collect::<Result<_, _>>()?;
    let texture_coordinates = (0..vertex_count)
        .map(|index| reader.f32x2(texcoords_offset + index * MD3_TEXCOORD_BYTES, "texture UV"))
        .collect::<Result<_, _>>()?;
    let triangles = (0..triangle_count)
        .map(|index| {
            let offset = triangles_offset + index * MD3_TRIANGLE_BYTES;
            let triangle = [
                reader.index(offset, vertex_count, "triangle vertex")?,
                reader.index(offset + 4, vertex_count, "triangle vertex")?,
                reader.index(offset + 8, vertex_count, "triangle vertex")?,
            ];
            Ok(triangle)
        })
        .collect::<Result<_, _>>()?;
    let frames = (0..frame_count)
        .map(|frame| {
            (0..vertex_count)
                .map(|vertex| {
                    let offset =
                        vertices_offset + (frame * vertex_count + vertex) * MD3_VERTEX_BYTES;
                    Ok(Md3Vertex {
                        position: [
                            f32::from(reader.i16(offset, "vertex X")?) / 64.0,
                            f32::from(reader.i16(offset + 2, "vertex Y")?) / 64.0,
                            f32::from(reader.i16(offset + 4, "vertex Z")?) / 64.0,
                        ],
                        normal: decode_normal(reader.u16(offset + 6, "vertex normal")?),
                    })
                })
                .collect()
        })
        .collect::<Result<_, _>>()?;

    Ok((
        Md3Surface {
            name: reader.name(base + 4, 64, "surface name")?,
            shaders,
            texture_coordinates,
            triangles,
            frames,
        },
        surface_end,
    ))
}

fn relative(
    reader: Reader<'_>,
    base: usize,
    field: usize,
    end: usize,
    what: &'static str,
) -> Result<usize, ModelError> {
    let offset = reader.offset(field, what)?;
    let absolute = base
        .checked_add(offset)
        .ok_or_else(|| ModelError::invalid(field, "relative offset overflow"))?;
    if absolute > end {
        return Err(ModelError::invalid(
            field,
            "relative offset exceeds surface",
        ));
    }
    Ok(absolute)
}

fn relative_end(
    reader: Reader<'_>,
    base: usize,
    field: usize,
    model_end: usize,
    what: &'static str,
) -> Result<usize, ModelError> {
    let end = relative(reader, base, field, model_end, what)?;
    if end > model_end {
        return Err(ModelError::invalid(field, "surface end exceeds model"));
    }
    Ok(end)
}

fn decode_normal(encoded: u16) -> [f32; 3] {
    let latitude = f32::from(encoded >> 8) * TAU / 255.0;
    let longitude = f32::from(encoded & 0xff) * TAU / 255.0;
    [
        latitude.cos() * longitude.sin(),
        latitude.sin() * longitude.sin(),
        longitude.cos(),
    ]
}

#[derive(Clone, Copy)]
struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    fn bytes(
        self,
        offset: usize,
        length: usize,
        what: &'static str,
    ) -> Result<&'a [u8], ModelError> {
        let end = offset
            .checked_add(length)
            .ok_or_else(|| ModelError::invalid(offset, "byte range overflow"))?;
        self.data
            .get(offset..end)
            .ok_or_else(|| ModelError::invalid(offset, what))
    }

    fn i16(self, offset: usize, what: &'static str) -> Result<i16, ModelError> {
        Ok(i16::from_le_bytes(
            self.bytes(offset, 2, what)?
                .try_into()
                .expect("checked two-byte range"),
        ))
    }

    fn u16(self, offset: usize, what: &'static str) -> Result<u16, ModelError> {
        Ok(u16::from_le_bytes(
            self.bytes(offset, 2, what)?
                .try_into()
                .expect("checked two-byte range"),
        ))
    }

    fn u32(self, offset: usize, what: &'static str) -> Result<u32, ModelError> {
        Ok(u32::from_le_bytes(
            self.bytes(offset, 4, what)?
                .try_into()
                .expect("checked four-byte range"),
        ))
    }

    fn i32(self, offset: usize, what: &'static str) -> Result<i32, ModelError> {
        Ok(i32::from_le_bytes(
            self.bytes(offset, 4, what)?
                .try_into()
                .expect("checked four-byte range"),
        ))
    }

    fn f32(self, offset: usize, what: &'static str) -> Result<f32, ModelError> {
        let value = f32::from_bits(u32::from_le_bytes(
            self.bytes(offset, 4, what)?
                .try_into()
                .expect("checked four-byte range"),
        ));
        if value.is_finite() {
            Ok(value)
        } else {
            Err(ModelError::invalid(offset, "non-finite model float"))
        }
    }

    fn f32x2(self, offset: usize, what: &'static str) -> Result<[f32; 2], ModelError> {
        Ok([self.f32(offset, what)?, self.f32(offset + 4, what)?])
    }

    fn f32x3(self, offset: usize, what: &'static str) -> Result<[f32; 3], ModelError> {
        Ok([
            self.f32(offset, what)?,
            self.f32(offset + 4, what)?,
            self.f32(offset + 8, what)?,
        ])
    }

    fn matrix3x4(self, offset: usize, what: &'static str) -> Result<[[f32; 4]; 3], ModelError> {
        let mut matrix = [[0.0; 4]; 3];
        for (row_index, row) in matrix.iter_mut().enumerate() {
            for (column_index, value) in row.iter_mut().enumerate() {
                *value = self.f32(offset + (row_index * 4 + column_index) * 4, what)?;
            }
        }
        Ok(matrix)
    }

    fn count(self, offset: usize, maximum: usize, what: &'static str) -> Result<usize, ModelError> {
        let count = usize::try_from(self.i32(offset, what)?)
            .map_err(|_| ModelError::invalid(offset, "negative model count"))?;
        if count > maximum {
            Err(ModelError::invalid(
                offset,
                "model count exceeds format limit",
            ))
        } else {
            Ok(count)
        }
    }

    fn offset(self, field: usize, what: &'static str) -> Result<usize, ModelError> {
        usize::try_from(self.i32(field, what)?)
            .map_err(|_| ModelError::invalid(field, "negative model offset"))
    }

    fn index(self, field: usize, count: usize, what: &'static str) -> Result<u32, ModelError> {
        let index = u32::try_from(self.i32(field, what)?)
            .map_err(|_| ModelError::invalid(field, "negative model index"))?;
        if usize::try_from(index).expect("u32 fits usize") < count {
            Ok(index)
        } else {
            Err(ModelError::invalid(field, "model index is out of range"))
        }
    }

    fn name(self, offset: usize, length: usize, what: &'static str) -> Result<String, ModelError> {
        let bytes = self.bytes(offset, length, what)?;
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(length);
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    fn region(
        self,
        offset: usize,
        count: usize,
        stride: usize,
        enclosing_end: usize,
        what: &'static str,
    ) -> Result<(), ModelError> {
        let length = count
            .checked_mul(stride)
            .ok_or_else(|| ModelError::invalid(offset, "model region length overflow"))?;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| ModelError::invalid(offset, "model region end overflow"))?;
        if end > enclosing_end || end > self.data.len() {
            Err(ModelError::invalid(offset, what))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelError {
    pub offset: usize,
    pub message: &'static str,
}

impl ModelError {
    /// Construct a validated-model error for adapter-side semantic checks.
    pub fn invalid(offset: usize, message: &'static str) -> Self {
        Self { offset, message }
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "model error at byte {}: {}",
            self.offset, self.message
        )
    }
}

impl Error for ModelError {}

#[cfg(test)]
mod old_humanoid_tests {
    use super::{Glm, GlmLod, GlmSurface};

    fn mesh(bone_count: usize, animation_name: &str, references: Vec<usize>) -> Glm {
        Glm {
            name: "model".into(),
            animation_name: animation_name.into(),
            bone_count,
            hierarchy: Vec::new(),
            lods: vec![GlmLod {
                surfaces: vec![GlmSurface {
                    hierarchy_index: 0,
                    vertices: Vec::new(),
                    triangles: Vec::new(),
                    bone_references: references,
                }],
            }],
        }
    }

    #[test]
    fn a_72_bone_humanoid_mesh_moves_onto_the_53_bone_skeleton() {
        let mut glm = mesh(
            72,
            "models/players/_humanoid/_humanoid",
            vec![0, 7, 13, 33, 70, 71],
        );
        glm.remap_old_humanoid();
        assert_eq!(glm.bone_count, 53);
        // ltarsal -> ltalus, lower_lumbar, l_d1_j3 -> 48, face_always_ -> 52.
        assert_eq!(
            glm.lods[0].surfaces[0].bone_references,
            [0, 6, 11, 34, 48, 52]
        );
    }

    #[test]
    fn other_meshes_keep_their_references() {
        for (bones, skeleton) in [
            (53, "models/players/_humanoid/_humanoid"),
            (72, "models/players/rancor/rancor"),
        ] {
            let mut glm = mesh(bones, skeleton, vec![0, 7, 13]);
            glm.remap_old_humanoid();
            assert_eq!(glm.bone_count, bones);
            assert_eq!(glm.lods[0].surfaces[0].bone_references, [0, 7, 13]);
        }
    }
}

#[cfg(test)]
mod surface_flag_tests {
    use super::*;

    fn surface(name: &str, flags: u32, shader: &str) -> (GlmSurfaceHierarchy, GlmSurface) {
        let vertex = GlmVertex {
            normal: [0.0, 0.0, 1.0],
            position: [0.0, 0.0, 0.0],
            texture_coordinates: [0.0, 0.0],
            weights: vec![GlmWeight {
                bone_reference: 0,
                weight: 1.0,
            }],
        };
        (
            GlmSurfaceHierarchy {
                name: name.to_owned(),
                flags,
                shader: shader.to_owned(),
                parent: None,
                children: Vec::new(),
            },
            GlmSurface {
                hierarchy_index: 0,
                vertices: vec![vertex.clone(), vertex.clone(), vertex],
                triangles: vec![[0, 1, 2]],
                bone_references: vec![0],
            },
        )
    }

    #[test]
    fn tag_surfaces_are_never_drawn() {
        let parts = [
            surface("torso", 0, "models/players/a/torso"),
            surface("*lchest_l", GLM_SURFACE_BOLT, "[nomaterial]"),
            surface("torso_cap_hips_off", GLM_SURFACE_OFF, "caps"),
            surface("*r_hand", GLM_SURFACE_BOLT, "[nomaterial]"),
        ];
        let mesh = Glm {
            name: "a".into(),
            animation_name: "_humanoid".into(),
            bone_count: 1,
            hierarchy: parts.iter().map(|(h, _)| h.clone()).collect(),
            lods: vec![GlmLod {
                surfaces: parts.iter().map(|(_, s)| s.clone()).collect(),
            }],
        };
        // A skin naming a tag does not draw it either.
        let skin = Skin::parse(
            b"*r_hand,models/players/a/glow
",
        );
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let surfaces = mesh.skin_pose_matrices(&skin, 0, &[identity]).unwrap();
        // The off surface is left out; the tags stay in the list (bolts and surface
        // indices count them) without a shader.
        let shaders: Vec<_> = surfaces
            .iter()
            .map(|s| (s.name.as_str(), s.shader.as_deref()))
            .collect();
        assert_eq!(
            shaders,
            [
                ("torso", Some("models/players/a/torso")),
                ("*lchest_l", None),
                ("*r_hand", None),
            ]
        );
    }
}
