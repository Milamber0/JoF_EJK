//! Generic data model for parsed Raven EFX definitions.

use std::fmt;

/// A scalar whose value is chosen once between two authored endpoints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub minimum: f32,
    pub maximum: f32,
}

impl Range {
    pub const ZERO: Self = Self {
        minimum: 0.0,
        maximum: 0.0,
    };
    pub const ONE: Self = Self {
        minimum: 1.0,
        maximum: 1.0,
    };

    pub fn sample(self, unit: f32) -> f32 {
        self.minimum + (self.maximum - self.minimum) * unit.clamp(0.0, 1.0)
    }
}

/// Three independently ranged coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorRange {
    pub minimum: [f32; 3],
    pub maximum: [f32; 3],
}

impl VectorRange {
    pub const ZERO: Self = Self {
        minimum: [0.0; 3],
        maximum: [0.0; 3],
    };

    pub fn sample(self, units: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|axis| {
            self.minimum[axis]
                + (self.maximum[axis] - self.minimum[axis]) * units[axis].clamp(0.0, 1.0)
        })
    }
}

/// Primitive kinds understood by Raven's EFX scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentKind {
    Particle,
    OrientedParticle,
    Line,
    Tail,
    Cylinder,
    Electricity,
    FxRunner,
    Decal,
    Sound,
    Light,
    CameraShake,
    Flash,
    Emitter,
    Other,
}

/// One scalar lifetime curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub start: Range,
    pub end: Range,
    pub parameter: Range,
    pub flags: CurveFlags,
}

/// Generic Raven FX lifetime-curve modes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CurveFlags {
    pub linear: bool,
    pub random: bool,
    pub modifier: CurveModifier,
}

/// Mutually-exclusive parameterized portion of an FX lifetime curve.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CurveModifier {
    #[default]
    None,
    NonLinear,
    Wave,
    Clamp,
}

/// Runtime flags authored on an EFX primitive's `flags` line.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrimitiveFlags {
    pub use_alpha: bool,
    pub apply_physics: bool,
    pub physics_flag_authored: bool,
    pub expensive_physics: bool,
    pub expensive_physics_flag_authored: bool,
    pub kill_on_impact: bool,
    pub impact_runs_effect: bool,
    pub death_runs_effect: bool,
    pub set_shader_time: bool,
    pub use_model: bool,
    pub use_bounding_box: bool,
    pub ghoul2_collision: bool,
    pub ghoul2_decals: bool,
    pub emit_effect: bool,
    pub depth_hack: bool,
    pub relative: bool,
}

/// Placement and sampling flags authored on an EFX `spawnFlags` line.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpawnFlags {
    pub absolute_velocity: bool,
    pub absolute_acceleration: bool,
    pub origin_on_sphere: bool,
    pub origin_on_cylinder: bool,
    pub axis_from_sphere: bool,
    pub random_rotation_around_forward: bool,
    pub even_distribution: bool,
    pub rgb_component_interpolation: bool,
    pub origin2_from_trace: bool,
    pub trace_impact_effect: bool,
    pub origin2_is_offset: bool,
    pub cheap_origin: bool,
    pub cheap_origin2: bool,
    pub affected_by_wind: bool,
    pub less_attenuation: bool,
}

/// One parsed primitive template.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub kind: ComponentKind,
    pub count: Range,
    pub life: Range,
    pub delay: Range,
    pub origin: VectorRange,
    pub origin2: VectorRange,
    pub velocity: VectorRange,
    pub acceleration: VectorRange,
    pub gravity: Range,
    pub rotation: Range,
    pub rotation_delta: Range,
    pub angles: VectorRange,
    pub angle_delta: VectorRange,
    pub density: Range,
    pub variance: Range,
    /// Retail's one `mElasticity` (`bounce` or `intensity`, default 0.1) is kept as three
    /// copies: `elasticity` (a particle's bounce), `chaos` (an electricity bolt's
    /// jaggedness) and `intensity` (a camera shake's strength). The parser sets all three.
    pub elasticity: Range,
    pub bounce_authored: bool,
    pub intensity_authored: bool,
    pub cull_range: Option<Range>,
    pub size: Curve,
    pub size2: Curve,
    pub size2_authored: bool,
    pub length: Curve,
    pub length_authored: bool,
    pub alpha: Curve,
    pub flags: PrimitiveFlags,
    pub spawn_flags: SpawnFlags,
    pub rgb_start: [Range; 3],
    pub rgb_end: [Range; 3],
    pub rgb_parameter: Range,
    pub rgb_flags: CurveFlags,
    pub intensity: Range,
    pub chaos: Range,
    pub chaos_authored: bool,
    pub radius: Range,
    pub height: Range,
    pub shaders: Vec<String>,
    pub models: Vec<String>,
    pub effects: Vec<String>,
    pub emit_effects: Vec<String>,
    pub impact_effects: Vec<String>,
    pub death_effects: Vec<String>,
    pub sounds: Vec<String>,
}

impl Component {
    pub(crate) fn new(kind: ComponentKind) -> Self {
        let flat = Curve {
            start: Range::ONE,
            end: Range::ONE,
            parameter: Range::ZERO,
            flags: CurveFlags::default(),
        };
        Self {
            kind,
            count: Range::ONE,
            life: Range {
                minimum: 50.0,
                maximum: 50.0,
            },
            delay: Range::ZERO,
            origin: VectorRange::ZERO,
            origin2: VectorRange::ZERO,
            velocity: VectorRange::ZERO,
            acceleration: VectorRange::ZERO,
            gravity: Range::ZERO,
            rotation: Range::ZERO,
            rotation_delta: Range::ZERO,
            angles: VectorRange::ZERO,
            angle_delta: VectorRange::ZERO,
            density: Range {
                minimum: 10.0,
                maximum: 10.0,
            },
            variance: Range::ONE,
            // `mElasticity.SetRange(0.1f, 0.1f)` (`FxTemplate.cpp:44`).
            elasticity: Range {
                minimum: 0.1,
                maximum: 0.1,
            },
            bounce_authored: false,
            intensity_authored: false,
            cull_range: None,
            size: flat,
            size2: flat,
            size2_authored: false,
            length: flat,
            length_authored: false,
            alpha: flat,
            flags: PrimitiveFlags::default(),
            spawn_flags: SpawnFlags::default(),
            rgb_start: [Range::ONE; 3],
            rgb_end: [Range::ONE; 3],
            rgb_parameter: Range::ZERO,
            rgb_flags: CurveFlags::default(),
            intensity: Range {
                minimum: 0.1,
                maximum: 0.1,
            },
            chaos: Range {
                minimum: 0.1,
                maximum: 0.1,
            },
            chaos_authored: false,
            radius: Range {
                minimum: 10.0,
                maximum: 10.0,
            },
            height: Range {
                minimum: 10.0,
                maximum: 10.0,
            },
            shaders: Vec::new(),
            models: Vec::new(),
            effects: Vec::new(),
            emit_effects: Vec::new(),
            impact_effects: Vec::new(),
            death_effects: Vec::new(),
            sounds: Vec::new(),
        }
    }
}

/// One parsed EFX graph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectDefinition {
    pub repeat_delay: Option<Range>,
    pub components: Vec<Component>,
}

/// EFX loading or syntax failure.
#[derive(Debug)]
pub enum EffectError {
    Vfs(sjk_vfs::VfsError),
    Missing(String),
    Parse(String),
}

impl fmt::Display for EffectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vfs(error) => error.fmt(formatter),
            Self::Missing(path) => write!(formatter, "effect definition {path} is missing"),
            Self::Parse(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for EffectError {}

impl From<sjk_vfs::VfsError> for EffectError {
    fn from(value: sjk_vfs::VfsError) -> Self {
        Self::Vfs(value)
    }
}
