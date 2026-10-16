// Ground HUD: two numbers and a stance dot lying flat around the local
// player's feet, in the player's yaw frame. Health + shield sits left of the
// feet, Force right of them, both reading upright from the camera behind the
// player (text "up" along the player's facing); the stance dot lies behind the
// heels. Each piece is one instanced quad: digits sample the UI font atlas
// with a dark outline, the dot is an antialiased disc with a contact halo.
// Scene depth is read, not tested: what stands in front of the HUD dims it
// instead of cutting it, so uneven floors never swallow it, and the player's
// own body (which covers the feet and the dot from behind) only softens it.

struct GpuQuad {
    // Text-plane rectangle x0, y0, x1, y1; x to the player's right, y along
    // the player's facing.
    rect: vec4<f32>,
    // Atlas u at x0, v at y0, u at x1, v at y1.
    uv: vec4<f32>,
    // Atlas sampling bounds u0, v0, u1, v1.
    bounds: vec4<f32>,
    // Glyph: outline radius in atlas units (xy). Dot: radius, halo in world units.
    outline: vec4<f32>,
    // rgb colour, w kind (0 glyph, 1 dot).
    colour: vec4<f32>,
};

struct Ground {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    // xyz camera position, w radius around the player whose occluders count as
    // the player's own body.
    camera: vec4<f32>,
    // xyz normalised view forward, w opacity behind the player's own body.
    forward: vec4<f32>,
    // xyz feet, w player yaw in radians.
    anchor: vec4<f32>,
    // width, height, 1/width, 1/height of the colour target.
    viewport: vec4<f32>,
    // ghost opacity, overall opacity, digit outline opacity, dot halo opacity.
    style: vec4<f32>,
    // occlusion tolerance start/end in view-depth units.
    depth: vec4<f32>,
    quads: array<GpuQuad, 7>,
};

@group(0) @binding(0) var<uniform> ground: Ground;
@group(1) @binding(0) var scene_depth: texture_depth_2d;
@group(2) @binding(0) var atlas: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    // Text-plane position in world units.
    @location(0) plane: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world: vec3<f32>,
    @location(3) @interpolate(flat) quad: u32,
};

@vertex
fn vertex_main(@builtin(vertex_index) index: u32, @builtin(instance_index) quad: u32)
    -> VertexOut {
    let q = ground.quads[quad];
    let corner = vec2<f32>(f32(index & 1u), f32((index >> 1u) & 1u));
    let plane = mix(q.rect.xy, q.rect.zw, corner);
    let uv = mix(q.uv.xy, q.uv.zw, corner);
    let yaw = ground.anchor.w;
    let forward = vec3<f32>(cos(yaw), sin(yaw), 0.0);
    let right = vec3<f32>(sin(yaw), -cos(yaw), 0.0);
    let world = ground.anchor.xyz + right * plane.x + forward * plane.y;
    var out: VertexOut;
    out.position = ground.view_projection * vec4<f32>(world, 1.0);
    out.plane = plane;
    out.uv = uv;
    out.world = world;
    out.quad = quad;
    return out;
}

// Premultiplied "over".
fn over(below: vec4<f32>, colour: vec3<f32>, alpha: f32) -> vec4<f32> {
    return vec4<f32>(colour * alpha, alpha) + below * (1.0 - alpha);
}

fn coverage_at(q: GpuQuad, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> f32 {
    return textureSampleGrad(atlas, atlas_sampler, clamp(uv, q.bounds.xy, q.bounds.zw), dx, dy).a;
}

// A digit: the atlas coverage over a dark outline (the coverage dilated by the
// outline radius in eight directions).
fn glyph(q: GpuQuad, uv: vec2<f32>) -> vec4<f32> {
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let ink = coverage_at(q, uv, dx, dy);
    let r = q.outline.xy;
    let d = r * 0.7071;
    var outline = max(
        max(coverage_at(q, uv + vec2<f32>(r.x, 0.0), dx, dy),
            coverage_at(q, uv - vec2<f32>(r.x, 0.0), dx, dy)),
        max(coverage_at(q, uv + vec2<f32>(0.0, r.y), dx, dy),
            coverage_at(q, uv - vec2<f32>(0.0, r.y), dx, dy)));
    outline = max(outline, max(
        max(coverage_at(q, uv + d, dx, dy), coverage_at(q, uv - d, dx, dy)),
        max(coverage_at(q, uv + vec2<f32>(d.x, -d.y), dx, dy),
            coverage_at(q, uv + vec2<f32>(-d.x, d.y), dx, dy))));
    var colour = over(vec4<f32>(0.0), vec3<f32>(0.0), max(outline, ink) * ground.style.z);
    return over(colour, q.colour.rgb, ink);
}

// The stance dot: a disc of radius outline.x with a soft dark halo of width
// outline.y, antialiased by the pixel footprint in world units.
fn stance_dot(q: GpuQuad, plane: vec2<f32>) -> vec4<f32> {
    let pixel = max(0.7071 * length(vec2<f32>(length(dpdx(plane)), length(dpdy(plane)))), 1e-4);
    let centre = (q.rect.xy + q.rect.zw) * 0.5;
    let d = length(plane - centre) - q.outline.x;
    let halo = exp(-max(d, 0.0) / (q.outline.y * 0.5)) * step(0.0, d)
        * clamp(1.0 - d / q.outline.y, 0.0, 1.0) * ground.style.w;
    let colour = over(vec4<f32>(0.0), vec3<f32>(0.0), halo);
    return over(colour, q.colour.rgb, clamp(0.5 - d / pixel, 0.0, 1.0));
}

@fragment
fn fragment_main(in: VertexOut) -> @location(0) vec4<f32> {
    let q = ground.quads[in.quad];
    // Both are evaluated so every derivative is taken in uniform control flow.
    let as_glyph = glyph(q, in.uv);
    let as_dot = stance_dot(q, in.plane);
    let colour = select(as_glyph, as_dot, q.colour.w > 0.5);
    if (colour.a <= 0.001) {
        discard;
    }
    // Compare view depth with the finished scene at this pixel: a surface
    // clearly in front ghosts the HUD rather than hiding it, and floor
    // unevenness within the tolerance does nothing. An occluder standing on
    // the player's own axis is the player's body: it only softens the HUD,
    // which belongs to that player and must read through them.
    let texel = vec2<i32>(in.position.xy);
    let stored = textureLoad(scene_depth, texel, 0);
    let ndc = vec4<f32>(in.position.x * ground.viewport.z * 2.0 - 1.0,
        1.0 - in.position.y * ground.viewport.w * 2.0, stored, 1.0);
    let scene4 = ground.inverse_view_projection * ndc;
    let scene = scene4.xyz / scene4.w;
    let scene_depth_view = dot(scene - ground.camera.xyz, ground.forward.xyz);
    let own_depth_view = dot(in.world - ground.camera.xyz, ground.forward.xyz);
    let hidden = smoothstep(ground.depth.x, ground.depth.y, own_depth_view - scene_depth_view);
    let rise = scene.z - ground.anchor.z;
    let own_body = length(scene.xy - ground.anchor.xy) < ground.camera.w && rise > -8.0
        && rise < 96.0;
    let covered = select(ground.style.x, ground.forward.w, own_body);
    return colour * mix(1.0, covered, hidden) * ground.style.y;
}
