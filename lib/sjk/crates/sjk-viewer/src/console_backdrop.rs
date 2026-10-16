//! The classic console's own 2D layer (`con_style classic`): the background
//! EternalJK draws from the `console` shader, the bar under it, selection
//! highlights and the console's text.
//!
//! `Con_DrawSolidConsole` stretches `cls.consoleShader` (`console`, from
//! `shaders/gfx.shader`) over the console with `DrawStretchPic`, so every
//! stage of that shader runs with its own blend, texture motion and colour.
//! The shader is read from the shader catalogue and its images through the
//! virtual file system, so a PK3 that overrides it (such as a cosmetic pack's
//! opaque picture with scrolling stars) wins exactly as it does in EternalJK.
//! Each stage keeps its `blendFunc` as a pipeline; `tcMod scroll`, `scale`,
//! `rotate`, `transform` and `stretch`, `rgbGen wave`/`const`/`vertex` and
//! `alphaGen vertex`/`wave`/`const` are evaluated here per frame (see
//! [`stage_corners`] and [`stage_colour`]); other generators draw at full
//! strength. `alphaGen vertex` takes the draw colour's alpha, which EternalJK
//! sets to `con_opacity` below full height and to 1 at full height. Without
//! the shader or its images the console falls back to a plain dark panel.
//!
//! The layer draws after every other 2D element, text included, and its text
//! last, so menu, chat and HUD text never shows through an opaque console. Its
//! text uses the console character set when it is loaded, else Inter, with the
//! shared text pipelines.
//!
//! The SJK UI's console (`con_style sjk`) draws on the
//! same layer without the shader: vertical and horizontal fades ([`Shade`]) under
//! the solid quads, and labels in the SJK UI's two families after the console's
//! own text, each family from its own atlas.

use crate::text::TextVertex;
use bytemuck::{Pod, Zeroable};
use sjk_shader::{AlphaGen, RgbGen, ShaderCatalog, StageBlend, TextureModification, WaveForm};
use sjk_vfs::VirtualFileSystem;
use wgpu::util::DeviceExt;

/// Shader the console background is drawn with (`cls.consoleShader`).
const SHADER: &str = "console";
/// Most stages drawn from the shader.
const MAX_STAGES: usize = 8;
/// Quads (bar, highlights, stages) per frame.
const MAX_QUADS: usize = 1_024;
/// Console text vertices per frame: a full-screen 4K console of dense text.
pub(crate) const MAX_TEXT_VERTICES: usize = 196_608;
/// Longest side kept for a background picture.
const MAX_SIDE: u32 = 4_096;
/// Background without the shader: a dark navy panel.
const FALLBACK: [f32; 3] = [0.03, 0.05, 0.12];

/// The background part of a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Background {
    /// Height of the background from the top of the screen, in pixels.
    pub(crate) height: f32,
    /// The draw colour's alpha: `con_opacity`, or 1 at full height.
    pub(crate) opacity: f32,
    /// Texture `t` at the top and bottom edge (`con_ratioFix`).
    pub(crate) t_range: [f32; 2],
}

/// A filled rectangle `[x, y, width, height]` in pixels, display colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SolidQuad {
    pub(crate) rect: [f32; 4],
    pub(crate) color: [f32; 4],
}

/// A rectangle `[x, y, width, height]` in pixels whose corners have their own
/// colours (top left, top right, bottom right, bottom left), blended across it:
/// the SJK UI's fades. Drawn after the background and before the quads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shade {
    pub(crate) rect: [f32; 4],
    pub(crate) corners: [[f32; 4]; 4],
}

impl Shade {
    /// `rect` fading from `top` at its top edge to `bottom` at its bottom edge.
    pub(crate) const fn vertical(rect: [f32; 4], top: [f32; 4], bottom: [f32; 4]) -> Self {
        Self {
            rect,
            corners: [top, top, bottom, bottom],
        }
    }

    /// `rect` fading from `left` at its left edge to `right` at its right edge.
    pub(crate) const fn horizontal(rect: [f32; 4], left: [f32; 4], right: [f32; 4]) -> Self {
        Self {
            rect,
            corners: [left, right, right, left],
        }
    }
}

/// Which atlas the frame's text was laid out with.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TextAtlas {
    /// The bundled Inter atlas.
    #[default]
    Inter,
    /// The console font ([`crate::text::console_font`]).
    Console,
}

/// What the console contributes to one frame of the layer.
#[derive(Default)]
pub(crate) struct ConsoleFrame {
    pub(crate) background: Option<Background>,
    /// Drawn over the background, in order, before the quads.
    pub(crate) shades: Vec<Shade>,
    /// Drawn over the shades, in order, before the text.
    pub(crate) quads: Vec<SolidQuad>,
    pub(crate) text: Vec<TextVertex>,
    pub(crate) atlas: TextAtlas,
    /// Text in the SJK UI's display (Rajdhani) and body (Exo 2) families, drawn
    /// after [`Self::text`] with their atlases; empty unless they are loaded.
    pub(crate) display_text: Vec<TextVertex>,
    pub(crate) body_text: Vec<TextVertex>,
}

impl ConsoleFrame {
    /// Empty the frame, keeping its storage.
    pub(crate) fn clear(&mut self) {
        self.background = None;
        self.shades.clear();
        self.quads.clear();
        self.text.clear();
        self.display_text.clear();
        self.body_text.clear();
    }
}

/// A texture coordinate change of a stage, in script order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TexMod {
    Scroll([f32; 2]),
    Scale([f32; 2]),
    /// Degrees per second.
    Rotate(f32),
    /// `s' = s*m00 + t*m10 + t0`, `t' = s*m01 + t*m11 + t1`.
    Transform([f32; 6]),
    Stretch(WaveForm),
}

impl TexMod {
    fn from_shader(modification: &TextureModification) -> Option<Self> {
        let arguments = &modification.arguments;
        let pair = || Some([*arguments.first()?, *arguments.get(1)?]);
        Some(match modification.kind.as_str() {
            "scroll" => Self::Scroll(pair()?),
            "scale" => Self::Scale(pair()?),
            "rotate" => Self::Rotate(*arguments.first()?),
            "transform" if arguments.len() >= 6 => Self::Transform([
                arguments[0],
                arguments[1],
                arguments[2],
                arguments[3],
                arguments[4],
                arguments[5],
            ]),
            "stretch" => Self::Stretch(modification.wave.clone()?),
            _ => return None,
        })
    }
}

/// A stage's colour source.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Channel<T> {
    /// Full strength (identity and the generators a 2D draw has no input for).
    One,
    /// The draw colour (`rgbGen vertex`, `alphaGen vertex`).
    Vertex,
    Wave(WaveForm),
    Const(T),
}

/// What a stage draws, apart from its GPU resources.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StageProgram {
    pub(crate) modifications: Vec<TexMod>,
    pub(crate) rgb: Channel<[f32; 3]>,
    pub(crate) alpha: Channel<f32>,
    pub(crate) blend: wgpu::BlendState,
    pub(crate) clamp: bool,
}

impl StageProgram {
    pub(crate) fn from_shader(stage: &sjk_shader::ShaderStage) -> Self {
        let rgb = match stage.resolved_colour.rgb {
            RgbGen::Waveform => stage.rgb_wave.clone().map_or(Channel::One, Channel::Wave),
            RgbGen::Const => stage.rgb_constant.map_or(Channel::One, Channel::Const),
            RgbGen::Vertex | RgbGen::ExactVertex => Channel::Vertex,
            _ => Channel::One,
        };
        let alpha = match stage.resolved_colour.alpha {
            AlphaGen::Vertex => Channel::Vertex,
            AlphaGen::Waveform => stage.alpha_wave.clone().map_or(Channel::One, Channel::Wave),
            AlphaGen::Const => stage.alpha_constant.map_or(Channel::One, Channel::Const),
            _ => Channel::One,
        };
        Self {
            modifications: stage
                .texture_modifications
                .iter()
                .filter_map(TexMod::from_shader)
                .collect(),
            rgb,
            alpha,
            blend: blend_state(&stage.blend),
            clamp: stage.clamp,
        }
    }
}

/// The GL blend of a stage as a wgpu blend state; the colour target's alpha is
/// blended "over" as the other 2D pipelines do.
pub(crate) fn blend_state(blend: &StageBlend) -> wgpu::BlendState {
    use wgpu::BlendFactor as F;
    let (source, destination) = match blend {
        StageBlend::Replace => return wgpu::BlendState::REPLACE,
        StageBlend::Add => (F::One, F::One),
        StageBlend::Filter => (F::Dst, F::Zero),
        StageBlend::Alpha => (F::SrcAlpha, F::OneMinusSrcAlpha),
        StageBlend::Custom {
            source,
            destination,
        } => (gl_factor(source, F::One), gl_factor(destination, F::Zero)),
    };
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: source,
            dst_factor: destination,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent::OVER,
    }
}

fn gl_factor(name: &str, unknown: wgpu::BlendFactor) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as F;
    match name {
        "gl_zero" => F::Zero,
        "gl_one" => F::One,
        "gl_src_color" => F::Src,
        "gl_one_minus_src_color" => F::OneMinusSrc,
        "gl_dst_color" => F::Dst,
        "gl_one_minus_dst_color" => F::OneMinusDst,
        "gl_src_alpha" => F::SrcAlpha,
        "gl_one_minus_src_alpha" => F::OneMinusSrcAlpha,
        "gl_dst_alpha" => F::DstAlpha,
        "gl_one_minus_dst_alpha" => F::OneMinusDstAlpha,
        "gl_src_alpha_saturate" => F::SrcAlphaSaturated,
        _ => unknown,
    }
}

/// Texture coordinates of the four corners (top-left, top-right, bottom-right,
/// bottom-left) of a quad drawn with `t` from `t_range[0]` at the top to
/// `t_range[1]` at the bottom, after `modifications` at `seconds`, in order, as
/// `RB_CalcScrollTexCoords`, `RB_CalcScaleTexCoords` and the rest apply them
/// (`tr_shade_calc.cpp`): scroll adds `speed * time` without its whole part,
/// scale multiplies.
pub(crate) fn stage_corners(
    modifications: &[TexMod],
    t_range: [f32; 2],
    seconds: f64,
) -> [[f32; 2]; 4] {
    let mut corners = [
        [0.0, t_range[0]],
        [1.0, t_range[0]],
        [1.0, t_range[1]],
        [0.0, t_range[1]],
    ];
    for modification in modifications {
        for corner in &mut corners {
            *corner = modify(modification, *corner, seconds);
        }
    }
    corners
}

fn modify(modification: &TexMod, [s, t]: [f32; 2], seconds: f64) -> [f32; 2] {
    match modification {
        TexMod::Scroll(speed) => {
            let offset = |speed: f32| {
                let value = f64::from(speed) * seconds;
                (value - value.floor()) as f32
            };
            [s + offset(speed[0]), t + offset(speed[1])]
        }
        TexMod::Scale(scale) => [s * scale[0], t * scale[1]],
        TexMod::Rotate(degrees) => {
            let angle = (-f64::from(*degrees) * seconds)
                .rem_euclid(360.0)
                .to_radians();
            let (sin, cos) = (angle.sin() as f32, angle.cos() as f32);
            [
                s * cos - t * sin + (0.5 - 0.5 * cos + 0.5 * sin),
                s * sin + t * cos + (0.5 - 0.5 * sin - 0.5 * cos),
            ]
        }
        TexMod::Transform([m00, m01, m10, m11, t0, t1]) => {
            [s * m00 + t * m10 + t0, s * m01 + t * m11 + t1]
        }
        TexMod::Stretch(wave) => {
            let value = crate::world_stage::evaluate_wave(wave, seconds as f32);
            let p = if value == 0.0 { 1.0 } else { 1.0 / value };
            let offset = 0.5 - 0.5 * p;
            [s * p + offset, t * p + offset]
        }
    }
}

/// A stage's vertex colour at `seconds` for a draw colour of white with
/// `opacity` alpha: waves are clamped to `[0, 1]` as `RB_CalcWaveColor` and
/// `RB_CalcWaveAlpha` do.
pub(crate) fn stage_colour(program: &StageProgram, opacity: f32, seconds: f64) -> [f32; 4] {
    let wave =
        |wave: &WaveForm| crate::world_stage::evaluate_wave(wave, seconds as f32).clamp(0.0, 1.0);
    let rgb = match &program.rgb {
        Channel::One | Channel::Vertex => [1.0; 3],
        Channel::Wave(form) => [wave(form); 3],
        Channel::Const(value) => *value,
    };
    let alpha = match &program.alpha {
        Channel::One => 1.0,
        Channel::Vertex => opacity,
        Channel::Wave(form) => wave(form),
        Channel::Const(value) => *value,
    };
    [rgb[0], rgb[1], rgb[2], alpha]
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

/// One loaded stage.
struct Stage {
    program: StageProgram,
    pipeline: usize,
    texture: wgpu::BindGroup,
}

/// A run of quads drawn with one pipeline and texture.
#[derive(Clone)]
struct Run {
    vertices: std::ops::Range<u32>,
    /// A stage index, or `None` for the white texture with alpha blending.
    stage: Option<usize>,
}

/// Text pipelines and atlases the layer's text can be drawn with.
pub(crate) struct TextDraw<'a> {
    pub(crate) pipeline: &'a wgpu::RenderPipeline,
    pub(crate) sdf_pipeline: &'a wgpu::RenderPipeline,
    /// The Inter atlas.
    pub(crate) inter: &'a wgpu::BindGroup,
    /// The console font's atlas and whether it is a distance field.
    pub(crate) console: Option<(&'a wgpu::BindGroup, bool)>,
    /// The SJK UI's display and body families' atlases, once loaded.
    pub(crate) families: Option<[(&'a wgpu::BindGroup, bool); 2]>,
}

/// GPU resources of the classic console layer, one per world.
pub(crate) struct ConsoleLayer {
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    texture_layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    /// Pipelines by blend state; the first is alpha blending.
    pipelines: Vec<(wgpu::BlendState, wgpu::RenderPipeline)>,
    stages: Vec<Stage>,
    /// The shader was looked for (found or not).
    loaded: bool,
    white: wgpu::BindGroup,
    quad_buffer: wgpu::Buffer,
    quad_vertices: Vec<Vertex>,
    runs: Vec<Run>,
    text_buffer: wgpu::Buffer,
    text_count: u32,
    /// Vertices of the display and body families' text after the console's.
    family_counts: [u32; 2],
    atlas: TextAtlas,
    frame: ConsoleFrame,
}

impl ConsoleLayer {
    /// Create the layer; the background shader is loaded now when `load` (the
    /// classic console is in use), else on first use ([`Self::ensure_loaded`]).
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        format: wgpu::TextureFormat,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        load: bool,
    ) -> Self {
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("console layer textures"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("console layer"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scope.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("console layer"),
            bind_group_layouts: &[Some(&texture_layout)],
            immediate_size: 0,
        });
        let white = texture_group(
            device,
            queue,
            &texture_layout,
            &image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
            true,
        );
        let mut layer = Self {
            shader,
            pipeline_layout,
            texture_layout,
            format,
            pipelines: Vec::with_capacity(4),
            stages: Vec::new(),
            loaded: false,
            white,
            quad_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("console layer quads"),
                size: (MAX_QUADS * 6 * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            quad_vertices: Vec::with_capacity(64),
            runs: Vec::with_capacity(MAX_STAGES + 2),
            text_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("console layer text"),
                size: (MAX_TEXT_VERTICES * std::mem::size_of::<TextVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            text_count: 0,
            family_counts: [0; 2],
            atlas: TextAtlas::Inter,
            frame: ConsoleFrame::default(),
        };
        layer.pipeline(device, wgpu::BlendState::ALPHA_BLENDING);
        if load {
            layer.ensure_loaded(device, queue, vfs, shaders);
        }
        layer
    }

    /// Load the `console` shader's stages unless that was already tried.
    pub(crate) fn ensure_loaded(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
    ) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let Some(definition) = shaders.get(SHADER) else {
            crate::log::progress(format_args!(
                "console: no {SHADER} shader, drawing a plain background"
            ));
            return;
        };
        for stage in definition.stages.iter().take(MAX_STAGES) {
            let Some(name) = stage.images.first() else {
                continue;
            };
            let image = match stage_image(vfs, shaders, name) {
                Ok(image) => image,
                Err(error) => {
                    crate::log::progress(format_args!("console: stage {name}: {error}"));
                    continue;
                }
            };
            let program = StageProgram::from_shader(stage);
            let texture =
                texture_group(device, queue, &self.texture_layout, &image, !program.clamp);
            let pipeline = self.pipeline(device, program.blend);
            self.stages.push(Stage {
                program,
                pipeline,
                texture,
            });
        }
    }

    /// Whether the background shader still has to be looked for.
    pub(crate) fn needs_loading(&self) -> bool {
        !self.loaded
    }

    /// The frame the console fills, cleared.
    pub(crate) fn begin_frame(&mut self) -> &mut ConsoleFrame {
        self.frame.clear();
        &mut self.frame
    }

    /// Nothing is drawn this frame (another console style, or none).
    pub(crate) fn clear(&mut self) {
        self.frame.clear();
        self.quad_vertices.clear();
        self.runs.clear();
        self.text_count = 0;
        self.family_counts = [0; 2];
    }

    /// Build this frame's quads from the filled frame and upload them with its text.
    pub(crate) fn upload(&mut self, queue: &crate::frame_queue::FrameQueue, viewport: [f32; 2]) {
        self.quad_vertices.clear();
        self.runs.clear();
        let seconds = crate::menu::art::motion::seconds();
        if let Some(background) = self.frame.background {
            let rect = [0.0, 0.0, viewport[0], background.height];
            if self.stages.is_empty() {
                let [r, g, b] = FALLBACK;
                let colour = [r, g, b, background.opacity];
                self.push(None, rect, FULL_UV, [colour; 4], viewport);
            }
            for index in 0..self.stages.len() {
                let program = &self.stages[index].program;
                let corners = stage_corners(&program.modifications, background.t_range, seconds);
                let colour = stage_colour(program, background.opacity, seconds);
                self.push(Some(index), rect, corners, [colour; 4], viewport);
            }
        }
        for shade in 0..self.frame.shades.len() {
            let Shade { rect, corners } = self.frame.shades[shade];
            self.push(None, rect, FULL_UV, corners, viewport);
        }
        for quad in 0..self.frame.quads.len() {
            let SolidQuad { rect, color } = self.frame.quads[quad];
            self.push(None, rect, FULL_UV, [color; 4], viewport);
        }
        if !self.quad_vertices.is_empty() {
            queue.write_buffer(
                &self.quad_buffer,
                0,
                bytemuck::cast_slice(&self.quad_vertices),
            );
        }
        // One buffer: the console's text, then each family's, within its size.
        let mut room = MAX_TEXT_VERTICES / 6 * 6;
        let mut offset = 0_u64;
        let size = std::mem::size_of::<TextVertex>() as u64;
        let buffer = &self.text_buffer;
        let mut write = |vertices: &[TextVertex]| {
            let count = vertices.len().min(room) / 6 * 6;
            room -= count;
            if count != 0 {
                queue.write_buffer(
                    buffer,
                    offset * size,
                    bytemuck::cast_slice(&vertices[..count]),
                );
            }
            offset += count as u64;
            u32::try_from(count).unwrap_or(0)
        };
        self.text_count = write(&self.frame.text);
        self.family_counts = [
            write(&self.frame.display_text),
            write(&self.frame.body_text),
        ];
        self.atlas = self.frame.atlas;
    }

    fn push(
        &mut self,
        stage: Option<usize>,
        [x, y, width, height]: [f32; 4],
        uv: [[f32; 2]; 4],
        colors: [[f32; 4]; 4],
        viewport: [f32; 2],
    ) {
        if width <= 0.0 || height <= 0.0 || self.quad_vertices.len() + 6 > MAX_QUADS * 6 {
            return;
        }
        let start = self.quad_vertices.len() as u32;
        let ndc = |px: f32, py: f32| [px / viewport[0] * 2.0 - 1.0, 1.0 - py / viewport[1] * 2.0];
        let corners = [
            ndc(x, y),
            ndc(x + width, y),
            ndc(x + width, y + height),
            ndc(x, y + height),
        ];
        for index in [0, 1, 2, 0, 2, 3] {
            self.quad_vertices.push(Vertex {
                position: corners[index],
                uv: uv[index],
                color: colors[index],
            });
        }
        let end = self.quad_vertices.len() as u32;
        match self.runs.last_mut() {
            Some(run) if run.stage == stage && run.vertices.end == start => run.vertices.end = end,
            _ => self.runs.push(Run {
                vertices: start..end,
                stage,
            }),
        }
    }

    /// Whether this frame draws anything.
    pub(crate) fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.text_count == 0 && self.family_counts == [0; 2]
    }

    /// Draw the uploaded layer: background stages, quads, then the text.
    pub(crate) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, text: TextDraw<'a>) {
        if !self.runs.is_empty() {
            pass.set_vertex_buffer(0, self.quad_buffer.slice(..));
            for run in &self.runs {
                let (pipeline, group) = match run.stage {
                    Some(index) => (self.stages[index].pipeline, &self.stages[index].texture),
                    None => (0, &self.white),
                };
                pass.set_pipeline(&self.pipelines[pipeline].1);
                pass.set_bind_group(0, group, &[]);
                pass.draw(run.vertices.clone(), 0..1);
            }
        }
        if self.text_count == 0 && self.family_counts == [0; 2] {
            return;
        }
        pass.set_vertex_buffer(0, self.text_buffer.slice(..));
        let console = match (self.atlas, text.console) {
            (TextAtlas::Console, Some((group, true))) => Some((text.sdf_pipeline, group)),
            (TextAtlas::Console, Some((group, false))) => Some((text.pipeline, group)),
            (TextAtlas::Console, None) => None,
            (TextAtlas::Inter, _) => Some((text.pipeline, text.inter)),
        };
        if let Some((pipeline, group)) = console
            && self.text_count != 0
        {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..self.text_count, 0..1);
        }
        let Some(families) = text.families else {
            return;
        };
        let mut start = self.text_count;
        for ((group, distance_field), count) in families.into_iter().zip(self.family_counts) {
            if count != 0 {
                pass.set_pipeline(if distance_field {
                    text.sdf_pipeline
                } else {
                    text.pipeline
                });
                pass.set_bind_group(0, group, &[]);
                pass.draw(start..start + count, 0..1);
            }
            start += count;
        }
    }

    /// The pipeline drawing with `blend`, created on first use.
    fn pipeline(&mut self, device: &wgpu::Device, blend: wgpu::BlendState) -> usize {
        if let Some(index) = self.pipelines.iter().position(|(state, _)| *state == blend) {
            return index;
        }
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("console layer"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.push((blend, pipeline));
        self.pipelines.len() - 1
    }
}

const FULL_UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// Read one stage's picture as display values, scaled down past [`MAX_SIDE`].
fn stage_image(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    name: &str,
) -> Result<image::RgbaImage, Box<dyn std::error::Error>> {
    if matches!(
        name.to_ascii_lowercase().as_str(),
        "$whiteimage" | "*white" | "$white"
    ) {
        return Ok(image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])));
    }
    let path = shaders
        .resolve_stage_image(vfs, name)?
        .ok_or_else(|| format!("{name} has no image"))?;
    let asset = vfs
        .read(path.as_str())?
        .ok_or_else(|| format!("{path} is missing"))?;
    let image = crate::decode_image(&asset.bytes, path.as_str())?;
    let image = if image.width().max(image.height()) > MAX_SIDE {
        image.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    Ok(image.into_rgba8())
}

fn texture_group(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    image: &image::RgbaImage,
    repeat: bool,
) -> wgpu::BindGroup {
    let texture = device.create_texture_with_data(
        queue.raw(),
        &wgpu::TextureDescriptor {
            label: Some("console layer picture"),
            size: wgpu::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::ui_target::TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        image.as_raw(),
    );
    let address = if repeat {
        wgpu::AddressMode::Repeat
    } else {
        wgpu::AddressMode::ClampToEdge
    };
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("console layer picture"),
        address_mode_u: address,
        address_mode_v: address,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("console layer picture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &texture.create_view(&Default::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    fn wave(base: f32, amplitude: f32, phase: f32, frequency: f32) -> WaveForm {
        WaveForm {
            function: "sin".into(),
            base,
            amplitude,
            phase,
            frequency,
        }
    }

    /// The retail and JoF stars stage: `tcMod scroll 0.01 0.02`, then `scale 2 1`.
    fn stars() -> Vec<TexMod> {
        vec![TexMod::Scroll([0.01, 0.02]), TexMod::Scale([2.0, 1.0])]
    }

    #[test]
    fn stars_scroll_then_scale_in_script_order() {
        let start = stage_corners(&stars(), [0.0, 1.0], 0.0);
        assert!(close(start[0], [0.0, 0.0]) && close(start[2], [2.0, 1.0]));
        // 10 s: scrolled by (0.1, 0.2), then doubled horizontally.
        let later = stage_corners(&stars(), [0.0, 1.0], 10.0);
        assert!(close(later[0], [0.2, 0.2]), "{:?}", later[0]);
        assert!(close(later[2], [2.2, 1.2]), "{:?}", later[2]);
        // The scroll keeps only its fraction: 1.5 and 3 turns leave 0.5 and 0.
        let wrapped = stage_corners(&[TexMod::Scroll([0.25, 0.5])], [0.0, 1.0], 6.0);
        assert!(close(wrapped[0], [0.5, 0.0]), "{:?}", wrapped[0]);
    }

    #[test]
    fn ratio_fix_range_feeds_the_modifications() {
        let corners = stage_corners(&[], [0.25, 0.75], 3.0);
        assert!(close(corners[0], [0.0, 0.25]) && close(corners[2], [1.0, 0.75]));
    }

    #[test]
    fn rotate_and_transform_follow_the_renderer() {
        // 18 s at 5 degrees per second: a quarter turn about the centre.
        let turned = stage_corners(&[TexMod::Rotate(5.0)], [0.0, 1.0], 18.0);
        assert!(close(turned[0], [0.0, 1.0]), "{:?}", turned[0]);
        let moved = stage_corners(
            &[TexMod::Transform([1.0, 0.0, 0.0, 1.0, 0.5, 0.25])],
            [0.0, 1.0],
            0.0,
        );
        assert!(close(moved[2], [1.5, 1.25]));
    }

    #[test]
    fn glow_wave_pulses_between_its_bounds() {
        // `rgbGen wave sin 0.2 0.1 0 0.2`: 0.2 at t = 0, 0.3 a quarter period
        // (1.25 s) later and 0.1 at three quarters.
        let program = StageProgram {
            modifications: Vec::new(),
            rgb: Channel::Wave(wave(0.2, 0.1, 0.0, 0.2)),
            alpha: Channel::One,
            blend: blend_state(&StageBlend::Add),
            clamp: false,
        };
        let at = |seconds| stage_colour(&program, 0.5, seconds);
        assert!((at(0.0)[0] - 0.2).abs() < 1e-3);
        assert!((at(1.25)[1] - 0.3).abs() < 1e-3);
        assert!((at(3.75)[2] - 0.1).abs() < 1e-3);
        assert_eq!(at(1.0)[3], 1.0);
    }

    #[test]
    fn vertex_alpha_is_the_console_opacity_and_waves_clamp() {
        let program = StageProgram {
            modifications: stars(),
            rgb: Channel::Vertex,
            alpha: Channel::Vertex,
            blend: blend_state(&StageBlend::Alpha),
            clamp: false,
        };
        assert_eq!(stage_colour(&program, 0.79, 4.0), [1.0, 1.0, 1.0, 0.79]);
        let loud = StageProgram {
            rgb: Channel::Wave(wave(0.9, 0.5, 0.25, 0.0)),
            ..program
        };
        assert_eq!(stage_colour(&loud, 1.0, 0.0)[0], 1.0);
    }

    #[test]
    fn blend_functions_map_to_gl_factors() {
        assert_eq!(blend_state(&StageBlend::Replace), wgpu::BlendState::REPLACE);
        let add = blend_state(&StageBlend::Add).color;
        assert_eq!(
            (add.src_factor, add.dst_factor),
            (wgpu::BlendFactor::One, wgpu::BlendFactor::One)
        );
        let custom = blend_state(&StageBlend::Custom {
            source: "gl_dst_color".into(),
            destination: "gl_one".into(),
        })
        .color;
        assert_eq!(
            (custom.src_factor, custom.dst_factor),
            (wgpu::BlendFactor::Dst, wgpu::BlendFactor::One)
        );
    }

    #[test]
    fn retail_and_cosmetic_console_shaders_parse_into_programs() {
        let retail = b"console\n{\n nopicmip\n nomipmaps\n {\n map gfx/interface/stars\n \
            blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA\n alphaGen vertex\n \
            tcMod scroll 0.01 0.02\n tcMod scale 2 1\n }\n {\n \
            map gfx/interface/console_jediacademy\n blendFunc GL_ONE GL_ONE\n glow\n \
            rgbGen wave sin 0.2 0.1 0 0.2\n }\n}\n";
        let path = "shaders/gfx.shader";
        let definitions = sjk_shader::parse_shader_script(retail, path).unwrap();
        let stages: Vec<_> = definitions[0]
            .stages
            .iter()
            .map(StageProgram::from_shader)
            .collect();
        assert_eq!(stages[0].modifications, stars());
        assert_eq!(stages[0].alpha, Channel::Vertex);
        assert_eq!(stages[0].blend, wgpu::BlendState::ALPHA_BLENDING);
        assert_eq!(stages[1].rgb, Channel::Wave(wave(0.2, 0.1, 0.0, 0.2)));
        assert_eq!(stages[1].blend, blend_state(&StageBlend::Add));
        let cosmetic = b"console\n{\n {\n map gfx/interface/console_jediacademy\n \
            blendFunc GL_ONE GL_ZERO\n }\n {\n map gfx/interface/stars2\n \
            blendFunc GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA\n alphaGen vertex\n \
            tcMod scroll 0.01 0.02\n tcMod scale 2 1\n rgbGen wave sin 0.2 0.1 0 0.2\n }\n}\n";
        let definitions = sjk_shader::parse_shader_script(cosmetic, path).unwrap();
        let first = StageProgram::from_shader(&definitions[0].stages[0]);
        assert_eq!(first.blend, wgpu::BlendState::REPLACE);
        let second = StageProgram::from_shader(&definitions[0].stages[1]);
        assert_eq!(second.alpha, Channel::Vertex);
        assert_eq!(second.rgb, Channel::Wave(wave(0.2, 0.1, 0.0, 0.2)));
    }
}
