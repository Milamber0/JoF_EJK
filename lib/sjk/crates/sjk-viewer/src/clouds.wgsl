// Volumetric clouds, drawn on the visible sky faces after the sky: each pixel marches
// its view ray through a layer of cloud above the camera, made of the noise volume
// drifting with the wind, lit by the map's sun and the sky, and is blended over the
// sky in the scene's light units (premultiplied).
struct Camera {
    view_projection: mat4x4<f32>,
    position: vec3<f32>,
    shader_time: f32,
    forward: vec3<f32>,
    view_flags: f32,
};

struct Clouds {
    // xyz unit direction towards the sun, w its strength (0 at night).
    sun: vec4<f32>,
    // rgb sunlight, w coverage (0 clear to 1 overcast).
    sun_color: vec4<f32>,
    // rgb skylight, w darkness (0 fair to 1 storm).
    ambient: vec4<f32>,
    // xyz noise offset in world units (the wind's drift), w 1 once the noise is made.
    flow: vec4<f32>,
    // x layer base above the camera, y thickness, z samples, w unused.
    shape: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> clouds: Clouds;
@group(1) @binding(1) var noise: texture_3d<f32>;
@group(1) @binding(2) var noise_sampler: sampler;

// Extinction per unit at full density, through the layer and towards the sun.
const SIGMA: f32 = 0.0035;
const SIGMA_LIGHT: f32 = 0.0025;
// One noise tile: the shapes across, their height, the eroding detail.
const SHAPE_SCALE: vec3<f32> = vec3(1.0 / 36000.0, 1.0 / 36000.0, 1.0 / 9000.0);
const DETAIL_SCALE: f32 = 1.0 / 6500.0;
const PI: f32 = 3.14159265;

// A sky face's depth, pushed back by `SKY_FACE_PUSH` of its distance (the clip-space
// share between its depth and the far plane's). The world's absolute coordinates round
// the depth of a far face by some tenths of a percent of its distance, so a ceiling a
// unit or two under the sky brush it is built against fought with the sky (and the
// clouds over it) and flickered as the camera moved (jofmemorial, Sol's note of
// 12/10/2026). rd-vanilla draws its box at the far plane and never covers the world;
// pushed this little, the sky still covers whatever lies further behind its faces.
const SKY_FACE_PUSH: f32 = 1.0 / 32.0;

fn sky_face_clip(clip: vec4<f32>, push: f32) -> vec4<f32> {
    return vec4(clip.xy, clip.z + push * (clip.w - clip.z), clip.w);
}

struct FaceInput {
    @location(0) position: vec3<f32>,
};

struct FaceOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) direction: vec3<f32>,
};

@vertex fn vertex_main(input: FaceInput) -> FaceOutput {
    var output: FaceOutput;
    // Half as far behind the face as the sky (`sky_stage.wgsl`'s `SKY_FACE_PUSH`), so
    // the depth test passes on exactly the sky's pixels whatever the two programs round,
    // and still fails on a ceiling just under the sky brush. A fixed step nearer (it was
    // 0.000002 of depth) is thousands of units at the far end of a big map.
    output.position = sky_face_clip(camera.view_projection * vec4<f32>(input.position, 1.0),
        SKY_FACE_PUSH * 0.5);
    output.direction = input.position - camera.position;
    return output;
}

// Henyey-Greenstein, normalised so an isotropic medium gives 1.
fn phase(cosine: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / pow(1.0 + g2 - 2.0 * g * cosine, 1.5);
}

// Cloud density at `p`, 0..1: the coverage-remapped shape, rounded off at the layer's
// base and top, its edges eroded by the detail noise.
fn density(p: vec3<f32>, base: f32, thickness: f32) -> f32 {
    let height = clamp((p.z - base) / thickness, 0.0, 1.0);
    let profile = smoothstep(0.0, 0.15, height) * (1.0 - smoothstep(0.5, 1.0, height));
    let q = p + clouds.flow.xyz;
    let shape = textureSampleLevel(noise, noise_sampler, q * SHAPE_SCALE, 0.0).r;
    let coverage = clouds.sun_color.w;
    var value = clamp((shape * profile - (1.0 - coverage)) / max(coverage, 0.05), 0.0, 1.0);
    if value <= 0.0 { return 0.0; }
    let detail = textureSampleLevel(noise, noise_sampler, q * DETAIL_SCALE, 0.0).gba;
    let erosion = dot(detail, vec3(0.625, 0.25, 0.125));
    value = clamp(value - (1.0 - erosion) * 0.4 * (1.0 - value), 0.0, 1.0);
    return value;
}

@fragment fn fragment_main(input: FaceOutput) -> @location(0) vec4<f32> {
    let direction = normalize(input.direction);
    if direction.z < 0.01 || clouds.flow.w == 0.0 { return vec4(0.0); }
    let thickness = clouds.shape.y;
    let base = camera.position.z + clouds.shape.x;
    let enter = clouds.shape.x / direction.z;
    let leave = min((clouds.shape.x + thickness) / direction.z, enter + 30000.0);
    let steps = max(u32(clouds.shape.z), 1u);
    let step = (leave - enter) / f32(steps);
    // Interleaved gradient noise: a per-pixel start that turns banding into fine grain.
    let jitter = fract(52.9829189 * fract(dot(input.position.xy, vec2(0.06711056, 0.00583715))));
    let sun = clouds.sun.xyz;
    let cosine = dot(direction, sun);
    // A bright rim towards the sun, a little light scattered back.
    let scatter = mix(phase(cosine, 0.6), phase(cosine, -0.25), 0.35);
    var transmittance = 1.0;
    var light = vec3(0.0);
    for (var i = 0u; i < steps; i++) {
        let p = camera.position + direction * (enter + (f32(i) + jitter) * step);
        let d = density(p, base, thickness);
        if d <= 0.004 { continue; }
        // Light reaching the sample: two looks towards the sun.
        let towards = density(p + sun * thickness * 0.12, base, thickness)
            + density(p + sun * thickness * 0.4, base, thickness);
        let beer = exp(-towards * thickness * 0.26 * SIGMA_LIGHT);
        // Dense insides darken at grazing light ("powder").
        let powder = 1.0 - exp(-d * 3.0);
        let height = clamp((p.z - base) / thickness, 0.0, 1.0);
        let lit = clouds.sun_color.rgb * clouds.sun.w * beer * scatter * mix(0.6, 1.0, powder)
            + clouds.ambient.rgb * mix(0.55, 1.0, height);
        let absorbed = exp(-d * SIGMA * step);
        light += transmittance * lit * (1.0 - absorbed);
        transmittance *= absorbed;
        if transmittance < 0.02 { break; }
    }
    light *= 1.0 - 0.6 * clouds.ambient.w;
    // Thin out towards the horizon and with distance, where the sky takes over.
    let fade = smoothstep(0.01, 0.12, direction.z) * (1.0 - smoothstep(20000.0, 60000.0, enter));
    return vec4(light * fade, (1.0 - transmittance) * fade);
}
