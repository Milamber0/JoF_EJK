//! Retarget authored sky images without replacing day/night resources.
use super::*;
impl Runtime {
    pub(crate) fn refresh_remaps(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        remaps: Option<&sjk_client::ShaderRemapTable>,
        map: &crate::world_materials::map_remaps::MapRemaps,
    ) -> Result<(), Box<dyn Error>> {
        for material in &mut self.materials {
            let Some(name) = sjk_client::shader_name(&material.name) else {
                continue;
            };
            if !map.affects(&name, remaps) && material.remapped.is_none() {
                continue;
            }
            let (target, _) = crate::world_materials::remap_target(map, remaps, &name);
            if material.remapped.as_deref() == Some(target) {
                continue;
            }
            let Some(sky) = shaders.get(target).and_then(|d| d.sky.as_ref()) else {
                continue;
            };
            let (bind_group, vertex_buffer, vertex_count, missing_faces) =
                sky.outer_box.as_deref().map_or_else(
                    || Ok((None, None, 0, Vec::new())),
                    |prefix| {
                        load_box(
                            device,
                            queue,
                            &gpu::texture_layout(device),
                            vfs,
                            shaders,
                            prefix,
                        )
                    },
                )?;
            material.bind_group = bind_group;
            material.vertex_buffer = vertex_buffer;
            material.vertex_count = vertex_count;
            material.missing_faces = missing_faces;
            material.remapped = Some(target.to_owned());
        }
        Ok(())
    }
}
