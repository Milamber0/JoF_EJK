//! Signed distance fields for the retail bitmap font atlases.
//!
//! Raven's fonts are anti-aliased coverage bitmaps, 512 texels on the long side or
//! less.
//! UI text at 1440p or 4K magnifies a glyph several times, and bilinear sampling of
//! coverage then turns every edge into a ramp several screen pixels wide. A distance
//! field stores, per texel, how far the glyph edge is; bilinear sampling of distance
//! is accurate between texels, so the shader can rebuild an edge that is one screen
//! pixel wide at any magnification.
//!
//! The edge is the 0.5 contour of the bilinearly interpolated coverage, which is what
//! the retail renderer's filtered sampling shows at 1:1. The coverage is resampled at
//! up to [`WORKING_LONG_SIDE`] texels, split into inside and outside at that contour,
//! and an exact Euclidean distance transform measures both sides (Felzenszwalb and
//! Huttenlocher, *Distance Transforms of Sampled Functions*, 2012). Each output texel
//! averages its block of the working grid.
//!
//! A retail-size atlas's field is stored at twice its size: distance is not linear
//! across a stroke's centre line, so at the atlas's own size a stroke one texel wide
//! would be rebuilt half as wide; at twice the size it keeps its width. An HD atlas's
//! strokes are already several texels wide, so its field keeps the atlas size.
//! Glyph rectangles are normalised, so they and the metrics are unchanged. The field
//! is the alpha channel of a white RGBA image, so the text pipeline's atlas layout,
//! mip chain and sampler stay as they are.
//!
//! HD replacement atlases of [`HD_LONG_SIDE`] texels or more (4x retail, such as an
//! 8x `ergoec`) already resolve 4K text and keep their own anti-aliasing, so
//! [`for_atlas`] leaves them as coverage; smaller HD atlases get a field like
//! retail ones.

use image::RgbaImage;

/// Distance, in output texels, that maps to the full 0..1 range either side of the
/// edge (encoded 0.5). It must exceed the anti-aliasing band at the coarsest scale the
/// font is sampled at: one screen pixel spans at most about two texels of the mip
/// level the sampler picks.
pub(crate) const SPREAD: f32 = 4.0;
/// Working-grid long side that sub-texel edge positions are resolved at; a retail
/// 512-texel atlas is resampled 4x, an HD atlas of 2048 texels or more is used as is.
const WORKING_LONG_SIDE: u32 = 2_048;
/// Largest supersampling factor of the working grid.
const MAX_SUPERSAMPLE: u32 = 4;
/// Long side of the retail atlases, whose fields are stored at twice their size.
const RETAIL_LONG_SIDE: u32 = 512;
/// Atlases this large on the long side are drawn from their coverage.
pub(crate) const HD_LONG_SIDE: u32 = 2_048;

/// The texture to upload for a font `atlas` and whether it is a distance field:
/// a field for retail-size atlases, the atlas itself for large HD ones.
pub(crate) fn for_atlas(atlas: image::RgbaImage) -> (image::RgbaImage, bool) {
    if atlas.width().max(atlas.height()) >= HD_LONG_SIDE {
        (atlas, false)
    } else {
        (distance_field(&atlas), true)
    }
}

/// Replace `atlas`'s coverage with a signed distance field: white RGB, alpha 0.5 on
/// the glyph edge, rising inside and falling outside over [`SPREAD`] texels. A small
/// atlas's field is larger than the atlas (see [`output_scale`]).
pub(crate) fn distance_field(atlas: &RgbaImage) -> RgbaImage {
    let (width, height) = atlas.dimensions();
    let factor = supersample(width, height);
    let coverage: Vec<f32> = atlas
        .pixels()
        .map(|pixel| f32::from(pixel[3]) / 255.0)
        .collect();
    let (work_width, work_height) = (width * factor, height * factor);
    let inside = resample_inside(&coverage, width, height, factor);
    // Squared distance of every texel to the nearest texel of the other class.
    let to_inside = squared_distance(&inside, work_width, work_height, true);
    let to_outside = squared_distance(&inside, work_width, work_height, false);
    let signed: Vec<f32> = inside
        .iter()
        .zip(to_inside.iter().zip(&to_outside))
        .map(|(&is_inside, (&near_in, &near_out))| {
            // Texel centres sit half a texel from the boundary between classes.
            if is_inside {
                -(near_out.sqrt() - 0.5)
            } else {
                near_in.sqrt() - 0.5
            }
        })
        .collect();
    let output = output_scale(width, height);
    let step = factor / output;
    let block = (step * step) as f32;
    let scale = 1.0 / (step as f32 * 2.0 * SPREAD);
    RgbaImage::from_fn(width * output, height * output, |x, y| {
        let mut sum = 0.0;
        for dy in 0..step {
            let row = ((y * step + dy) * work_width) as usize;
            for dx in 0..step {
                sum += signed[row + (x * step + dx) as usize];
            }
        }
        let value = (0.5 - sum / block * scale).clamp(0.0, 1.0);
        image::Rgba([255, 255, 255, (value * 255.0).round() as u8])
    })
}

/// Working-grid supersampling for an atlas of `width` x `height`.
fn supersample(width: u32, height: u32) -> u32 {
    (WORKING_LONG_SIDE / width.max(height).max(1)).clamp(1, MAX_SUPERSAMPLE)
}

/// Size of the stored field relative to an atlas of `width` x `height`: twice for
/// retail-size atlases, whose strokes can be one texel wide, else the atlas size.
fn output_scale(width: u32, height: u32) -> u32 {
    if width.max(height) <= RETAIL_LONG_SIDE {
        supersample(width, height).min(2)
    } else {
        1
    }
}

/// Bilinearly resample `coverage` `factor` times and classify each working texel as
/// inside (interpolated coverage at least 0.5) or outside.
fn resample_inside(coverage: &[f32], width: u32, height: u32, factor: u32) -> Vec<bool> {
    let at = |x: i64, y: i64| {
        let x = x.clamp(0, i64::from(width) - 1) as usize;
        let y = y.clamp(0, i64::from(height) - 1) as usize;
        coverage[y * width as usize + x]
    };
    let step = 1.0 / factor as f32;
    let (work_width, work_height) = (width * factor, height * factor);
    let mut inside = Vec::with_capacity((work_width * work_height) as usize);
    for y in 0..work_height {
        let source_y = (y as f32 + 0.5) * step - 0.5;
        let y0 = source_y.floor();
        let fy = source_y - y0;
        let y0 = y0 as i64;
        for x in 0..work_width {
            let source_x = (x as f32 + 0.5) * step - 0.5;
            let x0 = source_x.floor();
            let fx = source_x - x0;
            let x0 = x0 as i64;
            let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1, y0) * fx;
            let bottom = at(x0, y0 + 1) * (1.0 - fx) + at(x0 + 1, y0 + 1) * fx;
            inside.push(top * (1.0 - fy) + bottom * fy >= 0.5);
        }
    }
    inside
}

/// Squared Euclidean distance from every texel to the nearest texel whose class is
/// `target`; texels of that class are at zero.
fn squared_distance(inside: &[bool], width: u32, height: u32, target: bool) -> Vec<f32> {
    let (width, height) = (width as usize, height as usize);
    let mut field: Vec<f32> = inside
        .iter()
        .map(|&class| if class == target { 0.0 } else { f32::INFINITY })
        .collect();
    let longest = width.max(height);
    let mut line = vec![0.0; longest];
    let mut output = vec![0.0; longest];
    let mut parabolas = vec![0_usize; longest];
    let mut bounds = vec![0.0; longest + 1];
    for x in 0..width {
        for y in 0..height {
            line[y] = field[y * width + x];
        }
        transform_line(&line[..height], &mut output, &mut parabolas, &mut bounds);
        for y in 0..height {
            field[y * width + x] = output[y];
        }
    }
    for y in 0..height {
        let row = &mut field[y * width..(y + 1) * width];
        line[..width].copy_from_slice(row);
        transform_line(&line[..width], &mut output, &mut parabolas, &mut bounds);
        row.copy_from_slice(&output[..width]);
    }
    field
}

/// One-dimensional squared distance transform of `samples` (the lower envelope of
/// parabolas rooted at each finite sample) into `output`.
fn transform_line(
    samples: &[f32],
    output: &mut [f32],
    parabolas: &mut [usize],
    bounds: &mut [f32],
) {
    let count = samples.len();
    let Some(first) = samples.iter().position(|value| value.is_finite()) else {
        output[..count].fill(f32::INFINITY);
        return;
    };
    let mut top = 0;
    parabolas[0] = first;
    bounds[0] = f32::NEG_INFINITY;
    bounds[1] = f32::INFINITY;
    for q in first + 1..count {
        if !samples[q].is_finite() {
            continue;
        }
        let intersection = |v: usize| {
            ((samples[q] + (q * q) as f32) - (samples[v] + (v * v) as f32))
                / (2.0 * (q as f32 - v as f32))
        };
        let mut s = intersection(parabolas[top]);
        while s <= bounds[top] {
            top -= 1;
            s = intersection(parabolas[top]);
        }
        top += 1;
        parabolas[top] = q;
        bounds[top] = s;
        bounds[top + 1] = f32::INFINITY;
    }
    let mut k = 0;
    for (q, value) in output[..count].iter_mut().enumerate() {
        while bounds[k + 1] < q as f32 {
            k += 1;
        }
        let offset = q as f32 - parabolas[k] as f32;
        *value = offset * offset + samples[parabolas[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Coverage, in atlas texels, that the text shader rebuilds from `field` when one
    /// screen pixel covers one field texel.
    fn rebuilt_coverage(field: &RgbaImage, scale: u32) -> f32 {
        let band = 1.0 / (2.0 * SPREAD);
        let sum: f32 = field
            .pixels()
            .map(|pixel| ((f32::from(pixel[3]) / 255.0 - 0.5) / band + 0.5).clamp(0.0, 1.0))
            .sum();
        sum / (scale * scale) as f32
    }

    fn coverage(atlas: &RgbaImage) -> f32 {
        atlas
            .pixels()
            .map(|pixel| f32::from(pixel[3]) / 255.0)
            .sum()
    }

    fn atlas(width: u32, height: u32, alpha: impl Fn(u32, u32) -> u8) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([255, 255, 255, alpha(x, y)])
        })
    }

    #[test]
    fn one_texel_stroke_keeps_its_weight() {
        let stroke = atlas(64, 64, |x, y| {
            if x == 20 && (8..56).contains(&y) {
                255
            } else {
                0
            }
        });
        let field = distance_field(&stroke);
        assert_eq!(field.dimensions(), (128, 128));
        let rebuilt = rebuilt_coverage(&field, 2);
        let original = coverage(&stroke);
        assert!(
            (rebuilt - original).abs() / original < 0.05,
            "{rebuilt} vs {original}"
        );
        // Both field texels of the stroke are inside; the next ones out are outside.
        assert!(field.get_pixel(40, 64)[3] > 128 && field.get_pixel(41, 64)[3] > 128);
        assert!(field.get_pixel(39, 64)[3] < 128 && field.get_pixel(42, 64)[3] < 128);
    }

    #[test]
    fn anti_aliased_disc_keeps_its_area() {
        // A disc of radius 10 with a one-texel coverage ramp, like a retail glyph edge.
        let disc = atlas(64, 64, |x, y| {
            let distance = ((x as f32 - 31.5).powi(2) + (y as f32 - 31.5).powi(2)).sqrt();
            ((10.5 - distance).clamp(0.0, 1.0) * 255.0).round() as u8
        });
        let rebuilt = rebuilt_coverage(&distance_field(&disc), 2);
        let original = coverage(&disc);
        assert!(
            (rebuilt - original).abs() / original < 0.03,
            "{rebuilt} vs {original}"
        );
    }

    #[test]
    fn distance_is_encoded_around_the_edge() {
        // Left half covered: the edge lies between field columns 31 and 32.
        let half = atlas(32, 8, |x, _| if x < 16 { 255 } else { 0 });
        let field = distance_field(&half);
        let encoded = |x| f32::from(field.get_pixel(x, 8)[3]) / 255.0;
        let per_texel = 1.0 / (2.0 * SPREAD);
        assert!((encoded(31) - (0.5 + 0.5 * per_texel)).abs() < 0.01);
        assert!((encoded(32) - (0.5 - 0.5 * per_texel)).abs() < 0.01);
        assert!((encoded(28) - (0.5 + 3.5 * per_texel)).abs() < 0.01);
        assert_eq!(field.get_pixel(0, 8)[3], 255);
        assert_eq!(field.get_pixel(63, 8)[3], 0);
        assert_eq!(&field.get_pixel(63, 8).0[..3], &[255, 255, 255]);
    }

    #[test]
    fn empty_and_full_atlases_saturate() {
        assert!(
            distance_field(&atlas(8, 8, |_, _| 0))
                .pixels()
                .all(|p| p[3] == 0)
        );
        assert!(
            distance_field(&atlas(8, 8, |_, _| 255))
                .pixels()
                .all(|p| p[3] == 255)
        );
    }

    #[test]
    fn working_grid_targets_two_thousand_texels() {
        assert_eq!(supersample(512, 512), 4);
        assert_eq!(supersample(1_024, 512), 2);
        assert_eq!(supersample(4_096, 2_048), 1);
        assert_eq!(supersample(64, 64), 4);
        assert_eq!(output_scale(512, 512), 2);
        assert_eq!(output_scale(1_024, 512), 1);
        assert_eq!(output_scale(2_048, 1_024), 1);
    }

    #[test]
    fn large_hd_atlases_keep_their_coverage() {
        let (hd, field) = for_atlas(atlas(2_048, 4, |_, _| 7));
        assert!(!field);
        assert_eq!(hd.get_pixel(0, 0)[3], 7);
        let (retail, field) = for_atlas(atlas(512, 4, |_, _| 0));
        assert!(field);
        assert_eq!(retail.dimensions(), (1_024, 8));
    }
}
