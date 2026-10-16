//! Conservative screen coverage for mirror work; near-plane clipping avoids distance cutoffs.
use glam::{Mat4, Vec2, Vec3};

/// Normalized screen rectangle, clipped to the viewport, for a world-space box.
pub(super) fn project(matrix: Mat4, bounds: [[f32; 3]; 2]) -> Option<[f32; 4]> {
    let points: [_; 8] = std::array::from_fn(|i| {
        matrix
            * Vec3::new(
                bounds[i & 1][0],
                bounds[(i >> 1) & 1][1],
                bounds[(i >> 2) & 1][2],
            )
            .extend(1.)
    });
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    let mut include = |p: glam::Vec4| {
        if p.w > 0. {
            let uv = Vec2::new(p.x, -p.y) / p.w * 0.5 + Vec2::splat(0.5);
            lo = lo.min(uv);
            hi = hi.max(uv);
        }
    };
    for (i, &p) in points.iter().enumerate() {
        if p.z >= 0. {
            include(p);
        }
        for bit in [1, 2, 4] {
            if i & bit != 0 {
                continue;
            }
            let q = points[i | bit];
            if (p.z < 0.) != (q.z < 0.) {
                include(p.lerp(q, p.z / (p.z - q.z)));
            }
        }
    }
    lo = lo.max(Vec2::ZERO);
    hi = hi.min(Vec2::ONE);
    (hi.x > lo.x && hi.y > lo.y).then_some([lo.x, lo.y, hi.x, hi.y])
}

pub(super) fn pixels(rect: [f32; 4], size: [u32; 2]) -> [u32; 4] {
    let x = (rect[0] * size[0] as f32).floor() as u32;
    let y = (rect[1] * size[1] as f32).floor() as u32;
    let right = (rect[2] * size[0] as f32).ceil() as u32;
    let bottom = (rect[3] * size[1] as f32).ceil() as u32;
    [x, y, right.min(size[0]) - x, bottom.min(size[1]) - y]
}

/// Clip-space side planes for the pixels a reflection actually draws. This changes
/// CPU geometry selection only; uploaded camera/raster coordinates stay identical.
/// Two extra pixels retain raster helper lanes at the scissor boundary.
pub(super) fn receiver_frustum(clip: Mat4, rect: [f32; 4], size: [u32; 2], scale: f32) -> Mat4 {
    let [x, y, w, h] = pixels(rect, size);
    let dims = Vec2::new(size[0] as f32, size[1] as f32) * scale;
    if w == 0 || h == 0 || dims.min_element() <= 0. {
        return clip;
    }
    let low = ((Vec2::new(x as f32, y as f32) - Vec2::splat(2.)) / dims).max(Vec2::ZERO);
    let high =
        ((Vec2::new((x + w) as f32, (y + h) as f32) + Vec2::splat(2.)) / dims).min(Vec2::ONE);
    let span = high - low;
    if span.min_element() <= 0. {
        return clip;
    }
    let mut crop = Mat4::from_scale(Vec3::new(1. / span.x, 1. / span.y, 1.));
    crop.w_axis.x = (1. - low.x - high.x) / span.x;
    crop.w_axis.y = (low.y + high.y - 1.) / span.y;
    crop * clip
}
