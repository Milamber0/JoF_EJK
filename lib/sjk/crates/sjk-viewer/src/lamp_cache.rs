//! Static lamp irradiance, baked once per map into the world's lightmap parametrization.
//!
//! A map's lamps, their area form factors and their traced static visibility never
//! change, yet the light pass evaluated them for every receiver every frame: most of
//! its cost. The BSP already gives each lightmapped world surface unique coordinates, so
//! the same lamp program runs once per cache texel here and the light pass takes one
//! bilinear sample instead. Actor shadows from slotted lamps stay live (they subtract
//! from the cached sum, `lamp_light_cached`), and every receiver the cache cannot serve
//! exactly enough keeps the direct evaluation: movers, entities, vertex-lit or coarsely
//! mapped surfaces, faces seen from behind, texels where two surfaces share lightmap
//! space, texels where the light bends too sharply to interpolate (`lamp_cache_rim.wgsl`),
//! and chart borders beyond the one-texel rim.
//!
//! `SJK_LAMP_CACHE=0` disables the cache for same-binary comparisons.
use std::ops::Range;

/// World units per cache texel. Lamp shadows are filtered over five texels of a 128²
/// octahedral map (several units of penumbra at any useful distance) and area form
/// factors are softened over 16 units, so this spacing resolves both.
const TEXEL_UNITS: f32 = 4.;
/// A surface mapped this much coarser than the cache's spacing is evaluated directly.
/// 4, not 3: `JoFTemple`'s great hall floor is mapped 3.6 times coarser, and lighting it
/// lamp by lamp took 4.2 of a 7.5 ms frame at 4K; cached, the frame is 3.6 ms and the
/// view differs by at most 8/255 (steep texels still go direct, `lamp_cache_rim.wgsl`).
const COARSEST: f32 = 4.;
const MIN_RESOLUTION: u32 = 64;
const MAX_RESOLUTION: u32 = 2048;
/// Larger maps halve the cache's resolution; steep texels fall back to direct evaluation
/// either way, so a coarser cache costs some speed, not accuracy.
// A 128 MiB cap pushed large maps onto coarse pages and made many static
// receivers run the full lamp loop each frame. The bounded larger cache uses the
// same validity/error checks; small maps still allocate only their required pages.
const BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const TEXEL_BYTES: u64 = 8;
/// The direction layers of a directed cache (`bake_light_directed`): a unit-luminance
/// vector as unsigned colour, filtered like the light.
const DIRECTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgb10a2Unorm;
const DIRECTION_TEXEL_BYTES: u64 = 4;
/// Receiver attribute targets: two RGBA32F plus the RGBA32F cache coordinates.
pub(crate) const ATTACHMENT_BYTES: u32 = 48;

/// CPU plan: which world vertices read which cache layer, and what each layer draws.
pub(crate) struct Pages {
    resolution: u32,
    /// Per world vertex: cache layer + 1, or 0 for direct evaluation.
    stream: Vec<f32>,
    /// Per layer: the joined index runs of the surfaces baked into it.
    runs: Vec<Vec<Range<u32>>>,
    /// Per layer: each surface baked into it, for partial refreshes.
    surfaces: Vec<Vec<CachedSurface>>,
}

/// A surface of a cache layer: the texels it covers, `[x0, y0, x1, y1)`, and its world
/// bounds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CachedSurface {
    pub(crate) texels: [u32; 4],
    pub(crate) lower: glam::Vec3,
    pub(crate) upper: glam::Vec3,
}

/// One candidate surface: its index range, lightmap page and whether it is a static,
/// light-buffered world surface.
pub(crate) struct Surface {
    pub(crate) indices: Range<u32>,
    pub(crate) page: i32,
}

impl Pages {
    /// Per layer, its surfaces' texels and world bounds.
    pub(crate) fn surfaces(&self) -> &[Vec<CachedSurface>] {
        &self.surfaces
    }

    /// Texels per layer edge.
    pub(crate) fn resolution(&self) -> u32 {
        self.resolution
    }

    /// `None` when the map has no lightmapped static surface worth caching.
    pub(crate) fn plan(
        positions: &[[f32; 3]],
        coordinates: &[[f32; 2]],
        indices: &[u32],
        surfaces: &[Surface],
        max_layers: u32,
    ) -> Option<Self> {
        if std::env::var_os("SJK_LAMP_CACHE").is_some_and(|value| value == "0") {
            return None;
        }
        // World units per lightmap coordinate unit, per surface and area-weighted overall.
        let mut measured: Vec<(usize, f32, f32)> = Vec::new();
        for (index, surface) in surfaces.iter().enumerate() {
            if surface.page < 0 {
                continue;
            }
            let (mut world, mut mapped) = (0f64, 0f64);
            for t in indices[surface.indices.start as usize..surface.indices.end as usize]
                .chunks_exact(3)
            {
                let p = |i: u32| glam::Vec3::from_array(positions[i as usize]);
                let c = |i: u32| glam::Vec2::from_array(coordinates[i as usize]);
                world += f64::from((p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).length());
                mapped += f64::from((c(t[1]) - c(t[0])).perp_dot(c(t[2]) - c(t[0])).abs());
            }
            if world > 1e-3 && mapped > 1e-12 {
                measured.push((index, (world / mapped).sqrt() as f32, world as f32));
            }
        }
        if measured.is_empty() {
            return None;
        }
        let mut by_scale: Vec<(f32, f32)> = measured.iter().map(|m| (m.1, m.2)).collect();
        by_scale.sort_by(|a, b| a.0.total_cmp(&b.0));
        let half = by_scale.iter().map(|m| m.1).sum::<f32>() * 0.5;
        let mut seen = 0.;
        let median = by_scale
            .iter()
            .find(|m| {
                seen += m.1;
                seen >= half
            })
            .map_or(by_scale[0].0, |m| m.0);

        let mut layers = std::collections::BTreeMap::new();
        for &(index, _, _) in &measured {
            let next = layers.len() as u32;
            layers.entry(surfaces[index].page).or_insert(next);
        }
        if layers.len() as u32 > max_layers {
            return None;
        }
        let spacing = TEXEL_UNITS;

        let mut resolution = ((median / spacing).ceil() as u32)
            .next_power_of_two()
            .clamp(MIN_RESOLUTION, MAX_RESOLUTION);
        let budget = BUDGET_BYTES;

        while resolution > MIN_RESOLUTION
            && layers.len() as u64 * u64::from(resolution) * u64::from(resolution) * TEXEL_BYTES
                > budget
        {
            resolution /= 2;
        }

        let mut stream = vec![0f32; positions.len()];
        let mut runs: Vec<Vec<Range<u32>>> = vec![Vec::new(); layers.len()];
        let mut cached: Vec<Vec<CachedSurface>> = vec![Vec::new(); layers.len()];
        for &(index, scale, _) in &measured {
            if scale / resolution as f32 > COARSEST * spacing {
                continue;
            }
            let surface = &surfaces[index];
            let layer = layers[&surface.page];
            for &i in &indices[surface.indices.start as usize..surface.indices.end as usize] {
                stream[i as usize] = (layer + 1) as f32;
            }
            runs[layer as usize].push(surface.indices.clone());
            let range = &indices[surface.indices.start as usize..surface.indices.end as usize];
            let (mut lower, mut upper) = (glam::Vec3::INFINITY, glam::Vec3::NEG_INFINITY);
            let (mut low, mut high) = (glam::Vec2::INFINITY, glam::Vec2::NEG_INFINITY);
            for &i in range {
                let p = glam::Vec3::from_array(positions[i as usize]);
                let c = glam::Vec2::from_array(coordinates[i as usize]);
                (lower, upper) = (lower.min(p), upper.max(p));
                (low, high) = (low.min(c), high.max(c));
            }
            let size = resolution as f32;
            let texel =
                |v: f32, round: fn(f32) -> f32| (round(v * size).max(0.) as u32).min(resolution);
            cached[layer as usize].push(CachedSurface {
                texels: [
                    texel(low.x, f32::floor),
                    texel(low.y, f32::floor),
                    texel(high.x, f32::ceil),
                    texel(high.y, f32::ceil),
                ],
                lower,
                upper,
            });
        }
        for layer in &mut runs {
            *layer = join(std::mem::take(layer));
        }
        if runs.iter().all(Vec::is_empty) {
            return None;
        }
        Some(Self {
            resolution,
            stream,
            runs,
            surfaces: cached,
        })
    }
}

/// Colour targets of the bake and rim passes: the light, then the directions.
fn targets(directed: bool) -> &'static [Option<wgpu::ColorTargetState>] {
    const TARGETS: [Option<wgpu::ColorTargetState>; 2] = [
        Some(wgpu::ColorTargetState {
            format: FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: DIRECTION_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ];
    &TARGETS[..if directed { 2 } else { 1 }]
}

fn join(mut ranges: Vec<Range<u32>>) -> Vec<Range<u32>> {
    ranges.sort_by_key(|range| range.start);
    let mut joined: Vec<Range<u32>> = Vec::new();
    for range in ranges {
        match joined.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => joined.push(range),
        }
    }
    joined
}

/// Map-lifetime GPU cache. It survives lighting-settings changes: its content depends
/// only on the map's lamps and their static visibility.
pub(crate) struct Cache {
    resolution: u32,
    runs: Vec<Vec<Range<u32>>>,
    /// Vertex stream at slot 1 of the cached static receiver pipeline.
    pub(crate) pages: wgpu::Buffer,
    array: wgpu::TextureView,
    layers: Vec<wgpu::TextureView>,
    /// Where each texel's lamp light comes from, for material maps (`Cache::new`).
    directions: Option<(wgpu::TextureView, Vec<wgpu::TextureView>)>,
    sampler: wgpu::Sampler,
    baked: std::cell::Cell<bool>,
    /// Kept after the first bake only while movers re-bake the cache (`Cache::refresh`);
    /// its scratch targets take [`Cache::baker_bytes`].
    baker: std::cell::RefCell<Option<Baker>>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Bake {
    project: [f32; 4],
    limits: [f32; 4],
}

impl Cache {
    /// `directed` also bakes where the lamp light comes from: the map has material maps
    /// that move it to their mapped normals (`light_buffer::LightBuffer::directed`).
    pub(crate) fn new(device: &wgpu::Device, pages: &Pages, directed: bool) -> Self {
        use wgpu::util::DeviceExt;
        let layered = |label, format| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: pages.resolution,
                    height: pages.resolution,
                    depth_or_array_layers: pages.runs.len() as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let array = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            let layers = (0..pages.runs.len() as u32)
                .map(|layer| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect::<Vec<_>>();
            (array, layers)
        };
        let (array, layers) = layered("SJK lamp light cache", FORMAT);
        let directions = directed.then(|| layered("SJK lamp light directions", DIRECTION_FORMAT));
        let texel_bytes = TEXEL_BYTES + if directed { DIRECTION_TEXEL_BYTES } else { 0 };
        crate::log::progress(format_args!(
            "Lamp light cache: {} layers of {}x{} texels{}, {:.1} MiB",
            pages.runs.len(),
            pages.resolution,
            pages.resolution,
            if directed { " with directions" } else { "" },
            pages.runs.len() as f64
                * f64::from(pages.resolution)
                * f64::from(pages.resolution)
                * texel_bytes as f64
                / 1048576.
        ));
        Self {
            resolution: pages.resolution,
            runs: pages.runs.clone(),
            array,
            layers,
            directions,
            pages: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK lamp cache pages"),
                contents: bytemuck::cast_slice(&pages.stream),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("SJK lamp cache"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            baked: std::cell::Cell::new(false),
            baker: std::cell::RefCell::new(None),
        }
    }

    /// Slot-1 layout of the page stream.
    pub(crate) fn page_layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![5 => Float32];
        wgpu::VertexBufferLayout {
            array_stride: 4,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }

    /// Group of the cached fullscreen light program: receiver coordinates, cache, sampler,
    /// and with `directed` the direction layers.
    pub(crate) fn layout(device: &wgpu::Device, directed: bool) -> wgpu::BindGroupLayout {
        let texture = |binding, filterable, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let mut entries = vec![
            texture(0, false, wgpu::TextureViewDimension::D2),
            texture(1, true, wgpu::TextureViewDimension::D2Array),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // Receivers the cache cannot serve (`receivers::Direct`).
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];
        if directed {
            entries.push(texture(4, true, wgpu::TextureViewDimension::D2Array));
        }
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK lamp cache receivers"),
            entries: &entries,
        })
    }

    pub(crate) fn group(
        &self,
        device: &wgpu::Device,
        coordinates: &wgpu::TextureView,
        direct: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(coordinates),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&self.array),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: direct.as_entire_binding(),
            },
        ];
        if let Some((directions, _)) = &self.directions {
            entries.push(wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(directions),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &Self::layout(device, self.directions.is_some()),
            entries: &entries,
        })
    }

    /// Bake every layer into `encoder` the first time a frame lights the map. With `keep`
    /// (movers will re-bake parts of it) the pipelines and scratch targets stay for
    /// [`Cache::refresh`]; otherwise they live only for this call, as their memory
    /// ([`Cache::baker_bytes`]) would serve nothing.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bake_once(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        vertices: &wgpu::Buffer,
        indices: &wgpu::Buffer,
        lamps: &crate::lamp_lights::Gpu,
        bounds: [glam::Vec3; 2],
        keep: bool,
    ) {
        if self.baked.replace(true) {
            return;
        }
        let started = std::time::Instant::now();
        let baker = Baker::new(device, self, bounds);
        let light_group = baker.light_group(device, lamps);
        for index in 0..self.layers.len() {
            self.bake_layer(
                &baker,
                &light_group,
                encoder,
                vertices,
                indices,
                index,
                None,
            );
        }
        if keep {
            *self.baker.borrow_mut() = Some(baker);
        }
        crate::log::progress(format_args!(
            "Lamp light cache: bake encoded in {:.1} ms{}",
            started.elapsed().as_secs_f64() * 1000.,
            if keep {
                format!(
                    "; bake targets kept for mover refreshes, {:.1} MiB",
                    self.baker_bytes() as f64 / 1048576.
                )
            } else {
                String::new()
            }
        ));
    }

    /// GPU memory of the bake's scratch targets: two depth targets and one or two colour
    /// targets at the cache's resolution.
    pub(crate) fn baker_bytes(&self) -> u64 {
        bake_target_bytes(self.resolution, self.directions.is_some())
    }

    /// Bake `regions` (layer, texel rectangle `[x0, y0, x1, y1)`) again with the lamps'
    /// current visibility: a mover's shadow changed there (`mover_occlusion.rs`). Nothing
    /// before the first bake, which already sees it. The bake's targets are made again
    /// if the first bake did not keep them, and then kept.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn refresh(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        vertices: &wgpu::Buffer,
        indices: &wgpu::Buffer,
        lamps: &crate::lamp_lights::Gpu,
        bounds: [glam::Vec3; 2],
        regions: &[(u32, [u32; 4])],
    ) {
        if regions.is_empty() || !self.baked.get() {
            return;
        }
        let mut baker = self.baker.borrow_mut();
        let baker = baker.get_or_insert_with(|| Baker::new(device, self, bounds));
        let light_group = baker.light_group(device, lamps);
        for &(layer, rect) in regions {
            if (layer as usize) < self.layers.len() {
                self.bake_layer(
                    baker,
                    &light_group,
                    encoder,
                    vertices,
                    indices,
                    layer as usize,
                    Some(rect),
                );
            }
        }
    }

    /// One layer: its surfaces' extents, their light, then the steep and rim passes over
    /// the whole layer. With `region`, only that rectangle is cleared and lit again; the
    /// post passes still cover the layer, which outside it can only poison a few more
    /// texels (evaluated directly), never change their light.
    #[allow(clippy::too_many_arguments)]
    fn bake_layer(
        &self,
        baker: &Baker,
        light_group: &wgpu::BindGroup,
        encoder: &mut wgpu::CommandEncoder,
        vertices: &wgpu::Buffer,
        indices: &wgpu::Buffer,
        index: usize,
        region: Option<[u32; 4]>,
    ) {
        let layer = &self.layers[index];
        let runs = &self.runs[index];
        let direction = self.directions.as_ref().map(|(_, layers)| &layers[index]);
        let scissor = |pass: &mut wgpu::RenderPass<'_>| {
            if let Some([x0, y0, x1, y1]) = region {
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
            }
        };
        let color = |view, keep: bool| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: if keep {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        for (target, clear, pipeline) in [
            (&baker.nearest, 1., &baker.near_pipeline),
            (&baker.farthest, 0., &baker.far_pipeline),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK lamp cache extent"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: target,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &baker.extent_group, &[]);
            scissor(&mut pass);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for run in runs {
                pass.draw_indexed(run.clone(), 0, 0..1);
            }
        }
        let targets = || [Some(layer), direction].into_iter().flatten();
        if region.is_some() {
            // Empty the rectangle: texels no surface covers any more take rim again.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK lamp cache clear"),
                color_attachments: &targets().map(|v| color(v, true)).collect::<Vec<_>>(),
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&baker.clear_pipeline);
            pass.set_bind_group(0, &baker.from_rim, &[]);
            scissor(&mut pass);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK lamp cache light"),
                color_attachments: &targets()
                    .map(|v| color(v, region.is_some()))
                    .collect::<Vec<_>>(),
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&baker.light_pipeline);
            pass.set_bind_group(0, light_group, &[]);
            scissor(&mut pass);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for run in runs {
                pass.draw_indexed(run.clone(), 0, 0..1);
            }
        }
        // Poison texels too steep to interpolate, then add one texel of rim.
        let from_layer = baker.rim_group(layer, direction);
        for (target, target_direction, source, pipeline) in [
            (
                &baker.rim,
                baker.rim_direction.as_ref(),
                &from_layer,
                &baker.steep_pipeline,
            ),
            (layer, direction, &baker.from_rim, &baker.rim_pipeline),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK lamp cache rim"),
                color_attachments: &[Some(target), target_direction]
                    .into_iter()
                    .flatten()
                    .map(|v| color(v, false))
                    .collect::<Vec<_>>(),
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, source, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// The bake's scratch targets at `resolution`: nearest and farthest depth (Depth32Float),
/// the rim target ([`FORMAT`]) and, with directions, the rim directions
/// ([`DIRECTION_FORMAT`]).
fn bake_target_bytes(resolution: u32, directed: bool) -> u64 {
    let per_texel = 4 + 4 + TEXEL_BYTES + if directed { DIRECTION_TEXEL_BYTES } else { 0 };
    u64::from(resolution).pow(2) * per_texel
}

/// The bake's pipelines, groups and scratch targets, kept after the first bake while
/// movers refresh the cache. The light group is made per bake: the lamps' buffers change
/// with the lighting settings.
struct Baker {
    nearest: wgpu::TextureView,
    farthest: wgpu::TextureView,
    rim: wgpu::TextureView,
    rim_direction: Option<wgpu::TextureView>,
    uniform: wgpu::Buffer,
    extent_group: wgpu::BindGroup,
    light_layout: wgpu::BindGroupLayout,
    rim_layout: wgpu::BindGroupLayout,
    from_rim: wgpu::BindGroup,
    near_pipeline: wgpu::RenderPipeline,
    far_pipeline: wgpu::RenderPipeline,
    light_pipeline: wgpu::RenderPipeline,
    steep_pipeline: wgpu::RenderPipeline,
    rim_pipeline: wgpu::RenderPipeline,
    clear_pipeline: wgpu::RenderPipeline,
    device: wgpu::Device,
}

impl Baker {
    fn new(device: &wgpu::Device, cache: &Cache, bounds: [glam::Vec3; 2]) -> Self {
        use wgpu::util::DeviceExt;
        // Two surfaces sharing lightmap space differ in world position. An oblique
        // projection of it, kept as nearest and farthest depth per texel, exposes them.
        let axis = glam::Vec3::new(0.5477, 0.3651, 0.7518);
        let corners = (0..8).map(|c| {
            glam::Vec3::new(bounds[c & 1].x, bounds[c >> 1 & 1].y, bounds[c >> 2 & 1].z).dot(axis)
        });
        let (low, high) = corners.fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), s| {
            (low.min(s), high.max(s))
        });
        let span = (high - low).max(1.) * 1.02;
        let bake = Bake {
            project: [
                axis.x / span,
                axis.y / span,
                axis.z / span,
                0.01 - low / span,
            ],
            limits: [2. / span, 0., 0., 0.],
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::bytes_of(&bake),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let extent = wgpu::Extent3d {
            width: cache.resolution,
            height: cache.resolution,
            depth_or_array_layers: 1,
        };
        let scratch = |label, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let depth_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let nearest = scratch(
            "SJK lamp cache nearest",
            wgpu::TextureFormat::Depth32Float,
            depth_usage,
        );
        let farthest = scratch(
            "SJK lamp cache farthest",
            wgpu::TextureFormat::Depth32Float,
            depth_usage,
        );
        let rim = scratch("SJK lamp cache rim", FORMAT, depth_usage);
        let directed = cache.directions.is_some();
        let rim_direction = directed.then(|| {
            scratch(
                "SJK lamp cache rim directions",
                DIRECTION_FORMAT,
                depth_usage,
            )
        });

        let uniform_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let depth_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let extent_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[uniform_entry(7)],
        });
        let mut entries = crate::lamp_lights::Gpu::layout_entries(0).to_vec();
        entries.push(crate::lamp_lights::Gpu::visibility_layout(6));
        entries.extend([uniform_entry(7), depth_entry(8), depth_entry(9)]);
        let light_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let extent_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &extent_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 7,
                resource: uniform.as_entire_binding(),
            }],
        });
        let extent_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK lamp cache extent"),
            source: wgpu::ShaderSource::Wgsl(include_str!("lamp_cache_bake.wgsl").into()),
        });
        let light_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK lamp cache light"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}{}{}",
                    include_str!("lamp_cache_bake.wgsl"),
                    crate::lamp_lights::source(0, 0, false),
                    include_str!("lamp_cache_light.wgsl")
                )
                .into(),
            ),
        });
        let buffers = [Some(crate::GpuVertex::layout())];
        let pipeline = |label,
                        layout: &wgpu::BindGroupLayout,
                        shader: &wgpu::ShaderModule,
                        fragment: Option<&str>,
                        compare: Option<wgpu::CompareFunction>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("bake_vertex"),
                    compilation_options: Default::default(),
                    buffers: &buffers,
                },
                fragment: fragment.map(|entry| wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: targets(directed),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: compare.map(|compare| wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let near_pipeline = pipeline(
            "SJK lamp cache nearest",
            &extent_layout,
            &extent_shader,
            None,
            Some(wgpu::CompareFunction::Less),
        );
        let far_pipeline = pipeline(
            "SJK lamp cache farthest",
            &extent_layout,
            &extent_shader,
            None,
            Some(wgpu::CompareFunction::Greater),
        );
        let light_pipeline = pipeline(
            "SJK lamp cache light",
            &light_layout,
            &light_shader,
            Some(if directed {
                "bake_light_directed"
            } else {
                "bake_light"
            }),
            None,
        );

        let rim_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let rim_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[rim_entry(0), rim_entry(1)][..if directed { 2 } else { 1 }],
        });
        let rim_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK lamp cache rim"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}{}",
                    include_str!("lamp_response.wgsl"),
                    include_str!("lamp_cache_rim.wgsl")
                )
                .into(),
            ),
        });
        let rim_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&rim_layout)],
            immediate_size: 0,
        });
        let post = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("SJK lamp cache rim"),
                layout: Some(&rim_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &rim_shader,
                    entry_point: Some("rim_vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &rim_shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: targets(directed),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let (steep_pipeline, rim_pipeline) = if directed {
            (post("steep_directed"), post("rim_directed"))
        } else {
            (post("steep"), post("rim"))
        };
        let rim_group = |view: &wgpu::TextureView, direction: Option<&wgpu::TextureView>| {
            let entries = [view]
                .into_iter()
                .chain(direction)
                .enumerate()
                .map(|(binding, view)| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: wgpu::BindingResource::TextureView(view),
                })
                .collect::<Vec<_>>();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &rim_layout,
                entries: &entries,
            })
        };
        let from_rim = rim_group(&rim, rim_direction.as_ref());

        let clear_pipeline = post(if directed { "clear_directed" } else { "clear" });
        Self {
            nearest,
            farthest,
            rim,
            rim_direction,
            uniform,
            extent_group,
            light_layout,
            rim_layout,
            from_rim,
            near_pipeline,
            far_pipeline,
            light_pipeline,
            steep_pipeline,
            rim_pipeline,
            clear_pipeline,
            device: device.clone(),
        }
    }

    fn light_group(
        &self,
        device: &wgpu::Device,
        lamps: &crate::lamp_lights::Gpu,
    ) -> wgpu::BindGroup {
        let mut entries = lamps.entries(0).to_vec();
        entries.push(lamps.visibility_entry(6));
        entries.extend([
            wgpu::BindGroupEntry {
                binding: 7,
                resource: self.uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(&self.nearest),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(&self.farthest),
            },
        ]);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.light_layout,
            entries: &entries,
        })
    }

    fn rim_group(
        &self,
        view: &wgpu::TextureView,
        direction: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        let entries = [view]
            .into_iter()
            .chain(direction)
            .enumerate()
            .map(|(binding, view)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: wgpu::BindingResource::TextureView(view),
            })
            .collect::<Vec<_>>();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.rim_layout,
            entries: &entries,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_kept_bake_targets_take_20_bytes_a_texel_with_directions() {
        const MIB: u64 = 1 << 20;
        assert_eq!(super::bake_target_bytes(2048, true), 80 * MIB);
        assert_eq!(super::bake_target_bytes(2048, false), 64 * MIB);
        assert_eq!(super::bake_target_bytes(1024, true), 20 * MIB);
    }

    #[test]
    fn bake_and_rim_programs_validate_with_directions() {
        let light = format!(
            "{}{}{}",
            include_str!("lamp_cache_bake.wgsl"),
            crate::lamp_lights::source(0, 0, false),
            include_str!("lamp_cache_light.wgsl")
        );
        crate::wgsl_source::validate(&light);
        assert!(light.contains("fn bake_light_directed("));
        let rim = format!(
            "{}{}",
            include_str!("lamp_response.wgsl"),
            include_str!("lamp_cache_rim.wgsl")
        );
        crate::wgsl_source::validate(&rim);
        assert!(rim.contains("fn clear(") && rim.contains("fn clear_directed("));
        for entry in ["steep", "rim", "steep_directed", "rim_directed"] {
            assert!(rim.contains(&format!("fn {entry}(")), "{entry}");
        }
    }
}
