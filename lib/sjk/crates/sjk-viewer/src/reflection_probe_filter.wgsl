// Reflection probe filtering (`reflection_probe_gpu.rs`), compute only. A captured face
// is copied into the scratch cube's top level, the scratch is box-downsampled to 1x1,
// then every level of the probe's slot in the cube array is filtered from it: level 0
// is the mirror image, level m the GGX lobe of roughness m/levels (rend2
// `prefilterEnvMap.glsl`, with filtered importance sampling from the scratch's mips).

// The cube convention (WebGPU/Vulkan/D3D): direction of the texel at (s, t) in -1..1 of
// face `face` (+X, -X, +Y, -Y, +Z, -Z), t growing down the image.
fn cube_direction(face: u32, s: f32, t: f32) -> vec3<f32> {
    switch face {
        case 0u: { return normalize(vec3(1.0, -t, -s)); }
        case 1u: { return normalize(vec3(-1.0, -t, s)); }
        case 2u: { return normalize(vec3(s, 1.0, t)); }
        case 3u: { return normalize(vec3(s, -1.0, -t)); }
        case 4u: { return normalize(vec3(s, -t, 1.0)); }
        default: { return normalize(vec3(-s, -t, -1.0)); }
    }
}

// Copy: the capture camera is right-handed (look-at with the face's up), the cube faces
// are left-handed, so a face is the capture mirrored left to right.
@group(0) @binding(0) var capture: texture_2d<f32>;
@group(0) @binding(1) var scratch_top: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(2) var<uniform> copy_face: vec4<u32>;
@compute @workgroup_size(8, 8) fn copy(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(scratch_top).x;
    if id.x >= size || id.y >= size { return; }
    let texel = textureLoad(capture, vec2(size - 1u - id.x, id.y), 0);
    textureStore(scratch_top, id.xy, copy_face.x, vec4(max(texel.rgb, vec3(0.0)), 1.0));
}

// Downsample: each texel the mean of the four below it.
@group(0) @binding(0) var finer: texture_2d_array<f32>;
@group(0) @binding(1) var coarser: texture_storage_2d_array<rgba16float, write>;
@compute @workgroup_size(8, 8, 1) fn downsample(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(coarser).x;
    if id.x >= size || id.y >= size { return; }
    let base = vec2<i32>(id.xy*2u);
    let layer = i32(id.z);
    let sum = textureLoad(finer, base, layer, 0) + textureLoad(finer, base + vec2(1, 0), layer, 0)
        + textureLoad(finer, base + vec2(0, 1), layer, 0)
        + textureLoad(finer, base + vec2(1, 1), layer, 0);
    textureStore(coarser, id.xy, layer, sum*0.25);
}

// Prefilter one level of one probe.
struct Level { roughness: f32, source_size: f32, source_levels: f32, unused: f32 };
@group(0) @binding(0) var source: texture_cube<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var filtered: texture_storage_2d_array<rgba16float, write>;
@group(0) @binding(3) var<uniform> level: Level;
// x: the probe's first layer (6 * slot).
@group(0) @binding(4) var<uniform> probe: vec4<u32>;
const SAMPLES: u32 = 64u;
fn hammersley(i: u32) -> vec2<f32> {
    return vec2(f32(i)/f32(SAMPLES), f32(reverseBits(i))*2.3283064365386963e-10);
}
@compute @workgroup_size(8, 8, 1) fn prefilter(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(filtered).x;
    if id.x >= size || id.y >= size { return; }
    let st = (vec2<f32>(id.xy) + 0.5)/f32(size)*2.0 - 1.0;
    let normal = cube_direction(id.z, st.x, st.y);
    var color = vec3(0.0);
    if level.roughness <= 0.0 {
        color = textureSampleLevel(source, source_sampler, normal, 0.0).rgb;
    } else {
        let a = level.roughness*level.roughness;
        let a2 = a*a;
        let up = select(vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), abs(normal.z) < 0.999);
        let tangent_x = normalize(cross(up, normal));
        let tangent_y = cross(normal, tangent_x);
        let texel_angle = 4.0*3.14159265/(6.0*level.source_size*level.source_size);
        var weight = 0.0;
        for (var i = 0u; i < SAMPLES; i++) {
            let xi = hammersley(i);
            let phi = 6.2831853*xi.x;
            let cos_theta = sqrt((1.0 - xi.y)/(1.0 + (a2 - 1.0)*xi.y));
            let sin_theta = sqrt(max(1.0 - cos_theta*cos_theta, 0.0));
            let half = tangent_x*(sin_theta*cos(phi)) + tangent_y*(sin_theta*sin(phi))
                + normal*cos_theta;
            let light = 2.0*dot(normal, half)*half - normal;
            let n_l = dot(normal, light);
            if n_l <= 0.0 { continue; }
            // N = V = R: the pdf over light directions is D(h)/4.
            let n_h = max(dot(normal, half), 0.0);
            let d = (n_h*a2 - n_h)*n_h + 1.0;
            let pdf = a2/(3.14159265*d*d)*0.25 + 1e-4;
            let sample_angle = 1.0/(f32(SAMPLES)*pdf + 1e-4);
            let lod = clamp(0.5*log2(sample_angle/texel_angle) + 1.0, 0.0,
                level.source_levels - 1.0);
            color += textureSampleLevel(source, source_sampler, light, lod).rgb*n_l;
            weight += n_l;
        }
        color /= max(weight, 1e-4);
    }
    textureStore(filtered, id.xy, i32(probe.x + id.z), vec4(color, 1.0));
}
