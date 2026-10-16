// R_NoiseGet4f: nested permutation lookup and linear 4D interpolation.
fn noise_value(p: vec4<i32>) -> f32 {
    let t = surface_tables.noise_perm[p.w & 255];
    let z = surface_tables.noise_perm[(p.z + t) & 255];
    let y = surface_tables.noise_perm[(p.y + z) & 255];
    return surface_tables.noise_values[surface_tables.noise_perm[(p.x + y) & 255]];
}

fn legacy_noise(p: vec4<f32>) -> f32 {
    let whole = vec4<i32>(floor(p));
    let fraction = p - vec4<f32>(whole);
    var temporal: array<f32, 2>;
    for (var t = 0; t < 2; t++) {
        var depth: array<f32, 2>;
        for (var z = 0; z < 2; z++) {
            let a = noise_value(whole + vec4(0, 0, z, t));
            let b = noise_value(whole + vec4(1, 0, z, t));
            let c = noise_value(whole + vec4(0, 1, z, t));
            let d = noise_value(whole + vec4(1, 1, z, t));
            let front = a * (1.0 - fraction.x) + b * fraction.x;
            let back = c * (1.0 - fraction.x) + d * fraction.x;
            depth[z] = front * (1.0 - fraction.y) + back * fraction.y;
        }
        temporal[t] = depth[0] * (1.0 - fraction.z) + depth[1] * fraction.z;
    }
    return temporal[0] * (1.0 - fraction.w) + temporal[1] * fraction.w;
}
