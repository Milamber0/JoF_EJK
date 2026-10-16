// Dynamic glow blur (`post_glow.rs`). Every image is 8-bit UNORM, so each pass clamps
// to [0, 1] and quantises, as stock's framebuffer copies did.
struct Params {
    // r_DynamicGlowDelta, retail tap weight (Intensity / 4), Vulkan level factor, unused.
    live: vec4<f32>,
    // Reciprocal retail blur size, Vulkan centre and side tap weights.
    sizes: vec4<f32>,
    // Vulkan passes (horizontal, vertical per level) then the level sum: uv tap offset,
    // reciprocal target size.
    passes: array<vec4<f32>, 9>,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_clamp: sampler;
@group(0) @binding(2) var<uniform> params: Params;
// The vertical result of pyramid levels 1-3; level 0 is `source` in `vulkan_combine`.
@group(0) @binding(3) var level1: texture_2d<f32>;
@group(0) @binding(4) var level2: texture_2d<f32>;
@group(0) @binding(5) var level3: texture_2d<f32>;

struct Varyings {
    @builtin(position) position: vec4<f32>,
    // The pass being drawn, from the instance index.
    @location(0) @interpolate(flat) pass_index: u32,
}

@vertex fn vs_main(@builtin(vertex_index) index: u32,
    @builtin(instance_index) pass_index: u32) -> Varyings {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Varyings(vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0), pass_index);
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(source, linear_clamp, uv, 0.0).rgb;
}

// rd-vanilla RB_BlurGlowTexture: four diagonal taps (0.1 + pass * delta) source texels
// away, each weighted Intensity / 4. Pass 0 reads the full-size glow image, so its taps
// sit 0.1 full-size texels around the blur texel's centre; later passes read the
// previous pass at blur size.
@fragment fn retail(v: Varyings) -> @location(0) vec4<f32> {
    let uv = v.position.xy * params.sizes.xy;
    let size = vec2<f32>(textureDimensions(source));
    let o = vec2<f32>(0.1 + f32(v.pass_index) * params.live.x) / size;
    let sum = tap(uv + vec2(-o.x, -o.y)) + tap(uv + vec2(-o.x, o.y))
        + tap(uv + vec2(o.x, -o.y)) + tap(uv + o);
    return vec4<f32>(sum * params.live.y, 1.0);
}

// rd-vulkan blur.frag: three taps 1.2 target texels apart along one axis, weights
// 6/16 and 5/16 each raised by 0.15. The horizontal pass of a level also halves the
// previous level (linear filtering).
@fragment fn vulkan_blur(v: Varyings) -> @location(0) vec4<f32> {
    let pass_params = params.passes[v.pass_index];
    let uv = v.position.xy * pass_params.zw;
    let c = tap(uv) * params.sizes.z
        + (tap(uv + pass_params.xy) + tap(uv - pass_params.xy)) * params.sizes.w;
    return vec4<f32>(c, 1.0);
}

// rd-vulkan blend.frag: the sum of the four levels times (Intensity - 1), here at half
// size; the resolve adds it with GL_ONE GL_ONE, which clamps exactly as this 8-bit
// image does.
@fragment fn vulkan_combine(v: Varyings) -> @location(0) vec4<f32> {
    let uv = v.position.xy * params.passes[v.pass_index].zw;
    let sum = tap(uv) + textureSampleLevel(level1, linear_clamp, uv, 0.0).rgb
        + textureSampleLevel(level2, linear_clamp, uv, 0.0).rgb
        + textureSampleLevel(level3, linear_clamp, uv, 0.0).rgb;
    return vec4<f32>(sum * params.live.z, 1.0);
}
