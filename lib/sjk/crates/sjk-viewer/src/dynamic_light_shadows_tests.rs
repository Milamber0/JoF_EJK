//! The tracer's program and dispatch, and the shadow tiles' behaviour on small scenes.
//!
//! No GPU runs here, so the trace (`dynamic_light_shadows.wgsl`) and the receivers' test
//! (`point_light_visibility` in `point_lights.wgsl`, mapping in `point_light_octa.wgsl`)
//! are followed line by line in Rust below, against the same kind of triangles the tracer
//! sees, to pin what the programs compute: walls and corners stop a light, while the
//! surfaces it reaches, at any angle, in creases and on steps, stay as lit as before.
use super::*;
use glam::{Vec2, Vec3};

#[test]
fn the_tracer_program_validates() {
    crate::wgsl_source::validate(&source());
}

#[test]
fn every_light_gets_a_tile_and_the_copy_ends_at_the_block() {
    assert_eq!(planned(0, true), 0);
    assert_eq!(planned(5, true), 5);
    assert_eq!(planned(5, false), 0);
    assert_eq!(planned(MAX_POINT_LIGHTS + 3, true), MAX_POINT_LIGHTS as u32);
    // 392 words a tile, one invocation each.
    assert_eq!(workgroups(7), [7, 7, 1]);
    assert_eq!(copy_bytes(1), 16 + 1568);
    assert_eq!(
        SHADOW_HEADER_OFFSET + copy_bytes(MAX_POINT_LIGHTS as u32),
        crate::dynamic_lights::BLOCK_BYTES
    );
    assert_eq!(SHADOW_HEADER_OFFSET % wgpu::COPY_BUFFER_ALIGNMENT, 0);
    assert_eq!(copy_bytes(3) % wgpu::COPY_BUFFER_ALIGNMENT, 0);
}

const EDGE: i32 = crate::dynamic_lights::SHADOW_TILE_EDGE as i32;
const START: f32 = 1.0;

/// `select(vec2(-1.0), vec2(1.0), v >= vec2(0.0))`.
fn sign(v: Vec2) -> Vec2 {
    Vec2::new(
        if v.x >= 0.0 { 1.0 } else { -1.0 },
        if v.y >= 0.0 { 1.0 } else { -1.0 },
    )
}

/// `point_light_octa_vector`.
fn octa_vector(p: Vec2) -> Vec3 {
    let n = Vec3::new(p.x, p.y, 1.0 - p.x.abs() - p.y.abs());
    if n.z < 0.0 {
        ((Vec2::ONE - Vec2::new(n.y.abs(), n.x.abs())) * sign(n.truncate())).extend(n.z)
    } else {
        n
    }
}

/// `point_light_octa_square`.
fn octa_square(direction: Vec3) -> Vec2 {
    let n = direction / (direction.x.abs() + direction.y.abs() + direction.z.abs());
    if n.z < 0.0 {
        (Vec2::ONE - Vec2::new(n.y.abs(), n.x.abs())) * sign(n.truncate())
    } else {
        n.truncate()
    }
}

/// `point_light_texel_direction`.
fn texel_direction(pixel: [i32; 2]) -> Vec3 {
    let p = (Vec2::new(pixel[0] as f32, pixel[1] as f32) + 0.5) / EDGE as f32 * 2.0 - 1.0;
    octa_vector(p).normalize()
}

/// `point_light_tile_position`.
fn tile_position(direction: Vec3) -> Vec2 {
    (octa_square(direction) * 0.5 + 0.5) * EDGE as f32
}

/// `point_light_tile_pixel`.
fn tile_pixel(pixel: [i32; 2]) -> [i32; 2] {
    let last = EDGE - 1;
    let mut p = pixel;
    if p[0] < 0 || p[0] > last {
        p = [p[0].clamp(0, last), last - p[1]];
    }
    if p[1] < 0 || p[1] > last {
        p = [last - p[0], p[1].clamp(0, last)];
    }
    p
}

/// `point_light_normal_code`.
fn normal_code(normal: Vec3) -> u16 {
    let steps = ((octa_square(normal) * 0.5 + 0.5) * 14.0).round();
    (steps.x as u16).min(14) | ((steps.y as u16).min(14) << 4)
}

/// `point_light_code_normal`.
fn code_normal(code: u16) -> Vec3 {
    let p = Vec2::new(f32::from(code & 15), f32::from((code >> 4) & 15)) / 7.0 - 1.0;
    octa_vector(p).normalize()
}

/// `point_light_reach`.
fn reach(radius: f32) -> f32 {
    radius.max(0.0) + 8.0
}

/// The nearest triangle along the ray within `range` and its normal facing the ray, as
/// `gi_trace_through` finds them.
fn first_hit(
    triangles: &[[Vec3; 3]],
    origin: Vec3,
    direction: Vec3,
    range: f32,
) -> Option<(f32, Vec3)> {
    let mut nearest: Option<(f32, Vec3)> = None;
    for [a, b, c] in triangles {
        let (e1, e2) = (*b - *a, *c - *a);
        let h = direction.cross(e2);
        let determinant = e1.dot(h);
        if determinant.abs() < 1e-8 {
            continue;
        }
        let s = origin - *a;
        let u = s.dot(h) / determinant;
        let q = s.cross(e1);
        let v = direction.dot(q) / determinant;
        let distance = e2.dot(q) / determinant;
        if u < -1e-6
            || v < -1e-6
            || u + v > 1.000001
            || distance < 1e-3
            || distance > nearest.map_or(range, |(d, _)| d)
        {
            continue;
        }
        let normal = e1.cross(e2).normalize();
        let facing = if normal.dot(direction) > 0.0 {
            -normal
        } else {
            normal
        };
        nearest = Some((distance, facing));
    }
    nearest
}

/// One light's tile as `trace` writes it: `point_light_trace_texel` for every texel.
fn trace_tile(triangles: &[[Vec3; 3]], light: Vec3, radius: f32) -> Vec<u16> {
    let reach = reach(radius);
    (0..EDGE * EDGE)
        .map(|texel| {
            let direction = texel_direction([texel % EDGE, texel / EDGE]);
            first_hit(
                triangles,
                light + direction * START,
                direction,
                reach - START,
            )
            .map_or(255, |(distance, normal)| {
                let level = (((distance + START) / reach).max(0.0) * 255.0).min(254.0) as u16;
                level | (normal_code(normal) << 8)
            })
        })
        .collect()
}

/// `point_light_visibility` for a light with a tile.
fn visibility(tile: &[u16], light: Vec3, radius: f32, world: Vec3, normal: Vec3) -> f32 {
    let delta = world - light;
    let distance = delta.length();
    let reach = reach(radius);
    if distance < 1e-3 || distance >= reach {
        return 1.0;
    }
    let surface = normal.length_squared() > 1e-12;
    let unit = normal / normal.length_squared().max(1e-24).sqrt();
    let plane = delta.dot(unit);
    let oriented = unit * if plane >= 0.0 { 1.0 } else { -1.0 };
    let plane_distance = plane.abs();
    let rounding = reach / 255.0 + 0.5;
    let at = tile_position(delta) - 0.5;
    let base = at.floor();
    let blend = at - base;
    let (mut clear, mut beside, mut behind) = (0.0, 0.0, 0.0);
    for tap in 0..4 {
        let corner = [tap & 1, tap >> 1];
        let pixel = tile_pixel([base.x as i32 + corner[0], base.y as i32 + corner[1]]);
        let ray = texel_direction(pixel);
        let denominator = ray.dot(oriented);
        if surface && denominator < 1e-4 {
            continue;
        }
        let share = |far: i32, blend: f32| if far == 1 { blend } else { 1.0 - blend };
        let weight = share(corner[0], blend.x) * share(corner[1], blend.y);
        let texel = tile[(pixel[1] * EDGE + pixel[0]) as usize];
        let level = texel & 255;
        let free = (f32::from(level) + 1.0) * reach / 255.0 + 0.5;
        if level == 255
            || free >= distance
            || (surface && free * denominator - plane_distance > rounding * denominator)
        {
            clear += weight;
            continue;
        }
        if (delta - ray * free).dot(code_normal(texel >> 8)) >= -(1.0 + rounding) {
            beside += weight;
        } else {
            behind += weight;
        }
    }
    let total = clear + beside + behind;
    if total < 1e-6 {
        return 1.0;
    }
    (clear + beside * (1.0 - 2.0 * behind / total).max(0.0)) / total
}

/// The surface law of `dynamic_light_modulation`: radial falloff times facing.
fn light_law(light: Vec3, radius: f32, world: Vec3, normal: Vec3) -> f32 {
    let delta = light - world;
    let distance = delta.length();
    (1.0 - distance / radius).max(0.0) * (delta.dot(normal) / distance).max(0.0)
}

/// The twelve triangles of the box from `low` to `high`.
fn solid(low: Vec3, high: Vec3) -> Vec<[Vec3; 3]> {
    let corner = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { low.x } else { high.x },
            if i & 2 == 0 { low.y } else { high.y },
            if i & 4 == 0 { low.z } else { high.z },
        )
    };
    [
        [0, 2, 6, 4],
        [1, 3, 7, 5],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [0, 1, 3, 2],
        [4, 5, 7, 6],
    ]
    .iter()
    .flat_map(|[a, b, c, d]| {
        [
            [corner(*a), corner(*b), corner(*c)],
            [corner(*a), corner(*c), corner(*d)],
        ]
    })
    .collect()
}

/// A floor, its top at z 0.
fn floor() -> Vec<[Vec3; 3]> {
    solid(Vec3::new(-400., -400., -16.), Vec3::new(400., 400., 0.))
}

/// The floor and, from x 0 to 16, a wall across it.
fn floor_and_wall() -> Vec<[Vec3; 3]> {
    let mut triangles = floor();
    triangles.extend(solid(Vec3::new(0., -400., 0.), Vec3::new(16., 400., 200.)));
    triangles
}

/// The floor and a block filling x > 0, y > 0.
fn floor_and_corner() -> Vec<[Vec3; 3]> {
    let mut triangles = floor();
    triangles.extend(solid(Vec3::ZERO, Vec3::new(300., 300., 200.)));
    triangles
}

fn grid(x: std::ops::Range<i32>, y: std::ops::Range<i32>, step: usize) -> Vec<(f32, f32)> {
    x.step_by(step)
        .flat_map(|x| y.clone().step_by(step).map(move |y| (x as f32, y as f32)))
        .collect()
}

#[test]
fn texel_directions_map_back_to_their_texels_and_edges_join_neighbours() {
    for texel in 0..EDGE * EDGE {
        let pixel = [texel % EDGE, texel / EDGE];
        let at = tile_position(texel_direction(pixel));
        assert!(
            (at - Vec2::new(pixel[0] as f32 + 0.5, pixel[1] as f32 + 0.5)).length() < 1e-3,
            "{pixel:?} came back at {at}"
        );
    }
    // A tap one texel past any edge or corner reads a texel about a texel's angle away.
    let texel = std::f32::consts::FRAC_PI_2 / (EDGE as f32 / 2.0);
    for i in -1..=EDGE {
        for (inside, outside) in [
            ([0, i.clamp(0, EDGE - 1)], [-1, i]),
            ([EDGE - 1, i.clamp(0, EDGE - 1)], [EDGE, i]),
            ([i.clamp(0, EDGE - 1), 0], [i, -1]),
            ([i.clamp(0, EDGE - 1), EDGE - 1], [i, EDGE]),
        ] {
            let read = tile_pixel(outside);
            assert!(
                read.iter().all(|v| (0..EDGE).contains(v)),
                "{outside:?} read {read:?}"
            );
            let angle = texel_direction(inside).angle_between(texel_direction(read));
            assert!(angle < 1.6 * texel, "{inside:?} to {outside:?}: {angle}");
        }
    }
}

#[test]
fn normal_codes_keep_the_axes_exact_and_the_rest_within_a_step() {
    for axis in [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z] {
        assert!(
            code_normal(normal_code(axis)).distance(axis) < 1e-6,
            "{axis}"
        );
    }
    for texel in 0..EDGE * EDGE {
        let normal = texel_direction([texel % EDGE, texel / EDGE]);
        let angle = code_normal(normal_code(normal)).angle_between(normal);
        assert!(angle < 0.2, "{normal}: {angle}");
    }
}

#[test]
fn surfaces_the_light_reaches_stay_fully_lit() {
    let triangles = floor_and_wall();
    let light = Vec3::new(-30., 0., 40.);
    let tile = trace_tile(&triangles, light, 200.);
    // The floor on the light's side, out to its reach and right into the crease at the
    // wall's foot, at every angle down to grazing.
    for (x, y) in grid(-200..0, -200..201, 6)
        .into_iter()
        .chain(grid(-6..0, -150..151, 1))
    {
        let world = Vec3::new(x, y, 0.);
        let seen = visibility(&tile, light, 200., world, Vec3::Z);
        assert!(seen > 0.999, "floor at {world}: {seen}");
    }
    // The wall's lit face, down to the crease.
    for (y, z) in grid(-180..181, 0..181, 4)
        .into_iter()
        .chain(grid(-120..121, 0..6, 1))
    {
        let world = Vec3::new(0., y, z.max(0.25));
        let seen = visibility(&tile, light, 200., world, -Vec3::X);
        assert!(seen > 0.999, "wall at {world}: {seen}");
    }
    // A model's pixels in the open, even by the wall and just above the floor.
    for (x, y) in grid(-150..0, -100..101, 10) {
        for z in [1., 30., 120.] {
            let world = Vec3::new(x.min(-1.), y, z);
            let seen = visibility(&tile, light, 200., world, Vec3::ZERO);
            assert!(seen > 0.999, "model at {world}: {seen}");
        }
    }
}

#[test]
fn steps_and_their_edges_stay_lit() {
    // Stairs up along +x (8-unit risers, 16-unit treads) lit from in front and above:
    // a step's edge is right beside the next tread in the light's view.
    let mut triangles = floor();
    for step in 0..8 {
        let x = 40. + 16. * step as f32;
        triangles.extend(solid(
            Vec3::new(x, -64., 0.),
            Vec3::new(400., 64., 8. * (step + 1) as f32),
        ));
    }
    let light = Vec3::new(0., 0., 48.);
    let tile = trace_tile(&triangles, light, 200.);
    for step in 0..6 {
        let front = 40. + 16. * step as f32;
        let height = 8. * (step + 1) as f32;
        for x in (0..16).step_by(2) {
            let world = Vec3::new(front + x as f32 + 0.5, 0., height);
            let seen = visibility(&tile, light, 200., world, Vec3::Z);
            assert!(seen > 0.999, "tread at {world}: {seen}");
        }
        // The next riser, which faces the light from below its height (higher ones are
        // lit at a grazing angle past the step's edge, finer than a texel).
        if step < 4 {
            for z in (0..8).step_by(2) {
                let world = Vec3::new(front + 16., 0., height + z as f32 + 0.5);
                let seen = visibility(&tile, light, 200., world, -Vec3::X);
                assert!(seen > 0.999, "riser at {world}: {seen}");
            }
        }
    }
}

#[test]
fn a_wall_stops_the_light_from_reaching_the_room_behind_it() {
    let triangles = floor_and_wall();
    for (light, radius, edge) in [
        // A saber held by the wall and a bolt about to strike it: the room is dark.
        (Vec3::new(-30., 0., 40.), 200., 0.01),
        (Vec3::new(-3., 20., 24.), 120., 0.01),
        // An explosion further off: the floor behind the wall, seen past its foot almost
        // edge-on, keeps a trace within a texel of the foot.
        (Vec3::new(-90., -40., 60.), 300., 0.25),
    ] {
        let tile = trace_tile(&triangles, light, radius);
        let (mut before, mut after, mut dark, mut count) = (0.0, 0.0, 0, 0);
        for (x, y) in grid(20..300, -250..251, 5) {
            let world = Vec3::new(x, y, 0.);
            if world.distance(light) >= reach(radius) - 1. {
                continue;
            }
            let seen = visibility(&tile, light, radius, world, Vec3::Z);
            let law = light_law(light, radius, world, Vec3::Z);
            assert!(seen < edge, "floor behind the wall at {world}: {seen}");
            // At most 3% of the light's full strength (before: all of `law`).
            assert!(
                seen * law < 0.03,
                "floor behind the wall at {world}: {seen} of {law}"
            );
            before += law;
            after += seen * law;
            dark += usize::from(seen < 0.001);
            count += 1;
            let model = visibility(&tile, light, radius, world + Vec3::Z * 30., Vec3::ZERO);
            assert!(model < edge, "model behind the wall at {world}: {model}");
        }
        assert!(count > 100, "{count} receivers in reach");
        assert!(after < before * 0.003, "{after} of {before} left");
        assert!(dark * 100 > count * 97, "{dark} of {count} dark");
    }
}

#[test]
fn a_corner_stops_the_light_from_wrapping_round_it() {
    // The light is west of the block's corner, so the floor south of the block is lit
    // only past the line through the light and the corner (y < -x).
    let light = Vec3::new(-60., 60., 40.);
    let tile = trace_tile(&floor_and_corner(), light, 250.);
    let (mut dark, mut lit) = (0, 0);
    for (x, y) in grid(4..240, -240..0, 4) {
        let world = Vec3::new(x, y, 0.);
        let distance = world.distance(light);
        if distance >= reach(250.) - 1. {
            continue;
        }
        let seen = visibility(&tile, light, 250., world, Vec3::Z);
        // The soft edge: a tenth of the distance into the shadow, a fifth into the light.
        let past = (x + y) / std::f32::consts::SQRT_2;
        if past > 4. + 0.1 * distance {
            assert!(seen < 0.01, "round the corner at {world}: {seen}");
            dark += 1;
        } else if past < -(4. + 0.2 * distance) {
            assert!(seen > 0.99, "in sight past the corner at {world}: {seen}");
            lit += 1;
        }
    }
    assert!(dark > 400 && lit > 300, "{dark} dark, {lit} lit");
}

#[test]
fn the_shadow_edge_is_soft_and_ordered() {
    // Across the corner's shadow edge the light only fades out, over several units.
    let light = Vec3::new(-60., 60., 40.);
    let tile = trace_tile(&floor_and_corner(), light, 250.);
    let mut last = 1.0;
    let mut between = 0;
    for step in 0..=120 {
        // From in sight (y < -x) to round the corner, at x 80.
        let world = Vec3::new(80., -140. + step as f32, 0.);
        let seen = visibility(&tile, light, 250., world, Vec3::Z);
        assert!(
            seen <= last + 1e-4,
            "brighter again at {world}: {seen} after {last}"
        );
        if seen > 0.01 && seen < 0.99 {
            between += 1;
        }
        last = seen;
    }
    assert!(last < 0.001);
    assert!(between >= 10, "{between} units of soft edge");
}

#[test]
fn a_light_just_behind_the_face_it_was_placed_on_lights_its_side() {
    // An impact's flash a quarter unit inside the wall it hit still lights the floor in
    // front of the wall, and nothing behind it.
    let triangles = floor_and_wall();
    let light = Vec3::new(0.25, 0., 40.);
    let tile = trace_tile(&triangles, light, 150.);
    for (x, y) in grid(-120..-30, -60..61, 10) {
        let world = Vec3::new(x, y, 0.);
        let seen = visibility(&tile, light, 150., world, Vec3::Z);
        assert!(seen > 0.999, "floor in front at {world}: {seen}");
    }
    for (x, y) in grid(30..120, -60..61, 10) {
        let world = Vec3::new(x, y, 0.);
        let seen = visibility(&tile, light, 150., world, Vec3::Z);
        assert!(seen < 0.001, "floor behind at {world}: {seen}");
    }
}
