//! Renderer-neutral scene construction from owned compatibility assets.

use sjk_bsp::{Bsp, DrawVertex, Surface, SurfaceKind};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

mod patch_detail;
mod shadow_hulls;
pub use shadow_hulls::ShadowHulls;

const SURFACE_NODRAW: u32 = 0x0020_0000;
const SURFACE_SKY: u32 = 0x0000_2000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MaterialKey {
    pub shader: usize,
    pub lightmap: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct WorldVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub texture_coordinates: [f32; 2],
    pub lightmap_coordinates: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshBatch {
    pub material: MaterialKey,
    pub vertices: Vec<WorldVertex>,
    pub indices: Vec<u32>,
    pub draws: Vec<MeshDraw>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeshDraw {
    pub surface_index: usize,
    pub indices: std::ops::Range<u32>,
    pub clusters: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flare {
    pub shader: usize,
    pub origin: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticWorld {
    batches: Vec<MeshBatch>,
    flares: Vec<Flare>,
    source_surface_count: usize,
    skipped_nodraw_surfaces: usize,
    skipped_sky_surfaces: usize,
}

impl StaticWorld {
    pub fn build(bsp: &Bsp, options: MeshBuildOptions) -> Result<Self, SceneError> {
        if !(1..=32).contains(&options.patch_subdivisions) {
            return Err(SceneError::InvalidPatchSubdivisions(
                options.patch_subdivisions,
            ));
        }

        let render = bsp.render();
        let mut batches = BTreeMap::<MaterialKey, MeshBatch>::new();
        let mut flares = Vec::new();
        let mut skipped_nodraw_surfaces = 0;
        let mut skipped_sky_surfaces = 0;
        let surface_clusters = collect_surface_clusters(bsp);
        let patch_details = patch_detail::build(bsp, usize::from(options.patch_subdivisions));
        for (surface_index, surface) in render.surfaces().iter().enumerate() {
            let surface_flags = bsp.shaders()[surface.shader].surface_flags;
            if surface_flags & SURFACE_NODRAW != 0 {
                skipped_nodraw_surfaces += 1;
                continue;
            }
            if surface_flags & SURFACE_SKY != 0 && !options.include_sky_surfaces {
                skipped_sky_surfaces += 1;
                continue;
            }
            if surface.kind == SurfaceKind::Flare {
                flares.push(Flare {
                    shader: surface.shader,
                    origin: surface.lightmap_origin,
                    color: surface.lightmap_vectors[0],
                    normal: surface.lightmap_vectors[2],
                });
                continue;
            }

            let material = MaterialKey {
                shader: surface.shader,
                lightmap: surface.lightmaps[0],
            };
            let batch = batches.entry(material).or_insert_with(|| MeshBatch {
                material,
                vertices: Vec::new(),
                indices: Vec::new(),
                draws: Vec::new(),
            });
            let index_start = u32::try_from(batch.indices.len())
                .map_err(|_| SceneError::VertexIndexOverflow { surface_index })?;
            match surface.kind {
                SurfaceKind::Planar | SurfaceKind::TriangleSoup => {
                    append_indexed_surface(batch, bsp, surface, surface_index)?;
                }
                SurfaceKind::Patch => {
                    append_patch(
                        batch,
                        bsp,
                        surface,
                        surface_index,
                        patch_details[surface_index]
                            .as_ref()
                            .expect("patch dimensions validated by BSP"),
                    )?;
                }
                SurfaceKind::Flare => unreachable!("handled above"),
            }
            let index_end = u32::try_from(batch.indices.len())
                .map_err(|_| SceneError::VertexIndexOverflow { surface_index })?;
            batch.draws.push(MeshDraw {
                surface_index,
                indices: index_start..index_end,
                clusters: surface_clusters[surface_index].clone(),
            });
        }

        Ok(Self {
            batches: batches.into_values().collect(),
            flares,
            source_surface_count: render.surfaces().len(),
            skipped_nodraw_surfaces,
            skipped_sky_surfaces,
        })
    }

    pub fn batches(&self) -> &[MeshBatch] {
        &self.batches
    }

    pub fn flares(&self) -> &[Flare] {
        &self.flares
    }

    pub fn source_surface_count(&self) -> usize {
        self.source_surface_count
    }

    pub fn skipped_nodraw_surfaces(&self) -> usize {
        self.skipped_nodraw_surfaces
    }

    pub fn skipped_sky_surfaces(&self) -> usize {
        self.skipped_sky_surfaces
    }

    pub fn vertex_count(&self) -> usize {
        self.batches.iter().map(|batch| batch.vertices.len()).sum()
    }

    pub fn triangle_count(&self) -> usize {
        self.batches
            .iter()
            .map(|batch| batch.indices.len() / 3)
            .sum()
    }
}

fn collect_surface_clusters(bsp: &Bsp) -> Vec<Vec<usize>> {
    let mut surface_clusters = vec![Vec::new(); bsp.render().surfaces().len()];
    for (leaf_index, leaf) in bsp.leaves().iter().enumerate() {
        let Ok(cluster) = usize::try_from(leaf.cluster) else {
            continue;
        };
        let Some(surfaces) = bsp.leaf_surface_indices(leaf_index) else {
            continue;
        };
        for &surface in surfaces {
            let clusters = &mut surface_clusters[surface];
            if clusters.last() != Some(&cluster) && !clusters.contains(&cluster) {
                clusters.push(cluster);
            }
        }
    }
    surface_clusters
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshBuildOptions {
    /// Minimum steps per quadratic span; curved spans gain detail up to 32.
    pub patch_subdivisions: u8,
    /// Retain Q3 sky polygons for a renderer-provided sky iterator.
    pub include_sky_surfaces: bool,
}

impl MeshBuildOptions {
    /// Enable sky polygons while retaining the default patch tessellation.
    pub fn with_sky_surfaces(mut self) -> Self {
        self.include_sky_surfaces = true;
        self
    }
}

impl Default for MeshBuildOptions {
    fn default() -> Self {
        Self {
            patch_subdivisions: 4,
            include_sky_surfaces: false,
        }
    }
}

fn append_indexed_surface(
    batch: &mut MeshBatch,
    bsp: &Bsp,
    surface: &Surface,
    surface_index: usize,
) -> Result<(), SceneError> {
    let base = checked_vertex_base(batch, surface.vertices.len(), surface_index)?;
    batch.vertices.extend(
        bsp.render().vertices()[surface.vertices.clone()]
            .iter()
            .map(convert_vertex),
    );
    for index in &bsp.render().indices()[surface.indices.clone()] {
        batch.indices.push(
            base.checked_add(*index)
                .ok_or(SceneError::VertexIndexOverflow { surface_index })?,
        );
    }
    Ok(())
}

fn append_patch(
    batch: &mut MeshBatch,
    bsp: &Bsp,
    surface: &Surface,
    surface_index: usize,
    detail: &patch_detail::Detail,
) -> Result<(), SceneError> {
    let [width, height] = surface
        .patch_dimensions
        .ok_or(SceneError::MissingPatchDimensions { surface_index })?;
    let controls = &bsp.render().vertices()[surface.vertices.clone()];
    for patch_y in (0..height - 2).step_by(2) {
        for patch_x in (0..width - 2).step_by(2) {
            let nx = detail.horizontal[patch_x / 2];
            let ny = detail.vertical[patch_y / 2];
            let generated_vertices = (nx + 1) * (ny + 1);
            let base = checked_vertex_base(batch, generated_vertices, surface_index)?;
            for y in 0..=ny {
                let v = y as f32 / ny as f32;
                for x in 0..=nx {
                    let u = x as f32 / nx as f32;
                    batch.vertices.push(evaluate_patch_vertex(
                        controls, width, patch_x, patch_y, u, v,
                    ));
                }
            }
            let row = nx + 1;
            for y in 0..ny {
                for x in 0..nx {
                    let local = y * row + x;
                    let a = base + u32::try_from(local).expect("bounded patch index");
                    let b = a + 1;
                    let c = a + u32::try_from(row).expect("bounded patch row");
                    let d = c + 1;
                    batch.indices.extend_from_slice(&[a, c, b, b, c, d]);
                }
            }
        }
    }
    Ok(())
}

fn checked_vertex_base(
    batch: &MeshBatch,
    additional: usize,
    surface_index: usize,
) -> Result<u32, SceneError> {
    let end = batch
        .vertices
        .len()
        .checked_add(additional)
        .ok_or(SceneError::VertexIndexOverflow { surface_index })?;
    if end > u32::MAX as usize {
        return Err(SceneError::VertexIndexOverflow { surface_index });
    }
    Ok(batch.vertices.len() as u32)
}

fn convert_vertex(vertex: &DrawVertex) -> WorldVertex {
    WorldVertex {
        position: vertex.position,
        normal: normalized(vertex.normal),
        texture_coordinates: vertex.texture_coordinates,
        lightmap_coordinates: finite_or_zero(vertex.lightmap_coordinates[0]),
        color: vertex.colors[0],
    }
}

fn evaluate_patch_vertex(
    controls: &[DrawVertex],
    width: usize,
    patch_x: usize,
    patch_y: usize,
    u: f32,
    v: f32,
) -> WorldVertex {
    let u_weights = quadratic_weights(u);
    let v_weights = quadratic_weights(v);
    let mut position = [0.0; 3];
    let mut normal = [0.0; 3];
    let mut texture_coordinates = [0.0; 2];
    let mut lightmap_coordinates = [0.0; 2];
    let mut color = [0.0; 4];
    for row in 0..3 {
        for column in 0..3 {
            let weight = u_weights[column] * v_weights[row];
            let control = &controls[(patch_y + row) * width + patch_x + column];
            accumulate(&mut position, control.position, weight);
            accumulate(&mut normal, control.normal, weight);
            accumulate(
                &mut texture_coordinates,
                control.texture_coordinates,
                weight,
            );
            accumulate(
                &mut lightmap_coordinates,
                finite_or_zero(control.lightmap_coordinates[0]),
                weight,
            );
            for (channel, output) in color.iter_mut().enumerate() {
                *output += f32::from(control.colors[0][channel]) * weight;
            }
        }
    }
    WorldVertex {
        position,
        normal: normalized(normal),
        texture_coordinates,
        lightmap_coordinates,
        color: color.map(|channel| channel.round().clamp(0.0, 255.0) as u8),
    }
}

fn quadratic_weights(value: f32) -> [f32; 3] {
    let inverse = 1.0 - value;
    [inverse * inverse, 2.0 * value * inverse, value * value]
}

fn accumulate<const SIZE: usize>(output: &mut [f32; SIZE], input: [f32; SIZE], weight: f32) {
    for index in 0..SIZE {
        output[index] += input[index] * weight;
    }
}

fn normalized(vector: [f32; 3]) -> [f32; 3] {
    let length_squared = vector.into_iter().map(|value| value * value).sum::<f32>();
    if length_squared <= f32::EPSILON {
        return [0.0, 0.0, 1.0];
    }
    let inverse = length_squared.sqrt().recip();
    vector.map(|value| value * inverse)
}

fn finite_or_zero(value: [f32; 2]) -> [f32; 2] {
    value.map(|component| {
        if component.is_finite() {
            component
        } else {
            0.0
        }
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneError {
    InvalidPatchSubdivisions(u8),
    MissingPatchDimensions { surface_index: usize },
    VertexIndexOverflow { surface_index: usize },
}

impl fmt::Display for SceneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPatchSubdivisions(value) => {
                write!(
                    formatter,
                    "patch subdivision count {value} is outside 1..=32"
                )
            }
            Self::MissingPatchDimensions { surface_index } => {
                write!(formatter, "patch surface {surface_index} has no dimensions")
            }
            Self::VertexIndexOverflow { surface_index } => {
                write!(
                    formatter,
                    "surface {surface_index} exceeds the u32 mesh index range"
                )
            }
        }
    }
}

impl Error for SceneError {}
