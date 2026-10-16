// The highlights a glossy surface shows under the day model: needs `camera` and `shadow`.
// A power's base is clamped where rounding could take it below zero: that is a NaN, and a
// NaN pixel in a floating scene is a flash once bloom has spread it.
// Sun highlight on a surface of `gloss` (0 matte .. 1 mirror-like): a normalised
// Blinn-Phong lobe with a Schlick rim, scaled by the sun's visibility at the point.
fn sun_specular(world: vec3<f32>, normal: vec3<f32>, visibility: f32, gloss: f32) -> vec3<f32> {
    if gloss <= 0.0 || visibility <= 0.0 { return vec3(0.0); }
    let view = normalize(camera.camera_position - world);
    let light = shadow.sun.xyz;
    let half = normalize(light + view);
    let facing = max(dot(normal, light), 0.0);
    let ndh = max(dot(normal, half), 0.0);
    let power = mix(16.0, 256.0, gloss*gloss);
    let f0 = mix(0.03, 0.5, gloss);
    let fresnel = f0 + (1.0 - f0)*pow(clamp(1.0 - dot(half, view), 0.0, 1.0), 5.0);
    let lobe = (power + 2.0)/25.132741*pow(ndh, power);
    return shadow.radiance.rgb*shadow.radiance.w*visibility*facing*lobe*fresnel*gloss;
}
// Sky seen in a glossy surface at a grazing angle: the sky radiance through a Schlick rim.
fn sky_reflection(world: vec3<f32>, normal: vec3<f32>, gloss: f32) -> vec3<f32> {
    if gloss <= 0.0 { return vec3(0.0); }
    let view = normalize(camera.camera_position - world);
    let rim = pow(clamp(1.0 - dot(normal, view), 0.0, 1.0), 4.0);
    return shadow.ambient.rgb*gloss*(0.04 + 0.5*rim)*max(normal.z*0.5 + 0.5, 0.0);
}
