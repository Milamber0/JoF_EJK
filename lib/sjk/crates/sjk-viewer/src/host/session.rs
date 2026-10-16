//! One hosted renderer: the graphics context on the engine's window, the 2D layer,
//! the settings console, and the [`GpuState`] of the map the engine loaded.

use super::layer::{Layer, Picture};
use super::View;
use crate::console::ViewerConsole;
use crate::gpu_context::Context;
use crate::{GpuState, GpuWorldInput, assets};
use sjk_scene::{MeshBuildOptions, StaticWorld};
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

/// What the engine hands over at start-up.
pub(crate) struct Start {
    pub(crate) hwnd: isize,
    pub(crate) hinstance: isize,
    pub(crate) size: [u32; 2],
    /// `fs_basepath`: the GameData folder.
    pub(crate) base_path: PathBuf,
    /// `fs_homepath`, where downloads land; may equal `base_path`.
    pub(crate) home_path: PathBuf,
    /// `fs_game`; empty for `base`.
    pub(crate) game: String,
    /// The hosted renderer's own settings file.
    pub(crate) config: PathBuf,
    /// SJK's own profile, read (never written) to start a new hosted profile from
    /// the settings the player chose in SJK.
    pub(crate) sjk_config: Option<PathBuf>,
}

/// Lines of SJK's profile a new hosted profile takes: the renderer's settings.
fn imported(line: &str) -> Option<(&str, String)> {
    let rest = line.trim().strip_prefix("seta ")?;
    let (name, value) = rest.split_once(' ')?;
    let keep = name.starts_with("r_")
        && !matches!(
            name,
            "r_fullscreen"
                | "r_mode"
                | "r_customwidth"
                | "r_customheight"
                | "r_resolution"
                | "r_noborder"
                | "r_centerWindow"
                | "r_exclusiveFullscreen"
        );
    keep.then(|| (name, value.trim().trim_matches('"').to_owned()))
}

pub(crate) struct Host {
    context: Arc<Context>,
    size: [u32; 2],
    game_data: PathBuf,
    vfs: Arc<VirtualFileSystem>,
    shaders: Arc<ShaderCatalog>,
    console: Option<ViewerConsole>,
    gpu: Option<GpuState>,
    map: String,
    surface_configured: bool,
}

impl Host {
    pub(crate) fn start(start: Start) -> Result<Self, Box<dyn Error>> {
        crate::platform::initialize_hosted_storage(start.config.clone())?;
        let mut console = ViewerConsole::new(start.config.clone())?;
        if console.new_profile() {
            // SJK's defaults are its High level; the player's own SJK choices win.
            let mut taken = 0;
            if let Some(text) = start
                .sjk_config
                .as_ref()
                .and_then(|path| std::fs::read_to_string(path).ok())
            {
                for (name, value) in text.lines().filter_map(imported) {
                    taken += usize::from(console.try_set_cvar(name, &value).is_ok());
                }
            }
            crate::log::progress(format_args!(
                "rd-freedom: new settings profile; {taken} renderer settings taken from SJK"
            ));
        }
        // A renderer inside another client: no hub identity, no updates, and the
        // engine's own file system layout (`base`, `EternalJK`, `fs_game`).
        console.set_cvar("cl_identity", "0");
        console.set_cvar("cl_autoUpdate", "0");
        // A crash inside another engine is not SJK's to report to the SJK team.
        console.set_cvar("cl_crashReports", "0");
        console.set_cvar("fs_basegame", crate::host::ETERNALJK);
        console.set_cvar("fs_game", &start.game);
        if start.home_path != start.base_path {
            console.set_cvar("fs_portable", "0");
            console.set_cvar("fs_homepath", &start.home_path.to_string_lossy());
        }
        console.persist();
        assets::search_paths::initialize(&console)?;
        let vfs = assets::mount_game_data(&start.base_path)?;
        let shaders = assets::load_shaders(&vfs);
        crate::log::progress(format_args!(
            "rd-freedom: mounted {} ({} scripted shaders)",
            start.base_path.display(),
            shaders.len()
        ));

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = surface(&instance, start.hwnd, start.hinstance)?;
        let context = pollster::block_on(Context::new_hosted(instance, surface, Some(&console)))?;
        crate::log::progress(format_args!(
            "rd-freedom: {} ({})",
            context.adapter_name, context.adapter_backend
        ));
        let layer = Layer::new(
            &context.device,
            &context.queue,
            crate::ui_target::format(context.format),
        );
        super::install_layer(layer);
        Ok(Self {
            context,
            size: start.size,
            game_data: start.base_path,
            vfs: Arc::new(vfs),
            shaders: Arc::new(shaders),
            console: Some(console),
            gpu: None,
            map: String::new(),
            surface_configured: false,
        })
    }

    pub(crate) fn adapter(&self) -> String {
        format!(
            "SJK renderer: {} ({})",
            self.context.adapter_name, self.context.adapter_backend
        )
    }

    /// Build the world of `map` (`maps/mp/ffa3.bsp`), replacing the last one.
    pub(crate) fn load_world(&mut self, map: &str) -> Result<(), Box<dyn Error>> {
        self.unload_world();
        super::entities::reset();
        let started = std::time::Instant::now();
        let bsp = assets::load_bsp_from_vfs(&self.vfs, map)?;
        let scene = StaticWorld::build(&bsp, MeshBuildOptions::default().with_sky_surfaces())?;
        let (camera_origin, camera_yaw) = assets::initial_camera(&bsp).unwrap_or(([0.0; 3], 0.0));
        let bounds = bsp.render().models()[0].clone();
        let input = GpuWorldInput {
            scene,
            bsp,
            vfs: self.vfs.clone(),
            shaders: (*self.shaders).clone(),
            world_minimums: bounds.minimums,
            world_maximums: bounds.maximums,
            camera_origin,
            camera_yaw,
            player_preview: None,
            live_session: None,
            demo_session: None,
            build_game_state: None,
            build_snapshot: None,
            console: self.console.take(),
            client_menu: None,
            game_data: self.game_data.clone(),
            connect_timeline: None,
            game_fonts: false,
            completed_map_changes: 0,
        };
        match pollster::block_on(GpuState::new_with_context(
            self.context.clone(),
            self.size,
            input,
            None,
        )) {
            Ok(gpu) => {
                self.gpu = Some(gpu);
                self.surface_configured = true;
                self.map = map.to_owned();
                crate::log::progress(format_args!(
                    "rd-freedom: {map} ready in {:.0} ms",
                    started.elapsed().as_secs_f64() * 1000.0
                ));
                Ok(())
            }
            Err(error) => {
                // The console went into the failed build; start a fresh one on the
                // same file so settings keep working.
                self.console = ViewerConsole::new(crate::platform::user_config_file()?).ok();
                Err(error)
            }
        }
    }

    /// Drop the map's world (the engine flushes its renderer between maps).
    pub(crate) fn unload_world(&mut self) {
        if let Some(mut gpu) = self.gpu.take() {
            gpu.frame_pacer.split.wait_previous();
            if self.console.is_none() {
                self.console = gpu.console.take();
            }
            drop(gpu);
            self.map.clear();
        }
    }

    /// The window changed size.
    pub(crate) fn resize(&mut self, size: [u32; 2]) {
        self.size = size;
        self.surface_configured = false;
    }

    /// Present one frame: the world seen from `view` when the engine drew one this
    /// frame, then its 2D `pictures`.
    pub(crate) fn frame(
        &mut self,
        view: Option<View>,
        scene: super::entities::Scene,
        pictures: &[Picture],
    ) {
        super::entities::set_scene(scene);
        super::with_layer(|layer| {
            layer.prepare(&self.context.queue, &self.vfs, &self.shaders, pictures);
        });
        if let (Some(view), Some(gpu)) = (view, self.gpu.as_mut()) {
            super::set_view(Some(view));
            let status = gpu.render(&mut None);
            super::set_view(None);
            if status == crate::gpu_context::FrameStatus::Reconfigure {
                gpu.frame_pacer.split.wait_previous();
                if let Some(surface) = &self.context.surface {
                    surface.configure(&gpu.device, &gpu.configuration);
                }
            }
            return;
        }
        self.frame_2d();
    }

    /// A frame with no world (menus, loading screens): clear, draw the 2D layer, present.
    fn frame_2d(&mut self) {
        let Some(surface) = self.context.surface.clone() else {
            return;
        };
        if let Some(gpu) = &self.gpu {
            gpu.frame_pacer.split.wait_previous();
        }
        if !self.surface_configured {
            surface.configure(&self.context.device, &self.configuration());
            self.surface_configured = true;
        }
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface_configured = false;
                return;
            }
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let ui = crate::ui_target::surface_view(
            &frame.texture,
            self.context.format,
            self.context.ui_direct,
        )
        .unwrap_or_else(|| view.clone());
        let mut encoder = self
            .context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("host 2D frame"),
            });
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("host 2D clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
        }
        super::with_layer(|layer| layer.draw(&mut encoder, &ui));
        self.context.queue.submit(Some(encoder.finish()));
        self.context.queue.raw().present(frame);
    }

    fn configuration(&self) -> wgpu::SurfaceConfiguration {
        if let Some(gpu) = &self.gpu {
            let mut configuration = gpu.configuration.clone();
            configuration.width = self.size[0].max(1);
            configuration.height = self.size[1].max(1);
            return configuration;
        }
        let vsync = self
            .console
            .as_ref()
            .and_then(|console| console.bool_cvar("r_vsync"))
            .unwrap_or(false);
        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.context.format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: self.size[0].max(1),
            height: self.size[1].max(1),
            present_mode: self.context.present_mode(vsync),
            desired_maximum_frame_latency: 2,
            alpha_mode: self.context.alpha_mode,
            view_formats: crate::ui_target::surface_view_formats(
                self.context.format,
                self.context.ui_direct,
            ),
        }
    }

    /// Whether `name` is a shader script or an image the game data has, as rd-vanilla's
    /// `R_FindShader` finds one (else the engine's handle for it is 0).
    pub(crate) fn shader_exists(&self, name: &str) -> bool {
        self.shaders.get(name).is_some()
            || self
                .shaders
                .resolve_image(&self.vfs, name)
                .ok()
                .flatten()
                .is_some()
    }

    fn console_mut(&mut self) -> Option<&mut ViewerConsole> {
        match (&mut self.console, &mut self.gpu) {
            (Some(console), _) => Some(console),
            (None, Some(gpu)) => gpu.console.as_mut(),
            (None, None) => None,
        }
    }

    fn console_ref(&self) -> Option<&ViewerConsole> {
        self.console
            .as_ref()
            .or_else(|| self.gpu.as_ref().and_then(|gpu| gpu.console.as_ref()))
    }

    /// The renderer settings, one line each: `cvar<TAB>value<TAB>default<TAB>help`.
    pub(crate) fn settings(&self) -> String {
        let Some(console) = self.console_ref() else {
            return String::new();
        };
        let mut out = String::new();
        for (_, _, cvar, help) in crate::settings::renderer_rows() {
            let (Some(value), Some(default)) = (console.cvar(cvar), console.cvar_default(cvar)) else {
                continue;
            };
            out.push_str(&format!(
                "{cvar}\t{}\t{}\t{}\n",
                value.as_text(),
                default.as_text(),
                help.replace(['\t', '\n'], " ")
            ));
        }
        out
    }

    /// Run the graphics quality command (`graphicsquality [level]`); returns its lines.
    pub(crate) fn quality_command(&mut self, args: &[String]) -> String {
        let Some(console) = self.console_mut() else {
            return String::new();
        };
        let lines = match crate::graphics_quality::command(console, args) {
            Ok(lines) => lines,
            Err(error) => vec![error],
        };
        console.flush_config();
        lines.join("\n")
    }

    /// The loaded map's renderer, between `load_world` and `unload_world`.
    pub(crate) fn gpu(&self) -> Option<&GpuState> {
        self.gpu.as_ref()
    }

    /// Set a renderer setting by name (the engine's cvar of the same name changed).
    pub(crate) fn set_cvar(&mut self, name: &str, value: &str) -> bool {
        self.console_mut()
            .is_some_and(|console| console.try_set_cvar(name, value).is_ok())
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.unload_world();
        if let Some(console) = &mut self.console {
            console.flush_config();
        }
        super::remove_layer();
    }
}

/// A surface on the engine's window.
#[cfg(windows)]
fn surface(
    instance: &wgpu::Instance,
    hwnd: isize,
    hinstance: isize,
) -> Result<wgpu::Surface<'static>, Box<dyn Error>> {
    use wgpu::rwh::{
        RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
    };
    let hwnd = std::num::NonZeroIsize::new(hwnd).ok_or("the engine gave no window")?;
    let mut window = Win32WindowHandle::new(hwnd);
    window.hinstance = std::num::NonZeroIsize::new(hinstance);
    // SAFETY: the engine created this window before starting the renderer and destroys
    // it only after `SJK_HostShutdown`, which drops the surface first.
    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
            raw_window_handle: RawWindowHandle::Win32(window),
        })
    }?;
    Ok(surface)
}

#[cfg(not(windows))]
fn surface(
    _instance: &wgpu::Instance,
    _hwnd: isize,
    _hinstance: isize,
) -> Result<wgpu::Surface<'static>, Box<dyn Error>> {
    Err("rd-freedom presents into a Win32 window only".into())
}
