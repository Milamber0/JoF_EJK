// Invisible brushes (r_showBrushes): the faces of clip, trigger, ladder and
// other tool brushes, drawn see-through over the finished world like Radiant
// shows them. Faces carry their kind's tile (label and frame), projected on
// the face's main axis as Radiant's default texture projection does; edges
// outline each face. Scene depth is tested, never written.

const KINDS: u32 = 24u;
// World units one tile covers: a 128-pixel editor image at Radiant's default
// 0.5 scale.
const TILE: f32 = 64.0;
// Faces are lifted off their plane toward the viewer, so a clip brush flush
// with a wall is not lost in the wall's depth.
const LIFT: f32 = 0.35;

struct Camera {
    view_projection: mat4x4<f32>,
    camera_position: vec3<f32>,
    shader_time: f32,
    view_forward: vec3<f32>,
    flags: f32,
};

struct Look {
    // Linear rgb of each kind; w unused.
    colours: array<vec4<f32>, KINDS>,
    // Fill, frame, label and edge opacity.
    opacity: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> look: Look;
@group(1) @binding(1) var tiles: texture_2d_array<f32>;
@group(1) @binding(2) var tile_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) kind: u32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) normal: vec3<f32>,
    @location(2) @interpolate(flat) kind: u32,
};

@vertex
fn vertex_main(vertex: VertexIn) -> VertexOut {
    // Toward the side of the face the viewer is on.
    let side = select(-1.0, 1.0, dot(camera.camera_position - vertex.position, vertex.normal) >= 0.0);
    let world = vertex.position + vertex.normal * (LIFT * side);
    var out: VertexOut;
    out.clip = camera.view_projection * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = vertex.normal;
    out.kind = min(vertex.kind, KINDS - 1u);
    return out;
}

// Texture coordinates on the face's main axis, mirrored so the label reads
// left to right from the side the viewer is on.
fn face_uv(world: vec3<f32>, normal: vec3<f32>) -> vec2<f32> {
    let facing = select(-normal, normal, dot(camera.camera_position - world, normal) >= 0.0);
    let n = abs(facing);
    if n.z >= n.x && n.z >= n.y {
        return vec2<f32>(world.x, -world.y * sign(facing.z));
    }
    if n.x >= n.y {
        return vec2<f32>(world.y * sign(facing.x), -world.z);
    }
    return vec2<f32>(-world.x * sign(facing.y), -world.z);
}

@fragment
fn face_main(in: VertexOut) -> @location(0) vec4<f32> {
    let mask = textureSample(tiles, tile_sampler, face_uv(in.world, in.normal) / TILE, in.kind);
    let colour = look.colours[in.kind].rgb;
    let label = mask.r;
    let frame = mask.g;
    let alpha = max(look.opacity.x, max(frame * look.opacity.y, label * look.opacity.z));
    // The label is a lighter shade of the kind's colour.
    let rgb = mix(colour, mix(colour, vec3<f32>(1.0), 0.65), label);
    return vec4<f32>(rgb * alpha, alpha);
}

@fragment
fn edge_main(in: VertexOut) -> @location(0) vec4<f32> {
    let alpha = look.opacity.w;
    return vec4<f32>(look.colours[in.kind].rgb * alpha, alpha);
}
