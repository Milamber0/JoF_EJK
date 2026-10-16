//! The main view's dynamic glow sources (`r_DynamicGlow`), drawn after its effects.
//!
//! Stock draws the frame's surfaces a second time with only their glowing stages into a
//! black image that shares the scene's depth (`tr_backend.cpp:1736-1800`); the resolve
//! blurs and adds it (`post_glow.rs`). World surfaces and models draw through the image's
//! `world_format` view with glow variants of their stage pipelines, then the glowing
//! billboards, effect geometry and sabers through its display-value view with their
//! ordinary effect-layer pipelines. A frame without any glowing source records nothing
//! and the resolve skips the blur and the composite.
use crate::frame_target::aa::glow::Mode;
use crate::*;

impl GpuState {
    /// Draw this frame's glowing stages for the main view.
    pub(crate) fn encode_glow(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source_cluster: Option<usize>,
        particle_ranges: &effect_submission::Ranges,
    ) {
        let Some(glow) = self.post_aa.as_ref().and_then(|aa| aa.glow()) else {
            return;
        };
        let live = self.context.post_color.glow().live();
        glow.set_live(&self.queue, live);
        let depth_size = self.depth.view.texture().size();
        if glow.size() != [depth_size.width, depth_size.height] {
            glow.set_drawn(&self.queue, false, false);
            return;
        }
        let blades_only = live.mode == Mode::Sabers;
        let frame = world_materials::FrameDraw {
            camera: &self.camera_bind_group,
            vertices: &self.geometry.vertex_buffer,
            indices: &self.geometry.index_buffer,
            instances: &self.actor_instance_buffer,
            mover_ranges: &self.mover_instance_ranges,
            source_cluster,
            visibility: self.bsp.render().visibility(),
            entities: &[],
        };
        let entities = [
            self.entity_draw_queue.opaque(),
            self.entity_draw_queue.blended(),
        ];
        let world = !blades_only && self.world_materials.glow_visible(&frame, entities);
        let particles = !blades_only && particle_ranges.glow().any(|range| !range.is_empty());
        let geometry = !blades_only && self.effect_geometry.has_glow();
        let sabers = self.saber_gpu.has_glow(blades_only);
        let effects = particles || geometry || sabers;
        glow.set_drawn(&self.queue, world || effects, true);
        if world {
            let mut pass = glow.begin(encoder, &self.depth.view, true, true);
            self.world_materials.draw_glow(&mut pass, &frame, entities);
        }
        if !effects {
            return;
        }
        let mut pass = glow.begin(encoder, &self.depth.view, false, !world);
        let camera = &self.camera_bind_group;
        if particles {
            pass.set_bind_group(0, camera, &[]);
            pass.set_bind_group(1, &self.particle_atlas.bind_group, &[]);
            let soft = self.soften_particles(particle_ranges);
            if soft {
                pass.set_bind_group(2, &self.depth.sample_bind_group, &[]);
            }
            pass.set_vertex_buffer(0, self.entity_instance_buffer.slice(..));
            self.particle_pipelines
                .draw(&mut pass, particle_ranges.glow(), soft);
        }
        if geometry {
            self.effect_geometry
                .draw_glow(&mut pass, camera, &self.particle_atlas.bind_group);
        }
        if sabers {
            self.saber_gpu.draw_glow(&mut pass, camera, blades_only);
        }
    }

    /// A frame without the main world (an opaque menu) shows no glow.
    pub(crate) fn clear_glow(&self) {
        if let Some(glow) = self.post_aa.as_ref().and_then(|aa| aa.glow()) {
            glow.set_live(&self.queue, self.context.post_color.glow().live());
            glow.set_drawn(&self.queue, false, false);
        }
    }
}
