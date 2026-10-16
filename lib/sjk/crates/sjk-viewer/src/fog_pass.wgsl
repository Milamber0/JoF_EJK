// RB_FogPass, codemp/rd-vanilla/tr_shade.cpp:1100-1128.
struct Fog {
    color: vec4<f32>,
    surface: vec4<f32>,
    bounds_min: vec4<f32>,
    bounds_max: vec4<f32>,
};
struct FogTable { entries: array<Fog, 32>, };
// Both mode banks are immutable; mode uses existing selector padding.
struct FogSelector { index: u32, global_exp2: u32, };
@group(1) @binding(0) var<uniform> fogs: FogTable;
@group(1) @binding(1) var<uniform> selector: FogSelector;
@group(3) @binding(5) var<uniform> stage: Stage;

struct FogOutput {
    @builtin(position) @invariant position: vec4<f32>,
    @location(0) st: vec2<f32>,
    @location(1) @interpolate(flat) fog_index: u32,
};

// RB_CalcFogTexCoords, tr_shade_calc.cpp:855-940, in world space after skinning.
// Interpolate coordinates, as rd-vanilla does before sampling its 256x32 image.
fn fog_coordinates(position: vec3<f32>, fog: Fog) -> vec2<f32> {
    let s = (dot(position, camera.view_forward)
        - dot(camera.camera_position, camera.view_forward)) * fog.bounds_min.w + 1.0 / 512.0;
    var t = dot(position, fog.surface.xyz) - fog.surface.w;
    var eye_t = dot(camera.camera_position, fog.surface.xyz) - fog.surface.w;
    if fog.bounds_max.w == 0.0 { eye_t = 1.0; t = 1.0; }
    if eye_t < 0.0 {
        if t < 1.0 { t = 1.0 / 32.0; }
        else { t = 1.0 / 32.0 + 30.0 / 32.0 * t / (t - eye_t); }
    } else {
        t = select(31.0 / 32.0, 1.0 / 32.0, t < 0.0);
    }
    return vec2(s, t);
}

fn fog_vertex(position: vec3<f32>, index: u32, depth_hack: f32) -> FogOutput {
    var output: FogOutput;
    output.position = clip_position(position, depth_hack);
    output.st = fog_coordinates(position, fogs.entries[index]);
    output.fog_index = index;
    return output;
}

fn instance_fog(index: u32, instance: InstanceInput) -> Fog {
    var fog = fogs.entries[index];
    if fog.bounds_max.w >= 2.0 {
        let normal = rotate_vector(instance.rotation, fog.surface.xyz / instance.scale);
        fog.surface = vec4(normal, fog.surface.w + dot(normal, instance.position.xyz));
    }
    return fog;
}

fn generated_fog_st(vertex: VertexInput, index: u32, fog: Fog, origin: vec3<f32>,
    rotation: vec4<f32>, scale: vec3<f32>) -> vec2<f32> {
    if !geometry_sprites || stage.sprites[0].x == 0.0 {
        return fog_coordinates(origin + rotate_vector(rotation, vertex.position * scale), fog);
    }
    let weights = vec3(vertex.lightmap_coordinates, 1.0 - vertex.lightmap_coordinates.x -
        vertex.lightmap_coordinates.y);
    var st = vec2(0.0);
    let reference = geometry_quads[index];
    for (var i = 0u; i < 3u; i++) {
        var source = geometry_vertex(reference.vertices[i]);
        for (var d = 0u; d < 3u; d++) {
            if stage.deform_a[d].x == 0.0 { break; }
            source = deform_point(source, d);
        }
        st += weights[i] * fog_coordinates(origin + rotate_vector(rotation, source.position *
            scale), fog);
    }
    return st;
}

@vertex fn vertex_main(input: VertexInput, @builtin(vertex_index) index: u32) -> FogOutput {
    let vertex = material_vertex(input, index, vec3(0.0), vec4(0.0, 0.0, 0.0, 1.0), vec3(1.0));
    var result = fog_vertex(vertex.position, selector.index, 0.0);
    result.st = generated_fog_st(vertex, index, fogs.entries[selector.index], vec3(0.0), vec4(0.0,
        0.0, 0.0, 1.0), vec3(1.0));
    return result;
}

@vertex fn instanced_vertex_main(input: VertexInput, instance: InstanceInput,
    @builtin(vertex_index) index: u32) -> FogOutput {
    let vertex = material_vertex(input, index, instance.position.xyz, instance.rotation,
        instance.scale);
    var result = fog_vertex(instance_position(vertex, instance), selector.index, 0.0);
    result.st = generated_fog_st(vertex, index, instance_fog(selector.index, instance),
        instance.position.xyz, instance.rotation, instance.scale);
    if !instance_visible(instance) { result.position = vec4(2.0, 2.0, 2.0, 1.0); }
    return result;
}

@vertex fn entity_vertex_main(input: VertexInput, instance: InstanceInput,
    @builtin(vertex_index) index: u32) -> FogOutput {
    let vertex = material_vertex(input, index, instance.position.xyz, instance.rotation,
        instance.scale);
    var result = fog_vertex(instance_position(vertex, instance), instance.fog_index,
        instance.position.w);
    if !instance_visible(instance) { result.position = vec4(2.0, 2.0, 2.0, 1.0); }
    return result;
}

// R_FogFactor + R_InitFogTable, tr_image.cpp:1180-1225. Analytic sqrt evaluates
// the stored curve per fragment, avoiding interpolation of vertex fog factors.
fn fog_factor(st: vec2<f32>) -> f32 {
    var s = st.x - 1.0 / 512.0;
    if s < 0.0 || st.y < 1.0 / 32.0 { return 0.0; }
    if st.y < 31.0 / 32.0 { s *= (st.y - 1.0 / 32.0) / (30.0 / 32.0); }
    return sqrt(min(s * 8.0, 1.0));
}

// GL_EXP2 global fog: tr_shade.cpp:1565,1574-1610; RB_FogPass excludes it at 1879.
// s already contains z / (depthForOpaque * 8); log(255) = logtestExp2 squared.
// Brush fogs always keep R_FogFactor, including zero-plane brush fogs.
fn fog_alpha(st: vec2<f32>, fog: Fog, global_exp2: bool) -> f32 {
    if global_exp2 && fog.color.w != 0.0 {
        let depth_ratio = (st.x - 1.0 / 512.0) * 8.0;
        return 1.0 - exp(-5.541263545 * depth_ratio * depth_ratio);
    }
    return fog_factor(st);
}

@fragment fn fragment_main(input: FogOutput) -> @location(0) vec4<f32> {
    if (u32(camera._padding) & 2u) != 0u { return vec4(0.0); }
    if input.fog_index == 0u { return vec4(0.0); }
    let fog = fogs.entries[input.fog_index];
    let global_exp2 = selector.global_exp2 != 0u;
    var color = fog.color.rgb;
    if fog.color.w != 0.0 && !global_exp2 {
        color = floor(clamp(color, vec3(0.0), vec3(1.0)) * 255.0) / 255.0;
    }
    return vec4(color, fog_alpha(input.st, fog, global_exp2));
}
