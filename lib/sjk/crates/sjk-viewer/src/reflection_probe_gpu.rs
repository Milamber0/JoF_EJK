//! GPU resources of the reflection probes: the prefiltered cube array, the probe table
//! and the BRDF table the material program reads (bound in every material-mapped
//! stage's group, [`Shading`]), and the capture machinery ([`Probes`]): one capture
//! target, the per-face cameras, a scratch cube and the filter programs.
//!
//! Everything is allocated at map load; capturing and filtering a probe allocates
//! nothing. Memory: the array holds `6 * probes` layers of `size²` RGBA16F with
//! `log2(size) - 1` levels (about 1.05 MiB per probe at 128), plus one scratch cube
//! with every level and a `size²` capture target with its depth.

use super::{MAX_PROBES, Probe};
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use wgpu::util::DeviceExt;

pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Bindings in the material-mapped stage group (`material_map_gpu.rs`).
pub(crate) const CUBES: u32 = 13;
pub(crate) const SAMPLER: u32 = 14;
pub(crate) const TABLE: u32 = 15;
pub(crate) const LUT: u32 = 16;

/// Bytes of one probe record (centre and valid flag, box minimum, box maximum).
const RECORD: u64 = 48;
/// The table: a header (roughness levels, count) then [`MAX_PROBES`] records.
const TABLE_BYTES: u64 = 16 + RECORD * MAX_PROBES as u64;

/// The levels of a probe's cube, rend2's `CUBE_MAP_ROUGHNESS_MIPS + 1`: from the face
/// size down to 4x4, which roughness 1 reads.
pub(crate) fn levels(size: u32) -> u32 {
    size.max(4).ilog2() - 1
}

/// What the material program binds: the probes' cubes, table and BRDF table.
pub(crate) struct Shading {
    pub(crate) cubes: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) table: wgpu::Buffer,
    pub(crate) lut: wgpu::TextureView,
}

impl Shading {
    /// Layout entries of the four bindings, fragment visible.
    pub(crate) fn layout_entries() -> [wgpu::BindGroupLayoutEntry; 4] {
        let texture = |binding, view_dimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        [
            texture(CUBES, wgpu::TextureViewDimension::CubeArray),
            wgpu::BindGroupLayoutEntry {
                binding: SAMPLER,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: TABLE,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(TABLE_BYTES),
                },
                count: None,
            },
            texture(LUT, wgpu::TextureViewDimension::D2),
        ]
    }

    pub(crate) fn entries(&self) -> [wgpu::BindGroupEntry<'_>; 4] {
        [
            wgpu::BindGroupEntry {
                binding: CUBES,
                resource: wgpu::BindingResource::TextureView(&self.cubes),
            },
            wgpu::BindGroupEntry {
                binding: SAMPLER,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
            wgpu::BindGroupEntry {
                binding: TABLE,
                resource: self.table.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: LUT,
                resource: wgpu::BindingResource::TextureView(&self.lut),
            },
        ]
    }

    /// Stand-ins for maps without probes: one black cube, an empty table, a one-texel
    /// BRDF table. The program never reads them (every surface's probe index is 0).
    pub(crate) fn neutral(device: &wgpu::Device) -> Self {
        let cubes = cube_array(device, 1, 1, 1, wgpu::TextureUsages::empty())
            .create_view(&cube_array_view());
        let lut = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("SJK reflection BRDF stand-in"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: LUT_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Self {
            cubes,
            sampler: sampler(device),
            table: table_buffer(device, &[], 1),
            lut,
        }
    }
}

fn sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("SJK reflection probes"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    })
}

fn cube_array(
    device: &wgpu::Device,
    probes: u32,
    size: u32,
    levels: u32,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK reflection probes"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6 * probes,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: usage | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn cube_array_view() -> wgpu::TextureViewDescriptor<'static> {
    wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::CubeArray),
        ..Default::default()
    }
}

/// The probe table as the shader reads it (`ReflectionProbes`), every probe invalid
/// until its first capture is filtered.
fn table_buffer(device: &wgpu::Device, probes: &[Probe], levels: u32) -> wgpu::Buffer {
    let mut words = vec![[0f32; 4]; (TABLE_BYTES / 16) as usize];
    words[0] = [
        f32::from_bits(levels.saturating_sub(1)),
        f32::from_bits(probes.len() as u32),
        0.,
        0.,
    ];
    for (index, probe) in probes.iter().enumerate() {
        let at = 1 + index * 3;
        words[at] = probe.origin.extend(0.).to_array();
        words[at + 1] = probe.box_min.extend(0.).to_array();
        words[at + 2] = probe.box_max.extend(0.).to_array();
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK reflection probe table"),
        contents: bytemuck::cast_slice(&words),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

/// The capture camera of cube face `face` (+X, -X, +Y, -Y, +Z, -Z): forward along the
/// axis, up along the face's -t axis, so the right-handed image is the face mirrored
/// left to right (`reflection_probe_filter.wgsl`, `copy`).
pub(crate) fn face_axes(face: usize) -> (Vec3, Vec3) {
    match face {
        0 => (Vec3::X, Vec3::Y),
        1 => (-Vec3::X, Vec3::Y),
        2 => (Vec3::Y, -Vec3::Z),
        3 => (-Vec3::Y, Vec3::Z),
        4 => (Vec3::Z, Vec3::Y),
        _ => (-Vec3::Z, Vec3::Y),
    }
}

/// A 90° square projection of one face, with the viewer's near plane and `far`.
pub(crate) fn face_matrix(origin: Vec3, face: usize, far: f32) -> Mat4 {
    let (forward, up) = face_axes(face);
    let projection = glam::camera::rh::proj::directx::perspective(
        std::f32::consts::FRAC_PI_2,
        1.,
        1.,
        far.max(2.),
    );
    projection * glam::camera::rh::view::look_at_mat4(origin, origin + forward, up)
}

/// Where the capture loop stands: probes filtered at least once, probes whose content
/// predates a lighting change, and the probe whose faces are being captured.
#[derive(Debug, Default)]
pub(crate) struct State {
    pub(crate) valid: u64,
    pub(crate) stale: u64,
    /// The probe in progress and the faces of it already in the scratch cube.
    pub(crate) current: Option<(usize, u8)>,
    /// Lighting the probes' content matches.
    pub(crate) lighting: Option<super::Signature>,
}

/// This frame's work: `faces` of probe `probe`, and whether they complete it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) probe: usize,
    pub(crate) faces: std::ops::Range<usize>,
    pub(crate) completes: bool,
}

impl State {
    /// Choose this frame's faces. A first capture takes a whole probe per frame (map
    /// load); refreshing after a lighting change takes `refresh` faces per frame, and
    /// the old content stays visible until the probe's new faces are all filtered.
    pub(crate) fn plan(
        &mut self,
        count: usize,
        lighting: super::Signature,
        refresh: usize,
    ) -> Option<Plan> {
        if count == 0 {
            return None;
        }
        let all = if count >= 64 {
            u64::MAX
        } else {
            (1u64 << count) - 1
        };
        match self.lighting {
            Some(captured) if super::relit(&captured, &lighting) => {
                self.stale = self.valid;
                self.lighting = Some(lighting);
            }
            None => self.lighting = Some(lighting),
            _ => {}
        }
        let (probe, done) = match self.current {
            Some(current) => current,
            None => {
                let missing = all & !self.valid;
                let wanted = if missing != 0 {
                    missing
                } else {
                    self.stale & all
                };
                if wanted == 0 {
                    return None;
                }
                (wanted.trailing_zeros() as usize, 0)
            }
        };
        let first = done.trailing_ones() as usize;
        let rate = if self.valid & (1 << probe) == 0 {
            6
        } else {
            refresh.clamp(1, 6)
        };
        let end = (first + rate).min(6);
        let faces = first..end;
        let done = done | (((1u16 << end) - (1u16 << first)) as u8);
        let completes = done == 0x3f;
        self.current = (!completes).then_some((probe, done));
        if completes {
            self.valid |= 1 << probe;
            self.stale &= !(1 << probe);
        }
        Some(Plan {
            probe,
            faces,
            completes,
        })
    }
}

/// The capture side of a map's probes; see the module documentation.
pub(crate) struct Probes {
    pub(crate) probes: Vec<Probe>,
    pub(crate) size: u32,
    pub(crate) shading: Shading,
    pub(crate) color: wgpu::TextureView,
    pub(crate) depth: crate::DepthTarget,
    /// Per face: the light-pass camera (packed into the light buffer's corner) and the
    /// colour camera, each a uniform buffer and its group.
    pub(crate) cameras: Vec<[(wgpu::Buffer, wgpu::BindGroup); 2]>,
    copy: wgpu::ComputePipeline,
    copy_groups: Vec<wgpu::BindGroup>,
    downsample: wgpu::ComputePipeline,
    downsample_groups: Vec<(wgpu::BindGroup, u32)>,
    prefilter: wgpu::ComputePipeline,
    prefilter_groups: Vec<(wgpu::BindGroup, u32)>,
    probe_uniform: wgpu::Buffer,
    pub(crate) state: RefCell<State>,
}

impl Probes {
    /// Allocate everything for `probes` (at most [`MAX_PROBES`]) at `size` per face.
    /// `format` is the scene format the world pipelines draw into.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        probes: Vec<Probe>,
        size: u32,
    ) -> Self {
        let started = std::time::Instant::now();
        let probes: Vec<Probe> = probes.into_iter().take(MAX_PROBES).collect();
        let levels = levels(size);
        let count = probes.len().max(1) as u32;
        let cubes = cube_array(
            device,
            count,
            size,
            levels,
            wgpu::TextureUsages::STORAGE_BINDING,
        );
        let scratch_levels = size.ilog2() + 1;
        let scratch = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK reflection probe scratch"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: scratch_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let level_view = |texture: &wgpu::Texture, level: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let color = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("SJK reflection probe capture"),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let depth = crate::DepthTarget::new(device, size, size);
        let cameras = (0..6)
            .map(|_| {
                [0, 1].map(|_| {
                    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("SJK reflection probe camera"),
                        size: std::mem::size_of::<crate::CameraUniform>() as u64,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("SJK reflection probe camera"),
                        layout: camera_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        }],
                    });
                    (buffer, group)
                })
            })
            .collect();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK reflection probe filter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("reflection_probe_filter.wgsl").into()),
        });
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: FORMAT,
                view_dimension: wgpu::TextureViewDimension::D2Array,
            },
            count: None,
        };
        let texture = |binding, view_dimension, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension,
                multisampled: false,
            },
            count: None,
        };
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let pipeline = |entries: &[wgpu::BindGroupLayoutEntry], entry: &str| {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("SJK reflection probe filter"),
                entries,
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("SJK reflection probe filter"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("SJK reflection probe filter"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            });
            (layout, pipeline)
        };
        let (copy_layout, copy) = pipeline(
            &[
                texture(0, wgpu::TextureViewDimension::D2, false),
                storage(1),
                uniform(2),
            ],
            "copy",
        );
        let (downsample_layout, downsample) = pipeline(
            &[
                texture(0, wgpu::TextureViewDimension::D2Array, false),
                storage(1),
            ],
            "downsample",
        );
        let (prefilter_layout, prefilter) = pipeline(
            &[
                texture(0, wgpu::TextureViewDimension::Cube, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                storage(2),
                uniform(3),
                uniform(4),
            ],
            "prefilter",
        );
        let scratch_top = level_view(&scratch, 0);
        let copy_groups = (0..6u32)
            .map(|face| {
                let constant = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("SJK reflection probe face"),
                    contents: bytemuck::cast_slice(&[face, 0, 0, 0]),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("SJK reflection probe copy"),
                    layout: &copy_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&color),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&scratch_top),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: constant.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect();
        let downsample_groups = (1..scratch_levels)
            .map(|level| {
                let finer = level_view(&scratch, level - 1);
                let coarser = level_view(&scratch, level);
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("SJK reflection probe downsample"),
                    layout: &downsample_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&finer),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&coarser),
                        },
                    ],
                });
                (group, (size >> level).max(1))
            })
            .collect();
        let source = scratch.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let filter_sampler = sampler(device);
        let probe_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK reflection probe slot"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let prefilter_groups = (0..levels)
            .map(|level| {
                let roughness = level as f32 / (levels - 1).max(1) as f32;
                let constant = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("SJK reflection probe level"),
                    contents: bytemuck::cast_slice(&[
                        roughness,
                        size as f32,
                        scratch_levels as f32,
                        0.,
                    ]),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let target = level_view(&cubes, level);
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("SJK reflection probe prefilter"),
                    layout: &prefilter_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&filter_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&target),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: constant.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: probe_uniform.as_entire_binding(),
                        },
                    ],
                });
                (group, (size >> level).max(1))
            })
            .collect();

        let lut = upload_lut(device, queue);
        let shading = Shading {
            cubes: cubes.create_view(&cube_array_view()),
            sampler: sampler(device),
            table: table_buffer(device, &probes, levels),
            lut,
        };
        let bytes = |texels: u64, levels: u32| -> u64 {
            (0..levels).map(|l| (texels >> (2 * l)).max(1) * 8).sum()
        };
        let face = u64::from(size) * u64::from(size);
        let total =
            bytes(face, levels) * 6 * u64::from(count) + bytes(face, scratch_levels) * 6 + face * 8;
        crate::log::progress(format_args!(
            "Reflection probes: {} at {size}x{size}, {levels} levels, {:.1} MiB; \
             resources in {:.1} ms",
            probes.len(),
            total as f64 / 1048576.,
            started.elapsed().as_secs_f64() * 1000.
        ));
        Self {
            probes,
            size,
            shading,
            color,
            depth,
            cameras,
            copy,
            copy_groups,
            downsample,
            downsample_groups,
            prefilter,
            prefilter_groups,
            probe_uniform,
            state: RefCell::new(State::default()),
        }
    }

    /// Copy the captured `face` into the scratch cube (after its scene pass).
    pub(crate) fn copy_face(&self, encoder: &mut wgpu::CommandEncoder, face: usize) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SJK reflection probe copy"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.copy);
        pass.set_bind_group(0, &self.copy_groups[face], &[]);
        let groups = self.size.div_ceil(8);
        pass.dispatch_workgroups(groups, groups, 1);
    }

    /// Downsample the scratch cube and filter it into `probe`'s slot, then mark the slot
    /// valid in the table the material program reads.
    pub(crate) fn filter(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &crate::frame_queue::FrameQueue,
        probe: usize,
    ) {
        queue.write_buffer(
            &self.probe_uniform,
            0,
            bytemuck::cast_slice(&[6 * probe as u32, 0, 0, 0]),
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SJK reflection probe filter"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.downsample);
        for (group, size) in &self.downsample_groups {
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(size.div_ceil(8), size.div_ceil(8), 6);
        }
        pass.set_pipeline(&self.prefilter);
        for (group, size) in &self.prefilter_groups {
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(size.div_ceil(8), size.div_ceil(8), 6);
        }
        drop(pass);
        let centre = self.probes[probe].origin.extend(1.);
        queue.write_buffer(
            &self.shading.table,
            16 + RECORD * probe as u64,
            bytemuck::cast_slice(&centre.to_array()),
        );
    }
}

fn upload_lut(device: &wgpu::Device, queue: &crate::frame_queue::FrameQueue) -> wgpu::TextureView {
    let texels = super::brdf::table();
    let size = super::brdf::SIZE;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK reflection BRDF table"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: LUT_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * 8),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&Default::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Direction of texel (s, t) in -1..1 of `face`, as the filter program computes it.
    fn cube_direction(face: usize, s: f32, t: f32) -> Vec3 {
        match face {
            0 => Vec3::new(1., -t, -s),
            1 => Vec3::new(-1., -t, s),
            2 => Vec3::new(s, 1., t),
            3 => Vec3::new(s, -1., -t),
            4 => Vec3::new(s, -t, 1.),
            _ => Vec3::new(-s, -t, -1.),
        }
        .normalize()
    }

    #[test]
    fn captured_faces_are_the_cube_faces_mirrored() {
        // A point seen at capture pixel (x, y) must be the cube texel (1 - x, y): the
        // filter's copy mirrors the image left to right.
        for face in 0..6 {
            let matrix = face_matrix(Vec3::ZERO, face, 4096.);
            for &(s, t) in &[(0.5, -0.25), (-0.75, 0.6), (0.1, 0.9)] {
                let direction = cube_direction(face, s, t);
                let clip = matrix * (direction * 100.).extend(1.);
                let ndc = clip.truncate() / clip.w;
                // Image coordinates in -1..1 with y down, then mirrored in x.
                let (x, y) = (-ndc.x, -ndc.y);
                assert!(
                    (x - s).abs() < 1e-4 && (y - t).abs() < 1e-4,
                    "{face}: {x} {y}"
                );
            }
        }
    }

    #[test]
    fn levels_end_at_four_texels() {
        assert_eq!(levels(128), 6);
        assert_eq!(levels(256), 7);
        assert_eq!(levels(32), 4);
        assert_eq!(128 >> (levels(128) - 1), 4);
    }

    #[test]
    fn the_table_matches_its_shader_declaration() {
        // vec4 header + 64 records of three vec4.
        assert_eq!(TABLE_BYTES, 16 + 64 * 48);
        let source = include_str!("material_maps_reflection.wgsl");
        assert!(source.contains("probes: array<ReflectionProbe, 64>"));
        assert!(source.contains(&format!("@group(1) @binding({CUBES})")));
        assert!(source.contains(&format!("@group(1) @binding({TABLE})")));
        assert_eq!(MAX_PROBES, 64);
    }

    #[test]
    fn first_captures_take_a_probe_per_frame_and_refreshes_a_face() {
        let lit: super::super::Signature = [0., 0., 1., 1., 0.2, 0.2, 0.2, 1., 1.];
        let mut state = State::default();
        let plan = state.plan(2, lit, 1).expect("first probe");
        assert_eq!(
            (plan.probe, plan.faces.clone(), plan.completes),
            (0, 0..6, true)
        );
        let plan = state.plan(2, lit, 1).expect("second probe");
        assert_eq!((plan.probe, plan.completes), (1, true));
        assert_eq!(state.plan(2, lit, 1), None);
        // The sun moves: every probe is refreshed one face per frame, old content kept.
        let mut moved = lit;
        moved[1] = 0.2;
        for face in 0..6 {
            let plan = state.plan(2, moved, 1).expect("refresh");
            assert_eq!((plan.probe, plan.faces.clone()), (0, face..face + 1));
            assert_eq!(plan.completes, face == 5);
            assert_eq!(state.valid, 0b11);
        }
        assert_eq!(state.plan(2, moved, 1).map(|p| p.probe), Some(1));
        assert_eq!(state.plan(0, moved, 1), None);
    }
}

#[cfg(test)]
mod program_tests {
    #[test]
    fn filter_program_validates() {
        let source = include_str!("reflection_probe_filter.wgsl");
        crate::wgsl_source::validate(source);
        for entry in ["copy", "downsample", "prefilter"] {
            assert!(source.contains(&format!("fn {entry}(")), "{entry}");
        }
    }
}
