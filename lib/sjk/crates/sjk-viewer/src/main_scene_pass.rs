//! Main-view pass boundaries; secondary views keep their own unchanged renderer.
use crate::particle_draw::EffectResolve;
use crate::*;

impl GpuState {
    /// Split depth writes from optional ambient/shadow correction on the main view only.
    pub(crate) fn encode_world_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        source_cluster: Option<usize>,
        particle_ranges: &effect_submission::Ranges,
        has_entity_instances: bool,
    ) {
        let visibility = self.bsp.render().visibility();
        let _view_culling = self.world_materials.view_culling.main();
        let floor_reflections = self.has_floor_reflections();
        let ao_enabled = self.context.ssao.enabled() && self.world_materials.has_ssao_receivers();
        if ao_enabled {
            self.world_materials
                .set_ssao_intensity(&self.queue, self.context.ssao.intensity());
        }
        let shadow_input = world_materials::FrameDraw {
            camera: &self.camera_bind_group,
            vertices: &self.geometry.vertex_buffer,
            indices: &self.geometry.index_buffer,
            instances: &self.actor_instance_buffer,
            mover_ranges: &self.mover_instance_ranges,
            source_cluster,
            visibility,
            entities: self.entity_draw_queue.opaque(),
        };
        let shadows = self.world_materials.draw_sun_casters(
            encoder,
            &self.queue,
            &shadow_input,
            &self.actor_instances,
            self.actor_instance_ranges.last().map_or(0, |r| r.end),
            self.gpu_phases.as_ref(),
        );
        if let Some(phases) = &self.gpu_phases {
            phases.mark(encoder, "actor-casters");
        }
        // Reflection probes are captured with this frame's cascades, before the main
        // view's light pass overwrites the light buffer they borrow.
        self.capture_reflection_probes(encoder, shadows);
        self.frame_pacer.split.cut(&self.device, encoder);
        if shadows {
            self.world_materials.draw_light_buffer(
                encoder,
                &shadow_input,
                self.gpu_phases.as_ref(),
            );
        }
        if let Some(phases) = &self.gpu_phases {
            phases.mark(encoder, "light-pass");
        }
        self.frame_pacer.split.cut(&self.device, encoder);
        let depth_primed =
            self.world_materials
                .prime_depth(encoder, &self.depth.view, &shadow_input);
        {
            let mut pass = {
                scene_pass(
                    encoder,
                    target_view,
                    &self.depth.view,
                    frame_target::world_load(),
                    if depth_primed {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(1.0)
                    },
                )
            };
            let fog_frame = world_materials::FrameDraw {
                camera: &self.camera_bind_group,
                vertices: &self.geometry.vertex_buffer,
                indices: &self.geometry.index_buffer,
                instances: &self.actor_instance_buffer,
                mover_ranges: &self.mover_instance_ranges,
                source_cluster,
                visibility,
                entities: self.entity_draw_queue.opaque(),
            };

            self.composite_map_portal(&mut pass);
            self.world_materials.draw_main_opaque(
                &mut pass,
                &self.camera_bind_group,
                &self.geometry.vertex_buffer,
                &self.geometry.index_buffer,
                &self.actor_instance_buffer,
                &self.mover_instance_ranges,
                source_cluster,
                visibility,
            );
            // Rain wets the world before the players are drawn: the depth it reads
            // then holds only the world, so players stay dry.
            if self.weather.wet() {
                drop(pass);
                if let Some(phases) = &self.gpu_phases {
                    phases.mark(encoder, "world-opaque");
                }
                self.weather.draw_wet(
                    encoder,
                    target_view,
                    &self.camera_bind_group,
                    &self.depth.sample_bind_group,
                );
                if let Some(phases) = &self.gpu_phases {
                    phases.mark(encoder, "rain-wet");
                }
                pass = scene_pass(
                    encoder,
                    target_view,
                    &self.depth.view,
                    wgpu::LoadOp::Load,
                    wgpu::LoadOp::Load,
                );
            }
            self.world_materials.draw_main_entities(
                &mut pass,
                &self.camera_bind_group,
                &self.geometry.vertex_buffer,
                &self.geometry.index_buffer,
                &self.actor_instance_buffer,
                self.entity_draw_queue.opaque(),
            );
            // The main view's sky is shaded on its visible faces after the opaque world.
            self.draw_sky_portal_faces(&mut pass, source_cluster, visibility);
            self.draw_clouds(&mut pass, source_cluster, visibility);
            if ao_enabled || shadows || floor_reflections {
                drop(pass);
                if let Some(phases) = &self.gpu_phases {
                    phases.mark(encoder, "opaque");
                }
                self.frame_pacer.split.cut(&self.device, encoder);
                if ao_enabled {
                    self.world_materials.draw_ssao(
                        encoder,
                        target_view,
                        &self.depth,
                        &fog_frame,
                        shadows,
                    );
                }
                if shadows {
                    self.world_materials.draw_sun_receivers(
                        encoder,
                        target_view,
                        &self.depth,
                        &fog_frame,
                    );
                }
                if let Some(phases) = &self.gpu_phases {
                    phases.mark(encoder, "ambient-correction");
                }
                if floor_reflections {
                    self.draw_floor_reflections(
                        encoder,
                        target_view,
                        &fog_frame,
                        shadows,
                        particle_ranges,
                    );
                } else if let Some(phases) = &self.gpu_phases {
                    // The same sections every frame, for per-frame phase tables.
                    phases.mark(encoder, "floor-save");
                    phases.mark(encoder, "floor-planes");
                }
                if let Some(phases) = &self.gpu_phases {
                    phases.mark(encoder, "floor-reflections");
                }
                self.frame_pacer.split.cut(&self.device, encoder);

                if let Some(phases) = &self.gpu_phases {
                    phases.mark(encoder, "water");
                }
                pass = scene_pass(
                    encoder,
                    target_view,
                    &self.depth.view,
                    wgpu::LoadOp::Load,
                    wgpu::LoadOp::Load,
                );
            }
            self.world_materials.draw_decals(
                &mut pass,
                &self.camera_bind_group,
                &self.geometry.vertex_buffer,
                &self.geometry.index_buffer,
                &self.actor_instance_buffer,
                &self.mover_instance_ranges,
                source_cluster,
                visibility,
                true,
            );
            self.world_materials.draw_fog(&mut pass, &fog_frame);
            self.menu_stage.draw(
                &mut pass,
                &self.world_materials,
                &self.camera_bind_group,
                false,
            );
            self.world_materials.draw_main_blended(
                &mut pass,
                &self.camera_bind_group,
                &self.geometry.vertex_buffer,
                &self.geometry.index_buffer,
                &self.actor_instance_buffer,
                &self.mover_instance_ranges,
                source_cluster,
                visibility,
            );
            self.world_materials.draw_main_entities(
                &mut pass,
                &self.camera_bind_group,
                &self.geometry.vertex_buffer,
                &self.geometry.index_buffer,
                &self.actor_instance_buffer,
                self.entity_draw_queue.blended(),
            );
            self.menu_stage.draw(
                &mut pass,
                &self.world_materials,
                &self.camera_bind_group,
                true,
            );
            if has_entity_instances {
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_bind_group(1, &self.particle_atlas.bind_group, &[]);
                pass.set_vertex_buffer(0, self.entity_instance_buffer.slice(..));
                if !particle_ranges.opaque.is_empty() {
                    pass.set_pipeline(&self.entity_pipeline);
                    pass.draw(0..36, particle_ranges.opaque.clone());
                }
            }
        }
        self.encode_card_outline_mask(encoder, source_cluster);
        self.composite_effects(
            encoder,
            target_view,
            &self.depth,
            &self.camera_bind_group,
            particle_ranges,
            EffectResolve::Merge,
            None,
        );
        if let Some(phases) = &self.gpu_phases {
            phases.mark(encoder, "scene-pass");
        }
        self.encode_glow(encoder, source_cluster, particle_ranges);
        if let Some(phases) = &self.gpu_phases {
            phases.mark(encoder, "glow");
        }
        if shadows {
            self.world_materials.draw_volumetrics(
                encoder,
                target_view,
                &self.depth.sample_bind_group,
            );
            self.draw_dust_motes(encoder, target_view);
        }
        if let Some(phases) = &self.gpu_phases {
            phases.mark(encoder, "volumetrics");
        }
    }
}

impl GpuState {
    /// Draw the outlined bodies (the inspected player's, the team outlines') into the
    /// outlines' mask, against the finished scene depth (`card_outline.rs`). Cleared
    /// even with no body this frame.
    fn encode_card_outline_mask(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source_cluster: Option<usize>,
    ) {
        let Some(mask) = self.card_outline.mask_target() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK card outline mask"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: mask,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth.view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let draws = self.entity_draw_queue.outline();
        if draws.is_empty() {
            return;
        }
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
        self.world_materials.draw_card_outline_mask(
            &mut pass,
            &frame,
            draws,
            &self.model_material_overrides.outline_groups(),
        );
    }
}

/// Begin or resume main-scene attachment writes without changing their formats.
impl GpuState {
    /// The main view with no world: only the clears, for frames the classic
    /// menu covers with an opaque screen.
    pub(crate) fn encode_cleared_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) {
        self.clear_glow();
        let _pass = scene_pass(
            encoder,
            target_view,
            &self.depth.view,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            wgpu::LoadOp::Clear(1.0),
        );
    }
}

pub(crate) fn scene_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    color_load: wgpu::LoadOp<wgpu::Color>,
    depth_load: wgpu::LoadOp<f32>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("SJK world pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: color_load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: depth_load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
