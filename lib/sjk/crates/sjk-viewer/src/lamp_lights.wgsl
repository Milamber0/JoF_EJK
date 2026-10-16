// Local lights from the map's emissive faces (see lamp_lights.rs): Lambertian area
// lights, each cell of a world grid listing the lamps that reach it. One storage buffer
// holds lamps (five vec4 of floats, bit-cast), the cell table and the cell lists.
struct LampGrid { origin: vec4<i32>, counts: vec4<u32>, cell: vec4<f32>, offsets: vec4<u32>, selection:vec4<u32> };
@group(3) @binding(24) var<uniform> lamp_grid: LampGrid;
@group(3) @binding(25) var<storage, read> lamp_data: array<vec4<u32>>;
fn lamp_importance_threshold(world:vec3<f32>,index:vec3<i32>)->f32 {
    if lamp_grid.selection.y==0u {return 0.0;}
    let fraction=fract(world/lamp_grid.cell.x);
    let t=fraction*fraction*(3.0-2.0*fraction);
    let dims=lamp_grid.counts.xyz+vec3(1u);
    var value=0.0;
    for(var z=0u;z<2u;z++) {for(var y=0u;y<2u;y++) {for(var x=0u;x<2u;x++) {
        let p=vec3<u32>(index)+vec3(x,y,z);
        let k=p.x+(p.y+p.z*dims.y)*dims.x;
        let node=bitcast<f32>(lamp_data[lamp_grid.selection.x+k/4u][k&3u]);
        let weight=select(1.0-t.x,t.x,x==1u)*select(1.0-t.y,t.y,y==1u)*select(1.0-t.z,t.z,z==1u);
        value+=node*weight;
    }}}
    return value;
}
// Irradiance of one lamp at `world` before any shadowing, display units; zero when the
// lamp cannot reach the receiver. `lamp` is its first row in `lamp_data`.
fn lamp_unshadowed(lamp: u32, world: vec3<f32>, normal: vec3<f32>, threshold: f32) -> vec3<f32> {
    let position_radius = bitcast<vec4<f32>>(lamp_data[lamp]);
    let normal_power = bitcast<vec4<f32>>(lamp_data[lamp + 1u]);
    let color = bitcast<vec4<f32>>(lamp_data[lamp + 2u]);
    let to = position_radius.xyz - world;
    let d2 = dot(to, to);
    let d = sqrt(d2);
    let radius = position_radius.w;
    if d >= radius { return vec3(0.0); }
    let l = to/max(d, 1e-3);
    // The face emits over its front hemisphere; the receiver takes the cosine; the
    // finite area integral supplies close-range falloff; a smooth window ends its reach.
    let omni = dot(normal_power.xyz,normal_power.xyz)<0.5;
    let emit = select(max(dot(-l, normal_power.xyz), 0.0),1.0,omni);
    let receive = max(dot(l, normal), 0.0);
    if !omni && !lamp_front_side(world,position_radius.xyz,normal_power.xyz) { return vec3(0.0); }
    let u=bitcast<vec4<f32>>(lamp_data[lamp+3u]).xyz;
    let v=bitcast<vec4<f32>>(lamp_data[lamp+4u]).xyz;
    if color.w>0.0 {
        if dot(to,normal)+abs(dot(u,normal))+abs(dot(v,normal))<=0.0 { return vec3(0.0); }
    } else if receive<=0.0 { return vec3(0.0); }
    let ratio = d/radius;
    let window = 1.0 - ratio*ratio;
    let importance=normal_power.w*window*window/(d2+dot(u,u)+dot(v,v)+256.0);
    var influence=1.0;
    if threshold>0.0 {influence=smoothstep(threshold,1.25*threshold,importance);}
    if influence<=0.0 { return vec3(0.0); }
    var form = emit*receive/(d2+256.0);
    if color.w>0.0 {
        form=lamp_area_form(world,normal,position_radius.xyz,normal_power.xyz,u,v,color.w);
    }
    return color.rgb*(normal_power.w*form*window*window*influence);
}
// The receiver's lamp list: (first entry, count), empty outside the grid.
fn lamp_cell(world: vec3<f32>) -> vec4<i32> {
    if lamp_grid.counts.w == 0u { return vec4(0, 0, 0, -1); }
    let index = vec3<i32>(floor(world/lamp_grid.cell.x)) - lamp_grid.origin.xyz;
    if any(index < vec3(0)) || any(index >= vec3<i32>(lamp_grid.counts.xyz)) { return vec4(0, 0, 0, -1); }
    return vec4(index, 0);
}
// The receiver's lamp list (first entry, count) in its grid cell `index` (`lamp_cell`).
fn lamp_entry(world: vec3<f32>, index: vec3<i32>) -> vec2<u32> {
    let cell = u32(index.x) + (u32(index.y) + u32(index.z)*lamp_grid.counts.y)*lamp_grid.counts.x;
    let pair = lamp_data[lamp_grid.offsets.x + cell/2u];
    var entry = select(pair.xy, pair.zw, (cell & 1u) == 1u);
    if (entry.y & 0x80000000u) != 0u {
        let side=entry.y & 255u;
        let sub=min(vec3<u32>(fract(world/lamp_grid.cell.x)*f32(side)),vec3(side-1u));
        let child=entry.x+sub.x+(sub.y+sub.z*side)*side;
        let nested=lamp_data[lamp_grid.offsets.x+child/2u];
        entry=select(nested.xy,nested.zw,(child&1u)==1u);
    }
    return entry;
}
// Lamp list entry `k` at the receiver: unshadowed irradiance (rgb) and shadow (a), zero
// when it cannot reach. Callers form rgb*a beside their running sum, so it still compiles
// to the fused multiply-add the light has always been summed with.
fn lamp_term(k: u32, world: vec3<f32>, normal: vec3<f32>, threshold: f32) -> vec4<f32> {
    let lamp = lamp_data[lamp_grid.offsets.y + k/4u][k & 3u]*5u;
    let term = lamp_unshadowed(lamp, world, normal, threshold);
    if all(term <= vec3(0.0)) { return vec4(0.0); }
    let to = bitcast<vec4<f32>>(lamp_data[lamp]).xyz - world;
    let shadow = lamp_shadow(lamp/5u, world, -to, normal);
    return vec4(term, shadow);
}
// Irradiance from the lamps reaching `world` on a surface facing `normal`, display units.
fn lamp_light(world: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let at = lamp_cell(world);
    if at.w < 0 { return vec3(0.0); }
    let entry = lamp_entry(world, at.xyz);
    let threshold = lamp_importance_threshold(world, at.xyz);
    var sum = vec3(0.0);
    for (var i = 0u; i < entry.y; i++) {
        let light = lamp_term(entry.x + i, world, normal, threshold);
        if light.w <= 0.0 { continue; }
        sum += light.xyz*light.w;
    }
    return sum;
}
// Lamp irradiance (`light`) with the sum of the directions it arrives from, each weighted
// by its luminance (`vector`): the vector's length over the light's luminance is the share
// that arrives from one direction. Material maps move that share to the mapped normal.
struct LampLight { light: vec3<f32>, vector: vec3<f32> };
fn lamp_luminance(rgb: vec3<f32>) -> f32 { return dot(rgb, vec3(0.2126, 0.7152, 0.0722)); }
// Unit direction from `world` toward the centre of lamp list entry `k`.
fn lamp_direction(k: u32, world: vec3<f32>) -> vec3<f32> {
    let lamp = lamp_data[lamp_grid.offsets.y + k/4u][k & 3u]*5u;
    let to = bitcast<vec4<f32>>(lamp_data[lamp]).xyz - world;
    return to*inverseSqrt(max(dot(to, to), 1e-6));
}
fn lamp_light_directed(world: vec3<f32>, normal: vec3<f32>) -> LampLight {
    let at = lamp_cell(world);
    if at.w < 0 { return LampLight(vec3(0.0), vec3(0.0)); }
    let entry = lamp_entry(world, at.xyz);
    let threshold = lamp_importance_threshold(world, at.xyz);
    var sum = vec3(0.0);
    var vector = vec3(0.0);
    for (var i = 0u; i < entry.y; i++) {
        let light = lamp_term(entry.x + i, world, normal, threshold);
        if light.w <= 0.0 { continue; }
        sum += light.xyz*light.w;
        vector += lamp_direction(entry.x + i, world)*lamp_luminance(light.xyz*light.w);
    }
    return LampLight(sum, vector);
}
// LAMP_SHADOWS_BEGIN
// Immutable static visibility multiplied by the optional actor-shadow slot.
struct LampShadowTable { lamps: array<vec4<u32>, 2>, count: vec4<u32>, weights: array<vec4<f32>, 2>, vp: array<mat4x4<f32>, 48> };
@group(3) @binding(29) var lamp_dynamic_depth: texture_depth_2d_array;
@group(3) @binding(30) var<uniform> lamp_shadow_table: LampShadowTable;
fn lamp_shadow(lamp: u32, world: vec3<f32>, from_lamp: vec3<f32>, normal: vec3<f32>) -> f32 {
    let visibility = lamp_static_visibility(lamp,world,normal);
    if visibility <= 0.0 { return 0.0; }
    var slot = 8u;
    for (var s = 0u; s < 8u; s++) {
        if lamp_shadow_table.lamps[s/4u][s%4u] == lamp { slot = s; }
    }
    if slot == 8u { return visibility; }
    return visibility*lamp_slot_light(slot, world, from_lamp, normal);
}
// The share of a slotted lamp's light that actors leave at the receiver: 1 unshadowed.
fn lamp_slot_light(slot: u32, world: vec3<f32>, from_lamp: vec3<f32>, normal: vec3<f32>) -> f32 {
    // Pick the face using the same biased receiver that is projected below.
    // Otherwise a close receiver can move outside the selected face at a seam.
    let receiver=from_lamp+normal*3.0;
    let a = abs(receiver);
    var face = 0u;
    if a.y >= a.x && a.y >= a.z { face = 2u; } else if a.z > a.x && a.z > a.y { face = 4u; }
    let negative = select(select(receiver.x, receiver.y, face == 2u), receiver.z, face == 4u) < 0.0;
    if negative { face += 1u; }
    let layer = slot*6u + face;
    // `count.yz`: one bit per face layer that holds an actor this frame.
    if ((lamp_shadow_table.count[1u + layer/32u] >> (layer%32u)) & 1u) == 0u { return 1.0; }
    let clip = lamp_shadow_table.vp[layer]*vec4(world+normal*3.0, 1.0);
    if clip.w <= 0.0 { return 1.0; }
    let uv = clip.xy/clip.w*vec2(0.5, -0.5) + 0.5;
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) { return 1.0; }
    let depth = clip.z/clip.w - 0.0008;
    // Four bilinear compares on a rotated grid (a texel and a half across): a soft
    // penumbra instead of the face's texel staircase along a shadow edge.
    let texel = 1.0/vec2<f32>(textureDimensions(lamp_dynamic_depth).xy);
    var lit = 0.0;
    for (var i = 0u; i < 4u; i++) {
        let offset = select(vec2(-0.375, -0.75), select(vec2(0.75, -0.375),
            select(vec2(-0.75, 0.375), vec2(0.375, 0.75), i == 3u), i == 2u), i == 1u)*texel;
        lit += textureSampleCompareLevel(lamp_dynamic_depth, comparison, uv + offset, i32(layer), depth);
    }
    return mix(1.0,lit*0.25,lamp_shadow_table.weights[slot/4u][slot%4u]);
}
// Lamp light from the static cache (`lamp_cache.rs`): the cached sum already carries
// every lamp's static visibility, so only the light actors take from slotted lamps is
// evaluated here, and only where an actor's shadow actually falls.
fn lamp_light_cached(cached: vec3<f32>, world: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    var sum = cached;
    let at = lamp_cell(world);
    if at.w < 0 { return sum; }
    var threshold = -1.0;
    for (var s = 0u; s < 8u; s++) {
        let lamp = lamp_shadow_table.lamps[s/4u][s%4u];
        if lamp == 0xffffffffu { continue; }
        let to = bitcast<vec4<f32>>(lamp_data[lamp*5u]).xyz - world;
        let radius = bitcast<vec4<f32>>(lamp_data[lamp*5u]).w;
        if dot(to, to) >= radius*radius { continue; }
        let kept = lamp_slot_light(s, world, -to, normal);
        if kept >= 1.0 { continue; }
        if threshold < 0.0 { threshold = lamp_importance_threshold(world, at.xyz); }
        let term = lamp_unshadowed(lamp*5u, world, normal, threshold);
        if all(term <= vec3(0.0)) { continue; }
        sum -= term*(lamp_static_visibility(lamp, world, normal)*(1.0 - kept));
    }
    return max(sum, vec3(0.0));
}
// LAMP_SHADOWS_END
