// Day mode's ambient correction: the light pass's occlusion term (half the display
// resolution, `light_occlusion.wgsl`) stands in for the 32-tap obscurance, at the same
// strength. One filtered sample per pixel instead of 160 depth loads.
struct Camera {
    matrix: mat4x4<f32>, eye: vec3<f32>, time: f32,
    forward: vec3<f32>, padding: f32,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var occlusion: texture_2d<f32>;
@group(1) @binding(1) var occlusion_sampler: sampler;
// Scene pixel to buffer texel, xy.
@group(1) @binding(2) var<uniform> scale: vec4<f32>;

@vertex fn vertex(@location(0) position: vec3<f32>) -> @builtin(position) @invariant vec4<f32> {
    return camera.matrix * vec4(position, 1.0);
}

@fragment fn fragment(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    // Sample at the corner of the aligned 2x2 block: the bilinear fetch averages the
    // pass's four per-texel pattern turns instead of showing them as a lattice.
    let dims = vec2<f32>(textureDimensions(occlusion));
    let texel = pixel.xy * scale.xy;
    let corner = floor(texel * 0.5) * 2.0 + 1.0;
    let term = textureSampleLevel(occlusion, occlusion_sampler, corner / dims, 0.0).x;
    return vec4(vec3(ssao_visibility(0.35 * (1.0 - term))), 1.0);
}
