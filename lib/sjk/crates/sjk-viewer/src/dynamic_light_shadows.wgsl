// Point-light shadow tiles (`dynamic_light_shadows.rs`): every texel of every light's tile,
// traced against the static triangles the lamps' visibility uses (`lamp_geometry.wgsl`,
// group 0). Glass, grates and sky let light through, as for the lamps.
struct TracedLight { origin_radius: vec4<f32>, color: vec4<f32> };
// The head of the scene's point-light block (`point_lights.wgsl`): the lights and their count.
struct TracedLights { lights: array<TracedLight, 32>, metadata: vec4<u32> };
@group(1) @binding(0) var<uniform> traced: TracedLights;
// The header (lights with a tile), then the tiles, two texels to a word: copied whole into
// the block after its CPU part.
@group(1) @binding(1) var<storage, read_write> shadow_tiles: array<u32>;

// One texel: the 255th of the reach where the ray stops, rounded down (the receivers round
// up, so a stored hit is never nearer than the real one), and what it stopped on.
fn point_light_trace_texel(source: vec4<f32>, reach: f32, texel: u32) -> u32 {
    let pixel = vec2<i32>(vec2(texel % POINT_TILE_EDGE, texel / POINT_TILE_EDGE));
    let direction = point_light_texel_direction(pixel);
    let hit = gi_trace_through(source.xyz + direction*POINT_TILE_START, direction,
        reach - POINT_TILE_START, true);
    if !hit.hit { return 255u; }
    let level = min(u32(max((hit.distance + POINT_TILE_START)/reach, 0.0)*255.0), 254u);
    return level | (point_light_normal_code(hit.normal) << 8u);
}

// One word (two texels in a row) of light `id.y`'s tile per invocation.
@compute @workgroup_size(64) fn trace(@builtin(global_invocation_id) id: vec3<u32>) {
    let count = min(traced.metadata.x, 32u);
    if all(id == vec3(0u)) {
        shadow_tiles[0] = count;
        shadow_tiles[1] = 0u;
        shadow_tiles[2] = 0u;
        shadow_tiles[3] = 0u;
    }
    let word = id.x;
    let light = id.y;
    if light >= count || word >= POINT_TILE_TEXELS/2u { return; }
    let source = traced.lights[light].origin_radius;
    let reach = point_light_reach(source.w);
    let packed = point_light_trace_texel(source, reach, word*2u) |
        (point_light_trace_texel(source, reach, word*2u + 1u) << 16u);
    shadow_tiles[4u + light*(POINT_TILE_TEXELS/2u) + word] = packed;
}
