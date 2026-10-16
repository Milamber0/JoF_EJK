@group(0) @binding(0) var source: texture_2d<f32>;
override SCALE: u32 = 2u;
@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0));
    return vec4(positions[index], 0.0, 1.0);
}
@fragment fn fragment(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let origin = vec2<i32>(pixel.xy) * i32(SCALE);
    var total = vec4(0.0);
    for (var y = 0u; y < SCALE; y++) {
        for (var x = 0u; x < SCALE; x++) {
            total += textureLoad(source, origin + vec2<i32>(i32(x),i32(y)), 0);
        }
    }
    return total / f32(SCALE * SCALE);
}
