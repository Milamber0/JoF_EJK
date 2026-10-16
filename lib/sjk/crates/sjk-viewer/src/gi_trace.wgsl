// Voxel world bindings and a branchless-enough DDA through the fine occupancy bits.
// Shared by probe tracing and the evidence view; group index is fixed by the caller's
// pipeline layout (group 0 here).
struct GiHeader { origin: vec4<f32>, fine: vec4<u32>, coarse: vec4<u32>, sizes: vec4<f32> };
struct GiSurface { albedo: vec4<f32>, emission: vec4<f32> };
@group(0) @binding(0) var<uniform> gi: GiHeader;
@group(0) @binding(1) var<storage, read> gi_occupancy: array<u32>;
@group(0) @binding(2) var<storage, read> gi_materials: array<u32>;
@group(0) @binding(3) var<storage, read> gi_surfaces: array<GiSurface>;

fn gi_occupied(cell: vec3<i32>) -> bool {
    if any(cell < vec3(0)) || any(cell >= vec3<i32>(gi.fine.xyz)) { return false; }
    let index = u32(cell.x) + (u32(cell.y) + u32(cell.z)*gi.fine.y)*gi.fine.x;
    return (gi_occupancy[index/32u] & (1u << (index%32u))) != 0u;
}
// Surface table entry of the coarse cell containing `point`; index 0 when empty.
fn gi_surface_at(point: vec3<f32>) -> GiSurface {
    let c = vec3<i32>(floor((point-gi.origin.xyz)/gi.sizes.y));
    if any(c < vec3(0)) || any(c >= vec3<i32>(gi.coarse.xyz)) { return gi_surfaces[0]; }
    let index = u32(c.x) + (u32(c.y) + u32(c.z)*gi.coarse.y)*gi.coarse.x;
    let word = gi_materials[index/2u];
    let id = (word >> (16u*(index%2u))) & 0xffffu;
    if id == 0u { return gi_surfaces[0]; }
    return gi_surfaces[min(id-1u, u32(gi.sizes.z)-1u)];
}
struct GiHit { distance: f32, normal: vec3<f32>, hit: bool };
// Whether a fine cell belongs to a sky surface: sky is a boundary for radiance, never an
// occluder, so visibility traces pass through it.
fn gi_sky_cell(cell: vec3<i32>) -> bool {
    let center = gi.origin.xyz + (vec3<f32>(cell)+0.5)*gi.sizes.x;
    return gi_surface_at(center).albedo.w > 0.5;
}
// Amanatides-Woo traversal from `origin` along unit `direction`, at most `range` units.
fn gi_trace(origin: vec3<f32>, direction: vec3<f32>, range: f32) -> GiHit {
    return gi_trace_through(origin, direction, range, false);
}
// As `gi_trace`, optionally treating sky voxels as empty.
fn gi_trace_through(origin: vec3<f32>, direction: vec3<f32>, range: f32,
    through_sky: bool) -> GiHit {
    let size = gi.sizes.x;
    let local = (origin-gi.origin.xyz)/size;
    var cell = vec3<i32>(floor(local));
    let step = vec3<i32>(sign(direction));
    let inv = 1.0/max(abs(direction), vec3(1e-6));
    // Forward distance is nonnegative on either side of a cell. Using a signed
    // coordinate difference with abs(direction) sends negative rays behind the origin.
    var next = select(local-floor(local), floor(local)+1.0-local, direction > vec3(0.0))
        * inv * size;
    next = select(next, vec3(1e30), abs(direction) < vec3(1e-6));
    let delta = inv*size;
    var travelled = 0.0;
    var normal = vec3(0.0);
    for (var i = 0; i < 4096; i++) {
        if travelled > range { break; }
        if gi_occupied(cell) && !(through_sky && gi_sky_cell(cell)) {
            return GiHit(travelled, normal, true);
        }
        if next.x < next.y && next.x < next.z {
            travelled = next.x; next.x += delta.x; cell.x += step.x;
            normal = vec3(-f32(step.x), 0.0, 0.0);
        } else if next.y < next.z {
            travelled = next.y; next.y += delta.y; cell.y += step.y;
            normal = vec3(0.0, -f32(step.y), 0.0);
        } else {
            travelled = next.z; next.z += delta.z; cell.z += step.z;
            normal = vec3(0.0, 0.0, -f32(step.z));
        }
        if any(cell < vec3(0)) || any(cell >= vec3<i32>(gi.fine.xyz)) { break; }
    }
    return GiHit(range, normal, false);
}
