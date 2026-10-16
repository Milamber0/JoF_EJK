//! Capturing the reflection probes (`reflection_probes.rs`) through the ordinary scene
//! path, before the main view's light pass of a frame.
//!
//! A face is a 90° view rendered like a floor mirror: in live lighting the light pass
//! lights it into the upper-left corner of the light buffer (its camera squeezed into
//! that corner, as `floor_reflection_finish::raster_projection` does), then the colour
//! pass draws the sky, the opaque and the blended world into the probe's `size²` target
//! with the unsqueezed camera, whose pixels map onto the same light texels. In baked
//! lighting the faces show the lightmaps. Players, items and effects are not drawn, and
//! no surface reflects a probe while being captured (camera flag 16). The main view
//! then relights the whole light buffer as usual.
//!
//! After map load each frame captures one whole probe until all are done; when the
//! lighting changes (`reflection_probes::relit`), one face per frame refreshes them
//! while the old content stays in use.
use super::*;

/// Faces refreshed per frame after a lighting change.
const REFRESH_FACES: usize = 1;

/// Map clip space onto the upper-left `scale` of the target, like the floor mirrors'
/// packed raster, with separate x and y fractions.
fn corner(scale: [f32; 2]) -> Mat4 {
    let mut matrix = Mat4::from_scale(Vec3::new(scale[0], scale[1], 1.));
    matrix.w_axis.x = scale[0] - 1.;
    matrix.w_axis.y = 1. - scale[1];
    matrix
}

impl GpuState {
    /// Capture and filter this frame's share of the reflection probes. `shadows` says
    /// the frame's real-time lighting (cascades, light buffer) is ready.
    pub(crate) fn capture_reflection_probes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        shadows: bool,
    ) {
        let world = &self.world_materials;
        let Some(probes) = world.reflection_probes() else {
            return;
        };
        let scene = self.scene_size();
        let lit = world.realtime_materials_active();
        // Live lighting needs this frame's light: wait for it, and for a window large
        // enough to hold a face in the light buffer's corner.
        if lit
            && (!shadows || !world.light_buffer_fits(scene) || probes.size > scene[0].min(scene[1]))
        {
            return;
        }
        let plan = probes.state.borrow_mut().plan(
            probes.probes.len(),
            world.lighting_signature(),
            REFRESH_FACES,
        );
        let Some(plan) = plan else {
            return;
        };
        let probe = &probes.probes[plan.probe];
        let visibility = self.bsp.render().visibility();
        let leaf = self.bsp.leaf_at(probe.origin.to_array());
        let cluster = usize::try_from(self.bsp.leaves()[leaf].cluster).ok();
        let squeeze = corner([
            probes.size as f32 / scene[0] as f32,
            probes.size as f32 / scene[1] as f32,
        ]);
        for face in plan.faces.clone() {
            let clip = crate::world_materials::material_maps::reflections::gpu::face_matrix(
                probe.origin,
                face,
                self.far_plane,
            );
            let [(light_buffer, light_camera), (color_buffer, color_camera)] =
                &probes.cameras[face];
            let forward =
                crate::world_materials::material_maps::reflections::gpu::face_axes(face).0;
            for (buffer, matrix) in [(light_buffer, squeeze * clip), (color_buffer, clip)] {
                self.queue.write_buffer(
                    buffer,
                    0,
                    bytemuck::bytes_of(&crate::CameraUniform {
                        view_projection: matrix.to_cols_array_2d(),
                        camera_position: probe.origin.to_array(),
                        shader_time: 0.,
                        view_forward: forward.to_array(),
                        // A mirror-like view with its own light (9) that reflects no
                        // probe (16).
                        _padding: 25.,
                    }),
                );
            }
            // Each face is a whole scene: finish it on its own worker.
            self.frame_pacer.split.cut(&self.device, encoder);
            let _culling = world.view_culling.camera(clip);
            let input = crate::world_materials::FrameDraw {
                camera: light_camera,
                vertices: &self.geometry.vertex_buffer,
                indices: &self.geometry.index_buffer,
                instances: &self.actor_instance_buffer,
                mover_ranges: &self.mover_instance_ranges,
                source_cluster: cluster,
                visibility,
                entities: &[],
            };
            if lit {
                let region = [
                    0.,
                    0.,
                    probes.size as f32 / scene[0] as f32,
                    probes.size as f32 / scene[1] as f32,
                ];
                world.draw_light_buffer_region(
                    encoder,
                    &input,
                    None,
                    Some(region),
                    [region[2], region[3]],
                    Some(region),
                );
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("SJK reflection probe face"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &probes.color,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &probes.depth.view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let vertices = &self.geometry.vertex_buffer;
                let indices = &self.geometry.index_buffer;
                world.draw_sky(
                    &mut pass,
                    color_camera,
                    vertices,
                    indices,
                    cluster,
                    visibility,
                );
                world.draw_main_opaque(
                    &mut pass,
                    color_camera,
                    vertices,
                    indices,
                    &self.actor_instance_buffer,
                    &self.mover_instance_ranges,
                    cluster,
                    visibility,
                );
                world.draw_decals(
                    &mut pass,
                    color_camera,
                    vertices,
                    indices,
                    &self.actor_instance_buffer,
                    &self.mover_instance_ranges,
                    cluster,
                    visibility,
                    true,
                );
                world.draw_main_blended(
                    &mut pass,
                    color_camera,
                    vertices,
                    indices,
                    &self.actor_instance_buffer,
                    &self.mover_instance_ranges,
                    cluster,
                    visibility,
                );
            }
            probes.copy_face(encoder, face);
        }
        if plan.completes {
            probes.filter(encoder, &self.queue, plan.probe);
        }
        self.frame_pacer.split.cut(&self.device, encoder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_packs_clip_space_into_the_upper_left() {
        let packed = corner([0.25, 0.5]);
        let top_left = packed * glam::Vec4::new(-1., 1., 0.5, 1.);
        let bottom_right = packed * glam::Vec4::new(1., -1., 0.5, 1.);
        assert_eq!(top_left.truncate(), Vec3::new(-1., 1., 0.5));
        // x reaches a quarter of the width, y half the height, from the top.
        assert_eq!(bottom_right.truncate(), Vec3::new(-0.5, 0., 0.5));
    }
}
