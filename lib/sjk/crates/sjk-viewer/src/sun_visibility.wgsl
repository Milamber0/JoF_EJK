// Return a constant only if all texels of every PCF footprint satisfy the compare.
// The bounds level is the finest whose tiles (8 texels at level 0, doubling per level)
// are at least as wide as the footprint, which then spans at most 2x2 of them.
fn bounded_visibility(p: Projection, dimensions: vec2<f32>, depth: f32, radius: f32, layer: i32) -> f32 {
    let at = p.uv*dimensions - 0.5;
    let first = vec2<i32>(clamp(floor(at - radius)-1.0, vec2(0.0), dimensions-1.0));
    let last = vec2<i32>(clamp(floor(at + radius)+2.0, vec2(0.0), dimensions-1.0));
    let span = u32(max(last.x - first.x, last.y - first.y)) + 1u;
    let level = min(u32(max(i32(firstLeadingBit(max(span, 2u) - 1u)) - 2, 0)),
        textureNumLevels(shadow_bounds) - 1u);
    let lo = first >> vec2(level + 3u);
    let hi = last >> vec2(level + 3u);
    let a = textureLoad(shadow_bounds, lo, layer, i32(level)).xy;
    let b = textureLoad(shadow_bounds, vec2(hi.x,lo.y), layer, i32(level)).xy;
    let c = textureLoad(shadow_bounds, vec2(lo.x,hi.y), layer, i32(level)).xy;
    let d = textureLoad(shadow_bounds, hi, layer, i32(level)).xy;
    let low = min(min(a.x,b.x),min(c.x,d.x));
    let high = max(max(a.y,b.y),max(c.y,d.y));
    let variation = dot(abs(p.gradient), vec2(radius)/dimensions) + 1e-6;
    if depth+variation < low { return 1.0; }
    if depth-variation > high { return 0.0; }
    return -1.0;
}
// Fixed golden-angle directions; each angle retains f32 multiplication.
const SUN_DIRECTIONS = array<vec2<f32>, 128>(
    vec2(cos(0.0f * 2.39996323f), sin(0.0f * 2.39996323f)),
    vec2(cos(1.0f * 2.39996323f), sin(1.0f * 2.39996323f)),
    vec2(cos(2.0f * 2.39996323f), sin(2.0f * 2.39996323f)),
    vec2(cos(3.0f * 2.39996323f), sin(3.0f * 2.39996323f)),
    vec2(cos(4.0f * 2.39996323f), sin(4.0f * 2.39996323f)),
    vec2(cos(5.0f * 2.39996323f), sin(5.0f * 2.39996323f)),
    vec2(cos(6.0f * 2.39996323f), sin(6.0f * 2.39996323f)),
    vec2(cos(7.0f * 2.39996323f), sin(7.0f * 2.39996323f)),
    vec2(cos(8.0f * 2.39996323f), sin(8.0f * 2.39996323f)),
    vec2(cos(9.0f * 2.39996323f), sin(9.0f * 2.39996323f)),
    vec2(cos(10.0f * 2.39996323f), sin(10.0f * 2.39996323f)),
    vec2(cos(11.0f * 2.39996323f), sin(11.0f * 2.39996323f)),
    vec2(cos(12.0f * 2.39996323f), sin(12.0f * 2.39996323f)),
    vec2(cos(13.0f * 2.39996323f), sin(13.0f * 2.39996323f)),
    vec2(cos(14.0f * 2.39996323f), sin(14.0f * 2.39996323f)),
    vec2(cos(15.0f * 2.39996323f), sin(15.0f * 2.39996323f)),
    vec2(cos(16.0f * 2.39996323f), sin(16.0f * 2.39996323f)),
    vec2(cos(17.0f * 2.39996323f), sin(17.0f * 2.39996323f)),
    vec2(cos(18.0f * 2.39996323f), sin(18.0f * 2.39996323f)),
    vec2(cos(19.0f * 2.39996323f), sin(19.0f * 2.39996323f)),
    vec2(cos(20.0f * 2.39996323f), sin(20.0f * 2.39996323f)),
    vec2(cos(21.0f * 2.39996323f), sin(21.0f * 2.39996323f)),
    vec2(cos(22.0f * 2.39996323f), sin(22.0f * 2.39996323f)),
    vec2(cos(23.0f * 2.39996323f), sin(23.0f * 2.39996323f)),
    vec2(cos(24.0f * 2.39996323f), sin(24.0f * 2.39996323f)),
    vec2(cos(25.0f * 2.39996323f), sin(25.0f * 2.39996323f)),
    vec2(cos(26.0f * 2.39996323f), sin(26.0f * 2.39996323f)),
    vec2(cos(27.0f * 2.39996323f), sin(27.0f * 2.39996323f)),
    vec2(cos(28.0f * 2.39996323f), sin(28.0f * 2.39996323f)),
    vec2(cos(29.0f * 2.39996323f), sin(29.0f * 2.39996323f)),
    vec2(cos(30.0f * 2.39996323f), sin(30.0f * 2.39996323f)),
    vec2(cos(31.0f * 2.39996323f), sin(31.0f * 2.39996323f)),
    vec2(cos(32.0f * 2.39996323f), sin(32.0f * 2.39996323f)),
    vec2(cos(33.0f * 2.39996323f), sin(33.0f * 2.39996323f)),
    vec2(cos(34.0f * 2.39996323f), sin(34.0f * 2.39996323f)),
    vec2(cos(35.0f * 2.39996323f), sin(35.0f * 2.39996323f)),
    vec2(cos(36.0f * 2.39996323f), sin(36.0f * 2.39996323f)),
    vec2(cos(37.0f * 2.39996323f), sin(37.0f * 2.39996323f)),
    vec2(cos(38.0f * 2.39996323f), sin(38.0f * 2.39996323f)),
    vec2(cos(39.0f * 2.39996323f), sin(39.0f * 2.39996323f)),
    vec2(cos(40.0f * 2.39996323f), sin(40.0f * 2.39996323f)),
    vec2(cos(41.0f * 2.39996323f), sin(41.0f * 2.39996323f)),
    vec2(cos(42.0f * 2.39996323f), sin(42.0f * 2.39996323f)),
    vec2(cos(43.0f * 2.39996323f), sin(43.0f * 2.39996323f)),
    vec2(cos(44.0f * 2.39996323f), sin(44.0f * 2.39996323f)),
    vec2(cos(45.0f * 2.39996323f), sin(45.0f * 2.39996323f)),
    vec2(cos(46.0f * 2.39996323f), sin(46.0f * 2.39996323f)),
    vec2(cos(47.0f * 2.39996323f), sin(47.0f * 2.39996323f)),
    vec2(cos(48.0f * 2.39996323f), sin(48.0f * 2.39996323f)),
    vec2(cos(49.0f * 2.39996323f), sin(49.0f * 2.39996323f)),
    vec2(cos(50.0f * 2.39996323f), sin(50.0f * 2.39996323f)),
    vec2(cos(51.0f * 2.39996323f), sin(51.0f * 2.39996323f)),
    vec2(cos(52.0f * 2.39996323f), sin(52.0f * 2.39996323f)),
    vec2(cos(53.0f * 2.39996323f), sin(53.0f * 2.39996323f)),
    vec2(cos(54.0f * 2.39996323f), sin(54.0f * 2.39996323f)),
    vec2(cos(55.0f * 2.39996323f), sin(55.0f * 2.39996323f)),
    vec2(cos(56.0f * 2.39996323f), sin(56.0f * 2.39996323f)),
    vec2(cos(57.0f * 2.39996323f), sin(57.0f * 2.39996323f)),
    vec2(cos(58.0f * 2.39996323f), sin(58.0f * 2.39996323f)),
    vec2(cos(59.0f * 2.39996323f), sin(59.0f * 2.39996323f)),
    vec2(cos(60.0f * 2.39996323f), sin(60.0f * 2.39996323f)),
    vec2(cos(61.0f * 2.39996323f), sin(61.0f * 2.39996323f)),
    vec2(cos(62.0f * 2.39996323f), sin(62.0f * 2.39996323f)),
    vec2(cos(63.0f * 2.39996323f), sin(63.0f * 2.39996323f)),
    vec2(cos(64.0f * 2.39996323f), sin(64.0f * 2.39996323f)),
    vec2(cos(65.0f * 2.39996323f), sin(65.0f * 2.39996323f)),
    vec2(cos(66.0f * 2.39996323f), sin(66.0f * 2.39996323f)),
    vec2(cos(67.0f * 2.39996323f), sin(67.0f * 2.39996323f)),
    vec2(cos(68.0f * 2.39996323f), sin(68.0f * 2.39996323f)),
    vec2(cos(69.0f * 2.39996323f), sin(69.0f * 2.39996323f)),
    vec2(cos(70.0f * 2.39996323f), sin(70.0f * 2.39996323f)),
    vec2(cos(71.0f * 2.39996323f), sin(71.0f * 2.39996323f)),
    vec2(cos(72.0f * 2.39996323f), sin(72.0f * 2.39996323f)),
    vec2(cos(73.0f * 2.39996323f), sin(73.0f * 2.39996323f)),
    vec2(cos(74.0f * 2.39996323f), sin(74.0f * 2.39996323f)),
    vec2(cos(75.0f * 2.39996323f), sin(75.0f * 2.39996323f)),
    vec2(cos(76.0f * 2.39996323f), sin(76.0f * 2.39996323f)),
    vec2(cos(77.0f * 2.39996323f), sin(77.0f * 2.39996323f)),
    vec2(cos(78.0f * 2.39996323f), sin(78.0f * 2.39996323f)),
    vec2(cos(79.0f * 2.39996323f), sin(79.0f * 2.39996323f)),
    vec2(cos(80.0f * 2.39996323f), sin(80.0f * 2.39996323f)),
    vec2(cos(81.0f * 2.39996323f), sin(81.0f * 2.39996323f)),
    vec2(cos(82.0f * 2.39996323f), sin(82.0f * 2.39996323f)),
    vec2(cos(83.0f * 2.39996323f), sin(83.0f * 2.39996323f)),
    vec2(cos(84.0f * 2.39996323f), sin(84.0f * 2.39996323f)),
    vec2(cos(85.0f * 2.39996323f), sin(85.0f * 2.39996323f)),
    vec2(cos(86.0f * 2.39996323f), sin(86.0f * 2.39996323f)),
    vec2(cos(87.0f * 2.39996323f), sin(87.0f * 2.39996323f)),
    vec2(cos(88.0f * 2.39996323f), sin(88.0f * 2.39996323f)),
    vec2(cos(89.0f * 2.39996323f), sin(89.0f * 2.39996323f)),
    vec2(cos(90.0f * 2.39996323f), sin(90.0f * 2.39996323f)),
    vec2(cos(91.0f * 2.39996323f), sin(91.0f * 2.39996323f)),
    vec2(cos(92.0f * 2.39996323f), sin(92.0f * 2.39996323f)),
    vec2(cos(93.0f * 2.39996323f), sin(93.0f * 2.39996323f)),
    vec2(cos(94.0f * 2.39996323f), sin(94.0f * 2.39996323f)),
    vec2(cos(95.0f * 2.39996323f), sin(95.0f * 2.39996323f)),
    vec2(cos(96.0f * 2.39996323f), sin(96.0f * 2.39996323f)),
    vec2(cos(97.0f * 2.39996323f), sin(97.0f * 2.39996323f)),
    vec2(cos(98.0f * 2.39996323f), sin(98.0f * 2.39996323f)),
    vec2(cos(99.0f * 2.39996323f), sin(99.0f * 2.39996323f)),
    vec2(cos(100.0f * 2.39996323f), sin(100.0f * 2.39996323f)),
    vec2(cos(101.0f * 2.39996323f), sin(101.0f * 2.39996323f)),
    vec2(cos(102.0f * 2.39996323f), sin(102.0f * 2.39996323f)),
    vec2(cos(103.0f * 2.39996323f), sin(103.0f * 2.39996323f)),
    vec2(cos(104.0f * 2.39996323f), sin(104.0f * 2.39996323f)),
    vec2(cos(105.0f * 2.39996323f), sin(105.0f * 2.39996323f)),
    vec2(cos(106.0f * 2.39996323f), sin(106.0f * 2.39996323f)),
    vec2(cos(107.0f * 2.39996323f), sin(107.0f * 2.39996323f)),
    vec2(cos(108.0f * 2.39996323f), sin(108.0f * 2.39996323f)),
    vec2(cos(109.0f * 2.39996323f), sin(109.0f * 2.39996323f)),
    vec2(cos(110.0f * 2.39996323f), sin(110.0f * 2.39996323f)),
    vec2(cos(111.0f * 2.39996323f), sin(111.0f * 2.39996323f)),
    vec2(cos(112.0f * 2.39996323f), sin(112.0f * 2.39996323f)),
    vec2(cos(113.0f * 2.39996323f), sin(113.0f * 2.39996323f)),
    vec2(cos(114.0f * 2.39996323f), sin(114.0f * 2.39996323f)),
    vec2(cos(115.0f * 2.39996323f), sin(115.0f * 2.39996323f)),
    vec2(cos(116.0f * 2.39996323f), sin(116.0f * 2.39996323f)),
    vec2(cos(117.0f * 2.39996323f), sin(117.0f * 2.39996323f)),
    vec2(cos(118.0f * 2.39996323f), sin(118.0f * 2.39996323f)),
    vec2(cos(119.0f * 2.39996323f), sin(119.0f * 2.39996323f)),
    vec2(cos(120.0f * 2.39996323f), sin(120.0f * 2.39996323f)),
    vec2(cos(121.0f * 2.39996323f), sin(121.0f * 2.39996323f)),
    vec2(cos(122.0f * 2.39996323f), sin(122.0f * 2.39996323f)),
    vec2(cos(123.0f * 2.39996323f), sin(123.0f * 2.39996323f)),
    vec2(cos(124.0f * 2.39996323f), sin(124.0f * 2.39996323f)),
    vec2(cos(125.0f * 2.39996323f), sin(125.0f * 2.39996323f)),
    vec2(cos(126.0f * 2.39996323f), sin(126.0f * 2.39996323f)),
    vec2(cos(127.0f * 2.39996323f), sin(127.0f * 2.39996323f))
);
// Shared by the existing world receiver and main-view model diffuse lighting.
// Three cascades: the close fit (`close_map`, finest texels) nearest the camera, the view
// fit (`depth_map`) ahead of it, and a map-wide world-only far cascade (`far_map`) behind
// it, so no surface is ever assumed sunlit merely because it lies past a fit. The `.w`
// of each extra quality vector flags that cascade's presence.
struct Projection { uv: vec2<f32>, depth: f32, gradient: vec2<f32>, inside: bool };
fn disk(i: u32, count: u32) -> vec2<f32> {
    return sqrt((f32(i)+0.5)/f32(count)) * SUN_DIRECTIONS[i];
}
// Sub-texel normal offset plus receiver-plane derivatives; no world-space multi-unit lift.
// Derivatives are evaluated here, before any divergent rejection.
fn project(vp: mat4x4<f32>, texel: f32, world: vec3<f32>, normal: vec3<f32>,
    slope: bool) -> Projection {
    // A tenth of a texel along the normal. Every unit of bias lets sun through at a
    // crease where wall and floor meet: the blocker's and the receiver's depths agree at
    // the base, so the sunlit line along the base is as wide as the total bias over the
    // tangent of the sun's elevation. The receiver-plane term below covers the surface's
    // own slope; this only keeps the compare off the surface itself.
    let clip = vp * vec4(world + normal * texel * 0.1, 1.0);
    let uv = clip.xy * vec2(0.5, -0.5) + 0.5;
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let dz = vec2(dpdx(clip.z), dpdy(clip.z));
    let determinant = dx.x*dy.y-dx.y*dy.x;
    var gradient = vec2(0.0);
    if slope && abs(determinant) > 1e-12 {
        gradient = vec2(dy.y*dz.x-dx.y*dz.y, dx.x*dz.y-dy.x*dz.x)/determinant;
    }
    let inside = all(uv > vec2(0.0)) && all(uv < vec2(1.0)) && clip.z > 0.0 && clip.z < 1.0;
    return Projection(uv, clip.z, gradient, inside);
}
// Bilinearly reconstruct separation and blocked coverage, correcting the receiver
// plane at each texel centre. Interpolating raw depths would invent occluders at edges.
fn blockers(map: texture_depth_2d, p: Projection, depth: f32,
    dimensions: vec2<f32>, at: vec2<f32>) -> vec2<f32> {
    let base = vec2<i32>(floor(at));
    let blend = fract(at);
    let lo = clamp(base, vec2(0), vec2<i32>(dimensions)-1);
    let hi = clamp(base + vec2(1), vec2(0), vec2<i32>(dimensions)-1);
    let samples = vec4(textureLoad(map, lo, 0),
        textureLoad(map, vec2(hi.x, lo.y), 0),
        textureLoad(map, vec2(lo.x, hi.y), 0), textureLoad(map, hi, 0));
    let plane = dot(p.gradient, (vec2<f32>(lo) + 0.5)/dimensions - p.uv);
    let step = p.gradient * vec2<f32>(hi-lo)/dimensions;
    let gaps = max(vec4(depth + plane) + vec4(0.0, step.x, step.y, step.x+step.y)
        - samples, vec4(0.0));
    let weights = vec4((1.0-blend.x)*(1.0-blend.y), blend.x*(1.0-blend.y),
        (1.0-blend.x)*blend.y, blend.x*blend.y);
    return vec2(dot(gaps, weights), dot(select(vec4(0.0), weights, gaps > vec4(0.0)), vec4(1.0)));
}
// Same full-texel receiver-plane allowance for both layers; no added depth bias.
fn receiver_depth(p: Projection, dimensions: vec2<f32>, range: f32) -> f32 {
    return p.depth - dot(abs(p.gradient), 1.0/dimensions) - 0.05 / range;
}
// Sample a truncated Gaussian disk rather than an equal-weight disk with a hard rim.
// Fixed angles need no temporal history. A fractional final tap avoids count jumps.
fn reconstruct(map: texture_depth_2d, p: Projection, depth: f32,
    dimensions: vec2<f32>, radius: f32) -> f32 {
    let base_taps = shadow.quality.z;
    let count = clamp(base_taps * radius / 1.5, base_taps, base_taps * 4.0);
    var visibility = 0.0;
    for (var i = 0u; i < u32(ceil(count)); i++) {
        let quantile = min((f32(i)+0.5)/count, 1.0);
        // Inverse radial CDF of exp(-4*r*r), truncated at radius 1.
        let radial = sqrt(-log(1.0-quantile*0.98168436)*0.25);
        let offset = radial * SUN_DIRECTIONS[i] * radius/dimensions;
        visibility += min(count-f32(i), 1.0) * textureSampleCompareLevel(map,
            comparison, p.uv + offset, depth+dot(p.gradient, offset));
    }
    return visibility/count;
}
// Static-world penumbra: separation from the receiver, not camera distance. Search
// in world units so the close cascade cannot clip a tall caster's broad shadow at
// twelve tiny texels. The 24-unit bound limits receiver-plane extrapolation.
fn filtered(map: texture_depth_2d, p: Projection, texel: f32, range: f32,
    footprint: f32, layer: i32) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(map));
    let depth = receiver_depth(p, dimensions, range);
    let reach = max(24.0, footprint*1.41421356);
    if layer >= 0 {
        let bounded = bounded_visibility(p, dimensions, depth, reach/texel, layer);
        if bounded >= 0.0 { return bounded; }
    }
    var blocked = vec2(0.0);
    for (var i = 0u; i < 16u; i++) {
        let offset = select(disk(i, 16u)*reach/texel, vec2(0.0), i == 0u);
        blocked += blockers(map, p, depth, dimensions, p.uv*dimensions+offset-0.5);
    }
    let gap = blocked.x/max(blocked.y, 1e-6)*range;
    // Gaussian reconstruction retains approximately the disk's contact width.
    let radius = min(max(footprint, gap*0.0093)*1.41421356, reach)/texel;
    return reconstruct(map, p, depth, dimensions, radius);
}
// Moving casters use their own contact reconstruction. They cannot change the
// world's blocker estimate or kernel. Multiplying the separately filtered sun
// visibility approximates their union without brightening/reshaping the broad edge.
fn layered(moving: texture_depth_2d, world: texture_depth_2d, p: Projection,
    texel: f32, range: f32, footprint: f32, layer: i32) -> f32 {
    if shadow.quality.w <= 0.0 { return filtered(moving, p, texel, range, footprint, -1); }
    let static_visibility = filtered(world, p, texel, range, footprint, layer);
    if static_visibility <= 0.0 { return 0.0; }
    let dimensions = vec2<f32>(textureDimensions(moving));
    let dynamic_visibility = reconstruct(moving, p, receiver_depth(p, dimensions, range),
        dimensions, footprint*1.41421356/texel);
    return static_visibility*dynamic_visibility;
}
// Visibility and how much of it is known, respectively. The finest cascade covering the
// point answers; across each fit's axial fade band neighbouring cascades cross-blend;
// beyond every fit the far cascade alone. Without a far cascade, unknown air stays sunlit.
fn sun_visibility(world: vec3<f32>, normal: vec3<f32>, eye: vec3<f32>,
    forward: vec3<f32>) -> vec2<f32> {
    return sun_visibility_masked(world, normal, eye, forward, false);
}
// Baked receivers can already be completely occluded. Keep projection derivatives
// in uniform control flow, then omit filters whose result is multiplied by zero.
fn sun_visibility_masked(world: vec3<f32>, normal: vec3<f32>, eye: vec3<f32>,
    forward: vec3<f32>, fully_occluded: bool) -> vec2<f32> {
    // `r_dayDebug` bit 32: no sun shadow maps at all (everything the sun faces is lit).
    let debug = u32(shadow.realtime.w);
    if (debug & 32u) != 0u { return vec2(1.0, 1.0); }
    var coverage = 1.0;
    var close_coverage = 0.0;
    let slope = shadow.quality.w > 0.0;
    if slope {
        let distance = dot(world-eye, forward);
        // Refine gradually as the camera approaches. The old final-ten-percent
        // band changed close-map sharpness over only 25.6 units at default settings.
        coverage = 1.0-smoothstep(shadow.quality.w*0.75, shadow.quality.w, distance);
        if shadow.close_quality.w > 0.0 {
            close_coverage = 1.0-smoothstep(shadow.close_quality.z*0.5,
                shadow.close_quality.z, distance);
        }
    }
    let close = project(shadow.close_vp, shadow.close_quality.x, world, normal, slope);
    let near = project(shadow.vp, shadow.quality.x, world, normal, slope);
    let far = project(shadow.far_vp, shadow.far_quality.x, world, normal, slope);
    if fully_occluded && (debug & 1024u) == 0u { return vec2(0.0, 1.0); }
    var visibility = 1.0;
    var known = 0.0;
    if !close.inside { close_coverage = 0.0; }
    // Adjacent maps reconstruct the same world-space width while blending. The
    // width itself changes continuously toward the finer map's footprint, instead
    // of cross-fading an independently sharp edge with an independently soft one.
    let far_texel = select(shadow.quality.x, shadow.far_quality.x, shadow.far_quality.w > 0.0);
    let view_footprint = mix(far_texel, shadow.quality.x, coverage);
    let footprint = 1.5 * mix(view_footprint, shadow.close_quality.x, close_coverage);
    if shadow.far_quality.w > 0.0 && far.inside && coverage < 1.0 &&
        (u32(shadow.realtime.w) & 8u) == 0u {
        visibility = filtered(far_map, far, shadow.far_quality.x, shadow.far_quality.y, footprint, 2);
        known = 1.0;
    }
    if near.inside && coverage > 0.0 && close_coverage < 1.0 {
        let sharp = layered(depth_map, world_map, near, shadow.quality.x, shadow.quality.y, footprint, 0);
        visibility = mix(visibility, sharp, coverage);
        known = max(known, coverage);
    }
    // Bit 64: no close cascade (the near cascade serves the first 256 units too).
    if close_coverage > 0.0 && (debug & 64u) == 0u {
        let finest = layered(close_map, close_world_map, close, shadow.close_quality.x, shadow.close_quality.y, footprint, 1);
        visibility = mix(visibility, finest, close_coverage);
        known = max(known, close_coverage);
    }
    return vec2(visibility, known);
}
