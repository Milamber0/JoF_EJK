//! Degree-two triangle quadrature of texture emission and its spatial moments.
use super::*;

/// One sampled triangle, ready for shape-preserving patch accumulation.
pub(super) struct Sample {
    pub(super) position: Vec3,
    pub(super) uv: Vec2,
    pub(super) normal: Vec3,
    pub(super) luminance: f32,
    pub(super) weight: f32,
    pub(super) area: f32,
    pub(super) points: [Vec3; 3],
    pub(super) colors: [Vec3; 3],
    pub(super) gradient: [Vec3; 2],
}

pub(super) fn sample(corners: [Corner; 3], emitter: &Emitter<'_>, lod: u32) -> Option<Sample> {
    let p = corners.map(position);
    let coords = corners.map(uv);
    let radiance = Vec3::from_array(emitter.radiance);
    let cross = (p[1] - p[0]).cross(p[2] - p[0]);
    let area = cross.length() * 0.5;
    if area < 1e-6 || !area.is_finite() {
        return None;
    }
    let center_uv = (coords[0] + coords[1] + coords[2]) / 3.;
    let centroid = (p[0] + p[1] + p[2]) / 3.;
    let sample_uv = coords.map(|p| center_uv * 0.5 + p * 0.5);
    let samples = p.map(|p| centroid * 0.5 + p * 0.5);
    let colors = sample_uv.map(|uv| radiance * emitter.texture.sample(uv, lod));
    let luminance = colors.iter().map(|&c| luma(c)).sum::<f32>();
    if colors.iter().any(|c| !c.is_finite()) || luminance <= 1e-6 {
        return None;
    }
    let centroid = (0..3).map(|i| samples[i] * luma(colors[i])).sum::<Vec3>() / luminance;
    let center_uv = (0..3).map(|i| sample_uv[i] * luma(colors[i])).sum::<Vec2>() / luminance;
    let luminance = luma(radiance * emitter.texture.sample(center_uv, lod));
    let authored: Vec3 = corners.iter().map(|c| Vec3::from_array(c.1)).sum();
    let normal = (cross / (area * 2.)) * if cross.dot(authored) < 0. { -1. } else { 1. };
    let weight = colors.iter().map(|&c| luma(c)).sum::<f32>() * area / 3.;
    let a = p[1] - p[0];
    let b = p[2] - p[0];
    let n = a.cross(b);
    let gradient = std::array::from_fn(|i| {
        ((coords[1][i] - coords[0][i]) * b.cross(n) + (coords[2][i] - coords[0][i]) * n.cross(a))
            / n.length_squared()
    });
    Some(Sample {
        position: centroid,
        uv: center_uv,
        normal,
        luminance,
        weight,
        area,
        points: samples,
        colors,
        gradient,
    })
}
