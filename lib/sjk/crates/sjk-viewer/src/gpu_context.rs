//! Process-lifetime WGPU handles shared by successive map worlds.

use crate::runtime_settings;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use winit::window::Window;

/// Outcome of one production render attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameStatus {
    Rendered,
    Reconfigure,
    Skip,
}

static NEXT_CONTEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Stable graphics context whose surface is never dropped during a map swap.
pub(crate) struct Context {
    /// Startup scene precision, independent of display/HUD attachment format.
    pub(crate) hdr: crate::frame_target::aa::hdr::Settings,
    /// Startup-only dynamic sun-shadow quality policy; changes request a restart.
    pub(crate) sun_shadows: crate::world_materials::shadows::settings::Settings,
    /// Startup scene multiplier, retained across console-free map installs.
    pub(crate) render_scale: u32,
    /// Startup FXAA policy, preserved through background map installation.
    pub(crate) fxaa: bool,

    /// Live color policy survives console-free background map installation.
    pub(crate) post_color: crate::frame_target::aa::color::Settings,
    /// Live scene-light policy shared across map installation and console handoff.
    pub(crate) dynamic_light_settings: crate::dynamic_lights::Settings,
    /// Live soft-particle policy, independent of a particular map.
    pub(crate) soft_particles: crate::particle_draw::settings::Settings,
    /// Live default-off dust-mote intensity, independent of a particular map.
    pub(crate) dust_motes: crate::dust_motes::Settings,
    /// Live weather switch and density, independent of a particular map.
    pub(crate) weather: crate::weather::Settings,
    /// Live eye adaptation and base exposure, independent of a particular map.
    pub(crate) exposure: crate::frame_target::aa::exposure::Settings,
    /// Retained default-off main-world SSAO policy.
    pub(crate) ssao: crate::world_materials::ssao::settings::Settings,
    /// Startup filtering survives map installs, whose inputs deliberately contain no console.
    pub(crate) filtering: crate::world_materials::filtering::Policy,
    /// Startup material-map policy (rend2's latched `r_normalMapping` and friends).
    pub(crate) material_maps: crate::world_materials::material_maps::Settings,
    pub(crate) id: u64,
    pub(crate) window: Option<Arc<Window>>,
    /// Shared with the context a graphics reload makes ([`Self::resampled`]).
    pub(crate) surface: Option<Arc<wgpu::Surface<'static>>>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: crate::frame_queue::FrameQueue,
    pub(crate) format: wgpu::TextureFormat,
    /// The 2D layer draws straight into the swapchain image through a UNORM view
    /// ([`crate::ui_target::direct`]); otherwise it goes through the display pass.
    pub(crate) ui_direct: bool,
    pub(crate) alpha_mode: wgpu::CompositeAlphaMode,
    pub(crate) present_modes: Vec<wgpu::PresentMode>,
    pub(crate) surface_usages: wgpu::TextureUsages,
    pub(crate) adapter_name: String,
    pub(crate) adapter_backend: String,
    /// The anisotropy the adapter offers, which [`Self::filtering`] is clamped to.
    pub(crate) anisotropy_limit: u16,
}

impl Context {
    /// Create the only device and surface used during this process run.
    pub(crate) async fn new(
        window: Option<Arc<Window>>,
        console: Option<&crate::console::ViewerConsole>,
    ) -> Result<Arc<Self>, Box<dyn Error>> {
        let instance = window.as_ref().map_or_else(
            || wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env()),
            |window| {
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                    Box::new(window.clone()),
                ))
            },
        );
        let surface = window
            .as_ref()
            .map(|window| instance.create_surface(window.clone()).map(Arc::new))
            .transpose()?;
        Self::with_surface(instance, surface, window, console).await
    }

    /// The device and surface for a window another engine owns (`rd-sjk`, the renderer
    /// JoF EJK loads): wgpu presents straight into that window, and this context has no
    /// winit window of its own.
    pub(crate) async fn new_hosted(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        console: Option<&crate::console::ViewerConsole>,
    ) -> Result<Arc<Self>, Box<dyn Error>> {
        Self::with_surface(instance, Some(Arc::new(surface)), None, console).await
    }

    async fn with_surface(
        instance: wgpu::Instance,
        surface: Option<Arc<wgpu::Surface<'static>>>,
        window: Option<Arc<Window>>,
        console: Option<&crate::console::ViewerConsole>,
    ) -> Result<Arc<Self>, Box<dyn Error>> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_deref(),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let adapter_info = adapter.get_info();
        crate::crash_report::set_gpu(&adapter_info);
        let hdr = crate::frame_target::aa::hdr::Settings::sample(console);
        let stage_table = adapter
            .features()
            .contains(crate::world_materials::stage_table::FEATURES);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("SJK device"),
                // The stage table binds every world texture in one group, where offered.
                required_features: (adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC)
                    | crate::gpu_phases::features(&adapter)
                    | if stage_table {
                        crate::world_materials::stage_table::FEATURES
                    } else {
                        wgpu::Features::empty()
                    },
                // The lamp light cache attaches a third RGBA32F receiver target.
                required_limits: wgpu::Limits {
                    max_texture_array_layers: adapter.limits().max_texture_array_layers.min(1536),
                    max_color_attachment_bytes_per_sample: adapter
                        .limits()
                        .max_color_attachment_bytes_per_sample
                        .min(crate::world_materials::lamp_cache::ATTACHMENT_BYTES)
                        .max(wgpu::Limits::default().max_color_attachment_bytes_per_sample),
                    max_binding_array_elements_per_shader_stage: if stage_table {
                        adapter
                            .limits()
                            .max_binding_array_elements_per_shader_stage
                            .min(1 << 16)
                    } else {
                        0
                    },
                    // Large community maps: the shared vertex buffer of `amjh3te` is 183 MiB,
                    // above WebGPU's default 128 MiB storage binding (its world install
                    // failed validation) and close to the default 256 MiB buffer size.
                    max_storage_buffer_binding_size: adapter
                        .limits()
                        .max_storage_buffer_binding_size
                        .max(wgpu::Limits::default().max_storage_buffer_binding_size),
                    max_buffer_size: adapter
                        .limits()
                        .max_buffer_size
                        .max(wgpu::Limits::default().max_buffer_size),
                    max_immediate_size: if stage_table { 16 } else { 0 },
                    max_binding_array_sampler_elements_per_shader_stage: if stage_table {
                        adapter
                            .limits()
                            .max_binding_array_sampler_elements_per_shader_stage
                            .min(16)
                    } else {
                        0
                    },
                    ..wgpu::Limits::default()
                },
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                // Small blocks: a map change builds the next world while the last one
                // still lives, and once it goes the 128-256 MiB blocks of `Performance`
                // stayed partly used, holding about 3 GB of the process at 4K after a few
                // map changes (`docs/rendering.md`, "GPU memory").
                memory_hints: wgpu::MemoryHints::Manual {
                    suballocated_device_memory_block_size: (4 << 20)..(16 << 20),
                },
                trace: wgpu::Trace::Off,
            })
            .await?;
        let capabilities = surface
            .as_ref()
            .map(|surface| surface.get_capabilities(&adapter));
        let format =
            capabilities
                .as_ref()
                .map_or(wgpu::TextureFormat::Rgba8UnormSrgb, |capabilities| {
                    capabilities
                        .formats
                        .iter()
                        .copied()
                        .find(wgpu::TextureFormat::is_srgb)
                        .unwrap_or(capabilities.formats[0])
                });
        let downlevel = adapter.get_downlevel_capabilities().flags;
        let ui_direct = crate::ui_target::direct(
            format,
            surface.is_some() && downlevel.contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS),
        );
        let maximum = if downlevel.contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING) {
            16
        } else {
            1
        };
        Ok(Arc::new(Self {
            hdr,
            sun_shadows: crate::world_materials::shadows::settings::Settings::sample(console),
            render_scale: crate::frame_target::scale::requested(console),
            fxaa: crate::frame_target::aa::enabled(console),

            post_color: console.map(|c| c.post_color.clone()).unwrap_or_default(),
            dynamic_light_settings: console
                .map(|c| c.dynamic_light_settings.clone())
                .unwrap_or_default(),
            soft_particles: console
                .map(|c| c.soft_particles.clone())
                .unwrap_or_default(),
            dust_motes: console.map(|c| c.dust_motes.clone()).unwrap_or_default(),
            weather: console.map(|c| c.weather.clone()).unwrap_or_default(),
            exposure: console.map(|c| c.exposure.clone()).unwrap_or_default(),
            ssao: console.map(|c| c.ssao.clone()).unwrap_or_default(),
            filtering: crate::world_materials::filtering::Policy::sample(console, maximum),
            material_maps: crate::world_materials::material_maps::Settings::sample(console),
            id: NEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed),
            window,
            surface,
            device,
            queue: crate::frame_queue::FrameQueue::new(queue),
            format,
            ui_direct,
            alpha_mode: capabilities
                .as_ref()
                .map_or(wgpu::CompositeAlphaMode::Opaque, |value| {
                    value.alpha_modes[0]
                }),
            present_modes: capabilities
                .as_ref()
                .map(|capabilities| capabilities.present_modes.clone())
                .unwrap_or_default(),
            surface_usages: capabilities.as_ref().map_or(
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                |capabilities| capabilities.usages,
            ),
            adapter_name: adapter_info.name,
            adapter_backend: format!("{:?}", adapter_info.backend),
            anisotropy_limit: maximum,
        }))
    }

    /// A context on this one's device, window and surface with the settings read only
    /// at creation (scene precision, FXAA, supersampling, sun and sky, sun shadows, light
    /// shafts, material maps and filtering) read again: the graphics reload
    /// ([`crate::graphics_reload`]) builds the current world on it.
    pub(crate) fn resampled(&self, console: Option<&crate::console::ViewerConsole>) -> Arc<Self> {
        Arc::new(Self {
            hdr: crate::frame_target::aa::hdr::Settings::sample(console),
            sun_shadows: crate::world_materials::shadows::settings::Settings::sample(console),
            render_scale: crate::frame_target::scale::requested(console),
            fxaa: crate::frame_target::aa::enabled(console),
            post_color: self.post_color.clone(),
            dynamic_light_settings: self.dynamic_light_settings.clone(),
            soft_particles: self.soft_particles.clone(),
            dust_motes: self.dust_motes.clone(),
            weather: self.weather.clone(),
            exposure: self.exposure.clone(),
            ssao: self.ssao.clone(),
            filtering: crate::world_materials::filtering::Policy::sample(
                console,
                self.anisotropy_limit,
            ),
            material_maps: crate::world_materials::material_maps::Settings::sample(console),
            id: NEXT_CONTEXT_ID.fetch_add(1, Ordering::Relaxed),
            window: self.window.clone(),
            surface: self.surface.clone(),
            device: self.device.clone(),
            queue: self.queue.clone(),
            format: self.format,
            ui_direct: self.ui_direct,
            alpha_mode: self.alpha_mode,
            present_modes: self.present_modes.clone(),
            surface_usages: self.surface_usages,
            adapter_name: self.adapter_name.clone(),
            adapter_backend: self.adapter_backend.clone(),
            anisotropy_limit: self.anisotropy_limit,
        })
    }

    /// Scene pipelines and their offscreen targets share this format, never the HUD's.
    pub(crate) fn scene_format(&self) -> wgpu::TextureFormat {
        self.hdr.format(self.format)
    }

    /// Select a presentation mode without rebuilding the graphics context.
    pub(crate) fn present_mode(&self, vsync: bool) -> wgpu::PresentMode {
        if self.surface.is_none() {
            wgpu::PresentMode::Fifo
        } else {
            runtime_settings::preferred_present_mode(&self.present_modes, vsync)
        }
    }
}
