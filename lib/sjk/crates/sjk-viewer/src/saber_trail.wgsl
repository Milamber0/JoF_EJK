struct Camera {
    view_projection: mat4x4<f32>,
    position: vec3<f32>,
    _padding: f32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var blur_glow: texture_2d<f32>;
@group(1) @binding(1)
var blur_core: texture_2d<f32>;
@group(1) @binding(2)
var sword_trail: texture_2d<f32>;
@group(1) @binding(3)
var trail_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) style: u32,
}

@vertex
fn vertex_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) style: u32,
) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_projection * vec4(position, 1.0);
    // Stock fade (`CTrail::Update`, codemp/cgame/FxPrimitives.cpp:1771-1789):
    // `curST[0] = ST[0] * perc + (ST[0] + 1) * (1 - perc)`, clamped at 1.0,
    // computed per vertex before rasterization; `ST[1]` never moves and the
    // colour is left untouched. `color.a` carries `perc` (remaining life).
    let age = 1.0 - color.a;
    output.uv = vec2(min(uv.x + age, 1.0), uv.y);
    output.color = vec4(color.rgb, 1.0);
    output.style = style;
    return output;
}

// Both stock materials blend GL_ONE GL_ONE, so only rgb matters here:
// `saberTrail` = blurglow (rgbGen vertex) + blurcore (rgbGen identity),
// `swordTrail` = swordtrail (rgbGen vertex).
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.style == 1u {
        let sword = textureSample(sword_trail, trail_sampler, input.uv);
        return vec4(sword.rgb * input.color.rgb, 1.0);
    }
    let glow = textureSample(blur_glow, trail_sampler, input.uv);
    let core = textureSample(blur_core, trail_sampler, input.uv);
    return vec4(glow.rgb * input.color.rgb + core.rgb, 1.0);
}
