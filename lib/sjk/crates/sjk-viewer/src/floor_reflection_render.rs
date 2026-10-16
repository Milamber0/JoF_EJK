//! Render each visible plane through the real mirror camera and composite before main fog.
use super::*;

impl GpuState {
    pub(crate) fn has_floor_surfaces(&self) -> bool {
        self.scene_views.floors.target.is_some()
    }
    pub(crate) fn has_floor_reflections(&self) -> bool {
        self.scene_views.floors.active()
    }

    pub(crate) fn draw_floor_reflections(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        main: &crate::world_materials::FrameDraw<'_>,
        shadows: bool,
        particles: &crate::effect_submission::Ranges,
    ) {
        let floors = &self.scene_views.floors;
        let Some(target) = &floors.target else {
            return;
        };
        // Query/compute setup did not pay for itself on the one/two-view routes.
        // Amortize it across several candidates; either path has identical pixels.
        let check_visibility = floors.floors.iter().filter(|f| f.visible).take(4).count() == 4;

        if check_visibility {
            if let Some(check) = &floors.visibility {
                check.encode(self, encoder, main);
            }
        }
        let visibility = self.bsp.render().visibility();
        // Mirrors are lit in their own light-buffer images when there are any.
        let preserved = shadows && self.world_materials.begin_mirror_light();
        if let Some(phases) = &self.gpu_phases {
            phases.mark(encoder, "floor-save");
        }
        let mut rendered = false;
        let mut command_slot = 0;
        for (_index, floor) in floors.floors.iter().enumerate().filter(|(_, f)| f.visible) {
            // Every mirror is a whole scene: finish each on its own worker.
            self.frame_pacer.split.cut(&self.device, encoder);

            let _view_culling = self.world_materials.view_culling.camera(floor.clip);
            let commands = floors
                .visibility
                .as_ref()
                .and_then(|v| v.commands.as_ref())
                .filter(|_| {
                    check_visibility
                        && !floors.commands_disabled.get()
                        && command_slot < MAX_MIRRORS
                });
            let command_start = commands.map(|commands| {
                let start = self.world_materials.begin_mirror_commands();
                commands.encode(encoder, command_slot);
                start
            });
            let camera = &floor.camera.group;
            let input = crate::world_materials::FrameDraw {
                camera,
                vertices: main.vertices,
                indices: main.indices,
                instances: main.instances,
                mover_ranges: main.mover_ranges,
                source_cluster: floors.cluster,
                visibility,
                entities: self.entity_draw_queue.opaque(),
            };
            // Reuse the light buffer sequentially. Each mirror is consumed before the
            // next overwrites it; main lighting is restored once before later materials.
            let occlusion_region = Some(floor.region);

            if shadows {
                self.world_materials.draw_light_buffer_region(
                    encoder,
                    &input,
                    None,
                    Some(floor.region),
                    [floors.scale; 2],
                    occlusion_region,
                );
            }
            // A mirror's light buffer and its scene finish on separate workers.
            self.frame_pacer.split.cut(&self.device, encoder);
            {
                let clip =
                    bounds::receiver_frustum(floor.clip, floor.region, target.size, floors.scale);

                let _receiver_culling = self.world_materials.view_culling.camera(clip);
                let [x, y, w, h] = bounds::pixels(floor.region, target.size);
                let depth_primed = self.world_materials.prime_depth_region(
                    encoder,
                    &target.depth.view,
                    &input,
                    Some([x, y, w, h]),
                );
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("SJK floor reflected scene"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.color,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &target.depth.view,
                        depth_ops: Some(wgpu::Operations {
                            load: if depth_primed {
                                wgpu::LoadOp::Load
                            } else {
                                wgpu::LoadOp::Clear(1.)
                            },
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let world = &self.world_materials;
                pass.set_scissor_rect(x, y, w, h);

                world.draw_sky(
                    &mut pass,
                    camera,
                    main.vertices,
                    main.indices,
                    floors.cluster,
                    visibility,
                );
                world.draw_main_opaque(
                    &mut pass,
                    camera,
                    main.vertices,
                    main.indices,
                    main.instances,
                    main.mover_ranges,
                    floors.cluster,
                    visibility,
                );
                world.draw_main_entities(
                    &mut pass,
                    camera,
                    main.vertices,
                    main.indices,
                    main.instances,
                    self.entity_draw_queue.opaque(),
                );
                world.draw_decals(
                    &mut pass,
                    camera,
                    main.vertices,
                    main.indices,
                    main.instances,
                    main.mover_ranges,
                    floors.cluster,
                    visibility,
                    true,
                );
                world.draw_fog(&mut pass, &input);
                world.draw_main_blended(
                    &mut pass,
                    camera,
                    main.vertices,
                    main.indices,
                    main.instances,
                    main.mover_ranges,
                    floors.cluster,
                    visibility,
                );
                world.draw_main_entities(
                    &mut pass,
                    camera,
                    main.vertices,
                    main.indices,
                    main.instances,
                    self.entity_draw_queue.blended(),
                );
                pass.set_bind_group(0, camera, &[]);
                pass.set_bind_group(1, &self.particle_atlas.bind_group, &[]);
                pass.set_vertex_buffer(0, self.entity_instance_buffer.slice(..));
                if !particles.opaque.is_empty() {
                    pass.set_pipeline(&self.entity_pipeline);
                    pass.draw(0..36, particles.opaque.clone());
                }
                drop(pass);
                self.composite_effects(
                    encoder,
                    &target.color,
                    &target.depth,
                    camera,
                    particles,
                    crate::particle_draw::EffectResolve::WriteBack,
                    Some([x, y, w, h]),
                );
            }
            {
                let queries = None;

                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("SJK floor composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth.view,
                        depth_ops: None,
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: queries,
                    multiview_mask: None,
                });

                pass.set_pipeline(&floors.pipeline);
                pass.set_bind_group(0, main.camera, &[]);
                pass.set_bind_group(1, &floors.finish.as_ref().unwrap().group, &[]);
                pass.set_vertex_buffer(0, main.vertices.slice(..));
                pass.set_index_buffer(main.indices.slice(..), wgpu::IndexFormat::Uint32);
                let mut bound = None;
                for face in &floor.plane.faces {
                    if self.world_materials.areas.visible(
                        &face.clusters,
                        floors.cluster,
                        visibility,
                    ) {
                        if bound != Some(face.maps) {
                            pass.set_bind_group(2, &floors.map_groups[face.maps], &[]);
                            bound = Some(face.maps);
                        }
                        pass.draw_indexed(face.indices.clone(), 0, 0..1);
                    }
                }
            }

            if let (Some(commands), Some(start)) = (commands, command_start) {
                commands.publish(
                    &self.queue,
                    command_slot,
                    _index,
                    start..self.world_materials.mirror_command_cursor(),
                );
            }
            command_slot += 1;
            rendered = true;
        }

        if let Some(phases) = &self.gpu_phases {
            phases.mark(encoder, "floor-planes");
        }
        if preserved {
            self.world_materials.end_mirror_light();
        } else if rendered && shadows {
            self.world_materials.draw_light_buffer(encoder, main, None);
        }
    }
}
