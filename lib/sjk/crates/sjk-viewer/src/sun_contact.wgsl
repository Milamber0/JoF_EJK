// Screen-space contact shadows from sampled surface planes. Shared by world and entity lighting.
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
    var ray = cross(rx - ndc.x*rw, ry + ndc.y*rw);
    let magnitude = length(ray);
    if magnitude < 1e-6 { return vec4(0.0); }
    ray /= magnitude;
    let origin = m*vec4(camera.camera_position, 1.0);
    let denominator = dot(rz - depth*rw, ray);
    if abs(denominator) < 1e-8 { return vec4(0.0); }
    let distance = (depth*origin.w - origin.z)/denominator;
    return vec4(camera.camera_position + ray*distance, 1.0);
}

const CONTACT_LENGTH: f32 = 12.0;
const CONTACT_SAMPLES: u32 = 12u;

fn contact_texel(world: vec3<f32>, dims: vec2<f32>) -> vec3<f32> {
    let clip = camera.view_projection*vec4(world, 1.0);
    return vec3((clip.xy/clip.w*vec2(0.5, -0.5) + 0.5)*dims - 0.5, clip.w);
}

// Bound float depth reconstruction error, including cancellation when transforming a
// large world-space camera origin. It grows with distance; a fixed world epsilon
// falsely splits one distant plane into alternating receiver and blocker samples.
fn contact_depth_error(world: vec3<f32>) -> f32 {
    let clip = camera.view_projection*vec4(world, 1.0);
    let origin = camera.view_projection*vec4(camera.camera_position, 1.0);
    let distance = length(world - camera.camera_position);
    return 2e-7*(distance + length(camera.camera_position))*abs(clip.w)/max(abs(origin.z), 0.01);
}

// Test the ray against a surface, not against a slab extending behind its depth.
// Reject samples consistent with the receiver's local plane or smooth curvature.
fn contact_intersection(world: vec3<f32>, normal: vec3<f32>, surface: vec3<f32>,
    surface_normal: vec3<f32>, sun: vec3<f32>) -> f32 {
    let delta = surface - world;
    let alignment = dot(normal, surface_normal);
    let uncertainty = contact_depth_error(world) + contact_depth_error(surface);
    let tolerance = 0.02 + length(delta)*0.002 + uncertainty;
    if alignment > 0.95 && abs(dot(normal, delta)) <=
        tolerance + length(delta)*sqrt(max(0.0, 1.0 - alignment)) { return -1.0; }
    let denominator = dot(surface_normal, sun);
    if abs(denominator) < 0.05 { return -1.0; }
    let distance = dot(surface_normal, delta)/denominator;
    if distance <= 0.005 || distance >= CONTACT_LENGTH { return -1.0; }
    return distance;
}

// Bilinear coverage of the intersected plane. Never interpolate foreground and
// background depths into a fictitious blocker across a silhouette.
fn contact_coverage(hit: vec3<f32>, normal: vec3<f32>, at: vec2<f32>) -> f32 {
    let base = vec2<i32>(floor(at));
    let blend = fract(at);
    let dims = vec2<i32>(textureDimensions(light_depth));
    var coverage = 0.0;
    for (var y = 0; y < 2; y++) { for (var x = 0; x < 2; x++) {
        let pixel = base + vec2(x, y);
        if any(pixel < vec2(0)) || any(pixel >= dims) { continue; }
        let surface = depth_position(pixel);
        if surface.w == 0.0 { continue; }
        let n = normalize(textureLoad(contact_normal, pixel, 0).xyz*2.0 - 1.0);
        let delta = surface.xyz - hit;
        if dot(normal, n) < 0.9 || abs(dot(normal, delta)) > 0.05 + length(delta)*0.01 + contact_depth_error(surface.xyz) {
            continue;
        }
        coverage += select(1.0-blend.x, blend.x, x == 1)*
            select(1.0-blend.y, blend.y, y == 1);
    } }
    return coverage;
}

fn contact_shadow(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let sun = shadow.sun.xyz;
    if shadow.radiance.w <= 0.0 || dot(normal, sun) <= 0.0 { return 1.0; }
    let dims = vec2<f32>(textureDimensions(light_depth));
    let start = contact_texel(world, dims);
    if start.z <= 0.0 { return 1.0; }
    var end = contact_texel(world + sun*CONTACT_LENGTH, dims);
    // Keep the projected search in front of the eye even for a very close receiver.
    if end.z <= 0.0 {
        let length = CONTACT_LENGTH*start.z/(start.z - end.z)*0.95;
        end = contact_texel(world + sun*length, dims);
    }
    let delta = end.xy - start.xy;
    let extent = max(abs(delta.x), abs(delta.y));
    if extent < 0.01 { return 1.0; }
    // Search the first eight texels contiguously, then four samples to the ray end.
    // This preserves thin near contacts without repeatedly testing one distant texel.
    let step = delta/max(extent, 1.0);
    var occlusion = 0.0;
    for (var i = 0u; i < CONTACT_SAMPLES; i++) {
        let offset = select(f32(i), mix(8.0, max(extent, 8.0), (f32(i) - 7.0)/4.0), i >= 8u);
        if offset > extent + 1.0 { break; }
        let pixel = vec2<i32>(floor(start.xy + step*offset + 0.5));
        if any(pixel < vec2(0)) || any(pixel >= vec2<i32>(dims)) { break; }
        let surface = depth_position(pixel);
        if surface.w == 0.0 { continue; }
        let n = normalize(textureLoad(contact_normal, pixel, 0).xyz*2.0 - 1.0);
        let distance = contact_intersection(world, normal, surface.xyz, n, sun);
        if distance < 0.0 { continue; }
        let hit = world + sun*distance;
        let at = contact_texel(hit, dims);
        // Verify coverage at the actual hit; the candidate plane alone is not evidence
        // of an occluder behind an unrelated foreground edge.
        if length(surface.xyz - hit) > CONTACT_LENGTH*2.0 { continue; }
        let confidence = 1.0 - smoothstep(CONTACT_LENGTH*0.75, CONTACT_LENGTH, distance);
        // Tighten the filter at physical contact, where a half-resolution footprint
        // straddles the receiver and blocker. Detached shadows keep the wider filter.
        let width = mix(0.05, 0.5, smoothstep(0.5, 2.0, distance));
        let coverage = smoothstep(0.0, width, contact_coverage(hit, n, at.xy));
        occlusion = max(occlusion, coverage*confidence);
        if occlusion > 0.999 { break; }
    }
    return 1.0 - occlusion;
}
