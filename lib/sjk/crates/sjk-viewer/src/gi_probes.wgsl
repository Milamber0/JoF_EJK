// Probe update: one workgroup per live probe, two rays per thread, all threads then own
// one octahedral depth texel. Radiance per ray in three components: unit-sun bounce
// (albedo times sun visibility through the far cascade plus the previous unit-sun
// irradiance), unit-sky bounce (albedo times previous unit-sky irradiance, one on a sky
// miss) and emission (albedo times previous emission irradiance plus the surface's own).
const RAYS: u32 = 128u;
const RAYS_PER_LANE: u32 = 2u;
// A single ray onto a lamp face must not dominate a probe: emission is capped per ray in
// display units (a sunlit white surface is about one).
const EMISSION_CAP: f32 = 12.0;
// Rays are a fixed pattern per probe (a Fibonacci sphere turned by a hash of the probe),
// never re-randomised, so a still scene yields a bit-identical estimate every refresh and
// the stored value cannot drift; a refresh blends in at BLEND so bounce light settles over
// a few passes and a turning sun is followed within seconds. A jump of the light (day
// clock) replaces the value outright in one wide pass.
const BLEND: f32 = 0.5;
@group(1) @binding(7) var<storage, read_write> probe_display: array<vec4<f32>>;
// Every probe's three components under this frame's sun and sky, four coefficients that
// the pixel shaders read: a change of sun or sky is visible the same frame.
@compute @workgroup_size(64,1,1) fn combine(@builtin(global_invocation_id) id: vec3<u32>,
    @builtin(num_workgroups) groups: vec3<u32>) {
    let slot = id.x + id.y*groups.x*64u;
    if slot >= probes.counts.w { return; }
    let sun = probes.sun_color.rgb*probes.sun.w;
    for (var k = 0u; k < 4u; k++) {
        let b = slot*12u + k;
        probe_display[slot*4u + k] = vec4(sun*probe_sh[b].rgb + probes.sky.rgb*probe_sh[b + 4u].rgb
            + probe_sh[b + 8u].rgb, 0.0);
    }
}
var<workgroup> ray_sun: array<vec3<f32>, 128>;
var<workgroup> ray_sky: array<vec3<f32>, 128>;
var<workgroup> ray_emit: array<vec3<f32>, 128>;
var<workgroup> ray_direction: array<vec3<f32>, 128>;
var<workgroup> ray_distance: array<f32, 128>;
var<workgroup> keep_weight: f32;

fn hash(v: u32) -> u32 {
    var x = v ^ 0x9e3779b9u;
    x ^= x >> 16u; x *= 0x7feb352du; x ^= x >> 15u; x *= 0x846ca68bu; x ^= x >> 16u;
    return x;
}
fn unit_float(v: u32) -> f32 { return f32(v & 0xffffffu)/16777216.0; }
// Fibonacci sphere direction `i` of RAYS, rotated by the probe's own fixed quaternion.
fn ray_dir(i: u32, seed: u32) -> vec3<f32> {
    let z = 1.0 - 2.0*(f32(i)+0.5)/f32(RAYS);
    let r = sqrt(max(1.0-z*z, 0.0));
    let phi = f32(i)*2.39996323;
    let d = vec3(r*cos(phi), r*sin(phi), z);
    let u1 = unit_float(hash(seed)); let u2 = unit_float(hash(seed+1u));
    let u3 = unit_float(hash(seed+2u));
    let q = vec4(sqrt(1.0-u1)*sin(6.2831853*u2), sqrt(1.0-u1)*cos(6.2831853*u2),
        sqrt(u1)*sin(6.2831853*u3), sqrt(u1)*cos(6.2831853*u3));
    let t = 2.0*cross(q.xyz, d);
    return d + q.w*t + cross(q.xyz, t);
}
fn surface_radiance(point: vec3<f32>, normal: vec3<f32>) -> array<vec3<f32>, 3> {
    let surface = gi_surface_at(point - normal*0.5*gi.sizes.x);
    let facing = max(dot(normal, probes.sun.xyz), 0.0);
    let direct = facing*probe_sun_visibility(point + normal*gi.sizes.x, normal);
    let lifted = point + normal*gi.sizes.x;
    let e = probe_components_nearest(lifted, normal);
    let albedo = surface.albedo.rgb;
    // Lamps light surfaces directly per pixel; the probes carry only their bounce, so a
    // ray onto a lamp face sees no emission (that direct light is already on the pixel).
    return array<vec3<f32>, 3>(albedo*(direct + e[0]/3.1415927), albedo*e[1]/3.1415927,
        albedo*(lamp_light(lifted, normal) + e[2])/3.1415927);
}
@compute @workgroup_size(64,1,1) fn update(@builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) lane: u32, @builtin(num_workgroups) groups: vec3<u32>) {
    // Slots are visited in order; dead probes (inside solid) are skipped. This keeps the
    // update within a baseline device's eight storage buffers (no live-list buffer).
    let offset = group.x + group.y*groups.x;
    if offset >= u32(probes.origin_index.w) { return; }
    let slot = (probes.window.x + offset) % probes.window.y;
    if probe_state[slot].w < 0 { return; }
    let index = probe_index(slot);
    let position = probe_position(index);
    let refreshes = probe_state[slot].w;
    let fresh = refreshes <= 0;
    let seed = hash(slot*7919u + 104729u);
    for (var r = 0u; r < RAYS_PER_LANE; r++) {
        let ray = lane*RAYS_PER_LANE + r;
        let d = ray_dir(ray, seed);
        var radiance = array<vec3<f32>, 3>(vec3(0.0), vec3(1.0), vec3(0.0));
        var distance = probes.spacing.w;
        let hit = gi_trace(position, d, probes.spacing.w);
        if hit.hit {
            distance = hit.distance;
            let point = position + d*hit.distance;
            let surface = gi_surface_at(point - hit.normal*0.5*gi.sizes.x);
            // Sky voxels are emissive sky: unit sky, not a wall.
            if surface.albedo.w > 0.5 { distance = probes.spacing.w; }
            else { radiance = surface_radiance(point, hit.normal); }
        }
        ray_sun[ray] = radiance[0];
        ray_sky[ray] = radiance[1];
        ray_emit[ray] = radiance[2];
        ray_direction[ray] = d;
        ray_distance[ray] = distance;
    }
    workgroupBarrier();
    if lane < 3u {
        // Lanes 0..2 each project one component onto L0..L1.
        var c0 = vec3(0.0); var c1 = vec3(0.0); var c2 = vec3(0.0); var c3 = vec3(0.0);
        for (var i = 0u; i < RAYS; i++) {
            let d = ray_direction[i];
            var l = ray_sun[i];
            if lane == 1u { l = ray_sky[i]; } else if lane == 2u { l = ray_emit[i]; }
            c0 += l*0.282095; c1 += l*0.488603*d.y; c2 += l*0.488603*d.z; c3 += l*0.488603*d.x;
        }
        let scale = 12.566371/f32(RAYS);
        let base = slot*12u + lane*4u;
        // Deringing: with |L1| <= 0.866 L0 the irradiance is non-negative in every
        // direction, so a lamp next to a probe brightens one side without a dark halo on
        // the other. Applied to the fresh estimate so the stored value stays valid.
        c0 *= scale; c1 *= scale; c2 *= scale; c3 *= scale;
        for (var k = 0; k < 3; k++) {
            let lobe = length(vec3(c1[k], c2[k], c3[k]));
            let limit = 0.866*c0[k];
            if lobe > limit && lobe > 1e-6 {
                let f = limit/lobe;
                c1[k] *= f; c2[k] *= f; c3[k] *= f;
            }
        }
        // Fresh probes and the host-flagged pass after a light jump take the estimate
        // outright; otherwise the blend settles the bounce chain in a few passes.
        let keep = select(1.0 - BLEND, 0.0, fresh || probes.window.w != 0u);
        if lane == 0u {
            keep_weight = keep;
            probe_state[slot] = vec4(index, i32(f32(max(refreshes, 0)) + 1.0));
        }
        probe_sh[base] = mix(vec4(c0, 0.0), probe_sh[base], keep);
        probe_sh[base+1u] = mix(vec4(c1, 0.0), probe_sh[base+1u], keep);
        probe_sh[base+2u] = mix(vec4(c2, 0.0), probe_sh[base+2u], keep);
        probe_sh[base+3u] = mix(vec4(c3, 0.0), probe_sh[base+3u], keep);
    }
    workgroupBarrier();
    // Depth moments follow the same running average (lane 0 decided its weight).
    let depth_weight = keep_weight;
    // Every lane owns one octahedral texel: distance moments from nearby ray directions.
    let texel_dir = octa_decode((vec2<f32>(f32(lane % OCTA), f32(lane / OCTA))+0.5)/f32(OCTA));
    var mean = 0.0; var mean2 = 0.0; var wsum = 0.0;
    for (var i = 0u; i < RAYS; i++) {
        let w = pow(max(dot(texel_dir, ray_direction[i]), 0.0), 24.0);
        mean += ray_distance[i]*w; mean2 += ray_distance[i]*ray_distance[i]*w; wsum += w;
    }
    let moments = select(vec2(probes.spacing.w, probes.spacing.w*probes.spacing.w),
        vec2(mean, mean2)/wsum, wsum > 1e-4);
    let old = probe_depth[slot*64u + lane];
    probe_depth[slot*64u + lane] = mix(moments, old, depth_weight);
}
