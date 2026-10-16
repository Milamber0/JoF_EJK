// Point lights on surfaces: the block's layout, the grid that finds a fragment's candidates,
// the two ways a light reaches a surface, and the static world's shadow on it. The including
// program binds `point_lights` (a `PointLightBlock`), includes `point_light_octa.wgsl`, and
// defines `realtime_active()` and a `VertexOutput` with `world_position` and `world_normal`.

struct PointLight {
    origin_radius: vec4<f32>,
    color: vec4<f32>,
};

// CPU layout and dimensions: point_light_grid.rs. One bit per original light index.
struct PointLightGrid {
    low: vec4<f32>,
    high: vec4<f32>,
    scale: vec4<f32>,
    control: vec4<u32>,
    masks: array<vec4<u32>,432>,
};
// `shadows.x` lights have a static-world shadow tile this frame (`dynamic_light_shadows.rs`,
// mapping in `point_light_octa.wgsl`): the GPU copies them in after the CPU's part.
struct PointLightBlock {
    lights: array<PointLight, 32>,
    metadata: vec4<u32>,
    grid: PointLightGrid,
    shadows: vec4<u32>,
    tiles: array<vec4<u32>, 3136>,
};

// Bounds are shared by all cameras. Only finite surface laws use this mask;
// the legacy model inverse-square light has infinite support and keeps its loop.
fn finite_point_mask(world: vec3<f32>) -> u32 {
    let control = point_lights.grid.control;
    if control.y != 0u || control.x == 0u { return control.x; }
    let low = point_lights.grid.low.xyz;
    let high = point_lights.grid.high.xyz;
    if any(world < low) || any(world > high) { return 0u; }
    let cell = vec3<u32>(clamp((world-low)*point_lights.grid.scale.xyz,vec3(0.0),vec3(11.0)));
    let index = cell.x+(cell.y+cell.z*12u)*12u;
    return point_lights.grid.masks[index>>2u][index&3u];
}
// Tile texel `pixel` of light `index`: 255th of the reach in the low byte, normal code above.
fn point_light_texel(index: u32, pixel: vec2<i32>) -> u32 {
    let texel = index*POINT_TILE_TEXELS + u32(pixel.y)*POINT_TILE_EDGE + u32(pixel.x);
    let word = point_lights.tiles[texel >> 3u][(texel >> 1u) & 3u];
    return (word >> ((texel & 1u)*16u)) & 0xffffu;
}
// The share of light `index` that reaches `world` past the static world: 1 without a tile
// this frame or beyond the light's reach. Four bilinear taps of its tile, each a ray near
// the receiver's own:
// - clear when nothing stops it before the receiver's distance or, with the surface's
//   geometric `normal` (zero for a model's pixel), before the receiving plane (the lamps'
//   test, `lamp_visibility_sample.wgsl`, so grazing floors never shadow themselves);
// - beside when what stopped it leaves `world` in front of its face: the other face of the
//   receiver's crease, the receiver's own surface further on, a step's edge;
// - behind when `world` lies behind that face: a wall between it and the light.
// Behind taps also discount beside ones (twice their share): a floor that runs on under a
// wall reaches the receiving plane in front of it, which alone would light the room behind.
// A stored distance can lie a 255th of the reach and half a unit past the real hit; within
// that a hit counts as on the receiving plane, and within a unit more as level with a face.
fn point_light_visibility(index: u32, world: vec3<f32>, normal: vec3<f32>) -> f32 {
    if index >= point_lights.shadows.x { return 1.0; }
    let light = point_lights.lights[index].origin_radius;
    let delta = world - light.xyz;
    let distance = length(delta);
    let reach = point_light_reach(light.w);
    if distance < 1e-3 || distance >= reach { return 1.0; }
    let surface = dot(normal, normal) > 1e-12;
    let unit = normal*inverseSqrt(max(dot(normal, normal), 1e-24));
    let plane = dot(delta, unit);
    let oriented = unit*select(-1.0, 1.0, plane >= 0.0);
    let plane_distance = abs(plane);
    let rounding = reach/255.0 + 0.5;
    let at = point_light_tile_position(delta) - 0.5;
    let base = vec2<i32>(floor(at));
    let blend = at - floor(at);
    var clear = 0.0;
    var beside = 0.0;
    var behind = 0.0;
    for (var tap = 0u; tap < 4u; tap++) {
        let corner = vec2(tap & 1u, tap >> 1u);
        let pixel = point_light_tile_pixel(base + vec2<i32>(corner));
        let ray = point_light_texel_direction(pixel);
        let denominator = dot(ray, oriented);
        // A ray leaving for the far side of the receiving plane never meets it.
        if surface && denominator < 1e-4 { continue; }
        let weights = select(1.0 - blend, blend, corner == vec2(1u));
        let weight = weights.x*weights.y;
        let texel = point_light_texel(index, pixel);
        let level = texel & 255u;
        let free = (f32(level) + 1.0)*reach/255.0 + 0.5;
        if level == 255u || free >= distance ||
            (surface && free*denominator - plane_distance > rounding*denominator) {
            clear += weight;
            continue;
        }
        let face = point_light_code_normal(texel >> 8u);
        if dot(delta - ray*free, face) >= -(1.0 + rounding) {
            beside += weight;
        } else {
            behind += weight;
        }
    }
    let total = clear + beside + behind;
    if total < 1e-6 { return 1.0; }
    return (clear + beside*max(1.0 - 2.0*behind/total, 0.0))/total;
}

// The fragment's candidate point lights (`stage_fragment` sets it): emitted light and
// dynamic-light modulation share one grid lookup instead of making one each.
var<private> fragment_point_mask: u32;
// Emitted light illuminates albedo even where the existing environment is dark.
// This mask keeps the old point-light behavior for every unmarked source.
fn emitted_light(input: VertexOutput) -> vec3<f32> {
    if point_lights.metadata.w == 0u || !realtime_active() { return vec3(0.0); }
    var result = vec3(0.0);
    let normal = normalize(input.world_normal);
    var remaining = fragment_point_mask & point_lights.metadata.w;
    while remaining != 0u {
        let index = firstTrailingBit(remaining);
        remaining &= remaining - 1u;
        let light = point_lights.lights[index];
        if light.color.w == 0.0 { continue; }
        let delta = light.origin_radius.xyz-input.world_position;
        // Outside this finite light's box, its radial contribution is exactly zero.
        // Reject before square roots/normalization; no screen- or distance-quality LOD.
        if any(abs(delta) > vec3(max(light.origin_radius.w,0.001))) { continue; }
        let distance = length(delta);
        let falloff = max(1.0-distance/max(light.origin_radius.w,0.001),0.0);
        let facing = max(dot(normal,delta/max(distance,0.001)),0.0);
        let amount = falloff*falloff*facing;
        // Walls between the light and this surface stop it (the geometric normal: a
        // mapped one would shadow its own bumps).
        if amount <= 0.0 { continue; }
        result += light.color.rgb*amount*
            point_light_visibility(index,input.world_position,input.world_normal);
    }
    return result;
}

// rd-vanilla marks affected surfaces in R_DlightBmodel/R_MarkLights and adds
// a projected GL_DST_COLOR,GL_ONE pass in RB_ProjectDlightTexture
// (tr_light.cpp:44-111; tr_world.cpp:253-360; tr_shade.cpp:761-1091).
// We retain its additive modulation in the existing opaque pass: radial
// attenuation and facing replace the legacy 2-D projection texture.
fn dynamic_light_modulation(input: VertexOutput) -> vec3<f32> {
    var result = vec3(0.0);
    let normal = normalize(input.world_normal);
    var remaining = fragment_point_mask;
    if realtime_active() { remaining &= ~point_lights.metadata.w; }
    while remaining != 0u {
        let index = firstTrailingBit(remaining);
        remaining &= remaining - 1u;
        let light = point_lights.lights[index];
        if light.color.w > 0.0 && realtime_active() { continue; }
        let delta = light.origin_radius.xyz - input.world_position;
        if any(abs(delta) > vec3(light.origin_radius.w)) { continue; }
        let distance = length(delta);
        if distance < light.origin_radius.w && distance > 0.0001 {
            let attenuation = 1.0 - distance / light.origin_radius.w;
            let facing = max(dot(normal, delta / distance), 0.0);
            if facing <= 0.0 { continue; }
            result += light.color.rgb * attenuation * facing *
                point_light_visibility(index, input.world_position, input.world_normal);
        }
    }
    return result;
}
