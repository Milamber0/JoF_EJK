// Legacy blended-effect layer. rd-vanilla draws every blended stage into an 8-bit,
// display-encoded framebuffer, so each GL_ONE/GL_DST_COLOR blend runs on display values
// and clamps each channel to 1 (tr_backend.cpp GL_State + a UNORM colour buffer). This
// layer is that framebuffer: `fs_encode` fills it with the scene's display values,
// the effect pipelines blend into it, and `fs_write_back` (secondary views) or the final
// resolve (main view, `post_aa.wgsl`) takes back only what the effects changed.
//
// ENCODING: 0 the scene stores display values already (UNORM target), 1 it is an sRGB
// target sampled as linear, 2 it is a floating HDR scene shown through `hdr_encoded`.
// Every encoding shows the scene under `post_hdr.wgsl`'s `scene_exposure`, as the
// final resolve does.
override ENCODING: u32 = 1u;
@group(0) @binding(0) var source: texture_2d<f32>;
// Declared for `post_hdr.wgsl`'s colour cube helper; never read by this shader.
@group(0) @binding(1) var linear_clamp: sampler;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

// The display value the final resolve shows for scene colour `c`, before any colour cube.
fn display(c: vec3<f32>) -> vec3<f32> {
    let unit = clamp(c, vec3(0.0), vec3(1.0));
    if ENCODING == 0u { return clamp(ldr_exposed(unit), vec3(0.0), vec3(1.0)); }
    if ENCODING == 1u {
        return srgb_encode(clamp(ldr_expose(unit, scene_exposure.exposure), vec3(0.0), vec3(1.0)));
    }
    return clamp(hdr_encoded(c), vec3(0.0), vec3(1.0));
}

// Inverse of `hdr_map`'s shoulder: identity below a peak of 0.9. A clamped channel (peak
// 1) has no finite inverse, so the peak is held just under it; the resolve maps it back
// to the same 8-bit value.
fn hdr_unmap(m: vec3<f32>) -> vec3<f32> {
    let peak = max(m.r, max(m.g, m.b));
    if peak <= 0.9 { return m; }
    let k = min(peak, 0.999) - 0.9;
    return m * ((0.9 + 0.1 * k / (0.1 - k)) / peak);
}

// Scene colour that `display` shows as `g` (secondary views only).
fn scene_value(g: vec3<f32>) -> vec3<f32> {
    if ENCODING == 0u { return ldr_unexposed(g); }
    if ENCODING == 1u { return ldr_unexpose(srgb_decode(g), scene_exposure.exposure); }
    return hdr_unmap(srgb_decode(g)) / scene_exposure.exposure;
}

struct Encoded {
    @location(0) blended: vec4<f32>,
    @location(1) original: vec4<f32>,
}

// Both attachments receive the same 8-bit display value; effects then change only one.
@fragment fn fs_encode(@builtin(position) p: vec4<f32>) -> Encoded {
    let c = vec4(display(textureLoad(source, vec2<i32>(p.xy), 0).rgb), 1.0);
    return Encoded(c, c);
}

@group(0) @binding(2) var original: texture_2d<f32>;

// Untouched pixels keep their full scene precision; only effect pixels are replaced.
@fragment fn fs_write_back(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let at = vec2<i32>(p.xy);
    let blended = textureLoad(source, at, 0).rgb;
    if all(blended == textureLoad(original, at, 0).rgb) { discard; }
    return vec4(scene_value(blended), 1.0);
}
