//! World-owned first-use rigid-model loading. Parse on a worker, install on the GPU owner.
use super::*;
use std::sync::mpsc::{Receiver, TryRecvError};

type Prepared = Result<(StaticModelMesh, FlattenedScene), String>;

#[derive(Default)]
pub(super) struct Demand {
    initialized: bool,
    /// Includes attempted failures so a broken community model is not retried every frame.
    attempted: BTreeSet<Appearance>,
    task: Option<(Appearance, Receiver<Prepared>)>,
}

fn rigid(appearance: &Appearance) -> bool {
    !appearance.model.starts_with(['*', '$', '@'])
        && !sjk_client::legacy_npc_body_model(&appearance.model)
}

impl GpuState {
    /// At most one CPU preparation and one GPU installation in flight per world.
    pub(crate) fn refresh_demand_models(&mut self) {
        if !self.config_string_refresh.models.initialized {
            self.config_string_refresh.models.attempted.extend(
                self.object_meshes
                    .iter()
                    .map(|mesh| mesh.appearance.clone()),
            );
            self.config_string_refresh.models.initialized = true;
        }
        let result =
            self.config_string_refresh.models.task.as_ref().and_then(
                |(_, receiver)| match receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => Some(Err("model loader stopped".to_owned())),
                },
            );
        if let Some(result) = result {
            let (appearance, _) = self.config_string_refresh.models.task.take().unwrap();
            let installed = result.and_then(|(mesh, scene)| {
                self.install_config_model(mesh, scene)
                    .map_err(|error| error.to_string())
            });
            if let Err(error) = installed {
                log::progress(format_args!("model {}: {error}", appearance.model));
            }
        }
        if self.config_string_refresh.models.task.is_some() {
            return;
        }
        let world = self
            .demo_session
            .as_ref()
            .map_or(&self.live_world, demo_playback::Session::world);
        // Borrow existing appearances; no owned names are made during an idle scan.
        let wanted = world
            .entities()
            .filter(|entity| !matches!(entity.kind, EntityKind::Actor | EntityKind::Corpse))
            .filter_map(|entity| entity.appearance())
            .find(|appearance| {
                rigid(appearance)
                    && !self
                        .config_string_refresh
                        .models
                        .attempted
                        .contains(*appearance)
            })
            .cloned()
            .or_else(|| {
                // A mover's second model (`mover_models`) is not a world appearance.
                let (game, snapshot) = match (&self.live_session, &self.demo_session) {
                    (Some(live), _) => (live.game_state(), live.latest_snapshot()),
                    (None, Some(demo)) => (demo.game_state(), demo.latest_snapshot()),
                    (None, None) => return None,
                };
                let attempted = &self.config_string_refresh.models.attempted;
                crate::mover_models::wanted(game, snapshot, |appearance| {
                    attempted.contains(appearance)
                })
            });
        let (Some(appearance), Some(vfs)) = (wanted, self.vfs.clone()) else {
            return;
        };
        self.config_string_refresh
            .models
            .attempted
            .insert(appearance.clone());
        // Configstring/effect changes may already have installed this model.
        if self
            .object_meshes
            .iter()
            .any(|mesh| mesh.appearance == appearance)
        {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let requested = appearance.clone();
        match std::thread::Builder::new()
            .name("sjk-model-demand".into())
            .spawn(move || {
                let mut scene = FlattenedScene::default();
                let result = object_meshes::load_one(&vfs, &requested, &mut scene)
                    .map(|mesh| (mesh, scene))
                    .map_err(|error| error.to_string());
                // Dropping the owning world discards old results, never installs them in another map.
                let _ = sender.send(result);
            }) {
            Ok(_) => self.config_string_refresh.models.task = Some((appearance, receiver)),
            Err(error) => log::progress(format_args!("could not start model loader: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_models_and_deformable_bodies_stay_with_their_owners() {
        for model in ["*1", "$swoop", "@single_1", "models/players/kyle/model.glm"] {
            assert!(!rigid(&Appearance {
                model: model.into(),
                variant: String::new()
            }));
        }
        assert!(rigid(&Appearance {
            model: "models/props/crate.md3".into(),
            variant: String::new()
        }));
    }

    #[test]
    #[ignore = "needs a GPU and JKA_GAME_DATA; retail assets are external"]
    fn a_new_world_model_is_prepared_and_installed_once_on_a_gpu() {
        crate::world_shot::on_big_stack(|| {
            let started = std::time::Instant::now();
            let (mut gpu, _profile) = crate::world_shot::open(
                "maps/mp/duel6.bsp",
                [640, 480],
                None,
                &[("r_hdr", "0"), ("cg_materialMaps", "0")],
            )
            .expect("a GPU adapter");
            let model = Appearance {
                model: "models/map_objects/imperial/antenna.md3".into(),
                variant: String::new(),
            };
            assert!(
                !gpu.object_meshes
                    .iter()
                    .any(|mesh| mesh.appearance == model)
            );
            let count = gpu.object_meshes.len();
            let entity = EntityId::new(900);
            gpu.live_world.upsert(
                entity,
                EntityKind::Other,
                sjk_runtime::MotionSample {
                    time_millis: 0,
                    transform: sjk_runtime::Transform::IDENTITY,
                },
            );
            gpu.live_world.set_appearance(entity, Some(model.clone()));
            let loading = std::time::Instant::now();
            let deadline = loading + std::time::Duration::from_secs(30);
            loop {
                gpu.refresh_demand_models();
                if gpu
                    .object_meshes
                    .iter()
                    .any(|mesh| mesh.appearance == model)
                {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "model did not arrive");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            for _ in 0..100 {
                gpu.refresh_demand_models();
            }
            assert_eq!(gpu.object_meshes.len(), count + 1);
            assert_eq!(gpu.object_groups.len(), gpu.object_meshes.len());
            assert!(matches!(
                gpu.render(&mut None),
                crate::gpu_context::FrameStatus::Rendered
            ));
            println!(
                "duel6 startup {:.1} ms; demanded model {:.1} ms; {} initial rigid models",
                (loading - started).as_secs_f64() * 1000.0,
                loading.elapsed().as_secs_f64() * 1000.0,
                count
            );
        });
    }
}
