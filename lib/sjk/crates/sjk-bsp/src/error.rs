//! BSP parse errors.
use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BspError {
    HeaderTooShort {
        actual: usize,
        minimum: usize,
    },
    InvalidMagic([u8; 4]),
    UnsupportedVersion(i32),
    NegativeLump {
        lump: LumpKind,
        offset: i32,
        length: i32,
    },
    LumpOutOfBounds {
        lump: LumpKind,
        offset: usize,
        length: usize,
        file_bytes: usize,
    },
    MisalignedLump {
        lump: LumpKind,
        length: usize,
        record_bytes: usize,
    },
    EmptyRequiredLump(LumpKind),
    NonFinitePlane(usize),
    InvalidReference {
        owner: LumpKind,
        index: usize,
        target: i32,
        target_count: usize,
    },
    InvalidRange {
        owner: LumpKind,
        index: usize,
        first: i32,
        count: i32,
        target_count: usize,
    },
    NodeCycle(usize),
    InvalidBrushSideCount {
        index: usize,
        actual: i32,
    },
    NonFiniteRenderValue {
        lump: LumpKind,
        index: usize,
    },
    InvalidSurfaceType {
        index: usize,
        actual: i32,
    },
    InvalidTriangleIndexCount {
        surface: usize,
        actual: usize,
    },
    InvalidDrawIndex {
        surface: usize,
        index: usize,
        value: u32,
        vertex_count: usize,
    },
    InvalidPatchDimensions {
        surface: usize,
        width: i32,
        height: i32,
        vertex_count: usize,
    },
    InvalidVisibility {
        length: usize,
        cluster_count: usize,
        bytes_per_cluster: usize,
    },
    /// A bounded collision-patch generator rejected excessive geometry.
    PatchCollision {
        surface: usize,
        reason: &'static str,
    },
    InternalValidatedRangeFailure,
}

impl fmt::Display for BspError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderTooShort { actual, minimum } => {
                write!(
                    formatter,
                    "RBSP header is {actual} bytes; need at least {minimum}"
                )
            }
            Self::InvalidMagic(magic) => write!(formatter, "invalid RBSP magic {magic:?}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported RBSP version {version}")
            }
            Self::NegativeLump {
                lump,
                offset,
                length,
            } => write!(
                formatter,
                "{lump:?} lump has negative offset/length {offset}/{length}"
            ),
            Self::LumpOutOfBounds {
                lump,
                offset,
                length,
                file_bytes,
            } => write!(
                formatter,
                "{lump:?} lump at {offset}+{length} exceeds {file_bytes}-byte file"
            ),
            Self::MisalignedLump {
                lump,
                length,
                record_bytes,
            } => write!(
                formatter,
                "{lump:?} lump length {length} is not divisible by {record_bytes}"
            ),
            Self::EmptyRequiredLump(lump) => write!(formatter, "RBSP has no {lump:?} records"),
            Self::NonFinitePlane(index) => write!(formatter, "plane {index} is not finite"),
            Self::InvalidReference {
                owner,
                index,
                target,
                target_count,
            } => write!(
                formatter,
                "{owner:?} record {index} references {target}, outside 0..{target_count}"
            ),
            Self::InvalidRange {
                owner,
                index,
                first,
                count,
                target_count,
            } => write!(
                formatter,
                "{owner:?} record {index} range {first}+{count} exceeds 0..{target_count}"
            ),
            Self::NodeCycle(index) => {
                write!(formatter, "RBSP node graph cycles through node {index}")
            }
            Self::InvalidBrushSideCount { index, actual } => write!(
                formatter,
                "brush {index} has {actual} sides; collision brushes require at least 6"
            ),
            Self::NonFiniteRenderValue { lump, index } => {
                write!(
                    formatter,
                    "{lump:?} record {index} contains a non-finite float"
                )
            }
            Self::InvalidSurfaceType { index, actual } => {
                write!(formatter, "surface {index} has invalid type {actual}")
            }
            Self::InvalidTriangleIndexCount { surface, actual } => write!(
                formatter,
                "surface {surface} has {actual} triangle indices, which is not divisible by 3"
            ),
            Self::InvalidDrawIndex {
                surface,
                index,
                value,
                vertex_count,
            } => write!(
                formatter,
                "surface {surface} index {index} references vertex {value}, \
                 outside 0..{vertex_count}"
            ),
            Self::InvalidPatchDimensions {
                surface,
                width,
                height,
                vertex_count,
            } => write!(
                formatter,
                "patch {surface} dimensions {width}x{height} do not match \
                 {vertex_count} control points"
            ),
            Self::InvalidVisibility {
                length,
                cluster_count,
                bytes_per_cluster,
            } => write!(
                formatter,
                "invalid visibility layout: {length} bytes, \
                 {cluster_count} clusters x {bytes_per_cluster} bytes per cluster"
            ),
            Self::InternalValidatedRangeFailure => {
                formatter.write_str("internally validated RBSP range became invalid")
            }
            Self::PatchCollision { surface, reason } => {
                write!(formatter, "patch {surface}: {reason}")
            }
        }
    }
}

impl Error for BspError {}
