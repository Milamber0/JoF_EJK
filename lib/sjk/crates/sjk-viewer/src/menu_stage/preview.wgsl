// The classic profile's model preview: the stage actor drawn into a target of
// its own in the scene's format, turned into the display values the UI draws
// (as `effect_layer.wgsl`'s `display` does, at a neutral exposure). Coverage
// comes from depth, so the background is transparent; an edge pixel averages
// its covered 3x3 neighbours and takes their share as alpha, which smooths the
// silhouette by a pixel. The lit blades, already in display values, are added
// last: over the body they add to it, beyond it their brightest channel is the
// alpha (as additive menu art becomes alpha), so the glow shows on any backdrop.
//
// The picture fades out over its outer `EDGE_FADE` (a share of its shorter side), so a
// beam, a puff or a blade reaching past it softens away instead of ending in a straight,
// square cut where the picture stops (Creyon, 13/10/2026).
//
// ENCODING: 0 the scene stores display values already (UNORM target), 1 it is
// an sRGB target sampled as linear, 2 it is a floating HDR scene.
override ENCODING: u32 = 2u;
// TRAIL: the Force page's speed trail's two copies are drawn, each whole into its own
// picture with its own depth (`force_fx.rs`), and laid behind the body at
// `COPY_ALPHAS`, the nearer over the farther.
override TRAIL: bool = false;
const COPY_ALPHAS = vec2<f32>(100.0 / 255.0, 50.0 / 255.0);
const EDGE_FADE: f32 = 0.08;
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var blades: texture_2d<f32>;
@group(0) @binding(3) var near_copy: texture_2d<f32>;
@group(0) @binding(4) var near_depth: texture_depth_2d;
@group(0) @binding(5) var far_copy: texture_2d<f32>;
@group(0) @binding(6) var far_depth: texture_depth_2d;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

// `post_hdr.wgsl`'s shoulder: identity below a peak of 0.9, one multiplier for RGB.
fn hdr_map(c: vec3<f32>) -> vec3<f32> {
    let peak = max(c.r, max(c.g, c.b));
    if peak <= 0.9 { return c; }
    let excess = peak - 0.9;
    let mapped = 0.9 + 0.1 * excess / (excess + 0.1);
    return c * (mapped / peak);
}

fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
    return select(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, c > vec3(0.0031308));
}

fn display(c: vec3<f32>) -> vec3<f32> {
    if ENCODING == 0u { return clamp(c, vec3(0.0), vec3(1.0)); }
    if ENCODING == 1u { return srgb_encode(clamp(c, vec3(0.0), vec3(1.0))); }
    return clamp(srgb_encode(hdr_map(max(c, vec3(0.0)))), vec3(0.0), vec3(1.0));
}

fn covered(at: vec2<i32>) -> bool {
    return textureLoad(depth, at, 0) < 1.0;
}

// The body's display colour and coverage at `at`.
fn body(at: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(scene));
    if covered(at) {
        return vec4(display(textureLoad(scene, at, 0).rgb), 1.0);
    }
    // No body here: the speed trail's copies, faded as wholes, the nearer over the
    // farther. Nothing else beyond the body counts: a shell swelling past it leaves
    // alpha near one over a colour that may be nearly black, and drew a dark rim round
    // the body (Creyon's Sense, 12/10/2026).
    if TRAIL {
        let near = select(0.0, COPY_ALPHAS.x, textureLoad(near_depth, at, 0) < 1.0);
        let far = select(0.0, COPY_ALPHAS.y, textureLoad(far_depth, at, 0) < 1.0);
        let alpha = near + far * (1.0 - near);
        if alpha > 0.0 {
            let colour = display(textureLoad(near_copy, at, 0).rgb) * near
                + display(textureLoad(far_copy, at, 0).rgb) * far * (1.0 - near);
            return vec4(colour / alpha, alpha);
        }
    }
    var sum = vec3(0.0);
    var count = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let near = clamp(at + vec2(x, y), vec2(0), size - 1);
            if covered(near) {
                sum += textureLoad(scene, near, 0).rgb;
                count += 1.0;
            }
        }
    }
    if count == 0.0 { return vec4(0.0); }
    return vec4(display(sum / count), count / 9.0);
}

@fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let at = vec2<i32>(p.xy);
    let model = body(at);
    let light = textureLoad(blades, at, 0).rgb;
    // Premultiplied: the body's colour by its coverage, plus the added light.
    let colour = model.rgb * model.a + light;
    let alpha = clamp(max(model.a, max(light.r, max(light.g, light.b))), 0.0, 1.0);
    if alpha <= 0.0 { return vec4(0.0); }
    return vec4(clamp(colour / alpha, vec3(0.0), vec3(1.0)), alpha * edge_fade(p.xy));
}

// How much of a pixel at `xy` the picture keeps: all of it inside, easing to none at
// the picture's border over its outer `EDGE_FADE`.
fn edge_fade(xy: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(scene));
    let width = max(EDGE_FADE * min(size.x, size.y), 1.0);
    let inside = min(min(xy.x, size.x - xy.x), min(xy.y, size.y - xy.y));
    return smoothstep(0.0, width, inside);
}
