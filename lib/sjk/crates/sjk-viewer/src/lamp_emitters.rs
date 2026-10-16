//! Texture-aware quadrature at installation. Flux and its spatial moments become
//! finite rectangular sources; dark housing and gaps do not receive uniform emission.
use super::{Emitter, FLOOR, Lamp, POWER_SCALE};
use glam::{IVec3, Vec2, Vec3};
use std::collections::HashMap;
#[path = "lamp_patch.rs"]
mod patch;
use patch::Patch;
#[path = "lamp_patch_candidates.rs"]
mod candidates;
#[path = "lamp_quadrature.rs"]
mod quadrature;

const EDGE: f32 = 128.;
const MERGE: f32 = 96.;
const TEXTURE_WORK_PER_TRIANGLE: f32 = 8192.;
/// Position, authored normal and base material UV for one mesh corner.
pub(super) type Corner = ([f32; 3], [f32; 3], [f32; 2]);
fn luma(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}
fn position(c: Corner) -> Vec3 {
    Vec3::from_array(c.0)
}
fn uv(c: Corner) -> Vec2 {
    Vec2::from_array(c.2)
}

/// Integrate source textures over placed geometry, without building a lookup grid.
pub(crate) fn collect(vertices: &[Corner], indices: &[u32], emitters: &[Emitter<'_>]) -> Vec<Lamp> {
    let mut geometry_work = 0f64;
    for emitter in emitters {
        for range in &emitter.ranges {
            for t in indices[range.start as usize..range.end as usize].chunks_exact(3) {
                let corners = [t[0], t[1], t[2]].map(|i| vertices[i as usize]);
                let p = corners.map(position);
                geometry_work += f64::from(
                    p[0].distance_squared(p[1])
                        .max(p[1].distance_squared(p[2]))
                        .max(p[2].distance_squared(p[0])),
                );
            }
        }
    }
    let edge = (geometry_work / 131_072.).sqrt().max(EDGE as f64) as f32;
    let triangles: usize = emitters
        .iter()
        .flat_map(|e| &e.ranges)
        .map(|r| (r.end - r.start) as usize / 3)
        .sum();
    // Large low-triangle faces can generate more quadrature patches than a dense
    // mesh. Include their estimated subdivision work when choosing load workers.
    let parallel = triangles >= 64 || geometry_work / f64::from(EDGE * EDGE) >= 64.;
    let default = if parallel {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .saturating_sub(1)
            .clamp(1, 4)
    } else {
        1
    };
    let workers = std::env::var("SJK_LAMP_COOK_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
        .clamp(1, 16);
    let cooked = cook_emitters(vertices, indices, emitters, edge, workers);
    let max_lod = cooked.iter().map(|(_, lod)| *lod).max().unwrap_or(0);
    let result: Vec<_> = cooked.into_iter().flat_map(|(lamps, _)| lamps).collect();
    crate::log::progress(format_args!(
        "Emitting geometry: {} textured area patches, edge {edge:.0}, maximum mask LOD {max_lod}; {triangles} source triangles, {workers} workers",
        result.len()
    ));
    result
}

// Each material's patches are independent. Keep the original material/patch order
// after workers join so lamp IDs, accumulation and shadow selection remain exact.
fn cook_emitters(
    vertices: &[Corner],
    indices: &[u32],
    emitters: &[Emitter<'_>],
    edge: f32,
    workers: usize,
) -> Vec<(Vec<Lamp>, u32)> {
    if workers == 1 || emitters.len() < 2 {
        return emitters
            .iter()
            .map(|e| cook_emitter(vertices, indices, e, edge))
            .collect();
    }
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let next = &next;
        let handles: Vec<_> = (0..workers.min(emitters.len()))
            .map(|_| {
                scope.spawn(move || {
                    let mut results = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(emitter) = emitters.get(index) else {
                            break;
                        };
                        results.push((index, cook_emitter(vertices, indices, emitter, edge)));
                    }
                    results
                })
            })
            .collect();
        let mut results = vec![None; emitters.len()];
        for handle in handles {
            for (index, result) in handle.join().expect("lamp cook worker") {
                results[index] = Some(result);
            }
        }
        results
            .into_iter()
            .map(|r| r.expect("every emitter cooked"))
            .collect()
    })
}

/// A material whose mask cuts it into more patches than this is cooked again from a
/// coarser mask ([`COARSER`]). Following every strip of a tiled strip-light texture made
/// 51,957 patches of one `JKLevel1` material in 63 s, as dark gaps keep patches apart
/// and each sample tests every patch of the crowded cells. Stock maps stay well under
/// it: the most one material of `mp/ffa5` makes is 942, of `JoFTemple` 2,399.
const MASK_PATCH_LIMIT: usize = 4096;
/// A mask level past the last of any emission texture (they are at most 64 texels), so
/// sampling it reads the mean and adds no texture subdivision.
const MEAN_LOD: u32 = 16;
/// Mask levels added on each cook of a material: as authored, then four and sixteen
/// times coarser, then its mean, which has no patch limit.
const COARSER: [u32; 4] = [0, 1, 2, MEAN_LOD];

fn cook_emitter(
    vertices: &[Corner],
    indices: &[u32],
    emitter: &Emitter<'_>,
    edge: f32,
) -> (Vec<Lamp>, u32) {
    let (coarser, cooked) = COARSER
        .into_iter()
        .find_map(|coarser| {
            Some((
                coarser,
                cook_emitter_at(vertices, indices, emitter, edge, coarser)?,
            ))
        })
        .expect("a mean cook has no patch limit");
    if coarser > 0 {
        crate::log::progress(format_args!(
            "Emitting geometry: a material over {MASK_PATCH_LIMIT} patches cooked {}              ({} patches)",
            if coarser >= MEAN_LOD {
                "from its mean".to_owned()
            } else {
                format!("{coarser} mask levels coarser")
            },
            cooked.0.len()
        ));
    }
    cooked
}

/// Cook one material from its mask `coarser` levels down; `None` when that makes more
/// than [`MASK_PATCH_LIMIT`] patches, short of the mean ([`MEAN_LOD`]).
fn cook_emitter_at(
    vertices: &[Corner],
    indices: &[u32],
    emitter: &Emitter<'_>,
    edge: f32,
    coarser: u32,
) -> Option<(Vec<Lamp>, u32)> {
    let mean = coarser >= MEAN_LOD;
    let mut max_lod = 0;
    let radiance = Vec3::from_array(emitter.radiance);
    if !radiance.is_finite() || luma(radiance) <= 1e-4 {
        return Some((Vec::new(), 0));
    }
    let mut patches: Vec<Patch> = Vec::new();
    let mut cells: HashMap<IVec3, Vec<usize>> = HashMap::new();
    let mut pending = Vec::new();
    let mut candidates = candidates::Candidates::default();
    for range in &emitter.ranges {
        for t in indices[range.start as usize..range.end as usize].chunks_exact(3) {
            let corners = [t[0], t[1], t[2]].map(|i| vertices[i as usize]);
            // A huge tiled face must not reduce mask detail on other fixtures.
            let mask_lod = (emitter.texture.span(corners.map(uv))
                / TEXTURE_WORK_PER_TRIANGLE.sqrt())
            .max(1.)
            .log2()
            .ceil() as u32;
            max_lod = max_lod.max(mask_lod);
            let lod = mask_lod.saturating_add(coarser);
            let texel_step = 2f32.powi(lod as i32);
            pending.push((corners, 0u32));
            while let Some((corners, depth)) = pending.pop() {
                let p = corners.map(position);
                let coords = corners.map(uv);
                if p.iter().any(|p| !p.is_finite()) || coords.iter().any(|p| !p.is_finite()) {
                    continue;
                }
                let metric = std::array::from_fn::<_, 3, _>(|a| {
                    let b = (a + 1) % 3;
                    (p[a].distance_squared(p[b]) / (edge * edge)).max(
                        (emitter.texture.span([coords[a], coords[b], coords[b]]) / texel_step)
                            .powi(2),
                    )
                });
                let longest = (0..3)
                    .max_by(|&a, &b| metric[a].total_cmp(&metric[b]))
                    .unwrap();
                if metric[longest] > 1. && depth < 24 {
                    let a = longest;
                    let b = (a + 1) % 3;
                    let c = (a + 2) % 3;
                    let mid = (
                        ((p[a] + p[b]) * 0.5).to_array(),
                        ((Vec3::from_array(corners[a].1) + Vec3::from_array(corners[b].1)) * 0.5)
                            .to_array(),
                        ((coords[a] + coords[b]) * 0.5).to_array(),
                    );
                    pending.push(([mid, corners[b], corners[c]], depth + 1));
                    pending.push(([corners[a], mid, corners[c]], depth + 1));
                    continue;
                }
                let Some(sample) = quadrature::sample(corners, emitter, lod) else {
                    continue;
                };
                let cell = (sample.position / MERGE).floor().as_ivec3();
                let nearby = candidates.get(&cells, cell, patches.len());
                let found = find_patch(&patches, nearby, &sample, emitter, lod);
                let id = found.unwrap_or_else(|| {
                    let id = patches.len();
                    cells.entry(cell).or_default().push(id);
                    patches.push(Patch::new(
                        sample.position,
                        sample.uv,
                        sample.normal,
                        sample.luminance,
                        sample.gradient,
                    ));
                    id
                });
                patches[id].add(sample.points, sample.colors, sample.area);
                if !mean && patches.len() > MASK_PATCH_LIMIT {
                    return None;
                }
            }
        }
    }
    let mut result: Vec<Lamp> = patches
        .into_iter()
        .map(|patch| {
            let mut lamp = patch.finish();
            if emitter.omnidirectional {
                lamp.normal = Vec3::ZERO;
            }
            lamp
        })
        .collect();
    if emitter.shared_reach {
        // Cutting a fixture into texture patches must not shorten its useful reach:
        // the individual faint pieces can still add up to visible room illumination.
        let power: f32 = result.iter().map(|lamp| lamp.power).sum();
        let reach = (power / FLOOR).sqrt().clamp(32., 1024.);
        for lamp in &mut result {
            lamp.radius = reach + lamp.axis_u.length().max(lamp.axis_v.length());
        }
    }
    Some((result, max_lod))
}

fn find_patch(
    patches: &[Patch],
    candidates: &[usize],
    sample: &quadrature::Sample,
    emitter: &Emitter<'_>,
    lod: u32,
) -> Option<usize> {
    let point = sample.position;
    let normal = sample.normal;
    let uv = sample.uv;
    let luminance = sample.luminance;
    let weight = sample.weight;
    for &id in candidates {
        let patch = &patches[id];
        let offset = point - patch.anchor;
        if offset.length_squared() >= MERGE * MERGE
            || normal.dot(patch.normal) <= 0.999
            || offset.dot(normal).abs() >= 0.5
            || patch.texcoord(point).distance_squared(uv) > 1e-6
        {
            continue;
        }
        // Preserve the original texture-gap tests and their floating-point order.
        let floor = luminance.min(patch.luminance) * 0.25;
        let start = patch.mean_uv();
        if [
            start.lerp(uv, 0.25),
            start.lerp(uv, 0.5),
            start.lerp(uv, 0.75),
            patch.merged_uv(uv, weight),
        ]
        .into_iter()
        .all(|p| luma(Vec3::from_array(emitter.radiance) * emitter.texture.sample(p, lod)) >= floor)
        {
            return Some(id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_material_ends_in_a_cook_without_a_patch_limit() {
        assert_eq!(COARSER.first(), Some(&0), "the authored mask comes first");
        assert!(COARSER.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(COARSER.last(), Some(&MEAN_LOD));
    }
}
