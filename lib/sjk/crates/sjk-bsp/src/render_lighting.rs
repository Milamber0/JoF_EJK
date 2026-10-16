//! File-sized lightmap, light-grid and visibility decoding.
use super::*;

/// Copy only validated, complete RGB lightmap tiles from the file.
pub(super) fn parse_lightmaps(data: &[u8], lump: Lump) -> Result<Box<[u8]>, BspError> {
    if !lump.length.is_multiple_of(LIGHTMAP_BYTES) {
        return Err(BspError::MisalignedLump {
            lump: LumpKind::Lightmaps,
            length: lump.length,
            record_bytes: LIGHTMAP_BYTES,
        });
    }
    Ok(lump_bytes(data, lump)?.to_vec().into_boxed_slice())
}

/// Surface lightmap indices must refer to stored tiles or a format sentinel.
pub(super) fn validate_surface_lightmaps(
    surfaces: &[Surface],
    lightmap_count: usize,
) -> Result<(), BspError> {
    for (surface_index, surface) in surfaces.iter().enumerate() {
        for lightmap in surface.lightmaps {
            if lightmap >= 0 && lightmap as usize >= lightmap_count {
                return Err(BspError::InvalidReference {
                    owner: LumpKind::Surfaces,
                    index: surface_index,
                    target: lightmap,
                    target_count: lightmap_count,
                });
            }
            if lightmap < -5 {
                return Err(BspError::InvalidReference {
                    owner: LumpKind::Surfaces,
                    index: surface_index,
                    target: lightmap,
                    target_count: lightmap_count,
                });
            }
        }
    }
    Ok(())
}

/// Decode one sample per complete on-disk record.
pub(super) fn parse_light_grid(data: &[u8], lump: Lump) -> Result<Vec<LightGridSample>, BspError> {
    records::<LightGridSample>(data, lump, LumpKind::LightGrid, LIGHT_GRID_SAMPLE_BYTES)?
        .map(|record| {
            Ok(LightGridSample {
                ambient: std::array::from_fn(|style| {
                    record[style * 3..style * 3 + 3]
                        .try_into()
                        .expect("fixed light sample")
                }),
                directed: std::array::from_fn(|style| {
                    record[12 + style * 3..15 + style * 3]
                        .try_into()
                        .expect("fixed light sample")
                }),
                styles: record[24..28].try_into().expect("fixed style sample"),
                latitude_longitude: record[28..30].try_into().expect("fixed direction sample"),
            })
        })
        .collect()
}

/// Decode format-sized references, checking each against the actual sample count.
pub(super) fn parse_light_grid_array(
    data: &[u8],
    lump: Lump,
    light_grid_count: usize,
) -> Result<Vec<u16>, BspError> {
    records::<u16>(data, lump, LumpKind::LightArray, 2)?
        .enumerate()
        .map(|(index, record)| {
            let value = u16::from_le_bytes(record.try_into().expect("fixed light array record"));
            if value as usize >= light_grid_count {
                return Err(BspError::InvalidReference {
                    owner: LumpKind::LightArray,
                    index,
                    target: i32::from(value),
                    target_count: light_grid_count,
                });
            }
            Ok(value)
        })
        .collect()
}

/// Validate dimensions against the stored bytes before copying any PVS data.
pub(super) fn parse_visibility(data: &[u8], lump: Lump) -> Result<Option<Visibility>, BspError> {
    if lump.length == 0 {
        return Ok(None);
    }
    if lump.length < 8 {
        return Err(BspError::InvalidVisibility {
            length: lump.length,
            cluster_count: 0,
            bytes_per_cluster: 0,
        });
    }
    let bytes = lump_bytes(data, lump)?;
    let cluster_count = usize::try_from(read_i32(bytes, 0)).unwrap_or(usize::MAX);
    let bytes_per_cluster = usize::try_from(read_i32(bytes, 4)).unwrap_or(usize::MAX);
    if read_i32(bytes, 0) < 0
        || read_i32(bytes, 4) < 0
        || visibility_size(cluster_count, bytes_per_cluster) != Some(lump.length)
    {
        return Err(BspError::InvalidVisibility {
            length: lump.length,
            cluster_count,
            bytes_per_cluster,
        });
    }
    Ok(Some(Visibility {
        cluster_count,
        bytes_per_cluster,
        data: bytes[8..].to_vec().into_boxed_slice(),
    }))
}

fn visibility_size(clusters: usize, row_bytes: usize) -> Option<usize> {
    if row_bytes < clusters.div_ceil(8) {
        return None;
    }
    clusters.checked_mul(row_bytes)?.checked_add(8)
}
