//! The material program: the ordinary stage program of the lighting mode with
//! hooks for material maps.
//!
//! Like the stage table's program, it is the unchanged stage source with a few
//! exact replacements, each asserted to match once, so the ordinary programs
//! stay byte for byte what they were. The hooks:
//! - the vertex stage reads the vertex's frame (`material_map_frames`) and passes
//!   the tangent and light direction on two more varyings;
//! - the fragment stage first resolves the mapped normal, the specular sample and
//!   the parallax offset (`material_map_prepare`), and samples the diffuse image
//!   at the offset texture coordinate, with the coordinate's own derivatives;
//! - the lightmap (or the real-time light buffer) response is replaced by
//!   `material_map_lightmap`, and on vertex-lit paint the vertex colours' response
//!   by `material_map_vertex_light`; point lights use the mapped normal, and highlights
//!   and the emission map's colour are added after the albedo product and
//!   dynamic-light modulation (`material_map_finish`, which also draws the
//!   `r_materialMapsDebug` views).
//!
//! [`glow_source`] is the same program for the dynamic glow pass: a stage that
//! glows only for its emission map writes the emission there, not its lit colour.

use super::super::{STAGE_SHADER, world_sun_shader};

/// The material program's WGSL for the authored (`realtime == false`) or the
/// real-time (light buffer) lighting mode.
pub(in crate::world_materials) fn source(realtime: bool) -> String {
    let base = if realtime {
        world_sun_shader()
    } else {
        STAGE_SHADER
    };
    // Checkouts may carry CRLF line ends (`core.autocrlf`); the patterns use LF.
    let mut source = base.replace("\r\n", "\n");
    for (from, to) in [
        (
            "    @location(11) @interpolate(flat) animation_index: i32,\n};",
            "    @location(11) @interpolate(flat) animation_index: i32,\n    \
             @location(12) material_tangent: vec4<f32>,\n    \
             @location(13) material_light: vec4<f32>,\n};",
        ),
        (
            "    if (instance.view_flags & 8u) != 0u { output.animation_index",
            "    let material_map_vertex = material_map_frame(index, instance.rotation);\n    \
             output.material_tangent = material_map_vertex.tangent;\n    \
             output.material_light = vec4(material_map_vertex.light, material_map_vertex.probe);\n    \
             if (instance.view_flags & 8u) != 0u { output.animation_index",
        ),
        (
            "    fragment_point_mask = finite_point_mask(input.world_position);\n",
            "    fragment_point_mask = finite_point_mask(input.world_position);\n    \
             material_map_prepare(input);\n",
        ),
        // The diffuse image at the parallax offset, with the coordinates' own derivatives
        // (as the maps are read in `material_map_prepare`): the offset's jumps at relief
        // edges must not choose the mip level.
        (
            "textureSample(stage_images, stage_sampler, uv, frame)",
            "textureSampleGrad(stage_images, stage_sampler, uv + material_map_surface.uv_offset, \
             frame, dpdx(uv), dpdy(uv))",
        ),
        (
            "textureSample(secondary_images, secondary_sampler,\n                \
             input.secondary_uv, secondary_frame)",
            "textureSampleGrad(secondary_images, secondary_sampler,\n                \
             input.secondary_uv + material_map_surface.uv_offset, secondary_frame,\n                \
             dpdx(input.secondary_uv), dpdy(input.secondary_uv))",
        ),
        (
            "    var output = apply_lighting_mode(input, texel, secondary_texel);",
            "    material_map_remember_albedo(texel, secondary_texel);\n    \
             var output = apply_lighting_mode(input, texel, secondary_texel);",
        ),
        (
            "        output = vec4(output.rgb * (vec3(1.0) + dynamic_light_modulation(input)), output.a);\n    }\n    return output;\n}",
            "        output = vec4(output.rgb * (vec3(1.0) + dynamic_light_modulation(input)), output.a);\n        \
             material_map_point_highlights(input);\n    }\n    \
             return material_map_finish(output);\n}",
        ),
        // Point lights shine on the mapped normal.
        (
            "    var result = vec3(0.0);\n    let normal = normalize(input.world_normal);\n    var remaining = fragment_point_mask & point_lights.metadata.w;",
            "    var result = vec3(0.0);\n    let normal = material_map_surface.normal;\n    var remaining = fragment_point_mask & point_lights.metadata.w;",
        ),
        (
            "    let normal = normalize(input.world_normal);\n    var remaining = fragment_point_mask;\n",
            "    let normal = material_map_surface.normal;\n    var remaining = fragment_point_mask;\n",
        ),
        (
            "base = realtime_lightmap(input, base);",
            "base = material_map_lightmap(input, base);",
        ),
        (
            "extra = realtime_lightmap(input, extra);",
            "extra = material_map_lightmap(input, extra);",
        ),
        // Vertex-lit paint: the light buffer in real-time lighting, the vertex colours
        // as baked light otherwise (`material_map_vertex_light`).
        (
            "color = vec4(realtime_lightmap(input,vec4(1.0)).rgb+emitted,color.a);",
            "color = vec4(material_map_lightmap(input,vec4(1.0)).rgb+emitted,color.a);",
        ),
        (
            "    color = vec4(model_sun_color(input,color.rgb),color.a);\n",
            "    color = vec4(model_sun_color(input,color.rgb),color.a);\n    \
             color = material_map_vertex_light(input, color, flags, fullbright);\n",
        ),
    ] {
        assert_eq!(
            source.matches(from).count(),
            1,
            "stage program changed: {from}"
        );
        source = source.replace(from, to);
    }
    source.push_str(include_str!("material_maps.wgsl"));
    source.push_str(include_str!("material_maps_reflection.wgsl"));
    source.push_str(if realtime {
        include_str!("material_maps_realtime.wgsl")
    } else {
        include_str!("material_maps_authored.wgsl")
    });
    source
}

/// [`source`] for the dynamic glow pass (`world_glow.rs`): the final colour goes
/// through `material_map_glow`, which keeps it for stages with an authored `glow` and
/// replaces it by the emission for stages drawn there only for their emission map.
pub(in crate::world_materials) fn glow_source(realtime: bool) -> String {
    let source = source(realtime);
    let from = "return material_map_finish(output);";
    assert_eq!(source.matches(from).count(), 1, "material program changed");
    source.replace(
        from,
        "return material_map_glow(material_map_finish(output));",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga;

    fn validate(source: &str) -> naga::valid::ModuleInfo {
        let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|error| {
            panic!("{}", error.emit_to_string(source));
        });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)))
    }

    #[test]
    fn material_programs_validate_in_both_lighting_modes() {
        for realtime in [false, true] {
            for source in [source(realtime), glow_source(realtime)] {
                validate(&source);
                for entry in ["entity_vertex_main", "fragment_main"] {
                    assert!(source.contains(&format!("fn {entry}(")), "{entry}");
                }
            }
        }
    }

    #[test]
    fn only_the_glow_program_writes_emission_alone() {
        for realtime in [false, true] {
            let scene = source(realtime);
            let glow = glow_source(realtime);
            assert!(!scene.contains("return material_map_glow("));
            assert!(glow.contains("return material_map_glow(material_map_finish(output));"));
            // The glow variant tests the stage's own flag, which `resolve` sets for
            // stages without an authored glow.
            assert!(glow.contains(&format!(
                "(material_map_flags() & {}u) != 0u",
                super::super::FLAG_EMISSION_GLOW
            )));
            // The emission map is sampled under its own flag.
            assert!(scene.contains(&format!("(flags & {}u) != 0u", super::super::FLAG_EMISSION)));
        }
    }

    #[test]
    fn debug_views_read_the_lighting_mode_bits_the_cvar_writes() {
        let shift = format!(
            "(point_lights.metadata.z >> {}u) & 7u",
            super::super::DEBUG_SHIFT
        );
        for realtime in [false, true] {
            let source = source(realtime);
            // The final view and the reflection's opt-out read the same bits.
            assert_eq!(source.matches(&shift).count(), 2, "{shift}");
            // The views replace the final colour after every other hook ran.
            assert!(source.contains("return material_map_finish(output);"));
        }
    }

    #[test]
    fn offset_reads_take_the_coordinates_own_derivatives() {
        for realtime in [false, true] {
            let source = source(realtime);
            // The diffuse image at the parallax offset, primary and secondary bundle.
            assert!(source.contains(
                "textureSampleGrad(stage_images, stage_sampler, \
                 uv + material_map_surface.uv_offset, frame, dpdx(uv), dpdy(uv))"
            ));
            assert!(source.contains(
                "input.secondary_uv + material_map_surface.uv_offset, secondary_frame,\n                \
                 dpdx(input.secondary_uv), dpdy(input.secondary_uv))"
            ));
            assert!(!source.contains("uv_offset, frame)"));
        }
        // No map is read at the offset with implicit derivatives.
        let maps = include_str!("material_maps.wgsl");
        assert!(!maps.contains("textureSample(material_map_"));
        assert_eq!(
            maps.matches("uv + offset,\n            uv_dx, uv_dy)")
                .count(),
            3
        );
    }

    #[test]
    fn ordinary_programs_still_validate_unchanged() {
        validate(STAGE_SHADER);
        validate(world_sun_shader());
        assert!(!STAGE_SHADER.contains("material_map"));
        assert!(!world_sun_shader().contains("material_map"));
    }
}
