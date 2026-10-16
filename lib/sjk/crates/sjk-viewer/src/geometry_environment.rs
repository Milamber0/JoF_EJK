//! Map geometry's small, shared frame block. Wind evolves once per frame,
//! never once per surface or secondary view (codemp tr_surfacesprites.cpp).
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use wgpu::util::DeviceExt;

const CVARS: [(&str, f32); 10] = [
    ("r_windSpeed", 0.),
    ("r_windAngle", 0.),
    ("r_windGust", 0.),
    ("r_windDampFactor", 0.1),
    ("r_windPointForce", 0.),
    ("r_windPointX", 0.),
    ("r_windPointY", 0.),
    ("r_surfaceWeather", 0.),
    ("r_surfaceSprites", 1.),
    ("r_flares", 1.),
];

/// Callback-cached controls: frame reads do not allocate or lock the registry.
pub(crate) struct Cvars(Arc<[AtomicU32; 10]>);
impl Cvars {
    pub(crate) fn bind(
        registry: &mut sjk_shell::CvarRegistry,
    ) -> Result<Self, sjk_shell::CvarError> {
        let values = Arc::new(std::array::from_fn(|i| {
            AtomicU32::new(CVARS[i].1.to_bits())
        }));
        for (i, (name, default)) in CVARS.into_iter().enumerate() {
            let flags = if i >= 8 {
                sjk_shell::CvarFlags::ARCHIVE
            } else {
                sjk_shell::CvarFlags::NONE
            };
            registry.register(sjk_shell::CvarDefinition::new(
                name,
                f64::from(default),
                flags,
                "Legacy surface geometry control",
            ))?;
            let changed = Arc::clone(&values);
            registry.on_change(name, move |change| {
                if let sjk_shell::CvarValue::Float(value) = change.current {
                    if value.is_finite() {
                        changed[i].store((value as f32).to_bits(), Ordering::Relaxed);
                    }
                }
            })?;
        }
        Ok(Self(values))
    }
    fn values(&self) -> [f32; 10] {
        std::array::from_fn(|i| f32::from_bits(self.0[i].load(Ordering::Relaxed)))
    }
}

/// x range-scale numerator, y weather density, z sprites enabled, w flare mode.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct Data {
    pub control: [f32; 4],
    pub grass_speed: [f32; 4],
    pub blow_force: [f32; 4],
    pub point: [f32; 4],
}
impl Default for Data {
    fn default() -> Self {
        Self {
            control: [1., 0., 1., 1.],
            grass_speed: [0.; 4],
            blow_force: [0.; 4],
            point: [0.; 4],
        }
    }
}
pub(crate) fn buffer(device: &wgpu::Device, data: &Data) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK surface frame environment"),
        contents: bytemuck::bytes_of(data),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}

#[derive(Default)]
pub(crate) struct State {
    pub data: Data,
    last_time: Option<i32>,
    standard_tangent: Option<f32>,
    gust: f32,
    next_gust: i32,
    random: u32,
}
impl State {
    pub(crate) fn update(&mut self, time: i32, projection: Mat4, cvars: Option<&Cvars>) {
        let values = cvars.map_or_else(|| CVARS.map(|(_, v)| v), Cvars::values);
        let tangent = 1.0 / projection.x_axis.x;
        let fov = 2.0 * tangent.atan().to_degrees();
        if self.standard_tangent.is_none() && fov > 50.0 && fov < 135.0 {
            self.standard_tangent = Some(tangent);
        }
        self.data.control = [
            self.standard_tangent.unwrap_or(tangent),
            values[7].clamp(0., 1.),
            values[8],
            values[9],
        ];
        if self.last_time == Some(time) {
            return;
        }
        let dt = self
            .last_time
            .map_or(0, |previous| time.saturating_sub(previous));
        if dt < 0 {
            self.gust = 0.;
            self.next_gust = 0;
            self.data.grass_speed[3] = values[0];
        }
        self.last_time = Some(time);
        let mut speed = values[0].max(0.);
        if speed > 0. && values[2] > 0. {
            if self.gust > 0. {
                speed *= 1. + self.gust;
                self.gust -= dt.max(0) as f32 / 2500.;
                if self.gust <= 0. {
                    self.next_gust =
                        time.saturating_add((values[2] * 1000. * (1. + self.random() * 3.)) as i32);
                }
            } else if time >= self.next_gust {
                self.gust = 0.75 + self.random() * 0.75;
            }
        }
        let ratio = (1. - values[3].clamp(0., 1.)).powf(dt.max(0) as f32 / 50.);
        let yaw = values[1].to_radians();
        let pitch = (-90. + speed).min(-45.).to_radians();
        let grass = Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.cos() * yaw.sin(),
            -pitch.sin() - 1.,
        );
        self.data.grass_speed[3] = speed - ratio * (speed - self.data.grass_speed[3]);
        let blow = Vec3::new(yaw.cos(), yaw.sin(), 0.) * self.data.grass_speed[3];
        for axis in 0..3 {
            self.data.grass_speed[axis] =
                grass[axis] - ratio * (grass[axis] - self.data.grass_speed[axis]);
            self.data.blow_force[axis] =
                blow[axis] - ratio * (blow[axis] - self.data.blow_force[axis]);
        }
        self.data.blow_force[3] = values[4] - ratio * (values[4] - self.data.blow_force[3]);
        self.data.point = [values[5], values[6], 0., 0.];
    }
    fn random(&mut self) -> f32 {
        // Cosmetic gust RNG is isolated from simulation and network randomness.
        self.random = self.random.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.random >> 8) as f32 / 16777216.
    }
}
