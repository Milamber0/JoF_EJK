struct SkinVertex {
    position: vec4<f32>, normal: vec4<f32>, joints: vec4<u32>, weights: vec4<f32>,
};
struct SkinJoint { rows: array<vec4<f32>, 3>, valid: vec4<f32> };
@group(2) @binding(3) var<storage, read> skin_lookup: array<u32>;
@group(2) @binding(4) var<storage, read> skin_vertices: array<SkinVertex>;
@group(2) @binding(5) var<storage, read> skin_joints: array<SkinJoint>;

/// One joint row (without its translation) applied to a point or direction, summed
/// left to right.
fn skin_row(row: vec4<f32>, value: vec4<f32>) -> f32 {
    return ((0.0 + row.x * value.x) + row.y * value.y) + row.z * value.z;
}

fn skinned_vertex(source: VertexInput, index: u32) -> VertexInput {
    if index >= arrayLength(&skin_lookup) { return source; }
    let key = skin_lookup[index];
    if key == 0u { return source; }
    let vertex = skin_vertices[key - 1u];
    if skin_joints[vertex.joints.x].valid.x == 0.0 { return source; }
    var position = vec3(0.0);
    var normal = vec3(0.0);
    for (var i = 0u; i < u32(vertex.position.w); i++) {
        let joint = skin_joints[vertex.joints[i]];
        // Whole-vector updates, never `position[axis] += ...`: FXC (DX12 without
        // dxcompiler.dll) assigns a runtime-indexed component only by unrolling the
        // loops around it, which fails inside this runtime-bounded loop (X3511). Each
        // component still takes the same operations in the same order.
        let p = vec3(
            skin_row(joint.rows[0], vertex.position),
            skin_row(joint.rows[1], vertex.position),
            skin_row(joint.rows[2], vertex.position),
        );
        let n = vec3(
            skin_row(joint.rows[0], vertex.normal),
            skin_row(joint.rows[1], vertex.normal),
            skin_row(joint.rows[2], vertex.normal),
        );
        let offset = vec3(joint.rows[0].w, joint.rows[1].w, joint.rows[2].w);
        position += vertex.weights[i] * (offset + p);
        normal += vertex.weights[i] * n;
    }
    let length = sqrt((normal.x * normal.x + normal.y * normal.y) + normal.z * normal.z);
    if length > 0.00000011920928955078125 { normal /= length; }
    var result = source;
    result.position = position;
    result.normal = normal;
    return result;
}
