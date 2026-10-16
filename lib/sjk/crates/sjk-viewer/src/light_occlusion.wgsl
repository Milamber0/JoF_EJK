// Occlusion of the sky and bounce light over the half-resolution pre-pass: one thread per
// texel reconstructs its position from depth, takes the normal the pre-pass wrote, and
// samples a golden-angle disk of depth around it. A fullscreen pass of its own, before
// the light pass, at full occupancy instead of inside that register-heavy fragment.
struct Camera {
    view_projection: mat4x4<f32>, camera_position: vec3<f32>, shader_time: f32,
    view_forward: vec3<f32>, _padding: f32,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var light_depth: texture_depth_2d;
@group(1) @binding(1) var light_normal: texture_2d<f32>;
// World-unit reach of the occlusion term and its strength in a fully enclosed corner.
// A crease term, not a wall-base band: 64 units of range darkened a strip a player's
// height wide along every wall, hard and offset once seen from afar.
const AO_RANGE: f32 = 24.0;
const AO_STRENGTH: f32 = 3.5;
const AO_MAX: f32 = 0.45;
const AO_SAMPLES: u32 = 16u;
// Fixed sample directions and radii retain the original f32 expressions.
const AO_DIRECTIONS = array<vec2<f32>, AO_SAMPLES>(
    vec2(cos(0.0f * 2.39996323f), sin(0.0f * 2.39996323f)),
    vec2(cos(1.0f * 2.39996323f), sin(1.0f * 2.39996323f)),
    vec2(cos(2.0f * 2.39996323f), sin(2.0f * 2.39996323f)),
    vec2(cos(3.0f * 2.39996323f), sin(3.0f * 2.39996323f)),
    vec2(cos(4.0f * 2.39996323f), sin(4.0f * 2.39996323f)),
    vec2(cos(5.0f * 2.39996323f), sin(5.0f * 2.39996323f)),
    vec2(cos(6.0f * 2.39996323f), sin(6.0f * 2.39996323f)),
    vec2(cos(7.0f * 2.39996323f), sin(7.0f * 2.39996323f)),
    vec2(cos(8.0f * 2.39996323f), sin(8.0f * 2.39996323f)),
    vec2(cos(9.0f * 2.39996323f), sin(9.0f * 2.39996323f)),
    vec2(cos(10.0f * 2.39996323f), sin(10.0f * 2.39996323f)),
    vec2(cos(11.0f * 2.39996323f), sin(11.0f * 2.39996323f)),
    vec2(cos(12.0f * 2.39996323f), sin(12.0f * 2.39996323f)),
    vec2(cos(13.0f * 2.39996323f), sin(13.0f * 2.39996323f)),
    vec2(cos(14.0f * 2.39996323f), sin(14.0f * 2.39996323f)),
    vec2(cos(15.0f * 2.39996323f), sin(15.0f * 2.39996323f))
);
const AO_REACH = array<f32, AO_SAMPLES>(
    sqrt((0.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((1.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((2.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((3.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((4.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((5.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((6.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((7.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((8.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((9.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((10.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((11.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((12.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((13.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((14.0f + 0.5f)/f32(AO_SAMPLES)),
    sqrt((15.0f + 0.5f)/f32(AO_SAMPLES))
);
// Solve the screen ray from two clip-plane equations, then Z/W along it: no inverse matrix.
fn depth_position(pixel: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(light_depth));
    if any(pixel < vec2(0)) || any(pixel >= size) { return vec4(0.0); }
    let depth = textureLoad(light_depth, pixel, 0);
    if depth >= 1.0 { return vec4(0.0); }
    let ndc = (vec2<f32>(pixel) + vec2(0.5))/vec2<f32>(size)*2.0 - 1.0;
    let m = camera.view_projection;
    let rx = vec3(m[0].x, m[1].x, m[2].x);
    let ry = vec3(m[0].y, m[1].y, m[2].y);
    let rz = vec3(m[0].z, m[1].z, m[2].z);
    let rw = vec3(m[0].w, m[1].w, m[2].w);
    let ray = cross(rx - ndc.x*rw, ry + ndc.y*rw);
    let magnitude = length(ray);
    if magnitude < 1e-6 { return vec4(0.0); }
    // Ray scale cancels in ray * distance below; retain the scaled degeneracy guard.
    let origin = m*vec4(camera.camera_position, 1.0);
    let denominator = dot(rz - depth*rw, ray);
    if abs(denominator) < 1e-8*magnitude { return vec4(0.0); }
    let distance = (depth*origin.w - origin.z)/denominator;
    return vec4(camera.camera_position + ray*distance, 1.0);
}
// Obscurance of the sky and bounce light: nearby surfaces above this point's tangent plane
// within AO_RANGE, from a golden-angle disk of pre-pass depth samples turned by the texel
// parity so the fixed kernel's banding breaks into a pattern the upsample averages away.
fn occlusion(pixel: vec2<i32>, world: vec3<f32>, normal: vec3<f32>, clip_w: f32) -> f32 {
    let dims = vec2<f32>(textureDimensions(light_depth));
    let row_y = vec3(camera.view_projection[0].y, camera.view_projection[1].y,
        camera.view_projection[2].y);
    let radius = min(dims.y*0.25, AO_RANGE*length(row_y)*dims.y*0.5/clip_w);
    if radius < 1.0 { return 1.0; }
    let turn = f32((pixel.x & 1) + 2*(pixel.y & 1))*1.5707963;
    let rotation = vec2(cos(turn), sin(turn));
    var total = 0.0;
    for (var i = 0u; i < AO_SAMPLES; i++) {
        let reach = AO_REACH[i]*radius;
        var offset = AO_DIRECTIONS[i]*reach;
        offset = vec2(offset.x*rotation.x - offset.y*rotation.y,
            offset.x*rotation.y + offset.y*rotation.x);
        let sample = depth_position(pixel + vec2<i32>(round(offset)));
        if sample.w == 0.0 { continue; }
        let delta = sample.xyz - world;
        let distance = length(delta);
        if distance < 0.5 || distance >= AO_RANGE { continue; }
        let falloff = 1.0 - distance/AO_RANGE;
        total += max(dot(normal, delta)/distance - 0.05, 0.0)*falloff*falloff;
    }
    return 1.0 - min(AO_MAX, total/f32(AO_SAMPLES)*AO_STRENGTH);
}
@vertex fn fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(positions[index], 0.0, 1.0);
}
@fragment fn occlude(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let world = depth_position(pixel);
    if world.w == 0.0 { return vec4(1.0); }
    let normal = normalize(textureLoad(light_normal, pixel, 0).xyz*2.0 - 1.0);
    let clip_w = (camera.view_projection*vec4(world.xyz, 1.0)).w;
    return vec4(occlusion(pixel, world.xyz, normal, clip_w), 0.0, 0.0, 1.0);
}
