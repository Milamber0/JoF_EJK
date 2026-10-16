// Post-processing of one baked layer (`lamp_cache.rs`), two fullscreen passes.
//
// `steep` bounds the cache's error: where lamp light bends too sharply for bilinear
// interpolation between texel centres (beside a fixture, across a hard shadow edge), the
// texel is poisoned and the light pass evaluates those receivers directly, as before.
// `rim` then gives every chart one texel of rim: an uncovered texel takes the mean of its
// valid neighbours, so a bilinear footprint at a chart border reads four valid texels.
// Poisoned texels (negative colour, alpha 0) pass through both unchanged.
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn rim_vertex(@builtin(vertex_index) id: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0));
    return vec4(p[id], 0.0, 1.0);
}
fn texel_at(pixel: vec2<i32>) -> vec4<f32> {
    let limit = vec2<i32>(textureDimensions(source)) - 1;
    return textureLoad(source, clamp(pixel, vec2(0), limit), 0);
}
// Displayed lamp light of a valid texel; interpolation error is judged where it shows.
fn shown(texel: vec4<f32>) -> vec3<f32> { return lamp_response(max(texel.rgb, vec3(0.0))); }
// Bilinear interpolation of a curve errs by an eighth of its second difference: 0.04
// keeps the error near one 8-bit step. A missing neighbour falls back to the first
// difference, which also catches a step at a chart's edge.
fn bend(here: vec4<f32>, before: vec4<f32>, after: vec4<f32>) -> f32 {
    let centre = shown(here);
    var d = vec3(0.0);
    if before.a > 0.0 && after.a > 0.0 { d = abs(shown(before) + shown(after) - 2.0*centre); }
    else if before.a > 0.0 { d = abs(shown(before) - centre); }
    else if after.a > 0.0 { d = abs(shown(after) - centre); }
    return max(d.x, max(d.y, d.z));
}
@fragment fn steep(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return steep_texel(vec2<i32>(position.xy));
}
fn steep_texel(pixel: vec2<i32>) -> vec4<f32> {
    let here = texel_at(pixel);
    if here.a <= 0.0 { return here; }
    let across = bend(here, texel_at(pixel - vec2(1, 0)), texel_at(pixel + vec2(1, 0)));
    let along = bend(here, texel_at(pixel - vec2(0, 1)), texel_at(pixel + vec2(0, 1)));
    if max(across, along) > 0.04 { return vec4(-1.0, -1.0, -1.0, 0.0); }
    return here;
}
@fragment fn rim(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return rim_texel(vec2<i32>(position.xy));
}
fn rim_texel(pixel: vec2<i32>) -> vec4<f32> {
    let here = texel_at(pixel);
    if here.a > 0.0 || here.r < 0.0 { return here; }
    var sum = vec4(0.0);
    for (var y = -1; y <= 1; y++) { for (var x = -1; x <= 1; x++) {
        let texel = texel_at(pixel + vec2(x, y));
        if texel.a > 0.0 { sum += vec4(texel.rgb, 1.0); }
    } }
    if sum.a == 0.0 { return here; }
    return vec4(sum.rgb/sum.a, 1.0);
}
// The direction layer beside the light (`bake_light_directed`): `steep` passes it through,
// `rim` gives an uncovered texel the mean direction of the valid neighbours its light
// came from, so bilinear footprints at chart borders read valid directions too.
@group(0) @binding(1) var direction_source: texture_2d<f32>;
struct RimDirected { @location(0) light: vec4<f32>, @location(1) direction: vec4<f32> };
fn direction_at(pixel: vec2<i32>) -> vec4<f32> {
    let limit = vec2<i32>(textureDimensions(direction_source)) - 1;
    return textureLoad(direction_source, clamp(pixel, vec2(0), limit), 0);
}
@fragment fn steep_directed(@builtin(position) position: vec4<f32>) -> RimDirected {
    let pixel = vec2<i32>(position.xy);
    return RimDirected(steep_texel(pixel), direction_at(pixel));
}
@fragment fn rim_directed(@builtin(position) position: vec4<f32>) -> RimDirected {
    let pixel = vec2<i32>(position.xy);
    let here = texel_at(pixel);
    if here.a > 0.0 || here.r < 0.0 { return RimDirected(here, direction_at(pixel)); }
    var sum = vec4(0.0);
    for (var y = -1; y <= 1; y++) { for (var x = -1; x <= 1; x++) {
        if texel_at(pixel + vec2(x, y)).a > 0.0 {
            sum += vec4(direction_at(pixel + vec2(x, y)).xyz, 1.0);
        }
    } }
    if sum.a == 0.0 { return RimDirected(here, direction_at(pixel)); }
    return RimDirected(rim_texel(pixel), vec4(sum.xyz/sum.a, 1.0));
}
// Empties a rectangle (scissored) before a partial bake of it (`Cache::refresh`).
@fragment fn clear() -> @location(0) vec4<f32> { return vec4(0.0); }
@fragment fn clear_directed() -> RimDirected { return RimDirected(vec4(0.0), vec4(0.0)); }
