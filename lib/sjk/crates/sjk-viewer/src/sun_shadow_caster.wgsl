// The exact render-path skinning function is concatenated before this entry point.
@vertex fn world_shadow_vertex(input: VertexInput) -> @builtin(position) vec4<f32> {
    return camera.view_projection * vec4(input.position, 1.0);
}

@vertex fn shadow_vertex(input: VertexInput, instance: InstanceInput,
    @builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let vertex = skinned_vertex(input, index);
    var result = camera.view_projection * vec4(instance_position(vertex, instance), 1.0);
    if instance.position.w != 0.0 || !instance_visible(instance) {
        result = vec4(2.0, 2.0, 2.0, 1.0);
    }
    return result;
}
