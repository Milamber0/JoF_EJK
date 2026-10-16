@group(1) @binding(7) var shadow_bounds: texture_2d_array<f32>;
// Half-resolution light pass over the opaque world: a depth pre-pass, then the real-time
// light (sun cascades plus probe gather) once per texel of the nearest surface only. The
// stage shader upsamples the result (`sun_realtime_buffer.wgsl`). Group 1 is the shared
// receiver group; the shared sources above are re-numbered from group 3 at assembly.
@group(1) @binding(0) var depth_map: texture_depth_2d;
@group(1) @binding(1) var comparison: sampler_comparison;
@group(1) @binding(2) var<uniform> shadow: Shadow;
@group(1) @binding(3) var far_map: texture_depth_2d;
@group(1) @binding(4) var close_map: texture_depth_2d;
@group(1) @binding(5) var world_map: texture_depth_2d;
@group(1) @binding(6) var close_world_map: texture_depth_2d;
// The occlusion term of the pre-pass (`light_occlusion.wgsl`), one texel per fragment.
@group(1) @binding(22) var light_occlusion: texture_2d<f32>;
// Pre-pass surfaces for contact-shadow intersections.
@group(1) @binding(21) var light_depth: texture_depth_2d;
@group(1) @binding(23) var contact_normal: texture_2d<f32>;
// The occlusion pass turns its tap pattern per texel in a 2x2 cycle; the aligned 2x2
// block averages the four turns, so no lattice of the pattern reaches the light.
fn ambient_occlusion(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let clip = camera.view_projection*vec4(world, 1.0);
    if clip.w <= 0.0 { return 1.0; }
    let dims = vec2<i32>(textureDimensions(light_occlusion));
    let pixel = clamp(vec2<i32>((clip.xy/clip.w*vec2(0.5, -0.5) + 0.5)*vec2<f32>(dims)),
        vec2(0), dims - 1);
    let block = pixel & vec2(~1);
    var sum = 0.0;
    for (var y = 0; y < 2; y++) { for (var x = 0; x < 2; x++) {
        sum += textureLoad(light_occlusion, clamp(block + vec2(x, y), vec2(0), dims - 1), 0).x;
    } }
    return sum*0.25;
}
// Invariant: the light pass shades only fragments at exactly the pre-pass depth, so a
// culled back face nearer than the visible front face can never take over a texel.
struct LightOutput {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Texture coordinates for the entity pre-pass's alpha test.
    @location(2) uv: vec2<f32>,
    // Lamp cache coordinates and layer + 1 (`lamp_cache.rs`); zero: evaluate directly.
    @location(3) cache_uv: vec2<f32>,
    @location(4) @interpolate(flat) cache_page: f32,
};
@vertex fn static_vertex(input: VertexInput) -> LightOutput {
    var result: LightOutput;
    result.position = camera.view_projection*vec4(input.position, 1.0);
    result.world = input.position;
    result.normal = input.normal;
    result.uv = input.texture_coordinates;
    return result;
}
// Static world vertices with the lamp cache's page stream at slot 1.
@vertex fn static_vertex_cached(input: VertexInput, @location(5) page: f32) -> LightOutput {
    var result: LightOutput;
    result.position = camera.view_projection*vec4(input.position, 1.0);
    result.world = input.position;
    result.normal = input.normal;
    result.uv = input.texture_coordinates;
    result.cache_uv = input.lightmap_coordinates;
    result.cache_page = page;
    return result;
}

@vertex fn mover_vertex(input: VertexInput, instance: InstanceInput) -> LightOutput {
    var result: LightOutput;
    let world = instance_position(input, instance);
    result.position = camera.view_projection*vec4(world, 1.0);
    result.world = world;
    result.normal = rotate_vector(instance.rotation, input.normal);
    result.uv = input.texture_coordinates;
    return result;
}
// Pre-pass colour: the surface normal the occlusion pass needs, alongside its depth.
// The pre-pass texel: the surface normal turned toward the viewer, packed.
fn packed_normal(input: LightOutput) -> vec4<f32> {
    let normal = surface_normal(input.world, normalize(input.normal), camera.camera_position);
    return vec4(normal*0.5 + 0.5, 1.0);
}
@fragment fn normal_fragment(input: LightOutput) -> @location(0) vec4<f32> {
    return packed_normal(input);
}
@fragment fn light_fragment(input: LightOutput) -> @location(0) vec4<f32> {
    // Two-sided surfaces must use the same geometric side as the pre-pass.
    let normal = surface_normal(input.world, normalize(input.normal), camera.camera_position);
    return realtime_light(input.world, normal, camera.camera_position, camera.view_forward);
}
