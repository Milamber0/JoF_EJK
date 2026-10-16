//! Effect atlas entries follow server and console shader remaps.
//!
//! rd-vanilla `RB_BeginSurface` draws every surface of a remapped shader with
//! its target, so EFX particles, trails, beams and marks sampled from the atlas
//! take a copy of the target's original stages under their own name. This runs
//! when the world applies a remap state, never per frame.
use crate::*;

/// Atlas remap bookkeeping; a rebuilt atlas starts unremapped.
#[derive(Default)]
pub(crate) struct State {
    originals: HashMap<String, Vec<ParticleAtlasAnimation>>,
    plan: Vec<Retarget>,
    applied: Option<u64>,
    unloadable: BTreeSet<String>,
}

/// An atlas entry drawn with a source entry's original stages and clock offset.
#[derive(Debug, PartialEq)]
struct Retarget {
    key: String,
    source: String,
    offset: f32,
}

impl ParticleAtlas {
    /// Follow the world's remap `generation`, loading missing targets first.
    /// `resolve` maps a shader name to its visible shader and clock offset.
    pub(crate) fn refresh_remaps(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        generation: u64,
        resolve: impl Fn(&str) -> (String, f32),
    ) -> Result<(), Box<dyn Error>> {
        if self.remaps.applied == Some(generation) {
            return Ok(());
        }
        // A fresh atlas holds original stages and an empty plan.
        let plan = plan(self.animations.keys(), resolve);
        if plan == self.remaps.plan {
            self.remaps.applied = Some(generation);
            return Ok(());
        }
        let started = Instant::now();
        self.animations.extend(self.remaps.originals.drain());
        let missing = plan
            .iter()
            .map(|r| &r.source)
            .filter(|s| !self.animations.contains_key(*s) && !self.remaps.unloadable.contains(*s))
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut result = Ok(());
        if !missing.is_empty() {
            let mut required = self.animations.keys().cloned().collect::<BTreeSet<_>>();
            required.extend(missing.iter().cloned());
            let layout = particle_atlas::layout(device);
            match particle_atlas::create(device, queue, &layout, vfs, shaders, &required) {
                Ok(atlas) => {
                    let mut unloadable = std::mem::take(&mut self.remaps.unloadable);
                    unloadable.extend(
                        missing
                            .into_iter()
                            .filter(|s| !atlas.animations.contains_key(s)),
                    );
                    *self = atlas;
                    self.remaps.unloadable = unloadable;
                }
                Err(error) => result = Err(error),
            }
        }
        let changed = retarget(&mut self.animations, &mut self.remaps.originals, &plan);
        crate::log::progress(format_args!(
            "shader remaps: {changed} effect shaders remapped in {:.1}ms",
            started.elapsed().as_secs_f64() * 1000.
        ));
        self.remaps.plan = plan;
        self.remaps.applied = Some(generation);
        result
    }
}

impl ParticleAtlas {
    /// Swap every remapped entry with its original stages; a second call swaps them
    /// back. For a picture that must show stock effects (the Force page's preview),
    /// around building its draws. Cheap: it swaps the entries' lists in place.
    pub(crate) fn swap_remap_originals(&mut self) {
        for (key, original) in &mut self.remaps.originals {
            if let Some(current) = self.animations.get_mut(key) {
                std::mem::swap(current, original);
            }
        }
    }
}

/// Entries whose visible stages or clock differ from their own, sorted by key.
fn plan<'a>(
    keys: impl Iterator<Item = &'a String>,
    resolve: impl Fn(&str) -> (String, f32),
) -> Vec<Retarget> {
    let mut plan = keys
        .filter_map(|key| {
            let name = sjk_client::shader_name(key)?;
            let (target, offset) = resolve(&name);
            let source = if target == name {
                (offset != 0.).then(|| key.clone())?
            } else {
                target
            };
            Some(Retarget {
                key: key.clone(),
                source,
                offset,
            })
        })
        .collect::<Vec<_>>();
    plan.sort_unstable_by(|a, b| a.key.cmp(&b.key));
    plan
}

/// Copy each source's original stages over its entries; aliases do not chain.
/// A source the atlas could not load leaves its entry unchanged, as stock does.
fn retarget(
    animations: &mut HashMap<String, Vec<ParticleAtlasAnimation>>,
    originals: &mut HashMap<String, Vec<ParticleAtlasAnimation>>,
    plan: &[Retarget],
) -> usize {
    let swaps = plan
        .iter()
        .filter_map(|r| {
            let mut stages = animations.get(&r.source)?.clone();
            for stage in &mut stages {
                stage.time_offset = r.offset;
            }
            Some((r.key.clone(), stages))
        })
        .collect::<Vec<_>>();
    let changed = swaps.len();
    for (key, stages) in swaps {
        if let Some(original) = animations.insert(key.clone(), stages) {
            originals.insert(key, original);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(x: f32) -> Vec<ParticleAtlasAnimation> {
        vec![ParticleAtlasAnimation {
            frames: vec![[x; 4]],
            frequency: 0.,
            one_shot: false,
            blend: ParticleBlend::Add,
            rgb_wave: None,
            alpha_wave: None,
            tc_scale: [1.; 2],
            tc_scroll: [0.; 2],
            glow: false,
            tinted: true,
            time_offset: 0.,
        }]
    }

    fn entry(key: &str, source: &str, offset: f32) -> Retarget {
        Retarget {
            key: key.into(),
            source: source.into(),
            offset,
        }
    }

    fn resolver<'a>(
        server: &'a sjk_client::ShaderRemaps,
        mode: i64,
        local: &'a world_materials::map_remaps::MapRemaps,
    ) -> impl Fn(&str) -> (String, f32) + 'a {
        move |name| {
            let (target, offset) = world_materials::remap_target(local, server.table(mode), name);
            (target.to_owned(), offset)
        }
    }

    #[test]
    fn plan_follows_aliases_extensions_and_destination_clocks() {
        let mut server = sjk_client::ShaderRemaps::default();
        server.apply_config(b"gfx/A.tga=gfx/b:1.5@gfx/d=gfx/d:0@");
        let mut local = world_materials::map_remaps::MapRemaps::default();
        local.remap_local("gfx/c".into(), "gfx/a".into()).unwrap();
        let keys = ["gfx/a.tga", "gfx/b", "gfx/c", "gfx/d", "gfx/e"].map(str::to_owned);
        assert_eq!(
            plan(keys.iter(), resolver(&server, 1, &local)),
            [
                entry("gfx/a.tga", "gfx/b", 1.5),
                entry("gfx/b", "gfx/b", 1.5),
                entry("gfx/c", "gfx/a", 0.),
            ]
        );
    }

    #[test]
    fn retarget_copies_originals_and_restores_them() {
        let mut animations: HashMap<_, _> = [("a", 1.), ("b", 2.), ("c", 3.)]
            .map(|(k, x)| (k.to_owned(), stage(x)))
            .into();
        let mut originals = HashMap::new();
        let plan = [
            entry("a", "b", 0.5),
            entry("b", "c", 0.),
            entry("c", "x", 0.),
        ];
        assert_eq!(retarget(&mut animations, &mut originals, &plan), 2);
        let frame = |k: &str| (animations[k][0].frames[0][0], animations[k][0].time_offset);
        assert_eq!(
            [frame("a"), frame("b"), frame("c")],
            [(2., 0.5), (3., 0.), (3., 0.)]
        );
        animations.extend(originals.drain());
        let frame = |k: &str| (animations[k][0].frames[0][0], animations[k][0].time_offset);
        assert_eq!(
            [frame("a"), frame("b"), frame("c")],
            [(1., 0.), (2., 0.), (3., 0.)]
        );
    }

    #[test]
    fn self_remaps_and_disabled_remaps_plan_nothing() {
        let mut server = sjk_client::ShaderRemaps::default();
        server.apply_config(b"gfx/a=gfx/b:0@gfx/a=gfx/a:0@");
        let local = world_materials::map_remaps::MapRemaps::default();
        let keys = ["gfx/a", "gfx/b"].map(str::to_owned);
        assert!(plan(keys.iter(), resolver(&server, 1, &local)).is_empty());
        server.apply_config(b"gfx/a=gfx/b:2@");
        assert_eq!(plan(keys.iter(), resolver(&server, 1, &local)).len(), 2);
        assert!(plan(keys.iter(), resolver(&server, 0, &local)).is_empty());
    }
}
