//! Engine-generic skeletal angle overrides.
//!
//! An adapter supplies Euler angles, signed axis mappings, and composition
//! mode. The model layer precomputes the base-pose-conjugated override once per
//! update and applies it while walking the hierarchy; it contains no game bone
//! names or policy constants.

use super::{GlaBone, multiply_3x4, quake_angles_matrix};

/// Signed skeleton axis selected for one Euler rotation component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoneAxis {
    NegativeX,
    PositiveX,
    NegativeY,
    PositiveY,
    NegativeZ,
    PositiveZ,
}

/// Point at which an angle override composes with the animated local matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoneAngleMode {
    /// Compose the base-pose-conjugated command after the animated local.
    PostMultiply,
}

/// One adapter-authored bone-angle command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneAngleCommand {
    pub angles_degrees: [f32; 3],
    pub mode: BoneAngleMode,
    pub up: BoneAxis,
    pub left: BoneAxis,
    pub forward: BoneAxis,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BoneAngleOverride {
    pub(super) matrix: [[f32; 4]; 3],
}

pub(super) fn compile(bone: &GlaBone, command: BoneAngleCommand) -> BoneAngleOverride {
    let mapped = map_angles(
        command.angles_degrees,
        command.up,
        command.left,
        command.forward,
    );
    let rotation = quake_angles_matrix(mapped);
    let matrix = multiply_3x4(
        bone.base_pose,
        multiply_3x4(rotation, bone.inverse_base_pose),
    );
    let BoneAngleMode::PostMultiply = command.mode;
    BoneAngleOverride { matrix }
}

fn map_angles(angles: [f32; 3], up: BoneAxis, left: BoneAxis, forward: BoneAxis) -> [f32; 3] {
    let yaw = match up {
        BoneAxis::NegativeX => angles[2] + 180.0,
        BoneAxis::PositiveX => angles[2],
        BoneAxis::NegativeY | BoneAxis::PositiveY => angles[0],
        BoneAxis::NegativeZ => angles[1] + 180.0,
        BoneAxis::PositiveZ => angles[1],
    };
    let pitch = match left {
        BoneAxis::NegativeX => angles[2],
        BoneAxis::PositiveX => angles[2] + 180.0,
        BoneAxis::NegativeY => angles[0],
        BoneAxis::PositiveY => angles[0] + 180.0,
        BoneAxis::NegativeZ | BoneAxis::PositiveZ => angles[1],
    };
    let roll = match forward {
        BoneAxis::NegativeX | BoneAxis::PositiveX => angles[2],
        BoneAxis::NegativeY => angles[0],
        BoneAxis::PositiveY => angles[0] + 180.0,
        BoneAxis::NegativeZ => angles[1],
        BoneAxis::PositiveZ => angles[1] + 180.0,
    };
    [pitch, yaw, roll]
}
