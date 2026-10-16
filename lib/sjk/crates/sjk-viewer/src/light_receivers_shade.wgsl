@group(2) @binding(0) var receiver_world: texture_2d<f32>;
@group(2) @binding(1) var receiver_normal: texture_2d<f32>;
@vertex fn receiver_vertex(@builtin(vertex_index) id: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0));
    return vec4(p[id], 0.0, 1.0);
}
// Only texels the pre-pass covered hold a receiver: the attribute pass writes exactly
// those (depth-equal) and leaves the rest of its targets uncleared.
fn receiver_texel(pixel: vec2<i32>) -> bool {
    return textureLoad(light_depth, pixel, 0) < 1.0;
}
@fragment fn receiver_light(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    if !receiver_texel(pixel) { return vec4(0.0, 0.0, 0.0, 1.0); }
    let world = textureLoad(receiver_world, pixel, 0);
    let normal = textureLoad(receiver_normal, pixel, 0);
    return realtime_light_from_visibility(world.xyz, normal.xyz, vec2(world.w, normal.w));
}
// The same light with lamps taken from the static cache wherever it is valid.
@group(3) @binding(0) var receiver_cache: texture_2d<f32>;
@group(3) @binding(1) var lamp_cache: texture_2d_array<f32>;
@group(3) @binding(2) var lamp_cache_sampler: sampler;
// Receivers the cache cannot serve (`receivers::Direct`): the indirect dispatch size of
// `direct_lamps`, then the count and packed pixels. Each workgroup lights
// DIRECT_RECEIVERS receivers, DIRECT_LANES lanes walking each one's lamp list.
const DIRECT_LANES: u32 = 4u;
const DIRECT_RECEIVERS: u32 = 64u/DIRECT_LANES;
// A bounded two-dimensional dispatch also covers high-resolution uncached views
// without exceeding the device's workgroup limit along the x axis.
const DIRECT_GROUPS_PER_ROW: u32 = 256u;
struct DirectList { groups: array<atomic<u32>, 3>, count: atomic<u32>, pixels: array<u32> };
@group(3) @binding(3) var<storage, read_write> direct_list: DirectList;
// The cached lamp light at a receiver texel (rgb) and its cache layer + 1 (a), or a
// negative alpha when the cache cannot serve it: the texel is then listed for
// `direct_lamps`, which evaluates every lamp and writes it.
fn cached_lamps(pixel: vec2<i32>, at: vec4<f32>) -> vec4<f32> {
    // All four texels of the bilinear footprint must be valid; otherwise evaluate.
    let cached = textureSampleLevel(lamp_cache, lamp_cache_sampler, at.xy,
        i32(max(at.z, 1.0)) - 1, 0.0);
    if at.z < 1.0 || cached.a < 0.999 {
        // Listed instead: `direct_lamps` evaluates every lamp and writes this texel.
        let slot = atomicAdd(&direct_list.count, 1u);
        if slot % DIRECT_RECEIVERS == 0u {
            let group = slot/DIRECT_RECEIVERS;
            atomicMax(&direct_list.groups[0], min(group + 1u, DIRECT_GROUPS_PER_ROW));
            atomicMax(&direct_list.groups[1], group/DIRECT_GROUPS_PER_ROW + 1u);
        }
        direct_list.pixels[slot] = u32(pixel.x) | (u32(pixel.y) << 16u);
        return vec4(0.0, 0.0, 0.0, -1.0);
    }
    return vec4(cached.rgb, at.z);
}
@fragment fn receiver_light_cached(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    if !receiver_texel(pixel) { return vec4(0.0, 0.0, 0.0, 1.0); }
    let world = textureLoad(receiver_world, pixel, 0);
    let normal = textureLoad(receiver_normal, pixel, 0);
    let at = textureLoad(receiver_cache, pixel, 0);
    let cached = cached_lamps(pixel, at);
    if cached.a < 0.0 { return vec4(0.0, 0.0, 0.0, 1.0); }
    let lamps = lamp_light_cached(cached.rgb, world.xyz, normal.xyz);
    return realtime_light_with_lamps(world.xyz, normal.xyz, vec2(world.w, normal.w), lamps);
}
// The listed receivers, lit as `receiver_light` would: every lamp evaluated. The lanes of
// one receiver take every DIRECT_LANES-th entry of its list; lane 0 sums their shares in
// lane order and finishes the light.
struct DirectPixels { groups: array<u32, 3>, count: u32, pixels: array<u32> };
@group(3) @binding(8) var<storage, read> direct_pixels: DirectPixels;
@group(3) @binding(9) var direct_light: texture_storage_2d<rgba16float, write>;
var<workgroup> direct_shares: array<vec4<f32>, 64>;
@compute @workgroup_size(64) fn direct_lamps(@builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) local: u32) {
    let index = (group.y*DIRECT_GROUPS_PER_ROW + group.x)*DIRECT_RECEIVERS
        + local/DIRECT_LANES;
    let lane = local%DIRECT_LANES;
    let listed = index < direct_pixels.count;
    var pixel = vec2(0);
    var world = vec4(0.0);
    var normal = vec4(0.0);
    var share = vec3(0.0);
    if listed {
        let packed = direct_pixels.pixels[index];
        pixel = vec2<i32>(i32(packed & 0xffffu), i32(packed >> 16u));
        world = textureLoad(receiver_world, pixel, 0);
        normal = textureLoad(receiver_normal, pixel, 0);
        let at = lamp_cell(world.xyz);
        if at.w >= 0 {
            let entry = lamp_entry(world.xyz, at.xyz);
            let threshold = lamp_importance_threshold(world.xyz, at.xyz);
            for (var i = lane; i < entry.y; i += DIRECT_LANES) {
                let light = lamp_term(entry.x + i, world.xyz, normal.xyz, threshold);
                if light.w <= 0.0 { continue; }
                share += light.xyz*light.w;
            }
        }
    }
    direct_shares[local] = vec4(share, 0.0);
    workgroupBarrier();
    if listed && lane == 0u {
        var lamps = vec3(0.0);
        for (var k = 0u; k < DIRECT_LANES; k++) { lamps += direct_shares[local + k].xyz; }
        textureStore(direct_light, pixel,
            realtime_light_with_lamps(world.xyz, normal.xyz, vec2(world.w, normal.w), lamps));
    }
}
// Light buffers with a direction target (`sun_realtime.wgsl`, `DirectedLight`): the same
// light, with where its non-sun share comes from. The lamp cache then also holds the
// direction its lamp light arrives from (`lamp_cache_light.wgsl`).
@group(3) @binding(4) var lamp_direction_cache: texture_2d_array<f32>;
@group(3) @binding(10) var direct_direction: texture_storage_2d<rgba8unorm, write>;
const NO_DIRECTION: vec4<f32> = vec4(0.5, 0.5, 0.0, 0.0);
@fragment fn receiver_light_directed(@builtin(position) position: vec4<f32>) -> DirectedLight {
    let pixel = vec2<i32>(position.xy);
    if !receiver_texel(pixel) { return DirectedLight(vec4(0.0, 0.0, 0.0, 1.0), NO_DIRECTION); }
    let world = textureLoad(receiver_world, pixel, 0);
    let normal = textureLoad(receiver_normal, pixel, 0);
    return realtime_light_directed(world.xyz, normal.xyz, vec2(world.w, normal.w),
        lamp_light_directed(world.xyz, normal.xyz));
}
@fragment fn receiver_light_cached_directed(@builtin(position) position: vec4<f32>)
    -> DirectedLight {
    let pixel = vec2<i32>(position.xy);
    if !receiver_texel(pixel) { return DirectedLight(vec4(0.0, 0.0, 0.0, 1.0), NO_DIRECTION); }
    let world = textureLoad(receiver_world, pixel, 0);
    let normal = textureLoad(receiver_normal, pixel, 0);
    let at = textureLoad(receiver_cache, pixel, 0);
    let cached = cached_lamps(pixel, at);
    if cached.a < 0.0 { return DirectedLight(vec4(0.0, 0.0, 0.0, 1.0), NO_DIRECTION); }
    let lamps = lamp_light_cached(cached.rgb, world.xyz, normal.xyz);
    // The cached direction is a unit-luminance vector: scale it to the light actors leave.
    let encoded = textureSampleLevel(lamp_direction_cache, lamp_cache_sampler, at.xy,
        i32(cached.a) - 1, 0.0).xyz;
    let vector = (encoded*2.0 - 1.0)*lamp_luminance(lamps);
    return realtime_light_directed(world.xyz, normal.xyz, vec2(world.w, normal.w),
        LampLight(lamps, vector));
}
var<workgroup> direct_vectors: array<vec4<f32>, 64>;
@compute @workgroup_size(64) fn direct_lamps_directed(@builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) local: u32) {
    let index = (group.y*DIRECT_GROUPS_PER_ROW + group.x)*DIRECT_RECEIVERS
        + local/DIRECT_LANES;
    let lane = local%DIRECT_LANES;
    let listed = index < direct_pixels.count;
    var pixel = vec2(0);
    var world = vec4(0.0);
    var normal = vec4(0.0);
    var share = vec3(0.0);
    var vector = vec3(0.0);
    if listed {
        let packed = direct_pixels.pixels[index];
        pixel = vec2<i32>(i32(packed & 0xffffu), i32(packed >> 16u));
        world = textureLoad(receiver_world, pixel, 0);
        normal = textureLoad(receiver_normal, pixel, 0);
        let at = lamp_cell(world.xyz);
        if at.w >= 0 {
            let entry = lamp_entry(world.xyz, at.xyz);
            let threshold = lamp_importance_threshold(world.xyz, at.xyz);
            for (var i = lane; i < entry.y; i += DIRECT_LANES) {
                let light = lamp_term(entry.x + i, world.xyz, normal.xyz, threshold);
                if light.w <= 0.0 { continue; }
                share += light.xyz*light.w;
                vector += lamp_direction(entry.x + i, world.xyz)
                    *lamp_luminance(light.xyz*light.w);
            }
        }
    }
    direct_shares[local] = vec4(share, 0.0);
    direct_vectors[local] = vec4(vector, 0.0);
    workgroupBarrier();
    if listed && lane == 0u {
        var lamps = LampLight(vec3(0.0), vec3(0.0));
        for (var k = 0u; k < DIRECT_LANES; k++) {
            lamps.light += direct_shares[local + k].xyz;
            lamps.vector += direct_vectors[local + k].xyz;
        }
        let lit = realtime_light_directed(world.xyz, normal.xyz, vec2(world.w, normal.w), lamps);
        textureStore(direct_light, pixel, lit.light);
        textureStore(direct_direction, pixel, lit.direction);
    }
}
