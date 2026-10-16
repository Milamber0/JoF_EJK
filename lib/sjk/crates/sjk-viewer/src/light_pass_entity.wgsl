// Entity side of the light buffer: the stage program's own vertex path (skinning, deforms,
// the view-weapon depth hack) so the pre-pass depth equals the scene's, and the stage's
// alpha test on its first texture so cut-out cards leave their holes.
@vertex fn entity_light_vertex(input: VertexInput, instance: InstanceInput,
    @builtin(vertex_index) index: u32) -> LightOutput {
    let vertex = material_vertex(input, index, instance.position.xyz, instance.rotation,
        instance.scale);
    let position = instance_position(vertex, instance);
    var result: LightOutput;
    result.position = clip_position(position, instance.position.w);
    if !instance_visible(instance) { result.position = vec4(2.0, 2.0, 2.0, 1.0); }
    result.world = position;
    result.normal = normalize(rotate_vector(instance.rotation, vertex.normal));
    result.uv = vertex.texture_coordinates;
    return result;
}
@fragment fn entity_normal_fragment(input: LightOutput) -> @location(0) vec4<f32> {
    let alpha_test = i32(stage.generators.z);
    if alpha_test != 0 {
        let alpha = textureSample(stage_images, stage_sampler, input.uv, 0).a;
        if alpha_test == 1 && alpha <= 0.0 { discard; }
        if alpha_test == 2 && alpha >= (128.0 / 255.0) { discard; }
        if alpha_test == 3 && alpha < (128.0 / 255.0) { discard; }
        if alpha_test == 4 && alpha < (192.0 / 255.0) { discard; }
    }
    return packed_normal(input);
}
