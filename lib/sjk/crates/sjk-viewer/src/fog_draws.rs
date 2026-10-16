//! Prebuilt fog draw groups and allocation-free traversal.
use super::*;
use sjk_shader::FogPass;

/// One BSP surface, retaining its original PVS membership and optional mover mesh.
#[derive(Clone)]
pub(super) struct FogDraw {
    pub(super) geometry_source: Option<usize>,
    fog: u32,
    pipeline: usize,
    indices: Range<u32>,
    clusters: Vec<usize>,
    mover: Option<usize>,
}

impl FogDraw {
    pub(super) fn rebind(&mut self, pipeline: usize, geometry_source: Option<usize>) {
        self.pipeline = pipeline;
        self.geometry_source = geometry_source;
    }
}

/// Retain BSP fog indices and resolve inline/detached mover ranges at map load.
pub(super) fn collect(
    bsp: &Bsp,
    draws: &[DrawBatch],
    movers: &[crate::movers::Mesh],
    material: usize,
    pipeline: Option<usize>,
) -> Vec<FogDraw> {
    let Some(pipeline) = pipeline else {
        return Vec::new();
    };
    // Detached props can partition a BSP face into several mover ranges.
    // Keep that face's fog index on every partition as well as on inline models.
    let mut parts = movers
        .iter()
        .enumerate()
        .flat_map(|(mesh, mover)| {
            mover
                .draws
                .iter()
                .filter(move |draw| draw.material == material)
                .map(move |draw| (draw.indices.clone(), mesh))
        })
        .collect::<Vec<_>>();
    parts.sort_by_key(|(indices, _)| indices.start);
    let mut result = Vec::new();
    for draw in draws.iter().filter(|draw| draw.material == material) {
        let Some(fog) = draw
            .surface_index
            .and_then(|index| bsp.render().surfaces()[index].fog)
        else {
            continue;
        };
        let fog_draw = |indices, mover| FogDraw {
            geometry_source: None,
            fog: fog as u32 + 1,
            pipeline,
            indices,
            clusters: draw.clusters.clone(),
            mover,
        };
        if draw.world_surface {
            result.push(fog_draw(draw.indices.clone(), None));
        } else {
            for (indices, mesh) in mover_parts(&parts, draw.indices.clone()) {
                result.push(fog_draw(indices.clone(), Some(*mesh)));
            }
        }
    }
    compact(&mut result);
    result
}

fn mover_parts(
    parts: &[(Range<u32>, usize)],
    source: Range<u32>,
) -> impl Iterator<Item = &(Range<u32>, usize)> {
    let first = parts.partition_point(|(indices, _)| indices.start < source.start);
    parts[first..]
        .iter()
        .take_while(move |(indices, _)| indices.start < source.end)
        .filter(move |(indices, _)| indices.end <= source.end)
}

/// Coalesce adjacent ranges only when pipeline, fog, instance mesh and PVS agree.
/// Different colour materials can share this constant-colour pass.
pub(super) fn compact(draws: &mut Vec<FogDraw>) {
    for draw in draws.iter_mut() {
        draw.clusters.sort_unstable();
    }
    draws.sort_by_key(|draw| {
        (
            draw.fog,
            draw.mover,
            draw.pipeline,
            draw.geometry_source,
            draw.indices.start,
        )
    });
    let mut write = 0;
    for read in 0..draws.len() {
        if write != 0 {
            let previous = &draws[write - 1];
            let next = &draws[read];
            if previous.fog == next.fog
                && previous.pipeline == next.pipeline
                && previous.geometry_source == next.geometry_source
                && previous.mover == next.mover
                && previous.clusters == next.clusters
                && previous.indices.end == next.indices.start
            {
                draws[write - 1].indices.end = next.indices.end;
                continue;
            }
        }
        draws.swap(write, read);
        write += 1;
    }
    draws.truncate(write);
}

/// Precompute visible spans at map load, then merge across colour material boundaries.
/// Empty cluster lists mark ranges whose PVS decision was already made.
fn visible_plan(draws: &[FogDraw], is_visible: impl Fn(&[usize]) -> bool) -> Vec<FogDraw> {
    let mut plan = draws
        .iter()
        .filter(|draw| draw.mover.is_some() || is_visible(&draw.clusters))
        .map(|draw| FogDraw {
            geometry_source: draw.geometry_source,
            fog: draw.fog,
            pipeline: draw.pipeline,
            indices: draw.indices.clone(),
            clusters: Vec::new(),
            mover: draw.mover,
        })
        .collect::<Vec<_>>();
    compact(&mut plan);
    plan
}

impl Runtime {
    /// Preallocate the visible fog spans for every BSP PVS source cluster.
    pub(super) fn prepare_fog_visibility(&mut self, visibility: Option<&Visibility>) {
        let Some(visibility) = visibility else {
            return;
        };
        if self.opaque_fog.is_empty() {
            return;
        }
        // Colour stages retain the same PVS decisions, but share adjacent spans.
        // Build these alongside fog so adding the pass stays within the frame budget.
        for material in &mut self.materials {
            if material.static_draws.is_empty() {
                continue;
            }
            material.static_draws_by_cluster = (0..visibility.cluster_count)
                .map(|source| {
                    let mut spans = material
                        .static_draws
                        .iter()
                        .filter(|draw| visible(&draw.clusters, Some(source), Some(visibility)))
                        .map(|draw| StaticDraw {
                            view_cache: Default::default(),
                            indices: draw.indices.clone(),
                            clusters: Vec::new(),
                            bounds: draw.bounds,
                        })
                        .collect();
                    coalesce_static_draws(&mut spans);
                    spans
                })
                .collect();
        }
        self.opaque_fog_by_cluster = (0..visibility.cluster_count)
            .map(|source| {
                visible_plan(&self.opaque_fog, |clusters| {
                    visible(clusters, Some(source), Some(visibility))
                })
            })
            .collect();
    }

    fn opaque_fog_for(&self, source: Option<usize>, visibility: Option<&Visibility>) -> &[FogDraw] {
        if self.areas.active() {
            return &self.opaque_fog;
        }
        if visibility.is_some()
            && let Some(source) = source
        {
            if let Some(plan) = self.opaque_fog_by_cluster.get(source) {
                return plan;
            }
        }
        &self.opaque_fog
    }

    /// World/mover fog follows opaque entities, before blended material stages.
    /// Tables, draw lists, selectors and instance buffers are allocated at load.
    pub(crate) fn draw_fog<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        input: &FrameDraw<'pass>,
    ) {
        if self.fog_mode == crate::fog_volumes::Mode::Off {
            return;
        }
        let world = self.opaque_fog_for(input.source_cluster, input.visibility);
        self.draw_fog_ranges(pass, input, world);
        if self.fogged_entities == 0 || input.entities.is_empty() {
            return;
        }
        input.bind(pass);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        self.fog.bind(pass, 0, self.fog_mode);
        let mut last_pipeline = None;
        for draw in input.entities {
            let Some(material) = self.material(draw.material) else {
                continue;
            };
            if draw.no_depth || material.fog_pass != FogPass::Equal {
                continue;
            }
            let Some(pipeline) = material.fog_pipeline else {
                continue;
            };
            pass.set_bind_group(
                3,
                material
                    .stages
                    .first()
                    .map_or(&self.forge.empty_geometry_stage, |s| &s.geometry_group),
                &[],
            );
            if last_pipeline != Some(pipeline) {
                self.fog.set_pipeline(pass, pipeline, 2);
                last_pipeline = Some(pipeline);
            }
            // Fog index zero returns alpha zero: mixed instance ranges are safe.
            pass.draw_indexed(draw.indices.clone(), 0, draw.instances.clone());
        }
    }

    /// Submit preallocated world/mover ranges with one selector bind per fog.
    pub(super) fn draw_fog_ranges<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        input: &FrameDraw<'pass>,
        draws: &[FogDraw],
    ) {
        if self.fog_mode == crate::fog_volumes::Mode::Off || draws.is_empty() {
            return;
        }
        input.bind(pass);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        pass.set_bind_group(3, &self.forge.empty_geometry_stage, &[]);
        let mut last_geometry = None;
        let mut last_fog = None;
        let mut last_pipeline = None;
        for draw in draws {
            if draw.geometry_source != last_geometry {
                let stage = draw
                    .geometry_source
                    .and_then(|source| self.material(source))
                    .and_then(|m| m.stages.first())
                    .map_or(&self.forge.empty_geometry_stage, |s| &s.geometry_group);
                pass.set_bind_group(3, stage, &[]);
                last_geometry = draw.geometry_source;
            }
            let instances = if let Some(mesh) = draw.mover {
                let Some(range) = input
                    .mover_ranges
                    .get(mesh)
                    .filter(|range| !range.is_empty())
                else {
                    continue;
                };
                range.clone()
            } else {
                if !self
                    .areas
                    .visible(&draw.clusters, input.source_cluster, input.visibility)
                {
                    continue;
                }
                0..1
            };
            if last_fog != Some(draw.fog) {
                self.fog.bind(pass, draw.fog, self.fog_mode);
                last_fog = Some(draw.fog);
            }
            let pipeline = (draw.pipeline, usize::from(draw.mover.is_some()));
            if last_pipeline != Some(pipeline) {
                self.fog.set_pipeline(pass, pipeline.0, pipeline.1);
                // Statics ride the instanced program on the identity instance.
                pass.set_vertex_buffer(
                    1,
                    if draw.mover.is_some() {
                        input.instances.slice(..)
                    } else {
                        self.forge.identity_instance.slice(..)
                    },
                );
                last_pipeline = Some(pipeline);
            }
            pass.draw_indexed(draw.indices.clone(), 0, instances);
        }
    }
}

/// Shared buffers and visibility for an entire world submission.
pub(crate) struct FrameDraw<'pass> {
    pub(crate) camera: &'pass wgpu::BindGroup,
    pub(crate) vertices: &'pass wgpu::Buffer,
    pub(crate) indices: &'pass wgpu::Buffer,
    pub(crate) instances: &'pass wgpu::Buffer,
    pub(crate) mover_ranges: &'pass [Range<u32>],
    pub(crate) source_cluster: Option<usize>,
    pub(crate) visibility: Option<&'pass Visibility>,
    pub(crate) entities: &'pass [crate::entity_materials::Draw],
}

impl<'pass> FrameDraw<'pass> {
    fn bind(&self, pass: &mut wgpu::RenderPass<'pass>) {
        pass.set_bind_group(0, self.camera, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
    }
}
