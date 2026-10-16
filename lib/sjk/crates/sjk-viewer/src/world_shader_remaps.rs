//! Event-time replacement of compiled materials; draw-time indices stay stable.
use super::map_remaps::{MapRemaps, checked};
use super::*;
#[path = "world_remap_material.rs"]
mod material;

/// Original identity and binding retained when its visible shader changes.
pub(super) struct Source {
    pub key: ViewerMaterial,
    pub name: Option<String>,
    pub lightmap: wgpu::TextureView,
    pub fog: Vec<FogDraw>,
    /// The replacement drawn instead of the slot's own shader; `None` as loaded.
    pub applied: Option<Box<Applied>>,
}
/// A drawn replacement, and the slot's own compiled state kept aside for its restore.
pub(super) struct Applied {
    target: String,
    offset: f32,
    own: material::Look,
}
#[derive(Default)]
pub(super) struct State {
    pub sources: Vec<Source>,
    pub map: MapRemaps,
    pub applied: Option<(u64, u64, i64)>,
    pub generation: u64,
}

/// Visible shader and destination clock offset for an original shader name: the
/// target of its latest remap ([`MapRemaps::target`](crate::world_materials::map_remaps::MapRemaps::target))
/// and the server's clock for that target.
pub(crate) fn remap_target<'a>(
    map: &'a crate::world_materials::map_remaps::MapRemaps,
    server: Option<&'a sjk_client::ShaderRemapTable>,
    name: &'a str,
) -> (&'a str, f32) {
    let target = map.target(name, server);
    (target, server.map_or(0., |r| r.time_offset(target)))
}

impl Runtime {
    /// Apply only on a remap change or after registering new entity materials.
    pub(crate) fn refresh_remaps(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        remaps: &sjk_client::ShaderRemaps,
        mode: i64,
        visibility: Option<&Visibility>,
    ) -> Result<(), Box<dyn Error>> {
        let stamp = (remaps.stamp().0, remaps.stamp().1, mode);
        if self
            .remaps
            .applied
            .is_some_and(|previous| previous.0 != stamp.0)
        {
            self.remaps.map.clear_local();
        }
        let remaps = remaps.table(mode);
        if self.remaps.applied == Some(stamp) {
            return Ok(());
        }
        let started = Instant::now();
        let mut changed = 0;
        let mut images = ImageCache::new();
        for source in 0..self.remaps.sources.len() {
            let original = &self.remaps.sources[source];
            let Some(name) = original.name.clone() else {
                continue;
            };
            if !self.remaps.map.affects(&name, remaps) && original.applied.is_none() {
                continue;
            }
            let (target, offset) = remap_target(&self.remaps.map, remaps, &name);
            let applied = original
                .applied
                .as_deref()
                .map(|a| (a.target.as_str(), a.offset));
            let next = step(&name, applied, target, offset);
            let target = target.to_owned();
            let result = match next {
                Step::Keep => continue,
                Step::Restore => Ok(self.restore_remapped_material(source)),
                Step::Replace => self.replace_remapped_material(
                    source,
                    &target,
                    offset,
                    device,
                    queue,
                    vfs,
                    shaders,
                    &mut images,
                ),
            };
            match result {
                Ok(true) => changed += 1,
                Ok(false) => {}
                Err(error) => {
                    crate::log::progress(format_args!("shader remap {name} -> {target}: {error}"))
                }
            }
        }
        if changed != 0 {
            self.rebuild_remapped_fog(visibility);
            // SJK's dynamic glow keeps its own (material, stage) list: a remap can
            // change a slot's stages, so it is rebuilt with the draw orders.
            (
                self.opaque_order,
                self.decal_order,
                self.blended_order,
                self.glow_order,
            ) = draw_orders(&self.materials, &self.forge.pipeline_keys);
            self.prepare_camera_ranges();
            self.active.get_mut().invalidate();
            self.caster_runs.take();
            // Keep the map's sealed BSP shadow boundaries: dropping all hulls for
            // a recolor would reintroduce light leaking through unrelated walls.
            if let Some(table) = &mut self.stage_table {
                table.rebuild(device, &self.forge, &self.materials);
            }
            self.ssao = ssao::AmbientOcclusion::new(
                device,
                &self.forge.camera_layout,
                self.forge.format,
                &self.materials,
                &self.forge.pipeline_keys,
            );
            crate::log::progress(format_args!(
                "shader remaps: {changed} materials updated in {:.1}ms",
                started.elapsed().as_secs_f64() * 1000.
            ));
        }
        let sky_result =
            self.sky
                .refresh_remaps(device, queue, vfs, shaders, remaps, &self.remaps.map);
        self.remaps.applied = Some(stamp);
        self.remaps.generation = self.remaps.generation.wrapping_add(1);
        sky_result
    }

    /// Counts applied remap states, including local edits and late materials.
    pub(crate) fn remap_generation(&self) -> u64 {
        self.remaps.generation
    }
    /// This world's remap target for a shader name, shared by effects.
    pub(crate) fn remap_target<'a>(
        &'a self,
        server: Option<&'a sjk_client::ShaderRemapTable>,
        name: &'a str,
    ) -> (&'a str, f32) {
        remap_target(&self.remaps.map, server, name)
    }

    fn rebuild_remapped_fog(&mut self, visibility: Option<&Visibility>) {
        self.opaque_fog.clear();
        self.opaque_fog_by_cluster.clear();
        for (source, original) in self.remaps.sources.iter().enumerate() {
            let Some(material) = self.materials.get_mut(self.source_to_runtime[source]) else {
                continue;
            };
            material.fog_draws.clear();
            if let Some(pipeline) = material.fog_pipeline {
                material.fog_draws = original.fog.clone();
                for draw in &mut material.fog_draws {
                    draw.rebind(pipeline, (!material.view_bounded).then_some(source));
                }
                if material.sort <= SORT_OPAQUE && material.fog_pass == FogPass::Equal {
                    self.opaque_fog.append(&mut material.fog_draws);
                }
            }
        }
        fog_draws::compact(&mut self.opaque_fog);
        self.prepare_fog_visibility(visibility);
    }
}

impl Runtime {
    /// A local remap is transient and never sent to the server or saved in config.
    pub(crate) fn local_remap(
        &mut self,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        old: &str,
        new: &str,
    ) -> Result<(), String> {
        let (old, new) = checked(vfs, shaders, old, new)?;
        self.remaps.map.remap_local(old, new)?;
        self.remaps.applied = None;
        Ok(())
    }
    /// Map, server and local remaps for listRemaps, in application order.
    pub(crate) fn remap_listing(
        &self,
        server: Option<&sjk_client::ShaderRemapTable>,
    ) -> Vec<String> {
        self.remaps.map.listing(server)
    }
    /// Drop the console's remaps only, the map's kept: the world shots of notes apply
    /// each note's remaps in turn.
    #[cfg(test)]
    pub(crate) fn clear_console_remaps(&mut self) {
        self.remaps.map.clear_local();
        self.remaps.applied = None;
    }
    /// Map, server and local remaps with their sources, in application order, for
    /// world notes.
    pub(crate) fn remap_entries<'a>(
        &'a self,
        server: Option<&'a sjk_client::ShaderRemapTable>,
    ) -> Vec<super::map_remaps::RemapEntry<'a>> {
        self.remaps.map.entries(server)
    }
    /// Drop the map's and the console's remaps for `clearRemaps`, as EternalJK's
    /// renderer command resets every shader, worldspawn remaps included.
    pub(crate) fn clear_local_remaps(&mut self) {
        self.remaps.map.clear();
        self.remaps.applied = None;
    }
}

/// What a slot drawing `applied` needs in order to show `target` at `offset`.
#[derive(Debug, PartialEq)]
enum Step {
    Keep,
    Restore,
    Replace,
}

/// Load compiles every slot from its own name at shader time zero: only that state is
/// restored as kept. A native user retimed by its own shader's offset is recompiled.
fn step(name: &str, applied: Option<(&str, f32)>, target: &str, offset: f32) -> Step {
    if target == name && offset == 0. {
        if applied.is_some() {
            Step::Restore
        } else {
            Step::Keep
        }
    } else if applied == Some((target, offset)) {
        Step::Keep
    } else {
        Step::Replace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_load_state_is_restored_without_compiling() {
        assert_eq!(step("a", None, "a", 0.), Step::Keep);
        assert_eq!(step("a", Some(("b", 0.)), "a", 0.), Step::Restore);
        assert_eq!(step("a", Some(("a", 2.)), "a", 0.), Step::Restore);
        assert_eq!(step("a", None, "a", 2.), Step::Replace);
        assert_eq!(step("a", Some(("a", 2.)), "a", 3.), Step::Replace);
        assert_eq!(step("a", Some(("b", 2.)), "b", 2.), Step::Keep);
        assert_eq!(step("a", Some(("b", 2.)), "b", 3.), Step::Replace);
        assert_eq!(step("a", Some(("b", 0.)), "c", 0.), Step::Replace);
    }
}
