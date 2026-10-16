// Stage hook of the world program in day mode: the real-time light was evaluated once per
// half-resolution texel by the light pass, on this same opaque geometry, so a world pixel
// only upsamples it. Texels whose depth disagrees with the pixel belong to another
// surface across a silhouette and are rejected; if none agrees (a surface the light pass
// never drew: blended, alpha-tested, deformed), the depth-nearest texel stands in.
@group(3) @binding(2) var<uniform> shadow: Shadow;
@group(3) @binding(20) var light_buffer: texture_2d<f32>;
@group(3) @binding(21) var light_depth: texture_depth_2d;
@group(3) @binding(23) var light_normal: texture_2d<f32>;
fn realtime_active() -> bool { return shadow.ambient.w > 0.5 && shadow.realtime.y > 0.0; }
fn buffered_light(position: vec4<f32>, normal: vec3<f32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(light_buffer));
    let at = position.xy*shadow.realtime.yz - 0.5;
    let base = vec2<i32>(floor(at));
    let blend = fract(at);
    // A texel centre lies up to one texel spacing (1/scale pixels) away along this surface:
    // its depth may differ by that much of the surface's own depth slope, never more.
    let slope = abs(dpdx(position.z)) + abs(dpdy(position.z));
    let tolerance = slope*1.5/shadow.realtime.y + 4e-7;
    var sum = vec4(0.0);
    var weight = 0.0;
    // Fallback when no texel agrees: the depth-nearest texel among those facing the
    // pixel's way, any texel only if none does. At a crease the texels around a floor
    // pixel can all be the wall's, and a wall texel reads as sunlit (it is the caster
    // itself): taken by depth alone it lit shadowed floors along every wall base.
    var nearest = vec4(0.0);
    var nearest_gap = 1e9;
    var nearest_faces = false;
    for (var y = 0; y < 2; y++) { for (var x = 0; x < 2; x++) {
        let texel = clamp(base + vec2(x, y), vec2(0), dims - 1);
        let gap = abs(textureLoad(light_depth, texel, 0) - position.z);
        let light = textureLoad(light_buffer, texel, 0);
        let bilinear = select(1.0-blend.x, blend.x, x == 1)*select(1.0-blend.y, blend.y, y == 1);
        // At a crease the wall's texels lie within depth tolerance of the floor pixel but
        // carry the wall's light (a sunlit face next to a shadowed floor): the normal
        // tells them apart, so no light seeps under walls.
        let texel_normal = textureLoad(light_normal, texel, 0).xyz*2.0 - 1.0;
        let same_face = dot(normalize(texel_normal), normal) > 0.8;
        let w = select(0.0, bilinear, gap <= tolerance && same_face);
        sum += light*w;
        weight += w;
        if (same_face && !nearest_faces) || (same_face == nearest_faces && gap < nearest_gap) {
            nearest_gap = gap; nearest = light; nearest_faces = same_face;
        }
    } }
    if weight < 1e-4 { return nearest; }
    return sum/weight;
}
// Model diffuse in day mode: the buffered light (the light pass drew the entity too),
// the highlight for the material's gloss, then the existing point-light law on top.
// Legacy secondary views keep authored light; flag 8 identifies a mirror with
// its own freshly rendered light buffer and the current main-view receiver group.
fn model_sun_color(input: VertexOutput, original: vec3<f32>) -> vec3<f32> {
    let rgb = i32(stage.generators.x);
    if !realtime_active() || (rgb != 4 && rgb != 9) || (u32(camera._padding) & 7u) != 0u && (u32(camera._padding) & 8u) == 0u {
        return original;
    }
    // Entity colour (RF_RGB_TINT, 1) and disintegration colours (3) are final, as
    // in rd-vanilla, where both replace the lighting generators.
    if input.light_ambient.a == 0.0 || input.entity_control.x > 0.5 { return original; }
    // The light pass lit the side the viewer sees (its normal turned toward the camera):
    // a two-sided face seen from its back matches that texel, not the authored normal.
    let normal = surface_normal(input.world_position, normalize(input.world_normal),
        camera.camera_position);
    let sun = buffered_light(input.position, normal);
    let gloss = select(stage.emission.w, 0.0, (u32(shadow.realtime.w) & 512u) != 0u);
    let highlight = shadow.realtime.x*(sun_specular(input.world_position, normal, sun.a, gloss)
        + sky_reflection(input.world_position, normal, gloss));
    let local = finish_model_light(input.world_position,
        EntityLight(vec3(0.0), vec3(0.0), vec3(0.0)));
    var lit = sun.rgb + highlight + max(dot(normal, local.direction), 0.0)*local.directed;
    if rgb == 9 { lit *= input.entity_color.rgb; }
    return lit;
}
// A lightmap stage's texel in day mode: the buffered light, the sun highlight and sky
// rim for the material's gloss (`stage.emission.w`). Visible emission belongs to
// the authored texture stages, not this diffuse illumination multiplier.
fn realtime_lightmap(input: VertexOutput, texel: vec4<f32>) -> vec4<f32> {
    if !realtime_active() { return texel; }
    let normal = surface_normal(input.world_position, normalize(input.world_normal),
        camera.camera_position);
    let light = buffered_light(input.position, normal);
    // `r_dayDebug` bit 512: no sun highlight or sky rim.
    let gloss = select(stage.emission.w, 0.0, (u32(shadow.realtime.w) & 512u) != 0u);
    return vec4(light.rgb + shadow.realtime.x*(sun_specular(input.world_position, normal,
        light.a, gloss) + sky_reflection(input.world_position, normal, gloss)), texel.a);
}
