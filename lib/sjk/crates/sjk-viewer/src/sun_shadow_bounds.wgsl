// Conservative min/max depth of the static shadow maps as a mip chain: level 0 holds one
// 8x8-texel tile per texel, each further level the 2x2 tiles below it. Receivers pick the
// finest level whose tiles are as wide as their filter footprint (`bounded_visibility`).
// No depth is approximated, and clamping repeats edge texels in a partial final tile.
@group(0) @binding(1) var bounds: texture_storage_2d_array<rg32float, write>;
@group(0) @binding(2) var<uniform> parameters: vec4<u32>;
@group(0) @binding(0) var source_depth: texture_depth_2d;
@compute @workgroup_size(8, 8, 1)
fn build(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(bounds)) { return; }
    let last = vec2<i32>(textureDimensions(source_depth)) - 1;
    let origin = vec2<i32>(id.xy*8u);
    var low = 1.0;
    var high = 0.0;
    for (var y = 0; y < 8; y++) {
        for (var x = 0; x < 8; x++) {
            let depth = textureLoad(source_depth, min(origin + vec2(x, y), last), 0);
            low = min(low, depth);
            high = max(high, depth);
        }
    }
    textureStore(bounds, vec2<i32>(id.xy), i32(parameters.x), vec4(low, high, 0.0, 0.0));
}
// The level below the one `bounds` writes.
@group(0) @binding(3) var finer: texture_2d_array<f32>;
@compute @workgroup_size(8, 8, 1)
fn reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(bounds)) { return; }
    let last = vec2<i32>(textureDimensions(finer)) - 1;
    let layer = i32(parameters.x);
    let base = vec2<i32>(id.xy*2u);
    let a = textureLoad(finer, min(base, last), layer, 0).xy;
    let b = textureLoad(finer, min(base + vec2(1, 0), last), layer, 0).xy;
    let c = textureLoad(finer, min(base + vec2(0, 1), last), layer, 0).xy;
    let d = textureLoad(finer, min(base + vec2(1, 1), last), layer, 0).xy;
    textureStore(bounds, vec2<i32>(id.xy), layer,
        vec4(min(min(a.x, b.x), min(c.x, d.x)), max(max(a.y, b.y), max(c.y, d.y)), 0.0, 0.0));
}
