// Optional camera-local dust motes (r_dustMotes). Every mote is derived from its
// instance index and the frame time: no vertex or instance buffer exists.
struct Camera {
    view_projection: mat4x4<f32>,
    position: vec3<f32>,
    shader_time: f32,
    forward: vec3<f32>,
    view_flags: f32,
};

// Peak opacity (already capped on the CPU); remaining lanes are padding.
struct Dust {
    opacity: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> dust: Dust;

// Edge of the wrapping cube around the camera, in world units. The visible sphere
// ends inside it, so a mote wrapping across a face is never seen popping.
const BOX: f32 = 640.0;
// Distance fade: full opacity inside x, gone at y.
const FAR_FADE: vec2<f32> = vec2(150.0, 300.0);
// Motes near the eye fade out: nothing large ever sits in front of the view.
const NEAR_FADE: vec2<f32> = vec2(16.0, 48.0);
const RADIUS: f32 = 0.8;

struct DustOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) color: vec3<f32>,
};

// PCG hash (Jarzynski and Olano, 2020), three decorrelated unit values per input.
fn pcg(value: u32) -> u32 {
    let state = value * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn hash3(value: u32) -> vec3<f32> {
    let a = pcg(value);
    let b = pcg(a);
    let c = pcg(b);
    return vec3(f32(a), f32(b), f32(c)) * (1.0 / 4294967296.0);
}

// Two triangles: corners (-,-) (+,-) (+,+) and (-,-) (+,+) (-,+), as bit masks.
fn corner(vertex: u32) -> vec2<f32> {
    let x = f32((0x16u >> vertex) & 1u);
    let y = f32((0x34u >> vertex) & 1u);
    return vec2(x, y) * 2.0 - 1.0;
}

@vertex fn vertex_main(@builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32) -> DustOutput {
    var output: DustOutput;
    output.corner = corner(vertex);
    output.alpha = 0.0;
    output.color = vec3(0.0);
    output.position = vec4(2.0, 2.0, 2.0, 1.0);
    // Secondary views and sky-portal scenes keep their own presentation.
    if (u32(camera.view_flags) & 5u) != 0u { return output; }

    let seed = hash3(instance);
    let motion = hash3(instance ^ 0x9e3779b9u);
    let t = camera.shader_time;
    // Slow drift with a gentle settle, plus a per-mote sway of a few units.
    let drift = ((motion - 0.5) * vec3(10.0, 10.0, 4.0) - vec3(0.0, 0.0, 0.6)) * t;
    let sway = sin(t * (0.2 + 0.4 * motion.zxy) + seed.yzx * 6.2831853) * 5.0;
    let anchor = seed * BOX + drift + sway;
    // World-fixed motes: moving the camera moves through them, wrapping in the cube.
    let local = (fract((anchor - camera.position) / BOX) - 0.5) * BOX;
    let range = length(local);
    let fade = (1.0 - smoothstep(FAR_FADE.x, FAR_FADE.y, range))
        * smoothstep(NEAR_FADE.x, NEAR_FADE.y, range);
    if fade <= 0.0 { return output; }

    let center = camera.position + local;
    let beam = dust_beam(center, camera.view_projection*vec4(center,1.0));
    if beam.a <= 0.0 { return output; }
    output.color = beam.rgb;

    let forward = normalize(camera.forward);
    var right = cross(forward, vec3(0.0, 0.0, 1.0));
    if dot(right, right) < 1e-4 { right = vec3(0.0, 1.0, 0.0); }
    right = normalize(right);
    let up = cross(right, forward);
    let size = RADIUS * (0.6 + 0.8 * motion.x);
    let world = camera.position + local + (output.corner.x * right + output.corner.y * up) * size;
    // A slow shimmer, as motes turn and catch the light.
    let shimmer = 0.6 + 0.4 * sin(t * (0.7 + motion.y) + seed.x * 6.2831853);
    output.position = camera.view_projection * vec4(world, 1.0);
    output.alpha = dust.opacity.x * fade * shimmer * beam.a;
    return output;
}

@fragment fn fragment_main(input: DustOutput) -> @location(0) vec4<f32> {
    let falloff = max(1.0 - dot(input.corner, input.corner), 0.0);
    let alpha = input.alpha * falloff * falloff;
    // Premultiplied: the pipeline blends One / OneMinusSrcAlpha.
    return vec4(input.color * alpha, alpha);
}
