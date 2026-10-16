// Linear-light LDR brightness bloom. Peak RGB preserves saturated saber colors.
override HORIZONTAL: bool = false;
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_clamp: sampler;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn extract(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let base = vec2<i32>(p.xy) * 4;
    var sum = vec3<f32>(0.0);
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            // Clamp partial blocks at the right/bottom edge, never sample outside the image.
            let at = min(base + vec2<i32>(x, y), size - 1);
            let uv = (vec2<f32>(at) + 0.5) / vec2<f32>(size);
            let c = textureSampleLevel(source, linear_clamp, uv, 0.0).rgb;
            let peak = max(c.r, max(c.g, c.b));
            sum += c * (max(peak - 0.8, 0.0) / max(peak, 0.00001));
        }
    }
    return vec4<f32>(sum / 16.0, 1.0);
}

@fragment fn blur(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = 1.0 / vec2<f32>(textureDimensions(source));
    let uv = p.xy * pixel;
    let direction = select(vec2<f32>(0.0, pixel.y), vec2<f32>(pixel.x, 0.0), HORIZONTAL);
    // Normalized nine-tap Gaussian, sigma=2 quarter-size texels (8 full-size pixels).
    var weights = array<f32, 5>(0.204164, 0.180174, 0.123832, 0.066282, 0.027630);
    var c = textureSampleLevel(source, linear_clamp, uv, 0.0).rgb * weights[0];
    for (var i = 1; i < 5; i++) {
        let offset = direction * f32(i);
        c += weights[i] * (textureSampleLevel(source, linear_clamp, uv + offset, 0.0).rgb
            + textureSampleLevel(source, linear_clamp, uv - offset, 0.0).rgb);
    }
    return vec4<f32>(c, 1.0);
}
