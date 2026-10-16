// codemp RB_CalcDeformVertexes / RB_CalcBulgeVertexes / RB_CalcMoveVertexes.
// Evaluate in model space, in authored order, before instancing and tcGen.
fn deform_point(source: VertexInput, i: u32) -> VertexInput {
    var vertex = source;
        let a = stage.deform_a[i];
        let b = stage.deform_b[i];
        let kind = i32(a.x);
        if kind == 1 {
            var phase = b.z;
            if b.w != 0.0 {
                phase += (vertex.position.x + vertex.position.y + vertex.position.z) * a.z;
            }
            let amount = select(b.x + table_value(i32(a.y), phase + (camera.shader_time - stage.emission.z) * b.w) *
                b.y, wave(b, i32(a.y)), b.w == 0.0);
            vertex.position += vertex.normal * amount;
        } else if kind == 2 {
            var amount = b.y;
            if b.x != 0.0 || b.z != 0.0 {
                let phase = (vertex.texture_coordinates.x * b.x + (camera.shader_time - stage.emission.z) * b.z) /
                    6.28318530718;
                amount *= table_value(0, phase);
            }
            vertex.position += vertex.normal * amount;
        } else if kind == 3 {
            vertex.position += stage.deform_c[i].xyz * wave(b, i32(a.y));
        } else if kind == 4 {
            let p = vertex.position * 0.98;
            let t = (camera.shader_time - stage.emission.z) * b.y;
            vertex.normal += b.x * vec3(
                legacy_noise(vec4(p, t)),
                legacy_noise(vec4(p + vec3(100.0, 0.0, 0.0), t)),
                legacy_noise(vec4(p + vec3(200.0, 0.0, 0.0), t)));
            // Match codemp VectorNormalizeFast, including its single Newton step.
            let square = dot(vertex.normal, vertex.normal);
            var inverse = bitcast<f32>(0x5f3759dfu - (bitcast<u32>(square) >> 1u));
            inverse *= 1.5 - (square * 0.5 * inverse * inverse);
            vertex.normal *= inverse;
        } else if kind == 8 {
            // codemp cgame leaves refdef.text[0..7] empty. DeformText clears
            // the tessellator and emits no glyphs, so collapse the source.
            vertex.position = vec3(0.0);
        }
    return vertex;
}

struct QuadRef {
    vertices: vec4<u32>,
    directed_edges: u32,
    valid: u32,
    padding: vec2<u32>,
};
@group(2) @binding(0) var<storage, read> geometry_vertices: array<f32>;
@group(2) @binding(1) var<storage, read> geometry_quads: array<QuadRef>;

fn geometry_vertex(index: u32) -> VertexInput {
    // GpuVertex's packed vertex-fetch ABI: 14 floats, no std430 vec3 padding.
    let p = index * 14u;
    let source = VertexInput(
        vec3(geometry_vertices[p], geometry_vertices[p + 1u], geometry_vertices[p + 2u]),
        vec3(geometry_vertices[p + 3u], geometry_vertices[p + 4u], geometry_vertices[p + 5u]),
        vec4(geometry_vertices[p + 6u], geometry_vertices[p + 7u], geometry_vertices[p + 8u],
            geometry_vertices[p + 9u]),
        vec2(geometry_vertices[p + 10u], geometry_vertices[p + 11u]),
        vec2(geometry_vertices[p + 12u], geometry_vertices[p + 13u]));
    return skinned_vertex(source, index);
}

const QUAD_EDGES = array<vec2<u32>, 6>(vec2(0u, 1u), vec2(0u, 2u), vec2(0u, 3u), vec2(1u, 2u),
    vec2(1u, 3u), vec2(2u, 3u));
const QUAD_UV = array<vec2<f32>, 4>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));

fn sprite_quad(source: array<VertexInput, 4>, rotation: vec4<f32>,
    scale: vec3<f32>) -> array<VertexInput, 4> {
    var q = source;
    let middle = (q[0].position + q[1].position + q[2].position + q[3].position) * 0.25;
    let radius = length(q[0].position - middle) * 0.707;
    let inv = vec4(-rotation.xyz, rotation.w);
    let vp = camera.view_projection;
    let world_left = -normalize(vec3(vp[0].x, vp[1].x, vp[2].x));
    let world_up = normalize(vec3(vp[0].y, vp[1].y, vp[2].y));
    let compensation = select(0.0, 1.0 / abs(scale.x), scale.x != 0.0);
    let left = rotate_vector(inv, world_left) * scale * radius * compensation;
    let up = rotate_vector(inv, world_up) * scale * radius * compensation;
    for (var j = 0u; j < 4u; j++) {
        q[j].position = middle + left * (1.0 - 2.0 * QUAD_UV[j].x) + up * (1.0 - 2.0 *
            QUAD_UV[j].y);
        // RB_AddQuadStamp retains this world-direction normal even inside an
        // entity-local tessellation; preserve that reference quirk.
        q[j].normal = -camera.view_forward;
        q[j].texture_coordinates = QUAD_UV[j];
        q[j].lightmap_coordinates = QUAD_UV[j];
        q[j].color = source[0].color;
    }
    return q;
}

fn axial_sprite_quad(source: array<VertexInput, 4>, reference: QuadRef, rotation: vec4<f32>,
    scale: vec3<f32>) -> array<VertexInput, 4> {
    var q = source;
    var edges = vec2(0u);
    var distances = vec2(999999.0);
    for (var e = 0u; e < 6u; e++) {
        let delta = q[QUAD_EDGES[e].x].position - q[QUAD_EDGES[e].y].position;
        let distance = dot(delta, delta);
        if distance < distances.x {
            edges.y = edges.x; distances.y = distances.x;
            edges.x = e; distances.x = distance;
        } else if distance < distances.y { edges.y = e; distances.y = distance; }
    }
    var middle: array<vec3<f32>, 2>;
    for (var j = 0u; j < 2u; j++) {
        let edge = QUAD_EDGES[edges[j]];
        middle[j] = (q[edge.x].position + q[edge.y].position) * 0.5;
    }
    let forward = rotate_vector(vec4(-rotation.xyz, rotation.w), camera.view_forward) * scale;
    let crossed = cross(middle[1] - middle[0], forward);
    var minor = vec3(0.0);
    if dot(crossed, crossed) > 0.0 { minor = normalize(crossed); }
    for (var j = 0u; j < 2u; j++) {
        let edge = QUAD_EDGES[edges[j]];
        let sign = select(1.0, -1.0, (reference.directed_edges & (1u << edges[j])) != 0u);
        let half_edge = minor * (0.5 * sqrt(distances[j])) * sign;
        q[edge.x].position = middle[j] + half_edge;
        q[edge.y].position = middle[j] - half_edge;
    }
    return q;
}

fn deform_vertex(source: VertexInput, index: u32, rotation: vec4<f32>,
    scale: vec3<f32>) -> VertexInput {
    if !geometry_deforms { return source; }
    var quad = false;
    for (var i = 0u; i < 3u; i++) {
        quad = quad || stage.deform_a[i].x == 5.0 || stage.deform_a[i].x == 6.0;
    }
    var vertex = source;
    if !quad || index >= arrayLength(&geometry_quads) || geometry_quads[index].valid == 0u {
        for (var i = 0u; i < 3u; i++) {
            if stage.deform_a[i].x == 0.0 { break; }
            vertex = deform_point(vertex, i);
        }
        return vertex;
    }
    let reference = geometry_quads[index];
    var vertices: array<VertexInput, 4>;
    var slot = 0u;
    for (var j = 0u; j < 4u; j++) {
        vertices[j] = geometry_vertex(reference.vertices[j]);
        if reference.vertices[j] == index { slot = j; }
    }
    for (var i = 0u; i < 3u; i++) {
        let kind = stage.deform_a[i].x;
        if kind == 0.0 { break; }
        if kind == 5.0 { vertices = sprite_quad(vertices, rotation, scale); }
        else if kind == 6.0 { vertices = axial_sprite_quad(vertices, reference, rotation, scale); }
        else { for (var j = 0u; j < 4u; j++) { vertices[j] = deform_point(vertices[j], i); } }
    }
    return vertices[slot];
}
