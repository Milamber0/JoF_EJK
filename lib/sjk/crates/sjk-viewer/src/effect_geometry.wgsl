struct Camera {
    view_projection: mat4x4<f32>,
    position: vec3<f32>,
    _padding: f32,
    forward: vec3<f32>,
    view_flags: f32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;
@group(1) @binding(0)
var effect_atlas: texture_2d<f32>;
@group(1) @binding(1)
var effect_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) uv_transform: vec4<f32>,
    @location(3) color: vec4<f32>,
}

@vertex
fn vertex_main(
    @location(0) position: vec3<f32>,
    @location(1) local_uv: vec2<f32>,
    @location(2) uv_rect: vec4<f32>,
    @location(3) uv_transform: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) depth_hack: f32,
) -> VertexOutput {
    var output: VertexOutput;
    output.position = camera.view_projection * vec4(position, 1.0);
    output.position.z *= mix(1.0, 0.3, depth_hack);
    if (u32(camera.view_flags) & 1u) != 0u && depth_hack > 0.5 { output.position = vec4(2.0, 2.0,
        2.0, 1.0); }
    output.local_uv = local_uv;
    output.uv_rect = uv_rect;
    output.uv_transform = uv_transform;
    output.color = color;
    return output;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let unwrapped = input.local_uv * input.uv_transform.xy + input.uv_transform.zw;
    let transformed_uv = fract(unwrapped);
    let atlas_uv = mix(input.uv_rect.xy, input.uv_rect.zw, transformed_uv);
    // Mip level from the unwrapped coordinates, so a tile's wrap seam (tcMod
    // scale/scroll) does not jump to the smallest level.
    let span = input.uv_rect.zw - input.uv_rect.xy;
    let texel = textureSampleGrad(effect_atlas, effect_sampler, atlas_uv,
        dpdx(unwrapped) * span, dpdy(unwrapped) * span);
    if texel.a < 0.01 {
        discard;
    }
    return vec4(texel.rgb * input.color.rgb, texel.a * input.color.a);
}
