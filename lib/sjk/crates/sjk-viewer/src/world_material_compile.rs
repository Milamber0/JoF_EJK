//! Load-time material resolution, shared by map and detached model materials.
use super::*;
#[path = "world_declared_emission.rs"]
mod declared_emission;

/// Resolve a material's stages the way `create_runtime` does for the world.
/// `material_maps` enables the optional maps; detached and late entity materials pass
/// the default (off).
pub(in crate::world_materials) fn compile_material(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    key: &ViewerMaterial,
    lightmap: &wgpu::TextureView,
    collapse: bool,
    material_maps: crate::world_materials::material_maps::Settings,
    image_cache: &mut ImageCache,
    fixed_fixture: bool,
) -> Result<CompiledMaterial, Box<dyn Error>> {
    let sprite_source = crate::scene_flatten::sprites::source(&key.shader);
    let flare_source = key
        .shader
        .strip_prefix(crate::scene_flatten::flares::PREFIX);
    let source_name =
        flare_source.unwrap_or_else(|| sprite_source.map_or(key.shader.as_str(), |(name, _)| name));
    let definition = shaders.get(source_name);
    let declared = definition.is_some_and(|d| d.surface_light > 0.);
    let (stages, sort, cull) =
        if let Some(stage) = crate::scene_flatten::sprites::stage(shaders, &key.shader) {
            (
                vec![stage],
                definition.map_or(9.0, |d| d.resolved_sort()),
                ShaderCull::TwoSided,
            )
        } else {
            material_stages(definition, key.lightmap)
        };
    // Entities are lit per instance: only the static world has fixtures without a light.
    let self_lit = (fixed_fixture || key.lightmap != crate::world_stage::LIGHTMAP_NONE)
        && super::emission::self_lit(definition);
    // Fixed additive surfaces (including crystal glows) are sources too. View-
    // dependent environment maps remain excluded by the source-stage accumulator.
    let infer_emission = sprite_source.is_none()
        && definition.is_some_and(|d| {
            d.sky.is_none()
                && d.deforms.is_empty()
                && (sort == SORT_OPAQUE
                    || (self_lit
                        && d.stages.last().is_some_and(|s| {
                            s.blend == sjk_shader::StageBlend::Replace && s.alpha_function.is_none()
                        }))
                    || d.stages
                        .iter()
                        .all(|s| s.blend == sjk_shader::StageBlend::Add))
        });
    let hardware_stages = if collapse {
        collapse_multitexture(&stages)
    } else {
        stages
            .iter()
            .cloned()
            .map(|primary| CompiledStage {
                glow: primary.glow,
                output_blend: primary.blend.clone(),
                output_depth_write: primary.depth_write,
                output_depth_function: primary.depth_function,
                primary,
                secondary: None,
                combine: crate::world_stage::CollapseOperator::None,
            })
            .collect()
    };
    let mut resolved = 0;
    let mut compiled = Vec::new();
    let mut emission = [0.; 3];
    let mut emission_texture = super::emission::Texture::default();
    let mut fallback_emission = super::emission::Texture::default();
    let mut debug_end = false;
    // A shader that shows its own light takes no emission map: never the light twice.
    let material_maps = crate::world_materials::material_maps::Settings {
        emission: material_maps.emission
            && !definition.is_some_and(crate::world_materials::material_maps::shows_light),
        ..material_maps
    };
    // The first emission map and the diffuse stage it belongs to, for the light it gives.
    let mut emission_map = None;
    for (index, stage) in hardware_stages.iter().take(MAX_SHADER_STAGES).enumerate() {
        if index > 0
            && stage.primary.texture_generator != sjk_shader::TextureGenerator::Lightmap
            && !stage
                .secondary
                .as_ref()
                .is_some_and(|s| s.texture_generator == sjk_shader::TextureGenerator::Lightmap)
        {
            debug_end = true;
        }
        let (primary_pixels, primary_resolved, primary_key) =
            load_stage_images(vfs, shaders, &stage.primary, source_name, image_cache)?;
        let (secondary_pixels, secondary_resolved, secondary_clamp, secondary_key) =
            if let Some(secondary) = &stage.secondary {
                let (pixels, resolved, key) =
                    load_stage_images(vfs, shaders, secondary, source_name, image_cache)?;
                (Some(pixels), resolved, secondary.clamp, Some(key))
            } else {
                (None, false, false, None)
            };
        resolved += usize::from(primary_resolved) + usize::from(secondary_resolved);
        if infer_emission {
            let secondary = stage
                .secondary
                .as_ref()
                .zip(secondary_pixels.as_deref())
                .zip(secondary_key.as_deref())
                .map(|((source, images), key)| (source, images, key));
            let mut sources = [
                Some((
                    &stage.primary,
                    primary_pixels.as_slice(),
                    primary_key.as_str(),
                )),
                secondary,
            ];
            // Collapsing puts a leading lightmap in the second sampler. Composition
            // still follows source order, especially when an opaque stage covers glow.
            if secondary.is_some_and(|(s, _, _)| {
                s.texture_generator == sjk_shader::TextureGenerator::Lightmap
            }) {
                sources.swap(0, 1);
            }
            for (source, images, key) in sources.into_iter().flatten() {
                super::emission::accumulate(
                    &mut emission,
                    source,
                    images,
                    !key.contains("$missing:"),
                    self_lit,
                    !declared,
                );
            }
        }
        if declared || infer_emission {
            // Preserve original source order even after lightmap-stage collapsing.
            let secondary = stage.secondary.as_ref().zip(secondary_pixels.as_deref());
            let mut sources = [Some((&stage.primary, primary_pixels.as_slice())), secondary];
            if secondary
                .is_some_and(|(s, _)| s.texture_generator == sjk_shader::TextureGenerator::Lightmap)
            {
                sources.swap(0, 1);
            }
            for (source, images) in sources.into_iter().flatten() {
                let resolved = if std::ptr::eq(source, &stage.primary) {
                    primary_resolved
                } else {
                    secondary_resolved
                };
                let resolved = resolved
                    || source.images.iter().any(|s| {
                        s.eq_ignore_ascii_case("$whiteimage") || s.eq_ignore_ascii_case("*white")
                    });
                emission_texture.observe(source, images, resolved, false, self_lit, !declared);
                if declared {
                    fallback_emission.observe(source, images, resolved, true, false, false);
                }
            }
        }
        // Material maps: ordinary lightmapped world paint only, never generated geometry.
        let maps = if sprite_source.is_none()
            && flare_source.is_none()
            && definition.is_none_or(|d| d.deforms.is_empty() && d.sky.is_none())
        {
            crate::world_materials::material_maps::resolve(
                vfs,
                shaders,
                material_maps,
                stage,
                key.lightmap,
                cull == ShaderCull::TwoSided,
                source_name,
                image_cache,
            )?
        } else {
            None
        };
        if emission_map.is_none()
            && let Some(image) = maps.as_ref().and_then(|maps| maps.emission.as_ref())
            && let Some(bundle) =
                crate::world_materials::material_maps::diffuse_bundle(stage, key.lightmap)
        {
            let diffuse = match bundle {
                crate::world_materials::material_maps::Bundle::Primary => &stage.primary,
                crate::world_materials::material_maps::Bundle::Secondary => {
                    stage.secondary.as_ref().expect("collapsed stage")
                }
            };
            emission_map = Some((diffuse.clone(), Arc::clone(&image.pixels)));
        }
        compiled.push(PendingStage {
            allow_ssao: super::ssao::authored_diffuse(definition),
            glow: stage.glow,
            gpu: {
                let mut gpu = compile_hardware_stage(stage);
                super::visible_emission::configure(
                    &mut gpu,
                    stage,
                    definition,
                    sort == SORT_OPAQUE && sprite_source.is_none() && flare_source.is_none(),
                );
                // ComputeFinalVertexColor and RB_IterateStagesGeneric, rd-vanilla.
                gpu.wave_functions[3] = (u32::from(key.lightmap == -3)
                    | (u32::from(debug_end) << 1)
                    | if sort == SORT_OPAQUE
                        && crate::world_stage::relighting::diffuse_cover(
                            &stage.primary,
                            &stages,
                            key.lightmap,
                        )
                    {
                        crate::world_stage::relighting::DIFFUSE_COVER
                    } else {
                        0
                    }) as f32;
                if let Some(definition) = definition {
                    crate::world_stage::compile_deforms(&mut gpu, &definition.deforms);
                }
                crate::scene_flatten::sprites::compile(&mut gpu, &stage.primary);
                if flare_source.is_some() {
                    gpu.wave_functions[2] = definition.and_then(|d| d.portal_range).unwrap_or(30.0);
                }
                gpu
            },
            primary_pixels,
            primary_key,
            primary_clamp: stage.primary.clamp,
            secondary_pixels,
            secondary_key,
            secondary_clamp,
            lightmap: lightmap.clone(),
            key: {
                let mut key = hardware_pipeline_key(stage, cull);
                if definition.is_some_and(|d| !d.deforms.is_empty()) {
                    key.geometry |= 1;
                }

                if definition.is_some_and(|d| d.polygon_offset) {
                    key.geometry |= crate::world_stage::POLYGON_OFFSET;
                }
                if maps.is_some() {
                    key.geometry |= crate::world_materials::material_maps::PIPELINE_BIT;
                }
                key
            },
            maps,
        });
    }
    // rd-vanilla projects each dlight after the material stages. Our
    // single-pass approximation runs exactly once on the final opaque
    // hardware stage, never once per source stage.
    if sort == SORT_OPAQUE
        && let Some(last) = compiled.last_mut()
    {
        last.gpu.secondary_control[3] = 1.0;
    }
    if let Some(definition) = definition.filter(|d| d.surface_light > 0.) {
        (emission_texture, emission) = declared_emission::resolve(
            vfs,
            shaders,
            definition,
            emission_texture,
            fallback_emission,
            image_cache,
        )?;
    }
    // An emission map lights the room only where nothing else already does: a declared
    // surface light or an inferred fixture keeps its own (`r_emissiveLights`).
    let gain = crate::world_materials::material_maps::lights::gain();
    let mut mapped_emission = false;
    if let Some((stage, image)) = emission_map
        && crate::world_materials::material_maps::lights::applies(emission, gain)
    {
        let texture = super::emission::Texture::from_map(
            &stage,
            image,
            crate::world_materials::material_maps::lights::RADIANCE * gain,
        );
        if texture.mean().max_element() > 0. {
            emission = texture.mean().to_array();
            emission_texture = texture;
            mapped_emission = true;
        }
    }
    Ok(CompiledMaterial {
        sort,
        emission,
        emission_texture,
        mapped_emission,
        stages: compiled,
        resolved,
    })
}
