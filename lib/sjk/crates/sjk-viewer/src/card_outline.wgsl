// The player outlines' rings (card_outline.rs): every pixel outside the bodies' mask
// within `radius` pixels of it, in the colour of the nearest body's group, its outer
// edge softened over a pixel. The mask holds each body's group: 1 the card's player,
// 2/3 an enemy, 1/3 a teammate, 0 nothing.
struct Settings {
    card: vec4<f32>,
    ally: vec4<f32>,
    enemy: vec4<f32>,
    // Output size in pixels, the radius in pixels, the lookups along one direction.
    shape: vec4<f32>,
};
@group(0) @binding(0) var mask: texture_2d<f32>;
@group(0) @binding(1) var mask_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: Settings;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

fn group(pixel: vec2<f32>) -> f32 {
    return textureSampleLevel(mask, mask_sampler, pixel / settings.shape.xy, 0.0).r;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = position.xy;
    if group(pixel) > 0.16 {
        discard;
    }
    let radius = settings.shape.z;
    let steps = max(i32(settings.shape.w), 1);
    // Walk out ring by ring, with about one lookup per pixel of each ring's
    // circumference: eight on the first, sixteen at most.
    var nearest = radius + 2.0;
    var found = 0.0;
    for (var step = 1; step <= steps; step++) {
        let distance = radius * f32(step) / f32(steps);
        let directions = clamp(i32(ceil(6.2831853 * distance)), 8, 16);
        for (var i = 0; i < directions; i++) {
            let angle = f32(i) * 6.2831853 / f32(directions);
            let value = group(pixel + vec2<f32>(cos(angle), sin(angle)) * distance);
            if value > 0.16 {
                nearest = distance;
                found = value;
                break;
            }
        }
        if found > 0.0 {
            break;
        }
    }
    let alpha = 1.0 - smoothstep(radius - 1.0, radius + 0.5, nearest);
    if alpha <= 0.0 {
        discard;
    }
    var color = settings.ally;
    if found > 0.83 {
        color = settings.card;
    } else if found > 0.5 {
        color = settings.enemy;
    }
    return vec4<f32>(color.rgb, color.a * alpha);
}
