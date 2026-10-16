// Door tiles of the lamp visibility atlas (`mover_occlusion.rs`): the same biased ray
// parameters as `lamp_visibility_build.wgsl`, traced against the movers alone at their
// current pose. One workgroup layer per queued tile. `publish` then writes the pixel
// rectangle the movers cover into two border texels the filter never reads, so a
// receiver whose filter footprint lies outside it skips the filter
// (`lamp_door_visibility`).
struct LampGrid { origin: vec4<i32>, counts: vec4<u32>, cell: vec4<f32>, offsets: vec4<u32>, selection:vec4<u32> };
@group(0) @binding(0) var<uniform> lamps: LampGrid;
@group(0) @binding(1) var<storage,read> data: array<vec4<u32>>;
@group(0) @binding(2) var distances: texture_storage_2d<r32float,write>;
// Per occluder: rotation (xyzw), origin and blocking (w), model-space lower bound and
// first triangle (w, bit-cast), upper bound and triangle count (w, bit-cast).
@group(0) @binding(3) var<storage,read> occluders: array<vec4<f32>>;
// Per triangle: a corner and its two edges, model space.
@group(0) @binding(4) var<storage,read> triangles: array<vec4<f32>>;
// Per door tile: lamp, first entry of its occluder list, list length.
@group(0) @binding(5) var<storage,read> tiles: array<vec4<u32>>;
@group(0) @binding(6) var<storage,read> lists: array<u32>;
// The tiles queued this frame.
@group(0) @binding(7) var<storage,read> work: array<u32>;
// Per queued tile: lowest x and y, highest x and y of the pixels a mover covers, starting
// at (max, max, 0, 0) (written with the work list).
@group(0) @binding(8) var<storage,read_write> covered: array<atomic<u32>>;
var<workgroup> group_low_x: atomic<u32>;
var<workgroup> group_low_y: atomic<u32>;
var<workgroup> group_high_x: atomic<u32>;
var<workgroup> group_high_y: atomic<u32>;

fn rotate_vector(rotation: vec4<f32>, value: vec3<f32>) -> vec3<f32> {
    let doubled_cross = 2.0*cross(rotation.xyz, value);
    return value + rotation.w*doubled_cross + cross(rotation.xyz, doubled_cross);
}
fn mover_box(origin: vec3<f32>, direction: vec3<f32>, lo: vec3<f32>, hi: vec3<f32>, range: f32) -> bool {
    var near = 0.0; var far = range;
    for (var axis = 0; axis < 3; axis++) {
        if abs(direction[axis]) < 1e-8 {
            if origin[axis] < lo[axis] || origin[axis] > hi[axis] { return false; }
        } else {
            let a = (lo[axis]-origin[axis])/direction[axis];
            let b = (hi[axis]-origin[axis])/direction[axis];
            near = max(near,min(a,b)); far = min(far,max(a,b));
        }
    }
    return near <= far;
}
@compute @workgroup_size(8,8,1) fn trace(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) local: u32) {
    if local == 0u {
        atomicStore(&group_low_x, 0xffffffffu); atomicStore(&group_low_y, 0xffffffffu);
        atomicStore(&group_high_x, 0u); atomicStore(&group_high_y, 0u);
    }
    workgroupBarrier();
    let resolution = lamps.offsets.z;
    let pitch = resolution+2u;
    // Every invocation reaches the barriers below; only those on the tile trace.
    if resolution != 0u && all(id.xy < vec2(pitch)) && trace_texel(id) {
        let pixel = id.xy-vec2(1u);
        // Border texels mirror the interior; only interior pixels are filtered.
        if all(id.xy >= vec2(1u)) && all(pixel < vec2(resolution)) {
            atomicMin(&group_low_x, pixel.x); atomicMin(&group_low_y, pixel.y);
            atomicMax(&group_high_x, pixel.x); atomicMax(&group_high_y, pixel.y);
        }
    }
    workgroupBarrier();
    if local == 0u && atomicLoad(&group_high_x) >= atomicLoad(&group_low_x) {
        let base = id.z*4u;
        atomicMin(&covered[base], atomicLoad(&group_low_x));
        atomicMin(&covered[base+1u], atomicLoad(&group_low_y));
        atomicMax(&covered[base+2u], atomicLoad(&group_high_x));
        atomicMax(&covered[base+3u], atomicLoad(&group_high_y));
    }
}
// One texel of a queued tile; whether a mover is in its direction.
fn trace_texel(id: vec3<u32>) -> bool {
    let resolution = lamps.offsets.z;
    let pitch = resolution+2u;
    let tile = tiles[work[id.z]];
    let slot = lamps.counts.w+work[id.z];
    let at = vec2(slot%lamps.offsets.w, slot/lamps.offsets.w)*pitch+id.xy;
    let pixel = vec2<f32>(id.xy)-1.0;
    let direction = lamp_octa_direction((pixel+0.5)/f32(resolution)*2.0-1.0);
    let source = bitcast<vec4<f32>>(data[tile.x*5u]);
    let normal = bitcast<vec4<f32>>(data[tile.x*5u+1u]).xyz;
    let origin = source.xyz+normal*0.5;
    var nearest = source.w+4.0;
    var hit = false;
    for (var i = 0u; i < tile.z; i++) {
        let o = lists[tile.y+i]*4u;
        let placed = occluders[o+1u];
        if placed.w < 0.5 { continue; }
        // Into the mover's model space: the inverse of `instance_position`.
        let inverse = vec4(-occluders[o].xyz, occluders[o].w);
        let local_origin = rotate_vector(inverse, origin-placed.xyz);
        let local_direction = rotate_vector(inverse, direction);
        let lower = occluders[o+2u];
        let upper = occluders[o+3u];
        if !mover_box(local_origin, local_direction, lower.xyz, upper.xyz, nearest) { continue; }
        let first = bitcast<u32>(lower.w);
        let count = bitcast<u32>(upper.w);
        for (var k = first; k < first+count; k++) {
            let a = triangles[k*3u].xyz;
            let e1 = triangles[k*3u+1u].xyz;
            let e2 = triangles[k*3u+2u].xyz;
            let h = cross(local_direction, e2);
            let determinant = dot(e1, h);
            if abs(determinant) < 1e-8 { continue; }
            let s = local_origin-a;
            let u = dot(s, h)/determinant;
            let q = cross(s, e1);
            let v = dot(local_direction, q)/determinant;
            let distance = dot(e2, q)/determinant;
            if u < -1e-6 || v < -1e-6 || u+v > 1.000001 || distance < 1e-3 || distance > nearest { continue; }
            nearest = distance;
            hit = true;
        }
    }
    let ray_parameter = (nearest+0.5)*dot(abs(direction), vec3(1.0));
    textureStore(distances, at, vec4(select(1e20, ray_parameter, hit)));
    return hit;
}
// The covered rectangle of each traced tile, packed x + 256 y (exact in a float), into
// its top border row: lower corner, upper corner; -1 when no mover is in view.
@compute @workgroup_size(1,1,1) fn publish(@builtin(workgroup_id) id: vec3<u32>) {
    let resolution = lamps.offsets.z;
    if resolution == 0u { return; }
    let pitch = resolution+2u;
    let slot = lamps.counts.w+work[id.x];
    let origin = vec2(slot%lamps.offsets.w, slot/lamps.offsets.w)*pitch;
    let base = id.x*4u;
    let low = vec2(atomicLoad(&covered[base]), atomicLoad(&covered[base+1u]));
    let high = vec2(atomicLoad(&covered[base+2u]), atomicLoad(&covered[base+3u]));
    let any_cover = high.x >= low.x && high.y >= low.y;
    textureStore(distances, origin, vec4(select(-1.0, f32(low.x+low.y*256u), any_cover)));
    textureStore(distances, origin+vec2(1u, 0u), vec4(select(-1.0, f32(high.x+high.y*256u), any_cover)));
}
