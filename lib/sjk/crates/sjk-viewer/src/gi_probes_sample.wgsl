// Probe grid bindings and sampling. Probes live on a fixed world-aligned grid covering the
// map; irradiance is first-order spherical harmonics in three components — bounce of
// unit sun, bounce of unit sky, lamp emission — so a change of sun or sky colour and
// intensity relights the map in the same frame with no refresh; visibility is 8x8
// octahedral depth moments (DDGI). Dead probes (inside solid) carry state.w < 0.
// Component layout per probe: 12 vec4 = sun L0..L1(3), sky L0..L1(3), emission L0..L1(3).
struct ProbeParams {
    // origin_index.w is the number of probes in this update dispatch.
    origin_index: vec4<i32>, counts: vec4<u32>, spacing: vec4<f32>,
    sun: vec4<f32>, sun_color: vec4<f32>, sky: vec4<f32>,
    far_vp: mat4x4<f32>, far_quality: vec4<f32>, window: vec4<u32>,
};
@group(1) @binding(0) var<uniform> probes: ProbeParams;
@group(1) @binding(1) var<storage, read> probe_sh: array<vec4<f32>>;
@group(1) @binding(2) var<storage, read> probe_depth: array<vec2<f32>>;
@group(1) @binding(3) var<storage, read> probe_state: array<vec4<i32>>;
@group(1) @binding(4) var probe_far_map: texture_depth_2d;
@group(1) @binding(5) var probe_far_sampler: sampler_comparison;
const OCTA: u32 = 8u;

fn probe_slot(index: vec3<i32>) -> u32 {
    let local = vec3<u32>(index - probes.origin_index.xyz);
    return local.x + (local.y + local.z*probes.counts.y)*probes.counts.x;
}
fn probe_index(slot: u32) -> vec3<i32> {
    let local = vec3(slot % probes.counts.x, (slot / probes.counts.x) % probes.counts.y,
        slot / (probes.counts.x*probes.counts.y));
    return vec3<i32>(local) + probes.origin_index.xyz;
}
fn probe_position(index: vec3<i32>) -> vec3<f32> {
    return vec3<f32>(index)*probes.spacing.x;
}
fn octa_wrap(v: vec2<f32>) -> vec2<f32> {
    return (1.0-abs(v.yx))*select(vec2(-1.0), vec2(1.0), v >= vec2(0.0));
}
fn octa_encode(d: vec3<f32>) -> vec2<f32> {
    let n = d/(abs(d.x)+abs(d.y)+abs(d.z));
    let p = select(octa_wrap(n.xy), n.xy, n.z >= 0.0);
    return p*0.5+0.5;
}
fn octa_decode(uv: vec2<f32>) -> vec3<f32> {
    let f = uv*2.0-1.0;
    var n = vec3(f.x, f.y, 1.0-abs(f.x)-abs(f.y));
    let t = clamp(-n.z, 0.0, 1.0);
    n.x += select(t, -t, n.x >= 0.0);
    n.y += select(t, -t, n.y >= 0.0);
    return normalize(n);
}
// Neighbour texel across an octahedral edge: the mirrored texel on the same edge.
fn octa_fold(t: vec2<i32>) -> vec2<i32> {
    var r = t;
    let last = i32(OCTA)-1;
    if r.x < 0 { r.x = 0; r.y = last - r.y; } else if r.x > last { r.x = last; r.y = last - r.y; }
    if r.y < 0 { r.y = 0; r.x = last - r.x; } else if r.y > last { r.y = last; r.x = last - r.x; }
    return clamp(r, vec2(0), vec2(last));
}
// Bilinear depth moments; nearest texels project the eight sectors as wedges on floors.
fn octa_moments(slot: u32, d: vec3<f32>) -> vec2<f32> {
    let p = octa_encode(d)*f32(OCTA) - 0.5;
    let base = vec2<i32>(floor(p));
    let f = fract(p);
    var sum = vec2(0.0);
    for (var i = 0; i < 4; i++) {
        let corner = vec2(i & 1, i >> 1);
        let t = octa_fold(base + corner);
        let w = select(1.0-f.x, f.x, corner.x == 1)*select(1.0-f.y, f.y, corner.y == 1);
        sum += probe_depth[slot*64u + u32(t.x) + u32(t.y)*OCTA]*w;
    }
    return sum;
}
// Sun visibility of a world point from the map-wide far cascade (world casters only).
// Without a far cascade (the installation burst) nothing is sunlit: sky and emission only,
// never a whole map lit as if every room stood in the sun.
fn probe_sun_visibility(point: vec3<f32>, normal: vec3<f32>) -> f32 {
    if probes.far_quality.z <= 0.0 { return 0.0; }
    let clip = probes.far_vp*vec4(point + normal*probes.far_quality.x*0.5, 1.0);
    let uv = clip.xy*vec2(0.5,-0.5)+0.5;
    if any(uv <= vec2(0.0)) || any(uv >= vec2(1.0)) || clip.z <= 0.0 || clip.z >= 1.0 {
        return 1.0;
    }
    return textureSampleCompareLevel(probe_far_map, probe_far_sampler, uv,
        clip.z - 0.05/probes.far_quality.y);
}
// Cosine-lobe convolution of L1 radiance SH: A0 = pi, A1 = 2pi/3.
fn sh_evaluate(c: array<vec3<f32>, 4>, n: vec3<f32>) -> vec3<f32> {
    let e = c[0]*3.1415927*0.282095 + (c[1]*n.y + c[2]*n.z + c[3]*n.x)*2.0943951*0.488603;
    return max(e, vec3(0.0));
}
// Blend the eight surrounding probes' SH at a point. With a normal, probes behind the
// surface are down-weighted and the point is lifted off it; without, geometry-only
// weights (trilinear plus Chebyshev visibility) serve volume cells in open air.
const COMPONENTS: u32 = 12u;
fn probe_gather(point: vec3<f32>, normal: vec3<f32>, has_normal: bool) -> array<vec3<f32>, 12> {
    var sh: array<vec3<f32>, 12>;
    for (var k = 0u; k < COMPONENTS; k++) { sh[k] = vec3(0.0); }
    // No probes at all: half the sky (as unit sky component, or lit in the combined table).
    if probes.counts.w == 0u {
        if COMPONENTS == 4u { sh[0] = probes.sky.rgb*0.5/(3.1415927*0.282095); }
        else { sh[4] = vec3(0.5/(3.1415927*0.282095)); }
        return sh;
    }
    let spacing = probes.spacing.x;
    let biased = select(point, point + normal*spacing*0.3, has_normal);
    let local = biased/spacing;
    let base = vec3<i32>(floor(local));
    let fraction = local - vec3<f32>(base);
    var weight_sum = 0.0;
    for (var corner = 0u; corner < 8u; corner++) {
        let offset = vec3<i32>(i32(corner & 1u), i32((corner >> 1u) & 1u), i32(corner >> 2u));
        let index = base + offset;
        let rel = index - probes.origin_index.xyz;
        if any(rel < vec3(0)) || any(rel >= vec3<i32>(probes.counts.xyz)) { continue; }
        let slot = probe_slot(index);
        if probe_state[slot].w <= 0 { continue; }
        let tri = select(1.0-fraction, fraction, offset == vec3(1));
        var weight = tri.x*tri.y*tri.z;
        let to_probe = probe_position(index) - point;
        let distance = length(to_probe);
        let direction = to_probe/max(distance, 1e-3);
        if has_normal {
            let facing = (dot(direction, normal)+1.0)*0.5;
            weight *= facing*facing + 0.05;
        }
        // Depth moments come from voxel hits, so they carry the voxel size as error, and a
        // surface point must never be rejected by the surface it lies on: test a point
        // nudged toward the probe, with a tolerance of a voxel and a half. A probe behind a
        // wall thicker than that contributes nothing: no light through solid geometry.
        let tolerance = probes.far_quality.w*1.5 + spacing*0.02;
        let tested = distance - spacing*0.1;
        let moments = octa_moments(slot, -direction);
        if tested > moments.x + tolerance {
            let variance = max(moments.y - moments.x*moments.x, tolerance*tolerance);
            let gap = tested - moments.x - tolerance;
            var chebyshev = variance/(variance + gap*gap);
            weight *= chebyshev*chebyshev;
        }
        let b = slot*COMPONENTS;
        for (var k = 0u; k < COMPONENTS; k++) { sh[k] += probe_sh[b + k].rgb*weight; }
        weight_sum += weight;
    }
    // Every probe around this point sees it as behind a wall: nothing reaches it but a
    // trace of sky, never the lit space on the other side.
    if weight_sum < 1e-4 {
        for (var k = 0u; k < COMPONENTS; k++) { sh[k] = vec3(0.0); }
        if COMPONENTS == 4u { sh[0] = probes.sky.rgb*0.03/(3.1415927*0.282095); }
        else { sh[4] = vec3(0.03/(3.1415927*0.282095)); }
        return sh;
    }
    let inv = 1.0/weight_sum;
    for (var k = 0u; k < COMPONENTS; k++) { sh[k] *= inv; }
    return sh;
}
fn component(sh: array<vec3<f32>, 12>, first: u32) -> array<vec3<f32>, 4> {
    return array<vec3<f32>, 4>(sh[first], sh[first + 1u], sh[first + 2u], sh[first + 3u]);
}
// Irradiance of the three components at a surface point, in unit-sun, unit-sky and
// emission units: the caller scales the first two by the current light.
fn probe_components(point: vec3<f32>, normal: vec3<f32>) -> array<vec3<f32>, 3> {
    let sh = probe_gather(point, normal, true);
    return array<vec3<f32>, 3>(sh_evaluate(component(sh, 0u), normal),
        sh_evaluate(component(sh, 4u), normal), sh_evaluate(component(sh, 8u), normal));
}
// The bounce seen from a ray hit inside the probe update: the nearest live probe in
// front of the surface, one probe's worth of reads instead of eight per ray.
fn probe_components_nearest(point: vec3<f32>, normal: vec3<f32>) -> array<vec3<f32>, 3> {
    var e = array<vec3<f32>, 3>(vec3(0.0), vec3(0.0), vec3(0.0));
    if probes.counts.w == 0u { e[1] = vec3(0.5); return e; }
    let spacing = probes.spacing.x;
    let biased = point + normal*spacing*0.3;
    let base = vec3<i32>(floor(biased/spacing));
    var best_slot = 0u;
    var best_weight = 0.0;
    for (var corner = 0u; corner < 8u; corner++) {
        let offset = vec3<i32>(i32(corner & 1u), i32((corner >> 1u) & 1u), i32(corner >> 2u));
        let index = base + offset;
        let rel = index - probes.origin_index.xyz;
        if any(rel < vec3(0)) || any(rel >= vec3<i32>(probes.counts.xyz)) { continue; }
        let slot = probe_slot(index);
        if probe_state[slot].w <= 0 { continue; }
        let to_probe = probe_position(index) - point;
        let distance = length(to_probe);
        // Nearest, in front of the surface, and not behind a wall.
        var weight = max(dot(to_probe/max(distance, 1e-3), normal), 0.0)/max(distance, 1.0);
        let moments = octa_moments(slot, -to_probe/max(distance, 1e-3));
        if distance - spacing*0.1 > moments.x + probes.far_quality.w*1.5 + spacing*0.02 {
            weight = 0.0;
        }
        if weight > best_weight { best_weight = weight; best_slot = slot; }
    }
    if best_weight <= 0.0 { return e; }
    let b = best_slot*COMPONENTS;
    for (var c = 0u; c < 3u; c++) {
        let sh = array<vec3<f32>, 4>(probe_sh[b + c*4u].rgb, probe_sh[b + c*4u + 1u].rgb,
            probe_sh[b + c*4u + 2u].rgb, probe_sh[b + c*4u + 3u].rgb);
        e[c] = sh_evaluate(sh, normal);
    }
    return e;
}
// Irradiance at a surface point in display units: from the combined table when the
// module reads it (COMPONENTS == 4), else the three components under the current sun and
// sky (`probes.sun_color`, `probes.sun.w`, `probes.sky`).
fn probe_irradiance(point: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let sh = probe_gather(point, normal, true);
    if COMPONENTS == 4u { return sh_evaluate(component(sh, 0u), normal); }
    return probes.sun_color.rgb*probes.sun.w*sh_evaluate(component(sh, 0u), normal)
        + probes.sky.rgb*sh_evaluate(component(sh, 4u), normal)
        + sh_evaluate(component(sh, 8u), normal);
}
// `probe_irradiance` with the share that arrives from one direction, for material maps:
// L1 irradiance is a + b·cos about its dominant direction, and a light of intensity I
// from that direction gives a = I/4, b = I/2, so 2b·cos (at most the whole irradiance)
// is the directional part on a surface facing `normal`. `vector` is that direction
// scaled by the directional share of the irradiance's luminance.
struct ProbeDirected { irradiance: vec3<f32>, vector: vec3<f32> };
fn probe_irradiance_directed(point: vec3<f32>, normal: vec3<f32>) -> ProbeDirected {
    let sh = probe_gather(point, normal, true);
    var c = component(sh, 0u);
    if COMPONENTS != 4u {
        for (var k = 0u; k < 4u; k++) {
            c[k] = probes.sun_color.rgb*probes.sun.w*sh[k] + probes.sky.rgb*sh[4u + k]
                + sh[8u + k];
        }
    }
    let irradiance = sh_evaluate(c, normal);
    let weights = vec3(0.2126, 0.7152, 0.0722);
    let linear = vec3(dot(c[3], weights), dot(c[1], weights), dot(c[2], weights))
        *2.0943951*0.488603;
    let b = length(linear);
    let total = dot(irradiance, weights);
    if b < 1e-6 || total < 1e-6 { return ProbeDirected(irradiance, vec3(0.0)); }
    let direction = linear/b;
    let along = min(2.0*b*max(dot(direction, normal), 0.0), total);
    return ProbeDirected(irradiance, direction*(along/total));
}
