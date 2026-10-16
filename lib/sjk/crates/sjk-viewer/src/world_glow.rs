//! Dynamic glow: the glowing stages of world surfaces and entities, drawn again into the
//! glow target ([`crate::frame_target::aa::glow`]).
//!
//! rd-vanilla renders the frame's draw-surface list a second time with
//! `g_bRenderGlowingObjects` set (`tr_backend.cpp:1754-1757`): shaders without a glowing
//! stage are skipped (`tr_backend.cpp:774`), the other stages of a glowing shader are
//! rejected (`tr_shade.cpp:1638-1642`), and the colour buffer starts black while the
//! scene's depth is shared, not cleared (`tr_backend.cpp:556-560`). This module is that
//! second traversal for the main view: the prebuilt [`Runtime::glow_order`] for world
//! surfaces and the frame's entity queues for models, each glowing pass drawn with the
//! same vertex path, bind groups and blend as in the scene pass, through a pipeline
//! that targets the glow image and never writes depth.
//!
//! Not drawn: the sky, fog (stock fogs glow passes towards black, `tr_shade.cpp:1613`),
//! flares, the menu stage and secondary views (portals, sky portals, floor reflections).
use super::*;
use std::cell::OnceCell;

/// rd-vanilla `shader_t::hasGlow` (`tr_shader.cpp:2141-2143`), over the hardware passes'
/// glow flags.
pub(super) fn has_glow(passes: impl IntoIterator<Item = bool>) -> bool {
    passes.into_iter().any(|glow| glow)
}

/// The glowing colour passes of the scene's opaque then blended order, without fog. They
/// include stages that glow only for their emission map (`StagePass::emission_glow`).
pub(super) fn order(
    materials: &[Material],
    opaque: &[PassRef],
    blended: &[PassRef],
) -> Vec<PassRef> {
    opaque
        .iter()
        .chain(blended)
        .filter(|pass| {
            materials[pass.material]
                .stages
                .get(pass.stage)
                .is_some_and(|stage| stage.glow)
        })
        .copied()
        .collect()
}

/// Glow-target variants of every pipeline key, depth-tested or not, compiled on first
/// use like the scene's own entity pipelines.
#[derive(Default)]
pub(super) struct Pipelines {
    depth: Vec<OnceCell<wgpu::RenderPipeline>>,
    no_depth: Vec<OnceCell<wgpu::RenderPipeline>>,
}

impl Pipelines {
    /// One empty slot per list for the next key.
    pub(super) fn push(&mut self) {
        self.depth.push(OnceCell::new());
        self.no_depth.push(OnceCell::new());
    }

    /// Empty every slot: the stage program changed (`configure_model_sun`).
    pub(super) fn reset(&mut self, count: usize) {
        self.depth = (0..count).map(|_| OnceCell::new()).collect();
        self.no_depth = (0..count).map(|_| OnceCell::new()).collect();
    }
}

/// The glow pipeline key of a scene key: same blend, cull, depth test and bias, but the
/// glow pass only reads the scene's finished depth.
pub(super) fn key(key: PipelineKey) -> PipelineKey {
    PipelineKey {
        depth_write: false,
        ..key
    }
}

impl Runtime {
    /// Whether emission-mapped stages draw their halo (`r_emissiveGlow`, read from the
    /// lighting-mode word the frame already published).
    fn emission_glow(&self) -> bool {
        self.lighting_mode.get() & super::lighting_mode::NO_EMISSIVE_GLOW == 0
    }

    /// Compile now the depth-tested glow variants the glowing stages of materials
    /// `range` draw with, rather than on the first frame they glow: a driver compile
    /// in the middle of play is a stutter.
    pub(super) fn warm_glow(&self, range: std::ops::Range<usize>) {
        for material in &self.materials[range] {
            if !material.has_glow {
                continue;
            }
            for stage in material.stages.iter().filter(|stage| stage.glow) {
                self.glow_pipeline(stage.pipeline, true);
                self.glow_pipeline(stage.live_pipeline, true);
            }
        }
    }

    /// The glow pipeline of key `index`, compiled on first use.
    fn glow_pipeline(&self, index: usize, depth: bool) -> &wgpu::RenderPipeline {
        let slots = if depth {
            &self.glow_pipelines.depth
        } else {
            &self.glow_pipelines.no_depth
        };
        slots[index].get_or_init(|| self.make_glow_pipeline(index, depth)())
    }

    /// What compiles the glow pipeline of key `index`, on any thread.
    fn make_glow_pipeline(
        &self,
        index: usize,
        depth: bool,
    ) -> impl FnOnce() -> wgpu::RenderPipeline + Send + 'static {
        let scene_key = self.forge.pipeline_keys[index];
        // Material-mapped stages draw through the glow variant of their program, which
        // writes only the emission of stages drawn here for their emission map.
        let (layout, shader) = match &self.forge.material_maps {
            Some(maps) if scene_key.geometry & super::material_maps::PIPELINE_BIT != 0 => {
                maps.glow_program(&self.forge)
            }
            _ => self.forge.program_for(scene_key),
        };
        let (device, layout, shader) = (self.forge.device.clone(), layout.clone(), shader.clone());
        let format = crate::frame_target::aa::glow::world_format(self.forge.format);
        move || create_entity_pipeline(&device, &layout, &shader, format, key(scene_key), depth)
    }

    /// A job for the depth-tested glow pipeline of key `index` when it is not compiled yet.
    pub(super) fn glow_job(&self, index: usize, jobs: &mut super::pipeline_jobs::Jobs) {
        if self.glow_pipelines.depth[index].get().is_none() {
            jobs.push(
                super::pipeline_jobs::Slot::Glow { index },
                self.make_glow_pipeline(index, true),
            );
        }
    }

    /// Put a depth-tested glow pipeline a worker compiled in its slot.
    pub(super) fn install_glow(&self, index: usize, pipeline: wgpu::RenderPipeline) {
        if let Some(cell) = self.glow_pipelines.depth.get(index) {
            let _ = cell.set(pipeline);
        }
    }

    /// Whether the main view shows any glowing world pass or entity this frame. Walks
    /// only the glowing passes and the entity queues; visible ranges come from the
    /// scene pass's per-view caches.
    pub(crate) fn glow_visible(
        &self,
        frame: &FrameDraw<'_>,
        entities: [&[crate::entity_materials::Draw]; 2],
    ) -> bool {
        let entity = entities.iter().flat_map(|draws| draws.iter()).any(|draw| {
            self.material(draw.material)
                .is_some_and(|material| material.has_glow)
        });
        if entity {
            return true;
        }
        if self.glow_order.is_empty() {
            return false;
        }
        let emission_glow = self.emission_glow();
        let active = self.active_materials(frame.source_cluster, frame.visibility);
        self.glow_order.iter().any(|reference| {
            if !active.flags[reference.material] {
                return false;
            }
            let material = &self.materials[reference.material];
            let Some(stage) = material.stages.get(reference.stage) else {
                return false;
            };
            if !emission_glow && stage.emission_glow {
                return false;
            }
            material.mover_draws.iter().any(|draw| {
                frame
                    .mover_ranges
                    .get(draw.mesh)
                    .is_some_and(|range| !range.is_empty())
            }) || self
                .visible_static_ranges(material, frame.source_cluster, frame.visibility)
                .next()
                .is_some()
        })
    }

    /// Draw the main view's glowing world passes, then the glowing stages of its opaque
    /// and blended entities, into a pass whose colour target is the glow image's
    /// `world_format` view and whose depth is the scene's, read-only.
    pub(crate) fn draw_glow<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        frame: &FrameDraw<'pass>,
        entities: [&'pass [crate::entity_materials::Draw]; 2],
    ) {
        pass.set_bind_group(0, frame.camera, &[]);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        if let Some(sun) = &self.forge.model_sun {
            pass.set_bind_group(3, sun.binding(true), &[]);
        }
        let live_emission = self.lighting_mode.get() & 3 == 0 && self.realtime_materials_active();
        pass.set_vertex_buffer(0, frame.vertices.slice(..));
        pass.set_index_buffer(frame.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut last_pipeline = None;
        let mut last_bind_group = None;
        if !self.glow_order.is_empty() {
            let emission_glow = self.emission_glow();
            let active = self.active_materials(frame.source_cluster, frame.visibility);
            for &reference in &self.glow_order {
                if !active.flags[reference.material] {
                    continue;
                }
                let material = &self.materials[reference.material];
                let Some(stage) = material.stages.get(reference.stage) else {
                    continue;
                };
                if !emission_glow && stage.emission_glow {
                    continue;
                }
                let index = if live_emission {
                    stage.live_pipeline
                } else {
                    stage.pipeline
                };
                let mut ranges = self
                    .visible_static_ranges(material, frame.source_cluster, frame.visibility)
                    .peekable();
                let movers = material.mover_draws.iter().filter_map(|draw| {
                    frame
                        .mover_ranges
                        .get(draw.mesh)
                        .filter(|range| !range.is_empty())
                        .map(|range| (draw, range))
                });
                if ranges.peek().is_none() && movers.clone().next().is_none() {
                    continue;
                }
                if last_pipeline != Some((index, true)) {
                    pass.set_pipeline(self.glow_pipeline(index, true));
                    last_pipeline = Some((index, true));
                }
                if last_bind_group != Some(reference) {
                    pass.set_bind_group(1, stage.color_group(), &[]);
                    last_bind_group = Some(reference);
                }
                if ranges.peek().is_some() {
                    pass.set_vertex_buffer(1, self.forge.identity_instance.slice(..));
                    for range in ranges {
                        pass.draw_indexed(range, 0, 0..1);
                    }
                }
                let mut movers = movers.peekable();
                if movers.peek().is_some() {
                    pass.set_vertex_buffer(1, frame.instances.slice(..));
                    for (draw, range) in movers {
                        pass.draw_indexed(draw.indices.clone(), 0, range.clone());
                    }
                }
            }
        }
        pass.set_vertex_buffer(1, frame.instances.slice(..));
        for draw in entities.iter().flat_map(|draws| draws.iter()) {
            let Some(runtime_material) = self
                .source_to_runtime
                .get(draw.material)
                .copied()
                .filter(|index| *index != usize::MAX)
            else {
                continue;
            };
            let material = &self.materials[runtime_material];
            if !material.has_glow {
                continue;
            }
            for (stage_index, stage) in material.stages.iter().enumerate() {
                if !stage.glow {
                    continue;
                }
                let index = match (draw.forced_alpha, live_emission) {
                    (false, false) => stage.pipeline,
                    (false, true) => stage.live_pipeline,
                    (true, live) => stage.forced_alpha_pipelines[usize::from(live)],
                };
                let pipeline = (index, !draw.no_depth);
                if last_pipeline != Some(pipeline) {
                    pass.set_pipeline(self.glow_pipeline(index, !draw.no_depth));
                    last_pipeline = Some(pipeline);
                }
                let bind = PassRef {
                    material: runtime_material,
                    stage: stage_index,
                };
                if last_bind_group != Some(bind) {
                    pass.set_bind_group(1, stage.color_group(), &[]);
                    last_bind_group = Some(bind);
                }
                pass.draw_indexed(draw.indices.clone(), 0, draw.instances.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::has_glow;
    use crate::world_stage::collapse_multitexture;
    use sjk_shader::parse_shader_script;

    /// The hardware passes' glow flags of one shader text, after rd-vanilla's collapse.
    fn passes(body: &str) -> Vec<bool> {
        let script = format!(
            "models/test/glow
{{
{body}
}}
"
        );
        let mut definitions =
            parse_shader_script(script.as_bytes(), "shaders/glow_test.shader").unwrap();
        collapse_multitexture(&definitions.remove(0).stages)
            .iter()
            .map(|stage| stage.glow)
            .collect()
    }

    #[test]
    fn possessed_tavion_glows_through_both_passes() {
        // assets1.pk3 players.shader torso_blue_glow: the lit diffuse and the additive
        // glow map both carry `glow`. Their colour generators differ after defaults
        // (lightingDiffuse, identityLighting), so they stay two passes and the whole lit
        // body glows, not just the glow map.
        let glow = passes(
            "{ map models/players/tavion_new/torso_blue blendFunc GL_ONE GL_ZERO glow
               rgbGen lightingDiffuse }
             { map models/players/tavion_new/torso_blue_glow2 blendFunc GL_ONE GL_ONE glow }",
        );
        assert_eq!(glow, [true, true]);
        assert!(has_glow(glow));
    }

    #[test]
    fn olol_glows_only_through_its_third_stage() {
        // JoF_ModelOlol.pk3 olol.shader face: lit diffuse, specular detail, then the
        // additive `_glow` map with `glow`.
        let glow = passes(
            "{ map models/players/olol/face blendFunc GL_ONE GL_ZERO rgbGen lightingDiffuse }
             { map models/players/olol/face blendFunc GL_SRC_ALPHA GL_ONE detail
               alphaGen lightingSpecular }
             { map models/players/olol/face_glow blendFunc GL_ONE GL_ONE glow
               rgbGen identity }",
        );
        assert_eq!(glow, [false, false, true]);
        assert!(has_glow(glow));
    }

    #[test]
    fn a_glowing_pair_that_collapses_glows_as_one_pass() {
        let glow = passes(
            "{ map textures/test/lamp rgbGen identity glow }
             { map textures/test/lamp_add blendFunc GL_ONE GL_ONE rgbGen identity glow }",
        );
        assert_eq!(glow, [true]);
    }

    #[test]
    fn a_collapsed_pass_keeps_the_first_stage_glow_flag() {
        // rd-vanilla moves only the texture bundles: a glowing texture collapsed under a
        // leading lightmap does not glow; one ahead of a lightmap does.
        let behind = passes(
            "{ map $lightmap tcGen lightmap }
             { map textures/test/lamp blendFunc GL_DST_COLOR GL_ZERO rgbGen identity glow }",
        );
        assert_eq!(behind, [false]);
        assert!(!has_glow(behind));
        let ahead = passes(
            "{ map textures/test/lamp rgbGen identity glow }
             { map $lightmap tcGen lightmap blendFunc GL_DST_COLOR GL_ZERO }",
        );
        assert_eq!(ahead, [true]);
    }

    #[test]
    fn a_stage_that_does_not_collapse_keeps_its_own_flag() {
        let glow = passes(
            "{ map textures/test/base }
             { map textures/test/lamp blendFunc GL_ONE GL_ONE glow rgbGen wave sin 0.5 0.5 0 1 }",
        );
        assert_eq!(glow, [false, true]);
        assert!(!has_glow([false, false]));
    }
}
