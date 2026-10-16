//! The classic profile's live model preview, where retail drew its
//! `ITEM_TYPE_MODEL` character (`player2.menu`) and JoF EJK its cosmetics
//! preview: the stage actor, with what it wears, drawn into a target of its
//! own with its own camera, then encoded into an 8-bit texture the UI draws
//! as a quad ([`crate::ui_renderer::PREVIEW_TEXTURE`]).
//!
//! The colour target has the scene's format and a `Depth32Float` depth, so
//! the world material pipelines draw into it unchanged. Lit blades, when the
//! page shows sabers, go into an 8-bit texture of their own through the
//! game's blade renderer, against the model's depth, as the effect layer
//! takes them. `preview.wgsl` makes the display values, takes coverage from
//! depth and adds the blades, their glow beyond the body becoming alpha. The camera frames the
//! whole body from in front and turns around it at retail's
//! `model_rotation 50` (a degree every 50 ms). The actor is lit as retail lit a menu
//! model with no world, the same on every map: it is a picture of the model, not of
//! the map the player is on.
//!
//! A failure must not take the client down with it: the pipeline and
//! textures are made inside a validation scope, and the first frame is a
//! probe recorded in an encoder of its own, finished and submitted inside
//! one too. Any error turns the preview off for the session (the profile
//! keeps the model's portrait) and is logged once.

use super::*;
use crate::camera_uniform::CameraUniform;

/// Turn rate around the model (`model_rotation 50`).
const TURN_DEGREES_PER_SECOND: f32 = 20.0;
/// Vertical field of view.
pub(super) const FIELD_OF_VIEW_DEGREES: f32 = 30.0;
/// Half the height and half the width framed around the model's middle.
const HALF_HEIGHT: f32 = 40.0;
const HALF_WIDTH: f32 = 26.0;
/// Height of the framed middle above the actor origin (a player's box runs
/// from 24 below it to 40 above).
const MIDDLE_ABOVE_ORIGIN: f32 = 8.0;
/// Longest side of the target, and the step its sides are rounded up to.
const MAX_SIDE: u32 = 1_024;
const SIDE_STEP: u32 = 16;
/// Camera flags (`CameraUniform::_padding`): a secondary view without probe
/// reflections.
const VIEW_FLAGS: f32 = 17.0;

/// The encode pipeline, made for one scene format.
struct Encode {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    /// The same, for while the speed trail's see-through copies are drawn
    /// (`preview.wgsl`'s `TRAIL`).
    trail_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

/// One target size's textures and bindings.
struct Target {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// The blades, in the effect layer's 8-bit display values.
    blades: wgpu::TextureView,
    /// The speed trail's two copies, nearer then farther, each drawn whole on its own
    /// with its own depth, so the picture fades each as one (`preview.wgsl`'s `TRAIL`).
    copies: [(wgpu::TextureView, wgpu::TextureView); 2],
    display: wgpu::TextureView,
    camera: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    encode_bind: wgpu::BindGroup,
}

/// Preview state on [`MenuStage`].
#[derive(Default)]
pub(crate) struct Preview {
    encode: Option<Encode>,
    target: Option<Target>,
    /// The preview's size on screen in pixels, while the UI shows one.
    pub(super) wanted: Option<[u32; 2]>,
    /// The room round the body (1: retail's framing) and the still angle
    /// (none: turning), as the profile asked ([crate::player_menu::ModelPreview]).
    pub(super) room: f32,
    pub(super) angle: Option<f32>,
    started: Option<Instant>,
    /// The target holds a drawn frame the UI can show.
    pub(super) ready: bool,
    /// The probe frame went through: later frames record into the frame's
    /// own encoder.
    probed: bool,
    /// Making or drawing the preview failed; it stays off.
    failed: bool,
    /// This frame's blades in the actor's hands (`begin_saber_instances`).
    pub(super) blades: Vec<crate::saber::Instance>,
    /// This frame's showcase of the sabers alone, which the camera frames
    /// instead of the actor (`begin_saber_instances`).
    pub(super) showcase: Option<super::showcase::View>,
    /// How far above its spot a jumping model is seen this frame
    /// (`force_motion::rise_seen`): the camera looks that much lower.
    pub(super) lift: f32,
}

/// Run `create` inside validation and out-of-memory scopes: `None`, with the
/// error logged, when wgpu reported one.
fn guarded<T>(device: &wgpu::Device, what: &str, create: impl FnOnce() -> T) -> Option<T> {
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let value = create();
    let invalid = pollster::block_on(validation.pop());
    let exhausted = pollster::block_on(memory.pop());
    match invalid.or(exhausted) {
        Some(error) => {
            crate::log::progress(format_args!(
                "warning: model preview off, {what} failed: {error}"
            ));
            None
        }
        None => Some(value),
    }
}

/// The target size for `wanted` pixels on screen: no side over
/// [`MAX_SIDE`], the aspect kept, sides rounded up to [`SIDE_STEP`].
pub(super) fn target_size([width, height]: [u32; 2]) -> [u32; 2] {
    let longest = width.max(height).max(1);
    let scale = (MAX_SIDE as f32 / longest as f32).min(1.0);
    [width, height].map(|side| {
        let side = (side as f32 * scale).ceil().max(SIDE_STEP as f32) as u32;
        side.div_ceil(SIDE_STEP) * SIDE_STEP
    })
}

/// The camera for an actor at `origin` facing `facing`, `seconds` into the
/// turn, for a target of `aspect` (width over height), leaving `room` times
/// retail's framing round the body.
pub(super) fn camera(
    origin: Vec3,
    facing: Vec3,
    aspect: f32,
    seconds: f32,
    room: f32,
) -> (Mat4, Vec3, Vec3) {
    let middle = origin + Vec3::Z * MIDDLE_ABOVE_ORIGIN;
    let half = (FIELD_OF_VIEW_DEGREES * 0.5).to_radians().tan();
    let room = room.max(0.5);
    let distance = (HALF_HEIGHT * room / half).max(HALF_WIDTH * room / (half * aspect.max(0.1)));
    let turn = Quat::from_rotation_z((seconds * TURN_DEGREES_PER_SECOND).to_radians());
    let facing = Vec3::new(facing.x, facing.y, 0.0).normalize_or(Vec3::X);
    let eye = middle + turn * facing * distance + Vec3::Z * 4.0;
    let view = look_at_mat4(eye, middle, Vec3::Z);
    let projection = perspective(
        FIELD_OF_VIEW_DEGREES.to_radians(),
        aspect,
        2.0,
        distance * 4.0,
    );
    (projection * view, eye, (middle - eye).normalize_or(Vec3::X))
}

/// A fragment-visible 2D texture binding of the encode program.
fn copy_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        count: None,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type,
        },
    }
}

impl Encode {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK model preview encode"),
            source: wgpu::ShaderSource::Wgsl(include_str!("preview.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK model preview encode layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                },
                // The trail's copies: colour and depth of the nearer, then the farther.
                copy_entry(3, wgpu::TextureSampleType::Float { filterable: false }),
                copy_entry(4, wgpu::TextureSampleType::Depth),
                copy_entry(5, wgpu::TextureSampleType::Float { filterable: false }),
                copy_entry(6, wgpu::TextureSampleType::Depth),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let encoding = if matches!(
            format,
            wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rg11b10Ufloat
        ) {
            2.0
        } else if format.is_srgb() {
            1.0
        } else {
            0.0
        };
        let pipeline = |trail: bool| {
            let constants = [
                ("ENCODING", encoding),
                ("TRAIL", f64::from(u8::from(trail))),
            ];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("SJK model preview encode"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::ui_target::TEXTURE_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            format,
            pipeline: pipeline(false),
            trail_pipeline: pipeline(true),
            layout,
        }
    }
}

impl Target {
    fn new(
        device: &wgpu::Device,
        encode: &Encode,
        camera_layout: &wgpu::BindGroupLayout,
        size: [u32; 2],
    ) -> Self {
        let texture = |label, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attachment =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let color = texture("SJK model preview colour", encode.format, attachment);
        let depth = texture("SJK model preview depth", DepthTarget::FORMAT, attachment);
        let blades = texture(
            "SJK model preview blades",
            crate::frame_target::aa::effects::FORMAT,
            attachment,
        );
        let copies = [
            "SJK model preview speed trail copy (near)",
            "SJK model preview speed trail copy (far)",
        ]
        .map(|label| {
            (
                texture(label, encode.format, attachment),
                texture(label, DepthTarget::FORMAT, attachment),
            )
        });
        let display = texture(
            "SJK model preview display",
            crate::ui_target::TEXTURE_FORMAT,
            attachment,
        );
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK model preview camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK model preview camera"),
            layout: camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let encode_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK model preview encode"),
            layout: &encode.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&blades),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&copies[0].0),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&copies[0].1),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&copies[1].0),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&copies[1].1),
                },
            ],
        });
        Self {
            size,
            color,
            depth,
            blades,
            copies,
            display,
            camera,
            camera_bind,
            encode_bind,
        }
    }
}

impl GpuState {
    /// Draw the preview for this frame when the UI shows one: the stage
    /// actor into the preview target with the preview camera, then the
    /// display texture the UI samples.
    pub(crate) fn encode_stage_preview(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let wanted = self.menu_stage.preview.wanted.filter(|_| {
            self.menu_stage.preview_only
                && self.menu_stage.actor.is_some()
                && !self.menu_stage.preview.failed
        });
        let Some(wanted) = wanted else {
            self.menu_stage.preview.ready = false;
            self.menu_stage.preview.started = None;
            return;
        };
        if !self.prepare_stage_preview(wanted) {
            self.menu_stage.preview.failed = true;
            self.menu_stage.preview.ready = false;
            return;
        }
        self.menu_stage
            .preview
            .started
            .get_or_insert_with(Instant::now);
        let mut blades = std::mem::take(&mut self.menu_stage.preview.blades);
        self.saber_gpu.prepare_preview(&self.queue, &mut blades);
        self.menu_stage.preview.blades = blades;
        if self.menu_stage.preview.probed {
            // The power's effects, built for this frame's camera (the probe frame has none).
            if let Some((_, eye, _)) = self.stage_preview_view() {
                self.prepare_force_fx(encoder, eye);
                self.place_force_trail(eye);
            }
            self.record_stage_preview(encoder);
            self.menu_stage.preview.ready = true;
            return;
        }
        // The first frame goes on its own, inside a scope, so an
        // incompatibility is caught here rather than by the frame's submit.
        let device = self.device.clone();
        let probed = guarded(&device, "drawing it", || {
            let mut own = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("SJK model preview probe"),
            });
            self.record_stage_preview(&mut own);
            self.queue.submit([own.finish()]);
        });
        let preview = &mut self.menu_stage.preview;
        preview.probed = probed.is_some();
        preview.failed = probed.is_none();
        preview.ready = probed.is_some();
    }

    /// Make the encode pipeline and the target for `wanted` pixels unless
    /// they exist; false when wgpu refused them.
    fn prepare_stage_preview(&mut self, wanted: [u32; 2]) -> bool {
        let format = self.context.scene_format();
        let device = self.device.clone();
        let preview = &mut self.menu_stage.preview;
        if preview
            .encode
            .as_ref()
            .is_none_or(|encode| encode.format != format)
        {
            let Some(encode) = guarded(&device, "its pipeline", || Encode::new(&device, format))
            else {
                return false;
            };
            preview.encode = Some(encode);
            preview.target = None;
        }
        let size = target_size(wanted);
        if preview
            .target
            .as_ref()
            .is_none_or(|target| target.size != size)
        {
            let Some(encode) = preview.encode.as_ref() else {
                return false;
            };
            let camera_layout = &self.camera_layout;
            let Some(target) = guarded(&device, "its target", || {
                Target::new(&device, encode, camera_layout, size)
            }) else {
                return false;
            };
            self.ui_shapes.set_preview(&device, &target.display);
            preview.target = Some(target);
            preview.ready = false;
        }
        true
    }

    /// This frame's preview camera: its view projection, eye and forward.
    fn stage_preview_view(&self) -> Option<(Mat4, Vec3, Vec3)> {
        let preview = &self.menu_stage.preview;
        let actor = self.menu_stage.actor.as_ref()?;
        let target = preview.target.as_ref()?;
        // Turning since it started, or held at the angle asked for.
        let seconds = preview.angle.map_or_else(
            || {
                preview
                    .started
                    .map_or(0.0, |started| started.elapsed().as_secs_f32())
            },
            |angle| angle / TURN_DEGREES_PER_SECOND,
        );
        // The model faces its yaw; the actor's rotation includes the Ghoul2
        // facing turn (`weapon_view::actor_world_rotation`).
        let facing = actor.rotation * Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) * Vec3::X;
        let aspect = target.size[0] as f32 / target.size[1] as f32;
        Some(match preview.showcase {
            Some(showcase) if self.menu_stage.showcase => showcase.camera(aspect),
            // A jumping model is seen above its spot (`force_motion::rise_seen`).
            _ => camera(
                actor.origin - Vec3::Z * preview.lift,
                facing,
                aspect,
                seconds,
                preview.room,
            ),
        })
    }

    /// Record the preview's two passes: the actor with the preview camera,
    /// then the encode into the display texture.
    fn record_stage_preview(&self, encoder: &mut wgpu::CommandEncoder) {
        let preview = &self.menu_stage.preview;
        let (Some(actor), Some(target), Some(encode)) = (
            self.menu_stage.actor.as_ref(),
            preview.target.as_ref(),
            preview.encode.as_ref(),
        ) else {
            return;
        };
        let Some((view_projection, eye, forward)) = self.stage_preview_view() else {
            return;
        };
        let uniform = CameraUniform {
            view_projection: view_projection.to_cols_array_2d(),
            camera_position: eye.to_array(),
            shader_time: crate::menu::art::motion::seconds() as f32,
            view_forward: forward.to_array(),
            _padding: VIEW_FLAGS,
        };
        self.queue
            .write_buffer(&target.camera, 0, bytemuck::bytes_of(&uniform));
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK model preview"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for blended in [false, true] {
                self.menu_stage.draw_parts(
                    &mut pass,
                    &self.world_materials,
                    &target.camera_bind,
                    blended,
                );
            }
        }
        // Speed's trail: each copy of the body drawn whole into its own picture, which
        // the encode fades (only while the trail shows; otherwise they go unread).
        if self.menu_stage.force_fx.shows_trail() {
            for (copy, (color, depth)) in target.copies.iter().enumerate() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("SJK model preview speed trail copy"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                for blended in [false, true] {
                    self.menu_stage.force_fx.draw_copy(
                        &mut pass,
                        &self.world_materials,
                        &target.camera_bind,
                        actor,
                        copy,
                        blended,
                    );
                }
            }
        }
        {
            // The blades, hidden behind the body as the model's depth says.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK model preview blades"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.blades,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !preview.blades.is_empty() {
                self.saber_gpu.draw_preview(&mut pass, &target.camera_bind);
            }
            // The power's beam, puffs or spray, in the layer the blades glow in.
            self.menu_stage.force_fx.draw(
                &mut pass,
                &target.camera_bind,
                &self.particle_atlas.bind_group,
                &self.particle_pipelines,
            );
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK model preview encode"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.display,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(if self.menu_stage.force_fx.shows_trail() {
            &encode.trail_pipeline
        } else {
            &encode.pipeline
        });
        pass.set_bind_group(0, &target.encode_bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encode_program_validates() {
        crate::wgsl_source::validate(include_str!("preview.wgsl"));
    }

    #[test]
    fn targets_keep_the_aspect_within_the_largest_side() {
        assert_eq!(target_size([990, 990]), [992, 992]);
        assert_eq!(target_size([2000, 1000]), [1024, 512]);
        assert_eq!(target_size([10, 400]), [16, 400]);
        assert_eq!(target_size([0, 0]), [16, 16]);
    }

    #[test]
    fn the_camera_frames_the_model_from_in_front() {
        let origin = Vec3::new(100.0, 50.0, 24.0);
        let (view_projection, eye, forward) = camera(origin, Vec3::X, 1.0, 0.0, 1.0);
        // In front (on the facing side) and looking back at it.
        assert!(eye.x > origin.x + 100.0);
        assert!(forward.x < -0.9);
        // Head and feet both land inside the view.
        for z in [origin.z - 24.0, origin.z + 40.0] {
            let clip = view_projection * Vec3::new(origin.x, origin.y, z).extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(ndc.y.abs() < 1.0 && ndc.x.abs() < 1.0, "{z}: {ndc}");
        }
        // More room keeps a blade held out beside the body in view, where
        // retail's framing loses it.
        let blade_tip = Vec3::new(origin.x, origin.y + 50.0, origin.z + 30.0);
        let inside = |room: f32| {
            let (view_projection, ..) = camera(origin, Vec3::X, 1.0, 0.0, room);
            let clip = view_projection * blade_tip.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0
        };
        assert!(!inside(1.0));
        assert!(inside(1.35));
        // A quarter turn later the camera has moved round to the side.
        let (_, eye, _) = camera(origin, Vec3::X, 1.0, 90.0 / TURN_DEGREES_PER_SECOND, 1.0);
        assert!((eye.y - origin.y).abs() > 100.0);
    }
}
