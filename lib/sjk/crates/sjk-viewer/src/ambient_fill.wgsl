// A material-lighting floor, not a screen colour lift. Broad suppression keeps the
// total response monotonic: adding a light must never make a surface darker.
fn readability_fill(lighting: vec3<f32>, tint_amount: vec4<f32>) -> vec3<f32> {
    let amount = tint_amount.w;
    if amount <= 0.0 { return vec3(0.0); }
    let luminance = max(dot(lighting, vec3(0.2126, 0.7152, 0.0722)), 0.0);
    let weight = 4.0*amount/(4.0*amount + luminance);
    return tint_amount.rgb*amount*weight*weight;
}

// Artistic gain on existing sky/bounce, not extra bounces or direct-light energy.
// controls = (indirect gain, fraction of AO applied to fill). Defaults preserve
// the original (indirect + fill)*AO operation order. Keep real indirect occluded.
fn readable_indirect(direct: vec3<f32>, indirect: vec3<f32>, occlusion: f32,
    tint_amount: vec4<f32>, controls: vec2<f32>) -> vec3<f32> {
    let boosted = indirect*controls.x;
    let fill = readability_fill(direct + boosted, tint_amount);
    var result = (boosted + fill)*occlusion;
    if controls.y < 1.0 {
        result += fill*((1.0 - controls.y)*(1.0 - occlusion));
    }
    return result;
}
