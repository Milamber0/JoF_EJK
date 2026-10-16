//! The pipelines of a model loaded during a match, compiled on a worker.
//!
//! A model whose stages need a blend, cull or depth combination the map had not used
//! brought new pipeline keys, and each key's variants (entity with and without depth,
//! glow, fog) were compiled on the render thread as the model was installed: 3 to 6 ms
//! each, 30 to 85 ms for some player models, felt as a hitch (`config-refresh` in the
//! `hitch:` lines). Now [`Runtime::pipeline_jobs`] lists what such a model's materials
//! lack, [`Jobs::compile`] builds it on any thread, and [`Runtime::install_pipelines`]
//! fills the slots before the model is first drawn. A slot left empty still compiles on
//! first draw, as before.

use super::*;

/// Where a compiled pipeline goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Slot {
    Entity { index: usize, depth: bool },
    Glow { index: usize },
    Fog { index: usize, entity: bool },
}

type Make = Box<dyn FnOnce() -> wgpu::RenderPipeline + Send>;

/// Pipelines to compile, made against one runtime's current programs.
pub(crate) struct Jobs {
    generation: u32,
    jobs: Vec<(Slot, Make)>,
}

/// What [`Jobs::compile`] built, for [`Runtime::install_pipelines`].
pub(crate) struct Compiled {
    generation: u32,
    pipelines: Vec<(Slot, wgpu::RenderPipeline)>,
}

impl Jobs {
    pub(super) fn push(
        &mut self,
        slot: Slot,
        make: impl FnOnce() -> wgpu::RenderPipeline + Send + 'static,
    ) {
        if !self.jobs.iter().any(|(known, _)| *known == slot) {
            self.jobs.push((slot, Box::new(make)));
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Compile every pipeline; any thread.
    pub(crate) fn compile(self) -> Compiled {
        Compiled {
            generation: self.generation,
            pipelines: self
                .jobs
                .into_iter()
                .map(|(slot, make)| (slot, make()))
                .collect(),
        }
    }
}

impl Runtime {
    /// The pipelines the stages of materials `materials` draw with that are not compiled
    /// yet: both entity variants of their keys from `keys.start` (the keys registered
    /// with them, live and forced-alpha ones included), the depth-tested glow variants of
    /// their glowing stages and their fog variants.
    pub(crate) fn pipeline_jobs(&self, keys: Range<usize>, materials: Range<usize>) -> Jobs {
        let mut jobs = Jobs {
            generation: self.pipeline_generation,
            jobs: Vec::new(),
        };
        for index in keys {
            for depth in [true, false] {
                self.entity_job(index, depth, &mut jobs);
            }
        }
        for material in &self.materials[materials] {
            for stage in material.stages.iter().filter(|stage| stage.glow) {
                self.glow_job(stage.pipeline, &mut jobs);
                self.glow_job(stage.live_pipeline, &mut jobs);
            }
            if let Some(index) = material.fog_pipeline {
                self.fog.jobs(index, &mut jobs);
            }
        }
        jobs
    }

    /// Fill the slots of `compiled`, unless the programs changed since its jobs were
    /// made. A slot compiled meanwhile (a draw got there first) keeps its pipeline.
    pub(crate) fn install_pipelines(&self, compiled: Compiled) {
        if compiled.generation != self.pipeline_generation {
            return;
        }
        for (slot, pipeline) in compiled.pipelines {
            match slot {
                Slot::Entity { index, depth } => {
                    let cells = if depth {
                        &self.entity_pipelines
                    } else {
                        &self.entity_no_depth_pipelines
                    };
                    if let Some(cell) = cells.get(index) {
                        let _ = cell.set(pipeline);
                    }
                }
                Slot::Glow { index } => self.install_glow(index, pipeline),
                Slot::Fog { index, entity } => self.fog.install(index, entity, pipeline),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_compiled_once_per_job_list() {
        let mut jobs = Jobs {
            generation: 0,
            jobs: Vec::new(),
        };
        let slot = Slot::Entity {
            index: 3,
            depth: true,
        };
        jobs.push(slot, || unreachable!("never compiled in this test"));
        jobs.push(slot, || unreachable!("never compiled in this test"));
        jobs.push(Slot::Glow { index: 3 }, || {
            unreachable!("never compiled in this test")
        });
        assert_eq!(jobs.len(), 2);
        assert!(!jobs.is_empty());
    }
}
