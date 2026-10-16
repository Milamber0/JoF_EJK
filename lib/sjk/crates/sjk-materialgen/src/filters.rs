//! Single-channel image planes and the wrap-around filters the generator uses.
//!
//! Every filter treats the image as a torus: a sample left of column 0 is
//! column `width - 1`, and likewise for rows. World textures tile, so this
//! keeps the generated maps seamless where the source is.

/// A `width` × `height` plane of `f32` samples in row-major order.
#[derive(Clone, Debug, PartialEq)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Plane {
    /// A plane of `value`.
    pub fn filled(width: usize, height: usize, value: f32) -> Self {
        Self {
            width,
            height,
            data: vec![value; width * height],
        }
    }

    /// A plane computed from each sample's coordinates.
    pub fn from_fn(width: usize, height: usize, f: impl Fn(usize, usize) -> f32) -> Self {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                data.push(f(x, y));
            }
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// The sample at (`x`, `y`), wrapping both coordinates.
    pub fn wrapped(&self, x: isize, y: isize) -> f32 {
        let x = x.rem_euclid(self.width as isize) as usize;
        let y = y.rem_euclid(self.height as isize) as usize;
        self.data[y * self.width + x]
    }

    /// The sample at (`x`, `y`).
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }

    /// Combine two planes of the same size sample by sample.
    pub fn zip_map(&self, other: &Plane, f: impl Fn(f32, f32) -> f32) -> Plane {
        debug_assert_eq!((self.width, self.height), (other.width, other.height));
        Plane {
            width: self.width,
            height: self.height,
            data: self
                .data
                .iter()
                .zip(&other.data)
                .map(|(&a, &b)| f(a, b))
                .collect(),
        }
    }

    /// Apply `f` to every sample.
    pub fn map(&self, f: impl Fn(f32) -> f32) -> Plane {
        Plane {
            width: self.width,
            height: self.height,
            data: self.data.iter().map(|&a| f(a)).collect(),
        }
    }
}

/// Approximate Gaussian blur of standard deviation about `radius` texels:
/// three wrap-around box passes in each direction. A radius of 0 copies.
pub fn blur(plane: &Plane, radius: usize) -> Plane {
    let mut result = plane.clone();
    if radius == 0 {
        return result;
    }
    let mut scratch = Vec::new();
    for _ in 0..3 {
        box_rows(&mut result, radius, &mut scratch);
        box_columns(&mut result, radius, &mut scratch);
    }
    result
}

/// Blur of `value` weighted by `weight` (normalised convolution): texels of
/// zero weight (transparent ones) neither contribute nor pull the result down.
pub fn weighted_blur(value: &Plane, weight: &Plane, radius: usize) -> Plane {
    let product = value.zip_map(weight, |v, w| v * w);
    let numerator = blur(&product, radius);
    let denominator = blur(weight, radius);
    numerator.zip_map(&denominator, |n, d| if d > 1e-4 { n / d } else { 0.0 })
}

/// One box pass along each row, with the window clamped to the row length.
fn box_rows(plane: &mut Plane, radius: usize, scratch: &mut Vec<f32>) {
    let width = plane.width;
    for row in plane.data.chunks_exact_mut(width) {
        box_line(row, radius, scratch);
    }
}

/// One box pass along each column.
fn box_columns(plane: &mut Plane, radius: usize, scratch: &mut Vec<f32>) {
    let (width, height) = (plane.width, plane.height);
    let mut column = vec![0.0; height];
    for x in 0..width {
        for (y, value) in column.iter_mut().enumerate() {
            *value = plane.data[y * width + x];
        }
        box_line(&mut column, radius, scratch);
        for (y, value) in column.iter().enumerate() {
            plane.data[y * width + x] = *value;
        }
    }
}

/// Wrap-around moving average of `line` over `2 * radius + 1` samples, with a
/// running sum in `f64` so the result does not drift along the line.
fn box_line(line: &mut [f32], radius: usize, scratch: &mut Vec<f32>) {
    let length = line.len();
    // A window wider than the line would count samples twice.
    let radius = radius.min(length.saturating_sub(1) / 2);
    if radius == 0 {
        return;
    }
    scratch.clear();
    scratch.extend_from_slice(line);
    let window = (2 * radius + 1) as f64;
    let index = |i: isize| i.rem_euclid(length as isize) as usize;
    let mut sum: f64 = (-(radius as isize)..=radius as isize)
        .map(|i| f64::from(scratch[index(i)]))
        .sum();
    for (x, value) in line.iter_mut().enumerate() {
        *value = (sum / window) as f32;
        let x = x as isize;
        sum += f64::from(scratch[index(x + radius as isize + 1)]);
        sum -= f64::from(scratch[index(x - radius as isize)]);
    }
}

/// Scharr derivative of `plane` along x and y in units per texel, wrapping,
/// with the stencil's taps `step` texels apart (1 for the classic 3×3).
/// Positive x is the next column, positive y the next row.
pub fn scharr(plane: &Plane, x: usize, y: usize, step: usize) -> (f32, f32) {
    let (x, y, s) = (x as isize, y as isize, step.max(1) as isize);
    let p = |dx: isize, dy: isize| plane.wrapped(x + dx * s, y + dy * s);
    let gx =
        3.0 * (p(1, -1) - p(-1, -1)) + 10.0 * (p(1, 0) - p(-1, 0)) + 3.0 * (p(1, 1) - p(-1, 1));
    let gy =
        3.0 * (p(-1, 1) - p(-1, -1)) + 10.0 * (p(0, 1) - p(0, -1)) + 3.0 * (p(1, 1) - p(1, -1));
    // Each side weighs 16 and the two sides are two steps apart.
    let norm = 32.0 * s as f32;
    (gx / norm, gy / norm)
}

/// The value below which `fraction` of the samples lie (nearest rank), over
/// the samples whose `weight` is at least one half.
pub fn percentile(values: &Plane, weight: Option<&Plane>, fraction: f32) -> f32 {
    let mut samples: Vec<f32> = match weight {
        Some(weight) => values
            .data
            .iter()
            .zip(&weight.data)
            .filter(|(_, w)| **w >= 0.5)
            .map(|(v, _)| *v)
            .collect(),
        None => values.data.clone(),
    };
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(f32::total_cmp);
    let rank = ((samples.len() - 1) as f32 * fraction.clamp(0.0, 1.0)).round() as usize;
    samples[rank]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_keeps_constants_and_the_mean() {
        let flat = Plane::filled(16, 8, 0.25);
        assert!(blur(&flat, 3).data.iter().all(|v| (v - 0.25).abs() < 1e-6));
        let noisy = Plane::from_fn(16, 8, |x, y| ((x * 7 + y * 3) % 5) as f32);
        let mean = |p: &Plane| p.data.iter().sum::<f32>() / p.data.len() as f32;
        assert!((mean(&noisy) - mean(&blur(&noisy, 2))).abs() < 1e-4);
    }

    #[test]
    fn blur_wraps_around() {
        // A single bright column at x = 0 spreads to both sides of the seam.
        let line = Plane::from_fn(16, 1, |x, _| if x == 0 { 1.0 } else { 0.0 });
        let blurred = blur(&line, 1);
        assert!((blurred.at(1, 0) - blurred.at(15, 0)).abs() < 1e-6);
        assert!(blurred.at(15, 0) > 0.0);
    }

    #[test]
    fn oversized_radius_is_clamped() {
        let line = Plane::from_fn(4, 4, |x, _| x as f32);
        let blurred = blur(&line, 100);
        assert!(blurred.data.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn weighted_blur_ignores_zero_weight() {
        let value = Plane::from_fn(8, 8, |x, _| if x < 4 { 1.0 } else { 100.0 });
        let weight = Plane::from_fn(8, 8, |x, _| if x < 4 { 1.0 } else { 0.0 });
        let blurred = weighted_blur(&value, &weight, 2);
        assert!((blurred.at(1, 3) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn scharr_measures_slope_per_texel() {
        let ramp = Plane::from_fn(8, 8, |x, y| x as f32 * 0.5 + y as f32 * 0.25);
        let (gx, gy) = scharr(&ramp, 3, 3, 1);
        assert!((gx - 0.5).abs() < 1e-6 && (gy - 0.25).abs() < 1e-6);
        let (gx, gy) = scharr(&ramp, 3, 3, 2);
        assert!((gx - 0.5).abs() < 1e-6 && (gy - 0.25).abs() < 1e-6);
    }

    #[test]
    fn percentile_ranks() {
        let values = Plane::from_fn(101, 1, |x, _| x as f32);
        assert_eq!(percentile(&values, None, 0.0), 0.0);
        assert_eq!(percentile(&values, None, 0.5), 50.0);
        assert_eq!(percentile(&values, None, 1.0), 100.0);
        let weight = Plane::from_fn(101, 1, |x, _| if x >= 50 { 1.0 } else { 0.0 });
        assert_eq!(percentile(&values, Some(&weight), 0.0), 50.0);
    }
}
