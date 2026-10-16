//! Weather: the rain, snow, dust and fog a map asks for, and clouds over open sky;
//! presentation only.
//!
//! A map's weather entities (`fx_rain`, `fx_snow`, `fx_wind`, `fx_spacedust`) make the
//! server register effect names that start with `*` (`*heavyrain`, `*constantwind ( x y
//! z )`, ...). As cgame does (`CG_ParseWeatherEffect`), the client runs them as world
//! effect commands, in slot order ([`effects`]); `r_we` runs one more from the console,
//! and `r_weatherForce` replaces them with weather of the player's choosing. A world
//! without a server takes the names from its own entities.
//!
//! What SJK adds to the reference's particle clouds:
//!
//! - **Cover** ([`cover`], [`cover_map`]): weather exists only under open sky and above
//!   the first surface below it, surveyed from the map's collision data on a worker
//!   thread. Rain stops on roofs and the ground, stays out of buildings, and is cut
//!   exactly at eaves and windows, per pixel.
//! - **Splashes** where rain lands, anchored in the world, and a **far rain** layer.
//! - **Volumetric fog**: rain and snow leave a haze in the open air, and the map's fog
//!   commands become ground fog drifting with the wind, both marched per pixel against
//!   the cover instead of the reference's smoke sprites (`r_weatherFog`).
//! - **Clouds** ([`clouds`]) over every map with sky, lit by its sun, drifting with
//!   the wind and darkening in a storm (`r_clouds`).
//! - **Wet surfaces**: what the rain falls on darkens and mirrors the sky, water runs
//!   down slopes and walls, and flat ground gathers puddles, by quality level, in a
//!   pass over the world before the players are drawn (`fragment_wet`).
//! - Particles are generated on the GPU from their index and the wind the CPU
//!   integrates (`weather.wgsl`) and drawn into the display-space effect layer after the
//!   effects; rain is blended as a faint tinted streak rather than added.
//!
//! `r_weather 0` turns weather off; `r_weatherDensity` scales the particle counts (1 is
//! the reference's, SJK's default 2); `r_weatherQuality` (0 low to 3 ultra) chooses what
//! is drawn ([`settings::Quality`]).

#[path = "weather_clouds.rs"]
pub(crate) mod clouds;
#[path = "weather_cover.rs"]
pub(crate) mod cover;
#[path = "weather_cover_far.rs"]
pub(crate) mod cover_far;
#[path = "weather_cover_map.rs"]
pub(crate) mod cover_map;
#[path = "weather_effects.rs"]
pub(crate) mod effects;
#[path = "weather_gpu.rs"]
pub(crate) mod gpu;
#[path = "weather_noise.rs"]
pub(crate) mod noise;
#[path = "weather_settings.rs"]
pub(crate) mod settings;
#[path = "weather_wind.rs"]
pub(crate) mod wind;

use bytemuck::Zeroable;
use effects::{Cloud, Effects, Image, Look, MAX_CLOUDS, Mist};
use gpu::{BUCKETS, Batch, GpuCloud, GpuWeather, Kind};
pub(crate) use settings::{
    CLOUDS_CVAR, CVAR, DENSITY_CVAR, FOG_CVAR, FORCE_CVAR, QUALITY_CVAR, Settings,
};
use std::sync::Arc;

/// The console command running one world effect command (rd-vanilla's `r_we`).
pub(crate) const COMMAND: &str = "r_we";
pub(crate) const HELP: &str = "Run a weather command: rain, heavyrain, snow, fog, clear, ...";

/// `CS_EFFECTS` and `MAX_FX`.
const CS_EFFECTS: usize = 1355;
const MAX_FX: usize = 64;
/// Terminal speed of a weather particle per unit of force over mass: the reference adds
/// force / mass to the velocity and keeps 0.7 of it every frame (`mFrictionInverse`).
const TERMINAL: f64 = 0.7 / 0.3;
/// The far rain layer's reach and opacity, against the near box.
const FAR_SCALE: f32 = 3.0;
const FAR_OPACITY: f32 = 0.55;
/// Most particles in one batch (the instance number keeps 15 bits for them).
const MAX_BATCH: u32 = 0x7FFF;
/// Weather time wraps here, long before `f32` seconds lose precision.
const TIME_WRAP: f64 = 4096.0;
/// The haze of a heavy storm: what counts as a full storm for the clouds and the fog.
const STORM_HAZE: f32 = 2.8e-4;
/// The longest ray the fog marches; the map's own fog covers what lies beyond.
const FOG_REACH: f32 = 6000.0;
/// One tile of the fog's noise (`fragment_volume`), where its drift wraps.
const FOG_TILE: [f64; 3] = [1400.0, 1400.0, 500.0];
/// The fog's own slow drift, and its share of the wind.
const FOG_DRIFT: [f32; 3] = [24.0, 9.0, 0.0];
const FOG_WIND: f32 = 0.35;
/// Running water on walls: how fast it runs down (units a second), one noise tile of
/// its length (`STREAK_LENGTH` in weather.wgsl, where the scroll wraps), and how fast its
/// pattern changes as it runs (noise tiles a second).
const STREAK_SPEED: f64 = 110.0;
const STREAK_LENGTH: f64 = 520.0;
const STREAK_CHANGE: f64 = 0.04;
/// Rain haze and ground fog colour before light (display values): a cool grey.
const FOG_GREY: [f32; 3] = [0.64, 0.68, 0.74];
/// The rain haze of a downpour: what wets surfaces fully. Drizzle wets them about half.
const SOAKING_HAZE: f32 = 2.8e-4;

/// The map's weather state, its cover and its GPU resources.
pub(crate) struct Runtime {
    /// The server's (or the map's) commands, without their `*`, in slot order.
    server: Vec<String>,
    /// `r_we` commands typed since the map loaded, run after the server's.
    local: Vec<String>,
    /// The `r_weatherForce` the effects were built with.
    force: u32,
    effects: Effects,
    /// The effects include space dust: a space map, which gets no clouds.
    space: bool,
    /// The map's `misc_weather_zone` boxes.
    map_zones: Vec<[[f32; 3]; 2]>,
    /// The zones the running survey was started with.
    surveyed_zones: Option<Vec<[[f32; 3]; 2]>>,
    wind: wind::Wind,
    /// Per cloud, the force integrated over time (units·s per unit of mass).
    flows: [[f64; 3]; MAX_CLOUDS],
    /// The fog's and the clouds' drift.
    fog_flow: [f64; 3],
    /// Running water's scroll down the walls (wrapped to [`STREAK_LENGTH`]) and its
    /// pattern's change (wrapped to one tile).
    streak_flow: f64,
    streak_change: f64,
    cloud_flow: [f64; 2],
    time: f64,
    light: [f32; 3],
    last_frame: Option<std::time::Instant>,
    cover: Option<cover_map::CoverMap>,
    noise: Option<noise::Texture>,
    gpu: Option<gpu::Gpu>,
    clouds: Option<clouds::Gpu>,
    /// Clouds are drawn this frame.
    clouds_visible: bool,
    /// Rain wets the world this frame.
    wet: bool,
    uniform: GpuWeather,
    batches: Vec<Batch>,
}

/// What the frame needs from the rest of the client.
pub(crate) struct FrameInput<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a crate::frame_queue::FrameQueue,
    pub(crate) camera_layout: &'a wgpu::BindGroupLayout,
    pub(crate) scene_format: wgpu::TextureFormat,
    pub(crate) images: Option<(
        &'a sjk_vfs::VirtualFileSystem,
        &'a sjk_shader::ShaderCatalog,
    )>,
    pub(crate) bsp: &'a Arc<sjk_bsp::Bsp>,
    pub(crate) camera: [f32; 3],
    pub(crate) view_projection: [[f32; 4]; 4],
    /// Light at the camera, 0..1 per channel (ambient and directed).
    pub(crate) light: [f32; 3],
    /// The sun for the clouds.
    pub(crate) sky: clouds::SkyLight,
    /// The global fog's `1 / depthForOpaque`, 0 without one.
    pub(crate) fog: f32,
    pub(crate) viewport: [u32; 2],
    pub(crate) settings: &'a Settings,
}

impl Runtime {
    /// The weather of a world being installed: the gamestate's effect names, or without
    /// one the map's own weather entities.
    pub(crate) fn new(game: Option<&sjk_protocol::GameState>, bsp: &sjk_bsp::Bsp) -> Self {
        let entities = sjk_entity::parse_entity_lump(bsp.entities()).unwrap_or_default();
        let server = match game {
            Some(game) => server_commands(game),
            None => entities
                .iter()
                .flat_map(sjk_game_jka::map_scenery::weather_effects)
                .filter_map(|name| name.strip_prefix('*').map(str::to_owned))
                .collect(),
        };
        let map_zones = entities
            .iter()
            .filter(|entity| {
                entity
                    .classname()
                    .is_some_and(|name| name.eq_ignore_ascii_case("misc_weather_zone"))
            })
            .filter_map(|entity| {
                let model = entity.get("model")?.strip_prefix('*')?.parse().ok()?;
                let model = bsp.render().inline_model(model)?;
                Some([model.minimums, model.maximums])
            })
            .collect();
        if !server.is_empty() {
            crate::log::progress(format_args!("weather: {}", server.join(", ")));
        }
        let mut runtime = Self {
            server,
            local: Vec::new(),
            force: 0,
            effects: Effects::default(),
            space: false,
            map_zones,
            surveyed_zones: None,
            wind: wind::Wind::default(),
            flows: [[0.0; 3]; MAX_CLOUDS],
            fog_flow: [0.0; 3],
            streak_flow: 0.0,
            streak_change: 0.0,
            cloud_flow: [0.0; 2],
            time: 0.0,
            light: [1.0; 3],
            last_frame: None,
            cover: None,
            noise: None,
            gpu: None,
            clouds: None,
            clouds_visible: false,
            wet: false,
            uniform: GpuWeather::zeroed(),
            batches: Vec::with_capacity(2 * MAX_CLOUDS + 2),
        };
        runtime.rebuild();
        runtime
    }

    /// The server's effect names changed: rerun every command if its weather did.
    pub(crate) fn refresh_server(&mut self, game: &sjk_protocol::GameState) {
        let server = server_commands(game);
        if server != self.server {
            self.server = server;
            self.rebuild();
        }
    }

    /// `r_we <command>`.
    pub(crate) fn command(&mut self, command: &str) -> Result<(), &'static str> {
        let mut probe = self.effects.clone();
        probe.apply(command)?;
        self.local.push(command.to_owned());
        self.rebuild();
        Ok(())
    }

    /// Run the commands again: the forced weather's or the server's, then the console's.
    fn rebuild(&mut self) {
        let forced = settings::forced_commands(self.force);
        let commands: Vec<&str> = if forced.is_empty() {
            self.server.iter().map(String::as_str).collect()
        } else {
            forced.to_vec()
        };
        self.effects = Effects::from_commands(
            commands
                .into_iter()
                .chain(self.local.iter().map(String::as_str)),
        );
        self.space = self
            .effects
            .clouds
            .iter()
            .any(|cloud| cloud.look == Look::Sprite(Image::SnowPuff));
        self.flows = [[0.0; 3]; MAX_CLOUDS];
    }

    /// Rain to wet the world with this frame ([`Runtime::draw_wet`]).
    pub(crate) fn wet(&self) -> bool {
        self.wet
    }

    /// Rain emitters currently configured, including forced and local weather.
    /// Frozen weather has no new drops to strike a saber.
    pub(crate) fn saber_rain(&mut self, settings: &settings::Settings) -> bool {
        if settings.force() != self.force {
            self.force = settings.force();
            self.rebuild();
        }
        settings.enabled() && !self.effects.frozen && self.effects.clouds.iter().any(Cloud::is_rain)
    }

    /// Weather to draw into the effect layer this frame.
    pub(crate) fn visible(&self) -> bool {
        !self.batches.is_empty()
    }

    /// The cloud pipeline, when clouds are drawn this frame.
    pub(crate) fn clouds(&self) -> Option<&clouds::Gpu> {
        self.clouds.as_ref().filter(|_| self.clouds_visible)
    }

    /// How stormy the weather is, 0..1, from the haze it leaves.
    fn storm(&self) -> f32 {
        let haze: f32 = self.effects.clouds.iter().map(|cloud| cloud.haze).sum();
        (haze / STORM_HAZE).min(1.0)
    }

    /// The ground fog `r_weatherFog` asks for: the densest of the map's, or the default
    /// one on every map (2).
    fn mist(&self, fog: u32) -> Option<(Mist, [f32; 4])> {
        if fog == 0 {
            return None;
        }
        let own = self
            .effects
            .clouds
            .iter()
            .filter_map(|cloud| cloud.mist.map(|mist| (mist, cloud.color)))
            .max_by(|a, b| a.0.density.total_cmp(&b.0.density));
        own.or((fog == 2).then_some((Mist::DEFAULT, [0.5; 4])))
    }

    /// Advance the weather and the clouds and write this frame's uniforms.
    /// Allocation-free once set up, unless the cover window moves.
    pub(crate) fn prepare(&mut self, input: FrameInput<'_>) {
        self.batches.clear();
        self.wet = false;
        let now = std::time::Instant::now();
        let seconds = self
            .last_frame
            .replace(now)
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32().min(0.25));
        let settings = input.settings;
        if settings.force() != self.force {
            self.force = settings.force();
            self.rebuild();
        }
        let quality = settings.quality();
        let wind = self.wind.advance(&mut self.effects.winds, seconds);
        // Light changes ease over about half a second, so walking under a lamp does not
        // flicker the rain.
        let target = input.light.map(|value| (value / 0.7).clamp(0.3, 1.0));
        let ease = 1.0 - (-seconds / 0.4).exp();
        for (light, target) in self.light.iter_mut().zip(target) {
            *light += (target - *light) * if seconds == 0.0 { 1.0 } else { ease };
        }
        let weather = settings.enabled();
        let storm = if weather { self.storm() } else { 0.0 };
        self.prepare_clouds(&input, quality.cloud_steps, wind, storm, seconds);
        if !weather {
            return;
        }

        let mist = self.mist(settings.fog());
        let haze: f32 = self.effects.clouds.iter().map(|cloud| cloud.haze).sum();
        let volume = quality.fog_steps > 0 && (mist.is_some() || haze > 0.0);
        if self.effects.clouds.is_empty() && !volume {
            return;
        }
        let surveyed = self.surveyed_zones.as_deref().is_some_and(|surveyed| {
            surveyed
                .iter()
                .eq(self.map_zones.iter().chain(&self.effects.zones))
        });
        if self.gpu.is_none() || !surveyed {
            let zones = self.zones();
            let cover = self
                .cover
                .get_or_insert_with(|| cover_map::CoverMap::new(input.device));
            cover.start(input.bsp.clone(), cover::Marks::read(input.bsp, &zones));
            self.surveyed_zones = Some(zones);
            if self.gpu.is_none() {
                let noise = self
                    .noise
                    .get_or_insert_with(|| noise::Texture::new(input.device));
                self.gpu = Some(gpu::Gpu::new(
                    input.device,
                    input.queue,
                    input.images,
                    input.camera_layout,
                    input.scene_format,
                    &cover.view,
                    &cover.far_view,
                    noise,
                ));
            }
        }
        let noise_ready = self
            .noise
            .as_mut()
            .is_some_and(|noise| noise.update(input.queue));
        let (Some(cover), Some(gpu)) = (self.cover.as_mut(), self.gpu.as_ref()) else {
            return;
        };
        cover.update(input.queue, input.camera);
        let window = cover.window();
        // Weather the player forced stays off a map without sky, which would otherwise
        // rain indoors as the reference does there.
        if self.force != 0 && !window.enabled {
            return;
        }

        if !self.effects.frozen {
            self.time = (self.time + f64::from(seconds)) % TIME_WRAP;
            for (flow, cloud) in self.flows.iter_mut().zip(&self.effects.clouds) {
                let force = force(cloud, wind);
                for axis in 0..3 {
                    flow[axis] += f64::from(force[axis]) * f64::from(seconds);
                }
            }
            for axis in 0..3 {
                let speed = f64::from(FOG_DRIFT[axis] + FOG_WIND * wind[axis]);
                self.fog_flow[axis] =
                    (self.fog_flow[axis] + speed * f64::from(seconds)).rem_euclid(FOG_TILE[axis]);
            }
            self.streak_flow =
                (self.streak_flow + STREAK_SPEED * f64::from(seconds)).rem_euclid(STREAK_LENGTH);
            self.streak_change =
                (self.streak_change + STREAK_CHANGE * f64::from(seconds)).rem_euclid(1.0);
        }

        self.uniform.window = window.cells;
        self.uniform.far = window.far;
        self.uniform.cover = [
            cover_map::CELL,
            cover_map::SIZE as f32,
            f32::from(u8::from(window.enabled)),
            self.time as f32,
        ];
        if volume {
            self.batches.push(Batch {
                kind: Kind::Volume,
                instances: 0..1,
            });
        }
        // The fog's sprites are drawn only where no volumetric fog replaces them, and not
        // at all with the fog turned off.
        let mist_sprites = !volume && settings.fog() != 0;
        let density = settings.density();
        let mut splash = None;
        for (slot, cloud) in self.effects.clouds.iter().enumerate() {
            self.uniform.clouds[slot] = gpu_cloud(cloud, wind, &self.flows[slot]);
            if cloud.mist.is_some() && !mist_sprites {
                continue;
            }
            let near = ((cloud.count as f32 * density).round() as u32).min(MAX_BATCH);
            if near == 0 {
                continue;
            }
            let far = if cloud.is_rain() {
                ((near as f32 * quality.far_share).round() as u32).min(MAX_BATCH)
            } else {
                0
            };
            let base = (slot as u32) << 16;
            let kind = match cloud.look {
                Look::Streak => Kind::Streak,
                Look::Sprite(_) if cloud.additive => Kind::Sprite,
                Look::Sprite(_) => Kind::SpriteAlpha,
            };
            self.batches.push(Batch {
                kind,
                instances: base..base + near,
            });
            if far != 0 {
                let base = base | 0x8000;
                self.batches.push(Batch {
                    kind,
                    instances: base..base + far,
                });
            }
            if cloud.is_rain() && quality.splashes && splash.is_none() {
                let count = (near as f32 * quality.splash_share) as u32;
                splash = Some((slot, count.min(MAX_BATCH)));
            }
        }
        if let Some((slot, count)) = splash.filter(|&(_, count)| count != 0 && window.enabled) {
            let base = 7 << 16;
            self.batches.push(Batch {
                kind: Kind::Splash,
                instances: base..base + count,
            });
            self.uniform.light[3] = slot as f32;
        }
        self.uniform.view = [
            input.viewport[0] as f32,
            input.viewport[1] as f32,
            input.fog,
            0.0,
        ];
        self.uniform.light[..3].copy_from_slice(&self.light);
        let inverse = glam::Mat4::from_cols_array_2d(&input.view_projection).inverse();
        self.uniform.inverse_view_projection = inverse.to_cols_array_2d();
        let (ground, tint) = mist.map_or((None, [0.5; 4]), |(mist, color)| (Some(mist), color));
        self.uniform.haze = [
            if volume { haze } else { 0.0 },
            ground.map_or(0.0, |mist| mist.density),
            ground.map_or(1.0, |mist| mist.height),
            if volume {
                quality.fog_steps as f32
            } else {
                0.0
            },
        ];
        let (wetness, rain) = wetting(&self.effects.clouds, wind);
        // Without a cover (a map with no sky) the rain falls everywhere; it wets nothing.
        self.wet = quality.wet > 0 && wetness > 0.0 && window.enabled;
        self.uniform.wet = [
            wetness,
            quality.wet as f32,
            self.streak_flow as f32,
            self.streak_change as f32,
        ];
        let sky = wet_sky(input.sky, storm);
        self.uniform.wet_sky = [sky[0], sky[1], sky[2], 0.0];
        self.uniform.rain = [rain[0], rain[1], rain[2], 0.0];
        let color = fog_color(tint, self.light, storm);
        self.uniform.fog_color = [
            color[0],
            color[1],
            color[2],
            f32::from(u8::from(noise_ready)),
        ];
        self.uniform.fog_flow = [
            self.fog_flow[0] as f32,
            self.fog_flow[1] as f32,
            self.fog_flow[2] as f32,
            FOG_REACH,
        ];
        gpu.write(input.queue, &self.uniform);
    }

    /// Advance the clouds and write their uniform; they are drawn unless turned off or
    /// the map is in space.
    fn prepare_clouds(
        &mut self,
        input: &FrameInput<'_>,
        steps: u32,
        wind: [f32; 3],
        storm: f32,
        seconds: f32,
    ) {
        self.clouds_visible = false;
        if !input.settings.clouds() || self.space {
            return;
        }
        let noise = self
            .noise
            .get_or_insert_with(|| noise::Texture::new(input.device));
        let ready = noise.update(input.queue);
        let gpu = self.clouds.get_or_insert_with(|| {
            clouds::Gpu::new(input.device, input.camera_layout, input.scene_format, noise)
        });
        if !self.effects.frozen {
            clouds::drift(&mut self.cloud_flow, wind, seconds);
        }
        let sky = clouds::Sky { storm, wind };
        gpu.write(
            input.queue,
            &clouds::uniform(input.sky, sky, self.cloud_flow, steps, ready),
        );
        self.clouds_visible = ready;
    }

    /// Wet the opaque world just drawn into `target`; `depth` samples the depth it left.
    pub(crate) fn draw_wet(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        camera: &wgpu::BindGroup,
        depth: &wgpu::BindGroup,
    ) {
        if let Some(gpu) = self.gpu.as_ref().filter(|_| self.wet) {
            gpu.draw_wet(encoder, target, camera, depth);
        }
    }

    /// Record this frame's weather into the effect layer's pass.
    pub(crate) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        depth: &'pass wgpu::BindGroup,
    ) {
        if let Some(gpu) = &self.gpu {
            gpu.draw(pass, camera, depth, &self.batches);
        }
    }

    fn zones(&self) -> Vec<[[f32; 3]; 2]> {
        self.map_zones
            .iter()
            .chain(&self.effects.zones)
            .copied()
            .collect()
    }
}

/// The `*` effect names of the gamestate, in slot order, without the `*`.
fn server_commands(game: &sjk_protocol::GameState) -> Vec<String> {
    (1..MAX_FX)
        .filter_map(|slot| game.config_string(CS_EFFECTS + slot))
        .filter_map(|bytes| bytes.strip_prefix(b"*"))
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect()
}

/// The fog's colour (display values): the fog command's own hue, or a cool grey for rain
/// haze and grey fog, in the light where the camera is, darker in a storm.
fn fog_color(tint: [f32; 4], light: [f32; 3], storm: f32) -> [f32; 3] {
    let peak = tint[0].max(tint[1]).max(tint[2]);
    let grey = (tint[0] - tint[1]).abs() < 0.02 && (tint[1] - tint[2]).abs() < 0.02;
    let hue = if grey || peak <= 0.0 {
        FOG_GREY
    } else {
        [0, 1, 2].map(|channel| tint[channel] / peak * 0.72)
    };
    std::array::from_fn(|channel| hue[channel] * light[channel] * (1.0 - 0.3 * storm))
}

/// How wet the rain makes what it falls on (0 dry, 1 a downpour) and the direction it
/// falls in: the rain clouds' haze, and the densest one's force.
fn wetting(clouds: &[Cloud], wind: [f32; 3]) -> (f32, [f32; 3]) {
    let rain = clouds.iter().filter(|cloud| cloud.is_rain());
    let haze: f32 = rain.clone().map(|cloud| cloud.haze).sum();
    let Some(densest) = rain.max_by(|a, b| a.haze.total_cmp(&b.haze)) else {
        return (0.0, [0.0, 0.0, -1.0]);
    };
    let force = glam::Vec3::from(force(densest, wind));
    let direction = force.try_normalize().unwrap_or(glam::Vec3::NEG_Z);
    let wetness = (0.35 + 0.65 * haze / SOAKING_HAZE).min(1.0);
    (wetness, direction.to_array())
}

/// The overcast sky a wet surface mirrors, in the scene's light units: the clouds' own
/// skylight, dimmer at night and in a storm.
fn wet_sky(sky: clouds::SkyLight, storm: f32) -> [f32; 3] {
    let level = sky.radiance.max(0.0) * (0.08 + 0.92 * sky.strength.clamp(0.0, 1.0));
    [0.46, 0.52, 0.6].map(|channel| channel * level * (1.0 - 0.4 * storm))
}

/// Gravity and wind on a cloud's particles.
fn force(cloud: &Cloud, wind: [f32; 3]) -> [f32; 3] {
    [wind[0], wind[1], wind[2] - cloud.gravity]
}

/// A cloud as the shader reads it: its look, its box, and the velocity and wrapped flow
/// of each mass bucket.
fn gpu_cloud(cloud: &Cloud, wind: [f32; 3], flow: &[f64; 3]) -> GpuCloud {
    let [low, high] = cloud.range;
    let size: [f32; 3] = std::array::from_fn(|axis| (high[axis] - low[axis]).max(1.0));
    let force = force(cloud, wind);
    let mut result = GpuCloud {
        color: cloud.color,
        shape: match cloud.look {
            Look::Streak => [cloud.width * 0.5, cloud.height, 0.0, 0.0],
            Look::Sprite(image) => [
                cloud.width,
                cloud.height,
                image as u32 as f32,
                f32::from(u8::from(cloud.rotates)),
            ],
        },
        box_min: [low[0], low[1], low[2], 0.0],
        box_size: [size[0], size[1], size[2], 0.0],
        layers: [FAR_SCALE, FAR_OPACITY, 0.0, 0.0],
        velocity: [[0.0; 4]; BUCKETS],
        offset: [[0.0; 4]; BUCKETS],
    };
    // The flow wraps to the far box across and the box up and down: whole multiples of
    // the near box, so both layers read the same offset.
    let wrap = [
        f64::from(size[0] * FAR_SCALE),
        f64::from(size[1] * FAR_SCALE),
        f64::from(size[2]),
    ];
    for bucket in 0..BUCKETS {
        let share = (bucket as f32 + 0.5) / BUCKETS as f32;
        let mass = f64::from(cloud.mass[0] + (cloud.mass[1] - cloud.mass[0]) * share).max(0.001);
        for axis in 0..3 {
            result.velocity[bucket][axis] = (TERMINAL * f64::from(force[axis]) / mass) as f32;
            result.offset[bucket][axis] =
                (TERMINAL * flow[axis] / mass).rem_euclid(wrap[axis]) as f32;
        }
    }
    result
}

/// C `atoi`: the leading integer, 0 without one.
pub(crate) fn atoi(text: &str) -> i64 {
    let text = text.trim_start();
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    sign * digits[..end].parse::<i64>().unwrap_or(0)
}

/// C `atof`: the longest leading number, 0 without one.
pub(crate) fn atof(text: &str) -> f32 {
    let text = text.trim_start();
    (1..=text.len())
        .rev()
        .filter(|&end| text.is_char_boundary(end))
        .find_map(|end| text[..end].parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

impl crate::GpuState {
    /// Advance the weather for the main view about to be drawn.
    pub(crate) fn prepare_weather(&mut self, camera: &crate::camera_uniform::CameraUniform) {
        let light = self
            .entity_lighting
            .sample(&self.bsp, camera.camera_position, &[]);
        let light: [f32; 3] =
            std::array::from_fn(|channel| light.ambient[channel] + 0.5 * light.directed[channel]);
        let fogs = self.world_materials.fogs();
        let fog = if self.world_materials.fog_mode == crate::fog_volumes::Mode::Off {
            0.0
        } else {
            fogs.entries[1..=fogs.count]
                .iter()
                .find(|fog| fog.color[3] != 0.0)
                .map_or(0.0, |fog| fog.bounds_min[3] * 8.0)
        };
        let viewport = self
            .post_aa
            .as_ref()
            .and_then(|aa| aa.effect_layer())
            .map_or(
                [self.configuration.width, self.configuration.height],
                |layer| layer.size(),
            );
        let vfs = self.vfs.clone();
        let sky = self
            .world_materials
            .cloud_light()
            .unwrap_or(clouds::SkyLight::DEFAULT);
        self.weather.prepare(FrameInput {
            device: &self.device,
            queue: &self.queue,
            camera_layout: &self.camera_layout,
            scene_format: self.context.scene_format(),
            images: vfs.as_deref().map(|vfs| (vfs, &*self.shaders)),
            bsp: &self.bsp,
            camera: camera.camera_position,
            view_projection: camera.view_projection,
            light,
            sky,
            fog,
            viewport,
            settings: &self.context.weather,
        });
    }

    /// The clouds on the visible sky faces, right after the sky in the main view.
    pub(crate) fn draw_clouds<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        source_cluster: Option<usize>,
        visibility: Option<&'pass sjk_bsp::Visibility>,
    ) {
        let Some(clouds) = self.weather.clouds() else {
            return;
        };
        clouds.bind(pass, &self.camera_bind_group);
        self.world_materials.draw_sky_faces(
            pass,
            &self.geometry.vertex_buffer,
            &self.geometry.index_buffer,
            source_cluster,
            visibility,
        );
    }

    /// `r_we`: run one world effect command for this map.
    pub(crate) fn weather_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let command = args.join(" ");
        self.weather
            .command(&command)
            .map(|()| Vec::new())
            .map_err(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_number_parsing_takes_the_leading_number() {
        assert_eq!(atoi("300"), 300);
        assert_eq!(atoi("  -12abc"), -12);
        assert_eq!(atoi("x"), 0);
        assert_eq!(atof("-5000.000000"), -5000.0);
        assert_eq!(atof("1.5e2x"), 150.0);
        assert_eq!(atof("nope"), 0.0);
    }

    #[test]
    fn rain_falls_at_the_reference_terminal_speed_and_both_layers_share_the_flow() {
        let effects = Effects::from_commands(["heavyrain"]);
        let cloud = &effects.clouds[0];
        let wind = [-5000.0, 0.0, 0.0];
        let flow = [-123_456.0, 7.0, -98_765.0];
        let gpu = gpu_cloud(cloud, wind, &flow);
        // The lightest bucket: mass 5.3125, so 2800 / m × 7/3 downwards.
        let mass = 5.0 + 5.0 * 0.5 / BUCKETS as f32;
        let expected = -(TERMINAL as f32) * 2800.0 / mass;
        assert!((gpu.velocity[0][2] - expected).abs() < 0.01);
        assert!(gpu.velocity[0][0] < -2000.0, "the wind carries it");
        for bucket in gpu.offset {
            assert!((0.0..1250.0 * FAR_SCALE).contains(&bucket[0]));
            assert!((0.0..1250.0).contains(&bucket[2]));
        }
        assert_eq!(gpu.shape[..2], [0.6, 80.0]);
    }

    fn empty_world() -> Runtime {
        Runtime::new(None, &sjk_bsp::Bsp::empty([-64.0; 3], [64.0; 3]))
    }

    #[test]
    fn forced_weather_replaces_the_maps_and_keeps_console_commands() {
        let mut weather = empty_world();
        weather.server = vec!["heavyrain".into()];
        weather.command("fog").unwrap();
        weather.force = 4;
        weather.rebuild();
        let looks: Vec<_> = weather
            .effects
            .clouds
            .iter()
            .map(|cloud| cloud.look)
            .collect();
        assert_eq!(
            looks,
            [Look::Sprite(Image::Snowflake), Look::Sprite(Image::Smoke)]
        );
        weather.force = 0;
        weather.rebuild();
        assert_eq!(weather.effects.clouds[0].look, Look::Streak);
        weather.command("spacedust 100").unwrap();
        assert!(weather.space, "space maps get no clouds");
    }

    #[test]
    fn rain_wets_by_its_haze_and_falls_with_the_wind() {
        // Running water's scroll wraps where the shader's noise tile repeats.
        let shader = include_str!("weather.wgsl");
        assert!(shader.contains(&format!("const STREAK_LENGTH: f32 = {STREAK_LENGTH:.1};")));
        let clouds = |commands: &[&str]| Effects::from_commands(commands.iter().copied()).clouds;
        assert_eq!(
            wetting(&clouds(&["snow"]), [0.0; 3]).0,
            0.0,
            "snow is not rain"
        );
        assert_eq!(wetting(&[], [0.0; 3]), (0.0, [0.0, 0.0, -1.0]));
        let (drizzle, down) = wetting(&clouds(&["lightrain"]), [0.0; 3]);
        let (downpour, _) = wetting(&clouds(&["heavyrain"]), [0.0; 3]);
        assert!(drizzle > 0.4 && drizzle < 0.7, "{drizzle}");
        assert_eq!(downpour, 1.0);
        assert_eq!(down, [0.0, 0.0, -1.0]);
        // A wind along +x slants the rain that way.
        let (_, slanted) = wetting(&clouds(&["rain"]), [2000.0, 0.0, 0.0]);
        assert!(slanted[0] > 0.5 && slanted[2] < -0.5, "{slanted:?}");
        // The mirrored sky follows the light: none at night, dimmer in a storm.
        let night = clouds::SkyLight {
            strength: 0.0,
            ..clouds::SkyLight::DEFAULT
        };
        assert!(wet_sky(night, 0.0)[2] < 0.1 * wet_sky(clouds::SkyLight::DEFAULT, 0.0)[2]);
        assert!(
            wet_sky(clouds::SkyLight::DEFAULT, 1.0)[0] < wet_sky(clouds::SkyLight::DEFAULT, 0.0)[0]
        );
    }

    #[test]
    fn storms_haze_and_ground_fog_follow_the_weather_and_the_fog_setting() {
        let mut weather = empty_world();
        assert_eq!(weather.storm(), 0.0);
        assert!(weather.mist(1).is_none());
        assert_eq!(weather.mist(2).map(|(mist, _)| mist), Some(Mist::DEFAULT));
        weather.server = vec!["heavyrain".into(), "heavyrainfog".into(), "fog".into()];
        weather.rebuild();
        assert_eq!(weather.storm(), 1.0);
        // The densest of the map's fogs wins; 0 turns them all off.
        let (mist, _) = weather.mist(1).unwrap();
        assert_eq!(mist.density, 1.4e-3);
        assert!(weather.mist(0).is_none());
        // Grey fog stays grey; a tinted fog keeps its hue.
        let grey = fog_color([0.3; 4], [1.0; 3], 0.0);
        assert_eq!(grey, FOG_GREY);
        let teal = fog_color([0.19, 0.6, 0.7, 0.12], [1.0; 3], 0.0);
        assert!(teal[2] > teal[1] && teal[1] > teal[0]);
        assert!(fog_color([0.3; 4], [1.0; 3], 1.0)[0] < grey[0]);
    }

    #[test]
    fn worlds_without_a_server_take_their_own_weather_entities() {
        use sjk_bsp::{Bsp, CollisionShader, box_brush, write_collision_map_with_models};
        let shaders = [CollisionShader {
            name: "textures/stone".into(),
            surface_flags: 0,
            content_flags: 1,
        }];
        let entities = "{\n\"classname\" \"worldspawn\"\n}\n\
            {\n\"classname\" \"fx_rain\"\n\"spawnflags\" \"20\"\n}\n\
            {\n\"classname\" \"fx_wind\"\n\"spawnflags\" \"2\"\n\"angle\" \"180\"\n\"speed\" \"5000\"\n}\n\
            {\n\"classname\" \"misc_weather_zone\"\n\"model\" \"*1\"\n}\n";
        let zone = vec![box_brush([0.0; 3], [64.0, 32.0, 16.0], 0)];
        let data = write_collision_map_with_models(
            entities,
            &shaders,
            &[box_brush([-8.0; 3], [8.0; 3], 0)],
            &[zone],
        );
        let bsp = Bsp::parse(&data).unwrap();
        let weather = Runtime::new(None, &bsp);
        assert_eq!(weather.server[..2], ["heavyrain", "heavyrainfog"]);
        assert!(weather.server[2].starts_with("constantwind ( -5000.000000"));
        assert_eq!(weather.effects.clouds.len(), 2);
        assert!((weather.effects.winds[0].current()[0] + 5000.0).abs() < 0.01);
        assert_eq!(weather.map_zones, [[[0.0; 3], [64.0, 32.0, 16.0]]]);
    }

    #[test]
    fn console_commands_add_to_the_maps_and_reject_unknown_ones() {
        let bsp = sjk_bsp::Bsp::empty([-64.0; 3], [64.0; 3]);
        let mut weather = Runtime::new(None, &bsp);
        assert!(weather.effects.is_empty());
        assert_eq!(weather.command("snow"), Ok(()));
        assert_eq!(weather.command("hail"), Err(effects::HELP));
        assert_eq!(weather.local, ["snow"]);
        assert_eq!(weather.effects.clouds.len(), 1);
        assert_eq!(weather.command("clear"), Ok(()));
        assert!(weather.effects.clouds.is_empty());
    }
}
