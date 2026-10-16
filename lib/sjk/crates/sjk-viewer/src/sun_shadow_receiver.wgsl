@group(1) @binding(7) var shadow_bounds: texture_2d_array<f32>;
struct Camera {
    vp: mat4x4<f32>, position: vec3<f32>, time: f32, forward: vec3<f32>, flags: f32,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var depth_map: texture_depth_2d;
@group(1) @binding(1) var comparison: sampler_comparison;
@group(1) @binding(2) var<uniform> shadow: Shadow;
@group(1) @binding(3) var far_map: texture_depth_2d;
@group(1) @binding(4) var close_map: texture_depth_2d;
@group(1) @binding(5) var world_map: texture_depth_2d;
@group(1) @binding(6) var close_world_map: texture_depth_2d;
struct Input { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32> };
struct Output {
    @builtin(position) position: vec4<f32>, @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
@vertex fn vertex(input: Input) -> Output {
    var result: Output;
    result.position = camera.vp * vec4(input.position, 1.0);
    result.world = input.position;
    result.normal = input.normal;
    return result;
}
@fragment fn fragment(input: Output) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal);
    let facing = max(dot(normal, shadow.sun.xyz), 0.0);
    let sample = sun_visibility(input.world,normal,camera.position,camera.forward);
    let shade = 1.0 - (1.0-sample.x) * shadow.sun.w * facing * sample.y;
    return vec4(vec3(shade), 1.0);
}
