// Real-time lighting with material maps. The light buffer holds the light the light pass
// evaluated with the geometric normal at half resolution (`sun_realtime_buffer.wgsl`):
// the sun share is moved to the mapped normal per pixel, with the sun visibility the
// buffer kept in alpha. Maps with material maps also get a direction target
// (`DirectedLight` in `sun_realtime.wgsl`): the share of the lamp, bounce and sky light
// that arrives from one direction is moved the same way, and the lamps' part of it casts
// highlights; the rest stays ambient.
@group(3) @binding(41) var light_direction: texture_2d<f32>;

struct MaterialMapBuffered {
    light: vec4<f32>,
    // Dominant direction of the non-sun light, scaled by the share arriving from it.
    vector: vec3<f32>,
    // The lamps' part of that directional share.
    lamps: f32,
};
fn material_map_octa_decode(uv: vec2<f32>) -> vec3<f32> {
    let f = uv*2.0 - 1.0;
    var n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    let t = clamp(-n.z, 0.0, 1.0);
    n.x += select(t, -t, n.x >= 0.0);
    n.y += select(t, -t, n.y >= 0.0);
    return normalize(n);
}
// `buffered_light` with the direction target read through the same texel weights.
fn material_map_buffered(position: vec4<f32>, normal: vec3<f32>) -> MaterialMapBuffered {
    let dims = vec2<i32>(textureDimensions(light_buffer));
    let at = position.xy*shadow.realtime.yz - 0.5;
    let base = vec2<i32>(floor(at));
    let blend = fract(at);
    let slope = abs(dpdx(position.z)) + abs(dpdy(position.z));
    let tolerance = slope*1.5/shadow.realtime.y + 4e-7;
    let directed = all(textureDimensions(light_direction) == textureDimensions(light_buffer));
    var sum = vec4(0.0);
    var vector = vec3(0.0);
    var lamps = 0.0;
    var weight = 0.0;
    var nearest = vec4(0.0);
    var nearest_direction = vec4(0.5, 0.5, 0.0, 0.0);
    var nearest_gap = 1e9;
    var nearest_faces = false;
    for (var y = 0; y < 2; y++) { for (var x = 0; x < 2; x++) {
        let texel = clamp(base + vec2(x, y), vec2(0), dims - 1);
        let gap = abs(textureLoad(light_depth, texel, 0) - position.z);
        let light = textureLoad(light_buffer, texel, 0);
        var direction = vec4(0.5, 0.5, 0.0, 0.0);
        if directed { direction = textureLoad(light_direction, texel, 0); }
        let bilinear = select(1.0-blend.x, blend.x, x == 1)*select(1.0-blend.y, blend.y, y == 1);
        let texel_normal = textureLoad(light_normal, texel, 0).xyz*2.0 - 1.0;
        let same_face = dot(normalize(texel_normal), normal) > 0.8;
        let w = select(0.0, bilinear, gap <= tolerance && same_face);
        sum += light*w;
        vector += material_map_octa_decode(direction.xy)*direction.z*w;
        lamps += direction.w*direction.z*w;
        weight += w;
        if (same_face && !nearest_faces) || (same_face == nearest_faces && gap < nearest_gap) {
            nearest_gap = gap; nearest = light; nearest_faces = same_face;
            nearest_direction = direction;
        }
    } }
    if weight < 1e-4 {
        return MaterialMapBuffered(nearest,
            material_map_octa_decode(nearest_direction.xy)*nearest_direction.z,
            nearest_direction.w);
    }
    let share = length(vector);
    return MaterialMapBuffered(sum/weight, vector/weight,
        select(0.0, lamps/(share*weight), share > 1e-6));
}

fn material_map_lightmap(input: VertexOutput, texel: vec4<f32>) -> vec4<f32> {
    if !realtime_active() { return material_map_baked(input, texel); }
    let surface = material_map_surface;
    let buffered = material_map_buffered(input.position, normalize(input.world_normal));
    let light = buffered.light;
    let sun = shadow.sun.xyz;
    let received = dot(surface.geometric, sun);
    let direct = shadow.realtime.x*shadow.radiance.rgb*shadow.radiance.w*light.a;
    let baked = direct*max(received, 0.0);
    let rest = max(light.rgb - baked, vec3(0.0));
    // The mapped normal takes over toward the terminator's lit side only: nothing reaches a
    // face turned from the sun, and a flat map gives back exactly the buffered light.
    let facing = mix(max(received, 0.0), max(dot(surface.normal, sun), 0.0),
        clamp(4.0*received, 0.0, 1.0));
    // `r_dayDebug` bit 512: no sun highlight or sky rim.
    let highlights = (u32(shadow.realtime.w) & 512u) == 0u;
    // The non-sun light's directional share, as the baked path treats a lightmap texel:
    // taken as arriving along its direction, divided by the face's own cosine (at most
    // 4x) and received by the mapped normal; what the face did not receive stays ambient.
    // `r_dayDebug` bit 1024: the non-sun light stays as evaluated (comparison only).
    let share = select(length(buffered.vector), 0.0, (u32(shadow.realtime.w) & 1024u) != 0u);
    var lit = vec3(0.0);
    if share > 1e-3 {
        let toward = buffered.vector/share;
        let received_rest = clamp(dot(surface.geometric, toward), 0.0, 1.0);
        let along = rest*share/max(received_rest, 0.25);
        let ambient = max(rest - along*received_rest, vec3(0.0));
        let facing_rest = mix(received_rest, max(dot(surface.normal, toward), 0.0),
            clamp(4.0*received_rest, 0.0, 1.0));
        lit = material_map_shade(input.world_position, direct, sun, facing, ambient)
            + material_map_shade_light(input.world_position, along, toward, facing_rest,
                select(0.0, buffered.lamps, highlights));
    } else {
        lit = material_map_shade(input.world_position, direct, sun, facing, rest);
    }
    if material_map_layout() == 0u {
        // No specular map: the material's existing gloss, on the mapped normal.
        let gloss = select(0.0, stage.emission.w, highlights);
        lit += shadow.realtime.x*(sun_specular(input.world_position, surface.normal, light.a,
            gloss) + sky_reflection(input.world_position, surface.normal, gloss));
    } else {
        // `r_dayDebug` 512 drops the sun and lamp highlights, not the probe reflection.
        if !highlights { material_map_highlight = vec3(0.0); }
        let response = material_map_response();
        let reflection = material_map_reflection(input.world_position, response);
        if reflection.a > 0.0 {
            material_map_highlight += reflection.rgb;
        } else if highlights {
            // No captured probe: the sky's rim, as before probes existed.
            material_map_highlight += shadow.realtime.x*response.occlusion
                *mix(vec3(1.0), response.specular, response.metalness)
                *sky_reflection(input.world_position, surface.normal, 1.0 - response.roughness);
        }
    }
    return vec4(lit, texel.a);
}
