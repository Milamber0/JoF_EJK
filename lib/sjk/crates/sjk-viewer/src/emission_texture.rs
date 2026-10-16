//! Load-time spatial emission in material UVs. Images are area-reduced, never strided,
//! so thin luminous texels survive. Animated UV patterns retain their spatial mean.
use super::*;
use glam::{Mat3, Vec2, Vec3, Vec4};
#[path = "emission_reduction.rs"]
mod reduction;
use reduction::Reduction;

struct Level {
    size: [usize; 2],
    pixels: Vec<Vec4>,
}
struct Layer {
    levels: Vec<Level>,
    mean: Vec4,
    transform: Mat3,
    spatial: bool,
    cover: bool,
    gain: Vec3,
    clamp: bool,
}

/// Time-averaged emissive stages, with static UV transforms and alpha covers retained.
#[derive(Default)]
pub(crate) struct Texture {
    layers: Vec<Layer>,
    reference: Vec3,
}

impl Texture {
    /// Mean emission of the retained field before relative sampling.
    pub(crate) fn mean(&self) -> Vec3 {
        self.reference
    }

    /// The field of an emission map (`material_maps`): its sRGB colour times `radiance`,
    /// laid out by the diffuse `stage` it belongs to (texture transforms and clamping).
    pub(crate) fn from_map(stage: &ShaderStage, image: Arc<RgbaImage>, radiance: f32) -> Self {
        let layer = Layer::new(stage, &[image], Vec3::splat(radiance), false);
        let reference = layer.mean.truncate() * layer.gain;
        Self {
            layers: vec![layer],
            reference,
        }
    }

    /// Follow source-stage composition while preserving its luminous pixels.
    pub(crate) fn observe(
        &mut self,
        stage: &ShaderStage,
        images: &[Arc<RgbaImage>],
        resolved: bool,
        declared: bool,
        self_lit: bool,
        infer_fixture: bool,
    ) {
        if declared {
            if self.layers.is_empty()
                && resolved
                && !images.is_empty()
                && stage.texture_generator == TextureGenerator::Base
            {
                self.layers
                    .push(Layer::new(stage, images, Vec3::ONE, false));
            }
        } else {
            match stage.blend {
                StageBlend::Replace => {
                    self.layers.clear();
                    // Full-bright paint of a material no light reaches: a fixture.
                    if super::self_lit_radiance(stage, images, resolved, self_lit).is_some() {
                        if let Some(gain) = super::stage_gain(stage) {
                            self.layers.push(Layer::new(
                                stage,
                                images,
                                Vec3::from_array(gain) * super::SELF_LIT_RADIANCE,
                                false,
                            ));
                        }
                    }
                }
                StageBlend::Alpha if !self.layers.is_empty() => {
                    let alpha = match stage.alpha_generator.as_deref().unwrap_or("identity") {
                        "identity" => 1.,
                        "const" | "constant" => stage.alpha_constant.unwrap_or(1.),
                        _ => {
                            self.layers.clear();
                            return;
                        }
                    };
                    if !resolved || images.is_empty() {
                        self.layers.clear();
                        return;
                    }
                    self.layers.push(Layer::new(
                        stage,
                        images,
                        Vec3::splat(alpha.clamp(0., 1.)),
                        true,
                    ));
                }
                StageBlend::Add
                    if resolved
                        && !images.is_empty()
                        && stage.texture_generator == TextureGenerator::Base =>
                {
                    if let Some(gain) = super::stage_gain(stage) {
                        self.layers.push(Layer::new(
                            stage,
                            images,
                            Vec3::from_array(gain) * super::stage_radiance(stage, infer_fixture),
                            false,
                        ));
                    }
                }
                _ => {}
            }
        }
        self.reference = Vec3::ZERO;
        for layer in &self.layers {
            if layer.cover {
                self.reference *= Vec3::ONE - layer.gain * layer.mean.w;
            } else {
                self.reference += layer.mean.truncate() * layer.gain;
            }
        }
    }

    /// Relative RGB emission; the material's existing mean power remains authoritative.
    pub(crate) fn sample(&self, uv: Vec2, lod: u32) -> Vec3 {
        if self.layers.is_empty() || self.reference.max_element() <= 1e-8 {
            return Vec3::ONE;
        }
        let mut value = Vec3::ZERO;
        for layer in &self.layers {
            let pixel = layer.sample(uv, lod);
            if layer.cover {
                value *= Vec3::ONE - layer.gain * pixel.w;
            } else {
                value += pixel.truncate() * layer.gain;
            }
        }
        Vec3::from_array(std::array::from_fn(|c| {
            if self.reference[c] > 1e-8 {
                value[c] / self.reference[c]
            } else {
                0.
            }
        }))
    }

    /// Largest transformed texture edge in reduced-mask texels, for adaptive quadrature.
    pub(crate) fn span(&self, uv: [Vec2; 3]) -> f32 {
        self.layers
            .iter()
            .filter(|l| l.spatial)
            .map(|l| {
                let size = Vec2::new(l.levels[0].size[0] as f32, l.levels[0].size[1] as f32);
                let p = uv.map(|p| (l.transform * p.extend(1.)).truncate() * size);
                p[0].distance(p[1])
                    .max(p[1].distance(p[2]))
                    .max(p[2].distance(p[0]))
            })
            .fold(0., f32::max)
    }
}

impl Layer {
    fn new(stage: &ShaderStage, images: &[Arc<RgbaImage>], gain: Vec3, cover: bool) -> Self {
        static LINEAR: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
        let linear = LINEAR.get_or_init(|| std::array::from_fn(|i| (i as f32 / 255.).powf(2.2)));
        let size = [
            images.iter().map(|i| i.width()).min().unwrap().clamp(1, 64) as usize,
            images
                .iter()
                .map(|i| i.height())
                .min()
                .unwrap()
                .clamp(1, 64) as usize,
        ];
        let mut pixels = vec![Vec4::ZERO; size[0] * size[1]];
        let mut mean = Vec4::ZERO;
        for image in images {
            let reduction = Reduction::new(size, [image.width() as usize, image.height() as usize]);
            for (x, y, p) in image.enumerate_pixels() {
                let color = Vec4::new(
                    linear[p[0] as usize],
                    linear[p[1] as usize],
                    linear[p[2] as usize],
                    p[3] as f32 / 255.,
                );
                reduction.add(
                    &mut pixels,
                    [x as usize, y as usize],
                    color / images.len() as f32,
                );
                mean +=
                    color / (image.width() as f32 * image.height() as f32 * images.len() as f32);
            }
        }
        let varied = pixels.iter().any(|p| {
            if cover {
                (p.w - mean.w).abs() > 0.001
            } else {
                (p.truncate() - mean.truncate()).abs().max_element() > 0.001
            }
        });
        let mut transform = Mat3::IDENTITY;
        let mut spatial = varied && stage.texture_generator == TextureGenerator::Base;
        for modification in stage.texture_modifications.iter().take(4) {
            let a = |i| modification.arguments.get(i).copied().unwrap_or(0.);
            let next = match modification.kind.as_str() {
                "scale" => Mat3::from_diagonal(Vec3::new(a(0), a(1), 1.)),
                "transform" => Mat3::from_cols(
                    Vec3::new(a(0), a(1), 0.),
                    Vec3::new(a(2), a(3), 0.),
                    Vec3::new(a(4), a(5), 1.),
                ),
                _ => {
                    spatial = false;
                    Mat3::IDENTITY
                }
            };
            transform = next * transform;
        }
        let mut levels = vec![Level { size, pixels }];
        while levels.last().unwrap().size != [1, 1] {
            let last = levels.last().unwrap();
            let size = last.size.map(|n| (n / 2).max(1));
            let mut pixels = vec![Vec4::ZERO; size[0] * size[1]];
            let reduction = Reduction::new(size, last.size);
            for y in 0..last.size[1] {
                for x in 0..last.size[0] {
                    reduction.add(&mut pixels, [x, y], last.pixels[x + y * last.size[0]]);
                }
            }
            levels.push(Level { size, pixels });
        }
        Self {
            levels,
            mean,
            transform,
            spatial,
            cover,
            gain,
            clamp: stage.clamp,
        }
    }

    fn sample(&self, uv: Vec2, lod: u32) -> Vec4 {
        if !self.spatial {
            return self.mean;
        }
        let uv = (self.transform * uv.extend(1.)).truncate();
        let level = &self.levels[(lod as usize).min(self.levels.len() - 1)];
        let p = uv * Vec2::new(level.size[0] as f32, level.size[1] as f32) - Vec2::splat(0.5);
        let base = p.floor().as_ivec2();
        let f = p - p.floor();
        let get = |x: i32, y: i32| {
            let wrap = |n: i32, size: usize| {
                if self.clamp {
                    n.clamp(0, size as i32 - 1) as usize
                } else {
                    n.rem_euclid(size as i32) as usize
                }
            };
            level.pixels[wrap(x, level.size[0]) + wrap(y, level.size[1]) * level.size[0]]
        };
        get(base.x, base.y).lerp(get(base.x + 1, base.y), f.x).lerp(
            get(base.x, base.y + 1).lerp(get(base.x + 1, base.y + 1), f.x),
            f.y,
        )
    }
}
