override visible_emission: bool = false;

// One material-composition hook. Zero keeps the previous arithmetic and blend ordering.
fn apply_lighting_mode(input: VertexOutput, primary: vec4<f32>, secondary: vec4<f32>) -> vec4<f32> {
    let mode = point_lights.metadata.z;
    let fullbright = (mode & 1u) != 0u;
    let lightmap_only = (mode & 2u) != 0u;
    let flags = u32(stage.wave_functions.w);
    // Stock stops the stage loop at the first later non-lightmap stage.
    if lightmap_only && (flags & 2u) != 0u { discard; }
    var color = generated_color(input);
    // Apply texture-bundle radiance before the expensive lighting paths. Straight
    // arithmetic avoids another control-flow split in the shared material shader.
    var gains = vec2(1.0);
    if visible_emission {
        gains = select(vec2(1.0), max(stage.emission.xy, vec2(1.0)),
            realtime_active() && (mode & 3u) == 0u);
    }
    var base = vec4(primary.rgb*gains.x, primary.a);
    var extra = vec4(secondary.rgb*gains.y, secondary.a);
    let base_lightmap = i32(stage.animation.w) == 1;
    let extra_lightmap = i32(stage.secondary_animation.w) == 1;
    if fullbright {
        // R_BindAnimatedImage binds tr.whiteImage: actual white, never neutral gray.
        if base_lightmap { base = vec4(1.0); }
        if extra_lightmap { extra = vec4(1.0); }
        let rgb = i32(stage.generators.x);
        if (flags & 1u) != 0u && (rgb == 2 || rgb == 3) && input.entity_control.x <= 0.5 {
            color = vec4(vec3(1.0), color.a);
        }
        // Stock's fullbright grid override saturates ordinary model diffuse to white.
        // Disintegration colours (entity_control.x = 3) replace every generator
        // (`killGen`), so they are kept, as `generated_color` returns them.
        if input.light_ambient.a != 0.0 && (rgb == 4 || rgb == 9) && input.entity_control.x <= 2.5 {
            color = vec4(select(vec3(1.0), input.entity_color.rgb,
                rgb == 9 || input.entity_control.x > 0.5), color.a);
        }
    }
    color = vec4(model_sun_color(input,color.rgb),color.a);
    let emitted = select(vec3(0.0), emitted_light(input), !fullbright);
    // Real-time lighting replaces baked illumination; authored textures retain their material role.
    if !fullbright && base_lightmap { base = realtime_lightmap(input, base); base = vec4(base.rgb+emitted,base.a); }
    if !fullbright && extra_lightmap { extra = realtime_lightmap(input, extra); extra = vec4(extra.rgb+emitted,extra.a); }
    {
        let rgb = i32(stage.generators.x);
        // Some legacy stacks restore the base texture over additive effects. That
        // diffuse cover must receive the same light, or its opaque paint erases shadows.
        let vertex_bake = (flags & 1u) != 0u && (rgb == 2 || rgb == 3);
        let diffuse_cover = (flags & 4u) != 0u;
        if realtime_active() && !fullbright && (vertex_bake || diffuse_cover) &&
            input.entity_control.x <= 0.5 && input.light_ambient.a == 0.0 {
            color = vec4(realtime_lightmap(input,vec4(1.0)).rgb+emitted,color.a);
        }
    }
    if stage.secondary_control.z > 0.5 {
        // DrawMultitextured's GL_REPLACE replaces RGB and alpha, including primary color.
        if lightmap_only { return extra; }
        if i32(stage.secondary_control.y) == 1 { base *= extra; }
        else if i32(stage.secondary_control.y) == 2 { base += extra; }
    }
    return base * color;
}
