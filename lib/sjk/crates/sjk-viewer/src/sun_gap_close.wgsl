// Morphological close of a sun shadow map: a separable minimum (casters widen by the
// radius, holes narrower than twice the radius fill with the nearer neighbour) followed by
// a separable maximum (casters shrink back). Slits between surfaces that meet without
// overlap stop passing sun; caster outlines keep their place.
struct Pass { direction: vec2<i32>, radius: i32, minimum: u32 };
@group(0) @binding(0) var source: texture_depth_2d;
@group(0) @binding(1) var<uniform> pass_: Pass;

@vertex fn fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(positions[index], 0.0, 1.0);
}

@fragment fn close(@builtin(position) position: vec4<f32>) -> @builtin(frag_depth) f32 {
    let size = vec2<i32>(textureDimensions(source));
    let pixel = vec2<i32>(position.xy);
    var value = textureLoad(source, pixel, 0);
    for (var i = -pass_.radius; i <= pass_.radius; i++) {
        let at = clamp(pixel + pass_.direction*i, vec2(0), size - 1);
        let depth = textureLoad(source, at, 0);
        value = select(max(value, depth), min(value, depth), pass_.minimum != 0u);
    }
    return value;
}
