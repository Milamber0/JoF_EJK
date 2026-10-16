// Depth-only obscurance. No near/far constants, normals buffer or noise texture.
struct Camera {
    matrix: mat4x4<f32>, eye: vec3<f32>, time: f32,
    forward: vec3<f32>, padding: f32,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var scene_depth: texture_depth_2d;

@vertex fn vertex(@location(0) position: vec3<f32>) -> @builtin(position) @invariant vec4<f32> {
    return camera.matrix * vec4(position, 1.0);
}

// Solve the screen ray from two clip-plane equations, then solve Z/W along it.
// This works for ordinary and oblique perspective matrices; no depth copy is needed.
fn position_at(pixel: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(scene_depth));
    if any(pixel < vec2(0)) || any(pixel >= size) { return vec4(0.0); }
    let depth = textureLoad(scene_depth, pixel, 0);
    if depth >= 1.0 { return vec4(0.0); }
    let ndc = (vec2<f32>(pixel) + vec2(0.5)) / vec2<f32>(size) * 2.0 - 1.0;
    let m = camera.matrix;
    let rx = vec3(m[0].x, m[1].x, m[2].x);
    let ry = vec3(m[0].y, m[1].y, m[2].y);
    let rz = vec3(m[0].z, m[1].z, m[2].z);
    let rw = vec3(m[0].w, m[1].w, m[2].w);
    var ray = cross(rx - ndc.x * rw, ry + ndc.y * rw);
    let magnitude = length(ray);
    if magnitude < 0.000001 { return vec4(0.0); }
    ray /= magnitude;
    let origin = m * vec4(camera.eye, 1.0);
    let denominator = dot(rz - depth * rw, ray);
    if abs(denominator) < 0.00000001 { return vec4(0.0); }
    let distance = (depth * origin.w - origin.z) / denominator;
    return vec4(camera.eye + ray * distance, 1.0);
}

fn difference(center: vec3<f32>, low: vec4<f32>, high: vec4<f32>) -> vec3<f32> {
    if low.w == 0.0 && high.w == 0.0 { return vec3(0.0); }
    if low.w == 0.0 { return high.xyz - center; }
    if high.w == 0.0 { return center - low.xyz; }
    let a = center - low.xyz;
    let b = high.xyz - center;
    // Smaller view-depth change avoids taking a derivative across a silhouette.
    return select(b, a, abs(dot(a, camera.forward)) < abs(dot(b, camera.forward)));
}

fn obscurance(pixel: vec2<i32>) -> f32 {
    let point = position_at(pixel);
    if point.w == 0.0 { return 0.0; }
    let dx = difference(point.xyz, position_at(pixel - vec2(1,0)),
        position_at(pixel + vec2(1,0)));
    let dy = difference(point.xyz, position_at(pixel - vec2(0,1)),
        position_at(pixel + vec2(0,1)));
    var normal = cross(dx, dy);
    if length(normal) < 0.000001 { return 0.0; }
    normal = normalize(normal);
    normal *= select(-1.0, 1.0, dot(normal, camera.eye - point.xyz) >= 0.0);
    // Fixed golden-angle disk, no stochastic noise or two-radius banding.
    let kernel = array(
        vec2(0.125000,0.000000),vec2(-0.159645,0.146248),
        vec2(0.024436,-0.278438),vec2(0.201222,0.262459),
        vec2(-0.369268,-0.065318),vec2(0.349802,-0.222516),
        vec2(-0.117002,0.435242),vec2(-0.223136,-0.429634),
        vec2(0.484115,0.176798),vec2(-0.503641,0.207896),
        vec2(0.242788,-0.518824),vec2(0.179414,0.572001),
        vec2(-0.540757,-0.313380),vec2(0.634370,-0.139464),
        vec2(-0.387146,0.550675),vec2(-0.089440,-0.690200),
        vec2(0.549072,0.462758),vec2(-0.738878,0.030555),
        vec2(0.538955,-0.536332),vec2(-0.036058,0.779792),
        vec2(-0.512818,-0.614527),vec2(0.812360,0.109302),
        vec2(-0.688311,0.478909),vec2(0.188086,-0.836061),
        vec2(0.435033,0.759191),vec2(-0.850448,-0.271316),
        vec2(0.826102,-0.381680),vec2(-0.357888,0.855156),
        vec2(-0.319407,-0.888034),vec2(0.849909,0.446688),
        vec2(-0.944035,0.248845),vec2(0.536596,-0.834530));
    let clip = camera.matrix * vec4(point.xyz, 1.0);
    let row_y = vec3(camera.matrix[0].y,camera.matrix[1].y,camera.matrix[2].y);
    let radius = min(128.0, 32.0 * length(row_y) *
        f32(textureDimensions(scene_depth).y) * 0.5 / max(abs(clip.w), 0.001));
    var total = 0.0;
    for (var i = 0u; i < 32u; i++) {
        let offset = kernel[i] * radius;
        let sample = position_at(pixel + vec2<i32>(round(offset)));
        let delta = sample.xyz - point.xyz;
        let distance = length(delta);
        if sample.w != 0.0 && distance > 0.001 && distance < 32.0 {
            total += max(0.0, (dot(normal, delta) - 1.0) / distance) *
                (1.0 - distance / 32.0);
        }
    }
    return min(0.35, total / 32.0 * 2.0);
}

@fragment fn fragment(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4(vec3(ssao_visibility(obscurance(vec2<i32>(pixel.xy)))), 1.0);
}
