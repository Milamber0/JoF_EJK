// Scene exposure, r_hdrExposure times the eye adaptation, kept on the GPU by
// `post_exposure.rs`; kept in step with its `State`.
struct SceneExposure {
    exposure: f32,
    adapt: f32,
    metered: f32,
    pending: u32,
}
@group(0) @binding(4) var<storage, read> scene_exposure: SceneExposure;

// Identity below peak 0.9; a C1 rational shoulder above it. RGB shares ONE multiplier:
// unlike independent channel curves, hue and linear RGB ratios cannot drift toward grey.
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
fn srgb_decode(c: vec3<f32>) -> vec3<f32> {
    return select(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), c > vec3(0.04045));
}
fn hdr_encoded(c: vec3<f32>) -> vec3<f32> {
    return srgb_encode(hdr_map(max(c * scene_exposure.exposure, vec3(0.0))));
}

// An 8-bit scene (r_sceneHdr 0) is already clipped at white, so its exposure only
// brightens (e >= 1). Linear `c` times e up to an exposed peak of LDR_KNEE, then a C1
// rational shoulder that lands white exactly on white; RGB shares one multiplier as in
// `hdr_map`. e == 1 is the identity, so a neutral exposure leaves the image unchanged.
const LDR_KNEE: f32 = 0.5;
fn ldr_expose(c: vec3<f32>, e: f32) -> vec3<f32> {
    let peak = max(c.r, max(c.g, c.b));
    let x = peak * e;
    if e <= 1.0 || x <= LDR_KNEE { return c * e; }
    let s = (1.0 - LDR_KNEE) / (e - 1.0);
    let t = (x - LDR_KNEE) / (e - LDR_KNEE);
    let mapped = LDR_KNEE + (1.0 - LDR_KNEE) * t * (1.0 + s) / (t + s);
    return c * (mapped / peak);
}
// Inverse of `ldr_expose` for colours it can produce (peak at most 1).
fn ldr_unexpose(m: vec3<f32>, e: f32) -> vec3<f32> {
    let peak = max(m.r, max(m.g, m.b));
    if e <= 1.0 || peak <= LDR_KNEE { return m / e; }
    let s = (1.0 - LDR_KNEE) / (e - 1.0);
    let h = (min(peak, 1.0) - LDR_KNEE) / (1.0 - LDR_KNEE);
    let t = h * s / (1.0 + s - h);
    let x = LDR_KNEE + t * (e - LDR_KNEE);
    return m * (x / (peak * e));
}
// Display value `g` of an 8-bit scene, shown under the current exposure.
fn ldr_exposed(g: vec3<f32>) -> vec3<f32> {
    let e = scene_exposure.exposure;
    if e == 1.0 { return g; }
    return srgb_encode(ldr_expose(srgb_decode(g), e));
}
// Display value that `ldr_exposed` shows as `g`.
fn ldr_unexposed(g: vec3<f32>) -> vec3<f32> {
    let e = scene_exposure.exposure;
    if e == 1.0 { return g; }
    return srgb_encode(ldr_unexpose(srgb_decode(g), e));
}
