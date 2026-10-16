// Shared world/rigid/skinned vertex transforms; skinning is uploaded by the CPU.
struct Camera {
    view_projection: mat4x4<f32>,
    camera_position: vec3<f32>,
    shader_time: f32,
    view_forward: vec3<f32>,
    _padding: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) texture_coordinates: vec2<f32>,
    @location(4) lightmap_coordinates: vec2<f32>,
};

struct InstanceInput {
    // xyz world origin; w = 1.0 applies the RF_DEPTHHACK depth range.
    @location(5) position: vec4<f32>,
    @location(6) rotation: vec4<f32>,
    @location(7) scale: vec3<f32>,
    @location(8) entity_color: vec4<f32>,
    @location(9) shader_tex_coord: vec2<f32>,
    @location(10) entity_control: vec2<f32>,
    @location(11) light_ambient: vec3<f32>,
    @location(12) light_directed: vec3<f32>,
    @location(13) light_direction: vec3<f32>,
    @location(14) fog_index: u32,
    @location(15) view_flags: u32,
};

// CG_AddPacketEntities(qtrue) submits only isPortalEnt entities. Ordinary
// mirrors reuse the scene, with RF_FIRST_PERSON/RF_THIRD_PERSON filtering.
fn instance_visible(instance: InstanceInput) -> bool {
    let view = u32(camera._padding);
    if (view & 4u) != 0u { return (instance.view_flags & 2u) != 0u; }
    if (view & 1u) != 0u { return instance.position.w < 0.5; }
    return (instance.view_flags & 1u) == 0u;
}

fn rotate_vector(rotation: vec4<f32>, value: vec3<f32>) -> vec3<f32> {
    let doubled_cross = 2.0 * cross(rotation.xyz, value);
    return value + rotation.w * doubled_cross + cross(rotation.xyz, doubled_cross);
}

fn instance_position(input: VertexInput, instance: InstanceInput) -> vec3<f32> {
    return instance.position.xyz
        + rotate_vector(instance.rotation, input.position * instance.scale);
}

fn clip_position(position: vec3<f32>, depth_hack: f32) -> vec4<f32> {
    var clip = camera.view_projection * vec4(position, 1.0);
    // RF_DEPTHHACK, codemp/rd-vanilla/tr_backend.cpp:906.
    clip.z *= mix(1.0, 0.3, depth_hack);
    return clip;
}
