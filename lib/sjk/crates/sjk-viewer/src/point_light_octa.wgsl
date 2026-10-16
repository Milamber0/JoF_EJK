// Point-light shadow tiles (`dynamic_light_shadows.rs`), shared by their tracer and the
// programs lit by point lights. A tile is one light's octahedral map of directions, as the
// lamps' (`lamp_visibility_common.wgsl`, which not every one of these programs includes).
// Each texel is 16 bits: how far its ray travels before the static world stops it, in
// 255ths of the light's reach (255: nothing within reach), then the octahedral code of the
// normal of what stopped it. CPU layout: dynamic_lights.rs.
const POINT_TILE_EDGE: u32 = 28u;
const POINT_TILE_TEXELS: u32 = 784u;
// Rays start this far out, so a light a hair behind the face it was placed on (an impact's
// flash) still lights the side it belongs to.
const POINT_TILE_START: f32 = 1.0;

// Traced distance: a light's radius and a margin. Beyond it a tile tells nothing.
fn point_light_reach(radius: f32) -> f32 {
    return max(radius, 0.0) + 8.0;
}
// The octahedral map's unit square [-1, 1]² to an (unnormalised) direction and back.
fn point_light_octa_vector(p: vec2<f32>) -> vec3<f32> {
    var n = vec3(p, 1.0 - abs(p.x) - abs(p.y));
    if n.z < 0.0 {
        n = vec3((1.0 - abs(n.yx))*select(vec2(-1.0), vec2(1.0), n.xy >= vec2(0.0)), n.z);
    }
    return n;
}
fn point_light_octa_square(direction: vec3<f32>) -> vec2<f32> {
    let n = direction/(abs(direction.x) + abs(direction.y) + abs(direction.z));
    if n.z < 0.0 {
        return (1.0 - abs(n.yx))*select(vec2(-1.0), vec2(1.0), n.xy >= vec2(0.0));
    }
    return n.xy;
}
// The unit direction through the centre of tile texel `pixel`.
fn point_light_texel_direction(pixel: vec2<i32>) -> vec3<f32> {
    let p = (vec2<f32>(pixel) + 0.5)/f32(POINT_TILE_EDGE)*2.0 - 1.0;
    return normalize(point_light_octa_vector(p));
}
// Tile coordinates (texel units, texel centres at half-integers) of `direction`.
fn point_light_tile_position(direction: vec3<f32>) -> vec2<f32> {
    return (point_light_octa_square(direction)*0.5 + 0.5)*f32(POINT_TILE_EDGE);
}
// A filter tap one texel past an edge reads the texel the sphere puts there: octahedral
// edges fold onto themselves, mirrored (`lamp_visibility_pixel` for power-of-two tiles).
fn point_light_tile_pixel(pixel: vec2<i32>) -> vec2<i32> {
    let last = i32(POINT_TILE_EDGE) - 1;
    var p = pixel;
    if p.x < 0 || p.x > last { p = vec2(clamp(p.x, 0, last), last - p.y); }
    if p.y < 0 || p.y > last { p = vec2(last - p.x, clamp(p.y, 0, last)); }
    return p;
}
// A unit normal as 4 + 4 bits: 15 steps across the octahedral square, so the axes, which
// most walls and floors face, are exact.
fn point_light_normal_code(normal: vec3<f32>) -> u32 {
    let steps = vec2<u32>(round((point_light_octa_square(normal)*0.5 + 0.5)*14.0));
    return min(steps.x, 14u) | (min(steps.y, 14u) << 4u);
}
fn point_light_code_normal(code: u32) -> vec3<f32> {
    let p = vec2<f32>(vec2(code & 15u, (code >> 4u) & 15u))/7.0 - 1.0;
    return normalize(point_light_octa_vector(p));
}
