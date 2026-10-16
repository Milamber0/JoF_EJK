// Depth gap along camera forward, including oblique mirror projection.
@group(2) @binding(0) var soft_scene_depth: texture_depth_2d;
override soft_blend: u32 = 0u;

fn soft_gap(fragment: vec4<f32>, scene: f32) -> f32 {
    if scene >= 1.0 { return 16.0; }
    let w = 1.0 / fragment.w;
    let origin = camera.view_projection * vec4(camera.position, 1.0);
    if abs(w - origin.w) < 0.000001 { return 16.0; }
    let slope = (fragment.z * w - origin.z) / (w - origin.w);
    let denominator = scene - slope;
    if abs(denominator) < 0.00000001 { return 16.0; }
    let behind = (origin.z - slope * origin.w) / denominator;
    let row = vec3(camera.view_projection[0].w, camera.view_projection[1].w,
        camera.view_projection[2].w);
    return (behind - w) / max(length(row), 0.000001);
}

fn soften(color: vec4<f32>, fade: f32) -> vec4<f32> {
    if soft_blend == 0u || soft_blend == 2u {
        return vec4(color.rgb, color.a * fade);
    }
    if soft_blend == 3u { return mix(vec4(1.0), color, fade); }
    if soft_blend == 4u { return mix(vec4(0.5), color, fade); }
    return color * fade;
}

@fragment fn fragment_soft(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = particle_color(input);
    // Only FX quads, not world icons, clip-space overlays or entity placeholders.
    if input.particle < 0.5 || input.particle > 1.5 { return color; }
    let depth = textureLoad(soft_scene_depth, vec2<i32>(input.clip_position.xy), 0);
    return soften(color, clamp(soft_gap(input.clip_position, depth) / 16.0, 0.0, 1.0));
}
