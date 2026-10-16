//! Load-time render records, bounded by actual file slices and cross references.
use super::*;
use crate::Fog;
#[path = "render_lighting.rs"]
mod lighting;
use lighting::*;

/// Decode render records after file-range validation, with no toolchain count ceilings.
pub(crate) fn parse_render_data(
    data: &[u8],
    lumps: &[Lump; crate::HEADER_LUMPS],
    shaders: &[Shader],
    brush_count: usize,
) -> Result<RenderData, BspError> {
    let vertices = parse_vertices(data, lumps[LumpKind::DrawVertices as usize])?;
    let indices = parse_draw_indices(data, lumps[LumpKind::DrawIndices as usize])?;
    let fog_count =
        records::<Fog>(data, lumps[LumpKind::Fogs as usize], LumpKind::Fogs, 72)?.count();
    let surfaces = parse_surfaces(
        data,
        lumps[LumpKind::Surfaces as usize],
        shaders.len(),
        fog_count,
        vertices.len(),
        &indices,
    )?;
    let models = parse_models(
        data,
        lumps[LumpKind::Models as usize],
        surfaces.len(),
        brush_count,
    )?;
    let lightmap_pixels = parse_lightmaps(data, lumps[LumpKind::Lightmaps as usize])?;
    validate_surface_lightmaps(&surfaces, lightmap_pixels.len() / LIGHTMAP_BYTES)?;
    let light_grid = parse_light_grid(data, lumps[LumpKind::LightGrid as usize])?;
    let light_grid_array =
        parse_light_grid_array(data, lumps[LumpKind::LightArray as usize], light_grid.len())?;
    let visibility = parse_visibility(data, lumps[LumpKind::Visibility as usize])?;

    Ok(RenderData {
        models: models.into_boxed_slice(),
        vertices: vertices.into_boxed_slice(),
        indices: indices.into_boxed_slice(),
        surfaces: surfaces.into_boxed_slice(),
        lightmap_pixels,
        light_grid: light_grid.into_boxed_slice(),
        light_grid_array: light_grid_array.into_boxed_slice(),
        visibility,
    })
}

fn parse_models(
    data: &[u8],
    lump: Lump,
    surface_count: usize,
    brush_count: usize,
) -> Result<Vec<Model>, BspError> {
    let models = records::<Model>(data, lump, LumpKind::Models, 40)?
        .enumerate()
        .map(|(index, record)| {
            let minimums = read_f32x3(record, 0);
            let maximums = read_f32x3(record, 12);
            validate_finite(
                LumpKind::Models,
                index,
                minimums.into_iter().chain(maximums),
            )?;
            Ok(Model {
                minimums,
                maximums,
                surfaces: validate_range(
                    LumpKind::Models,
                    index,
                    read_i32(record, 24),
                    read_i32(record, 28),
                    surface_count,
                )?,
                brushes: validate_range(
                    LumpKind::Models,
                    index,
                    read_i32(record, 32),
                    read_i32(record, 36),
                    brush_count,
                )?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if models.is_empty() {
        return Err(BspError::EmptyRequiredLump(LumpKind::Models));
    }
    Ok(models)
}

fn parse_vertices(data: &[u8], lump: Lump) -> Result<Vec<DrawVertex>, BspError> {
    records::<DrawVertex>(data, lump, LumpKind::DrawVertices, DRAW_VERTEX_BYTES)?
        .enumerate()
        .map(|(index, record)| {
            let mut lightmap_coordinates = [[0.0; 2]; 4];
            for (style, coordinates) in lightmap_coordinates.iter_mut().enumerate() {
                *coordinates = read_f32x2(record, 20 + style * 8);
            }
            let vertex = DrawVertex {
                position: read_f32x3(record, 0),
                texture_coordinates: read_f32x2(record, 12),
                lightmap_coordinates,
                normal: read_f32x3(record, 52),
                colors: std::array::from_fn(|style| {
                    record[64 + style * 4..68 + style * 4]
                        .try_into()
                        .expect("fixed color record")
                }),
            };
            validate_finite(
                LumpKind::DrawVertices,
                index,
                vertex
                    .position
                    .into_iter()
                    .chain(vertex.texture_coordinates)
                    .chain(vertex.normal),
            )?;
            Ok(vertex)
        })
        .collect()
}

fn parse_draw_indices(data: &[u8], lump: Lump) -> Result<Vec<u32>, BspError> {
    records::<u32>(data, lump, LumpKind::DrawIndices, 4)?
        .enumerate()
        .map(|(index, record)| {
            u32::try_from(read_i32(record, 0)).map_err(|_| BspError::InvalidReference {
                owner: LumpKind::DrawIndices,
                index,
                target: read_i32(record, 0),
                target_count: 0,
            })
        })
        .collect()
}

fn parse_surfaces(
    data: &[u8],
    lump: Lump,
    shader_count: usize,
    fog_count: usize,
    vertex_count: usize,
    indices: &[u32],
) -> Result<Vec<Surface>, BspError> {
    records::<Surface>(data, lump, LumpKind::Surfaces, SURFACE_BYTES)?
        .enumerate()
        .map(|(surface_index, record)| {
            let shader = validate_index(
                LumpKind::Surfaces,
                surface_index,
                read_i32(record, 0),
                shader_count,
            )?;
            let fog_wire = read_i32(record, 4);
            let fog = if fog_wire == -1 {
                None
            } else {
                Some(validate_index(
                    LumpKind::Surfaces,
                    surface_index,
                    fog_wire,
                    fog_count,
                )?)
            };
            let kind = match read_i32(record, 8) {
                1 => SurfaceKind::Planar,
                2 => SurfaceKind::Patch,
                3 => SurfaceKind::TriangleSoup,
                4 => SurfaceKind::Flare,
                actual => {
                    return Err(BspError::InvalidSurfaceType {
                        index: surface_index,
                        actual,
                    });
                }
            };
            let vertices = validate_range(
                LumpKind::Surfaces,
                surface_index,
                read_i32(record, 12),
                read_i32(record, 16),
                vertex_count,
            )?;
            let surface_indices = validate_range(
                LumpKind::Surfaces,
                surface_index,
                read_i32(record, 20),
                read_i32(record, 24),
                indices.len(),
            )?;
            if matches!(kind, SurfaceKind::Planar | SurfaceKind::TriangleSoup)
                && !surface_indices.len().is_multiple_of(3)
            {
                return Err(BspError::InvalidTriangleIndexCount {
                    surface: surface_index,
                    actual: surface_indices.len(),
                });
            }
            for (relative_index, value) in
                indices[surface_indices.clone()].iter().copied().enumerate()
            {
                if value as usize >= vertices.len() {
                    return Err(BspError::InvalidDrawIndex {
                        surface: surface_index,
                        index: relative_index,
                        value,
                        vertex_count: vertices.len(),
                    });
                }
            }

            let patch_dimensions = if kind == SurfaceKind::Patch {
                let width = read_i32(record, 140);
                let height = read_i32(record, 144);
                let dimensions = usize::try_from(width)
                    .ok()
                    .zip(usize::try_from(height).ok())
                    .filter(|(width, height)| {
                        *width >= 3
                            && *height >= 3
                            && width % 2 == 1
                            && height % 2 == 1
                            && width.checked_mul(*height) == Some(vertices.len())
                    })
                    .ok_or(BspError::InvalidPatchDimensions {
                        surface: surface_index,
                        width,
                        height,
                        vertex_count: vertices.len(),
                    })?;
                Some([dimensions.0, dimensions.1])
            } else {
                None
            };

            let lightmap_origin = read_f32x3(record, 92);
            let lightmap_vectors =
                std::array::from_fn(|vector| read_f32x3(record, 104 + vector * 12));
            let required_surface_values = match kind {
                SurfaceKind::Planar => lightmap_vectors[2].to_vec(),
                SurfaceKind::Patch => lightmap_vectors[..2].concat(),
                SurfaceKind::TriangleSoup => Vec::new(),
                SurfaceKind::Flare => lightmap_origin
                    .into_iter()
                    .chain(lightmap_vectors[0])
                    .chain(lightmap_vectors[2])
                    .collect(),
            };
            validate_finite(
                LumpKind::Surfaces,
                surface_index,
                required_surface_values.into_iter(),
            )?;

            Ok(Surface {
                shader,
                fog,
                kind,
                vertices,
                indices: surface_indices,
                lightmap_styles: record[28..32].try_into().expect("fixed style record"),
                vertex_styles: record[32..36].try_into().expect("fixed style record"),
                lightmaps: std::array::from_fn(|style| read_i32(record, 36 + style * 4)),
                lightmap_rectangles: std::array::from_fn(|style| {
                    [
                        read_i32(record, 52 + style * 4),
                        read_i32(record, 68 + style * 4),
                        read_i32(record, 84),
                        read_i32(record, 88),
                    ]
                }),
                lightmap_origin,
                lightmap_vectors,
                patch_dimensions,
            })
        })
        .collect()
}

fn validate_finite(
    lump: LumpKind,
    index: usize,
    values: impl Iterator<Item = f32>,
) -> Result<(), BspError> {
    if !values.into_iter().all(f32::is_finite) {
        return Err(BspError::NonFiniteRenderValue { lump, index });
    }
    Ok(())
}

fn read_f32x2(data: &[u8], offset: usize) -> [f32; 2] {
    [read_f32(data, offset), read_f32(data, offset + 4)]
}

fn read_f32x3(data: &[u8], offset: usize) -> [f32; 3] {
    [
        read_f32(data, offset),
        read_f32(data, offset + 4),
        read_f32(data, offset + 8),
    ]
}
