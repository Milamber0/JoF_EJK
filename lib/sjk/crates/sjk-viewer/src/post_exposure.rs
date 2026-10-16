//! Eye adaptation (`r_autoExposure`): the scene's exposure follows what the camera sees,
//! within a small range around `r_hdrExposure`.
//!
//! SJK's choice, and a deliberate departure from a fixed exposure: the view
//! brightens slowly in dark areas and darkens a little, then settles, in bright light.
//! Exposure changes what a player can see in the shadows, so the range is small by
//! default (-0.5 to +1 EV), the setting is one switch on the Renderer page, and
//! `r_autoExposure 0` restores the fixed exposure exactly.
//!
//! The work stays on the GPU; there is no readback. After the main view's resolve,
//! [`Exposure::record`] meters the scene target (`post_exposure.wgsl` `meter`, a
//! centre-weighted log-luminance histogram of 1/8-resolution cells) and updates a
//! 16-byte state buffer (`adapt`, one thread). The resolve (`post_aa.wgsl`) and the
//! legacy effect layer read the exposure from that buffer, so the next frame shows it.
//! Metering reads the scene before exposure, effects, HUD and text, which never change.
//!
//! The CPU writes one 32-byte [`Params`] per frame through the frame queue: this
//! frame's smoothing fractions, the range and what the frame is ([`Mode`]): a cut to a
//! new view snaps, a hidden or replaced world (menus, loading, the hyperspace flash) and
//! the open Renderer settings page freeze the adaptation, and no session (the menu
//! world) holds the plain `r_hdrExposure`.
//!
//! With `r_sceneHdr 0` the scene is 8-bit and its highlights are already clipped, so
//! darkening would only grey them: the range is limited to brightening (min 0 EV) and
//! `r_hdrExposure` does not apply, as before.

use bytemuck::{Pod, Zeroable};
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use wgpu::util::DeviceExt;

/// Switch: 0 holds the fixed `r_hdrExposure`.
pub(crate) const ENABLED: &str = "r_autoExposure";
/// Largest darkening in EV (negative), -2..0.
pub(crate) const MIN_EV: &str = "r_autoExposureMin";
/// Largest brightening in EV, 0..2.
pub(crate) const MAX_EV: &str = "r_autoExposureMax";
/// Time constant in seconds when the view gets brighter.
pub(crate) const TO_BRIGHT: &str = "r_autoExposureToBright";
/// Time constant in seconds when the view gets darker.
pub(crate) const TO_DARK: &str = "r_autoExposureToDark";
/// Scene luminance the adaptation aims for.
pub(crate) const KEY: &str = "r_autoExposureKey";
/// The base exposure, registered by `post_hdr.rs`.
const BASE: &str = "r_hdrExposure";

/// Log2 luminance covered by histogram bins 1..63, shared with the shader as overrides.
const LOG_MIN: f32 = -12.0;
const LOG_RANGE: f32 = 16.0;
/// Pixels per metering cell side; kept in step with `CELL` in the shader.
const CELL: u32 = 8;
/// Cells per metering workgroup side; kept in step with `@workgroup_size`.
const GROUP: u32 = 16;

/// One float cvar: name, default, range and help.
struct FloatCvar {
    name: &'static str,
    default: f32,
    range: (f32, f32),
    help: &'static str,
}

/// Float cvars in [`Settings`] slot order (slot 0 is [`ENABLED`]).
const FLOATS: [FloatCvar; 6] = [
    FloatCvar {
        name: MIN_EV,
        default: -0.5,
        range: (-2.0, 0.0),
        help: "Eye adaptation: most it darkens bright views, EV -2..0; HDR scene only",
    },
    FloatCvar {
        name: MAX_EV,
        default: 1.0,
        range: (0.0, 2.0),
        help: "Eye adaptation: most it brightens dark views, EV 0..2",
    },
    FloatCvar {
        name: TO_BRIGHT,
        default: 0.4,
        range: (0.0, 10.0),
        help: "Eye adaptation: seconds to settle when the view gets brighter, 0 instant",
    },
    FloatCvar {
        name: TO_DARK,
        default: 2.5,
        range: (0.0, 20.0),
        help: "Eye adaptation: seconds to settle when the view gets darker, 0 instant",
    },
    FloatCvar {
        name: KEY,
        default: 0.18,
        range: (0.03, 0.8),
        help: "Eye adaptation: scene brightness it aims for, 0.03..0.8; higher is brighter",
    },
    FloatCvar {
        name: BASE,
        default: 1.0,
        range: (0.25, 4.0),
        help: "",
    },
];

/// Live adaptation policy shared by the console and every installed world.
#[derive(Clone)]
pub(crate) struct Settings(Arc<[AtomicU32; 7]>);

impl Default for Settings {
    fn default() -> Self {
        let slots = std::array::from_fn(|slot| {
            AtomicU32::new(match slot {
                0 => 1,
                _ => FLOATS[slot - 1].default.to_bits(),
            })
        });
        Self(Arc::new(slots))
    }
}

/// One frame's reading of [`Settings`], already clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Values {
    pub(crate) enabled: bool,
    pub(crate) min_ev: f32,
    pub(crate) max_ev: f32,
    pub(crate) to_bright: f32,
    pub(crate) to_dark: f32,
    pub(crate) key: f32,
    pub(crate) base: f32,
}

impl Default for Values {
    fn default() -> Self {
        Settings::default().values()
    }
}

impl Settings {
    /// Register the archived cvars and follow them and `r_hdrExposure`, which must
    /// already be registered. No frame-time name lookup or formatting.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        cvars.register(CvarDefinition::new(
            ENABLED,
            1_i64,
            CvarFlags::ARCHIVE,
            "Eye adaptation: 1 exposure follows the view (default), 0 fixed r_hdrExposure",
        ))?;
        // The last, `r_hdrExposure`, belongs to `post_hdr.rs`; it is only followed here.
        for cvar in &FLOATS[..FLOATS.len() - 1] {
            cvars.register(CvarDefinition::new(
                cvar.name,
                f64::from(cvar.default),
                CvarFlags::ARCHIVE,
                cvar.help,
            ))?;
        }
        let settings = Self::default();
        for slot in 0..=FLOATS.len() {
            let name = if slot == 0 {
                ENABLED
            } else {
                FLOATS[slot - 1].name
            };
            if let Some(cvar) = cvars.get(name) {
                settings.set(slot, &cvar.value);
            }
            let changed = settings.clone();
            cvars.on_change(name, move |change| changed.set(slot, &change.current))?;
        }
        Ok(settings)
    }

    fn set(&self, slot: usize, value: &CvarValue) {
        let value = match value {
            CvarValue::Bool(value) => f64::from(u8::from(*value)),
            CvarValue::Integer(value) => *value as f64,
            CvarValue::Float(value) => *value,
            CvarValue::Text(text) => match text.trim().parse() {
                Ok(value) => value,
                Err(_) => return,
            },
        };
        let bits = if slot == 0 {
            u32::from(value != 0.0 && !value.is_nan())
        } else {
            clamp(&FLOATS[slot - 1], value).to_bits()
        };
        self.0[slot].store(bits, Ordering::Relaxed);
    }

    /// Relaxed reads of every value; no allocation or locking.
    pub(crate) fn values(&self) -> Values {
        let float = |slot: usize| f32::from_bits(self.0[slot].load(Ordering::Relaxed));
        Values {
            enabled: self.0[0].load(Ordering::Relaxed) != 0,
            min_ev: float(1),
            max_ev: float(2),
            to_bright: float(3),
            to_dark: float(4),
            key: float(5),
            base: float(6),
        }
    }
}

/// A value inside the cvar's range; a non-finite one is its default.
fn clamp(cvar: &FloatCvar, value: f64) -> f32 {
    if value.is_finite() {
        (value as f32).clamp(cvar.range.0, cvar.range.1)
    } else {
        cvar.default
    }
}

/// What the adaptation does this frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    /// Off, or nothing to adapt to (menu world): the plain base exposure, and the next
    /// metered frame snaps.
    Inactive = 0,
    /// Keep the current adaptation; the base still applies live.
    Frozen = 1,
    /// Meter the scene and move towards its target.
    Metering = 2,
    /// Meter and jump straight to the target (a cut to another view).
    Snap = 3,
}

/// The frame's situation, as [`Mode::decide`] needs it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Frame {
    /// `r_autoExposure` is on.
    pub(crate) enabled: bool,
    /// A match, demo or explored map is on view, not the menu world.
    pub(crate) active: bool,
    /// The world is hidden or replaced (classic menus, loading, hyperspace flash), or the
    /// Renderer settings page is open for comparisons.
    pub(crate) frozen: bool,
    /// The view cut since the last metered frame and has not been snapped to yet.
    pub(crate) cut: bool,
}

impl Mode {
    /// Off and inactive first, then frozen: a cut seen while frozen waits for the
    /// first metered frame.
    pub(crate) fn decide(frame: Frame) -> Self {
        if !frame.enabled || !frame.active {
            Self::Inactive
        } else if frame.frozen {
            Self::Frozen
        } else if frame.cut {
            Self::Snap
        } else {
            Self::Metering
        }
    }

    const fn meters(self) -> bool {
        matches!(self, Self::Metering | Self::Snap)
    }
}

/// What identifies the view: a change is a cut (respawn, teleport, another followed
/// player, free spectating, intermission, another session). Dying is not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ViewIdentity {
    demo: bool,
    client: u16,
    /// `EF_TELEPORT_BIT`, toggled by the server at every respawn and teleport.
    teleport: bool,
    /// 1 free spectator (`PM_SPECTATOR`), 2 intermission (`PM_INTERMISSION`,
    /// `PM_SPINTERMISSION`), 0 any other movement type.
    stage: u8,
}

impl ViewIdentity {
    /// The identity of the view `player` describes.
    pub(crate) fn of(demo: bool, player: &sjk_protocol::PlayerState) -> Self {
        Self {
            demo,
            client: player.client_num(),
            teleport: player.entity_flags() & sjk_client::EF_TELEPORT_BIT != 0,
            stage: match player.movement_type() {
                4 => 1,
                7 | 8 => 2,
                _ => 0,
            },
        }
    }
}

/// Cuts between views, held until a metered frame can snap to the new one.
#[derive(Debug, Default)]
pub(crate) struct Cuts {
    view: Option<ViewIdentity>,
    pending: bool,
}

impl Cuts {
    /// The mode of a frame showing `view`; any change of view is a cut. Metered frames
    /// take the cut, inactive ones leave it to the GPU's own snap, frozen ones keep it.
    pub(crate) fn mode(&mut self, mut frame: Frame, view: Option<ViewIdentity>) -> Mode {
        if view != self.view {
            self.view = view;
            self.pending = true;
        }
        frame.cut = self.pending;
        let mode = Mode::decide(frame);
        if mode != Mode::Frozen {
            self.pending = false;
        }
        mode
    }
}

/// Per-frame parameters; kept in step with `Params` in `post_exposure.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct Params {
    base: f32,
    min_ev: f32,
    max_ev: f32,
    log_key: f32,
    to_bright: f32,
    to_dark: f32,
    mode: u32,
    _padding: u32,
}

impl Params {
    /// Parameters for one frame `delta` seconds after the last. `hdr` is `r_sceneHdr`:
    /// an 8-bit scene only brightens and keeps exposure 1.
    pub(crate) fn new(values: Values, mode: Mode, delta: f32, hdr: bool) -> Self {
        Self {
            base: if hdr { values.base } else { 1.0 },
            min_ev: if hdr { values.min_ev.min(0.0) } else { 0.0 },
            max_ev: values.max_ev.max(0.0),
            log_key: values.key.max(1e-3).log2(),
            to_bright: smoothing(delta, values.to_bright),
            to_dark: smoothing(delta, values.to_dark),
            mode: mode as u32,
            _padding: 0,
        }
    }
}

/// The fraction of the remaining distance an exponential approach with time constant
/// `tau` covers in `delta` seconds. A hitch counts as at most a quarter second.
pub(crate) fn smoothing(delta: f32, tau: f32) -> f32 {
    let delta = if delta.is_finite() {
        delta.clamp(0.0, 0.25)
    } else {
        0.0
    };
    if tau.is_nan() || tau <= 0.0 {
        return 1.0;
    }
    1.0 - (-delta / tau).exp()
}

/// GPU state; kept in step with `State` in `post_exposure.wgsl` and `SceneExposure` in
/// `post_hdr.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct State {
    exposure: f32,
    adapt: f32,
    metered: f32,
    pending: u32,
}

impl State {
    /// The base exposure, with the first metered frame snapping.
    fn fresh(base: f32) -> Self {
        Self {
            exposure: base,
            adapt: 0.0,
            metered: 0.0,
            pending: 1,
        }
    }
}

/// A fixed exposure of 1 for passes that never expose (the display gamma pass).
pub(crate) fn neutral(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK neutral exposure"),
        contents: bytemuck::bytes_of(&State::fresh(1.0)),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

/// The adaptation of one scene resolve: buffers, both passes and the CPU bookkeeping.
pub(crate) struct Exposure {
    state: wgpu::Buffer,
    params: wgpu::Buffer,
    bind: wgpu::BindGroup,
    meter: wgpu::ComputePipeline,
    adapt: wgpu::ComputePipeline,
    groups: [u32; 2],
    hdr: bool,
    mode: Mode,
    /// The next prepared frame rewrites the state (a new target starts at the base).
    fresh: bool,
    cuts: Cuts,
}

impl Exposure {
    /// Create everything for a scene target of `size`, read through `scene` (its
    /// sampled view: display values for an 8-bit scene, linear for HDR). Startup and
    /// resize only; `base` is the exposure shown until the first prepared frame.
    pub(crate) fn new(
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        size: [u32; 2],
        hdr: bool,
        base: f32,
    ) -> Self {
        let base = if hdr { base } else { 1.0 };
        let state = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK eye adaptation state"),
            contents: bytemuck::bytes_of(&State::fresh(base)),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK eye adaptation parameters"),
            contents: bytemuck::bytes_of(&Params::new(
                Values {
                    base,
                    ..Values::default()
                },
                Mode::Inactive,
                0.0,
                hdr,
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let histogram = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK eye adaptation histogram"),
            contents: &[0; 64 * 4],
            usage: wgpu::BufferUsages::STORAGE,
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let storage = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK eye adaptation"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(2, storage),
                entry(3, storage),
                entry(
                    4,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK eye adaptation"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: histogram.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: state.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK eye adaptation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post_exposure.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK eye adaptation"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let constants = [
            ("LOG_MIN", f64::from(LOG_MIN)),
            ("LOG_RANGE", f64::from(LOG_RANGE)),
            ("DECODE", f64::from(u8::from(!hdr))),
        ];
        let pipeline = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                cache: None,
            })
        };
        Self {
            state,
            params,
            bind,
            meter: pipeline("meter"),
            adapt: pipeline("adapt"),
            groups: dispatch(size),
            hdr,
            mode: Mode::Inactive,
            fresh: true,
            cuts: Cuts::default(),
        }
    }

    /// The state buffer the resolve and the effect layer read.
    pub(crate) const fn state(&self) -> &wgpu::Buffer {
        &self.state
    }

    /// Decide this frame's mode and upload its parameters: 32 bytes, plus the state
    /// once for a new target. `view` is the identity of the view on show, if any.
    pub(crate) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        values: Values,
        frame: Frame,
        view: Option<ViewIdentity>,
        delta: f32,
    ) {
        if self.fresh {
            self.fresh = false;
            let base = if self.hdr { values.base } else { 1.0 };
            queue.write_buffer(&self.state, 0, bytemuck::bytes_of(&State::fresh(base)));
        }
        self.mode = self.cuts.mode(frame, view);
        let params = Params::new(values, self.mode, delta, self.hdr);
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
    }

    /// Meter the finished scene (when the mode asks) and update the state for the next
    /// frame. Record after the last pass that reads the state this frame.
    pub(crate) fn record(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SJK eye adaptation"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, &self.bind, &[]);
        if self.mode.meters() {
            pass.set_pipeline(&self.meter);
            pass.dispatch_workgroups(self.groups[0], self.groups[1], 1);
        }
        pass.set_pipeline(&self.adapt);
        pass.dispatch_workgroups(1, 1, 1);
    }
}

/// Metering workgroups covering a target of `size` pixels.
fn dispatch(size: [u32; 2]) -> [u32; 2] {
    size.map(|pixels| pixels.max(1).div_ceil(CELL).div_ceil(GROUP))
}

impl crate::GpuState {
    /// Prepare this frame's eye adaptation; call once per frame before recording.
    /// `backdrop` is the menu backdrop holding the view.
    pub(crate) fn prepare_eye_adaptation(&mut self, delta: f32, backdrop: bool) {
        let values = self.context.exposure.values();
        let snapshot = self
            .live_session
            .as_ref()
            .map(|session| (false, session.latest_snapshot()))
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(|session| (true, session.latest_snapshot()))
            });
        let frame = Frame {
            enabled: values.enabled,
            // A hosting engine's view is a live game view (`crate::host`).
            active: !backdrop
                && (snapshot.is_some() || self.resident.exploring() || crate::host::active()),
            frozen: self.world_hidden
                || self.local_prediction.hyperspace_shade().is_some()
                || self
                    .client_menu
                    .as_ref()
                    .is_some_and(crate::menu::ClientMenu::renderer_settings_open),
            cut: false,
        };
        let view = snapshot.map(|(demo, snapshot)| ViewIdentity::of(demo, &snapshot.player));
        if let Some(exposure) = self.post_aa.as_mut().and_then(|aa| aa.exposure.as_mut()) {
            exposure.prepare(&self.queue, values, frame, view, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga;

    /// Validate `source`, then specialise each entry point with each set of constants and
    /// translate it for Vulkan (SPIR-V) and DX12 (HLSL), as wgpu does at pipeline creation.
    fn translate(
        name: &str,
        source: &str,
        entry_points: &[(naga::ShaderStage, &str)],
        variants: &[&[(&str, f64)]],
    ) {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{name}: {}", error.emit_to_string(source)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        for &(stage, entry) in entry_points {
            for constants in variants {
                let mut values = naga::back::PipelineConstants::default();
                for &(key, value) in *constants {
                    values.insert(key.to_owned(), value);
                }
                let (module, info) = naga::back::pipeline_constants::process_overrides(
                    &module,
                    &info,
                    Some((stage, entry)),
                    &values,
                )
                .unwrap_or_else(|error| panic!("{name} {entry} {constants:?}: {error:?}"));
                naga::back::spv::write_vec(
                    &module,
                    &info,
                    &naga::back::spv::Options::default(),
                    Some(&naga::back::spv::PipelineOptions {
                        shader_stage: stage,
                        entry_point: entry.to_owned(),
                    }),
                )
                .unwrap_or_else(|error| panic!("{name} {entry} SPIR-V: {error:?}"));
                let options = naga::back::hlsl::Options::default();
                let pipeline = naga::back::hlsl::PipelineOptions {
                    entry_point: Some((stage, entry.to_owned())),
                };
                naga::back::hlsl::Writer::new(String::new(), &options, &pipeline)
                    .write(&module, &info, None)
                    .unwrap_or_else(|error| panic!("{name} {entry} HLSL: {error:?}"));
            }
        }
    }

    #[test]
    fn the_exposure_programs_translate() {
        use naga::ShaderStage::{Compute, Fragment};
        translate(
            "post_exposure",
            include_str!("post_exposure.wgsl"),
            &[(Compute, "meter"), (Compute, "adapt")],
            &[&[("DECODE", 0.0)], &[("DECODE", 1.0)]],
        );
        let resolve = |hdr: f64, exposure: f64| {
            [
                ("SRGB_OUTPUT", 1.0),
                ("HDR_INPUT", hdr),
                ("SCENE_EXPOSURE", exposure),
                ("EFFECTS", 1.0),
            ]
        };
        translate(
            "post_aa",
            concat!(include_str!("post_aa.wgsl"), include_str!("post_hdr.wgsl")),
            &[(Fragment, "fs_main")],
            &[&resolve(1.0, 1.0), &resolve(0.0, 1.0), &resolve(0.0, 0.0)],
        );
        translate(
            "effect_layer",
            concat!(
                include_str!("effect_layer.wgsl"),
                include_str!("post_hdr.wgsl")
            ),
            &[(Fragment, "fs_encode"), (Fragment, "fs_write_back")],
            &[
                &[("ENCODING", 0.0)],
                &[("ENCODING", 1.0)],
                &[("ENCODING", 2.0)],
            ],
        );
    }

    #[test]
    fn the_gpu_structs_keep_their_sizes() {
        assert_eq!(std::mem::size_of::<State>(), 16);
        assert_eq!(std::mem::size_of::<Params>(), 32);
    }

    #[test]
    fn smoothing_approaches_exponentially_and_caps_hitches() {
        assert_eq!(smoothing(0.0, 2.5), 0.0);
        // One time constant covers 1 - 1/e of the distance, however it is split.
        let whole = smoothing(0.25, 0.25);
        assert!((whole - (1.0 - (-1.0_f32).exp())).abs() < 1e-6);
        let half = smoothing(0.125, 0.25);
        assert!((1.0 - (1.0 - half) * (1.0 - half) - whole).abs() < 1e-6);
        // A hitch counts as a quarter second at most; bad input moves nothing.
        assert_eq!(smoothing(5.0, 2.5), smoothing(0.25, 2.5));
        assert_eq!(smoothing(f32::NAN, 2.5), 0.0);
        assert_eq!(smoothing(-1.0, 2.5), 0.0);
        // Zero or invalid time constants are instant.
        assert_eq!(smoothing(0.016, 0.0), 1.0);
        assert_eq!(smoothing(0.016, f32::NAN), 1.0);
        // Getting used to the dark takes longer than to bright light.
        let values = Values::default();
        assert!(smoothing(0.016, values.to_bright) > smoothing(0.016, values.to_dark));
    }

    #[test]
    fn hdr_uses_the_range_and_base_ldr_only_brightens() {
        let values = Values {
            base: 1.5,
            ..Values::default()
        };
        let hdr = Params::new(values, Mode::Metering, 0.016, true);
        assert_eq!((hdr.base, hdr.min_ev, hdr.max_ev), (1.5, -0.5, 1.0));
        assert!((hdr.log_key - 0.18_f32.log2()).abs() < 1e-6);
        assert_eq!(hdr.mode, 2);
        let ldr = Params::new(values, Mode::Snap, 0.016, false);
        assert_eq!((ldr.base, ldr.min_ev, ldr.max_ev), (1.0, 0.0, 1.0));
        assert_eq!(ldr.mode, 3);
    }

    #[test]
    fn mode_follows_switch_session_freeze_and_cuts() {
        let on = Frame {
            enabled: true,
            active: true,
            frozen: false,
            cut: false,
        };
        assert_eq!(Mode::decide(on), Mode::Metering);
        assert_eq!(Mode::decide(Frame { cut: true, ..on }), Mode::Snap);
        assert_eq!(
            Mode::decide(Frame {
                frozen: true,
                cut: true,
                ..on
            }),
            Mode::Frozen
        );
        assert_eq!(
            Mode::decide(Frame {
                enabled: false,
                ..on
            }),
            Mode::Inactive
        );
        assert_eq!(
            Mode::decide(Frame {
                active: false,
                frozen: true,
                ..on
            }),
            Mode::Inactive
        );
    }

    #[test]
    fn a_cut_snaps_on_the_first_metered_frame() {
        let player = |client, teleport, movement| {
            let mut player = sjk_protocol::PlayerState::zero();
            player.set_client_num(client);
            player.set_movement_type(movement);
            let flags = if teleport {
                sjk_client::EF_TELEPORT_BIT
            } else {
                0
            };
            assert!(player.set_raw_field(17, flags), "eFlags is field 17");
            ViewIdentity::of(false, &player)
        };
        let on = Frame {
            enabled: true,
            active: true,
            ..Frame::default()
        };
        let mut cuts = Cuts::default();
        let me = player(0, false, 0);
        assert_eq!(cuts.mode(on, Some(me)), Mode::Snap, "joining is a cut");
        assert_eq!(cuts.mode(on, Some(me)), Mode::Metering);
        // Dying keeps the view; respawning toggles the teleport bit.
        assert_eq!(cuts.mode(on, Some(player(0, false, 5))), Mode::Metering);
        assert_eq!(cuts.mode(on, Some(player(0, true, 0))), Mode::Snap);
        // Following someone else while the world is frozen snaps once it is back.
        let frozen = Frame { frozen: true, ..on };
        assert_eq!(cuts.mode(frozen, Some(player(3, true, 0))), Mode::Frozen);
        assert_eq!(cuts.mode(frozen, Some(player(3, true, 0))), Mode::Frozen);
        assert_eq!(cuts.mode(on, Some(player(3, true, 0))), Mode::Snap);
        assert_eq!(cuts.mode(on, Some(player(3, true, 0))), Mode::Metering);
        // Free spectating and intermission are views of their own.
        assert_eq!(cuts.mode(on, Some(player(3, true, 4))), Mode::Snap);
        assert_eq!(cuts.mode(on, Some(player(3, true, 7))), Mode::Snap);
        // Switched off: the GPU holds the base and snaps by itself later.
        let off = Frame {
            enabled: false,
            ..on
        };
        assert_eq!(cuts.mode(off, Some(player(4, true, 0))), Mode::Inactive);
        assert_eq!(cuts.mode(on, Some(player(4, true, 0))), Mode::Metering);
    }

    #[test]
    fn defaults_and_ranges_match_the_cvars() {
        let mut cvars = CvarRegistry::new();
        crate::frame_target::aa::register(&mut cvars).unwrap();
        let settings = Settings::bind(&mut cvars).unwrap();
        assert_eq!(
            settings.values(),
            Values {
                enabled: true,
                min_ev: -0.5,
                max_ev: 1.0,
                to_bright: 0.4,
                to_dark: 2.5,
                key: 0.18,
                base: 1.0,
            }
        );
        cvars.set_text(ENABLED, "0").unwrap();
        cvars.set_text(MIN_EV, "-5").unwrap();
        cvars.set_text(MAX_EV, "0.5").unwrap();
        cvars.set_text(TO_DARK, "100").unwrap();
        cvars.set_text(KEY, "0").unwrap();
        cvars.set_text(BASE, "1.25").unwrap();
        let values = settings.values();
        assert!(!values.enabled);
        assert_eq!(
            (values.min_ev, values.max_ev, values.to_dark, values.key),
            (-2.0, 0.5, 20.0, 0.03)
        );
        assert_eq!(values.base, 1.25);
        cvars.set_text(MIN_EV, "0.75").unwrap();
        cvars.set_text(BASE, "9").unwrap();
        assert_eq!(settings.values().min_ev, 0.0);
        assert_eq!(settings.values().base, 4.0);
        cvars.set_text(ENABLED, "1").unwrap();
        assert!(settings.values().enabled);
    }

    #[test]
    fn archived_values_seed_the_settings() {
        let mut cvars = CvarRegistry::new();
        crate::frame_target::aa::register(&mut cvars).unwrap();
        cvars.set_text(BASE, "2").unwrap();
        let settings = Settings::bind(&mut cvars).unwrap();
        assert_eq!(settings.values().base, 2.0);
    }

    #[test]
    fn dispatch_covers_every_cell() {
        assert_eq!(dispatch([3840, 2160]), [30, 17]);
        assert_eq!(dispatch([128, 128]), [1, 1]);
        assert_eq!(dispatch([129, 1]), [2, 1]);
        assert_eq!(dispatch([0, 0]), [1, 1]);
    }
}
