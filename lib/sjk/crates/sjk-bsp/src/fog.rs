//! RBSP fog records and axial brush volumes.
//!
//! Port of codemp/rd-vanilla/tr_bsp.cpp:1658-1795 (`R_LoadFogs`).

use crate::{Brush, Bsp, BspError, Lump, LumpKind, read_i32, records, validate_index};

/// A `dfog_t` (codemp/qcommon/qfiles.h:435-439), using zero-based references.
#[derive(Clone, Debug, PartialEq)]
pub struct Fog {
    /// NUL-terminated shader name from the 64-byte field.
    pub shader: String,
    /// Bounding brush, or `None` for global fog (`brushNum == -1`).
    pub brush: Option<usize>,
    /// Visible side relative to the brush's first side; `None` means a zero plane.
    pub visible_side: Option<usize>,
}

/// Brush-derived spatial data, independent of shader and renderer ownership.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogVolume {
    /// Axial minimum and maximum corners.
    pub bounds: [[f32; 3]; 2],
    /// Inward-facing plane, or `None` for the reference's zero plane.
    pub surface: Option<[f32; 4]>,
}

impl FogVolume {
    /// Derive a validated BSP fog's bounds and visible plane.
    ///
    /// `R_LoadFogs`, codemp/rd-vanilla/tr_bsp.cpp:1724-1792. Global bounds use
    /// `MAX_WORLD_COORD` from codemp/qcommon/q_shared.h:54, contained in this adapter.
    pub fn derive(fog: &Fog, bsp: &Bsp) -> Self {
        let Some(brush) = fog.brush else {
            return Self {
                bounds: [[-65536.0; 3], [65536.0; 3]],
                surface: None,
            };
        };
        let first = bsp.brushes()[brush].sides.start;
        let plane = |side: usize| bsp.planes()[bsp.brush_sides()[first + side].plane];
        let bounds = [
            std::array::from_fn(|axis| -plane(axis * 2).distance),
            std::array::from_fn(|axis| plane(axis * 2 + 1).distance),
        ];
        let surface = fog.visible_side.map(|side| {
            let plane = plane(side);
            [
                -plane.normal[0],
                -plane.normal[1],
                -plane.normal[2],
                -plane.distance,
            ]
        });
        Self { bounds, surface }
    }
}

pub(super) fn parse(data: &[u8], lump: Lump, brushes: &[Brush]) -> Result<Vec<Fog>, BspError> {
    records::<Fog>(data, lump, LumpKind::Fogs, 72)?
        .enumerate()
        .map(|(index, record)| {
            let brush = optional_index(index, read_i32(record, 64), brushes.len())?;
            let side_count = brush.map_or(0, |brush| brushes[brush].sides.len());
            if brush.is_some() && side_count < 6 {
                return Err(BspError::InvalidReference {
                    owner: LumpKind::Fogs,
                    index,
                    target: 5,
                    target_count: side_count,
                });
            }
            let visible_side = optional_index(index, read_i32(record, 68), side_count)?;
            let end = record[..64]
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(64);
            Ok(Fog {
                shader: String::from_utf8_lossy(&record[..end]).into_owned(),
                brush,
                visible_side,
            })
        })
        .collect()
}

fn optional_index(index: usize, value: i32, count: usize) -> Result<Option<usize>, BspError> {
    if value == -1 {
        Ok(None)
    } else {
        validate_index(LumpKind::Fogs, index, value, count).map(Some)
    }
}
