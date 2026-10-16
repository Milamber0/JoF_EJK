struct Fog { color: vec4<f32>, surface: vec4<f32>, low: vec4<f32>, high: vec4<f32> };
@group(0) @binding(0) var<uniform> p: Parameters;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(12) var world_map: texture_depth_2d;
@group(0) @binding(13) var close_world_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var<uniform> fogs: array<Fog, 32>;
@group(0) @binding(4) var input_volume: texture_3d<f32>;
@group(0) @binding(5) var output_volume: texture_storage_3d<rgba16float, write>;
@group(0) @binding(6) var volume_sampler: sampler;
@group(0) @binding(7) var tile_means: texture_3d<f32>;
@group(0) @binding(8) var close_map: texture_depth_2d;
@group(0) @binding(11) var far_map: texture_depth_2d;
// Per froxel column: nearest and farthest visible surface over its sample lattice.
@group(0) @binding(9) var column_range: texture_2d<f32>;
@group(0) @binding(10) var column_output: texture_storage_2d<rgba32float, write>;
@group(1) @binding(0) var scene_depth: texture_depth_2d;
const TILES: u32 = 6u;
// Sixteen-sample cells visit their four lattice corners and the centre first; a cell
// whose five agree (all sunlit, all shadowed, all inside a wall) is not on a beam edge and
// its remaining eleven samples would agree too, so it stops there.
const ORDER = array<u32, 16>(0u, 3u, 12u, 15u, 5u, 1u, 2u, 4u, 6u, 7u, 8u, 9u, 10u, 11u,
    13u, 14u);
fn lattice_samples() -> u32 { return select(select(4u,8u,p.grid.x >= 128u),16u,p.grid.x >= 192u); }

fn world(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let q = p.inverse*vec4(uv*vec2(2.0,-2.0)+vec2(-1.0,1.0), depth, 1.0);
    return q.xyz/q.w;
}
fn ray(uv: vec2<f32>) -> vec3<f32> { return normalize(world(uv, 1.0)-p.eye.xyz); }
fn phase(cosine: f32) -> f32 {
    let g = p.sun.w;
    return (1.0-g*g)/(12.5663706144*pow(1.0+g*g-2.0*g*cosine,1.5));
}
// Length fraction of the eye-to-sample segment inside a root brush fog. Using
// the whole segment avoids a discontinuity when a sample leaves the volume.
fn fog_segment(point: vec3<f32>, fog: Fog) -> f32 {
    let delta = point-p.eye.xyz;
    var enter = 0.0;
    var leave = 1.0;
    for (var axis = 0u; axis < 3u; axis++) {
        if abs(delta[axis]) < 1e-6 {
            if p.eye[axis] < fog.low[axis] || p.eye[axis] > fog.high[axis] { return 0.0; }
        } else {
            let a = (fog.low[axis]-p.eye[axis])/delta[axis];
            let b = (fog.high[axis]-p.eye[axis])/delta[axis];
            enter = max(enter,min(a,b));
            leave = min(leave,max(a,b));
        }
    }
    return max(leave-enter,0.0);
}
fn density(point: vec3<f32>) -> f32 {
    // Authored fog already supplies its color/extinction in the surface pass.
    // Add only shadowed sunlight, attenuated on its journey back to the eye.
    // eye.w carries r_drawfog's normalized mode (0=off, 1=volume, 2=EXP2).
    if p.eye.w == 0.0 { return p.color.w; }
    let axial = max(dot(point-p.eye.xyz,p.forward.xyz),0.0);
    var transmission = 1.0;
    for (var i = 1u; i <= min(p.grid.w,31u); i++) {
        let f = fogs[i];
        if f.low.w <= 0.0 || f.high.w >= 2.0 { continue; }
        let global = f.color.w > 0.0;
        var fraction = 1.0;
        if !global { fraction = fog_segment(point,f); }
        let ratio = axial*fraction*f.low.w*8.0;
        if global && p.eye.w == 2.0 {
            transmission *= exp(-5.541263545*ratio*ratio);
        } else {
            transmission *= 1.0-sqrt(clamp(ratio,0.0,1.0));
        }
    }
    return p.color.w*transmission;
}
// Axial distance of the visible surface on this screen ray; sky returns the far plane.
fn surface_axial(uv: vec2<f32>) -> f32 {
    let dimensions = vec2<f32>(textureDimensions(scene_depth));
    let pixel = clamp(vec2<i32>(uv*dimensions), vec2(0), vec2<i32>(dimensions)-1);
    return dot(world(uv,textureLoad(scene_depth,pixel,0))-p.eye.xyz,p.forward.xyz);
}
// Sunlight averaged over a froxel cell's own footprint, `radius` in shadow texels. Cells
// are far coarser than shadow texels, so a point sample aliases the beam edge into
// stair-steps across columns; pre-filtering to the cell size keeps the edge smooth.
fn sunlight(map: texture_depth_2d, world_map: texture_depth_2d, vp: mat4x4<f32>, point: vec3<f32>, radius: f32) -> f32 {
    let q = vp*vec4(point,1.0);
    let uv = q.xy*vec2(0.5,-0.5)+0.5;
    if any(uv <= vec2(0.0)) || any(uv >= vec2(1.0)) || q.z <= 0.0 || q.z >= 1.0 {
        return 0.0;
    }
    let depth = q.z-0.000001;
    if radius < 0.5 {
        return min(textureSampleCompareLevel(map,shadow_sampler,uv,depth),
            textureSampleCompareLevel(world_map,shadow_sampler,uv,depth));
    }
    let step = radius/vec2<f32>(textureDimensions(map));
    var light = 0.0;
    for (var i = 0u; i < 4u; i++) {
        let angle = 0.7853982+f32(i)*1.5707963;
        let sample_uv = uv+vec2(cos(angle),sin(angle))*step;
        light += min(textureSampleCompareLevel(map,shadow_sampler,sample_uv,depth),
            textureSampleCompareLevel(world_map,shadow_sampler,sample_uv,depth));
    }
    return light*0.25;
}
// Surface range of one column over the same lattice the injection samples, so cells
// entirely behind the visible surfaces store nothing and cells entirely in front skip the
// per-sample depth test.
@compute @workgroup_size(8,8,1) fn columns(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.grid.xy) { return; }
    let samples = lattice_samples();
    let cols = select(2u,4u,samples == 16u);
    let rows = samples/cols;
    var nearest = 1e30;
    var farthest = 0.0;
    for (var sample = 0u; sample < samples; sample++) {
        let lattice = vec2<f32>(f32(sample%cols),f32(sample/cols))+0.5;
        let offset = lattice/vec2<f32>(f32(cols),f32(rows));
        let axial = surface_axial((vec2<f32>(id.xy)+offset)/vec2<f32>(p.grid.xy));
        nearest = min(nearest, axial);
        farthest = max(farthest, axial);
    }
    // z: the surface at the column centre, which the composite weighs every pixel against.
    // It is constant per column: evaluated here once instead of per pixel per corner.
    let centre = surface_axial((vec2<f32>(id.xy)+0.5)/vec2<f32>(p.grid.xy));
    textureStore(column_output, vec2<i32>(id.xy), vec4(nearest, farthest, centre, 0.0));
}
@compute @workgroup_size(4,4,4) fn inject(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id >= p.grid.xyz) { return; }
    // Deterministic cell quadrature, not frame-varying jitter or temporal history. Every
    // sample has a distinct XY position (a cols x rows lattice) so partial beam coverage
    // is resolved in samples+1 levels, and depths are stratified with a coprime stride.
    let samples = lattice_samples();
    let cols = select(2u,4u,samples == 16u);
    let rows = samples/cols;
    let start = slice_depth(f32(id.z)/f32(p.grid.z));
    let end = slice_depth(f32(id.z+1u)/f32(p.grid.z));
    let range = textureLoad(column_range, vec2<i32>(id.xy), 0).xy;
    if start >= range.y {
        textureStore(output_volume,vec3<i32>(id),vec4(0.0));
        return;
    }
    let center = (vec2<f32>(id.xy)+0.5)/vec2<f32>(p.grid.xy);
    let cell_angle = length(ray(center+vec2(1.0/f32(p.grid.x),0.0))-ray(center));
    var source = vec3(0.0);
    var visible = 0u;
    var first_shade = -1.0;
    var agree = true;
    for (var i = 0u; i < samples; i++) {
        let sample = select(i, ORDER[i], samples == 16u);
        let lattice = vec2<f32>(f32(sample%cols),f32(sample/cols))+0.5;
        let offset = lattice/vec2<f32>(f32(cols),f32(rows));
        let uv = (vec2<f32>(id.xy)+offset)/vec2<f32>(p.grid.xy);
        let direction = ray(uv);
        let depth = mix(start,end,(f32((sample*5u)%samples)+0.5)/f32(samples));
        // Air behind the visible surface is not on this ray: a cell straddling an indoor
        // wall must not gather the sunlit outdoors beyond it (camera-relative wall streaks).
        var shade = -1.0;
        if depth < range.x || depth < surface_axial(uv) {
            visible += 1u;
            shade = cell_shade(uv, direction, depth, cell_angle);
            if shade > 0.0 {
                source += p.color.rgb*phase(dot(direction,p.sun.xyz))*shade;
            }
        }
        if samples == 16u {
            if i == 0u { first_shade = shade; }
            else if abs(shade - first_shade) > 1e-4 { agree = false; }
            if i == 4u && agree { break; }
        }
    }
    // Estimate the source in visible air, not its screen-space coverage. Hidden samples
    // are unavailable, not unlit air: counting them darkens the background beside a
    // foreground silhouette. Visible shadowed samples still count, preserving shadows.
    // Composite clips the integrated source to each pixel's own surface distance.
    textureStore(output_volume,vec3<i32>(id),vec4(source/f32(max(visible,1u)),0.0));
}
// Fade over the filter footprint before the projection boundary.
fn cascade_coverage(vp: mat4x4<f32>, point: vec3<f32>) -> f32 {
    let q = vp*vec4(point,1.0);
    let uv = q.xy*vec2(0.5,-0.5)+0.5;
    let edge = min(min(uv.x,1.0-uv.x),min(uv.y,1.0-uv.y));
    let margin = 8.0/f32(textureDimensions(shadow_map).x);
    return smoothstep(0.0,margin,edge)*select(0.0,1.0,q.z > 0.0 && q.z < 1.0);
}
// Sunlight times medium density at one sample of a cell.
fn cell_shade(uv: vec2<f32>, direction: vec3<f32>, depth: f32, cell_angle: f32) -> f32 {
    let point = p.eye.xyz+direction*(depth/dot(direction,p.forward.xyz));
    // Blend valid cascade interiors across their axial transitions. Out-of-fit samples
    // use the next cascade instead of being interpreted as opaque blockers.
    let close_weight = p.close_range.z * cascade_coverage(p.close, point)
        *(1.0-smoothstep(p.close_range.y*0.8,p.close_range.y*0.95,depth));
    let view_weight = cascade_coverage(p.shadow, point)
        *(1.0-smoothstep(p.range.y*0.85,p.range.y,depth));
    var light = 0.0;
    if close_weight < 1.0 {
        if view_weight < 1.0 && p.far_range.y > 0.0 {
            let radius = min(0.5*depth*cell_angle/max(p.far_range.x,0.001),6.0);
            light = sunlight(far_map,far_map,p.far,point,radius)*(1.0-view_weight);
        }
        if view_weight > 0.0 {
            let radius = min(0.5*depth*cell_angle/max(p.range.w,0.001),6.0);
            light += sunlight(shadow_map,world_map,p.shadow,point,radius)*view_weight;
        }
        light *= 1.0-close_weight;
    }
    if close_weight > 0.0 {
        let radius = min(0.5*depth*cell_angle/max(p.close_range.x,0.001),6.0);
        light += sunlight(close_map,close_world_map,p.close,point,radius)*close_weight;
    }
    return light*density(point);
}
// Mean sunlit source of one wide XY tile of one slice: the low-pass that clarity removes.
var<workgroup> partial: array<vec3<f32>, 64>;
@compute @workgroup_size(64,1,1) fn reduce(@builtin(workgroup_id) tile: vec3<u32>,
    @builtin(local_invocation_index) lane: u32) {
    let lo = tile.xy*p.grid.xy/TILES;
    let hi = (tile.xy+1u)*p.grid.xy/TILES;
    let count = (hi.x-lo.x)*(hi.y-lo.y);
    var sum = vec3(0.0);
    for (var i = lane; i < count; i += 64u) {
        let cell = lo+vec2(i%(hi.x-lo.x), i/(hi.x-lo.x));
        sum += textureLoad(input_volume,vec3<i32>(vec2<i32>(cell),i32(tile.z)),0).rgb;
    }
    partial[lane] = sum;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride >>= 1u) {
        if lane < stride { partial[lane] += partial[lane+stride]; }
        workgroupBarrier();
    }
    if lane == 0u {
        textureStore(output_volume,vec3<i32>(tile),vec4(partial[0]/f32(max(count,1u)),0.0));
    }
}
@compute @workgroup_size(8,8,1) fn integrate(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.grid.xy) { return; }
    let uv = (vec2<f32>(id.xy)+0.5)/vec2<f32>(p.grid.xy);
    let cosine = dot(ray(uv),p.forward.xyz);
    var scattering = vec3(0.0);
    var start_depth = slice_depth(0.0);
    for (var z = 0u; z < p.grid.z; z++) {
        var cell = textureLoad(input_volume,vec3<i32>(vec2<i32>(id.xy),i32(z)),0).rgb;
        // Rays are the contrast in sunlit air, not its mean: air lit uniformly across a
        // whole slice reads as fog, so clarity removes the wide, smoothly interpolated
        // tile mean. A beam narrower than a tile keeps most of its light; a fully sunlit
        // view adds nothing. Slice-local, so depth agreement is unchanged.
        let tile_uv = vec3(uv,(f32(z)+0.5)/f32(p.grid.z));
        let wide = textureSampleLevel(tile_means,volume_sampler,tile_uv,0.0).rgb;
        cell = max(cell-p.range.z*wide,vec3(0.0));
        // The previous end is this slice's exact start; avoid repeating its power.
        let end_depth = slice_depth(f32(z+1u)/f32(p.grid.z));
        let length = (end_depth-start_depth)/cosine;
        start_depth = end_depth;
        // Thin-air approximation: sunlit in-scattering only, no global extinction.
        scattering += cell*length;
        textureStore(output_volume,vec3<i32>(vec2<i32>(id.xy),i32(z)),
            vec4(scattering,1.0));
    }
}
@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    return vec4(positions[index],0.0,1.0);
}
@fragment fn composite(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = pixel.xy/vec2<f32>(textureDimensions(scene_depth));
    let depth = textureLoad(scene_depth,vec2<i32>(pixel.xy),0);
    let axial = dot(world(uv,depth)-p.eye.xyz,p.forward.xyz);
    if axial <= p.range.x { return vec4(0.0,0.0,0.0,1.0); }
    let distance = min(axial,p.far_range.z);
    let z = depth_layer(distance);
    let layer = min(i32(z),i32(p.grid.z)-1);
    let start = slice_depth(f32(layer)/f32(p.grid.z));
    let end = slice_depth(f32(layer+1)/f32(p.grid.z));
    // Prefix values are at slice ends: interpolate physical distance, not log depth.
    let fraction = clamp((distance-start)/(end-start),0.0,1.0);
    let position = uv*vec2<f32>(p.grid.xy)-0.5;
    let base = vec2<i32>(floor(position));
    let blend = fract(position);
    var sum = vec3(0.0);
    var weight_sum = 0.0;
    for (var y = 0; y < 2; y++) { for (var x = 0; x < 2; x++) {
        let column = clamp(base+vec2(x,y),vec2(0),vec2<i32>(p.grid.xy)-1);
        let column_axial = textureLoad(column_range,column,0).z;
        // Reject volume columns across a foreground depth discontinuity.
        let tolerance = max(2.0,axial*0.02);
        let mismatch = abs(column_axial-axial)/tolerance;
        let agreement = 1.0/(1.0+pow(mismatch,4.0));
        let bilinear = select(1.0-blend.x,blend.x,x == 1)*
            select(1.0-blend.y,blend.y,y == 1);
        let weight = bilinear*agreement;
        var before = vec4(0.0);
        if layer > 0 { before = textureLoad(input_volume,vec3(column,layer-1),0); }
        let after = textureLoad(input_volume,vec3(column,layer),0);
        let integral = mix(before,after,fraction);
        sum += integral.rgb*weight;
        weight_sum += weight;
    } }
    // Continuous bilateral weights avoid holes when all four columns lie on a sloping face.
    let normalization = max(weight_sum,1e-30);
    let scattering = max(sum/normalization,vec3(0.0));
    let luminance = dot(scattering,vec3(0.2126,0.7152,0.0722));
    // Colour-preserving soft limit on ADDED radiance, never on visibility or scene colour.
    // Y/(1+Y/0.02) increases monotonically: revealing sunlight cannot invert a beam.
    return vec4(scattering/(1.0+luminance/0.02),1.0);
}
