//! Opt-in actor or world-volume sun casters and diffuse receivers, main view only.
use super::*;
use glam::Vec3;
#[path = "day_night.rs"]
pub(crate) mod day;
#[path = "sun_shadow_settings.rs"]
pub(crate) mod settings;

#[path = "sun_shadow_fit.rs"]
mod fit;
#[path = "sun_gap_close.rs"]
mod gap_close;
#[path = "sun_shadow_held.rs"]
mod held;
#[path = "sun_shadow_hulls.rs"]
pub(super) mod hulls;
#[path = "lamp_shadows.rs"]
pub(super) mod lamp_shadows;
#[path = "light_buffer.rs"]
pub(super) mod light_buffer;

#[path = "sun_shadow_bounds.rs"]
pub(super) mod bounds;
#[path = "sun_shadow_gpu.rs"]
mod resources;
#[path = "sun_shadow_volume.rs"]
mod volume;
#[path = "volumetric_light.rs"]
mod volumetric_light;

/// The world receiver program: shared visibility plus the actor-shade fragment.
pub(super) fn receiver_shader() -> String {
    format!(
        "{}{}{}",
        include_str!("sun_shadow_uniform.wgsl"),
        include_str!("sun_visibility.wgsl"),
        include_str!("sun_shadow_receiver.wgsl")
    )
}

/// Scene pixels per light-buffer texel: half the display resolution whatever the
/// supersampling, so supersampling sharpens edges and textures but not the light cost.
/// `SJK_LIGHT_DIVISOR` overrides it for evidence (one isolates the upsample).
fn light_divisor(supersampling: u32) -> u32 {
    2 * supersampling.max(1)
}

/// The mirror images' counterparts of `Runtime::receiver` and `Runtime::light_group`.
pub(super) struct MirrorGroups {
    pub(super) receiver: wgpu::BindGroup,
    light: wgpu::BindGroup,
}

/// Map-lifetime resources; absent entirely in the default-off path.
pub(super) struct Runtime {
    view: std::cell::Cell<Option<glam::Mat4>>,
    time: std::cell::Cell<f32>,
    day: std::cell::Cell<day::Settings>,
    director_sun: std::cell::Cell<Option<(Vec3, f32)>>,
    /// Live `r_volumetricClarity`; only the medium reads it.
    clarity: std::cell::Cell<f32>,
    /// Live `r_dayBrightness` for the real-time lighting mode.
    light_scale: std::cell::Cell<f32>,
    ambient_fill: std::cell::Cell<f32>,
    indirect_readability: std::cell::Cell<[f32; 2]>,
    /// Live `r_dayDebug` bitmask, handed to the shaders in `Parameters.realtime.w`.
    pub(in crate::world_materials) debug: std::cell::Cell<u32>,
    filter_reference: std::cell::Cell<bool>,
    depth: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    camera: wgpu::BindGroup,
    /// Map-wide world-only cascade behind the view fit; absent in the actor-only mode.
    far: Option<FarCascade>,
    /// Finest moving-caster cascade, truncated at `settings.near`.
    close: Option<Cascade>,
    /// Separate static casters for the view and close fits. Their contents are
    /// reused between frames only with `settings.held`.
    held: Option<[held::Held; 2]>,
    /// Probe global illumination traced through the voxel world; needs the far cascade.
    pub(crate) probes: Option<super::gi_probes::Runtime>,
    receiver_buffer: wgpu::Buffer,

    /// Sun parameters and evaluated light textures for world/model materials.
    receiver: wgpu::BindGroup,
    /// Lighting inputs for evaluation and baked diffuse correction; no colour output.
    light_group: wgpu::BindGroup,
    /// `receiver` and `light_group` over the light buffer's mirror images.
    mirror_groups: Option<MirrorGroups>,
    /// Geometry-only sun visibility; lamp/probe bindings stay in the fullscreen pass.
    sun_group: wgpu::BindGroup,
    /// Half-resolution light buffer and its pass; day mode only.
    pub(in crate::world_materials) light: Option<light_buffer::LightBuffer>,
    light_pipelines: Option<light_buffer::Pipelines>,
    /// The forge's point-light block, bound into rebuilt material receiver groups.
    point_lights: wgpu::Buffer,
    /// The cached light program's group: this light buffer's receiver coordinates and
    /// the map's lamp cache.
    cache_group: Option<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    bounds: bounds::Bounds,
    /// The map's lamps as GPU buffers, bound during lighting evaluation.
    lamps: crate::lamp_lights::Gpu,
    /// Movers shadowing the lamps near them (`mover_occlusion.rs`).
    movers: Option<super::mover_occlusion::gpu::Runtime>,
    /// Shadow maps of the nearest lamps; day mode only.
    lamp_shadows: Option<lamp_shadows::Runtime>,
    /// Slit closing over every cascade after it renders (`r_sunShadowGapClose`).
    gap_close: gap_close::Runtime,
    /// Live `r_sunShadowGapClose`: holes narrower than this many units stop passing sun.
    gap_width: std::cell::Cell<f32>,
    caster: wgpu::RenderPipeline,
    world_caster: wgpu::RenderPipeline,

    volumetrics: Option<volumetric_light::Runtime>,
    receiver_pipelines: [wgpu::RenderPipeline; 3],
    receivers: Vec<(usize, usize)>,
    empty: wgpu::BindGroup,
    sun: sjk_shader::SunParms,
    environment: bool,
    suppress_sun: bool,
    settings: settings::Settings,
}

impl Runtime {
    /// The light pass group of the active light-buffer images (`LightBuffer::images`).
    pub(in crate::world_materials) fn light_group(&self) -> &wgpu::BindGroup {
        match &self.mirror_groups {
            Some(groups) if self.light.as_ref().is_some_and(|l| l.mirror_active()) => &groups.light,
            _ => &self.light_group,
        }
    }

    fn light_frame(&self, time: f32) -> day::Frame {
        let day = self.day.get();
        let mut frame = if day.enabled {
            self.director_sun.get().map_or_else(
                || day.frame(self.sun, time),
                |(sun, _)| day::frame_for_direction(self.sun, sun),
            )
        } else {
            day.frame(self.sun, time)
        };
        if !self.environment {
            frame.sun.direction = self.sun.direction;
            frame.sun.intensity = 0.;
            frame.ambient = [0.; 3];
        } else if self.suppress_sun && !day.enabled {
            frame.sun.intensity = 0.;
        }
        frame
    }
}

/// One additional depth map with its own light camera.
pub(super) struct Cascade {
    depth: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    camera: wgpu::BindGroup,
}

/// The far cascade is re-rendered only when the sun has turned past a threshold.
pub(super) struct FarCascade {
    cascade: Cascade,
    /// Sun direction and fit the current depth contents were rendered with.
    rendered: std::cell::Cell<Option<(Vec3, fit::Fit)>>,
    /// The mover poses (`mover_occlusion::gpu::Runtime::generation`) it was rendered
    /// with, and when.
    movers: std::cell::Cell<(u64, Option<std::time::Instant>)>,
}

/// The far cascade follows movers at most this often; the next refresh after they stop
/// shows their final pose.
const FAR_MOVER_REFRESH: std::time::Duration = std::time::Duration::from_millis(250);

/// Whether the far cascade, drawn with mover poses `drawn` (`Poses::generation`) at `at`,
/// must be drawn again for poses `generation` at `now`.
fn far_follows_movers(
    (drawn, at): (u64, Option<std::time::Instant>),
    generation: u64,
    now: impl FnOnce() -> std::time::Instant,
) -> bool {
    drawn != generation && at.is_none_or(|at| now().duration_since(at) >= FAR_MOVER_REFRESH)
}

/// One cleared depth-only pass onto a lamp shadow face.
fn face_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("SJK lamp shadow face"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// The far cascade follows the sun once it has turned a twelfth of a degree (below a far
/// texel): a running day clock re-renders it every frame or two, a held clock never.
const FAR_REFRESH_COS: f32 = 0.999_999;

/// Receiver uniform shared by world, model and diagnostic shaders; append-only layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Parameters {
    matrix: [[f32; 4]; 4],
    sun: [f32; 4],
    quality: [f32; 4],
    ambient: [f32; 4],
    radiance: [f32; 4],
    /// Far cascade transform; `far_quality` = texel, depth range, unused, present flag.
    far_matrix: [[f32; 4]; 4],
    far_quality: [f32; 4],
    /// Close cascade transform; `close_quality` = texel, depth range, axial depth, present.
    close_matrix: [[f32; 4]; 4],
    close_quality: [f32; 4],
    /// Real-time lighting: brightness scale, light-buffer scale x and y, unused.
    realtime: [f32; 4],
    /// Readability fill tint and strength, independent of the sky environment.
    fill: [f32; 4],
    /// Indirect gain, fill occlusion fraction, two reserved lanes.
    readability: [f32; 4],
}

impl crate::GpuState {
    /// Allocate only at map installation when explicitly requested and an authored sun exists.
    pub(crate) fn with_sun_shadows(mut self) -> Self {
        let scene = self.scene_size();
        self.world_materials.enable_sun_shadows(
            &self.device,
            &self.queue,
            self.context.sun_shadows,
            scene,
            scene[0] / self.size.width.max(1),
        );
        self.world_materials
            .configure_light_preservation(&self.device, self.has_floor_surfaces());
        self.scene_views
            .configure_floor_commands(&self.device, self.world_materials.mirror_arguments());
        if self.world_materials.sun_shadows_active() {
            self.player_shadows.set_enabled(false);
        }
        self
    }
}

impl super::Runtime {
    /// The lighting reflection probes are captured under: sun direction and intensity,
    /// sky colour, light scale and indirect gain (`reflection_probes::relit`). Constant
    /// zero without the real-time model.
    pub(crate) fn lighting_signature(&self) -> super::material_maps::reflections::Signature {
        let Some(shadow) = &self.shadows else {
            return [0.; 9];
        };
        let frame = shadow.light_frame(shadow.time.get());
        let [x, y, z] = frame.sun.direction;
        let [r, g, b] = frame.ambient;
        [
            x,
            y,
            z,
            frame.sun.intensity,
            r,
            g,
            b,
            shadow.light_scale.get(),
            shadow.indirect_readability.get()[0],
        ]
    }

    /// Whether the main view's light buffer is fitted to `scene` (probe captures pack
    /// their faces into its corner).
    pub(crate) fn light_buffer_fits(&self, scene: [u32; 2]) -> bool {
        self.shadows
            .as_ref()
            .and_then(|shadow| shadow.light.as_ref())
            .is_some_and(|light| light.scene == scene)
    }

    /// Current rendered or automatic sunlight for seeding the shot panel.
    pub(crate) fn shot_sun(&self, natural: bool) -> Option<Vec3> {
        self.shadows
            .as_ref()
            .filter(|s| s.day.get().enabled && s.environment)
            .map(|s| {
                let frame = if natural {
                    s.day.get().frame(s.sun, s.time.get())
                } else {
                    s.light_frame(s.time.get())
                };
                Vec3::from_array(frame.sun.direction)
            })
    }

    /// Unmodified day-clock sun, for continuous handoffs out of Camera control.
    pub(crate) fn natural_sun(&self, time: f32) -> Option<Vec3> {
        self.shadows
            .as_ref()
            .filter(|s| s.day.get().enabled)
            .map(|s| Vec3::from_array(s.day.get().frame(s.sun, time).sun.direction))
    }

    /// The sun the clouds are lit by this frame: the lighting passes' sun (day clock or
    /// director included) when the real-time model runs, else the sky's authored sun;
    /// its strength relative to the map's own, and the sky's radiance scale.
    pub(crate) fn cloud_light(&self) -> Option<crate::weather::clouds::SkyLight> {
        let radiance = self.sky.radiance;
        if let Some(shadow) = &self.shadows {
            let frame = shadow.light_frame(shadow.time.get());
            let strength = if shadow.sun.intensity > 0.0 {
                frame.sun.intensity / shadow.sun.intensity
            } else {
                1.0
            };
            return Some(crate::weather::clouds::SkyLight {
                direction: frame.sun.direction,
                color: frame.sun.color,
                strength: if shadow.environment { strength } else { 1.0 },
                radiance,
            });
        }
        self.sky.sun.map(|sun| crate::weather::clouds::SkyLight {
            direction: sun.direction,
            color: sun.color,
            strength: 1.0,
            radiance,
        })
    }

    /// Publish one directed sun to every lighting pass; no resource recreation.
    pub(crate) fn set_director_sun(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        sun: Option<(Vec3, f32)>,
    ) {
        if let Some(shadow) = &self.shadows {
            let sun = sun.and_then(|(v, weight)| v.try_normalize().map(|v| (v, weight)));
            if shadow.director_sun.replace(sun) != sun {
                self.sky.update_director_sun(queue, sun);
            }
        }
    }

    /// Update already-installed day resources; shape/enabling changes still require restart.
    pub(crate) fn update_day_clock(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        values: [f32; 4],
    ) {
        let Some(shadow) = &self.shadows else {
            return;
        };

        shadow.clarity.set(values[2]);
        shadow.light_scale.set(values[3]);
        let mut day = shadow.day.get();
        if !day.enabled || (day.hour == values[0] && day.minutes == values[1]) {
            return;
        }
        day.hour = values[0];
        day.minutes = values[1];
        shadow.day.set(day);
        self.sky
            .update_day_clock(queue, [values[0], values[1]], day::azimuth(&shadow.sun));
    }
    /// Live `r_dayDebug` (terms to leave out of the real-time light).
    pub(crate) fn set_day_debug(&self, bits: u32) {
        if let Some(shadow) = &self.shadows {
            shadow.debug.set(bits);
        }
    }

    /// Live ambient readability contribution, shared by world and actors.
    pub(crate) fn set_ambient_fill(&self, amount: f32) {
        if let Some(shadow) = &self.shadows {
            if amount.is_finite() {
                shadow.ambient_fill.set(amount.clamp(0., 0.2));
            }
        }
    }
    /// Live indirect gain and fill occlusion, shared by world, actors and reflections.
    pub(crate) fn set_indirect_readability(&self, values: [f32; 2]) {
        if let Some(shadow) = &self.shadows {
            if values.iter().all(|v| v.is_finite()) {
                shadow
                    .indirect_readability
                    .set([values[0].clamp(0., 4.), values[1].clamp(0., 1.)]);
            }
        }
    }

    /// Live `r_sunShadowGapClose` width in world units.
    pub(crate) fn set_gap_close(&self, width: f32) {
        if let Some(shadow) = &self.shadows {
            shadow.gap_width.set(width);
        }
    }
    /// Use the same presentation time as the sky; no simulation clock is advanced here.
    pub(crate) fn shadow_time(&self, time: f32) {
        if let Some(shadows) = &self.shadows {
            shadows.time.set(time);
        }
    }
    /// Secondary views must never replace the main camera used to fit the shadow projection.
    pub(crate) fn shadow_view(&self, view: glam::Mat4) {
        if let Some(shadows) = &self.shadows {
            shadows.view.set(Some(view));
        }
    }
    /// Refit the light buffer to a new scene size and supersampling factor; no-op in the
    /// baked mode.
    pub(crate) fn fit_light_buffer(
        &mut self,
        device: &wgpu::Device,
        scene: [u32; 2],
        supersampling: u32,
    ) {
        let divisor = light_divisor(supersampling);
        let Some(shadow) = &mut self.shadows else {
            return;
        };
        if shadow
            .light
            .as_ref()
            .is_none_or(|light| light.scene == scene && light.divisor == divisor)
        {
            return;
        }
        let preserve = shadow
            .light
            .as_ref()
            .is_some_and(|l| l.preservation_enabled());
        let directed = self.forge.directed_light();
        let mut light = light_buffer::LightBuffer::new(device, scene, divisor, directed);
        light.configure_preservation(device, preserve);
        shadow.light = Some(light);
        shadow.receiver = shadow.build_receiver(device);
        shadow.light_group = shadow.build_light_group(device);
        shadow.mirror_groups = shadow.build_mirror_groups(device);
        shadow.cache_group = cache_group(device, self.lamp_cache.as_ref(), shadow.light.as_ref());
        let receiver = shadow.receiver.clone();
        let mirror = shadow
            .mirror_groups
            .as_ref()
            .map(|groups| groups.receiver.clone());
        self.configure_model_sun(device, Some(&receiver), mirror.as_ref());
    }
    /// Configure map-local shadow resources, without changing shared material shaders.
    /// `scene` is the main-view raster size the light buffer is fitted to, `supersampling`
    /// the render-scale factor it already contains.
    pub(crate) fn enable_sun_shadows(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        settings: settings::Settings,
        scene: [u32; 2],
        supersampling: u32,
    ) {
        self.shadows = None;
        self.configure_model_sun(device, None, None);
        self.sky.configure_day(
            device,
            &self.forge.camera_layout,
            self.forge.format,
            settings.day,
        );
        if !settings.enabled
            || (!self.has_ssao_receivers() && settings.volumetrics == 0 && !settings.day.enabled)
        {
            return;
        }
        let authored = self.sky.sun.filter(|s| {
            s.intensity > 0.
                && Vec3::from_array(s.direction).is_finite()
                && Vec3::from_array(s.direction).length_squared() > 0.5
        });
        let environment = authored.is_some() || self.surfaces_by_source.iter().any(|s| s.sky);
        // Allocate local lighting even without sun/sky. The fallback direction only
        // supports shared shadow resources; it must not inject illumination indoors.
        let sun = match authored {
            Some(sun) => sun,
            None if settings.day.enabled => {
                let sun = super::lighting_environment::default_sun();
                crate::log::progress(format_args!(
                    "Lighting environment: {}",
                    if environment {
                        "sky with a default sun"
                    } else {
                        "local sources only"
                    }
                ));
                sun
            }
            None => {
                crate::log::progress(format_args!("Sun shadows unavailable: no authored sky sun"));
                return;
            }
        };
        self.sky.update_day_clock(
            queue,
            [settings.day.hour, settings.day.minutes],
            day::azimuth(&sun),
        );
        let mut shadow = resources::new(
            device,
            queue,
            &self.forge,
            sun,
            settings,
            self.gi.as_ref(),
            scene,
            light_divisor(supersampling),
            &self.lamps,
            &self.mover_occluders,
            self.lamp_cache_pages.as_ref(),
            self.shadow_bounds,
            &self.probe_domain,
        );
        if settings.day.enabled && self.lamp_cache.is_none() {
            self.lamp_cache = self
                .lamp_cache_pages
                .as_ref()
                .filter(|_| self.gi.as_ref().is_some_and(|gi| gi.fixtures.is_some()))
                .map(|pages| lamp_cache::Cache::new(device, pages, self.forge.directed_light()));
        }
        shadow.cache_group = cache_group(device, self.lamp_cache.as_ref(), shadow.light.as_ref());
        shadow.environment = environment;
        shadow.suppress_sun = self.environment_policy.suppress_sun;
        // Real-time lighting replaces the lightmap in the stage shader; the multiplicative
        // actor-shade receivers only serve the baked mode.
        shadow.receivers = if settings.day.enabled {
            Vec::new()
        } else {
            self.ssao.receivers.clone()
        };
        if settings.day.enabled {
            crate::log::progress(format_args!(
                "Lighting: {} with sun cascades and geometry GI in a half-resolution buffer \
                 for a {}x{} scene; authored hour {:.2}",
                "source lighting",
                scene[0],
                scene[1],
                day::authored_hour(sun)
            ));
        }
        if shadow.far.is_some() {
            let texel = volume::fit_map(
                Vec3::from_array(sun.direction),
                self.shadow_bounds,
                shadow.settings.resolution,
            )
            .map_or(0., |fit| fit.texel);
            crate::log::progress(format_args!(
                "Sun cascades: close {}-unit fit, {}-unit view fit, map-wide far at \
                 {texel:.2} units/texel; casters culled per cascade",
                shadow.settings.near, shadow.settings.distance
            ));
        }
        if settings.volumetrics > 0 {
            shadow.volumetrics = Some(volumetric_light::Runtime::new(
                device,
                self.forge.format,
                &shadow.depth,
                shadow
                    .close
                    .as_ref()
                    .map_or(&shadow.depth, |close| &close.depth),
                shadow.far.as_ref().map(|far| &far.cascade.depth),
                shadow.held.as_ref().map(|w| [w[0].depth(), w[1].depth()]),
                &self.fog.table,
                settings.volumetrics,
            ));
        }
        let models = settings.day.enabled;

        if models {
            self.configure_model_sun(device, Some(&shadow.receiver), None);
        }
        // Converge the probes now so the first frame is lit; the far cascade for the sun
        // does not exist yet, so this pass is sky and emission, refined in play.
        if let (Some(probes), Some(voxels)) = (&shadow.probes, &self.gi) {
            let frame = shadow.light_frame(0.);
            let light = super::gi_probes::Light {
                direction: Vec3::from_array(frame.sun.direction),
                color: frame.sun.color,
                intensity: frame.sun.intensity / 175.,
                sky: frame.ambient,
            };
            let started = std::time::Instant::now();
            for _ in 0..3 {
                let mut encoder = device.create_command_encoder(&Default::default());
                probes.update_window(
                    &mut encoder,
                    queue,
                    voxels,
                    &light,
                    None,
                    probes.live_count(),
                );
                queue.submit([encoder.finish()]);
            }
            crate::log::progress(format_args!(
                "GI probes converged at installation: 3 passes \
                over {} probes queued in {:.0} ms",
                probes.live_count(),
                started.elapsed().as_secs_f64() * 1e3
            ));
        }
        // The light pass's first frame would otherwise compile these mid-frame.
        let realtime = settings.day.enabled && shadow.light.is_some();
        self.shadows = Some(shadow);
        if realtime && self.forge.model_sun.is_some() {
            let started = std::time::Instant::now();
            self.prewarm_depth_prime();
            let cached = self.prewarm_cached_receivers();
            crate::log::progress(format_args!(
                "Light pass: depth priming{} pipelines compiled at load in {:.0} ms",
                if cached {
                    " and lamp cache receiver"
                } else {
                    ""
                },
                started.elapsed().as_secs_f64() * 1e3
            ));
        }
    }
    /// Mirror the main-view shader's live-material gate, including neutral resources
    /// before a valid frame. Secondary views select their legacy pipelines separately.
    pub(crate) fn realtime_materials_active(&self) -> bool {
        self.shadows
            .as_ref()
            .is_some_and(|s| s.settings.day.enabled && s.light.is_some())
            && self.forge.model_sun.as_ref().is_some_and(|s| s.ready.get())
    }

    /// Place this frame's movers for lamp shadows (`mover_occlusion.rs`); the movers of
    /// `baselines` (with whether each is `EF_PERMANENT`) that no snapshot has shown yet
    /// stand at their spawn pose. With `eye`, `presented` is that snapshot's: a mover it
    /// would hold but does not (`mover_occlusion::Sight`) stops blocking.
    pub(crate) fn observe_movers(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        presented: &[crate::movers::Presented],
        baselines: Option<impl Iterator<Item = (crate::movers::Presented, bool)>>,
        mesh_of: impl Fn(usize) -> Option<usize>,
        eye: Option<super::mover_occlusion::Eye<'_>>,
    ) {
        let areas = &self.areas;
        let in_view = eye.map(|eye| {
            move |sight: &super::mover_occlusion::Sight| {
                // An eye outside the map tells nothing: keep the movers as they are.
                eye.cluster.is_some_and(|from| {
                    sight.seen(
                        |to| {
                            eye.visibility
                                .is_none_or(|pvs| pvs.is_cluster_visible(from, to))
                        },
                        |area| areas.open(area),
                    )
                })
            }
        });
        if let Some(movers) = self.shadows.as_mut().and_then(|s| s.movers.as_mut()) {
            movers.observe(queue, presented, baselines, mesh_of, in_view);
        }
    }

    /// The positions of the lamps occluder `occluder` shadows, for world shots, and
    /// whether door tiles are still waiting to be traced.
    #[cfg(test)]
    pub(crate) fn door_lamps(&self, occluder: usize) -> (Vec<Vec3>, bool) {
        let Some(movers) = self.shadows.as_ref().and_then(|s| s.movers.as_ref()) else {
            return (Vec::new(), false);
        };
        let (lamps, busy) = movers.door_lamps(occluder);
        (
            lamps
                .iter()
                .map(|&lamp| self.lamps.lamps[lamp as usize].position)
                .collect(),
            busy,
        )
    }

    /// Whether real main-view actor shadows replace the temporary blob producer.
    pub(crate) fn sun_shadows_active(&self) -> bool {
        self.shadows.is_some()
    }

    /// Render opaque actors, plus BSP casters only in the explicit world-volume mode.
    /// `phases` marks the cascades, probe refresh and lamp shadows for the frame budget.
    pub(crate) fn draw_sun_casters(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &crate::frame_queue::FrameQueue,
        input: &FrameDraw<'_>,
        instances: &[crate::ActorInstance],
        actor_end: u32,
        phases: Option<&crate::gpu_phases::Profiler>,
    ) -> bool {
        if let Some(sun) = &self.forge.model_sun {
            sun.ready.set(false);
        }
        let Some(shadow) = &self.shadows else {
            return false;
        };
        if let Some(medium) = &shadow.volumetrics {
            medium.invalidate();
        }
        // Door tiles before anything samples lamp visibility this frame.
        if let Some(movers) = &shadow.movers {
            movers.encode(encoder);
        }
        if let Some(phases) = phases {
            phases.mark(encoder, "door-tiles");
        }
        let frame = shadow.light_frame(shadow.time.get());
        let sun = Vec3::from_array(frame.sun.direction);
        let fit = if shadow.settings.world {
            shadow
                .view
                .get()
                .and_then(|view| self.view_fit(shadow, 0, view, sun, shadow.settings.distance))
        } else {
            fit::fit_visible(
                &instances[..(actor_end as usize).min(instances.len())],
                sun,
                shadow.settings.resolution,
                shadow.view.get(),
            )
            .map(|fit| (fit, true))
        };
        let Some((fit, fresh)) = fit else {
            return false;
        };

        let camera = crate::CameraUniform {
            view_projection: fit.matrix.to_cols_array_2d(),
            camera_position: [0.; 3],
            shader_time: 0.,
            view_forward: [0., 0., -1.],
            _padding: 0.,
        };
        queue.write_buffer(&shadow.camera_buffer, 0, bytemuck::bytes_of(&camera));
        let far = shadow.far.as_ref().and_then(|far| {
            let (fit, fresh) = self.refresh_far_cascade(
                encoder,
                queue,
                far,
                sun,
                shadow.settings.resolution,
                input,
            )?;
            if fresh {
                shadow.bounds.encode(encoder, 2);
            }
            Some(fit)
        });

        let close = shadow.close.as_ref().and_then(|close| {
            let (fit, fresh) = shadow
                .view
                .get()
                .and_then(|view| self.view_fit(shadow, 1, view, sun, shadow.settings.near))?;
            self.render_cascade(
                encoder,
                queue,
                close,
                &fit,
                input,
                Some(actor_end),
                shadow.held.as_ref().map(|held| (&held[1], fresh)),
            );
            if fresh {
                shadow.bounds.encode(encoder, 1);
            }
            Some(fit)
        });
        if let Some(phases) = phases {
            phases.mark(encoder, "cascades");
        }
        if let (Some(probes), Some(voxels), Some(_)) = (&shadow.probes, &self.gi, shadow.view.get())
        {
            let light = super::gi_probes::Light {
                direction: sun,
                color: frame.sun.color,
                intensity: frame.sun.intensity / 175.,
                sky: frame.ambient,
            };
            probes.update(
                encoder,
                queue,
                voxels,
                &light,
                far.map(|f| (f.matrix, f.texel, f.depth)),
            );
        }

        if let Some(phases) = phases {
            phases.mark(encoder, "probes");
        }
        if let (Some(lamp_shadows), Some(view)) = (&shadow.lamp_shadows, shadow.view.get()) {
            self.render_lamp_shadows(
                encoder,
                queue,
                lamp_shadows,
                input,
                instances,
                actor_end,
                view,
            );
        }
        if let Some(phases) = phases {
            phases.mark(encoder, "lamp-shadows");
        }
        if let Some(phases) = phases {
            phases.mark(encoder, "projector-shadows");
        }
        if let (Some(medium), Some(view)) = (&shadow.volumetrics, shadow.view.get()) {
            medium.update(
                queue,
                view,
                fit.matrix,
                frame.sun,
                shadow.settings.distance,
                shadow.clarity.get(),
                fit.texel,
                close.map(|c| (c.matrix, c.texel, shadow.settings.near)),
                far.map(|f| (f.matrix, f.texel)),
                self.shadow_bounds,
                self.fog_mode,
            );
        }
        let parameters = Parameters {
            matrix: fit.matrix.to_cols_array_2d(),
            sun: [sun.x, sun.y, sun.z, 0.5],
            // Day mode: sky radiance and sun radiance in display units (sunlit white ≈ 1).
            ambient: [
                frame.ambient[0],
                frame.ambient[1],
                frame.ambient[2],
                f32::from(shadow.settings.day.enabled),
            ],
            radiance: [
                frame.sun.color[0],
                frame.sun.color[1],
                frame.sun.color[2],
                frame.sun.intensity / 175.,
            ],
            quality: [
                fit.texel,
                fit.depth,
                shadow.settings.taps as f32,
                if shadow.settings.world {
                    shadow.settings.distance
                } else {
                    0.
                },
            ],
            far_matrix: far
                .map_or(glam::Mat4::IDENTITY, |f| f.matrix)
                .to_cols_array_2d(),
            far_quality: far.map_or([0.; 4], |f| [f.texel, f.depth, 0., 1.]),
            close_matrix: close
                .map_or(glam::Mat4::IDENTITY, |f| f.matrix)
                .to_cols_array_2d(),
            close_quality: close.map_or([0.; 4], |f| [f.texel, f.depth, shadow.settings.near, 1.]),
            realtime: [
                shadow.light_scale.get(),
                shadow.light.as_ref().map_or(0., |light| light.scale()[0]),
                shadow.light.as_ref().map_or(0., |light| light.scale()[1]),
                (shadow.debug.get()
                    | if shadow.filter_reference.get() {
                        1024
                    } else {
                        0
                    }) as f32,
            ],
            fill: [0.95, 0.97, 1., shadow.ambient_fill.get()],
            readability: {
                let [gain, occlusion] = shadow.indirect_readability.get();
                [gain, occlusion, 0., 0.]
            },
        };
        queue.write_buffer(&shadow.receiver_buffer, 0, bytemuck::bytes_of(&parameters));
        let main = Cascade {
            depth: shadow.depth.clone(),
            camera_buffer: shadow.camera_buffer.clone(),
            camera: shadow.camera.clone(),
        };
        self.render_cascade(
            encoder,
            queue,
            &main,
            &fit,
            input,
            Some(actor_end),
            shadow
                .held
                .as_ref()
                .filter(|_| shadow.settings.world)
                .map(|held| (&held[0], fresh)),
        );
        if fresh && shadow.settings.world {
            shadow.bounds.encode(encoder, 0);
        }
        if let Some(sun) = &self.forge.model_sun {
            sun.ready.set(true);
        }
        true
    }

    /// Main-camera PVS/areamask cannot cull light occluders: an unseen roof can shadow a
    /// visible floor. Draws are culled only against the light projection's own square.
    /// Mover (inline model) casters with their instance ranges; the actor pipeline must
    /// already be bound. Doors, lifts and `func_static` trims cast like the world they
    /// stand in: left out, sun passed under a wall-base trim as a lit seam.
    fn draw_mover_casters(&self, pass: &mut wgpu::RenderPass<'_>, input: &FrameDraw<'_>) {
        for material in &self.materials {
            if material.blended
                || material.flare
                || !material
                    .stages
                    .first()
                    .is_some_and(|stage| stage.shadow_caster)
            {
                continue;
            }
            for draw in &material.mover_draws {
                if let Some(range) = input
                    .mover_ranges
                    .get(draw.mesh)
                    .filter(|range| !range.is_empty())
                {
                    pass.draw_indexed(draw.indices.clone(), 0, range.clone());
                }
            }
        }
    }

    /// Every static caster as a few joined index runs, without a per-draw light test.
    /// A box outside the light's XY square is clipped by the GPU, cascade fits cover the
    /// map's whole light-space depth, and a depth-only pass does not depend on draw
    /// order: the shadow map matches the former per-draw submission at a fraction of
    /// its thousands of draws and their bounds transforms.
    fn draw_world_casters(&self, pass: &mut wgpu::RenderPass<'_>) {
        let runs = self.caster_runs.get_or_init(|| {
            join_runs(
                self.materials
                    .iter()
                    .filter(|material| {
                        !(material.blended || material.flare)
                            && material
                                .stages
                                .first()
                                .is_some_and(|stage| stage.shadow_caster)
                    })
                    .flat_map(|material| material.static_draws.iter().map(|d| d.indices.clone()))
                    .collect(),
            )
        });
        for run in runs {
            pass.draw_indexed(run.clone(), 0, 0..1);
        }
    }

    /// Actor shadow faces for selected lamps; world visibility stays in the static atlas.
    #[allow(clippy::too_many_arguments)]
    fn render_lamp_shadows(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &crate::frame_queue::FrameQueue,
        lamp_shadows: &lamp_shadows::Runtime,
        input: &FrameDraw<'_>,
        instances: &[crate::ActorInstance],
        actor_end: u32,
        view: glam::Mat4,
    ) {
        let Some(shadow) = &self.shadows else {
            return;
        };
        let inverse = view.inverse();
        if !inverse.is_finite() || inverse.z_axis.w.abs() < 1e-8 {
            return;
        }
        let eye = inverse.z_axis.truncate() / inverse.z_axis.w;
        let plan = lamp_shadows.select(&self.lamps, eye);
        let actors = &instances[..(actor_end as usize).min(instances.len())];
        for (slot, lamp) in plan {
            let light = &self.lamps.lamps[lamp];
            let reach = light.radius + 64.;
            // An actor casts into a face when it is within reach and its 64 unit sphere
            // touches that face's 90 degree frustum. A face without one keeps its clear
            // depth from the last empty pass and is not encoded at all.
            for face in 0..lamp_shadows::Runtime::FACES {
                let casts = |actor: &crate::ActorInstance| {
                    let offset = Vec3::from_array(actor.position) - light.position;
                    offset.length_squared() < reach * reach
                        && lamp_shadows::face_contains(face, offset, 64.)
                };
                let actors_near = actors.iter().any(casts);
                if !actors_near && lamp_shadows.is_clear(slot, face) {
                    continue;
                }
                let (camera, _) = lamp_shadows.face_camera(queue, &self.lamps, slot, lamp, face);
                let mut pass = face_pass(encoder, lamp_shadows.dynamic_layer(slot, face));
                lamp_shadows.set_clear(slot, face, !actors_near);
                if actors_near {
                    pass.set_pipeline(&shadow.caster);
                    pass.set_bind_group(0, camera, &[]);
                    pass.set_bind_group(1, &shadow.empty, &[]);
                    pass.set_bind_group(2, &self.forge.geometry, &[]);
                    pass.set_vertex_buffer(0, input.vertices.slice(..));
                    pass.set_vertex_buffer(1, input.instances.slice(..));
                    pass.set_index_buffer(input.indices.slice(..), wgpu::IndexFormat::Uint32);
                    self.draw_actor_casters(&mut pass, input, actor_end, |range| {
                        actors
                            .get(range.start as usize..(range.end as usize).min(actors.len()))
                            .is_some_and(|drawn| drawn.iter().any(casts))
                    });
                }
            }
        }
        lamp_shadows.publish(queue);
    }

    /// Opaque main-view actors; the actor pipeline must already be bound.
    /// `casts` rejects instance ranges that cannot reach this light's projection.
    fn draw_actor_casters(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        input: &FrameDraw<'_>,
        actor_end: u32,
        casts: impl Fn(Range<u32>) -> bool,
    ) {
        for draw in input.entities {
            if draw.no_depth || draw.instances.start >= actor_end {
                continue;
            }
            if !casts(draw.instances.start..draw.instances.end.min(actor_end)) {
                continue;
            }
            let Some(material) = self.material(draw.material) else {
                continue;
            };
            let Some(stage) = material.stages.first() else {
                continue;
            };
            if material.blended || !stage.shadow_caster {
                continue;
            }
            pass.draw_indexed(
                draw.indices.clone(),
                0,
                draw.instances.start..draw.instances.end.min(actor_end),
            );
        }
    }

    /// The fit of view cascade `index` (0 the view fit, 1 the close cascade), `distance`
    /// deep, and whether its static casters must be drawn: every frame into a fresh frustum
    /// fit, or, held, only when the eye has left the square or the sun has turned.
    fn view_fit(
        &self,
        shadow: &Runtime,
        index: usize,
        view: glam::Mat4,
        sun: Vec3,
        distance: f32,
    ) -> Option<(fit::Fit, bool)> {
        match &shadow.held {
            Some(held) if shadow.settings.held => held[index].fit(
                view,
                sun,
                self.shadow_bounds,
                distance,
                shadow.settings.resolution,
            ),
            _ => volume::fit(
                view,
                sun,
                self.shadow_bounds,
                distance,
                shadow.settings.resolution,
            )
            .map(|fit| (fit, true)),
        }
    }

    /// Render one shadow map: the static casters (the world, where the mode has world
    /// casters), then the moving ones — movers, and actors up to `actors`. With `held`, the
    /// static casters go into their own map when `fresh`; moving casters clear
    /// and fill the separate cascade each frame. Receivers combine their visibility.
    fn render_cascade(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &crate::frame_queue::FrameQueue,
        cascade: &Cascade,
        fit: &fit::Fit,
        input: &FrameDraw<'_>,
        actors: Option<u32>,
        held: Option<(&held::Held, bool)>,
    ) {
        let Some(shadow) = &self.shadows else {
            return;
        };
        let camera = crate::CameraUniform {
            view_projection: fit.matrix.to_cols_array_2d(),
            camera_position: [0.; 3],
            shader_time: 0.,
            view_forward: [0., 0., -1.],
            _padding: 0.,
        };
        queue.write_buffer(&cascade.camera_buffer, 0, bytemuck::bytes_of(&camera));
        fn open<'encoder>(
            encoder: &'encoder mut wgpu::CommandEncoder,
            depth: &wgpu::TextureView,
            clear: bool,
        ) -> wgpu::RenderPass<'encoder> {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK sun cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(1.)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
        }
        let bind = |pass: &mut wgpu::RenderPass<'_>| {
            pass.set_pipeline(&shadow.caster);
            pass.set_bind_group(0, &cascade.camera, &[]);
            pass.set_bind_group(1, &shadow.empty, &[]);
            pass.set_bind_group(2, &self.forge.geometry, &[]);
            pass.set_vertex_buffer(0, input.vertices.slice(..));
            pass.set_vertex_buffer(1, input.instances.slice(..));
            pass.set_index_buffer(input.indices.slice(..), wgpu::IndexFormat::Uint32);
        };
        let statics = |pass: &mut wgpu::RenderPass<'_>| {
            if !shadow.settings.world {
                return;
            }
            pass.set_pipeline(&shadow.world_caster);
            if let Some(hulls) = &self.shadow_hulls {
                hulls.draw(pass, input.vertices);
            }
            self.draw_world_casters(pass);
            pass.set_pipeline(&shadow.caster);
        };
        let moving = |pass: &mut wgpu::RenderPass<'_>| {
            if shadow.settings.world {
                self.draw_mover_casters(pass, input);
            }
            if let Some(actor_end) = actors {
                self.draw_actor_casters(pass, input, actor_end, |_| true);
            }
        };
        let Some((held, fresh)) = held else {
            let mut pass = open(encoder, &cascade.depth, true);
            bind(&mut pass);
            statics(&mut pass);
            moving(&mut pass);
            drop(pass);
            shadow.gap_close.apply(
                &self.forge.device,
                queue,
                encoder,
                &cascade.depth,
                fit.texel,
                shadow.gap_width.get(),
            );
            return;
        };
        if fresh {
            let mut pass = open(encoder, held.depth(), true);
            bind(&mut pass);
            statics(&mut pass);
            drop(pass);
            // The slits closed are the world's; the moving casters have none.
            shadow.gap_close.apply(
                &self.forge.device,
                queue,
                encoder,
                held.depth(),
                fit.texel,
                shadow.gap_width.get(),
            );
        }
        let mut pass = open(encoder, &cascade.depth, true);
        bind(&mut pass);
        moving(&mut pass);
    }

    /// Return the far fit and whether its depth changed, rebuilding only as the sun turns.
    fn refresh_far_cascade(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &crate::frame_queue::FrameQueue,
        far: &FarCascade,
        sun: Vec3,
        resolution: u32,
        input: &FrameDraw<'_>,
    ) -> Option<(fit::Fit, bool)> {
        let movers = self
            .shadows
            .as_ref()
            .and_then(|shadow| shadow.movers.as_ref())
            .map_or(0, |movers| movers.generation());
        let movers_moved = far_follows_movers(far.movers.get(), movers, std::time::Instant::now);
        if let Some((rendered, fit)) = far.rendered.get() {
            if rendered.dot(sun) >= FAR_REFRESH_COS && !movers_moved {
                return Some((fit, false));
            }
        }
        let fit = volume::fit_map(sun, self.shadow_bounds, resolution)?;
        self.render_cascade(encoder, queue, &far.cascade, &fit, input, None, None);
        far.rendered.set(Some((sun, fit)));
        far.movers.set((movers, Some(std::time::Instant::now())));
        Some((fit, true))
    }

    /// Multiply only known diffuse world receivers, before fog and all transparent/emissive work.
    pub(crate) fn draw_sun_receivers(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &crate::DepthTarget,
        input: &FrameDraw<'_>,
    ) {
        let Some(shadow) = &self.shadows else {
            return;
        };
        let mut pass = flares::begin_pass(encoder, color, &depth.view);
        pass.set_bind_group(0, input.camera, &[]);
        pass.set_bind_group(1, shadow.light_group(), &[]);
        pass.set_vertex_buffer(0, input.vertices.slice(..));
        pass.set_index_buffer(input.indices.slice(..), wgpu::IndexFormat::Uint32);
        for &(material, pipeline) in &shadow.receivers {
            pass.set_pipeline(&shadow.receiver_pipelines[pipeline]);
            for draw in &self.materials[material].static_draws {
                if self
                    .areas
                    .visible(&draw.clusters, input.source_cluster, input.visibility)
                {
                    pass.draw_indexed(draw.indices.clone(), 0, 0..1);
                }
            }
        }
    }

    /// Dust samples the current main-view godray source, never a previous or hidden volume.
    pub(crate) fn dust_beams(&self) -> Option<&wgpu::BindGroup> {
        self.shadows
            .as_ref()
            .filter(|s| s.debug.get() & 256 == 0)
            .and_then(|s| s.volumetrics.as_ref())
            .and_then(|medium| medium.dust_beams())
    }

    /// Main-view medium only; reflected/remote views never reuse this volume or depth.
    pub(crate) fn draw_volumetrics(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::BindGroup,
    ) {
        // `r_dayDebug` bit 256: no volumetric composite.
        if let Some(medium) = self
            .shadows
            .as_ref()
            .filter(|s| s.debug.get() & 256 == 0)
            .and_then(|s| s.volumetrics.as_ref())
        {
            medium.draw(encoder, target, depth);
        }
    }
}

/// The cached light program's group, when the map has a cache and the light buffer a
/// coordinate target for it.
fn cache_group(
    device: &wgpu::Device,
    cache: Option<&lamp_cache::Cache>,
    light: Option<&light_buffer::LightBuffer>,
) -> Option<wgpu::BindGroup> {
    let light = light?;
    Some(cache?.group(device, light.receiver_cache()?, light.direct_list()?))
}

/// Sort index ranges and join the touching ones; empty ranges are dropped.
fn join_runs(mut ranges: Vec<Range<u32>>) -> Vec<Range<u32>> {
    ranges.retain(|range| !range.is_empty());
    ranges.sort_by_key(|range| range.start);
    let mut joined: Vec<Range<u32>> = Vec::new();
    for range in ranges {
        match joined.last_mut() {
            Some(last) if last.end >= range.start => last.end = last.end.max(range.end),
            _ => joined.push(range),
        }
    }
    joined
}

#[cfg(test)]
mod far_mover_tests {
    use super::*;

    #[test]
    fn the_far_cascade_follows_movers_at_most_every_quarter_second() {
        let start = std::time::Instant::now();
        let later = |ms| start + std::time::Duration::from_millis(ms);
        // Never drawn with movers: at once.
        assert!(far_follows_movers((0, None), 1, || start));
        // Drawn with these poses: not again.
        assert!(!far_follows_movers((3, Some(start)), 3, || later(1000)));
        // Moved since: after 250 ms, not before.
        assert!(!far_follows_movers((3, Some(start)), 4, || later(249)));
        assert!(far_follows_movers((3, Some(start)), 4, || later(250)));
    }
}
