// Real-time lighting for the no-bake mode: sun through the cascades plus probe bounce, in
// display units where a sunlit white surface is about one, scaled by the shadow pass's light scale. `shadow.radiance` is the sun radiance,
// `shadow.ambient` the sky radiance, `shadow.realtime` = (scale, buffer scale x, y, unused).
// Direct sun, lamps and bounce at a point (rgb) and the sun's visibility there (a), for
// the highlight the caller adds with its material's gloss.
fn realtime_light(world: vec3<f32>, normal: vec3<f32>, eye: vec3<f32>,
    forward: vec3<f32>) -> vec4<f32> {
    return realtime_light_from_visibility(world, normal, sun_visibility(world, normal, eye, forward));
}
// Geometry computes derivative-dependent sun visibility before storing the winning
// receiver. Lamp, probe and contact evaluation can then run once per final pixel.
fn realtime_light_from_visibility(world: vec3<f32>, normal: vec3<f32>, visibility: vec2<f32>) -> vec4<f32> {
    return realtime_light_with_lamps(world, normal, visibility, lamp_light(world, normal));
}
// `lamps` is the receiver's lamp irradiance, evaluated directly or taken from the cache.
fn realtime_light_with_lamps(world: vec3<f32>, normal: vec3<f32>, visibility: vec2<f32>,
    lamps: vec3<f32>) -> vec4<f32> {
    // `r_dayDebug` bits leave terms out, live, to name the one behind an artefact.
    let debug = u32(shadow.realtime.w);
    var sample = visibility;

    // Screen-space contact shadows (`sun_contact.wgsl`) only where the cascades say lit.
    if sample.x > 0.02 && (debug & 2u) == 0u { sample.x *= contact_shadow(world, normal); }
    let facing = max(dot(normal, shadow.sun.xyz), 0.0);
    let direct = shadow.radiance.rgb*shadow.radiance.w*facing*mix(1.0, sample.x, sample.y);
    // The map's lamps, direct and per pixel; their bounce rides the probes.
    var local = lamp_response(lamps);

    if (debug & 4u) != 0u { local = vec3(0.0); }
    // Bounce is gathered from the probes at the point itself (per light-buffer texel for
    // the world, per pixel for models).
    var indirect = shadow.ambient.rgb*0.5;
    if probes.counts.w != 0u { indirect = probe_irradiance(world, normal)/3.1415927; }
    if (debug & 16u) != 0u { indirect = shadow.ambient.rgb*0.5; }
    // Gain is applied when shading, never fed back into the probe bounce solver.
    // Screen-space obscurance affects sky/bounce; fill can retain a bounded floor.
    var occlusion = 1.0;
    if (debug & 1u) == 0u { occlusion = ambient_occlusion(world, normal); }
    indirect = readable_indirect(direct + local, indirect, occlusion, shadow.fill,
        shadow.readability.xy);
    // Bit 128: the sun's visibility itself as the light (grey where the maps say shadow).
    if (debug & 128u) != 0u { return vec4(vec3(0.15 + 0.85*sample.x), 1.0); }
    return vec4(shadow.realtime.x*(direct + local + indirect), mix(1.0, sample.x, sample.y));
}
// The same light for a light buffer with a direction target (maps with material maps):
// `direction` says where the non-sun share (lamps, probe bounce and sky) comes from, so
// the material program can move it to the mapped normal as it does the sun's. rg: the
// dominant direction, octahedral; b: the share of that light arriving from it, the rest
// being ambient; a: the lamps' part of the directional share, which alone casts
// highlights (reflected bounce and sky belong to the reflection probes).
struct DirectedLight { @location(0) light: vec4<f32>, @location(1) direction: vec4<f32> };
fn realtime_light_directed(world: vec3<f32>, normal: vec3<f32>, visibility: vec2<f32>,
    lamps: LampLight) -> DirectedLight {
    let debug = u32(shadow.realtime.w);
    var sample = visibility;
    if sample.x > 0.02 && (debug & 2u) == 0u { sample.x *= contact_shadow(world, normal); }
    let facing = max(dot(normal, shadow.sun.xyz), 0.0);
    let direct = shadow.radiance.rgb*shadow.radiance.w*facing*mix(1.0, sample.x, sample.y);
    var local = lamp_response(lamps.light);
    if (debug & 4u) != 0u { local = vec3(0.0); }
    var indirect = shadow.ambient.rgb*0.5;
    var bounce = vec3(0.0);
    if probes.counts.w != 0u {
        let probe = probe_irradiance_directed(world, normal);
        indirect = probe.irradiance/3.1415927;
        bounce = probe.vector;
    }
    if (debug & 16u) != 0u { indirect = shadow.ambient.rgb*0.5; bounce = vec3(0.0); }
    var occlusion = 1.0;
    if (debug & 1u) == 0u { occlusion = ambient_occlusion(world, normal); }
    indirect = readable_indirect(direct + local, indirect, occlusion, shadow.fill,
        shadow.readability.xy);
    if (debug & 128u) != 0u {
        return DirectedLight(vec4(vec3(0.15 + 0.85*sample.x), 1.0), vec4(0.5, 0.5, 0.0, 0.0));
    }
    let light = vec4(shadow.realtime.x*(direct + local + indirect), mix(1.0, sample.x, sample.y));
    return DirectedLight(light, rest_direction(local, lamps, indirect, bounce));
}
// Encode the non-sun light's direction (see `DirectedLight`). The lamp vector is a share
// of the raw lamp irradiance, the bounce vector of the raw probe irradiance; both shares
// are applied to the light as finally shown.
fn rest_direction(local: vec3<f32>, lamps: LampLight, indirect: vec3<f32>,
    bounce: vec3<f32>) -> vec4<f32> {
    let lamp_luminance_raw = lamp_luminance(lamps.light);
    let local_luminance = lamp_luminance(local);
    let indirect_luminance = lamp_luminance(indirect);
    let total = local_luminance + indirect_luminance;
    var lamp_vector = vec3(0.0);
    if lamp_luminance_raw > 1e-6 {
        lamp_vector = lamps.vector*(local_luminance/lamp_luminance_raw);
    }
    let bounce_vector = bounce*indirect_luminance;
    let vector = lamp_vector + bounce_vector;
    let length_vector = length(vector);
    if total < 1e-6 || length_vector < 1e-6 { return vec4(0.5, 0.5, 0.0, 0.0); }
    let lamp_part = length(lamp_vector);
    return vec4(octa_encode(vector/length_vector), min(length_vector/total, 1.0),
        lamp_part/max(lamp_part + length(bounce_vector), 1e-6));
}
