// Raven's triangle-cell sampler and QuickSprite geometry, evaluated from
// immutable seeds. No frame-local CPU tessellation or random-number state.
struct GeometryEnvironment {
    control: vec4<f32>,
    grass_speed: vec4<f32>,
    blow_force: vec4<f32>,
    point: vec4<f32>,
};
@group(2) @binding(2) var<uniform> environment: GeometryEnvironment;
fn sprite_random(index: u32) -> f32 { return surface_tables.sprite_random[index & 255u]; }

fn material_vertex(input_source: VertexInput, index: u32, origin: vec3<f32>, rotation: vec4<f32>,
    scale: vec3<f32>) -> VertexInput {
    let source = skinned_vertex(input_source, index);
    if !geometry_sprites || stage.sprites[0].x == 0.0 {
        return deform_vertex(source, index, rotation, scale);
    }
    var result = source;
    if index >= arrayLength(&geometry_quads) || geometry_quads[index].valid == 0u { return result; }
    let reference = geometry_quads[index];
    var q: array<VertexInput, 4>;
    var slot = 0u;
    for (var i = 0u; i < 4u; i++) {
        q[i] = geometry_vertex(reference.vertices[i]);
        if reference.vertices[i] == index { slot = i; }
    }
    for (var i = 0u; i < 3u; i++) {
        if stage.deform_a[i].x == 0.0 { break; }
        for (var j = 0u; j < 3u; j++) { q[j] = deform_point(q[j], i); }
    }
    let kind = i32(stage.sprites[0].x);
    let vertical = kind == 1 || kind == 4;
    var step = q[3].normal.x;
    var cell = q[3].lightmap_coordinates;
    var weather_visible = true;
    if stage.sprites[4].z != 0.0 && environment.control.y > 0.0 && environment.control.y < 1.0 {
        step /= environment.control.y;
        let rows = ceil(1.0 / step);
        let ordinal = q[3].color.x;
        weather_visible = ordinal<rows * (rows + 1.0) * 0.5;
        let root = max((2.0 * rows + 1.0) * (2.0 * rows + 1.0) - 8.0 * ordinal, 0.0);
        let row = floor((2.0 * rows + 1.0 - sqrt(root)) * 0.5);
        let column = ordinal - row * (2.0 * rows - row + 1.0) * 0.5;
        cell = vec2(row, column) * step;
    }
    let start_seed = u32(q[3].normal.y);
    let interval = u32(q[3].normal.z);
    var seed = start_seed;
    var phase = 0.0;
    if kind == 3 {
        let life = ((camera.shader_time - stage.emission.z) * 1000.0 + 10000.0 * sprite_random(seed)) /
            stage.sprites[3].y;
        phase = fract(life);
        seed = u32(f32(seed) + life);
    }
    let fa = cell.x + sprite_random(seed) * step;
    seed += select(interval, 1u, kind == 3);
    let fb = cell.y + sprite_random(seed) * step;
    seed += select(interval, 1u, kind == 3);
    let fc = 1.0 - fa - fb;
    let weights = vec3(fa, fb, fc);
    let inv = vec4(-rotation.xyz, rotation.w);
    let eye = rotate_vector(inv, camera.camera_position - origin) / scale;
    let vp = camera.view_projection;
    let horizontal_scale = max(length(vec3(vp[0].x, vp[1].x, vp[2].x)), 0.41421356);
    let fadedist = stage.sprites[1].x * horizontal_scale * environment.control.x;
    let cutdist = stage.sprites[1].y * horizontal_scale * environment.control.x;
    let fade_range = min(250.0 / (cutdist - fadedist), 1.0);
    var vertex_fade = vec3(0.0);
    for (var i = 0u; i < 3u; i++) {
        let delta = eye - q[i].position;
        vertex_fade[i] = 1.0 - (dot(delta, delta) - fadedist * fadedist) / (cutdist * cutdist -
            fadedist * fadedist);
    }
    let alpha_position = dot(vertex_fade, weights);
    let fade_start = fade_range + (1.0 - fade_range) * sprite_random(seed);
    var alpha = clamp(1.0 - (fade_start - alpha_position) / fade_range, 0.0, 1.0);
    if fa > 1.0 || fb > 1.0 - fa || all(vertex_fade <= vec3(0.0)) { alpha = 0.0; }
    if !weather_visible || environment.control.z == 0.0 || (stage.sprites[4].z != 0.0 &&
        environment.control.y < 0.01) { alpha = 0.0; }
    if kind == 3 { seed = start_seed + interval; }
    else { seed += select(interval * 2u, interval, vertical); }
    var width = stage.sprites[0].y * (1.0 + stage.sprites[2].x * sprite_random(seed));
    var height = stage.sprites[0].z * (1.0 + stage.sprites[2].y * sprite_random(seed));
    seed += 1u;
    if kind == 3 {
        width *= 1.0 + phase * stage.sprites[3].z;
        height *= 1.0 + phase * stage.sprites[3].w;
        var fade_phase = phase;
        if stage.sprites[4].y < 0.05 && stage.sprites[0].y >= 0.1 && stage.sprites[0].z >= 0.1 {
            fade_phase = abs(phase - 0.5) * 2.0;
        }
        alpha *= mix(stage.sprites[4].x, stage.sprites[4].y, fade_phase);
    }
    if sprite_random(seed) > 0.5 { width = -width; }
    seed += 1u;
    width *= 1.0 + stage.sprites[1].z * max(1.0 - alpha_position, 0.0);
    var loc = q[0].position * fa + q[1].position * fb + q[2].position * fc;
    if kind == 3 && stage.sprites[1].w > 0.0 && environment.grass_speed.w > 0.001 {
        loc += phase * stage.sprites[1].w * environment.blow_force.xyz;
    }
    let view_right = rotate_vector(inv, -normalize(vec3(vp[0].x, vp[1].x, vp[2].x))) / scale;
    let view_up = rotate_vector(inv, normalize(vec3(vp[0].y, vp[1].y, vp[2].y))) / scale;
    let forward = cross(view_right, vec3(0.0, 0.0, 1.0));
    var top = loc + height * view_up;
    var right = view_right * width * 0.5;
    var corner_skew = 0.2;
    if vertical {
        let variation = array<vec2<f32>, 4>(vec2(0.985, 0.174), vec2(0.866, -0.5), vec2(0.866,
            0.5), vec2(0.985, -0.174));
        let axes = variation[u32(q[3].color.x) & 3u];
        right = (view_right * axes.x + forward * axes.y) * width * 0.5;
        if kind == 4 {
            right = vec3(sin(loc.x * 0.01745329252) * width,
                cos(loc.x * 0.01745329252) * height, 0.0);
        }
        let skew = height * stage.sprites[2].z * (2.0 * vec2(sprite_random(seed),
            sprite_random(seed + 1u)) - 1.0);
        let angle = (loc.x + loc.y) * 0.02 + (camera.shader_time - stage.emission.z) * 1.5;
        let sway = height * stage.sprites[3].x * 0.075;
        top = loc + vec3(skew + vec2(cos(angle), sin(angle)) * sway,
            select(height, -height, stage.sprites[2].w != 0.0));
        let wind = stage.sprites[1].w;
        if wind > 0.0 && environment.grass_speed.w > 0.001 {
            top += height * wind * environment.grass_speed.xyz;
            top.z += sin(angle * 2.5) * height * wind * 0.075 * min(environment.grass_speed.w /
                100.0, 0.4);
        }
        if wind > 0.0 && environment.blow_force.w >= 0.01 {
            var force = 0.0;
            var direction = vec2(0.0);
            for (var i = 0u; i < 3u; i++) {
                let delta = q[i].position.xy - environment.point.xy;
                let square = dot(delta, delta);
                if square < 750.0 * 750.0 {
                    var strength = environment.blow_force.w * wind;
                    var unit = vec2(0.0);
                    if square >= 1.0 {
                        var inverse = bitcast<f32>(0x5f3759dfu - (bitcast<u32>(square)>>1u));
                        inverse *= 1.5 - square * 0.5 * inverse * inverse;
                        unit = delta * inverse; strength *= 1.0 - 1.0 / (inverse * 750.0);
                    }
                    force += strength * weights[i]; direction += unit * weights[i];
                }
            }
            if force > 0.0 {
                // RB_VerticalSurfaceSpriteWindPoint uses 0.15, not the
                // ordinary vertical sprite's 0.2 corner offset.
                corner_skew = 0.15;
                force = min(force, 1.0);
                let point_sway = select(height * stage.sprites[3].x * 0.1 * (1.0 + force), 0.0,
                    environment.grass_speed.w >= 80.0);
                top = loc + vec3(skew + vec2(cos(angle), sin(angle)) * point_sway, select(height,
                    -height, stage.sprites[2].w != 0.0));
                if environment.grass_speed.w > 0.001 {
                    top += height * wind * environment.grass_speed.xyz;
                }
                top += vec3(height * direction * force, -height * force * (0.75 + 0.15 *
                    sin(((camera.shader_time - stage.emission.z) * 1000.0 + 500.0 * force) * 0.01)));
            }
        }
    }
    var points = array<vec3<f32>, 4>(loc + right, top + right, top - right, loc - right);
    if vertical { points[2] += forward * width * corner_skew; }
    else if stage.sprites[2].w != 0.0 {
        let w = width * 0.5;
        points = array<vec3<f32>, 4>(loc + vec3(w, -w, 1.0), loc + vec3(w, w, 1.0), loc + vec3(-w,
            w, 1.0), loc + vec3(-w, -w, 1.0));
    }
    var light = dot(vec3(q[0].color.b, q[1].color.b, q[2].color.b), weights);
    if stage.sprites[4].w != 0.0 { light = (128.0 / 255.0 + light * 0.5) * alpha; alpha = 1.0; }
    result.position = points[slot];
    result.normal = vec3(0.0, 0.0, 1.0);
    result.color = floor(clamp(vec4(vec3(light), alpha), vec4(0.0), vec4(1.0)) * 255.0) / 255.0;
    result.texture_coordinates = array<vec2<f32>, 4>(vec2(1.0, 1.0), vec2(1.0, 0.0), vec2(0.0,
        0.0), vec2(0.0, 1.0))[slot];
    result.lightmap_coordinates = weights.xy;
    return result;
}
