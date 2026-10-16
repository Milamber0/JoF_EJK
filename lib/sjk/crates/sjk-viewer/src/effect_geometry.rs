//! Fixed-capacity tessellation for Raven cylinder and electricity primitives.
//!
//! Cylinders follow `RB_SurfaceCylinder` (`rd-vanilla/tr_surface.cpp:721-863`).
//! Electricity follows `RB_SurfaceElectricity` and its `DoBoltSeg` path
//! (`tr_surface.cpp:868-1079`). Random bolt offsets use a primitive seed rather
//! than renderer-global randomness so demo replay remains deterministic.

use super::*;
use crate::particle_types::PrimitiveShape;
use bytemuck::{Pod, Zeroable};

#[path = "effect_expansion.rs"]
/// GPU tessellation descriptions owned by the viewer, independent of prediction.
pub(crate) mod expansion;
#[path = "effect_geometry_quads.rs"]
mod quads;
#[path = "effect_geometry_support.rs"]
mod support;
use support::{normal_vectors, trace_endpoint, trace_length};

pub(crate) const MAX_VERTICES: usize = 65_536;
pub(crate) const MAX_INDICES: usize = 98_304;
const MAX_TRACE_DISTANCE: f32 = 16_384.0;
const CYLINDER_SEGMENTS: usize = 32;
const MIN_CYLINDER_SEGMENTS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) local_uv: [f32; 2],
    pub(crate) uv_rect: [f32; 4],
    pub(crate) uv_transform: [f32; 4],
    pub(crate) color: [f32; 4],
    pub(crate) depth_hack: f32,
}

impl Vertex {
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x2,
            2 => Float32x4,
            3 => Float32x4,
            4 => Float32x4,
            5 => Float32
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stats {
    /// Actual host upload bytes, not generated GPU output size.
    pub(crate) vertices: usize,
    pub(crate) indices: usize,
}

pub(crate) struct Mesh {
    /// Present only after compute resources were successfully constructed.
    pub(crate) expansion: Option<expansion::Batch>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    ranges: [Range<u32>; crate::effect_blend::PIPELINE_COUNT],
    /// World decals per blend slot; drawn with the polygon-offset pipelines.
    decal_ranges: [Range<u32>; crate::effect_blend::PIPELINE_COUNT],
    /// The dynamic glow layers at the end of each of `ranges`.
    glow_ranges: [Range<u32>; crate::effect_blend::PIPELINE_COUNT],
    dropped: usize,
    /// Per particle, the blend slots it draws into this frame; storage reused across frames.
    slots: Vec<u8>,
}

/// End an `org2fromTrace` line at the first solid between its origin and its untraced
/// end, once, before it is first drawn: `CFxScheduler::CreateEffect` traces when it
/// creates the line (`FxScheduler.cpp:1392-1418`), as electricity's end is traced below.
pub(crate) fn resolve_traced_streak(
    particle: &mut Particle,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
) {
    if !std::mem::take(&mut particle.trace_streak) {
        return;
    }
    if let Some(streak) = &mut particle.streak {
        let start = particle.motion.sample().origin;
        *streak = trace_endpoint(start, start + *streak, bsp, scratch) - start;
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self {
            expansion: None,
            vertices: Vec::with_capacity(MAX_VERTICES),
            indices: Vec::with_capacity(MAX_INDICES),
            ranges: std::array::from_fn(|_| 0..0),
            decal_ranges: std::array::from_fn(|_| 0..0),
            glow_ranges: std::array::from_fn(|_| 0..0),
            dropped: 0,
            slots: Vec::new(),
        }
    }
}

impl Mesh {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build(
        &mut self,
        particles: &mut [Particle],
        decals: &mut crate::decal_store::DecalStore,
        atlas: &ParticleAtlas,
        now: Instant,
        global_seconds: f32,
        camera: Vec3,
        field_of_view: f32,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
    ) -> Stats {
        self.vertices.clear();
        self.indices.clear();
        self.dropped = 0;
        if let Some(batch) = &mut self.expansion {
            batch.clear();
        }
        let has_geometry = particles.iter().any(|particle| {
            !matches!(
                particle.shape,
                PrimitiveShape::Billboard | PrimitiveShape::FrameBillboard
            )
        });
        if !has_geometry && !decals.has_polys() {
            self.ranges.fill(0..0);
            self.decal_ranges.fill(0..0);
            self.glow_ranges.fill(0..0);
            return Stats::default();
        }

        // Which blend slots each particle draws into, found once: the slot loop below visited
        // every particle seven times and sampled its envelopes and shader layers each time
        // before discovering that six of the visits had nothing to draw.
        let mut slots = std::mem::take(&mut self.slots);
        slots.clear();

        slots.extend(particles.iter().map(|particle| {
            if matches!(
                particle.shape,
                PrimitiveShape::Billboard | PrimitiveShape::FrameBillboard
            ) {
                return 0u8;
            }
            let age = now.saturating_duration_since(particle.spawned_at);
            if age < particle.delay {
                return 0;
            }
            let seconds = age.saturating_sub(particle.delay).as_secs_f32();
            atlas
                .layers_for(
                    &particle.shader,
                    particle.shader_seconds(seconds, global_seconds),
                )
                .iter()
                .fold(0u8, |mask, layer| {
                    mask | 1 << crate::effect_blend::slot(layer.blend)
                })
        }));
        // With glowing stages in the atlas, each slot emits its other layers first and
        // its glowing ones last, so the glow pass draws one tail range per slot.
        let parts: &[Option<bool>] = if atlas.any_glow {
            &[Some(false), Some(true)]
        } else {
            &[None]
        };
        for blend_index in 0..crate::effect_blend::PIPELINE_COUNT {
            let range_start = self.counts().1 as u32;
            let mut glow_start = range_start;
            for &glow in parts {
                glow_start = self.counts().1 as u32;
                for (particle, _) in particles
                    .iter_mut()
                    .zip(&slots)
                    .filter(|(_, mask)| **mask >> blend_index & 1 == 1)
                {
                    self.append_particle(
                        particle,
                        atlas,
                        now,
                        global_seconds,
                        camera,
                        field_of_view,
                        bsp,
                        scratch,
                        blend_index,
                        glow,
                    );
                }
            }
            self.ranges[blend_index] = range_start..self.counts().1 as u32;
            self.glow_ranges[blend_index] = if atlas.any_glow {
                glow_start..self.counts().1 as u32
            } else {
                0..0
            };

            let decal_start = self.counts().1 as u32;
            decals.emit(now, |draw| {
                if usize::from(draw.slot) == blend_index {
                    self.append_decal(draw, atlas, global_seconds, blend_index);
                }
            });
            self.decal_ranges[blend_index] = decal_start..self.counts().1 as u32;
        }
        self.slots = slots;
        Stats {
            vertices: self.counts().0,
            indices: self.counts().1,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn append_particle(
        &mut self,
        particle: &mut Particle,
        atlas: &ParticleAtlas,
        now: Instant,
        global_seconds: f32,
        camera: Vec3,
        field_of_view: f32,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
        blend_index: usize,
        glow: Option<bool>,
    ) {
        // Sprite shading belongs to effect_submission. This pass emits only
        // cylinders/electricity; sampling sprite envelopes and shader stages
        // here previously repeated pure, discarded work for every blend slot.

        if matches!(
            particle.shape,
            PrimitiveShape::Billboard | PrimitiveShape::FrameBillboard
        ) {
            return;
        }
        let age = now.saturating_duration_since(particle.spawned_at);
        if age < particle.delay {
            return;
        }
        let seconds = age.saturating_sub(particle.delay).as_secs_f32();
        let life_millis = particle.lifetime.as_secs_f32() * 1_000.0;
        let elapsed_millis = seconds * 1_000.0;
        let (size, alpha, rgb) = particle.sample_envelopes(seconds);
        let (rgb_fade, vertex_alpha) = effect_runtime::particle_fade(particle.use_alpha, alpha);
        let color = [
            rgb[0] * rgb_fade,
            rgb[1] * rgb_fade,
            rgb[2] * rgb_fade,
            vertex_alpha,
        ];
        let layers = atlas.layers_for(
            &particle.shader,
            particle.shader_seconds(seconds, global_seconds),
        );
        for layer in layers.iter().filter(|layer| {
            crate::effect_blend::slot(layer.blend) == blend_index
                && glow.is_none_or(|glow| layer.glow == glow)
        }) {
            let tint = layer.tint([color[0], color[1], color[2]]);
            let layer_color = [
                tint[0] * layer.rgb,
                tint[1] * layer.rgb,
                tint[2] * layer.rgb,
                color[3] * layer.alpha,
            ];
            match &mut particle.shape {
                PrimitiveShape::Cylinder {
                    axis,
                    size2,
                    length,
                    trace_end,
                    depth_hack,
                } => {
                    let origin = particle.motion.sample().origin;
                    let length = if *trace_end {
                        trace_length(origin, *axis, bsp, scratch)
                    } else {
                        length.sample(elapsed_millis, life_millis, particle.seed.wrapping_add(41))
                    };
                    let end_radius =
                        size2.sample(elapsed_millis, life_millis, particle.seed.wrapping_add(42));
                    self.cylinder(
                        origin,
                        *axis,
                        size,
                        end_radius,
                        length,
                        camera,
                        field_of_view,
                        layer.uv_rect,
                        layer.uv_transform,
                        layer_color,
                        *depth_hack,
                    );
                }
                PrimitiveShape::Electricity {
                    end,
                    chaos,
                    tapered,
                    branched,
                    grow,
                    trace_end,
                    depth_hack,
                } => {
                    let start = particle.motion.sample().origin;
                    if *trace_end {
                        *end = trace_endpoint(start, *end, bsp, scratch);
                        *trace_end = false;
                    }
                    let progress = (seconds / particle.lifetime.as_secs_f32()).clamp(0.0, 1.0);
                    let visible_end = if *grow {
                        start.lerp(*end, progress)
                    } else {
                        *end
                    };
                    self.electricity(
                        start,
                        visible_end,
                        size,
                        *chaos,
                        *tapered,
                        *branched,
                        camera,
                        particle.seed ^ (elapsed_millis as u32).wrapping_mul(0x9e37_79b9),
                        layer.uv_rect,
                        layer.uv_transform,
                        layer_color,
                        *depth_hack,
                    );
                }
                PrimitiveShape::Billboard | PrimitiveShape::FrameBillboard => {}
            }
        }
    }

    /// One stored decal poly (`R_AddDecals` → `RE_AddPolyToScene`): the
    /// fragment's vertex colour is the decal colour with the fade alpha.
    fn append_decal(
        &mut self,
        draw: crate::decal_store::DecalDraw<'_>,
        atlas: &ParticleAtlas,
        global_seconds: f32,
        blend_index: usize,
    ) {
        let layers = atlas.layers_for(draw.shader, global_seconds);
        for layer in layers
            .iter()
            .filter(|layer| crate::effect_blend::slot(layer.blend) == blend_index)
        {
            let tint = layer.tint([draw.color[0], draw.color[1], draw.color[2]]);
            let color = [
                tint[0] * layer.rgb,
                tint[1] * layer.rgb,
                tint[2] * layer.rgb,
                draw.color[3] * layer.alpha,
            ];
            self.polygon(draw.vertices, layer.uv_rect, layer.uv_transform, color);
        }
    }

    /// Append a convex polygon as a triangle fan.
    pub(crate) fn polygon(
        &mut self,
        points: &[crate::decal_marks::DecalVertex],
        uv: [f32; 4],
        uv_transform: [f32; 4],
        color: [f32; 4],
    ) {
        if self.expanded_polygon(points, uv, uv_transform, color) {
            return;
        }
        if points.len() < 3
            || self.vertices.len() + points.len() > MAX_VERTICES
            || self.indices.len() + (points.len() - 2) * 3 > MAX_INDICES
        {
            self.dropped += 1;
            return;
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend(points.iter().map(|point| Vertex {
            position: point.position,
            local_uv: point.st,
            uv_rect: uv,
            uv_transform,
            color,
            depth_hack: 0.0,
        }));
        for corner in 1..points.len() as u32 - 1 {
            self.indices
                .extend([base, base + corner, base + corner + 1]);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cylinder(
        &mut self,
        origin: Vec3,
        axis: Vec3,
        radius_at_end: f32,
        radius_at_origin: f32,
        length: f32,
        camera: Vec3,
        field_of_view: f32,
        uv: [f32; 4],
        uv_transform: [f32; 4],
        color: [f32; 4],
        depth_hack: bool,
    ) {
        let axis = axis.normalize_or(Vec3::X);
        let end = origin + axis * length;
        let midpoint_distance = camera.distance(origin.lerp(end, 0.5));
        let detail = 1.0 - midpoint_distance * (field_of_view / 90.0) / 1_024.0;
        let segments = ((CYLINDER_SEGMENTS as f32 * detail) as usize)
            .clamp(MIN_CYLINDER_SEGMENTS, CYLINDER_SEGMENTS);
        let (right, up) = normal_vectors(axis);
        if self.expanded_cylinder(
            origin,
            end,
            right,
            up,
            radius_at_origin,
            radius_at_end,
            segments,
            uv,
            uv_transform,
            color,
            depth_hack,
        ) {
            return;
        }
        for segment in 0..segments {
            let first = segment as f32 / segments as f32;
            let second = (segment + 1) as f32 / segments as f32;
            let ring = |turn: f32, radius: f32| {
                (right * (turn * std::f32::consts::TAU).cos()
                    + up * (turn * std::f32::consts::TAU).sin())
                    * radius
            };
            self.quad(
                [
                    (origin + ring(first, radius_at_origin), [first, 1.0]),
                    (end + ring(first, radius_at_end), [first, 0.0]),
                    (end + ring(second, radius_at_end), [second, 0.0]),
                    (origin + ring(second, radius_at_origin), [second, 1.0]),
                ],
                uv,
                uv_transform,
                color,
                depth_hack,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn electricity(
        &mut self,
        start: Vec3,
        end: Vec3,
        radius: f32,
        chaos: f32,
        tapered: bool,
        branched: bool,
        camera: Vec3,
        seed: u32,
        uv: [f32; 4],
        uv_transform: [f32; 4],
        color: [f32; 4],
        depth_hack: bool,
    ) {
        crate::effect_electricity::append(
            self,
            start,
            end,
            radius,
            chaos,
            tapered,
            branched,
            camera,
            seed,
            uv,
            uv_transform,
            color,
            depth_hack,
        );
    }

    pub(crate) fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    pub(crate) fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub(crate) fn ranges(&self) -> &[Range<u32>; crate::effect_blend::PIPELINE_COUNT] {
        &self.ranges
    }

    pub(crate) fn decal_ranges(&self) -> &[Range<u32>; crate::effect_blend::PIPELINE_COUNT] {
        &self.decal_ranges
    }

    /// Per blend slot, the glowing tail of [`Self::ranges`].
    pub(crate) fn glow_ranges(&self) -> &[Range<u32>; crate::effect_blend::PIPELINE_COUNT] {
        &self.glow_ranges
    }
}
