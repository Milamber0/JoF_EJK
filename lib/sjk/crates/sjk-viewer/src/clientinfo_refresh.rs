//! In-match reaction to clientinfo changes — JKA's `CG_NewClientInfo`
//! (`codemp/cgame/cg_players.c`, called from `CG_ConfigStringModified` in
//! `cg_servercmds.c:894-896` whenever a `CS_PLAYERS` string changes).
//!
//! The server rewrites a player's clientinfo when their model changes
//! (`ClientUserinfoChanged`, at once) or their sabers change (`ClientSpawn`,
//! on the next spawn). The load-time actor meshes only know the roster of
//! the first gamestate, so a player who changes model kept the old mesh and
//! a saber nobody carried at load fell back to `single_1`. This module
//! receives dirty client indices from the session and rebuilds that player's
//! mesh and missing hilts on a change, appending
//! the geometry and materials to the shared buffers. The model itself is read on
//! a worker ([`player_loads`]); the slot keeps its mesh until the new one is in.

use super::*;
use crate::actor_load::{build_actor_mesh, client_saber_names};
use crate::player_assets::{GlaCache, load_player_appearance_with};
use crate::shared_geometry::relocate;
use crate::world_materials::pipeline_jobs::{Compiled, Jobs as PipelineJobs};
use sjk_client::legacy_client_appearance_forced;

#[path = "corpse_actors.rs"]
mod corpse_actors;
#[path = "clientinfo_forcing.rs"]
mod forcing;
#[path = "player_loads.rs"]
pub(crate) mod player_loads;

/// Per-world appearance loading caches used by dirty clientinfo notifications.
pub(crate) struct ClientInfoWatch {
    pub(crate) body_commands: Vec<sjk_client::BaseServerCommandEvent>,
    gla_cache: GlaCache,
    forced: bool,
    forced_model: String,
    overrides: [String; 3],
    local_identity: (i32, i32),
    /// Appearances whose load failed; players wearing one show Kyle, and the
    /// load is not retried in this world.
    pub(crate) failed: BTreeSet<Appearance>,
    /// Players' models being read on workers.
    loads: player_loads::Loads,
}

impl ClientInfoWatch {
    pub(crate) fn new() -> Self {
        Self {
            body_commands: Vec::new(),
            gla_cache: GlaCache::default(),
            forced: false,
            forced_model: String::new(),
            overrides: std::array::from_fn(|_| String::new()),
            local_identity: (-1, -1),
            failed: BTreeSet::new(),
            loads: player_loads::Loads::default(),
        }
    }
}

impl GpuState {
    /// Rebuild live player meshes only when the forced-model policy changes.
    pub(crate) fn refresh_forced_models(&mut self) {
        let forced = self
            .console
            .as_ref()
            .and_then(|c| c.bool_cvar("cg_forcemodel"))
            .unwrap_or(false);
        let model = self
            .console
            .as_ref()
            .and_then(|c| c.text_cvar("model").ok())
            .unwrap_or("kyle/default");
        let overrides = [
            "cg_forceenemymodel",
            "cg_forceallymodel",
            "cg_forceownsaber",
        ]
        .map(|name| {
            self.console
                .as_ref()
                .and_then(|c| c.text_cvar(name).ok())
                .unwrap_or("none")
        });
        let game = self
            .live_session
            .as_ref()
            .map(ClientSession::game_state)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::game_state)
            });
        let snapshot = self
            .live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            });
        let has_game = game.is_some();
        let local = game.map_or((-1, -1), |game| {
            (
                game.client_num,
                snapshot.map_or(0, |snap| i32::from(snap.player.team())),
            )
        });
        if self.clientinfo_watch.forced == forced
            && (!forced || self.clientinfo_watch.forced_model == model)
            && self
                .clientinfo_watch
                .overrides
                .iter()
                .map(String::as_str)
                .eq(overrides)
            && self.clientinfo_watch.local_identity == local
        {
            return;
        }
        self.clientinfo_watch.forced = forced;
        self.clientinfo_watch.forced_model.clear();
        self.clientinfo_watch.forced_model.push_str(model);
        for (stored, value) in self.clientinfo_watch.overrides.iter_mut().zip(overrides) {
            stored.clear();
            stored.push_str(value);
        }
        self.clientinfo_watch.local_identity = local;
        // Without a game state there is no player to rebuild; the first one
        // changes `local` and comes back here.
        if !has_game {
            return;
        }
        for client in 0..32 {
            if let Err(error) = self.apply_clientinfo(client) {
                log::progress(format_args!("force-model client {client}: {error}"));
            }
        }
    }

    /// Apply one dirty player slot using the existing actor and hilt caches.
    pub(crate) fn apply_clientinfo(&mut self, client: u16) -> Result<(), Box<dyn Error>> {
        let (appearance, saber_names) = {
            let game_state = self
                .live_session
                .as_ref()
                .map(ClientSession::game_state)
                .or_else(|| {
                    self.demo_session
                        .as_ref()
                        .map(demo_playback::Session::game_state)
                })
                .ok_or("no game state")?;
            let forced = self
                .clientinfo_watch
                .forced
                .then_some(self.clientinfo_watch.forced_model.as_str());
            let override_model = forcing::model(
                forced,
                &self.clientinfo_watch.overrides,
                client,
                self.clientinfo_watch.local_identity,
                forcing::team(game_state, client),
            );
            // A player muted on this PC is drawn as Kyle with the default saber
            // (`muted_players.rs`), whatever else forces a model.
            let muted = self.muted_players.contains(client);
            let model = if muted {
                Some(crate::muted_players::MODEL)
            } else {
                override_model.as_deref().or(forced)
            };
            let Some(appearance) = legacy_client_appearance_forced(game_state, client, model)
            else {
                self.clientinfo_watch.loads.cancel(client);
                return Ok(());
            };
            let mut names = client_saber_names(Some(game_state), client);
            if i32::from(client) == game_state.client_num {
                forcing::sabers(&mut names, &self.clientinfo_watch.overrides[2]);
            }
            if muted {
                crate::muted_players::sabers(&mut names);
                (crate::muted_players::appearance(&appearance), names)
            } else {
                (appearance, names)
            }
        };
        for name in saber_names.iter().flatten() {
            self.load_hilt(name)?;
        }
        let entity_id = EntityId::new(u64::from(client) + 1);
        let index = self
            .actor_meshes
            .iter()
            .position(|mesh| !mesh.corpse_pool && mesh.entity_id == Some(entity_id));
        if let Some(index) = index {
            self.actor_meshes[index].saber_names = saber_names.clone();
            // `c1`/`c2` may name another hat or cape (JoF EJK).
            self.actor_meshes[index].cosmetics.invalidate();
            if self.actor_meshes[index].appearance == appearance {
                self.clientinfo_watch.loads.cancel(client);
                return Ok(());
            }
        }
        let known_failed = self.clientinfo_watch.failed.contains(&appearance);
        self.clientinfo_watch.loads.request(player_loads::Request {
            client,
            appearance,
            entity_id,
            saber_names,
            known_failed,
        });
        Ok(())
    }

    /// Put `mesh`, just uploaded, on `client`'s slot.
    pub(crate) fn place_client_mesh(
        &mut self,
        client: u16,
        mesh: ActorMesh,
        appearance: &Appearance,
    ) {
        let entity_id = EntityId::new(u64::from(client) + 1);
        let index = self
            .actor_meshes
            .iter()
            .position(|mesh| !mesh.corpse_pool && mesh.entity_id == Some(entity_id));
        match index {
            Some(index) => self.actor_meshes[index] = mesh,
            None => {
                self.actor_meshes.push(mesh);
                self.actor_groups.push(Vec::with_capacity(4));
            }
        }
        let wears = if self.clientinfo_watch.failed.contains(appearance) {
            "Kyle in place of"
        } else {
            "now wears"
        };
        log::progress(format_args!(
            "client {client} {wears} {}/{}",
            appearance.model, appearance.variant
        ));
    }

    /// [`Self::build_live_actor`], or Kyle standing in for an appearance that
    /// cannot be loaded here.
    ///
    /// `CG_LoadClientInfo` (`codemp/cgame/cg_players.c`) registers
    /// `DEFAULT_MODEL` when a player's model fails, so a model this client lacks
    /// does not leave the slot showing its previous model. The stand-in carries
    /// the requested appearance, as at world load (`actor_load`), so an
    /// unchanged clientinfo does not rebuild it and a body copy of that player
    /// reuses it. A failed appearance is only tried once per world.
    pub(crate) fn build_live_actor_or_kyle(
        &mut self,
        appearance: &Appearance,
        entity_id: EntityId,
        saber_names: [Option<String>; 2],
    ) -> Result<ActorMesh, Box<dyn Error>> {
        if !self.clientinfo_watch.failed.contains(appearance) {
            match self.build_live_actor(appearance, entity_id, saber_names.clone()) {
                Ok(mesh) => return Ok(mesh),
                Err(error) => {
                    log::progress(format_args!(
                        "could not load {}/{}: {error}; using Kyle",
                        appearance.model, appearance.variant
                    ));
                    self.clientinfo_watch.failed.insert(appearance.clone());
                }
            }
        }
        let kyle = crate::actor_load::fallback_appearance();
        let vfs = self.vfs.clone().ok_or("no VFS")?;
        let preview = load_player_appearance_with(
            &vfs,
            &kyle.model,
            &kyle.variant,
            [0.0; 3],
            0.0,
            &mut self.clientinfo_watch.gla_cache,
        )?;
        self.upload_actor(preview, appearance, entity_id, saber_names)
    }

    /// Load `appearance` for `entity_id` and append it to the shared buffers.
    pub(crate) fn build_live_actor(
        &mut self,
        appearance: &Appearance,
        entity_id: EntityId,
        saber_names: [Option<String>; 2],
    ) -> Result<ActorMesh, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("no VFS")?;
        let preview = load_player_appearance_with(
            &vfs,
            &appearance.model,
            &appearance.variant,
            [0.0; 3],
            0.0,
            &mut self.clientinfo_watch.gla_cache,
        )?;
        self.upload_actor(preview, appearance, entity_id, saber_names)
    }

    /// Upload a captured preview into independent pose storage and vertex ranges.
    pub(crate) fn upload_actor(
        &mut self,
        preview: PlayerPreview,
        appearance: &Appearance,
        entity_id: EntityId,
        saber_names: [Option<String>; 2],
    ) -> Result<ActorMesh, Box<dyn Error>> {
        let mut scene = FlattenedScene::default();
        let mesh = build_actor_mesh(
            &mut scene,
            preview,
            Some(entity_id),
            false,
            appearance.clone(),
            saber_names,
        )?;
        self.upload_built_actor(mesh, scene, appearance)
    }

    /// Upload a mesh built from a preview ([`build_actor_mesh`]): its materials, its
    /// geometry into the shared buffers and its GPU skin; its new pipelines compile now.
    pub(crate) fn upload_built_actor(
        &mut self,
        mesh: ActorMesh,
        scene: FlattenedScene,
        appearance: &Appearance,
    ) -> Result<ActorMesh, Box<dyn Error>> {
        let (mesh, jobs) = self.upload_built_actor_deferred(mesh, scene, appearance)?;
        self.world_materials.install_pipelines(jobs.compile());
        Ok(mesh)
    }

    /// [`Self::upload_built_actor`], leaving its new pipelines to the returned jobs: the
    /// mesh must not be drawn before they are installed, or its first draw compiles them.
    pub(crate) fn upload_built_actor_deferred(
        &mut self,
        mut mesh: ActorMesh,
        scene: FlattenedScene,
        appearance: &Appearance,
    ) -> Result<(ActorMesh, PipelineJobs), Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("no VFS")?;
        let (material_base, jobs) = self.world_materials.append_entity_materials_deferred(
            &self.device,
            &self.queue,
            &vfs,
            &self.shaders,
            &scene.materials,
        )?;
        let placement =
            self.geometry
                .append(&self.device, &self.queue, &scene.vertices, &scene.indices)?;
        self.world_materials.bind_geometry(&self.geometry);
        relocate(
            &mut mesh.draws,
            &mut mesh.vertex_ranges,
            placement,
            material_base,
        );
        // Palette-skin the new mesh like the actors known at world load; on a topology
        // mismatch this one mesh keeps the CPU path.
        match self
            .geometry
            .append_actor_skin(&self.device, &self.queue, &mut mesh)
        {
            Ok(true) => self.world_materials.bind_geometry(&self.geometry),
            Ok(false) => {}
            Err(error) => log::progress(format_args!(
                "GPU skinning unavailable for {}: {error}",
                appearance.model
            )),
        }
        Ok((mesh, jobs))
    }

    /// Make sure the hilt catalog can answer for `name`, loading the `.sab`
    /// model into the shared buffers if it never was.
    pub(crate) fn load_hilt(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        let Some(catalog) = &mut self.saber_hilts else {
            return Ok(());
        };
        if catalog.is_resolved(name) {
            return Ok(());
        }
        let vfs = self.vfs.clone().ok_or("no VFS")?;
        let mut scene = FlattenedScene::default();
        if !catalog.load(&vfs, name, &mut scene, &mut self.object_meshes) {
            return Ok(());
        }
        // The catalog appended one object mesh; give it a group slot even if
        // the upload fails, so `mesh_index` stays valid (an empty draw list
        // is an invisible hilt, not a crash).
        self.object_groups.push(Vec::with_capacity(32));
        let materials = &scene.materials;
        let uploaded = self
            .world_materials
            .append_entity_materials(&self.device, &self.queue, &vfs, &self.shaders, materials)
            .and_then(|material_base| {
                let placement = self.geometry.append(
                    &self.device,
                    &self.queue,
                    &scene.vertices,
                    &scene.indices,
                )?;
                self.world_materials.bind_geometry(&self.geometry);
                Ok((placement, material_base))
            });
        let mesh = self.object_meshes.last_mut().ok_or("hilt mesh vanished")?;
        match uploaded {
            Ok((placement, material_base)) => {
                relocate(&mut mesh.draws, &mut [], placement, material_base);
                log::progress(format_args!("loaded saber hilt {name} mid-match"));
                Ok(())
            }
            Err(error) => {
                mesh.draws.clear();
                Err(error)
            }
        }
    }
}
