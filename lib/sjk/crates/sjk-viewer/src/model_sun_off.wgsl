// The authored program has no sun resources, bindings, or arithmetic changes.
fn model_sun_color(input: VertexOutput, original: vec3<f32>) -> vec3<f32> {
    return original;
}
fn realtime_lightmap(input: VertexOutput, texel: vec4<f32>) -> vec4<f32> {
    return texel;
}
fn realtime_active() -> bool { return false; }
