
// Stage table entry points (`stage_table.rs`): the instance index names the stage record.
// Static world geometry only: no instance transform, no entity light.
@vertex fn table_vertex_main(input: VertexInput, @builtin(vertex_index) index: u32,
    @builtin(instance_index) record: u32) -> VertexOutput {
    table_record = record;
    table_generators = stage_records[record].stage.generators;
    table_animation = stage_records[record].stage.animation;
    table_secondary_control = stage_records[record].stage.secondary_control;
    table_emission = stage_records[record].stage.emission;
    let vertex = material_vertex(input, index, vec3(0.0), vec4(0.0, 0.0, 0.0, 1.0), vec3(1.0));
    var output = vertex_result(vertex, vertex.position, normalize(vertex.normal), vec4(1.0),
        vec2(0.0), vec2(0.0), no_entity_light(), false);
    output.record_animation.x = i32(record);
    // Fragments read generators.xyz and, of secondary_control, the combine mode (y: 0..2)
    // and two 0/1 flags (z, w).
    output.table_packed = vec4(table_generators.xyz,
        step(0.5, table_secondary_control.z) + 2.0*step(0.5, table_secondary_control.w)
            + 4.0*floor(table_secondary_control.y));
    output.table_animation_out = table_animation;
    output.table_emission_out = table_emission;
    output.table_images = stage_records[record].images;
    return output;
}
@fragment fn table_fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return table_fragment_main_body(input);
}
fn table_fragment_main_body(input: VertexOutput) -> vec4<f32> {
    table_record = u32(input.record_animation.x);
    table_index = input.table_images;
    let flags = u32(input.table_packed.w);
    table_generators = vec4(input.table_packed.xyz, 0.0);
    table_secondary_control = vec4(0.0, f32(flags >> 2u), f32(flags & 1u), f32((flags >> 1u) & 1u));
    table_animation = input.table_animation_out;
    table_emission = input.table_emission_out;
    return stage_fragment(input);
}

// Entities keep their instance index for the transform: an immediate names the record.
struct TableDraw { record: u32 };
var<immediate> table_draw: TableDraw;
@vertex fn table_entity_vertex_main(input: VertexInput, instance: InstanceInput,
    @builtin(vertex_index) index: u32) -> VertexOutput {
    let record = table_draw.record;
    table_record = record;
    table_generators = stage_records[record].stage.generators;
    table_animation = stage_records[record].stage.animation;
    table_secondary_control = stage_records[record].stage.secondary_control;
    table_emission = stage_records[record].stage.emission;
    var output = entity_vertex(input, instance, index);
    output.record_animation.x = i32(record);
    output.table_packed = vec4(table_generators.xyz,
        step(0.5, table_secondary_control.z) + 2.0*step(0.5, table_secondary_control.w)
            + 4.0*floor(table_secondary_control.y));
    output.table_animation_out = table_animation;
    output.table_emission_out = table_emission;
    output.table_images = stage_records[record].images;
    return output;
}
@fragment fn table_entity_fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return table_fragment_main_body(input);
}
