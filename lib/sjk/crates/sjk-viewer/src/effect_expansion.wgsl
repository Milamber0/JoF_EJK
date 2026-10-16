struct Description {
    info: vec4<u32>, a: vec4<f32>, b: vec4<f32>, right: vec4<f32>, up: vec4<f32>,
    uv: vec4<f32>, transform: vec4<f32>, color: vec4<f32>,
};
struct Point { position: vec4<f32>, uv: vec4<f32> };
@group(0) @binding(0) var<storage, read> descriptions: array<Description>;
@group(0) @binding(1) var<storage, read> points: array<Point>;
// Scalar storage preserves the existing 72-byte vertex layout, without vec3 padding.
@group(0) @binding(2) var<storage, read_write> vertices: array<f32>;
@group(0) @binding(3) var<storage, read_write> indices: array<u32>;

fn store(d: Description, index: u32, position: vec3<f32>, uv: vec2<f32>) {
    let offset = (d.info.y + index) * 18u;
    for (var i = 0u; i < 3u; i++) { vertices[offset + i] = position[i]; }
    vertices[offset + 3u] = uv.x; vertices[offset + 4u] = uv.y;
    for (var i = 0u; i < 4u; i++) {
        vertices[offset + 5u + i] = d.uv[i];
        vertices[offset + 9u + i] = d.transform[i];
        vertices[offset + 13u + i] = d.color[i];
    }
    vertices[offset + 17u] = d.right.w;
}

@compute @workgroup_size(64)
fn expand(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>) {
    let d = descriptions[group.x];
    for (var i = local.x; i < d.info.w; i += 64u) {
        var position: vec3<f32>;
        var uv: vec2<f32>;
        if d.info.x >= 2u {
            let p = points[u32(d.a.x) + i]; position = p.position.xyz; uv = p.uv.xy;
        } else if d.info.x == 1u {
            let at_end = i >= 2u;
            let point = select(d.a, d.b, at_end);
            let width = d.right.xyz * point.w;
            position = select(point.xyz + width, point.xyz - width, (i & 1u) != 0u);
            uv = vec2(f32(i & 1u), select(0.0, 1.0, at_end));
        } else {
            let corner = i % 4u;
            let turn = f32(i / 4u + select(0u, 1u, corner >= 2u)) / d.up.w;
            let point = select(d.a, d.b, corner == 1u || corner == 2u);
            let angle = turn * 6.283185307179586;
            let ring = (d.right.xyz * cos(angle) + d.up.xyz * sin(angle)) * point.w;
            position = point.xyz + ring;
            uv = vec2(turn, select(1.0, 0.0, corner == 1u || corner == 2u));
        }
        store(d, i, position, uv);
    }
    let count = select(d.info.w / 4u * 6u, (d.info.w - 2u) * 3u, d.info.x == 2u);
    for (var i = local.x; i < count; i += 64u) {
        var index: u32;
        if d.info.x == 2u {
            index = select(i / 3u + i % 3u, 0u, i % 3u == 0u);
        } else if d.info.x == 1u {
            let order = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
            index = order[i];
        } else {
            let order = array<u32, 6>(0u, 1u, 2u, 2u, 3u, 0u);
            index = i / 6u * 4u + order[i % 6u];
        }
        indices[d.info.z + i] = d.info.y + index;
    }
}
