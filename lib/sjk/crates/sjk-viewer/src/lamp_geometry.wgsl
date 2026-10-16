// Static triangle visibility for fixture-atlas generation. Sky boundaries can be
// traversed without treating them as walls. Work and stack depth are bounded.
struct GiHeader { counts:vec4<u32>, settings:vec4<f32> };
struct GiSurface { albedo: vec4<f32>, emission: vec4<f32> };
@group(0) @binding(0) var<uniform> gi: GiHeader;
@group(0) @binding(1) var<storage, read> gi_nodes: array<vec4<u32>>;
@group(0) @binding(2) var<storage, read> gi_triangles: array<vec4<u32>>;
@group(0) @binding(3) var<storage, read> gi_surfaces: array<GiSurface>;
struct GiHit { distance: f32, normal: vec3<f32>, hit: bool, sky: bool, material: u32 };
fn gi_surface(hit: GiHit) -> GiSurface { return gi_surfaces[min(hit.material, gi.counts.z-1u)]; }
fn gi_box(origin: vec3<f32>, direction: vec3<f32>, lo: vec3<f32>, hi: vec3<f32>, range: f32) -> bool {
    var near = 0.0; var far = range;
    for (var axis = 0; axis < 3; axis++) {
        if abs(direction[axis]) < 1e-8 {
            if origin[axis] < lo[axis] || origin[axis] > hi[axis] { return false; }
        } else {
            let a = (lo[axis]-origin[axis])/direction[axis];
            let b = (hi[axis]-origin[axis])/direction[axis];
            near = max(near,min(a,b)); far = min(far,max(a,b));
        }
    }
    return near <= far;
}
fn gi_trace(origin: vec3<f32>, direction: vec3<f32>, range: f32) -> GiHit {
    return gi_trace_through(origin,direction,range,false);
}
fn gi_trace_through(origin: vec3<f32>, direction: vec3<f32>, range: f32, through_sky: bool) -> GiHit {
    var result = GiHit(range,vec3(0.0),false,false,0u);
    if gi.counts.y == 0u { return result; }
    var stack: array<u32,32>; stack[0] = 0u; var count = 1u;
    // A balanced tree is at most twice its triangle count. The bound cannot cut short
    // a valid traversal; unlike a distance budget, exhausting the domain is no sky hit.
    for (var visited = 0u; visited < gi.counts.x && count > 0u; visited++) {
        count -= 1u; let node = stack[count]*2u;
        let lo = gi_nodes[node]; let hi = gi_nodes[node+1u];
        if !gi_box(origin,direction,bitcast<vec3<f32>>(lo.xyz),bitcast<vec3<f32>>(hi.xyz),result.distance) { continue; }
        if hi.w == 0u {
            stack[count] = lo.w; stack[count+1u] = lo.w+1u; count += 2u;
            continue;
        }
        for (var i = 0u; i < hi.w; i++) {
            let t = (lo.w+i)*3u;
            let a = gi_triangles[t]; let b = gi_triangles[t+1u]; let c = gi_triangles[t+2u];
            if through_sky && b.w != 0u { continue; }
            let e1 = bitcast<vec3<f32>>(b.xyz); let e2 = bitcast<vec3<f32>>(c.xyz);
            let h = cross(direction,e2); let determinant = dot(e1,h);
            if abs(determinant) < 1e-8 { continue; }
            let s = origin-bitcast<vec3<f32>>(a.xyz);
            let u = dot(s,h)/determinant;
            let q = cross(s,e1); let v = dot(direction,q)/determinant;
            let distance = dot(e2,q)/determinant;
            if u < -1e-6 || v < -1e-6 || u+v > 1.000001 || distance < 1e-3 || distance > result.distance { continue; }
            // A solid sharing a boundary with sky wins independently of BVH order.
            if b.w != 0u && result.hit && !result.sky && abs(distance-result.distance)<1e-3 { continue; }
            let normal = normalize(cross(e1,e2));
            result = GiHit(distance,select(normal,-normal,dot(normal,direction)>0.0),true,b.w!=0u,a.w);
        }
    }
    return result;
}
