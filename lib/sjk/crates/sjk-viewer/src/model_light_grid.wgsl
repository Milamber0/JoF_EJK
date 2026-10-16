// R_SetupEntityLighting tr_light.cpp:343-412, now evaluated at the fragment position.
fn spatial_model_light(position: vec3<f32>) -> EntityLight {
    var light = sample_model_grid(position);
    return finish_model_light(position,light);
}

// Keep the grid's indirect fill and existing point-light law; replace only baked direction.
fn model_light_without_baked_direction(position: vec3<f32>) -> EntityLight {
    let grid = sample_model_grid(position);
    return finish_model_light(position,EntityLight(grid.ambient,vec3(0.0),vec3(0.0)));
}

fn finish_model_light(position: vec3<f32>, source: EntityLight) -> EntityLight {
    var light = source;
    light.ambient = light.ambient * 0.6 + vec3(32.0);
    var direction = light.direction * length(light.directed);
    for (var index = 0u; index < min(point_lights.metadata.x, 32u); index += 1u) {
        let source = point_lights.lights[index];
        let toward = source.origin_radius.xyz - position;
        let distance = max(length(toward), 16.0);
        // Within the light's reach, the static world between it and this pixel stops it
        // (`point_light_visibility`, with no surface plane: a model is not in the tile).
        let strength = 16.0 * source.origin_radius.w * source.origin_radius.w /
            (distance * distance) * point_light_visibility(index, position, vec3(0.0));
        light.directed += strength * source.color.rgb;
        direction += strength * grid_unit(toward);
    }
    light.direction = grid_unit(direction);
    if dot(direction, direction) == 0.0 {
        light.direction = vec3(0.42857, 0.28571, 0.85714);
    }
    light.ambient = min(light.ambient, vec3(255.0)) / 255.0;
    light.directed /= 255.0;
    return light;
}
