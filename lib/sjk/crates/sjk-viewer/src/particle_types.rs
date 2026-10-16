//! Fixed particle-pool records and shader-stage sampling helpers.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParticleBlend {
    Alpha,
    Add,
    AlphaAdd,
    Filter,
    TwiceModulate,
    DstColorAdd,
    OneMinusSrcAlpha,
    Unsupported,
}

pub(crate) struct Particle {
    pub(crate) motion: crate::particle_motion::Motion,

    pub(crate) spawned_at: Instant,
    pub(crate) delay: Duration,
    pub(crate) lifetime: Duration,
    pub(crate) size: crate::effect_envelope::Envelope,
    pub(crate) start_length: f32,
    pub(crate) end_length: f32,
    pub(crate) streak: Option<Vec3>,
    /// `streak` still points at an `org2fromTrace` line's untraced end
    /// ([`crate::effect_shapes::origin2_trace_target`]); the first draw traces it
    /// ([`crate::effect_geometry::resolve_traced_streak`]).
    pub(crate) trace_streak: bool,
    pub(crate) normal: Option<Vec3>,
    pub(crate) alpha: crate::effect_envelope::Envelope,
    pub(crate) use_alpha: bool,
    pub(crate) set_shader_time: bool,
    pub(crate) rgb: [crate::effect_envelope::Envelope; 3],
    pub(crate) seed: u32,
    pub(crate) shader: Arc<str>,
    pub(crate) physics: crate::particle_physics::State,
    pub(crate) shape: PrimitiveShape,
}

#[derive(Clone, Copy)]
pub(crate) enum PrimitiveShape {
    Billboard,
    /// A retained-world sprite replaced each frame rather than simulated over its lifetime.
    FrameBillboard,
    Cylinder {
        axis: Vec3,
        size2: crate::effect_envelope::Envelope,
        length: crate::effect_envelope::Envelope,
        trace_end: bool,
        depth_hack: bool,
    },
    Electricity {
        end: Vec3,
        chaos: f32,
        tapered: bool,
        branched: bool,
        grow: bool,
        trace_end: bool,
        depth_hack: bool,
    },
}

impl Particle {
    pub(crate) fn sample_envelopes(&self, elapsed_seconds: f32) -> (f32, f32, [f32; 3]) {
        let elapsed_millis = elapsed_seconds * 1_000.0;
        let lifetime_millis = self.lifetime.as_secs_f32() * 1_000.0;
        (
            self.size
                .sample(elapsed_millis, lifetime_millis, self.seed.wrapping_add(31)),
            self.alpha
                .sample(elapsed_millis, lifetime_millis, self.seed.wrapping_add(32)),
            std::array::from_fn(|axis| {
                self.rgb[axis].sample(
                    elapsed_millis,
                    lifetime_millis,
                    self.seed.wrapping_add(33 + axis as u32),
                )
            }),
        )
    }

    pub(crate) fn shader_seconds(&self, age: f32, global: f32) -> f32 {
        if self.set_shader_time { age } else { global }
    }
}

/// Effect particles (EFX, impacts, muzzle flashes, Force puffs) stop at this count;
/// past it [`crate::particle_room::Room`] makes room by ending the oldest smoke early.
/// JoF's HD `rocket/shot` keeps about 1,200 particles alive per rocket in flight, so
/// 4,096 holds three full trails before anything is cut. In the offline barrage test
/// (`particle_room_barrage.rs`, release, 2 ms frames) a full pool costs about 0.35 to
/// 0.45 ms of effect update and 55 us of depth sort per frame, against 0.2 ms and
/// 25 us at the former 2,048.
pub(crate) const MAX_PARTICLES: usize = 4_096;

/// Pool slots only per-frame billboards may use: player sprites (talk balloon,
/// connection icon), simple pickup icons and hook ropes.
///
/// Those are cleared and appended again every frame (`pickups::simple::append_frame`),
/// so effects spawned in between (map effects, other players' muzzle flashes during
/// actor submission) used to take the slots they had just freed, and a pool saturated
/// by effects dropped every balloon at once. Stock draws `CG_PlayerFloatSprite` as a
/// scene entity that effects never compete with (`codemp/cgame/cg_players.c`).
pub(crate) const FRAME_BILLBOARD_RESERVE: usize = 256;

/// The pool's allocated size; nothing appends past it, so it never reallocates.
pub(crate) const PARTICLE_POOL: usize = MAX_PARTICLES + FRAME_BILLBOARD_RESERVE;

/// Entity instances a frame can draw: room for every pool slot's billboard at its
/// shader's full eight stages (one instance per stage), so a full pool never loses a
/// layer, plus the opaque placeholder entities that share the buffer. It used to be
/// 1,024 in all, and the newest particles (a rocket's fresh trail puffs, last in the
/// pool) were the ones left out once older smoke filled it.
pub(crate) const INSTANCE_CAPACITY: usize = MAX_PARTICLE_SHADER_STAGES * PARTICLE_POOL + 1_024;

/// Whether a per-frame billboard still fits in a pool holding `len` particles.
/// Effects stop at [`MAX_PARTICLES`], so the reserve stays for billboards.
pub(crate) fn frame_billboard_fits(len: usize) -> bool {
    len < PARTICLE_POOL
}
const MAX_PARTICLE_SHADER_STAGES: usize = 8;

/// Borrowed stage selection; iteration evaluates at most eight original stages.
/// Empty or unknown shaders yield the same single fallback layer.
pub(crate) struct ParticleLayerSamples<'a> {
    atlas: &'a ParticleAtlas,
    animations: &'a [ParticleAtlasAnimation],
    seconds: f32,
}

impl<'a> ParticleLayerSamples<'a> {
    /// Bound the selection to the renderer's existing eight-stage capacity.
    pub(crate) fn new(
        atlas: &'a ParticleAtlas,
        animations: &'a [ParticleAtlasAnimation],
        seconds: f32,
    ) -> Self {
        Self {
            atlas,
            animations: &animations[..animations.len().min(MAX_PARTICLE_SHADER_STAGES)],
            seconds,
        }
    }

    /// Evaluate stages on demand, retaining order and the single-layer fallback.
    pub(crate) fn iter(&self) -> impl Iterator<Item = ParticleLayerSample> + '_ {
        (0..self.animations.len().max(1)).map(|index| {
            self.animations
                .get(index)
                .map(|animation| self.atlas.sample_animation(animation, self.seconds))
                .unwrap_or(ParticleLayerSample {
                    uv_rect: self.atlas.fallback,
                    blend: ParticleBlend::Add,
                    rgb: 1.0,
                    alpha: 1.0,
                    uv_transform: [1.0, 1.0, 0.0, 0.0],
                    glow: false,
                    tinted: true,
                })
        })
    }
}

pub(crate) fn blend_for_stage(blend: Option<&StageBlend>) -> ParticleBlend {
    match blend {
        Some(StageBlend::Add) => ParticleBlend::Add,
        Some(StageBlend::Filter) => ParticleBlend::Filter,
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_src_alpha" && destination == "gl_one" => ParticleBlend::AlphaAdd,
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_dst_color" && destination == "gl_src_color" => {
            ParticleBlend::TwiceModulate
        }
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_dst_color" && destination == "gl_one" => ParticleBlend::DstColorAdd,
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_one" && destination == "gl_one_minus_src_alpha" => {
            ParticleBlend::OneMinusSrcAlpha
        }
        Some(StageBlend::Custom { .. }) => ParticleBlend::Unsupported,
        Some(StageBlend::Replace | StageBlend::Alpha) | None => ParticleBlend::Alpha,
    }
}

pub(crate) fn fade(use_alpha: bool, life_envelope: f32) -> (f32, f32) {
    if use_alpha {
        (1.0, life_envelope)
    } else {
        (life_envelope, 1.0)
    }
}

#[cfg(test)]
mod pool_tests {
    use super::*;

    /// The frame that hid every balloon at once: a pool saturated by effects,
    /// billboards cleared, effects (another player's muzzle flash) refilling the
    /// freed slots up to their cap, then the billboards appended again.
    #[test]
    fn effects_refilling_the_pool_leave_room_for_every_billboard() {
        let billboards = 32 + 32 + 96; // balloons, hook ropes, pickup icons
        let mut len = MAX_PARTICLES;
        len -= billboards; // `pickups::simple::append_frame` clears them
        while len < MAX_PARTICLES {
            len += 1; // the effect spawners' own guard
        }
        for _ in 0..billboards {
            assert!(frame_billboard_fits(len));
            len += 1;
        }
        assert!(len <= PARTICLE_POOL);
    }

    #[test]
    fn every_pool_slot_can_draw_all_its_layers() {
        const { assert!(INSTANCE_CAPACITY >= MAX_PARTICLE_SHADER_STAGES * PARTICLE_POOL + 1_024) };
        // The instance buffer stays a few megabytes.
        let bytes = INSTANCE_CAPACITY * std::mem::size_of::<EntityInstance>();
        assert!(bytes < 4 << 20, "{bytes} bytes");
        assert_eq!(MAX_PARTICLES, 4_096);
        assert_eq!(PARTICLE_POOL, MAX_PARTICLES + FRAME_BILLBOARD_RESERVE);
    }

    #[test]
    fn billboards_stop_at_the_allocated_pool() {
        assert!(frame_billboard_fits(PARTICLE_POOL - 1));
        assert!(!frame_billboard_fits(PARTICLE_POOL));
    }
}
