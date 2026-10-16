// Baked light grid sampling shared by entity shading and the day-mode world relight.
struct EntityLight {
    ambient: vec3<f32>,
    directed: vec3<f32>,
    direction: vec3<f32>,
};
// tr_light.cpp:134-264: preserve indirection, flat-index bounds, wall skips and renormalisation.
struct ModelGrid {
    origin: vec4<f32>,
    inverse: vec4<f32>,
    bounds: vec4<u32>,
    offsets: vec4<u32>,
    words: array<u32>,
};
@group(1) @binding(7) var<storage, read> model_grid: ModelGrid;

fn grid_vector(offset: u32) -> vec3<f32> {
    return vec3(bitcast<f32>(model_grid.words[offset]),
        bitcast<f32>(model_grid.words[offset + 1u]),
        bitcast<f32>(model_grid.words[offset + 2u]));
}

fn grid_unit(value: vec3<f32>) -> vec3<f32> {
    let magnitude = length(value);
    if magnitude > 0.0 { return value / magnitude; }
    return vec3(0.0);
}

// Raw 0..255 terms, before ambient/directed scales and minimum-light addition.
fn sample_model_grid(position: vec3<f32>) -> EntityLight {
    let coord = (position - model_grid.origin.xyz) * model_grid.inverse.xyz;
    let base = floor(coord);
    let fraction = coord - base;
    let cell = vec3<u32>(clamp(base, vec3(0.0), vec3<f32>(model_grid.bounds.xyz) - 1.0));
    let step = vec3(1u, model_grid.bounds.x, model_grid.bounds.x * model_grid.bounds.y);
    let start = cell.x + cell.y * step.y + cell.z * step.z;
    var light = EntityLight(vec3(0.0), vec3(0.0), vec3(0.0));
    var total = 0.0;
    for (var corner = 0u; corner < 8u; corner += 1u) {
        var index = start;
        var factor = 1.0;
        for (var axis = 0u; axis < 3u; axis += 1u) {
            if (corner & (1u << axis)) != 0u {
                factor *= fraction[axis];
                index += step[axis];
            } else { factor *= 1.0 - fraction[axis]; }
        }
        if index >= model_grid.bounds.w { continue; }
        let sample = model_grid.words[model_grid.offsets.y + index];
        if sample >= model_grid.offsets.x { continue; }
        let offset = sample * 12u;
        if model_grid.words[offset + 3u] == 0u { continue; }
        total += factor;
        light.ambient += factor * grid_vector(offset);
        light.directed += factor * grid_vector(offset + 4u);
        light.direction += factor * grid_vector(offset + 8u);
    }
    if total > 0.0 && total < 0.99 {
        light.ambient *= 1.0 / total;
        light.directed *= 1.0 / total;
    }
    light.direction = grid_unit(light.direction);
    return light;
}
