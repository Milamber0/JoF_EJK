//! Validated BSP parse helpers.
use super::*;

pub(super) fn parse_header(data: &[u8]) -> Result<[Lump; HEADER_LUMPS], BspError> {
    if data.len() < HEADER_BYTES {
        return Err(BspError::HeaderTooShort {
            actual: data.len(),
            minimum: HEADER_BYTES,
        });
    }
    let magic: [u8; 4] = data[0..4].try_into().expect("fixed slice length");
    if magic != RBSP_MAGIC {
        return Err(BspError::InvalidMagic(magic));
    }
    let version = read_i32(data, 4);
    if version != RBSP_VERSION {
        return Err(BspError::UnsupportedVersion(version));
    }

    let mut lumps = [Lump {
        offset: 0,
        length: 0,
    }; HEADER_LUMPS];
    for (index, kind) in LumpKind::ALL.into_iter().enumerate() {
        let base = 8 + index * 8;
        let wire_offset = read_i32(data, base);
        let wire_length = read_i32(data, base + 4);
        let offset = usize::try_from(wire_offset).map_err(|_| BspError::NegativeLump {
            lump: kind,
            offset: wire_offset,
            length: wire_length,
        })?;
        let length = usize::try_from(wire_length).map_err(|_| BspError::NegativeLump {
            lump: kind,
            offset: wire_offset,
            length: wire_length,
        })?;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= data.len())
            .ok_or(BspError::LumpOutOfBounds {
                lump: kind,
                offset,
                length,
                file_bytes: data.len(),
            })?;
        let _ = end;
        lumps[index] = Lump { offset, length };
    }
    Ok(lumps)
}

pub(super) fn lump_bytes(data: &[u8], lump: Lump) -> Result<&[u8], BspError> {
    let end = lump
        .offset
        .checked_add(lump.length)
        .ok_or(BspError::InternalValidatedRangeFailure)?;
    data.get(lump.offset..end)
        .ok_or(BspError::InternalValidatedRangeFailure)
}

/// Validate the file slice and decoded element layout before any record allocation.
/// T is the persistent decoded type, including conversions from smaller raw records.
pub(super) fn records<T>(
    data: &[u8],
    lump: Lump,
    kind: LumpKind,
    record_bytes: usize,
) -> Result<impl Iterator<Item = &[u8]>, BspError> {
    if record_bytes == 0 || !lump.length.is_multiple_of(record_bytes) {
        return Err(BspError::MisalignedLump {
            lump: kind,
            length: lump.length,
            record_bytes,
        });
    }
    let bytes = lump_bytes(data, lump)?;
    decoded_layout::<T>(bytes.len() / record_bytes)?;
    Ok(bytes.chunks_exact(record_bytes))
}

fn decoded_layout<T>(count: usize) -> Result<(), BspError> {
    count
        .checked_mul(std::mem::size_of::<T>())
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or(BspError::InternalValidatedRangeFailure)?;
    Ok(())
}

pub(super) fn parse_shaders(data: &[u8], lump: Lump) -> Result<Vec<Shader>, BspError> {
    records::<Shader>(data, lump, LumpKind::Shaders, 72)?
        .map(|record| {
            let name_end = record[..64]
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(64);
            Ok(Shader {
                name: record[..name_end].to_vec().into_boxed_slice(),
                surface_flags: read_u32(record, 64),
                content_flags: read_u32(record, 68),
            })
        })
        .collect()
}

pub(super) fn parse_planes(data: &[u8], lump: Lump) -> Result<Vec<Plane>, BspError> {
    records::<Plane>(data, lump, LumpKind::Planes, 16)?
        .enumerate()
        .map(|(index, record)| {
            let plane = Plane {
                normal: [
                    read_f32(record, 0),
                    read_f32(record, 4),
                    read_f32(record, 8),
                ],
                distance: read_f32(record, 12),
            };
            if !plane.normal.into_iter().all(f32::is_finite) || !plane.distance.is_finite() {
                return Err(BspError::NonFinitePlane(index));
            }
            Ok(plane)
        })
        .collect()
}

pub(super) fn parse_nodes(data: &[u8], lump: Lump) -> Result<Vec<RawNode>, BspError> {
    records::<Node>(data, lump, LumpKind::Nodes, 36)?
        .map(|record| {
            Ok(RawNode {
                plane: read_i32(record, 0),
                children: [read_i32(record, 4), read_i32(record, 8)],
                minimums: read_i32x3(record, 12),
                maximums: read_i32x3(record, 24),
            })
        })
        .collect()
}

pub(super) fn parse_leaves(data: &[u8], lump: Lump) -> Result<Vec<RawLeaf>, BspError> {
    records::<Leaf>(data, lump, LumpKind::Leaves, 48)?
        .map(|record| {
            Ok(RawLeaf {
                cluster: read_i32(record, 0),
                area: read_i32(record, 4),
                minimums: read_i32x3(record, 8),
                maximums: read_i32x3(record, 20),
                first_leaf_surface: read_i32(record, 32),
                leaf_surface_count: read_i32(record, 36),
                first_leaf_brush: read_i32(record, 40),
                leaf_brush_count: read_i32(record, 44),
            })
        })
        .collect()
}

pub(super) fn parse_indices(
    data: &[u8],
    lump: Lump,
    kind: LumpKind,
) -> Result<Vec<usize>, BspError> {
    records::<usize>(data, lump, kind, 4)?
        .enumerate()
        .map(|(index, record)| {
            usize::try_from(read_i32(record, 0)).map_err(|_| BspError::InvalidReference {
                owner: kind,
                index,
                target: read_i32(record, 0),
                target_count: 0,
            })
        })
        .collect()
}

pub(super) fn parse_brushes(data: &[u8], lump: Lump) -> Result<Vec<RawBrush>, BspError> {
    records::<Brush>(data, lump, LumpKind::Brushes, 12)?
        .map(|record| {
            Ok(RawBrush {
                first_side: read_i32(record, 0),
                side_count: read_i32(record, 4),
                shader: read_i32(record, 8),
            })
        })
        .collect()
}

pub(super) fn parse_brush_sides(data: &[u8], lump: Lump) -> Result<Vec<BrushSide>, BspError> {
    records::<BrushSide>(data, lump, LumpKind::BrushSides, 12)?
        .map(|record| {
            Ok(BrushSide {
                plane: usize::try_from(read_i32(record, 0)).unwrap_or(usize::MAX),
                shader: usize::try_from(read_i32(record, 4)).unwrap_or(usize::MAX),
                draw_surface: read_i32(record, 8),
            })
        })
        .collect()
}
