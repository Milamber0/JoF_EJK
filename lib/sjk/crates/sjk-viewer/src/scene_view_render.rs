//! Scene-view preparation and rendering, outside the frame-loop orchestrator.
use super::*;

impl GpuState {
    /// Resize depth and existing secondary targets at the window event, never at activation.
    pub(crate) fn resize_scene_depth(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.render_scale = None;
        self.depth = crate::DepthTarget::new(&self.device, size.width, size.height);
        self.world_materials
            .fit_light_buffer(&self.device, [size.width, size.height], 1);
        if self.post_aa.is_some() {
            self.post_aa = crate::frame_target::aa::Runtime::configured(
                &self.device,
                self.configuration.format,
                self.context.ui_direct,
                [size.width, size.height],
                self.context.fxaa,
                self.context.post_color.policy(),
                self.context.hdr,
                Some([size.width, size.height]),
            );
        }
        let views = &mut self.scene_views;
        for target in [&mut views.sky_target, &mut views.portal_target] {
            if target.is_some() {
                *target = Some(gpu::Target::new(
                    &self.device,
                    &views.camera_layout,
                    &views.sample_layout,
                    views.format,
                    [size.width, size.height],
                ));
            }
        }
        views.floors.resize(
            &self.device,
            &views.camera_layout,
            &views.sample_layout,
            views.format,
            [size.width, size.height],
        );
        self.install_render_scale();
    }

    /// Allocate all scaled scene attachments together; failures leave native rendering intact.
    pub(crate) fn install_render_scale(&mut self) {
        self.install_render_scale_for(self.context.render_scale);
    }

    /// Transaction shared by startup, resize and fixed offscreen evidence.
    pub(crate) fn install_render_scale_for(&mut self, requested: u32) {
        let output = [self.size.width, self.size.height];
        let scale = crate::frame_target::scale::supported_bytes(
            requested,
            output,
            self.device.limits().max_texture_dimension_2d,
            if self.context.hdr.mode == 1 { 48 } else { 32 },
        );
        for factor in (2..=scale).rev() {
            let views = &self.scene_views;
            let candidate = crate::frame_target::scale::checked(&self.device, || {
                let scaled = crate::frame_target::scale::Runtime::new(
                    &self.device,
                    self.context.scene_format(),
                    output,
                    factor,
                );
                let size = scaled.size;
                let depth = crate::DepthTarget::new(&self.device, size[0], size[1]);
                let target = |present: bool| {
                    present.then(|| {
                        gpu::Target::new(
                            &self.device,
                            &views.camera_layout,
                            &views.sample_layout,
                            views.format,
                            size,
                        )
                    })
                };
                let sky = target(views.sky_target.is_some());
                let portal = target(views.portal_target.is_some());
                let floors = target(views.floors.target.is_some());
                (scaled, depth, sky, portal, floors)
            });
            if let Some((scaled, depth, sky, portal, floors)) = candidate {
                self.depth = depth;
                self.scene_views.sky_target = sky;
                self.scene_views.portal_target = portal;
                self.scene_views.floors.set_target(floors, &self.device);
                self.render_scale = Some(scaled);
                let scene = self.scene_size();
                if let Some(aa) = &mut self.post_aa {
                    aa.fit_effects(&self.device, scene);
                }
                self.world_materials
                    .fit_light_buffer(&self.device, scene, factor);
                crate::log::progress(format_args!(
                    "scene scale requested={requested} effective={factor}"
                ));
                return;
            }
        }
        if requested > 1 {
            crate::log::progress(format_args!(
                "scene scale requested={requested}; native fallback"
            ));
        }
    }

    /// Select visible secondary views and upload their small camera uniforms.
    pub(crate) fn prepare_scene_views(&mut self, view: Mat4, projection: Mat4, time: i32) {
        self.world_materials.begin_world_frame();
        self.world_materials
            .view_culling
            .prepare_main(projection * view);
        if self.scene_views.needs_environment {
            self.geometry.update_environment(
                &self.queue,
                time,
                projection,
                self.console.as_ref().map(|c| &c.geometry_controls),
            );
        }
        self.scene_views.floors.prepare(
            &self.queue,
            &self.bsp,
            &self.world_materials.areas,
            view,
            projection,
            time,
        );
        self.scene_views.selection = None;
        self.scene_views.sky_view = None;
        let game = self
            .live_session
            .as_ref()
            .map(|s| s.game_state())
            .or_else(|| self.demo_session.as_ref().map(|s| s.game_state()))
            .or_else(|| self.resident.scenery.as_ref().map(|(game, _)| game));
        let snapshot = crate::first_person_view::presented_snapshot(
            self.live_session.as_ref(),
            self.demo_session.as_ref(),
            time,
        )
        .or_else(|| self.resident.scenery.as_ref().map(|(_, snapshot)| snapshot));

        if let (Some(game), Some(snapshot)) = (game, snapshot) {
            self.scene_views.select(
                &self.bsp,
                game,
                snapshot,
                view,
                projection,
                time,
                self.world_materials.areas.mask(),
            );
        } else if self.resident.exploring() || crate::host::active() {
            // Without SJK's own session (exploring, or hosted by another engine) the sky
            // portal comes from the map itself.
            self.scene_views.select_sky(
                &self.bsp,
                self.scene_views.offline_sky,
                view,
                projection,
                self.world_materials.areas.mask(),
            );
        } else {
            return;
        }

        let scene_size = self.scene_size();
        let views = &mut self.scene_views;
        if let (Some(selection), Some(target)) = (&views.selection, &views.portal_target) {
            debug_assert_eq!(target.size, scene_size);
            let p = crate::oblique_clip::oblique_projection(
                projection,
                selection.view.matrix,
                selection.view.clip_point,
                selection.view.clip_normal,
            );
            upload(&self.queue, target, selection.view, p, time, false);
            self.queue.write_buffer(
                &target.instances,
                0,
                bytemuck::bytes_of(&selection.instance),
            );
        }
        if let (Some(view), Some(target)) = (views.sky_view, &views.sky_target) {
            upload(&self.queue, target, view, projection, time, true);
        }
    }

    /// Never calls itself: secondary views deliberately do not recurse.
    pub(crate) fn draw_scene_views(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        particles: &crate::effect_submission::Ranges,
    ) {
        for (index, (view, target)) in [
            (
                self.scene_views.selection.as_ref().map(|s| s.view),
                self.scene_views.portal_target.as_ref(),
            ),
            (
                self.scene_views.sky_view,
                self.scene_views.sky_target.as_ref(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let (Some(view), Some(target)) = (view, target) else {
                continue;
            };
            let leaf = self.bsp.leaf_at(view.pvs.to_array());
            let cluster = usize::try_from(self.bsp.leaves()[leaf].cluster).ok();
            let visibility = self.bsp.render().visibility();
            // CG_CalcViewValues clears refdef; CG_DrawActiveFrame copies the main mask
            // only AFTER CG_DrawSkyBoxPortal (cg_view.c:1511,2637,2672).
            let unmasked_sky = index == 1;

            let main_areas = unmasked_sky.then(|| std::mem::take(&mut self.world_materials.areas));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK map secondary view"),
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
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let camera = &target.camera_bind;
            let geometry = &self.geometry;
            self.world_materials.draw_sky(
                &mut pass,
                camera,
                &geometry.vertex_buffer,
                &geometry.index_buffer,
                cluster,
                visibility,
            );
            self.world_materials.draw_opaque(
                &mut pass,
                camera,
                &geometry.vertex_buffer,
                &geometry.index_buffer,
                &self.actor_instance_buffer,
                &self.mover_instance_ranges,
                cluster,
                visibility,
            );
            self.world_materials.draw_entities(
                &mut pass,
                camera,
                &geometry.vertex_buffer,
                &geometry.index_buffer,
                &self.actor_instance_buffer,
                self.entity_draw_queue.opaque(),
            );
            let fog = crate::world_materials::FrameDraw {
                camera,
                vertices: &geometry.vertex_buffer,
                indices: &geometry.index_buffer,
                instances: &self.actor_instance_buffer,
                mover_ranges: &self.mover_instance_ranges,
                source_cluster: cluster,
                visibility,
                entities: self.entity_draw_queue.opaque(),
            };
            self.world_materials.draw_decals(
                &mut pass,
                camera,
                &geometry.vertex_buffer,
                &geometry.index_buffer,
                &self.actor_instance_buffer,
                &self.mover_instance_ranges,
                cluster,
                visibility,
                false,
            );
            self.world_materials.draw_fog(&mut pass, &fog);
            self.world_materials.draw_blended(
                &mut pass,
                camera,
                &geometry.vertex_buffer,
                &geometry.index_buffer,
                &self.actor_instance_buffer,
                &self.mover_instance_ranges,
                cluster,
                visibility,
            );
            self.world_materials.draw_entities(
                &mut pass,
                camera,
                &geometry.vertex_buffer,
                &geometry.index_buffer,
                &self.actor_instance_buffer,
                self.entity_draw_queue.blended(),
            );
            if index == 0 {
                pass.set_bind_group(0, camera, &[]);
                pass.set_bind_group(1, &self.particle_atlas.bind_group, &[]);
                pass.set_vertex_buffer(0, self.entity_instance_buffer.slice(..));
                if !particles.opaque.is_empty() {
                    pass.set_pipeline(&self.entity_pipeline);
                    pass.draw(0..36, particles.opaque.clone());
                }
            }
            drop(pass);
            if index == 0 {
                self.composite_effects(
                    encoder,
                    &target.color,
                    &target.depth,
                    camera,
                    particles,
                    crate::particle_draw::EffectResolve::WriteBack,
                    None,
                );
            }
            if self.world_materials.has_flares() && self.geometry.environment.data.control[3] != 0.
            {
                let mut pass = crate::world_materials::flares::begin_pass(
                    encoder,
                    &target.color,
                    &target.depth.view,
                );
                self.world_materials
                    .draw_flares(&mut pass, &fog, &target.depth.sample_bind_group);
            }
            if let Some(areas) = main_areas {
                self.world_materials.areas = areas;
            }
        }
    }

    /// The main view's sky on the visible sky faces, depth-tested and depth-writing: a sky
    /// portal's view where the map has one, else the Q3 sky stages.
    /// It used to be a whole-frame copy ahead of the world plus a depth-only mask: at
    /// render scale 2 that shaded eleven million pixels for the few a room's windows show.
    /// Drawn after the opaque world, hidden sky is rejected before it is shaded.
    pub(crate) fn draw_sky_portal_faces<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        cluster: Option<usize>,
        visibility: Option<&'a sjk_bsp::Visibility>,
    ) {
        let Some(target) = self
            .scene_views
            .sky_target
            .as_ref()
            .filter(|_| self.scene_views.sky_view.is_some())
        else {
            // No sky portal: the Q3 sky stages, shaded on their faces.
            self.world_materials.draw_main_sky_faces(
                pass,
                &self.camera_bind_group,
                &self.geometry.vertex_buffer,
                &self.geometry.index_buffer,
                cluster,
                visibility,
            );
            return;
        };
        pass.set_pipeline(&self.scene_views.sky_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, &target.normal_sample, &[]);
        self.world_materials.draw_sky_faces(
            pass,
            &self.geometry.vertex_buffer,
            &self.geometry.index_buffer,
            cluster,
            visibility,
        );
    }

    pub(crate) fn composite_map_portal<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let (Some(selection), Some(target)) =
            (&self.scene_views.selection, &self.scene_views.portal_target)
        else {
            return;
        };
        let selected = &self.scene_views.faces[selection.face];
        pass.set_pipeline(&self.scene_views.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(
            1,
            if selection.view.mirror {
                &target.mirror_sample
            } else {
                &target.normal_sample
            },
            &[],
        );
        pass.set_vertex_buffer(0, self.geometry.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, target.instances.slice(..));
        pass.set_index_buffer(
            self.geometry.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        for face in &self.scene_views.faces {
            if face.model == selected.model
                && face.normal.dot(selected.normal) > 0.9999
                && (face.distance - selected.distance).abs() < 0.1
                && self.scene_views.portal_areas.visible(
                    &face.clusters,
                    self.scene_views.portal_cluster,
                    self.bsp.render().visibility(),
                )
            {
                pass.draw_indexed(face.indices.clone(), 0, 0..1);
            }
        }
    }
}

fn upload(
    queue: &crate::frame_queue::FrameQueue,
    target: &gpu::Target,
    view: math::View,
    projection: Mat4,
    time: i32,
    sky: bool,
) {
    queue.write_buffer(
        &target.camera,
        0,
        bytemuck::bytes_of(&CameraUniform {
            view_projection: (projection * view.matrix).to_cols_array_2d(),
            camera_position: view.eye.to_array(),
            view_forward: view.forward.to_array(),
            shader_time: time as f32 * 0.001,
            _padding: if sky { 5.0 } else { 1.0 },
        }),
    );
}
