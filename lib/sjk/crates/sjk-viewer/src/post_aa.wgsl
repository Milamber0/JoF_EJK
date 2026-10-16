// Edge-directed FXAA: diagonal luma gradient, bounded eight-pixel search direction,
// two/four-tap candidates, rejecting the wider candidate outside the local luma range.
// Filter perceptual color through a UNORM alias. Decode only for an sRGB output attachment.
override SRGB_OUTPUT: bool = true;
override HDR_INPUT: bool = false;
// A scene resolve, exposed through `post_hdr.wgsl`'s `scene_exposure` (binding 4); off
// for the display gamma pass, whose input already holds the HUD.
override SCENE_EXPOSURE: bool = false;
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var linear_clamp: sampler;
@group(0) @binding(2) var<uniform> controls: vec4<f32>;
@group(0) @binding(3) var bloom: texture_2d<f32>;
// The legacy effect layer (`effect_layer.rs`): what the blended effects changed, in
// display values, added after the world's own FXAA, bloom and shoulder.
override EFFECTS: bool = false;
@group(0) @binding(5) var effects_blended: texture_2d<f32>;
@group(0) @binding(6) var effects_original: texture_2d<f32>;
// Texture-space rectangle [u0, v0, u1, v1] the effects can touch this frame.
@group(0) @binding(7) var<uniform> effects_region: vec4<f32>;
// Dynamic glow (`post_glow.rs`): the blurred glow image, display values, and its
// controls [drawn this frame, soft composite, glow alone (r_DynamicGlow 3), unused].
override GLOW: bool = false;
@group(0) @binding(8) var glow_image: texture_2d<f32>;
@group(0) @binding(9) var<uniform> glow_controls: vec4<f32>;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
fn sample_at(uv: vec2<f32>) -> vec3<f32> {
    var c = textureSampleLevel(scene, linear_clamp, uv, 0.0).rgb;
    if HDR_INPUT {
        if controls.w != 0.0 {
            c += controls.w * textureSampleLevel(bloom, linear_clamp, uv, 0.0).rgb;
        }
        return hdr_encoded(c);
    }
    if SCENE_EXPOSURE { return ldr_exposed(c); }
    return c;
}
fn luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
}
fn with_effects(rgb: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    if !EFFECTS || any(uv < effects_region.xy) || any(uv >= effects_region.zw) { return rgb; }
    let blended = textureSampleLevel(effects_blended, linear_clamp, uv, 0.0).rgb;
    let changed = blended - textureSampleLevel(effects_original, linear_clamp, uv, 0.0).rgb;
    // A channel the effects clamped stays clamped: the world's own FXAA/bloom difference
    // must not show through a saturated blade as dark or bright seams.
    return select(clamp(rgb + changed, vec3<f32>(0.0), vec3<f32>(1.0)), blended,
        (blended >= vec3<f32>(1.0) | blended <= vec3<f32>(0.0)) & changed != vec3<f32>(0.0));
}
// rd-vanilla RB_DrawGlowOverlay on display values: r_DynamicGlowSoft's GL_ONE,
// GL_ONE_MINUS_SRC_COLOR is a screen blend; otherwise GL_ONE GL_ONE, clamped.
fn with_glow(rgb: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    // Nothing glowed: the frame is untouched, and the image is not read.
    if !GLOW || (glow_controls.x == 0.0 && glow_controls.z == 0.0) { return rgb; }
    var glow = vec3<f32>(0.0);
    if glow_controls.x != 0.0 { glow = textureSampleLevel(glow_image, linear_clamp, uv, 0.0).rgb; }
    if glow_controls.z != 0.0 { return glow; }
    let scene = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    if glow_controls.y != 0.0 { return scene + glow - scene * glow; }
    return min(scene + glow, vec3<f32>(1.0));
}
fn output_color(rgb: vec3<f32>, alpha: f32, uv: vec2<f32>) -> vec4<f32> {
    var encoded = with_glow(with_effects(rgb, uv), uv);
    if controls.z != 1.0 {
        // OpenJK tr_image.cpp R_SetColorMappings: byte lookup, rounded, no overbright.
        let index = floor(clamp(encoded, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5);
        encoded = floor(255.0 * pow(index / 255.0, vec3<f32>(1.0 / controls.z)) + 0.5) / 255.0;
    }
    var linear = encoded;
    if SRGB_OUTPUT {
        linear = select(encoded / 12.92, pow((encoded + 0.055) / 1.055, vec3<f32>(2.4)),
            encoded > vec3<f32>(0.04045));
    }
    if controls.w != 0.0 && !HDR_INPUT {
        // Bloom was extracted from the ungraded scene; composite after AA, before filmic.
        linear += controls.w * textureSampleLevel(bloom, linear_clamp, uv, 0.0).rgb;
    }
    if controls.y != 0.0 {
        // Narkowicz 2016 public-domain filmic fit, linear input/output. LDR, not HDR recovery.
        linear = clamp((linear * (2.51 * linear + 0.03)) /
            (linear * (2.43 * linear + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(linear, alpha);
}
@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = 1.0 / vec2<f32>(textureDimensions(scene));
    let uv = position.xy * pixel;
    var center = textureLoad(scene, vec2<i32>(position.xy), 0);
    if HDR_INPUT {
        center = vec4(sample_at(uv), center.a);
    } else if SCENE_EXPOSURE {
        center = vec4(ldr_exposed(center.rgb), center.a);
    }
    if controls.x == 0.0 { return output_color(center.rgb, center.a, uv); }
    let nw = luma(sample_at(uv + vec2<f32>(-1.0, -1.0) * pixel));
    let ne = luma(sample_at(uv + vec2<f32>(1.0, -1.0) * pixel));
    let sw = luma(sample_at(uv + vec2<f32>(-1.0, 1.0) * pixel));
    let se = luma(sample_at(uv + vec2<f32>(1.0, 1.0) * pixel));
    let mid = luma(center.rgb);
    let lo = min(mid, min(min(nw, ne), min(sw, se)));
    let hi = max(mid, max(max(nw, ne), max(sw, se)));
    if hi - lo < max(0.0312, hi * 0.125) { return output_color(center.rgb, center.a, uv); }
    let gradient = vec2<f32>(-((nw + ne) - (sw + se)), (nw + sw) - (ne + se));
    let reduce = max((nw + ne + sw + se) * (0.25 * 0.125), 1.0 / 128.0);
    let direction = clamp(gradient / (min(abs(gradient.x), abs(gradient.y)) + reduce),
        vec2<f32>(-8.0), vec2<f32>(8.0)) * pixel;
    let a = 0.5 * (sample_at(uv + direction * (1.0 / 3.0 - 0.5))
        + sample_at(uv + direction * (2.0 / 3.0 - 0.5)));
    let b = a * 0.5 + 0.25 * (sample_at(uv - direction * 0.5)
        + sample_at(uv + direction * 0.5));
    let lb = luma(b);
    return output_color(select(b, a, lb < lo || lb > hi), center.a, uv);
}
