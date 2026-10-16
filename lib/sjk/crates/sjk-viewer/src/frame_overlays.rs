//! Depth-sampling world accents precede HUD/text in a read-only depth pass.
use crate::GpuState;

impl GpuState {
    /// Filter the completed scene after flares, then draw untouched HUD/text into final color.
    pub(crate) fn draw_frame_overlays(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        output: &wgpu::TextureView,
        output_ui: &wgpu::TextureView,
        cluster: Option<usize>,
        visibility: Option<&sjk_bsp::Visibility>,
        text: u32,
        classic_text: u32,
    ) {
        // The game menu, the report and note dialog and a world note's selection also
        // show over a map explored without a server.
        let hud = self.live_session.is_some()
            || self.demo_session.is_some()
            || self.game_menu
            || self.text_dialog.is_open()
            || self.world_notes.highlight().is_some()
            || self.entity_view.active()
            || self.console.as_ref().is_some_and(|c| c.is_open())
            || !self.console_layer.is_empty()
            || self.client_menu.as_ref().is_some_and(|m| m.is_visible());
        // A world shot's made-up scoreboard, quick wheel, unlock pop-up, chat box and pings
        // draw without a session.
        #[cfg(test)]
        let hud = hud
            || self.chat.for_shot
            || self.scoreboard.showing_shot()
            || self.quick_wheel.shown_without_a_game()
            || self.unlock_toast.pending() > 0
            || self.world_pings.forced;
        let hyperspace = self.local_prediction.hyperspace_shade();
        let flares = hyperspace.is_none()
            && !self.world_hidden
            && self.world_materials.has_flares()
            && self.geometry.environment.data.control[3] != 0.;
        let brushes = self.brush_view.active();
        let hosted = crate::host::active();
        if !hud
            && !hosted
            && !flares
            && !brushes
            && self.post_aa.is_none()
            && self.render_scale.is_none()
        {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK flares and overlays"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // cg_view.c:1647-1649 / RB_Hyperspace: replace the world,
                    // but retain the ordinary HUD pass over the grey flash.
                    load: hyperspace.map_or(wgpu::LoadOp::Load, |c| {
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: c,
                            g: c,
                            b: c,
                            a: 1.0,
                        })
                    }),
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
        if flares {
            self.world_materials.draw_flares(
                &mut pass,
                &crate::world_materials::FrameDraw {
                    camera: &self.camera_bind_group,
                    vertices: &self.geometry.vertex_buffer,
                    indices: &self.geometry.index_buffer,
                    instances: &self.actor_instance_buffer,
                    mover_ranges: &self.mover_instance_ranges,
                    source_cluster: cluster,
                    visibility,
                    entities: &[],
                },
                &self.depth.sample_bind_group,
            );
        }
        // In-world, so before FXAA and the unfiltered HUD layer.
        self.brush_view.draw(&mut pass, &self.camera_bind_group);
        self.ground_hud.draw(
            &mut pass,
            &self.depth.sample_bind_group,
            &self.text_bind_group,
        );
        drop(pass);
        if let Some(scale) = &self.render_scale {
            scale.draw(
                encoder,
                self.post_aa.as_ref().map_or(output, |aa| &aa.scene),
            );
        }
        if let Some(aa) = &self.post_aa {
            aa.draw_scene(encoder, output);
        }
        if !hud {
            // The hosting engine's 2D layer (its menus, HUD and console), in display space
            // as SJK's own 2D layer is, before the display ramp.
            if hosted {
                crate::host::draw_layer(
                    encoder,
                    self.post_aa
                        .as_ref()
                        .map_or(output_ui, |aa| aa.hud_target(output_ui)),
                    [self.configuration.width, self.configuration.height],
                );
            }
            if let Some(aa) = &self.post_aa {
                aa.draw_display(encoder, output);
            }
            return;
        }
        // The 2D layer draws display values through a UNORM view (`ui_target.rs`),
        // so it gets its own pass after the linear world and its resolve.
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK unfiltered HUD after FXAA"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self
                    .post_aa
                    .as_ref()
                    .map_or(output_ui, |aa| aa.hud_target(output_ui)),
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: self
                    .render_scale
                    .as_ref()
                    .map_or(&self.depth.view, |scale| &scale.hud_depth.view),
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.hud_pipeline);
        pass.set_bind_group(0, &self.hud_bind_group, &[]);
        match self.hud_scissors {
            None => pass.draw(0..3, 0..1),
            Some(rectangles) => {
                for [x, y, width, height] in rectangles.into_iter().flatten() {
                    pass.set_scissor_rect(x, y, width, height);
                    pass.draw(0..3, 0..1);
                }
                pass.set_scissor_rect(0, 0, self.configuration.width, self.configuration.height);
            }
        }
        // Under every 2D shape, so the card sits over the ring.
        self.card_outline.draw(
            &mut pass,
            [self.configuration.width, self.configuration.height],
        );
        self.menu_hud.draw(&mut pass);
        self.ui_shapes.draw(&mut pass);
        if let Some(mask) = &self.scope_mask {
            mask.draw(&mut pass);
        }
        // Menu and chat game fonts sit under the Inter console and HUD text,
        // as they did when every surface shared one buffer.
        self.game_fonts
            .draw(&mut pass, &self.text_pipeline, &self.sdf_text_pipeline);
        if text != 0 {
            pass.set_pipeline(&self.text_pipeline);
            pass.set_bind_group(0, &self.text_bind_group, &[]);
            pass.set_vertex_buffer(0, self.text_vertex_buffer.slice(..));
            pass.draw(0..text, 0..1);
        }
        if classic_text != 0
            && let Some(group) = &self.classic_text_bind_group
        {
            pass.set_pipeline(if self.classic_text_sdf {
                &self.sdf_text_pipeline
            } else {
                &self.text_pipeline
            });
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, self.classic_text_vertex_buffer.slice(..));
            pass.draw(0..classic_text, 0..1);
        }
        // The console's character set goes last so the console covers all text.
        self.game_fonts
            .draw_console(&mut pass, &self.text_pipeline, &self.sdf_text_pipeline);
        // The classic console's background, then its text, over everything else.
        self.console_layer.draw(
            &mut pass,
            crate::console_backdrop::TextDraw {
                pipeline: &self.text_pipeline,
                sdf_pipeline: &self.sdf_text_pipeline,
                inter: &self.text_bind_group,
                console: self.game_fonts.console_atlas(),
                families: self.game_fonts.sjk_atlases(),
            },
        );
        drop(pass);
        if hosted {
            crate::host::draw_layer(
                encoder,
                self.post_aa
                    .as_ref()
                    .map_or(output_ui, |aa| aa.hud_target(output_ui)),
                [self.configuration.width, self.configuration.height],
            );
        }

        if let Some(aa) = &self.post_aa {
            aa.draw_display(encoder, output);
        }
    }
}
