@group(1) @binding(0) var reflected_scene: texture_2d<f32>;
// Inverse scene dimensions, packed raster scale, and finish roughness.
@group(1) @binding(1) var<uniform> finish: vec4<f32>;
@group(1) @binding(2) var reflected_sampler: sampler;
// The floor material's maps (`material_maps::FloorMaps`), or neutral stand-ins with no
// flags: the normal map bends the mirror lookup, the specular map's roughness widens the
// blur and its occlusion dims the reflection.
struct FloorMaps {
    // rend2 normalScale, specularScale, then flags (1 normal map), specular layout
    // (0 none, 1 spec/gloss, 2 packed), unused, unused (`material_maps::Params`).
    normal_scale: vec4<f32>,
    specular_scale: vec4<f32>,
    control: vec4<f32>,
};
@group(2) @binding(0) var floor_normal: texture_2d<f32>;
@group(2) @binding(1) var floor_specular: texture_2d<f32>;
@group(2) @binding(2) var floor_map_sampler: sampler;
@group(2) @binding(3) var<uniform> floor_maps: FloorMaps;
// Roughness the region margins allow (`floor_reflection_finish::MAPPED_ROUGHNESS`).
const FLOOR_MAX_ROUGHNESS: f32 = 0.6;
// Distance assumed to the reflected scene when a bent normal moves the lookup.
const FLOOR_DEPTH: f32 = 48.0;
struct FloorOutput {
    @builtin(position) @invariant position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};
@vertex fn floor_vertex(input: VertexInput) -> FloorOutput {
    return FloorOutput(camera.view_projection*vec4(input.position,1.0),input.position,input.normal,
        input.texture_coordinates);
}
fn floor_screen(world: vec3<f32>) -> vec2<f32> {
    let clip = camera.view_projection*vec4(world, 1.0);
    return clip.xy/max(clip.w, 1e-4);
}
@fragment fn floor_fragment(input: FloorOutput) -> @location(0) vec4<f32> {
    var uv = vec2(1.0-input.position.x*finish.x,input.position.y*finish.y)*finish.z;
    let geometric = normalize(input.normal);
    let view = normalize(camera.camera_position-input.world);
    var normal = geometric;
    var roughness = finish.w;
    var occlusion = 1.0;
    let flags = u32(floor_maps.control.x);
    let specular_layout = u32(floor_maps.control.y);
    // Flags and layout are per draw (uniform): derivatives and samples stay in uniform flow.
    let dp1 = dpdx(input.world);
    let dp2 = dpdy(input.world);
    let duv1 = dpdx(input.uv);
    let duv2 = dpdy(input.uv);
    if (flags & 1u) != 0u {
        // A frame from the derivatives (the floor is planar): +s along the tangent, +t
        // along the bitangent, as the material program's frames are.
        let across = cross(dp2, geometric);
        let along = cross(geometric, dp1);
        let tangent = across*duv1.x + along*duv2.x;
        let bitangent = across*duv1.y + along*duv2.y;
        let scale = inverseSqrt(max(max(dot(tangent, tangent), dot(bitangent, bitangent)), 1e-12));
        var n = textureSample(floor_normal, floor_map_sampler, input.uv).rgb - vec3(0.5);
        n = vec3(n.xy*floor_maps.normal_scale.xy, 0.0);
        n.z = sqrt(clamp((0.25 - n.x*n.x) - n.y*n.y, 0.0, 1.0));
        normal = normalize(n.x*tangent*scale + n.y*bitangent*scale + n.z*geometric);
        // The bent reflection, mirrored back through the floor, moves the reflected point
        // seen here: read the mirror image where that point appears on screen.
        let shift = reflect(-view, normal) - reflect(-view, geometric);
        let mirrored = shift - 2.0*geometric*dot(geometric, shift);
        let moved = floor_screen(input.world + mirrored*FLOOR_DEPTH) - floor_screen(input.world);
        // At most the margin the region keeps for it (`MAPPED_MARGIN`).
        let limit = FLOOR_MAX_ROUGHNESS*12.0/1080.0*finish.z*vec2(finish.x/finish.y, 1.0);
        uv += clamp(-moved*0.5*finish.z, -limit, limit);
    }
    if specular_layout != 0u {
        let texel = textureSample(floor_specular, floor_map_sampler, input.uv);
        let scale = floor_maps.specular_scale;
        if specular_layout == 1u {
            roughness = mix(1.0, 0.01, texel.a*(1.0 - scale.w));
        } else {
            let orms = texel*scale.zwxy;
            roughness = mix(0.01, 1.0, orms.y);
            occlusion = orms.x;
        }
        roughness = min(roughness, FLOOR_MAX_ROUGHNESS);
    }
    let half_texel = finish.xy*0.5;
    let high = max(half_texel,vec2(finish.z)-half_texel);
    // A fixed angular footprint: soft at every resolution, without noise or LOD switches.
    // Normalized weights keep a constant reflection's radiance unchanged.
    let reach = roughness*12.0/1080.0*finish.z*vec2(finish.x/finish.y,1.0);
    var reflection = vec3(0.0);
    for (var y = -1; y <= 1; y++) { for (var x = -1; x <= 1; x++) {
        let weight = select(1.0,2.0,x==0)*select(1.0,2.0,y==0)/16.0;
        let at = clamp(uv+vec2<f32>(f32(x),f32(y))*reach,half_texel,high);
        reflection += textureSampleLevel(reflected_scene,reflected_sampler,at,0.0).rgb*weight;
    }}
    let facing = clamp(dot(normal,view),0.0,1.0);
    // A polished finish retains its diffuse pattern and gains reflection at grazing angles.
    let polish = 0.12+0.58*pow(1.0-facing,5.0);
    let strength = polish*(1.0-0.6*roughness)*occlusion;
    return vec4(reflection,strength);
}
