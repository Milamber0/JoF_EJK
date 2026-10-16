//! Map-wide probe grid: spherical-harmonic irradiance and octahedral depth moments per
//! probe, traced through the voxel world. The grid never moves, so nothing pops when the
//! camera does; it converges at installation and then refreshes a window of live probes
//! per frame to follow the sun. Map-lifetime buffers; no per-frame allocation or readback.
#[path = "gi_probe_dispatch.rs"]
mod dispatch;
#[path = "gi_probe_idle.rs"]
mod idle;
use glam::{IVec3, Vec3};
#[path = "gi_probe_refresh.rs"]
mod refresh;

/// Probe spacing is chosen per map so the grid stays under this many probes.
const MAX_PROBES: usize = 48_000;
const MIN_SPACING: f32 = 64.;
/// Live probes refreshed every frame: a full pass every 190 frames or so on ffa3, so the
/// bounce follows the day clock within seconds at a fraction of a millisecond per frame.
const WINDOW: u32 = 256;
/// The first sun-lit pass after installation and the pass after a light jump run wide:
/// the whole map relit in a handful of frames instead of a slow cycle.
const FIRST_WINDOW: u32 = 8192;
/// First lamp binding inside the update group.
const LAMP_BASE: u32 = 8;
/// Full passes under an unchanged light before the bounce counts as settled: with the
/// running average at BLEND, four passes leave under two percent of the bounce chain
/// unaccounted for, and the refresh then idles until the light changes.
const SETTLE_PASSES: u32 = 4;
const RANGE: f32 = 2048.;
pub(crate) const RAYS: u32 = 128;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    origin_index: [i32; 4],
    counts: [u32; 4],
    spacing: [f32; 4],
    sun: [f32; 4],
    sun_color: [f32; 4],
    sky: [f32; 4],
    far_vp: [[f32; 4]; 4],
    far_quality: [f32; 4],
    window: [u32; 4],
}

/// Everything the update pass and the samplers share.
pub(crate) struct Runtime {
    pub(crate) params: wgpu::Buffer,

    /// The components combined under the current light, reused while unchanged.
    pub(crate) display: wgpu::Buffer,
    pub(crate) depth: wgpu::Buffer,
    pub(crate) state: wgpu::Buffer,
    live_count: u32,
    pub(crate) origin: IVec3,
    pub(crate) counts: [u32; 3],
    pub(crate) spacing: f32,
    /// Fine voxel size, the error bar of every probe depth moment.
    pub(crate) voxel_size: f32,
    update: wgpu::ComputePipeline,
    update_group: wgpu::BindGroup,
    combine: wgpu::ComputePipeline,
    total: u32,
    dispatch_limit: u32,
    idle: idle::Idle,

    cursor: std::cell::Cell<u32>,
    frame: std::cell::Cell<u32>,
    /// Consecutive-frame lighting history; continuous motion never resets the averages.
    history: std::cell::Cell<refresh::History>,
    /// Probes still to refresh in the wide first pass; zero once it completed.
    remaining: std::cell::Cell<u32>,
    /// Probes refreshed since the far cascade appeared or the light last changed at all.
    settled: std::cell::Cell<u32>,
    /// Probes still to refresh in the restart pass after the light jumped.
    restarting: std::cell::Cell<u32>,
    /// Placement state (dead flags, every probe fresh) to restore for a full replacement.
    initial_state: Vec<[i32; 4]>,
}

/// Sun and sky radiance for one update, in display units (sunlit white floor ≈ 1).
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Light {
    pub(crate) direction: Vec3,
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
    pub(crate) sky: [f32; 3],
}

impl Light {
    /// A jump of the sun direction beyond five degrees (a set of the day clock): sun and
    /// sky colour or intensity need no refresh at all, the components are scaled live,
    /// and a running cycle never turns five degrees between two frames.
    fn changed_much(&self, other: &Light) -> bool {
        self.direction.dot(other.direction) < 0.996_2
    }
}

/// Grid placement for a map: spacing and counts covering `bounds` within the probe cap.
pub(crate) fn placement(bounds: [Vec3; 2]) -> (f32, IVec3, [u32; 3]) {
    let extent = (bounds[1] - bounds[0]).max(Vec3::splat(MIN_SPACING));
    let volume = f64::from(extent.x) * f64::from(extent.y) * f64::from(extent.z);
    let spacing = ((volume / MAX_PROBES as f64).cbrt() as f32).max(MIN_SPACING);
    let spacing = (spacing / 16.).ceil() * 16.;
    let origin = (bounds[0] / spacing).floor().as_ivec3();
    let last = (bounds[1] / spacing).ceil().as_ivec3();
    let counts = (last - origin + IVec3::ONE).max(IVec3::ONE);
    (
        spacing,
        origin,
        [counts.x as u32, counts.y as u32, counts.z as u32],
    )
}

fn layout(device: &wgpu::Device, read_only: bool) -> wgpu::BindGroupLayout {
    let mut entries = layout_entries(0).to_vec();
    if !read_only {
        for entry in &mut entries[1..4] {
            entry.ty = wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            };
        }
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        entries.extend(crate::lamp_lights::Gpu::layout_entries(LAMP_BASE));
        entries.push(crate::lamp_lights::Gpu::visibility_layout(6));
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK GI probes"),
        entries: &entries,
    })
}

pub(crate) const SAMPLE_SHADER: &str = include_str!("gi_probes_sample.wgsl");

/// The sampling module with its six bindings moved to `group` starting at `base`, for
/// programs whose group 1 is already taken. Pixels read the per-frame combined table
/// (four coefficients per probe, current light applied); only the update reads the
/// three components.
pub(crate) fn sample_source(group: u32, base: u32) -> String {
    let mut source = SAMPLE_SHADER.replace(
        "const COMPONENTS: u32 = 12u;",
        "const COMPONENTS: u32 = 4u;",
    );
    for binding in 0..6 {
        source = source.replace(
            &format!("@group(1) @binding({binding})"),
            &format!("@group({group}) @binding({})", base + binding),
        );
    }
    source
}

/// Layout entries for the six probe bindings at `base`, fragment and compute visible.
pub(crate) fn layout_entries(base: u32) -> [wgpu::BindGroupLayoutEntry; 6] {
    let stages = wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT;
    let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding: base + binding,
        visibility: stages,
        ty,
        count: None,
    };
    let storage = wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    [
        entry(
            0,
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        ),
        entry(1, storage),
        entry(2, storage),
        entry(3, storage),
        entry(
            4,
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
        ),
        entry(
            5,
            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
        ),
    ]
}

/// Zeroed stand-ins for programs that must bind the probe slots without a volume: a
/// zero probe count makes the sampler return the sky fill.
pub(crate) struct Neutral {
    pub(crate) params: wgpu::Buffer,
    pub(crate) storage: wgpu::Buffer,
}

impl Neutral {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            params: buffer(
                "SJK GI probes absent",
                std::mem::size_of::<Params>() as u64,
                wgpu::BufferUsages::UNIFORM,
            ),
            storage: buffer("SJK GI probes absent", 64, wgpu::BufferUsages::STORAGE),
        }
    }
}

impl Runtime {
    /// Allocate the grid over the map bounds, marking probes inside solid space dead
    /// (`dead(position)`), and compile the update pass against the voxel world and the
    /// map-wide far cascade (`far_depth`), which supplies sun visibility at ray hits.
    pub(crate) fn new(
        device: &wgpu::Device,
        voxels: &super::gi::Runtime,
        far_depth: &wgpu::TextureView,
        bounds: [Vec3; 2],
        dead: impl Fn(Vec3) -> bool,
        lamps: &crate::lamp_lights::Gpu,
    ) -> Self {
        let (spacing, origin, counts) = placement(bounds);
        let total = counts[0] as usize * counts[1] as usize * counts[2] as usize;
        let mut live = Vec::with_capacity(total);
        let mut state = vec![[0i32; 4]; total];
        for slot in 0..total {
            let index = IVec3::new(
                (slot % counts[0] as usize) as i32,
                ((slot / counts[0] as usize) % counts[1] as usize) as i32,
                (slot / (counts[0] as usize * counts[1] as usize)) as i32,
            ) + origin;
            let position = index.as_vec3() * spacing;
            if dead(position) {
                state[slot] = [index.x, index.y, index.z, -1];
            } else {
                live.push(slot as u32);
            }
        }
        let live_count = live.len() as u32;
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let storage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;
        let params = buffer(
            "SJK GI probe parameters",
            std::mem::size_of::<Params>() as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        // Three L1 components (unit sun, unit sky, emission): 12 vec4 per probe, and the
        // per-frame combination under the current light that pixels read (4 vec4).
        let sh = buffer("SJK GI probe irradiance SH", (total * 192) as u64, storage);
        let display = buffer("SJK GI probe display SH", (total * 64) as u64, storage);
        let depth = buffer(
            "SJK GI probe depth moments",
            (total * 64 * 8) as u64,
            storage,
        );
        let state_buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("SJK GI probe state"),
                contents: bytemuck::cast_slice(&state),
                usage: storage,
            },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shared = [
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: display.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: depth.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: state_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(far_depth),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ];
        let mut with_live = shared.to_vec();
        with_live[1] = wgpu::BindGroupEntry {
            binding: 1,
            resource: sh.as_entire_binding(),
        };
        with_live.push(wgpu::BindGroupEntry {
            binding: 7,
            resource: display.as_entire_binding(),
        });
        with_live.extend(lamps.entries(LAMP_BASE));
        with_live.push(lamps.visibility_entry(6));
        let update_layout = layout(device, false);

        let update_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK GI probes update"),
            layout: &update_layout,
            entries: &with_live,
        });

        let source = format!(
            "{}{}{}{}",
            include_str!("gi_trace.wgsl"),
            SAMPLE_SHADER.replace(
                "var<storage, read> probe_",
                "var<storage, read_write> probe_"
            ),
            crate::lamp_lights::source(1, LAMP_BASE, false),
            include_str!("gi_probes.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK GI probe update"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&voxels.layout), Some(&update_layout)],
            immediate_size: 0,
        });
        let update = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SJK GI probe update"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("update"),
            compilation_options: Default::default(),
            cache: None,
        });
        let combine = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SJK GI probe combine"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("combine"),
            compilation_options: Default::default(),
            cache: None,
        });
        crate::log::progress(format_args!(
            "GI probes: {}x{}x{} at {spacing} units, {live_count} live of {total}, {WINDOW} \
             refreshed per frame, {:.1} MB",
            counts[0],
            counts[1],
            counts[2],
            (total * (64 + 512 + 16)) as f64 / 1e6
        ));
        Self {
            params,
            display,
            depth,
            state: state_buffer,
            live_count,
            origin,
            counts,
            spacing,
            voxel_size: voxels.world.fine_size,
            update,
            combine,
            idle: idle::Idle::new(),
            total: total as u32,
            dispatch_limit: device.limits().max_compute_workgroups_per_dimension,
            update_group,

            cursor: std::cell::Cell::new(0),
            frame: std::cell::Cell::new(0),
            history: std::cell::Cell::new(refresh::History::default()),
            remaining: std::cell::Cell::new(0),
            settled: std::cell::Cell::new(0),
            restarting: std::cell::Cell::new(0),
            initial_state: state,
        }
    }

    /// Refresh `window` live probes. `far` is the far cascade's matrix, texel and depth
    /// range; without it hits see sky and emission only.
    pub(crate) fn update_window(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &crate::frame_queue::FrameQueue,
        voxels: &super::gi::Runtime,
        light: &Light,
        far: Option<(glam::Mat4, f32, f32)>,
        window: u32,
    ) {
        if self.total == 0 {
            return;
        }
        let window = window.min(self.total);
        let frame = self.frame.get();
        let params = Params {
            origin_index: [self.origin.x, self.origin.y, self.origin.z, window as i32],
            counts: [
                self.counts[0],
                self.counts[1],
                self.counts[2],
                self.counts[0] * self.counts[1] * self.counts[2],
            ],
            spacing: [self.spacing, 0., RAYS as f32, RANGE],
            sun: [
                light.direction.x,
                light.direction.y,
                light.direction.z,
                light.intensity,
            ],
            sun_color: [light.color[0], light.color[1], light.color[2], 0.],
            sky: [light.sky[0], light.sky[1], light.sky[2], 0.],
            far_vp: far.map_or(glam::Mat4::IDENTITY, |f| f.0).to_cols_array_2d(),
            far_quality: [
                far.map_or(0., |f| f.1),
                far.map_or(0., |f| f.2),
                f32::from(far.is_some()),
                self.voxel_size,
            ],
            window: [
                self.cursor.get(),
                self.total,
                frame,
                u32::from(self.restarting.get() > 0),
            ],
        };

        self.frame.set(frame.wrapping_add(1));
        if self.idle.reuse(params, window) {
            return;
        }
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SJK GI probe update"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.update);
        pass.set_bind_group(0, &voxels.bind_group, &[]);
        pass.set_bind_group(1, &self.update_group, &[]);
        if window > 0 {
            let [x, y] = dispatch::grid(window, self.dispatch_limit);
            pass.dispatch_workgroups(x, y, 1);
        }
        // Refreshed probes or changed shading inputs need a new display table.
        pass.set_pipeline(&self.combine);
        let [x, y] = dispatch::grid(self.total.div_ceil(64), self.dispatch_limit);
        pass.dispatch_workgroups(x, y, 1);
        self.cursor.set((self.cursor.get() + window) % self.total);
    }

    /// The per-frame refresh in play: a window of live probes every frame, so the bounce
    /// follows the sun and the day clock with the hysteresis as its only lag. The first
    /// pass with a far cascade (none exists at installation) replaces the burst state
    /// outright, every probe fresh again, and runs wider.
    pub(crate) fn update(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &crate::frame_queue::FrameQueue,
        voxels: &super::gi::Runtime,
        light: &Light,
        far: Option<(glam::Mat4, f32, f32)>,
    ) {
        let mut history = self.history.get();
        let update = history.observe(*light, far.is_some());
        self.history.set(history);
        match update {
            refresh::Update::Initial => {
                self.remaining.set(self.total);
                queue.write_buffer(&self.state, 0, bytemuck::cast_slice(&self.initial_state));
            }
            refresh::Update::Jump => {
                self.remaining.set(self.total);
                self.restarting.set(self.total);
            }
            _ => {}
        }
        if update != refresh::Update::Steady {
            self.settled.set(0);
        }
        let remaining = self.remaining.get();
        let idle = remaining == 0 && self.settled.get() >= self.total.saturating_mul(SETTLE_PASSES);
        let window = if remaining > 0 {
            FIRST_WINDOW.min(remaining)
        } else if idle {
            0
        } else {
            WINDOW
        };
        self.update_window(encoder, queue, voxels, light, far, window);
        self.remaining.set(remaining.saturating_sub(window));
        self.restarting
            .set(self.restarting.get().saturating_sub(window));
        self.settled.set(self.settled.get().saturating_add(window));
    }

    /// Live probe count, for burst convergence at installation.
    pub(crate) fn live_count(&self) -> u32 {
        self.live_count
    }
}

#[cfg(test)]
mod program_tests {
    use super::*;

    #[test]
    fn probe_update_program_validates() {
        // The update program reads the full three-component table, where the directed
        // irradiance combines the components under the current sun and sky.
        let source = format!(
            "{}{}{}{}",
            include_str!("gi_trace.wgsl"),
            SAMPLE_SHADER.replace(
                "var<storage, read> probe_",
                "var<storage, read_write> probe_"
            ),
            crate::lamp_lights::source(1, LAMP_BASE, false),
            include_str!("gi_probes.wgsl")
        );
        crate::wgsl_source::validate(&source);
        assert!(source.contains("fn probe_irradiance_directed("));
    }
}
