//! `tri_tri_intersect` (`codemp/game/tri_coll_test.c`), Tomas Möller's triangle-triangle
//! test as the game ships it, which `WP_SabersIntersect` asks whether two blades' swept
//! triangles meet.
//!
//! Ported as it is, in float, including the game's change to the last line: the
//! reference answers **true when the two intervals on the planes' line do not overlap**
//! (Möller's original answers false there). Blades "meet" when each triangle's plane
//! crosses the other triangle but the crossings are apart. Coplanar triangles take
//! Möller's 2D edge and containment tests unchanged.

type Vec3 = [f32; 3];

const EPSILON: f64 = 0.000_001;

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `fabs(x) < EPSILON`, a double comparison, zeroes a distance.
fn snap(value: f32) -> f32 {
    if f64::from(value).abs() < EPSILON {
        0.0
    } else {
        value
    }
}

/// The `ISECT` macro.
fn isect(vv0: f32, vv1: f32, vv2: f32, d0: f32, d1: f32, d2: f32) -> [f32; 2] {
    [
        vv0 + (vv1 - vv0) * d0 / (d0 - d1),
        vv0 + (vv2 - vv0) * d0 / (d0 - d2),
    ]
}

/// `COMPUTE_INTERVALS`: the interval on the line, or `None` for coplanar triangles.
fn interval(vv: [f32; 3], d: [f32; 3], d0d1: f32, d0d2: f32) -> Option<[f32; 2]> {
    let ([v0, v1, v2], [d0, d1, d2]) = (vv, d);
    if d0d1 > 0.0 {
        Some(isect(v2, v0, v1, d2, d0, d1))
    } else if d0d2 > 0.0 {
        Some(isect(v1, v0, v2, d1, d0, d2))
    } else if d1 * d2 > 0.0 || d0 != 0.0 {
        Some(isect(v0, v1, v2, d0, d1, d2))
    } else if d1 != 0.0 {
        Some(isect(v1, v0, v2, d1, d0, d2))
    } else if d2 != 0.0 {
        Some(isect(v2, v0, v1, d2, d0, d1))
    } else {
        None
    }
}

/// `tri_tri_intersect(V0, V1, V2, U0, U1, U2)`.
pub fn tri_tri_intersect(v: [Vec3; 3], u: [Vec3; 3]) -> bool {
    let n1 = cross(sub(v[1], v[0]), sub(v[2], v[0]));
    let d1 = -dot(n1, v[0]);
    let du = u.map(|point| snap(dot(n1, point) + d1));
    let (du0du1, du0du2) = (du[0] * du[1], du[0] * du[2]);
    if du0du1 > 0.0 && du0du2 > 0.0 {
        return false;
    }
    let n2 = cross(sub(u[1], u[0]), sub(u[2], u[0]));
    let d2 = -dot(n2, u[0]);
    let dv = v.map(|point| snap(dot(n2, point) + d2));
    let (dv0dv1, dv0dv2) = (dv[0] * dv[1], dv[0] * dv[2]);
    if dv0dv1 > 0.0 && dv0dv2 > 0.0 {
        return false;
    }
    let direction = cross(n1, n2);
    let mut index = 0;
    let mut max = direction[0].abs();
    if direction[1].abs() > max {
        max = direction[1].abs();
        index = 1;
    }
    if direction[2].abs() > max {
        index = 2;
    }
    let Some(mut first) = interval(v.map(|point| point[index]), dv, dv0dv1, dv0dv2) else {
        return coplanar(n1, v, u);
    };
    let Some(mut second) = interval(u.map(|point| point[index]), du, du0du1, du0du2) else {
        return coplanar(n1, v, u);
    };
    if first[0] > first[1] {
        first.swap(0, 1);
    }
    if second[0] > second[1] {
        second.swap(0, 1);
    }
    // The game's line: true where Möller's original says the triangles miss.
    first[1] < second[0] || second[1] < first[0]
}

/// `coplanar_tri_tri`.
fn coplanar(n: Vec3, v: [Vec3; 3], u: [Vec3; 3]) -> bool {
    let a = n.map(f32::abs);
    let (i0, i1) = if a[0] > a[1] {
        if a[0] > a[2] { (1, 2) } else { (0, 1) }
    } else if a[2] > a[1] {
        (0, 1)
    } else {
        (0, 2)
    };
    for (start, end) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
        if edge_against_edges(start, end, u, i0, i1) {
            return true;
        }
    }
    point_in_triangle(v[0], u, i0, i1) || point_in_triangle(u[0], v, i0, i1)
}

/// `EDGE_AGAINST_TRI_EDGES`: the edge from `v0` to `v1` against each of `u`'s.
fn edge_against_edges(v0: Vec3, v1: Vec3, u: [Vec3; 3], i0: usize, i1: usize) -> bool {
    let (ax, ay) = (v1[i0] - v0[i0], v1[i1] - v0[i1]);
    [(u[0], u[1]), (u[1], u[2]), (u[2], u[0])]
        .into_iter()
        .any(|(u0, u1)| {
            // `EDGE_EDGE_TEST`.
            let (bx, by) = (u0[i0] - u1[i0], u0[i1] - u1[i1]);
            let (cx, cy) = (v0[i0] - u0[i0], v0[i1] - u0[i1]);
            let f = ay * bx - ax * by;
            let d = by * cx - bx * cy;
            if (f > 0.0 && d >= 0.0 && d <= f) || (f < 0.0 && d <= 0.0 && d >= f) {
                let e = ax * cy - ay * cx;
                if f > 0.0 {
                    e >= 0.0 && e <= f
                } else {
                    e <= 0.0 && e >= f
                }
            } else {
                false
            }
        })
}

/// `POINT_IN_TRI`: whether `point` is inside the triangle `u`, projected.
fn point_in_triangle(point: Vec3, u: [Vec3; 3], i0: usize, i1: usize) -> bool {
    let side = |from: Vec3, to: Vec3| {
        let a = to[i1] - from[i1];
        let b = -(to[i0] - from[i0]);
        let c = -a * from[i0] - b * from[i1];
        a * point[i0] + b * point[i1] + c
    };
    let (d0, d1, d2) = (side(u[0], u[1]), side(u[1], u[2]), side(u[2], u[0]));
    d0 * d1 > 0.0 && d0 * d2 > 0.0
}
