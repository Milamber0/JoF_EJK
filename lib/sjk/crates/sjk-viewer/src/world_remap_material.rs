//! Compile a replacement without touching the material until its uploads succeed.
use super::*;

/// The fields of a [`Material`] that its shader decides. Stages keep their bind groups
/// and append-only pipeline indices, so a kept look is drawable again as it was.
pub(super) struct Look {
    stages: Vec<StagePass>,
    sort: f32,
    blended: bool,
    view_bounded: bool,
    light_buffered: Option<u8>,
    light_cutout: bool,
    fog_pass: FogPass,
    fog_pipeline: Option<usize>,
    /// Some stage draws into the dynamic glow image (SJK's `r_DynamicGlow`).
    has_glow: bool,
}

impl Look {
    fn swap(&mut self, material: &mut Material) {
        let Self {
            stages,
            sort,
            blended,
            view_bounded,
            light_buffered,
            light_cutout,
            fog_pass,
            fog_pipeline,
            has_glow,
        } = self;
        std::mem::swap(stages, &mut material.stages);
        std::mem::swap(sort, &mut material.sort);
        std::mem::swap(blended, &mut material.blended);
        std::mem::swap(view_bounded, &mut material.view_bounded);
        std::mem::swap(light_buffered, &mut material.light_buffered);
        std::mem::swap(light_cutout, &mut material.light_cutout);
        std::mem::swap(fog_pass, &mut material.fog_pass);
        std::mem::swap(fog_pipeline, &mut material.fog_pipeline);
        std::mem::swap(has_glow, &mut material.has_glow);
    }
}

/// Stage metadata bit: the stage's vertex colour is the map's baked light, which
/// real-time lighting replaces (`world_lighting_mode.wgsl`).
const VERTEX_BAKE: u32 = 1;

/// A vertex-coloured target stage (`rgbGen vertex` or `exactVertex`, codes 2 and 3)
/// drawn on a lightmapped surface shows that surface's baked vertex light: q3map2
/// stores it beside the lightmap, and rd-vanilla draws it there. Only vertex-lit
/// (`LIGHTMAP_BY_VERTEX`) surfaces are marked at load, so without this a
/// `q3map_onlyvertexlighting` target such as `textures/yavin/stonewall2_vertex`
/// kept the static bake under real-time lighting: brighter than its neighbours and
/// blind to live light.
fn baked_vertex_light(lightmap: i32, rgb_generator: f32) -> bool {
    lightmap >= 0 && matches!(rgb_generator as i32, 2 | 3)
}

impl Runtime {
    /// Draw `look` in `source`'s slot; `look` receives what the slot drew before.
    fn show(&mut self, source: usize, look: &mut Look) {
        let material = &mut self.materials[self.source_to_runtime[source]];
        look.swap(material);
        material.camera_ranges = Default::default();
        self.source_order[source] = (
            material.sort,
            material.stages.first().map_or(0, |s| s.pipeline),
        );
    }

    /// Put back the slot's own compiled state as loaded, without recompiling it.
    pub(super) fn restore_remapped_material(&mut self, source: usize) -> bool {
        let Some(mut applied) = self.remaps.sources[source].applied.take() else {
            return false;
        };
        self.show(source, &mut applied.own);
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn replace_remapped_material(
        &mut self,
        source: usize,
        target: &str,
        offset: f32,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        images: &mut ImageCache,
    ) -> Result<bool, Box<dyn Error>> {
        let original = &self.remaps.sources[source];
        // A failed registration must not replace usable geometry with the checkerboard.
        if shaders.get(target).is_none() && shaders.resolve_image(vfs, target)?.is_none() {
            crate::log::progress(format_args!("shader remap ignored: missing {target}"));
            return Ok(false);
        }
        let index = self.source_to_runtime[source];
        if index == usize::MAX {
            return Ok(false);
        }
        // The replacement keeps the slot's own lighting: its lightmap page, vertex
        // light or per-entity light. rd-vanilla's R_RemapShader instead draws the
        // target as it was first registered: with LIGHTMAP_NONE when the world never
        // used it (`$lightmap` becomes the white image, an unscripted texture
        // `lightingDiffuse`), or with another surface's lightmap page, which this
        // surface's lightmap coordinates do not address. Both leave the surface unlit
        // by the map, so SJK departs from it here.
        let key = ViewerMaterial {
            shader: target.to_owned(),
            lightmap: original.key.lightmap,
        };
        let lightmap = &original.lightmap;
        let definition = shaders.get(target);
        let mut compiled = compile_material(
            vfs,
            shaders,
            &key,
            lightmap,
            true,
            Default::default(),
            images,
            false,
        )?;
        let hidden = definition.is_some_and(|d| d.sky.is_some() || !d.has_color_pass());
        if hidden {
            compiled.stages.clear();
        }
        for stage in &mut compiled.stages {
            if baked_vertex_light(key.lightmap, stage.gpu.generators[0]) {
                stage.gpu.wave_functions[3] =
                    (stage.gpu.wave_functions[3] as u32 | VERTEX_BAKE) as f32;
            }
            stage.gpu.emission[2] = offset;
            stage.gpu.emission[3] = if definition.is_some_and(|d| {
                d.stages
                    .iter()
                    .any(|s| s.texture_generator == sjk_shader::TextureGenerator::Environment)
            }) {
                0.7
            } else {
                0.12
            };
        }
        let stages = build_passes(
            device,
            queue,
            &mut self.forge,
            &self.dynamic_light_buffer,
            compiled.stages,
        );
        // Upload failure can still intern earlier stages' pipeline keys.
        // Complete the slots even on error, before a subsequent load uses them.
        while self.entity_pipelines.len() < self.forge.pipeline_keys.len() {
            let key = self.forge.pipeline_keys[self.entity_pipelines.len()];
            self.push_pipelines(device, key);
        }
        let stages = stages?;
        let fog_pass = if hidden {
            FogPass::None
        } else {
            definition.map_or(FogPass::Equal, |d| d.fog_pass())
        };
        let deform = definition.is_some_and(|d| !d.deforms.is_empty());
        let fog_pipeline = self.fog.pipeline_with_geometry(
            device,
            self.forge.format,
            fog_pass,
            definition.map_or(ShaderCull::Front, |d| d.cull),
            u8::from(deform) | super::super::fog_gpu::polygon_offset(definition),
        );
        let blended = compiled.sort > SORT_OPAQUE;
        let mut look = Look {
            view_bounded: !deform,
            blended,
            sort: compiled.sort,
            light_buffered: (!blended
                && !self.materials[index].flare
                && stages.first().is_some_and(|s| s.shadow_caster))
            .then(|| match self.forge.pipeline_keys[stages[0].pipeline].cull {
                Some(wgpu::Face::Front) => 0,
                Some(wgpu::Face::Back) => 1,
                None => 2,
            }),
            light_cutout: !blended && stages.first().is_some_and(|s| s.light_cutout),
            fog_pass,
            fog_pipeline,
            has_glow: glow::has_glow(stages.iter().map(|stage| stage.glow)),
            stages,
        };
        self.show(source, &mut look);
        // Keep the slot's own state from its first replacement; drop later ones.
        let own = match self.remaps.sources[source].applied.take() {
            Some(previous) => previous.own,
            None => look,
        };
        self.remaps.sources[source].applied = Some(Box::new(Applied {
            target: target.to_owned(),
            offset,
            own,
        }));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::baked_vertex_light;

    #[test]
    fn vertex_colour_on_a_lightmapped_slot_is_baked_light() {
        // `rgbGen vertex` and `exactVertex` on a lightmap page.
        assert!(baked_vertex_light(0, 2.));
        assert!(baked_vertex_light(7, 3.));
        // Identity, lightingDiffuse and constants keep their authored colour.
        assert!(!baked_vertex_light(0, 0.));
        assert!(!baked_vertex_light(0, 4.));
        assert!(!baked_vertex_light(0, 6.));
        // Vertex-lit slots are marked at load; entity slots are lit per instance.
        assert!(!baked_vertex_light(
            crate::world_stage::LIGHTMAP_BY_VERTEX,
            2.
        ));
        assert!(!baked_vertex_light(crate::world_stage::LIGHTMAP_NONE, 2.));
    }
}
