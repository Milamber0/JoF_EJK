//! Map-lifetime opaque solid boundaries omitted from the camera's visible mesh.
use wgpu::util::DeviceExt;

pub(crate) struct Hulls {
    vertices: wgpu::Buffer,
    count: u32,
}

impl super::super::Runtime {
    pub(crate) fn build_shadow_hulls(
        &mut self,
        device: &wgpu::Device,
        bsp: &sjk_bsp::Bsp,
        draws: &[crate::scene_flatten::DrawBatch],
    ) {
        let mut opaque = vec![false; bsp.render().surfaces().len()];
        for draw in draws {
            if !draw.world_surface {
                continue;
            }
            let Some(surface) = draw.surface_index else {
                continue;
            };
            let material = &self.materials[self.source_to_runtime[draw.material]];
            opaque[surface] = !material.blended
                && !material.flare
                && material.stages.first().is_some_and(|s| s.shadow_caster);
        }
        let hulls = sjk_scene::ShadowHulls::build(bsp, |surface| opaque[surface]);
        if hulls.positions.is_empty() {
            return;
        }
        let vertices: Vec<crate::GpuVertex> = hulls
            .positions
            .iter()
            .map(|&position| {
                let p = glam::Vec3::from_array(position);
                self.shadow_bounds[0] = self.shadow_bounds[0].min(p);
                self.shadow_bounds[1] = self.shadow_bounds[1].max(p);
                crate::GpuVertex {
                    position,
                    normal: [0.; 3],
                    color: [0.; 4],
                    texture_coordinates: [0.; 2],
                    lightmap_coordinates: [0.; 2],
                }
            })
            .collect();
        crate::log::progress(format_args!(
            "opaque shadow hulls: {} triangles",
            vertices.len() / 3
        ));
        self.shadow_hulls = Some(Hulls {
            count: vertices
                .len()
                .try_into()
                .expect("shadow hull vertex count fits u32"),
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK opaque brush shadow boundaries"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        });
    }
}

impl Hulls {
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, vertices: &wgpu::Buffer) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..self.count, 0..1);
        pass.set_vertex_buffer(0, vertices.slice(..));
    }
}
