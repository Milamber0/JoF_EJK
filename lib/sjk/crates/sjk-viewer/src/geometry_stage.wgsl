// Shared stage ABI and waveform evaluator for colour and geometry-aware fog.
override geometry_deforms: bool = false;
override geometry_sprites: bool = false;
struct Stage {
    animation: vec4<f32>, generators: vec4<f32>, rgb_wave: vec4<f32>, alpha_wave: vec4<f32>,
    constant_color: vec4<f32>, wave_functions: vec4<f32>,
    tcmod_a: array<vec4<f32>, 4>, tcmod_b: array<vec4<f32>, 4>,
    secondary_animation: vec4<f32>, secondary_control: vec4<f32>,
    secondary_tcmod_a: array<vec4<f32>, 4>, secondary_tcmod_b: array<vec4<f32>, 4>,
    deform_a: array<vec4<f32>, 3>, deform_b: array<vec4<f32>, 3>, deform_c: array<vec4<f32>, 3>,
    sprites: array<vec4<f32>, 5>,
    emission: vec4<f32>,
};

// TableForFunc/EvalWaveForm; rd-vanilla's sin table has a 1023 denominator.
fn table_value(function: i32, value: f32) -> f32 {
    let index = i32(value * 1024.0) & 1023;
    if function == 1 { return select(-1.0, 1.0, index < 512); }
    if function == 2 {
        let quarter = index / 256;
        let fraction = f32(index & 255) / 256.0;
        if quarter == 0 { return fraction; }
        if quarter == 1 { return 1.0 - fraction; }
        if quarter == 2 { return -fraction; }
        return -1.0 + fraction;
    }
    if function == 3 { return f32(index) / 1024.0; }
    if function == 4 { return 1.0 - f32(index) / 1024.0; }
    return sin(f32(index) * 6.28318530718 / 1023.0);
}

fn wave(parameters: vec4<f32>, function: i32) -> f32 {
    if function == 5 {
        return parameters.x + legacy_noise(vec4(0.0, 0.0, 0.0,
            ((camera.shader_time - stage.emission.z) + parameters.z) * parameters.w)) * parameters.y;
    }
    if function == 6 {
        let value = 1.0 + surface_tables.noise_values[surface_tables.noise_perm[
            i32((camera.shader_time - stage.emission.z) * 1000.0 + parameters.z) & 255]];
        return parameters.x + select(0.0, parameters.y, value <= parameters.w);
    }
    return parameters.x + table_value(function, parameters.z + (camera.shader_time - stage.emission.z) * parameters.w) *
        parameters.y;
}
