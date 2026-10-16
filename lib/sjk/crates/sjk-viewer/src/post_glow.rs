//! Dynamic glow: blur the frame's glowing stages and let the final resolve add them.
//!
//! Stock Jedi Academy draws every shader stage marked `glow` a second time into a
//! black image sharing the scene's depth, blurs that image and adds it to the frame
//! (`r_DynamicGlow`). [`crate::world_materials`] (`world_glow.rs`) and the effect
//! passes fill [`Glow`]'s 8-bit image with display values; this module blurs it and the
//! resolve (`post_aa.wgsl`, `with_glow`) adds the result after the effect layer and
//! before the `r_gamma` ramp. Two blurs:
//!
//! - [`Style::Vulkan`] (`r_dynamicGlowStyle 1`, default): EternalJK's rd-vulkan, which
//!   is what most players see. A four-level pyramid at 1/2 … 1/16 of the frame, each
//!   level a three-tap horizontal then vertical blur of the previous one with taps
//!   ±1.2 texels apart and each weight raised by 0.15 (`blur.frag`, `vk_pipelines.cpp`
//!   :1713, :1772-1773), the levels summed and scaled by `r_DynamicGlowIntensity - 1`
//!   (`blend.frag`, `vk_pipelines.cpp:1519-1523`) and added. Every level is an 8-bit,
//!   clamped image as rd-vulkan's are. rd-vulkan ignores the other glow settings.
//! - [`Style::Retail`] (`r_dynamicGlowStyle 0`): rd-vanilla's `RB_BlurGlowTexture`
//!   (`tr_backend.cpp:2271-2445`). `r_DynamicGlowPasses` passes at `r_DynamicGlowScale`
//!   of the frame (or `r_DynamicGlowWidth` × `r_DynamicGlowHeight`), each summing four
//!   diagonal taps ±(0.1 + pass · `r_DynamicGlowDelta`) texels away, weighted
//!   `r_DynamicGlowIntensity` / 4 and clamped by the 8-bit target; the first pass reads
//!   the full-size image. `r_DynamicGlowSoft 1` composites with `GL_ONE,
//!   GL_ONE_MINUS_SRC_COLOR` (`RB_DrawGlowOverlay`, `tr_backend.cpp:2448-2520`).
//!
//! Every resource is created with the resolve (startup, resize, policy change); a frame
//! records at most the passes above, and none when nothing glowed.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::cell::Cell;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Upper bound on `r_DynamicGlowPasses`; stock has none, but each pass is a full
/// draw of the blur image.
pub(crate) const MAX_PASSES: u32 = 32;
/// rd-vulkan `VK_NUM_BLUR_PASSES` (`vk_local.h:76`).
const LEVELS: usize = 4;
/// Format of the glow image and every blur image: stock's 8-bit framebuffer.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Which blur turns the glow image into the halo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Style {
    /// rd-vanilla's multi-pass diagonal kernel.
    Retail,
    /// rd-vulkan's four-level pyramid.
    Vulkan,
}

/// What the glow resources depend on; a change rebuilds the resolve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Policy {
    /// `r_DynamicGlow` is nonzero.
    pub(crate) enabled: bool,
    pub(crate) style: Style,
    /// `r_DynamicGlowWidth` / `Height`, used when both are positive (retail style).
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// `r_DynamicGlowScale` of the frame otherwise (retail style).
    pub(crate) scale: f32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: true,
            style: Style::Vulkan,
            width: 0,
            height: 0,
            scale: 0.25,
        }
    }
}

/// What may change every frame without new resources.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Live {
    pub(crate) mode: Mode,
    /// `r_DynamicGlowPasses`, 1..=[`MAX_PASSES`].
    pub(crate) passes: u32,
    pub(crate) delta: f32,
    pub(crate) intensity: f32,
    pub(crate) soft: bool,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            mode: Mode::On,
            passes: 5,
            delta: 0.8,
            intensity: 1.13,
            soft: true,
        }
    }
}

/// `r_DynamicGlow` (JoF EternalJK's meanings for 2 and 3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Off,
    On,
    /// Only saber blades (`RT_SABER_GLOW` entities, `tr_backend.cpp:767-771`).
    Sabers,
    /// Debug: the blurred glow alone, without the scene (`tr_backend.cpp:2471`).
    GlowOnly,
}

impl Mode {
    /// Stock treats any nonzero value other than the two special ones as on.
    pub(crate) fn from_cvar(value: i64) -> Self {
        match value {
            0 => Self::Off,
            2 => Self::Sabers,
            3 => Self::GlowOnly,
            _ => Self::On,
        }
    }
}

/// Retained cvar values shared with console-free world installs.
#[derive(Clone)]
pub(crate) struct Settings(Arc<[AtomicU32; SLOTS]>);

const SLOTS: usize = 9;
/// `(name, default, slot, description)`; integers and floats are stored as `f32` bits.
const CVARS: [(&str, Seed, usize, &str); SLOTS] = [
    (
        "r_DynamicGlow",
        Seed::Integer(1),
        0,
        "Dynamic glow: 0 off, 1 on, 2 sabers only, 3 glow only (debug); applies immediately",
    ),
    (
        "r_DynamicGlowPasses",
        Seed::Integer(5),
        1,
        "Retail glow blur passes (r_dynamicGlowStyle 0), 1-32",
    ),
    (
        "r_DynamicGlowDelta",
        Seed::Float(0.8),
        2,
        "Retail glow tap spread added per pass, in blur texels",
    ),
    (
        "r_DynamicGlowIntensity",
        Seed::Float(1.13),
        3,
        "Glow strength: retail per-pass gain, Vulkan style (Intensity - 1) x levels",
    ),
    (
        "r_DynamicGlowSoft",
        Seed::Integer(1),
        4,
        "Retail glow composite: 1 soft (screen), 0 additive",
    ),
    (
        "r_DynamicGlowWidth",
        Seed::Integer(0),
        5,
        "Retail glow blur width; with Height, overrides r_DynamicGlowScale",
    ),
    (
        "r_DynamicGlowHeight",
        Seed::Integer(0),
        6,
        "Retail glow blur height; with Width, overrides r_DynamicGlowScale",
    ),
    (
        "r_DynamicGlowScale",
        Seed::Float(0.25),
        7,
        "Retail glow blur size as a fraction of the frame",
    ),
    (
        "r_dynamicGlowStyle",
        Seed::Integer(1),
        8,
        "Glow blur: 0 retail (rd-vanilla), 1 EternalJK Vulkan pyramid",
    ),
];

/// A cvar's registered default.
#[derive(Clone, Copy)]
enum Seed {
    Integer(i64),
    Float(f64),
}

impl Default for Settings {
    fn default() -> Self {
        Self(Arc::new(CVARS.map(|(_, seed, _, _)| {
            AtomicU32::new(seed.value().to_bits())
        })))
    }
}

impl Seed {
    fn value(self) -> f32 {
        match self {
            Self::Integer(value) => value as f32,
            Self::Float(value) => value as f32,
        }
    }
}

/// A numeric cvar value as `f32`; text and booleans are not glow values.
fn number(value: &CvarValue) -> Option<f32> {
    match value {
        CvarValue::Integer(value) => Some(*value as f32),
        CvarValue::Float(value) => Some(*value as f32),
        CvarValue::Bool(value) => Some(f32::from(u8::from(*value))),
        CvarValue::Text(_) => None,
    }
}

impl Settings {
    /// Register the retail glow cvars (archived, stock names and defaults except
    /// `r_DynamicGlow`, on here) and SJK's `r_dynamicGlowStyle`, and follow their edits.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        for (name, seed, slot, description) in CVARS {
            let default = match seed {
                Seed::Integer(value) => CvarValue::Integer(value),
                Seed::Float(value) => CvarValue::Float(value),
            };
            cvars.register(CvarDefinition::new(
                name,
                default,
                CvarFlags::ARCHIVE,
                description,
            ))?;
            let slots = settings.0.clone();
            cvars.on_change(name, move |change| {
                if let Some(value) = number(&change.current).filter(|value| value.is_finite()) {
                    slots[slot].store(value.to_bits(), Ordering::Relaxed);
                }
            })?;
        }
        Ok(settings)
    }

    fn value(&self, slot: usize) -> f32 {
        f32::from_bits(self.0[slot].load(Ordering::Relaxed))
    }

    fn mode(&self) -> Mode {
        Mode::from_cvar(self.value(0) as i64)
    }

    /// Resource policy; read without allocation or locking.
    pub(crate) fn policy(&self) -> Policy {
        Policy {
            enabled: self.mode() != Mode::Off,
            style: if self.value(8) as i64 == 0 {
                Style::Retail
            } else {
                Style::Vulkan
            },
            width: self.value(5).max(0.0) as u32,
            height: self.value(6).max(0.0) as u32,
            scale: self.value(7),
        }
    }

    /// Per-frame values; read without allocation or locking.
    pub(crate) fn live(&self) -> Live {
        Live {
            mode: self.mode(),
            passes: (self.value(1) as i64).clamp(1, i64::from(MAX_PASSES)) as u32,
            delta: self.value(2),
            intensity: self.value(3),
            soft: self.value(4) as i64 != 0,
        }
    }
}

/// Retail blur size: `r_DynamicGlowWidth` × `Height` when both are positive, else
/// `r_DynamicGlowScale` of the frame (`tr_image.cpp:1559-1568`), at most the frame
/// (`tr_image.cpp:1347-1354`) and at least one texel.
pub(crate) fn retail_size(frame: [u32; 2], policy: Policy) -> [u32; 2] {
    let size = if policy.width > 0 && policy.height > 0 {
        [policy.width, policy.height]
    } else {
        let scale = if policy.scale.is_finite() {
            policy.scale.max(0.0)
        } else {
            0.25
        };
        frame.map(|n| (n as f32 * scale) as u32)
    };
    [
        size[0].clamp(1, frame[0].max(1)),
        size[1].clamp(1, frame[1].max(1)),
    ]
}

/// rd-vulkan's pyramid: each level half the previous one, from half the frame
/// (`vk_attachments.cpp:326-334`), at least one texel.
pub(crate) fn vulkan_levels(frame: [u32; 2]) -> [[u32; 2]; LEVELS] {
    let mut size = frame;
    std::array::from_fn(|_| {
        size = size.map(|n| (n / 2).max(1));
        size
    })
}

/// Retail tap distance of pass `pass` in source texels: `fTexelWidthOffset` starts at
/// 0.1 and grows by `r_DynamicGlowDelta` after every pass (`tr_backend.cpp:2421-2422`).
/// `post_glow.wgsl` (`retail`) evaluates it per pass from the uploaded delta.
#[cfg(test)]
pub(crate) fn retail_offset(pass: u32, delta: f32) -> f32 {
    0.1 + pass as f32 * delta
}

/// Retail weight of each of the four taps, `r_DynamicGlowIntensity · 0.25`
/// (`tr_backend.cpp:2292`).
pub(crate) fn retail_weight(intensity: f32) -> f32 {
    intensity * 0.25
}

/// rd-vulkan's three taps, centre and either side: 6/16 and 5/16 each raised by the
/// dynamic glow's `correction` of 0.15 (`blur.frag`, `vk_pipelines.cpp:1713`).
pub(crate) const fn vulkan_weights() -> [f32; 2] {
    [6.0 / 16.0 + 0.15, 5.0 / 16.0 + 0.15]
}

/// rd-vulkan's tap distance in destination texels (`vk_pipelines.cpp:1772-1773`).
const VULKAN_OFFSET: f32 = 1.2;

/// rd-vulkan's level-sum factor, `r_DynamicGlowIntensity - 1` within [0.01, 4]
/// (`vk_pipelines.cpp:1519-1523`).
pub(crate) fn vulkan_factor(intensity: f32) -> f32 {
    (intensity - 1.0).clamp(0.01, 4.0)
}

/// The colour format the stage programs draw the glow image through. They write scene
/// colour, so an sRGB or floating scene draws through the image's sRGB view (display
/// values in the 8-bit image, as stock's framebuffer holds) and a scene that stores
/// display values already through the plain one.
///
/// Exposure hook: the image holds unexposed display values, which equal the scene's at
/// `r_hdrExposure 1`. A scene exposure (fixed or automatic) belongs on the world's
/// glowing stages before encoding here; effects are display values and take none.
pub(crate) fn world_format(scene: wgpu::TextureFormat) -> wgpu::TextureFormat {
    if scene.is_srgb()
        || matches!(
            scene,
            wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rg11b10Ufloat
        )
    {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        FORMAT
    }
}

/// Uniform block of `post_glow.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    /// Delta, retail tap weight, Vulkan level factor, unused.
    live: [f32; 4],
    /// Reciprocal retail blur size, Vulkan centre and side weights.
    sizes: [f32; 4],
    /// Vulkan blur passes then the level sum: uv tap offset, reciprocal target size.
    passes: [[f32; 4]; LEVELS * 2 + 1],
}

/// The blur passes of one style; every image is [`FORMAT`].
// One per resolve: the variants' sizes do not matter.
#[allow(clippy::large_enum_variant)]
enum Blur {
    Retail {
        pipeline: wgpu::RenderPipeline,
        /// Reads of the glow image, `ping` and `pong`.
        reads: [wgpu::BindGroup; 3],
        /// `ping`, `pong` and the output.
        targets: [wgpu::TextureView; 3],
    },
    Vulkan {
        blur: wgpu::RenderPipeline,
        combine: wgpu::RenderPipeline,
        /// Horizontal then vertical pass of every level: what it reads, where it writes.
        passes: [(wgpu::BindGroup, wgpu::TextureView); LEVELS * 2],
        combine_bind: wgpu::BindGroup,
    },
}

/// The glow image, its blur and the resolve's view of the result.
pub(crate) struct Glow {
    size: [u32; 2],
    /// The image through [`world_format`], for the stage programs.
    world: wgpu::TextureView,
    /// The image as display values, for effect pipelines (`effect_layer::FORMAT`).
    effects: wgpu::TextureView,
    blur: Blur,
    /// The blurred glow the resolve adds.
    output: wgpu::TextureView,
    params: wgpu::Buffer,
    params_value: Cell<Params>,
    /// Uniform of the resolve: drawn, soft, glow only, unused.
    controls: wgpu::Buffer,
    controls_value: Cell<[f32; 4]>,
    live: Cell<Live>,
    drawn: Cell<bool>,
}

impl Glow {
    /// Allocate the glow image at `scene` (the depth target's size) and the blur for
    /// `frame` (the window); startup, resize and policy changes only.
    pub(crate) fn new(
        device: &wgpu::Device,
        scene: [u32; 2],
        frame: [u32; 2],
        scene_format: wgpu::TextureFormat,
        policy: Policy,
    ) -> Self {
        let scene = scene.map(|n| n.max(1));
        let frame = frame.map(|n| n.max(1));
        let world_format = world_format(scene_format);
        let image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dynamic glow image"),
            size: extent(scene),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: if world_format == FORMAT {
                &[]
            } else {
                &[wgpu::TextureFormat::Rgba8UnormSrgb]
            },
        });
        let effects = image.create_view(&Default::default());
        let world = image.create_view(&wgpu::TextureViewDescriptor {
            format: Some(world_format),
            ..Default::default()
        });
        let target = |label, size: [u32; 2]| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent(size),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Dynamic glow blur"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post_glow.wgsl").into()),
        });
        let pipeline = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(FORMAT.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Dynamic glow linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut values = Params {
            live: [0.0; 4],
            sizes: [0.0; 4],
            passes: [[0.0; 4]; LEVELS * 2 + 1],
        };
        let [center, side] = vulkan_weights();
        values.sizes[2] = center;
        values.sizes[3] = side;
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic glow blur parameters"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = |pipeline: &wgpu::RenderPipeline, views: &[&wgpu::TextureView]| {
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ];
            for (view, binding) in views.iter().zip([0, 3, 4, 5]) {
                entries.push(wgpu::BindGroupEntry {
                    binding,
                    resource: wgpu::BindingResource::TextureView(view),
                });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Dynamic glow blur input"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &entries,
            })
        };
        let (blur, output) = match policy.style {
            Style::Retail => {
                let size = retail_size(frame, policy);
                values.sizes[0] = 1.0 / size[0] as f32;
                values.sizes[1] = 1.0 / size[1] as f32;
                let pipeline = pipeline("retail");
                let targets = [
                    target("Dynamic glow blur A", size),
                    target("Dynamic glow blur B", size),
                    target("Dynamic glow blur output", size),
                ];
                let reads = [
                    bind(&pipeline, &[&effects]),
                    bind(&pipeline, &[&targets[0]]),
                    bind(&pipeline, &[&targets[1]]),
                ];
                let output = targets[2].clone();
                (
                    Blur::Retail {
                        pipeline,
                        reads,
                        targets,
                    },
                    output,
                )
            }
            Style::Vulkan => {
                let levels = vulkan_levels(frame);
                let blur = pipeline("vulkan_blur");
                let combine = pipeline("vulkan_combine");
                let horizontal = levels.map(|size| target("Dynamic glow level, horizontal", size));
                let vertical = levels.map(|size| target("Dynamic glow level", size));
                let passes = std::array::from_fn(|index| {
                    let level = index / 2;
                    let size = levels[level].map(|n| n as f32);
                    let reciprocal = [1.0 / size[0], 1.0 / size[1]];
                    if index % 2 == 0 {
                        values.passes[index] = [
                            VULKAN_OFFSET * reciprocal[0],
                            0.0,
                            reciprocal[0],
                            reciprocal[1],
                        ];
                        let source = if level == 0 {
                            &effects
                        } else {
                            &vertical[level - 1]
                        };
                        (bind(&blur, &[source]), horizontal[level].clone())
                    } else {
                        values.passes[index] = [
                            0.0,
                            VULKAN_OFFSET * reciprocal[1],
                            reciprocal[0],
                            reciprocal[1],
                        ];
                        (bind(&blur, &[&horizontal[level]]), vertical[level].clone())
                    }
                });
                let first = levels[0].map(|n| 1.0 / n as f32);
                values.passes[LEVELS * 2] = [0.0, 0.0, first[0], first[1]];
                let combine_bind = bind(
                    &combine,
                    &[&vertical[0], &vertical[1], &vertical[2], &vertical[3]],
                );
                // The level sum reuses the first horizontal image, read only before it.
                let output = horizontal[0].clone();
                (
                    Blur::Vulkan {
                        blur,
                        combine,
                        passes,
                        combine_bind,
                    },
                    output,
                )
            }
        };
        let controls = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic glow composite controls"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            size: scene,
            world,
            effects,
            blur,
            output,
            params,
            // Never equal to a real upload, so the first `set_live` writes the block.
            params_value: Cell::new(Params {
                live: [f32::NAN; 4],
                ..values
            }),
            controls,
            controls_value: Cell::new([0.0; 4]),
            live: Cell::new(Live::default()),
            drawn: Cell::new(false),
        }
    }

    /// The glow image's size; always the scene targets'.
    pub(crate) const fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The blurred glow, sampled by the resolve.
    pub(crate) const fn output(&self) -> &wgpu::TextureView {
        &self.output
    }

    /// The resolve's uniform: drawn, soft, glow only.
    pub(crate) const fn controls(&self) -> &wgpu::Buffer {
        &self.controls
    }

    /// This frame's live settings; 16 to 192 bytes written only when a value changed.
    pub(crate) fn set_live(&self, queue: &crate::frame_queue::FrameQueue, live: Live) {
        self.live.set(live);
        let mut params = self.params_value.get();
        params.live = [
            live.delta,
            retail_weight(live.intensity),
            vulkan_factor(live.intensity),
            0.0,
        ];
        if self.params_value.replace(params) != params {
            queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        }
    }

    /// Whether the main view drew glow this frame; `world_view` lets `r_DynamicGlow 3`
    /// show the glow alone, as stock does only for a world scene. 16 bytes, written only
    /// when a control changed.
    pub(crate) fn set_drawn(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        drawn: bool,
        world_view: bool,
    ) {
        self.drawn.set(drawn);
        let live = self.live.get();
        let soft = matches!(self.blur, Blur::Retail { .. }) && live.soft;
        let value = [
            f32::from(u8::from(drawn)),
            f32::from(u8::from(soft)),
            f32::from(u8::from(world_view && live.mode == Mode::GlowOnly)),
            0.0,
        ];
        if self.controls_value.replace(value) != value {
            queue.write_buffer(&self.controls, 0, bytemuck::cast_slice(&value));
        }
    }

    /// Begin drawing into the glow image: the stage programs through [`world_format`]
    /// (`world`), effect pipelines as display values otherwise. The first pass of a frame
    /// clears to black; the scene's depth is attached read-only.
    pub(crate) fn begin<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        world: bool,
        clear: bool,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Dynamic glow sources"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: if world { &self.world } else { &self.effects },
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Blur this frame's glow image; nothing when nothing glowed.
    pub(crate) fn blur(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.drawn.get() {
            return;
        }
        let draw = |encoder: &mut wgpu::CommandEncoder,
                    pipeline: &wgpu::RenderPipeline,
                    bind: &wgpu::BindGroup,
                    view: &wgpu::TextureView,
                    pass_index: u32| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Dynamic glow blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            // The instance index names the pass for the shader's offsets.
            pass.draw(0..3, pass_index..pass_index + 1);
        };
        match &self.blur {
            Blur::Retail {
                pipeline,
                reads,
                targets,
            } => {
                let passes = self.live.get().passes.clamp(1, MAX_PASSES);
                for pass in 0..passes {
                    let read = if pass == 0 {
                        &reads[0]
                    } else {
                        &reads[1 + (pass as usize - 1) % 2]
                    };
                    let target = if pass + 1 == passes {
                        &targets[2]
                    } else {
                        &targets[pass as usize % 2]
                    };
                    draw(encoder, pipeline, read, target, pass);
                }
            }
            Blur::Vulkan {
                blur,
                combine,
                passes,
                combine_bind,
            } => {
                for (index, (read, target)) in passes.iter().enumerate() {
                    draw(encoder, blur, read, target, index as u32);
                }
                draw(
                    encoder,
                    combine,
                    combine_bind,
                    &self.output,
                    (LEVELS * 2) as u32,
                );
            }
        }
    }
}

fn extent(size: [u32; 2]) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: size[0].max(1),
        height: size[1].max(1),
        depth_or_array_layers: 1,
    }
}

#[cfg(test)]
#[path = "post_glow_tests.rs"]
mod tests;
