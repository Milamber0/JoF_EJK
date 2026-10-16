struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
@vertex fn vs_main(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>) -> Output {
    return Output(vec4<f32>(position, 0.0, 1.0), uv, color);
}
@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> {
    return textureSample(picture, picture_sampler, input.uv) * input.color;
}
