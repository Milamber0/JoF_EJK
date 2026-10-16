// The cached texel: the same static lamp program the light pass runs per receiver.
// Alpha 1 marks a valid texel. A texel shared by separate surfaces is poisoned (negative
// colour, alpha 0) so neither the rim pass nor a bilinear footprint ever uses it.
@group(0) @binding(8) var nearest: texture_depth_2d;
@group(0) @binding(9) var farthest: texture_depth_2d;
@fragment fn bake_light(input: BakeOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(input.position.xy);
    if textureLoad(farthest, pixel, 0) - textureLoad(nearest, pixel, 0) > bake.limits.x {
        return vec4(-1.0, -1.0, -1.0, 0.0);
    }
    // Beside a strong fixture raw irradiance can exceed the half-float range; the lamp
    // response saturates long before this bound, so clamping leaves the display unchanged.
    return vec4(min(lamp_light(input.world, normalize(input.normal)), vec3(60000.0)), 1.0);
}
// With material maps the cache also keeps where the lamp light comes from: the luminance-
// weighted sum of lamp directions over the light's luminance (length one when it all
// comes from one direction), as unsigned colour. Poisoned and empty texels hold zero.
struct BakeDirected { @location(0) light: vec4<f32>, @location(1) direction: vec4<f32> };
@fragment fn bake_light_directed(input: BakeOutput) -> BakeDirected {
    let pixel = vec2<i32>(input.position.xy);
    if textureLoad(farthest, pixel, 0) - textureLoad(nearest, pixel, 0) > bake.limits.x {
        return BakeDirected(vec4(-1.0, -1.0, -1.0, 0.0), vec4(0.5, 0.5, 0.5, 0.0));
    }
    let lamps = lamp_light_directed(input.world, normalize(input.normal));
    let luminance = lamp_luminance(lamps.light);
    var vector = vec3(0.0);
    if luminance > 1e-6 { vector = lamps.vector/luminance; }
    return BakeDirected(vec4(min(lamps.light, vec3(60000.0)), 1.0),
        vec4(clamp(vector*0.5 + 0.5, vec3(0.0), vec3(1.0)), 1.0));
}
