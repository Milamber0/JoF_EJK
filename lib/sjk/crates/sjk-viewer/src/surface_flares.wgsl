// RB_SurfaceFlare / RB_TestZFlare, codemp/rd-vanilla/tr_surface.cpp.
@group(3) @binding(0) var flare_depth: texture_depth_2d;

fn flare_vertex(input: VertexInput, center: vec3<f32>, normal: vec3<f32>) -> VertexOutput {
    let origin = center + normal * 3.0;
    let direction = origin - camera.camera_position;
    let dist = length(direction);
    let brightness = floor(abs(dot(normal, direction / max(dist, 0.0001))) * 255.0) / 255.0;
    let radius = max(5.0, stage.wave_functions.z * min(dist / 512.0, 1.0));
    let vp = camera.view_projection;
    let left = -normalize(vec3(vp[0].x, vp[1].x, vp[2].x)) * radius;
    let up = normalize(vec3(vp[0].y, vp[1].y, vp[2].y)) * radius;
    var vertex = input;
    vertex.position = origin + left * (1.0 - 2.0 * input.texture_coordinates.x) + up * (1.0 - 2.0 *
        input.texture_coordinates.y);
    vertex.color = vec4(vec3(brightness), 1.0);
    var output = vertex_result(vertex, vertex.position, -camera.view_forward, vec4(1.0), vec2(0.0),
        vec2(0.0), no_entity_light(), false);
    let clip = vp * vec4(center, 1.0);
    let ndc = clip.xyz / clip.w;
    let dimensions = vec2<f32>(textureDimensions(flare_depth));
    let pixel = vec2<i32>((ndc.xy * vec2(0.5, -0.5) + 0.5) * dimensions);
    // Stock accepts a blocker within 24 view-space units of the source.
    // Compare against that point's projected depth; also works with oblique portals.
    let tolerance = vp * vec4(center - camera.view_forward * 24.0, 1.0);
    let depth = textureLoad(flare_depth, pixel, 0);
    if clip.w <= 0.0 || any(abs(ndc.xy) >= vec2(1.0)) || ndc.z < 0.0 || ndc.z > 1.0 ||
        (environment.control.w == 1.0 && tolerance.w > 0.0 && depth<tolerance.z / tolerance.w) {
        output.position = vec4(2.0, 2.0, 2.0, 1.0);
    }
    return output;
}
@vertex fn flare_vertex_main(input: VertexInput) -> VertexOutput {
    return flare_vertex(input, input.position, input.normal);
}
@vertex fn instanced_flare_vertex_main(input: VertexInput,
    instance: InstanceInput) -> VertexOutput {
    var output = flare_vertex(input, instance_position(input, instance),
        rotate_vector(instance.rotation, input.normal));
    if !instance_visible(instance) { output.position = vec4(2.0, 2.0, 2.0, 1.0); }
    return output;
}
