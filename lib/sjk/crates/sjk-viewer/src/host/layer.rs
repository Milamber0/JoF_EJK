//! The hosting engine's 2D layer: its menus, HUD, console and fonts, drawn from the
//! `DrawStretchPic` family of calls it makes each frame.
//!
//! Retail Jedi Academy draws 2D in a 640x480 virtual screen (`RB_SetGL2D`,
//! `qglOrtho(0, 640, 480, 0, 0, 1)`), and so does every picture command handed
//! over here. Each picture is a shader handle the engine registered by name; the
//! picture is its shader's image ([`crate::shader_image::load_shader_image`]), and
//! its first stage decides the blend, as SJK's menu-file HUD does
//! ([`crate::menu_hud`]). A name with no image (`white`, a solid fill) draws the
//! vertex colour alone. Like SJK's own 2D layer ([`crate::ui_target`]) the layer
//! draws display values through the UNORM view of the finished frame.

use bytemuck::{Pod, Zeroable};
use sjk_shader::{ShaderCatalog, StageBlend};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;

/// One picture command, as `SJK_HostFrame` receives it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Picture {
    pub(crate) shader: i32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) s1: f32,
    pub(crate) t1: f32,
    pub(crate) s2: f32,
    pub(crate) t2: f32,
    pub(crate) color: [f32; 4],
    /// Degrees; `kind` says about which point the picture turns.
    pub(crate) angle: f32,
    /// [`KIND_STRETCH`], [`KIND_ROTATE`] or [`KIND_ROTATE2`].
    pub(crate) kind: i32,
}

/// `RE_StretchPic`.
pub(crate) const KIND_STRETCH: i32 = 0;
/// `RE_RotatePic`: turned about its top-left corner.
#[allow(dead_code)]
pub(crate) const KIND_ROTATE: i32 = 1;
/// `RE_RotatePic2`: `x`, `y` is the centre the picture turns about.
pub(crate) const KIND_ROTATE2: i32 = 2;

/// How a picture mixes with what is under it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Blend {
    Alpha,
    Add,
    /// `GL_DST_COLOR GL_ZERO`: darkens what is under it.
    Filter,
    /// `GL_ONE GL_ZERO`.
    Opaque,
}

impl Blend {
    const ALL: [Self; 4] = [Self::Alpha, Self::Add, Self::Filter, Self::Opaque];

    fn of(stage: Option<&StageBlend>) -> Self {
        match stage {
            None | Some(StageBlend::Alpha) => Self::Alpha,
            Some(StageBlend::Add) => Self::Add,
            Some(StageBlend::Filter) => Self::Filter,
            Some(StageBlend::Replace) => Self::Alpha,
            Some(StageBlend::Custom {
                source,
                destination,
            }) => {
                let source = source.to_ascii_uppercase();
                let destination = destination.to_ascii_uppercase();
                match (source.as_str(), destination.as_str()) {
                    ("GL_ONE", "GL_ONE") | ("GL_SRC_ALPHA", "GL_ONE") => Self::Add,
                    ("GL_DST_COLOR", "GL_ZERO") | ("GL_ZERO", "GL_SRC_COLOR") => Self::Filter,
                    ("GL_ONE", "GL_ZERO") => Self::Opaque,
                    _ => Self::Alpha,
                }
            }
        }
    }

    fn state(self) -> wgpu::BlendState {
        let component = |src_factor, dst_factor| wgpu::BlendComponent {
            src_factor,
            dst_factor,
            operation: wgpu::BlendOperation::Add,
        };
        use wgpu::BlendFactor as F;
        match self {
            Self::Alpha => wgpu::BlendState::ALPHA_BLENDING,
            Self::Add => wgpu::BlendState {
                color: component(F::SrcAlpha, F::One),
                alpha: wgpu::BlendComponent::OVER,
            },
            Self::Filter => wgpu::BlendState {
                color: component(F::Dst, F::Zero),
                alpha: component(F::Zero, F::One),
            },
            Self::Opaque => wgpu::BlendState::REPLACE,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|blend| *blend == self).unwrap_or(0)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

/// What a stage samples: one image, an `animMap` cycle or a `videoMap` video.
enum Images {
    Still(Rc<wgpu::BindGroup>),
    Cycle {
        frames: Vec<Rc<wgpu::BindGroup>>,
        frequency: f32,
        one_shot: bool,
    },
    Video(Box<Video>),
}

/// A RoQ video playing in a stage, looping at its own rate (`CIN_loop | CIN_shader`).
struct Video {
    roq: crate::cinematic_roq::Roq,
    texture: wgpu::Texture,
    bind_group: Rc<wgpu::BindGroup>,
    started: f32,
    pixels: Vec<u8>,
}

/// One stage of a picture's shader, drawn as rd-vanilla's 2D path draws it: its
/// image, blend, texture-coordinate modifiers and colour generators.
struct Stage {
    images: Images,
    blend: Blend,
    modifications: Vec<sjk_shader::TextureModification>,
    colour: sjk_shader::StageColour,
    rgb_wave: Option<sjk_shader::WaveForm>,
    alpha_wave: Option<sjk_shader::WaveForm>,
    rgb_constant: Option<[f32; 3]>,
    alpha_constant: Option<f32>,
}

/// A loaded picture: its shader's stages (an image alone is one blended stage).
struct Loaded {
    stages: Vec<Stage>,
}

use std::rc::Rc;

/// Consecutive pictures with one image and one blend.
struct Batch {
    bind_group: std::rc::Rc<wgpu::BindGroup>,
    blend: Blend,
    vertices: std::ops::Range<u32>,
}

/// Pipelines, the picture cache and this frame's quads.
pub(crate) struct Layer {
    device: wgpu::Device,
    pipelines: Vec<wgpu::RenderPipeline>,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    clamp_sampler: wgpu::Sampler,
    white: std::rc::Rc<wgpu::BindGroup>,
    /// Images by path and clamping, shared between the shaders using them.
    images: HashMap<(String, bool), Rc<wgpu::BindGroup>>,
    epoch: std::time::Instant,
    buffer: wgpu::Buffer,
    capacity: usize,
    names: HashMap<i32, String>,
    loaded: HashMap<i32, Loaded>,
    vertices: Vec<Vertex>,
    batches: Vec<Batch>,
}

impl Layer {
    /// `format` is the 2D target's format ([`crate::ui_target::format`]).
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("host 2D pictures"),
            entries: &[
                crate::render_helpers::texture_layout_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("host 2D pictures"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // The scope's program is exactly a textured, vertex-coloured quad.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("host 2D"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../scope.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("host 2D"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipelines = Blend::ALL
            .iter()
            .map(|blend| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("host 2D"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &ATTRIBUTES,
                        })],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(blend.state()),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .collect();
        let clamp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("host 2D clamped pictures"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white_image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]));
        let white_view =
            crate::gpu_texture::upload_display_image(device, queue, "host 2D white", &white_image);
        let white = std::rc::Rc::new(bind(device, &layout, &sampler, &white_view));
        let capacity = 4096;
        Self {
            device: device.clone(),
            pipelines,
            layout,
            sampler,
            clamp_sampler,
            white,
            images: HashMap::new(),
            epoch: std::time::Instant::now(),
            buffer: vertex_buffer(device, capacity),
            capacity,
            names: HashMap::new(),
            loaded: HashMap::new(),
            vertices: Vec::with_capacity(capacity),
            batches: Vec::new(),
        }
    }

    /// The engine registered `name` as shader `handle`.
    pub(crate) fn name(&mut self, handle: i32, name: String) {
        if self.names.get(&handle) != Some(&name) {
            self.loaded.remove(&handle);
            self.names.insert(handle, name);
        }
    }

    /// Build this frame's quads from `pictures`, loading pictures first used.
    pub(crate) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        pictures: &[Picture],
    ) {
        self.vertices.clear();
        self.batches.clear();
        let now = self.epoch.elapsed().as_secs_f32();
        for picture in pictures {
            if picture.color[3] <= 0.0 && picture.w != 0.0 {
                continue;
            }
            self.load(queue, vfs, shaders, picture.shader);
            let Some(loaded) = self.loaded.get_mut(&picture.shader) else {
                continue;
            };
            for stage in &mut loaded.stages {
                let bind_group = stage.images.frame(queue, now);
                let blend = stage.blend;
                let start = self.vertices.len() as u32;
                push_quad(
                    &mut self.vertices,
                    picture,
                    &stage.modifications,
                    stage_colour(stage, picture.color, now),
                    now,
                );
                let end = self.vertices.len() as u32;
                match self.batches.last_mut() {
                    Some(last)
                        if last.blend == blend
                            && Rc::ptr_eq(&last.bind_group, &bind_group)
                            && last.vertices.end == start =>
                    {
                        last.vertices.end = end;
                    }
                    _ => self.batches.push(Batch {
                        bind_group,
                        blend,
                        vertices: start..end,
                    }),
                }
            }
        }
        if self.vertices.len() > self.capacity {
            self.capacity = self.vertices.len().next_power_of_two();
            self.buffer = vertex_buffer(&self.device, self.capacity);
        }
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.vertices));
        }
    }

    /// Load shader `handle`'s stages the first time it is drawn.
    fn load(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        handle: i32,
    ) {
        if self.loaded.contains_key(&handle) {
            return;
        }
        let name = self.names.get(&handle).cloned().unwrap_or_default();
        let still = |images: Images| Stage {
            images,
            blend: Blend::Alpha,
            modifications: Vec::new(),
            colour: sjk_shader::StageColour {
                rgb: sjk_shader::RgbGen::Vertex,
                alpha: sjk_shader::AlphaGen::Vertex,
            },
            rgb_wave: None,
            alpha_wave: None,
            rgb_constant: None,
            alpha_constant: None,
        };
        let mut stages = Vec::new();
        if name.is_empty() || name.eq_ignore_ascii_case("white") {
            stages.push(still(Images::Still(self.white.clone())));
        } else if let Some(definition) = shaders.get(&name).filter(|definition| !definition.stages.is_empty()) {
            for stage in &definition.stages {
                let Some(images) = self.stage_images(queue, vfs, shaders, stage) else {
                    continue;
                };
                stages.push(Stage {
                    images,
                    blend: Blend::of(Some(&stage.blend)),
                    modifications: stage.texture_modifications.clone(),
                    colour: stage.resolved_colour,
                    rgb_wave: stage.rgb_wave.clone(),
                    alpha_wave: stage.alpha_wave.clone(),
                    rgb_constant: stage.rgb_constant,
                    alpha_constant: stage.alpha_constant,
                });
            }
        } else {
            // An image without a script: rd-vanilla's implicit 2D shader, one stage
            // blended by its alpha and coloured by the vertex colour.
            let image = shaders
                .resolve_image(vfs, &name)
                .ok()
                .flatten()
                .and_then(|path| self.image(queue, vfs, path.as_str(), false));
            stages.push(still(Images::Still(image.unwrap_or_else(|| self.white.clone()))));
        }
        self.loaded.insert(handle, Loaded { stages });
    }

    /// A stage's images; `None` for a stage this 2D path does not draw (a lightmap).
    fn stage_images(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        stage: &sjk_shader::ShaderStage,
    ) -> Option<Images> {
        if let Some(video) = &stage.video {
            return Some(
                self.video(queue, vfs, video, stage.clamp)
                    .map_or_else(|| Images::Still(self.white.clone()), |video| Images::Video(Box::new(video))),
            );
        }
        let mut frames = Vec::new();
        for image in &stage.images {
            let lower = image.to_ascii_lowercase();
            if lower == "$lightmap" || lower == "*lightmap" {
                return None;
            }
            if lower == "$whiteimage" || lower == "*white" {
                frames.push(self.white.clone());
                continue;
            }
            let resolved = shaders.resolve_stage_image(vfs, image).ok().flatten();
            frames.push(
                resolved
                    .and_then(|path| self.image(queue, vfs, path.as_str(), stage.clamp))
                    .unwrap_or_else(|| self.white.clone()),
            );
        }
        match frames.len() {
            0 => Some(Images::Still(self.white.clone())),
            1 => Some(Images::Still(frames.remove(0))),
            _ => Some(Images::Cycle {
                frames,
                frequency: stage.animation_frequency.unwrap_or(1.0),
                one_shot: stage.one_shot,
            }),
        }
    }

    fn image(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        path: &str,
        clamp: bool,
    ) -> Option<Rc<wgpu::BindGroup>> {
        let key = (path.to_owned(), clamp);
        if let Some(bind_group) = self.images.get(&key) {
            return Some(bind_group.clone());
        }
        let asset = vfs.read(path).ok().flatten()?;
        let rgba = crate::decode_image(&asset.bytes, path).ok()?.into_rgba8();
        let view = crate::gpu_texture::upload_display_image(&self.device, queue, path, &rgba);
        let sampler = if clamp { &self.clamp_sampler } else { &self.sampler };
        let bind_group = Rc::new(bind(&self.device, &self.layout, sampler, &view));
        self.images.insert(key, bind_group.clone());
        Some(bind_group)
    }

    fn video(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        argument: &str,
        clamp: bool,
    ) -> Option<Video> {
        let path = crate::world_materials::videos::path(argument);
        let asset = vfs.read(&path).ok().flatten()?;
        let mut roq = crate::cinematic_roq::Roq::open(asset.bytes.into()).ok()?;
        if !roq.advance() {
            return None;
        }
        let (width, height) = (roq.width(), roq.height());
        let mut pixels = vec![0; (width * height * 4) as usize];
        roq.write_rgba(&mut pixels);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&path),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        upload_video(queue, &texture, width, height, &pixels);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = if clamp { &self.clamp_sampler } else { &self.sampler };
        let bind_group = Rc::new(bind(&self.device, &self.layout, sampler, &view));
        Some(Video {
            roq,
            texture,
            bind_group,
            started: self.epoch.elapsed().as_secs_f32(),
            pixels,
        })
    }

    /// Draw the prepared quads over `target`.
    pub(crate) fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if self.batches.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("host 2D layer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for batch in &self.batches {
            pass.set_pipeline(&self.pipelines[batch.blend.index()]);
            pass.set_bind_group(0, batch.bind_group.as_ref(), &[]);
            pass.draw(batch.vertices.clone(), 0..1);
        }
    }
}

impl Images {
    /// The image to draw at `now` seconds, decoding and uploading a video's frames
    /// up to then.
    fn frame(&mut self, queue: &crate::frame_queue::FrameQueue, now: f32) -> Rc<wgpu::BindGroup> {
        match self {
            Self::Still(bind_group) => bind_group.clone(),
            Self::Cycle {
                frames,
                frequency,
                one_shot,
            } => {
                let index = (now * *frequency).max(0.0) as usize;
                let index = if *one_shot {
                    index.min(frames.len() - 1)
                } else {
                    index % frames.len()
                };
                frames[index].clone()
            }
            Self::Video(video) => {
                let wanted = ((now - video.started) * video.roq.fps().max(1.0)) as u64 + 1;
                let mut steps = 0;
                while video.roq.frames() < wanted && steps < 6 {
                    if !video.roq.advance() {
                        break;
                    }
                    steps += 1;
                }
                if steps > 0 {
                    video.roq.write_rgba(&mut video.pixels);
                    upload_video(
                        queue,
                        &video.texture,
                        video.roq.width(),
                        video.roq.height(),
                        &video.pixels,
                    );
                }
                if video.roq.frames() < wanted {
                    // A long gap (a hitch): carry on from here rather than catch up.
                    video.started = now - video.roq.frames() as f32 / video.roq.fps().max(1.0);
                }
                video.bind_group.clone()
            }
        }
    }
}

fn upload_video(
    queue: &crate::frame_queue::FrameQueue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    pixels: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// The vertex colour rd-vanilla's `ComputeColors` gives a 2D stage: `vertex` is the
/// engine's `RE_SetColor`, `identity` full white, waves at the 2D clock.
fn stage_colour(stage: &Stage, vertex: [f32; 4], now: f32) -> [f32; 4] {
    use sjk_shader::{AlphaGen, RgbGen};
    let wave = |wave: &Option<sjk_shader::WaveForm>| {
        wave.as_ref()
            .map_or(1.0, |wave| crate::world_stage::evaluate_wave(wave, now).clamp(0.0, 1.0))
    };
    let rgb = match stage.colour.rgb {
        RgbGen::Vertex | RgbGen::ExactVertex => [vertex[0], vertex[1], vertex[2]],
        RgbGen::OneMinusVertex => [1.0 - vertex[0], 1.0 - vertex[1], 1.0 - vertex[2]],
        RgbGen::Waveform => [wave(&stage.rgb_wave); 3],
        RgbGen::Const => stage.rgb_constant.unwrap_or([1.0; 3]),
        _ => [1.0; 3],
    };
    let alpha = match stage.colour.alpha {
        AlphaGen::Vertex => vertex[3],
        AlphaGen::OneMinusVertex => 1.0 - vertex[3],
        AlphaGen::Waveform => wave(&stage.alpha_wave),
        AlphaGen::Const => stage.alpha_constant.unwrap_or(1.0),
        _ => 1.0,
    };
    [rgb[0], rgb[1], rgb[2], alpha]
}

/// rd-vanilla's tcMods for one texture coordinate at `now` seconds (`RB_CalcScrollTexCoords`
/// and the rest, `tr_shade_calc.cpp`); `turb` and `entityTranslate` leave it as it is.
fn modify(modifications: &[sjk_shader::TextureModification], [mut s, mut t]: [f32; 2], now: f32) -> [f32; 2] {
    for modification in modifications {
        let a = |i: usize| modification.arguments.get(i).copied().unwrap_or(0.0);
        match modification.kind.to_ascii_lowercase().as_str() {
            "scroll" => {
                s += (a(0) * now).fract();
                t += (a(1) * now).fract();
            }
            "scale" => {
                s *= a(0);
                t *= a(1);
            }
            "rotate" => {
                let (sine, cosine) = (-a(0) * now).to_radians().sin_cos();
                let (s0, t0) = (s, t);
                s = s0 * cosine - t0 * sine + (0.5 - 0.5 * cosine + 0.5 * sine);
                t = s0 * sine + t0 * cosine + (0.5 - 0.5 * sine - 0.5 * cosine);
            }
            "stretch" => {
                let value = modification
                    .wave
                    .as_ref()
                    .map_or(1.0, |wave| crate::world_stage::evaluate_wave(wave, now));
                let p = if value.abs() > 1e-6 { 1.0 / value } else { 1.0 };
                s = s * p + (0.5 - 0.5 * p);
                t = t * p + (0.5 - 0.5 * p);
            }
            "transform" => {
                let (s0, t0) = (s, t);
                s = s0 * a(0) + t0 * a(2) + a(4);
                t = s0 * a(1) + t0 * a(3) + a(5);
            }
            _ => {}
        }
    }
    [s, t]
}

fn vertex_buffer(device: &wgpu::Device, vertices: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("host 2D quads"),
        size: (vertices * std::mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("host 2D picture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Two triangles for `picture` drawn with one stage, in clip space of the 640x480
/// virtual screen.
fn push_quad(
    vertices: &mut Vec<Vertex>,
    picture: &Picture,
    modifications: &[sjk_shader::TextureModification],
    color: [f32; 4],
    now: f32,
) {
    let Picture {
        x,
        y,
        w,
        h,
        s1,
        t1,
        s2,
        t2,
        angle,
        kind,
        ..
    } = *picture;
    // Corners: top-left, top-right, bottom-right, bottom-left.
    let mut corners = match kind {
        // `RB_RotatePic2`: x, y is the centre.
        KIND_ROTATE2 => [
            [-w * 0.5, -h * 0.5],
            [w * 0.5, -h * 0.5],
            [w * 0.5, h * 0.5],
            [-w * 0.5, h * 0.5],
        ],
        _ => [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
    };
    if kind != KIND_STRETCH && angle != 0.0 {
        // Positive angles turn counter-clockwise on screen, as `RB_RotatePic` builds
        // its matrix from `-a`.
        let (sine, cosine) = (-angle).to_radians().sin_cos();
        for corner in &mut corners {
            let [cx, cy] = *corner;
            *corner = [cx * cosine - cy * sine, cx * sine + cy * cosine];
        }
    }
    let uvs = [[s1, t1], [s2, t1], [s2, t2], [s1, t2]].map(|uv| modify(modifications, uv, now));
    let to_clip = |[cx, cy]: [f32; 2]| [(x + cx) / 320.0 - 1.0, 1.0 - (y + cy) / 240.0];
    for index in [0, 1, 2, 0, 2, 3] {
        vertices.push(Vertex {
            position: to_clip(corners[index]),
            uv: uvs[index],
            color,
        });
    }
}
