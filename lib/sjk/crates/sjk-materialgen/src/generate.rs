//! Turning one diffuse texture into its material maps.
//!
//! All steps are deterministic and wrap around the texture edges
//! ([`crate::filters`]), so a tiling texture gives tiling maps.
//!
//! 1. **Height from albedo.** Luminance (Rec. 709 weights on the stored sRGB
//!    values) is high-passed twice at [`GRADIENT_RADIUS`] (32 texels at 256
//!    texels, scaled with the texture): lighting gradients baked into the
//!    paint over an eighth of the texture or more are suppressed. The rest
//!    is split into band-pass layers between wrap-around Gaussian blurs of
//!    radius 0, 1, 2, 4, 8 and 16 texels at 256, summed with [`BANDS`]'
//!    weights, so pixel noise counts less than shapes. The sum is centred on
//!    its 2nd–98th percentile range and scaled to 0–1. Low-contrast textures
//!    are not stretched beyond [`MIN_HEIGHT_RANGE`], so flat paint stays flat.
//!    The class's `fine_detail` weighs the two finest bands (smooth metal keeps
//!    little of the texel grain). Bright is high unless the class's `relief`
//!    says otherwise or, left to `auto`, the paint's own top light clearly shows
//!    the bright parts recessed ([`painted_relief`]): then the height is turned
//!    upside down.
//! 2. **Normal.** Scharr derivatives of the height, times the class's normal
//!    strength and the `--strength` factor, give the normal
//!    `normalize(-k·dh/ds, -k·dh/dt, 1)`. Up to [`SLOPE_REFERENCE`] (512)
//!    texels slopes are per texel. Larger textures, usually high-resolution
//!    replacements covering the same wall as the retail texture, measure them
//!    per 1/512 of the texture over a lightly blurred height, so their texel
//!    noise is not steepened. Red follows +s (right), green +t
//!    (down the image): the tangent frame of rend2 and of SJK's material
//!    program. Encoded as `round((n * 0.5 + 0.5) * 255)`, so flat is
//!    (128, 128, 255).
//! 3. **Packed roughness, metalness, occlusion** (`_rmo`, red, green, blue):
//!    the class's base roughness, moved by local luminance variation (busier is
//!    rougher), brightness (brighter is smoother) and cavities (rougher); the
//!    class's metalness on bright unsaturated texels only (paint and grime are
//!    not metal); occlusion darkens cavities of the height by the class's
//!    occlusion strength.
//!
//! Alpha-tested textures use their alpha as a mask: transparent texels add
//! nothing to the blurs, get a flat normal, and the normal map keeps the
//! source alpha. They never get parallax height.

use crate::classes::{MaterialClass, Relief};
use crate::filters::{Plane, blur, percentile, scharr, weighted_blur};
use image::{Rgb, RgbImage, Rgba, RgbaImage};

/// Radius in texels at 256 texels of the high-pass that removes lighting
/// gradients, and how often it is applied (each pass squares the suppression).
pub const GRADIENT_RADIUS: f32 = 32.0;
pub const GRADIENT_PASSES: usize = 2;

/// Band-pass layers of the high-passed luminance: (outer blur radius in texels
/// at 256 texels, weight). Each layer is the blur of the previous radius minus
/// this one; what is coarser than the last radius gets [`COARSE_WEIGHT`].
pub const BANDS: [(f32, f32); 5] = [(1.0, 0.35), (2.0, 0.6), (4.0, 1.0), (8.0, 1.0), (16.0, 0.8)];
pub const COARSE_WEIGHT: f32 = 0.5;

/// Texture size up to which normals use per-texel slopes. Larger textures
/// (high-resolution replacements) measure slopes over `size / SLOPE_REFERENCE`
/// texels of a lightly blurred height, as if they had this size.
pub const SLOPE_REFERENCE: f32 = 512.0;

/// Smallest band-passed luminance range that is scaled to the full height range.
pub const MIN_HEIGHT_RANGE: f32 = 0.12;

/// [`painted_relief`] below which an `auto` texture's height is turned upside down.
/// Most retail and HD textures score near 0 (their painted light is too faint to tell);
/// a clear inset panel or stud row scores -0.1 to -0.5.
pub const INVERT_BELOW: f32 = -0.1;

/// Luminance of void texels (holes, gaps, black screens), and the share of them above
/// which a texture is never turned: voids are low whatever their rims suggest, and a
/// perforated grate's black holes fooled the painted-light test.
pub const VOID_LUMINANCE: f32 = 0.06;
pub const VOID_SHARE: f32 = 0.4;

/// Settings that apply to every texture of a run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Multiplies every class's normal strength.
    pub strength: f32,
    /// Halve textures larger than this (both sides even) before generating.
    pub max_size: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strength: 1.0,
            max_size: None,
        }
    }
}

/// Which normal-map name and alpha meaning a texture gets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalKind {
    /// `<texture>_nh`: alpha is height for parallax.
    Height,
    /// `<texture>_n`: alpha is the source alpha (opaque for opaque textures).
    Plain,
}

impl NormalKind {
    /// The rend2 name suffix.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Height => "_nh",
            Self::Plain => "_n",
        }
    }
}

/// Suffix of the packed roughness, metalness and occlusion map.
pub const PACKED_SUFFIX: &str = "_rmo";

/// The generated maps of one texture.
#[derive(Clone, Debug)]
pub struct Maps {
    pub normal_kind: NormalKind,
    /// Normal in RGB; alpha as [`NormalKind`] says.
    pub normal: RgbaImage,
    /// Whether the normal map's alpha carries information (height or a mask).
    pub normal_alpha: bool,
    /// Roughness, metalness, occlusion.
    pub packed: RgbImage,
    /// No texel's normal leans by more than one step: the image has no relief
    /// to show (flat colours), and the maps would only cost memory.
    pub flat: bool,
    /// The height was turned upside down (dark is high), and why.
    pub inverted: Option<&'static str>,
    /// [`painted_relief`] of the texture (0 for alpha-tested ones).
    pub painted: f32,
}

/// Generate the maps of `source`.
pub fn generate(
    source: &RgbaImage,
    class: &MaterialClass,
    alpha_tested: bool,
    settings: &Settings,
) -> Maps {
    let source = cap_size(source, settings.max_size);
    let (width, height) = (source.width() as usize, source.height() as usize);
    let channel = |index: usize| {
        Plane::from_fn(width, height, |x, y| {
            f32::from(source.get_pixel(x as u32, y as u32).0[index]) / 255.0
        })
    };
    let (red, green, blue) = (channel(0), channel(1), channel(2));
    let mask = alpha_tested.then(|| channel(3));
    let luminance = Plane::from_fn(width, height, |x, y| {
        0.2126 * red.at(x, y) + 0.7152 * green.at(x, y) + 0.0722 * blue.at(x, y)
    });
    let scale = width.max(height) as f32 / 256.0;
    let height_map = height_from_luminance(&luminance, mask.as_ref(), scale, class.fine_detail);
    // Alpha-tested textures keep bright high: their holes are cut out, not painted.
    let painted = if mask.is_none() {
        painted_relief(&luminance, scale)
    } else {
        0.0
    };
    let inverted = match class.relief {
        Relief::Inverted => Some("overrides"),
        Relief::Keep => None,
        Relief::Auto => {
            (mask.is_none() && painted < INVERT_BELOW && void_share(&luminance) <= VOID_SHARE)
                .then_some("painted shading")
        }
    };
    let height_map = if inverted.is_some() {
        height_map.map(|value| 1.0 - value)
    } else {
        height_map
    };

    // Above SLOPE_REFERENCE texels slopes are measured per 1/512 of the texture,
    // over a lightly blurred height: a high-resolution replacement of a retail
    // texture covers the same wall, and its texel noise must not be steepened.
    let slope_scale = (width.max(height) as f32 / SLOPE_REFERENCE).max(1.0);
    let strength = class.normal_strength * settings.strength * slope_scale;
    let step = (slope_scale.round() as usize).max(1);
    let relief = if step > 1 {
        blur(&height_map, step / 2)
    } else {
        height_map.clone()
    };
    let normal_kind = if class.parallax && !alpha_tested {
        NormalKind::Height
    } else {
        NormalKind::Plain
    };
    let mut normal = normal_from_height(&relief, strength, step);
    let opaque = |x: usize, y: usize| mask.as_ref().is_none_or(|mask| mask.at(x, y) >= 0.5);
    for (x, y, pixel) in normal.enumerate_pixels_mut() {
        let (x, y) = (x as usize, y as usize);
        if !opaque(x, y) {
            pixel.0 = [128, 128, 255, 0];
        }
        pixel.0[3] = match normal_kind {
            NormalKind::Height => to_byte(relief.at(x, y)),
            NormalKind::Plain => source.get_pixel(x as u32, y as u32).0[3],
        };
    }
    let normal_alpha = match normal_kind {
        NormalKind::Height => true,
        NormalKind::Plain => alpha_tested && source.pixels().any(|pixel| pixel.0[3] != 255),
    };
    if !normal_alpha {
        for pixel in normal.pixels_mut() {
            pixel.0[3] = 255;
        }
    }

    let flat = normal
        .pixels()
        .all(|pixel| pixel.0[0].abs_diff(128) <= 1 && pixel.0[1].abs_diff(128) <= 1);
    let packed = packed_map(
        class,
        [&red, &green, &blue],
        &luminance,
        &height_map,
        mask.as_ref(),
        scale,
    );
    Maps {
        normal_kind,
        normal,
        normal_alpha,
        packed,
        flat,
        inverted,
        painted,
    }
}

/// Halve `source` with a 2×2 box while it is larger than `max_size` and both
/// sides are even (which keeps a tiling texture tiling).
pub fn cap_size(source: &RgbaImage, max_size: Option<u32>) -> RgbaImage {
    let mut image = source.clone();
    let Some(max_size) = max_size else {
        return image;
    };
    while image.width().max(image.height()) > max_size.max(1)
        && image.width().is_multiple_of(2)
        && image.height().is_multiple_of(2)
    {
        let (width, height) = (image.width() / 2, image.height() / 2);
        image = RgbaImage::from_fn(width, height, |x, y| {
            let mut sum = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let pixel = image.get_pixel(2 * x + dx, 2 * y + dy).0;
                for (total, value) in sum.iter_mut().zip(pixel) {
                    *total += u32::from(value);
                }
            }
            Rgba(sum.map(|total| ((total + 2) / 4) as u8))
        });
    }
    image
}

/// Step 1 of the module documentation: normalised 0–1 height, bright high. `fine`
/// weighs the two finest bands.
pub fn height_from_luminance(
    luminance: &Plane,
    mask: Option<&Plane>,
    scale: f32,
    fine: f32,
) -> Plane {
    let smooth = |plane: &Plane, radius: usize| match mask {
        Some(mask) => weighted_blur(plane, mask, radius),
        None => blur(plane, radius),
    };
    let texels = |radius: f32| ((radius * scale).round() as usize).max(1);
    let mut detail = match mask {
        // Transparent texels take their neighbours' value instead of their own.
        Some(mask) => {
            let filled = smooth(luminance, 1);
            Plane::from_fn(luminance.width, luminance.height, |x, y| {
                if mask.at(x, y) >= 0.5 {
                    luminance.at(x, y)
                } else {
                    filled.at(x, y)
                }
            })
        }
        None => luminance.clone(),
    };
    for _ in 0..GRADIENT_PASSES {
        let low = smooth(&detail, texels(GRADIENT_RADIUS));
        detail = detail.zip_map(&low, |value, low| value - low);
    }
    let mut raw = Plane::filled(luminance.width, luminance.height, 0.0);
    let mut previous_radius = 0;
    let mut previous = detail;
    for (band, (radius, weight)) in BANDS.into_iter().enumerate() {
        let weight = if band < 2 { weight * fine } else { weight };
        let radius = texels(radius);
        if radius <= previous_radius {
            continue;
        }
        let next = smooth(&previous, radius);
        for ((sum, a), b) in raw.data.iter_mut().zip(&previous.data).zip(&next.data) {
            *sum += weight * (a - b);
        }
        previous = next;
        previous_radius = radius;
    }
    for (sum, coarse) in raw.data.iter_mut().zip(&previous.data) {
        *sum += COARSE_WEIGHT * coarse;
    }
    let low = percentile(&raw, mask, 0.02);
    let high = percentile(&raw, mask, 0.98);
    let middle = 0.5 * (low + high);
    let range = (high - low).max(MIN_HEIGHT_RANGE);
    raw.map(|value| (0.5 + (value - middle) / range).clamp(0.0, 1.0))
}

/// How the paint's baked top light agrees with bright-is-high, in standard deviations of
/// the fine luminance: positive when it does, negative when the bright parts look
/// recessed, near 0 when the paint shows no clear light.
///
/// Retail textures are painted lit from above: the top rim of anything raised is light
/// and its bottom rim dark, inside a pit the other way round. The coarse luminance split
/// at its median gives the bright regions; going down the image, their edges rise (into a
/// bright region) or fall. Where bright-is-high is right, the paint's fine luminance is
/// light where the regions rise and dark where they fall. The regions are thresholded on
/// purpose: a linear comparison of the luminance with its own derivative is zero for
/// every texture.
pub fn painted_relief(luminance: &Plane, scale: f32) -> f32 {
    let texels = |radius: f32| ((radius * scale).round() as usize).max(1);
    let detail = luminance.zip_map(&blur(luminance, texels(GRADIENT_RADIUS)), |v, low| v - low);
    let coarse = blur(&detail, texels(4.0));
    let median = percentile(&coarse, None, 0.5);
    let regions = blur(&coarse.map(|value| f32::from(value > median)), texels(2.0));
    let fine = luminance.zip_map(&blur(luminance, texels(4.0)), |v, low| v - low);
    let (mut agreement, mut slopes, mut sum, mut squares) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for y in 0..luminance.height {
        for x in 0..luminance.width {
            let (x, y) = (x as isize, y as isize);
            let rise = f64::from(regions.wrapped(x, y + 1) - regions.wrapped(x, y - 1)) * 0.5;
            let paint = f64::from(fine.wrapped(x, y));
            agreement += paint * rise;
            slopes += rise.abs();
            sum += paint;
            squares += paint * paint;
        }
    }
    let count = (luminance.width * luminance.height) as f64;
    let deviation = (squares / count - (sum / count).powi(2)).max(0.0).sqrt();
    if slopes <= 0.0 || deviation <= 1e-6 {
        return 0.0;
    }
    (agreement / (deviation * slopes)) as f32
}

/// Share of `luminance` darker than [`VOID_LUMINANCE`].
fn void_share(luminance: &Plane) -> f32 {
    let voids = luminance
        .data
        .iter()
        .filter(|value| **value < VOID_LUMINANCE)
        .count();
    voids as f32 / luminance.data.len().max(1) as f32
}

/// Step 2 of the module documentation: an opaque RGBA normal map of `height`,
/// with slopes per texel measured over `step` texels and scaled by `strength`.
pub fn normal_from_height(height: &Plane, strength: f32, step: usize) -> RgbaImage {
    RgbaImage::from_fn(height.width as u32, height.height as u32, |x, y| {
        let (ds, dt) = scharr(height, x as usize, y as usize, step);
        let normal = [-strength * ds, -strength * dt, 1.0];
        let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
        let [r, g, b] = normal.map(|v| to_byte(v / length * 0.5 + 0.5));
        Rgba([r, g, b, 255])
    })
}

/// Step 3 of the module documentation.
fn packed_map(
    class: &MaterialClass,
    [red, green, blue]: [&Plane; 3],
    luminance: &Plane,
    height: &Plane,
    mask: Option<&Plane>,
    scale: f32,
) -> RgbImage {
    let variation_radius = ((2.0 * scale).round() as usize).max(1);
    let cavity_radius = ((3.0 * scale).round() as usize).max(1);
    let mean = blur(luminance, variation_radius);
    let mean_square = blur(&luminance.map(|v| v * v), variation_radius);
    let surroundings = blur(height, cavity_radius);
    let average = {
        let (sum, count) = luminance
            .data
            .iter()
            .enumerate()
            .filter(|(index, _)| mask.is_none_or(|mask| mask.data[*index] >= 0.5))
            .fold((0.0f64, 0usize), |(sum, count), (_, value)| {
                (sum + f64::from(*value), count + 1)
            });
        if count == 0 {
            0.5
        } else {
            (sum / count as f64) as f32
        }
    };
    RgbImage::from_fn(luminance.width as u32, luminance.height as u32, |x, y| {
        let (x, y) = (x as usize, y as usize);
        let deviation = (mean_square.at(x, y) - mean.at(x, y).powi(2))
            .max(0.0)
            .sqrt();
        let busy = (deviation / 0.12).clamp(0.0, 1.0);
        let brightness = (luminance.at(x, y) - average).clamp(-0.5, 0.5);
        let cavity = ((surroundings.at(x, y) - height.at(x, y)) * 4.0).clamp(0.0, 1.0);
        let roughness = (class.roughness
            + class.roughness_variation * ((busy - 0.5) * 0.6 - brightness * 0.8 + cavity * 0.4))
            .clamp(0.02, 1.0);
        let (r, g, b) = (red.at(x, y), green.at(x, y), blue.at(x, y));
        let brightest = r.max(g).max(b);
        let saturation = if brightest > 1e-4 {
            (brightest - r.min(g).min(b)) / brightest
        } else {
            0.0
        };
        let metalness = class.metalness
            * (0.6 + 1.5 * brightness).clamp(0.0, 1.0)
            * (1.0 - smoothstep(0.2, 0.45, saturation));
        let occlusion = 1.0 - class.occlusion * cavity;
        Rgb([to_byte(roughness), to_byte(metalness), to_byte(occlusion)])
    })
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 0–1 to a byte, rounding half away from zero (0.5 becomes 128).
fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::{GENERIC, by_name};
    use std::f32::consts::TAU;

    /// Paint lit from above, 256 texels: a `plate` with 32-texel squares of `inside`
    /// every 64 texels, their top four rows `top` and bottom four `bottom`. Light top
    /// and dark bottom rims are a raised square, the reverse a sunken one.
    fn squares(plate: f32, inside: f32, top: f32, bottom: f32) -> Plane {
        Plane::from_fn(256, 256, |x, y| {
            let (u, v) = ((x + 48) % 64, (y + 48) % 64);
            match (u < 32 && v < 32, v) {
                (false, _) => plate,
                (true, 0..4) => top,
                (true, 28..) => bottom,
                (true, _) => inside,
            }
        })
    }

    #[test]
    fn painted_light_tells_raised_from_sunken() {
        // Bright high is right: bright raised squares, dark pits.
        assert!(painted_relief(&squares(0.35, 0.6, 0.8, 0.15), 1.0) > -INVERT_BELOW);
        assert!(painted_relief(&squares(0.6, 0.35, 0.15, 0.8), 1.0) > -INVERT_BELOW);
        // Upside down: dark raised studs, light inset panels.
        assert!(painted_relief(&squares(0.6, 0.35, 0.8, 0.15), 1.0) < INVERT_BELOW);
        assert!(painted_relief(&squares(0.35, 0.6, 0.15, 0.8), 1.0) < INVERT_BELOW);
        // No painted light, no opinion.
        let flat = painted_relief(&squares(0.35, 0.6, 0.6, 0.6), 1.0);
        assert!(flat.abs() < 0.05, "{flat}");
    }

    #[test]
    fn dark_studs_are_raised_unless_the_overrides_say_otherwise() {
        let paint = squares(0.6, 0.35, 0.8, 0.15);
        let source = RgbaImage::from_fn(256, 256, |x, y| {
            let value = to_byte(paint.at(x as usize, y as usize));
            Rgba([value, value, value, 255])
        });
        let settings = Settings::default();
        let stud = |maps: &Maps| maps.packed.get_pixel(48, 64).0[2];
        let auto = generate(&source, &GENERIC, false, &settings);
        assert_eq!(auto.inverted, Some("painted shading"));
        let kept = generate(
            &source,
            &MaterialClass {
                relief: Relief::Keep,
                ..GENERIC.clone()
            },
            false,
            &settings,
        );
        assert_eq!(kept.inverted, None);
        // Raised, the stud's middle is no cavity; kept upside down, it is one.
        assert!(stud(&auto) > stud(&kept), "{} {}", stud(&auto), stud(&kept));
        let forced = MaterialClass {
            relief: Relief::Inverted,
            ..GENERIC.clone()
        };
        let bright = generate(&source, &forced, false, &settings);
        assert_eq!(bright.inverted, Some("overrides"));
    }

    fn stone() -> &'static MaterialClass {
        by_name("stone").expect("stone class")
    }

    #[test]
    fn flat_texture_gives_flat_maps() {
        let source = RgbaImage::from_pixel(64, 64, Rgba([90, 120, 150, 255]));
        let maps = generate(&source, stone(), false, &Settings::default());
        assert_eq!(maps.normal_kind, NormalKind::Height);
        assert!(maps.flat);
        for pixel in maps.normal.pixels() {
            assert_eq!(pixel.0, [128, 128, 255, 128]);
        }
        let first = maps.packed.get_pixel(0, 0);
        assert!(maps.packed.pixels().all(|pixel| pixel == first));
        // No cavities: full occlusion value; stone is not metal.
        assert_eq!(first.0[1], 0);
        assert_eq!(first.0[2], 255);
    }

    #[test]
    fn height_ramp_tilts_against_the_slope() {
        // Height rising toward +s (right) tilts the normal toward -s; rising
        // toward +t (down) tilts it toward -t: red and green below 128.
        let along_s = Plane::from_fn(32, 32, |x, _| x as f32 * 0.01);
        let pixel = normal_from_height(&along_s, 10.0, 1).get_pixel(10, 10).0;
        let expected = -0.1 / (1.0f32 + 0.01).sqrt();
        assert_eq!(pixel[0], to_byte(expected * 0.5 + 0.5));
        assert_eq!(pixel[1], 128);
        assert!(pixel[0] < 128);
        let along_t = Plane::from_fn(32, 32, |_, y| y as f32 * 0.01);
        let pixel = normal_from_height(&along_t, 10.0, 1).get_pixel(10, 10).0;
        assert_eq!(pixel[0], 128);
        assert_eq!(pixel[1], to_byte(expected * 0.5 + 0.5));
    }

    #[test]
    fn brighter_stripes_are_raised() {
        // Luminance rising to the right inside each stripe: the height rises
        // with it and the normal leans left (red below 128).
        let source = RgbaImage::from_fn(128, 128, |x, _| {
            let v = 128.0 + 100.0 * (TAU * x as f32 / 32.0).sin();
            Rgba([v as u8, v as u8, v as u8, 255])
        });
        let maps = generate(&source, &GENERIC, false, &Settings::default());
        // sin rises fastest at x = 0, 32, 64...
        assert!(maps.normal.get_pixel(64, 5).0[0] < 120);
        // ...and falls fastest at x = 16, 48...
        assert!(maps.normal.get_pixel(48, 5).0[0] > 136);
        assert!(!maps.flat);
        assert!((127..=129).contains(&maps.normal.get_pixel(64, 5).0[1]));
    }

    #[test]
    fn baked_gradients_are_removed() {
        // A strong bright-to-dark sweep across the whole texture (three times the
        // detail's contrast) over a fine checker.
        let size = 256;
        let source = RgbaImage::from_fn(size, size, |x, y| {
            let gradient = 0.3 * (TAU * x as f32 / size as f32).sin();
            let detail = 0.1 * if (x / 4 + y / 4) % 2 == 0 { 1.0 } else { -1.0 };
            let v = ((0.5 + gradient + detail) * 255.0) as u8;
            Rgba([v, v, v, 255])
        });
        let luminance = Plane::from_fn(size as usize, size as usize, |x, y| {
            f32::from(source.get_pixel(x as u32, y as u32).0[0]) / 255.0
        });
        let halves = |plane: &Plane| {
            let half = |range: std::ops::Range<usize>| {
                let mut sum = 0.0;
                for y in 0..plane.height {
                    for x in range.clone() {
                        sum += plane.at(x, y);
                    }
                }
                sum / (range.len() * plane.height) as f32
            };
            (half(0..128) - half(128..256)).abs()
        };
        // Plain normalisation keeps the sweep as most of the height range...
        let low = percentile(&luminance, None, 0.02);
        let high = percentile(&luminance, None, 0.98);
        let plain = halves(&luminance.map(|v| (v - low) / (high - low)));
        // ...the high-pass leaves about a quarter of it (a 256-texel period
        // keeps 7% after two passes at radius 32; the coarse layer is not cut),
        let height = height_from_luminance(&luminance, None, 1.0, 1.0);
        let residual = halves(&height);
        assert!(residual < 0.3 * plain, "residual {residual} plain {plain}");
        // and that fraction no longer tilts the normals: the regions where the
        // sweep rises (x near 0) and falls (x near 128) lean the same way.
        let maps = generate(&source, stone(), false, &Settings::default());
        let mean_red = |columns: std::ops::Range<u32>| {
            let mut sum = 0.0;
            for y in 0..size {
                for x in columns.clone() {
                    sum += f32::from(maps.normal.get_pixel(x, y).0[0]);
                }
            }
            sum / (columns.len() as u32 * size) as f32
        };
        let (rising, falling) = (mean_red(0..16), mean_red(120..136));
        assert!(
            (rising - falling).abs() < 2.0,
            "rising {rising} falling {falling}"
        );
    }

    #[test]
    fn maps_tile_seamlessly() {
        // Circularly shifting a tiling texture shifts every map by the same
        // amount: nothing depends on where the image edges are.
        let pattern = |x: u32, y: u32| {
            let v = 128.0
                + 60.0 * (TAU * x as f32 / 64.0).sin() * (TAU * y as f32 / 32.0).cos()
                + 30.0 * (TAU * (x + 2 * y) as f32 / 16.0).sin();
            Rgba([v as u8, (v * 0.9) as u8, (v * 0.8) as u8, 255])
        };
        let (width, height, shift) = (128, 64, 37);
        let source = RgbaImage::from_fn(width, height, pattern);
        let shifted = RgbaImage::from_fn(width, height, |x, y| pattern((x + shift) % width, y));
        let a = generate(&source, stone(), false, &Settings::default());
        let b = generate(&shifted, stone(), false, &Settings::default());
        for y in 0..height {
            for x in 0..width {
                let (pa, pb) = (
                    a.normal.get_pixel((x + shift) % width, y),
                    b.normal.get_pixel(x, y),
                );
                for channel in 0..4 {
                    assert!(
                        pa.0[channel].abs_diff(pb.0[channel]) <= 1,
                        "normal at {x},{y}"
                    );
                }
                let (pa, pb) = (
                    a.packed.get_pixel((x + shift) % width, y),
                    b.packed.get_pixel(x, y),
                );
                for channel in 0..3 {
                    assert!(
                        pa.0[channel].abs_diff(pb.0[channel]) <= 1,
                        "packed at {x},{y}"
                    );
                }
            }
        }
        // The seam column pair differs no more than an interior pair does.
        let step = |x0: u32, x1: u32| {
            (0..height)
                .map(|y| a.normal.get_pixel(x0, y).0[0].abs_diff(a.normal.get_pixel(x1, y).0[0]))
                .max()
                .unwrap_or(0)
        };
        assert!(step(width - 1, 0) <= step(0, 1) + step(1, 2) + 2);
    }

    #[test]
    fn large_textures_do_not_steepen_texel_noise() {
        // The same texel noise on a 1024 texture (a high-resolution replacement)
        // leans its normals less than on a 512 one: it is finer on the wall.
        let noise = |size: u32| {
            let mut state = 12345u32;
            RgbaImage::from_fn(size, size, |_, _| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = 96 + (state >> 26) as u8;
                Rgba([v, v, v, 255])
            })
        };
        let mean_lean = |image: &RgbaImage| {
            let maps = generate(image, &GENERIC, false, &Settings::default());
            let sum: f32 = maps
                .normal
                .pixels()
                .map(|p| f32::from(p.0[0].abs_diff(128)) + f32::from(p.0[1].abs_diff(128)))
                .sum();
            sum / (image.width() * image.height()) as f32
        };
        let (small, large) = (mean_lean(&noise(512)), mean_lean(&noise(1024)));
        assert!(large < small, "1024: {large}, 512: {small}");
    }

    #[test]
    fn alpha_tested_textures_keep_their_alpha() {
        let source = RgbaImage::from_fn(64, 64, |x, y| {
            let v = if (x / 8 + y / 8) % 2 == 0 { 200 } else { 60 };
            let alpha = if x < 32 { 255 } else { 0 };
            Rgba([v, v, v, alpha])
        });
        let metal = by_name("metal").expect("metal class");
        let maps = generate(&source, metal, true, &Settings::default());
        assert_eq!(maps.normal_kind, NormalKind::Plain);
        assert!(maps.normal_alpha);
        for (x, y, pixel) in maps.normal.enumerate_pixels() {
            assert_eq!(pixel.0[3], source.get_pixel(x, y).0[3]);
            if x >= 32 {
                assert_eq!(&pixel.0[..3], &[128, 128, 255]);
            }
        }
        // Parallax classes never get height on alpha-tested textures.
        let maps = generate(&source, stone(), true, &Settings::default());
        assert_eq!(maps.normal_kind, NormalKind::Plain);
    }

    #[test]
    fn opaque_plain_maps_are_opaque() {
        let source = RgbaImage::from_fn(32, 32, |x, _| Rgba([x as u8 * 8, 0, 0, 7]));
        let maps = generate(&source, &GENERIC, false, &Settings::default());
        assert_eq!(maps.normal_kind, NormalKind::Plain);
        assert!(!maps.normal_alpha);
        assert!(maps.normal.pixels().all(|pixel| pixel.0[3] == 255));
    }

    #[test]
    fn size_cap_halves_even_textures() {
        let source = RgbaImage::from_pixel(512, 256, Rgba([10, 20, 30, 255]));
        let settings = Settings {
            max_size: Some(128),
            ..Settings::default()
        };
        let maps = generate(&source, &GENERIC, false, &settings);
        assert_eq!(maps.normal.dimensions(), (128, 64));
        assert_eq!(maps.packed.dimensions(), (128, 64));
        let odd = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        assert_eq!(cap_size(&odd, Some(16)).dimensions(), (50, 25));
    }

    #[test]
    fn metal_is_metallic_only_where_bright_and_grey() {
        let metal = by_name("metal").expect("metal class");
        let source = RgbaImage::from_fn(64, 64, |x, _| {
            if x < 32 {
                Rgba([200, 200, 200, 255])
            } else {
                Rgba([200, 30, 30, 255])
            }
        });
        let maps = generate(&source, metal, false, &Settings::default());
        let grey = maps.packed.get_pixel(10, 10).0[1];
        let painted = maps.packed.get_pixel(48, 10).0[1];
        assert!(grey > 40, "grey metal {grey}");
        assert_eq!(painted, 0);
    }
}
