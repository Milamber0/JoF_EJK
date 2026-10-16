//! A tileable 3D noise volume for the volumetric fog and the clouds.
//!
//! The channels follow the usual cloud recipe (Schneider, "The Real-time Volumetric
//! Cloudscapes of Horizon Zero Dawn", 2015): red is Perlin-Worley noise, billowy blobs
//! for the overall shape; green, blue and alpha are inverted Worley noise at three rising
//! frequencies, to erode the shape's edges. Every channel tiles across the cube, so the
//! texture repeats seamlessly with a repeating sampler.
//!
//! The volume is made once per run on a worker thread and shared by every map; [`volume`] answers `None` until it is ready.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Texels along each edge.
pub(crate) const SIZE: u32 = 64;

static VOLUME: OnceLock<Box<[u8]>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// The RGBA8 volume, x fastest, then y, then z; `None` while it is being made (the first
/// call starts it).
pub(crate) fn volume() -> Option<&'static [u8]> {
    if let Some(volume) = VOLUME.get() {
        return Some(volume);
    }
    if !STARTED.swap(true, Ordering::AcqRel) {
        let spawned = std::thread::Builder::new()
            .name("sjk-weather-noise".into())
            .spawn(|| {
                let _ = VOLUME.set(generate(SIZE));
            });
        if spawned.is_err() {
            // Without a thread, make it here: a short stall rather than no clouds.
            let _ = VOLUME.set(generate(SIZE));
        }
    }
    VOLUME.get().map(|volume| &**volume)
}

/// The volume on the GPU, with its repeating sampler. Created empty; [`Texture::update`]
/// fills it the first frame the volume is ready.
pub(crate) struct Texture {
    texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    ready: bool,
}

impl Texture {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK weather noise"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: SIZE,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK weather noise"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            texture,
            view,
            sampler,
            ready: false,
        }
    }

    /// Upload the volume once it is made; whether the texture holds it.
    pub(crate) fn update(&mut self, queue: &crate::frame_queue::FrameQueue) -> bool {
        if !self.ready
            && let Some(volume) = volume()
        {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                volume,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
                wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: SIZE,
                },
            );
            self.ready = true;
        }
        self.ready
    }
}

/// Make a `size`³ volume.
pub(crate) fn generate(size: u32) -> Box<[u8]> {
    let mut texels = Vec::with_capacity((size * size * size * 4) as usize);
    let scale = 1.0 / size as f32;
    for z in 0..size {
        for y in 0..size {
            for x in 0..size {
                let p = [x as f32 * scale, y as f32 * scale, z as f32 * scale];
                // Perlin fbm, 0..1, remapped onto low-frequency Worley: round blobs whose
                // insides are mottled rather than flat.
                let perlin = (0..3)
                    .map(|octave| {
                        let period = 4 << octave;
                        perlin(p, period) * 0.5f32.powi(octave)
                    })
                    .sum::<f32>()
                    / 1.75
                    * 0.5
                    + 0.5;
                let cells = worley_fbm(p, 4);
                let shape = remap(perlin.clamp(0.0, 1.0), 0.0, 1.0, cells, 1.0);
                texels.push(byte(shape));
                texels.push(byte(worley_fbm(p, 4)));
                texels.push(byte(worley_fbm(p, 8)));
                texels.push(byte(worley_fbm(p, 16)));
            }
        }
    }
    texels.into_boxed_slice()
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn remap(value: f32, low: f32, high: f32, new_low: f32, new_high: f32) -> f32 {
    new_low + (value - low) / (high - low).max(1e-6) * (new_high - new_low)
}

/// Inverted Worley noise in three octaves from `frequency` cells per edge, 0..1.
fn worley_fbm(p: [f32; 3], frequency: u32) -> f32 {
    worley(p, frequency) * 0.625
        + worley(p, frequency * 2) * 0.25
        + worley(p, frequency * 4) * 0.125
}

/// 1 at a feature point, falling to 0 a cell away; tiles every `cells` cells.
fn worley(p: [f32; 3], cells: u32) -> f32 {
    let scaled = p.map(|value| value * cells as f32);
    let base = scaled.map(|value| value.floor() as i32);
    let mut nearest = f32::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell = [base[0] + dx, base[1] + dy, base[2] + dz];
                let wrapped = cell.map(|value| value.rem_euclid(cells as i32) as u32);
                let jitter = hash3(wrapped, cells);
                let point: [f32; 3] = std::array::from_fn(|axis| cell[axis] as f32 + jitter[axis]);
                let distance = (0..3)
                    .map(|axis| (point[axis] - scaled[axis]).powi(2))
                    .sum::<f32>();
                nearest = nearest.min(distance);
            }
        }
    }
    1.0 - nearest.sqrt().min(1.0)
}

/// Gradient noise in about -1..1, tiling every `period` lattice cells.
fn perlin(p: [f32; 3], period: u32) -> f32 {
    let scaled = p.map(|value| value * period as f32);
    let base = scaled.map(|value| value.floor() as i32);
    let fraction: [f32; 3] = std::array::from_fn(|axis| scaled[axis] - base[axis] as f32);
    let fade = fraction.map(|t| t * t * t * (t * (t * 6.0 - 15.0) + 10.0));
    let mut corners = [0.0f32; 8];
    for (index, corner) in corners.iter_mut().enumerate() {
        let offset = [index & 1, (index >> 1) & 1, (index >> 2) & 1].map(|bit| bit as i32);
        let lattice: [u32; 3] = std::array::from_fn(|axis| {
            (base[axis] + offset[axis]).rem_euclid(period as i32) as u32
        });
        let random = hash3(lattice, period ^ 0x5bd1);
        let gradient = random.map(|value| value * 2.0 - 1.0);
        *corner = (0..3)
            .map(|axis| gradient[axis] * (fraction[axis] - offset[axis] as f32))
            .sum();
    }
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x = [
        lerp(corners[0], corners[1], fade[0]),
        lerp(corners[2], corners[3], fade[0]),
        lerp(corners[4], corners[5], fade[0]),
        lerp(corners[6], corners[7], fade[0]),
    ];
    let y = [lerp(x[0], x[1], fade[1]), lerp(x[2], x[3], fade[1])];
    lerp(y[0], y[1], fade[2]) * 1.5
}

/// Three values in 0..1 from a lattice point and a salt.
fn hash3(cell: [u32; 3], salt: u32) -> [f32; 3] {
    let mut state = cell[0].wrapping_mul(0x8da6_b343)
        ^ cell[1].wrapping_mul(0xd816_3841)
        ^ cell[2].wrapping_mul(0xcb1a_b31f)
        ^ salt.wrapping_mul(0x2545_f491);
    std::array::from_fn(|_| {
        state ^= state >> 16;
        state = state.wrapping_mul(0x7feb_352d);
        state ^= state >> 15;
        state = state.wrapping_mul(0x846c_a68b);
        state ^= state >> 16;
        (state >> 8) as f32 / (1 << 24) as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_tiles_and_uses_its_range() {
        let size = 16;
        let volume = generate(size);
        assert_eq!(volume.len(), (size * size * size * 4) as usize);
        // Each channel spans most of its range and is not constant.
        for channel in 0..4 {
            let values: Vec<u8> = volume.iter().skip(channel).step_by(4).copied().collect();
            let (low, high) = (values.iter().min().unwrap(), values.iter().max().unwrap());
            assert!(high - low > 100, "channel {channel}: {low}..{high}");
        }
    }

    #[test]
    fn noise_wraps_without_a_seam() {
        // A point and the same point one period on give the same value.
        for p in [[0.1, 0.7, 0.3], [0.95, 0.02, 0.5]] {
            let shifted = [p[0] + 1.0, p[1], p[2] - 1.0];
            assert!((worley(p, 4) - worley(shifted, 4)).abs() < 1e-4);
            assert!((perlin(p, 8) - perlin(shifted, 8)).abs() < 1e-4);
        }
        // Neighbouring texels across the edge differ no more than inside the volume.
        let edge = (worley([0.999, 0.5, 0.5], 4) - worley([0.0, 0.5, 0.5], 4)).abs();
        assert!(edge < 0.05, "{edge}");
    }
}
