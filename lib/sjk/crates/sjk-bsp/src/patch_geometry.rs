//! Load-time plane/winding helpers matching codemp cm_patch/cm_polylib.
use super::{Plane, dot};

pub(super) type Point = [f32; 3];
pub(super) const MAP_BOUNDS: f32 = 65535.0;
pub(super) const NORMAL_EPSILON: f32 = 0.00015;

pub(super) fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(super) fn add(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(super) fn scale(a: Point, s: f32) -> Point {
    a.map(|v| v * s)
}
pub(super) fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(super) fn normalize(v: &mut Point) -> f32 {
    let length = dot(*v, *v).sqrt();
    if length != 0.0 {
        *v = scale(*v, 1.0 / length);
    }
    length
}
pub(super) fn flipped(p: Plane) -> Plane {
    Plane {
        normal: scale(p.normal, -1.0),
        distance: -p.distance,
    }
}
pub(super) fn from_points(a: Point, b: Point, c: Point) -> Option<Plane> {
    let mut normal = cross(sub(c, a), sub(b, a));
    (normalize(&mut normal) != 0.0).then(|| Plane {
        normal,
        distance: dot(a, normal),
    })
}
pub(super) fn equal(a: Plane, b: Plane) -> Option<bool> {
    for flip in [false, true] {
        let b = if flip { flipped(b) } else { b };
        if (0..3).all(|i| (a.normal[i] - b.normal[i]).abs() < NORMAL_EPSILON)
            && (a.distance - b.distance).abs() < 0.0235
        {
            return Some(flip);
        }
    }
    None
}
pub(super) fn bounds(points: &[Point]) -> [Point; 2] {
    let mut bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
    for p in points {
        for i in 0..3 {
            bounds[0][i] = bounds[0][i].min(p[i]);
            bounds[1][i] = bounds[1][i].max(p[i]);
        }
    }
    bounds
}
pub(super) fn base_winding(plane: Plane) -> Vec<Point> {
    let mut axis = 0;
    for i in 1..3 {
        if plane.normal[i].abs() > plane.normal[axis].abs() {
            axis = i;
        }
    }
    let mut up = [0.0; 3];
    up[if axis == 2 { 0 } else { 2 }] = 1.0;
    up = add(up, scale(plane.normal, -dot(up, plane.normal)));
    normalize(&mut up);
    let origin = scale(plane.normal, plane.distance);
    let right = scale(cross(up, plane.normal), MAP_BOUNDS);
    up = scale(up, MAP_BOUNDS);
    vec![
        add(sub(origin, right), up),
        add(add(origin, right), up),
        sub(add(origin, right), up),
        sub(sub(origin, right), up),
    ]
}

/// Keep the front half, including axial intersections without roundoff.
pub(super) fn chop(points: &mut Vec<Point>, plane: Plane) {
    let distances: Vec<_> = points.iter().map(|p| plane.signed_distance(*p)).collect();
    let side = |d: f32| {
        if d > 0.1 {
            0
        } else if d < -0.1 {
            1
        } else {
            2
        }
    };
    if !distances.iter().any(|d| side(*d) == 0) {
        points.clear();
        return;
    }
    if !distances.iter().any(|d| side(*d) == 1) {
        return;
    }
    let mut result = Vec::with_capacity(points.len() + 4);
    for i in 0..points.len() {
        let j = (i + 1) % points.len();
        let a = points[i];
        let b = points[j];
        let s = side(distances[i]);
        let next = side(distances[j]);
        if s == 2 {
            result.push(a);
            continue;
        }
        if s == 0 {
            result.push(a);
        }
        if next == 2 || next == s {
            continue;
        }
        let fraction = distances[i] / (distances[i] - distances[j]);
        result.push(std::array::from_fn(|k| {
            if plane.normal[k] == 1.0 {
                plane.distance
            } else if plane.normal[k] == -1.0 {
                -plane.distance
            } else {
                a[k] + fraction * (b[k] - a[k])
            }
        }));
    }
    *points = result;
}
