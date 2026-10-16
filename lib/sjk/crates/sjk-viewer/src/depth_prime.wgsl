// Match the material vertex transform exactly; later passes compare this depth.
struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@vertex fn vertex(@location(0) position: vec3<f32>) -> @builtin(position) @invariant vec4<f32> {
    return camera.view_projection * vec4(position, 1.0);
}
