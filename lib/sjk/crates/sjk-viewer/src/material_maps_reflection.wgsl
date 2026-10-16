// Reflection probes on specular-mapped surfaces (`reflection_probes.rs`): rend2's
// `CalcIBLContribution` with Lagarde's box projection instead of its sphere radius.
struct ReflectionProbe { centre: vec4<f32>, box_min: vec4<f32>, box_max: vec4<f32> };
// control.x: the cubes' last level (roughness 1); centre.w: 1 once captured.
struct ReflectionProbes { control: vec4<u32>, probes: array<ReflectionProbe, 64> };
@group(1) @binding(13) var reflection_cubes: texture_cube_array<f32>;
@group(1) @binding(14) var reflection_sampler: sampler;
@group(1) @binding(15) var<uniform> reflection_probes: ReflectionProbes;
@group(1) @binding(16) var reflection_brdf: texture_2d<f32>;
// The surface's probe + 1 from its vertex frame, 0 without one.
var<private> material_map_probe: u32;
// The reflection last added, for `r_materialMapsDebug 4`.
var<private> material_map_reflected: vec3<f32>;

// The probe's reflection on the mapped normal (rgb), alpha 1 when a captured probe
// served it. Not in probe captures (camera flag 16: no reflections of reflections) or
// with `r_materialMapsDebug 5`.
fn material_map_reflection(world: vec3<f32>, response: MaterialMapResponse) -> vec4<f32> {
    let view_mode = (point_lights.metadata.z >> 8u) & 7u;
    if material_map_probe == 0u || view_mode == 5u || (u32(camera._padding) & 16u) != 0u {
        return vec4(0.0);
    }
    let probe = reflection_probes.probes[min(material_map_probe - 1u, 63u)];
    if probe.centre.w <= 0.0 { return vec4(0.0); }
    // Half the mapped tilt bends the reflected room: generated normal maps guess relief
    // from paint, and at full tilt every guessed bump warps the box-projected room like
    // a funhouse mirror that ripples as the view moves.
    let surface = material_map_surface;
    let normal = normalize(mix(surface.geometric, surface.normal, 0.5));
    let view = normalize(camera.camera_position - world);
    let facing = clamp(dot(normal, view), 0.0, 1.0);
    let ray = reflect(-view, normal);
    // Box projection: where the reflected ray leaves the probe's room, seen from the
    // probe. Outside its room (a surface the nearest probe does not stand in) the
    // direction alone, as from infinitely far.
    var lookup = ray;
    let low = probe.box_min.xyz - vec3(1.0);
    let high = probe.box_max.xyz + vec3(1.0);
    if all(world >= low) && all(world <= high) {
        let safe = select(ray, vec3(1e-5), abs(ray) < vec3(1e-5));
        let exits = max((high - world)/safe, (low - world)/safe);
        let distance = min(min(exits.x, exits.y), exits.z);
        lookup = world + ray*max(distance, 0.0) - probe.centre.xyz;
    }
    let level = response.roughness*f32(reflection_probes.control.x);
    let radiance = textureSampleLevel(reflection_cubes, reflection_sampler, lookup,
        i32(material_map_probe - 1u), level).rgb;
    let brdf = textureSampleLevel(reflection_brdf, reflection_sampler,
        vec2(response.roughness, facing), 0.0).rg;
    material_map_reflected = radiance*(response.specular*brdf.x + brdf.y)*response.occlusion;
    return vec4(material_map_reflected, 1.0);
}
