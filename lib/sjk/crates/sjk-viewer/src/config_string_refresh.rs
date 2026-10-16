//! `CG_ConfigStringModified` dispatch at the legacy viewer boundary.

use super::*;
use sjk_protocol::{ConfigStringDirty, MAX_CONFIGSTRINGS};

#[path = "model_demand.rs"]
mod model_demand;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Consumer {
    ServerInfo,
    SystemInfo,
    Music,
    Sound,
    Model(i16),
    Player(u16),
    Effect,
}

fn consumer(index: usize) -> Option<Consumer> {
    Some(match index {
        0 => Consumer::ServerInfo,
        1 => Consumer::SystemInfo,
        2 => Consumer::Music,
        32 | 37..=292 | 811..=1066 => Consumer::Sound,
        298..=809 => Consumer::Model((index - 298) as i16),
        1131..=1162 => Consumer::Player((index - 1131) as u16),
        1355..=1418 => Consumer::Effect,
        _ => return None,
    })
}

/// Bytes installed in this world, allowing a replacement gamestate's `mark_all`
/// to skip assets already built by the world-install worker.
pub(crate) struct ConfigStringRefresh {
    values: Vec<Vec<u8>>,
    initial_remaps: sjk_client::ShaderRemaps,
    models: model_demand::Demand,
}

impl ConfigStringRefresh {
    /// Seed from the exact gamestate used by the world builder.
    pub(crate) fn new(game_state: Option<&GameState>) -> Self {
        Self {
            models: Default::default(),
            initial_remaps: game_state
                .map_or_else(Default::default, sjk_client::ShaderRemaps::from_game_state),
            values: (0..MAX_CONFIGSTRINGS)
                .map(|index| {
                    game_state
                        .and_then(|game| game.config_string(index))
                        .unwrap_or_default()
                        .to_vec()
                })
                .collect(),
        }
    }

    /// `clearRemaps` without a session clears the remaps this world was built with.
    pub(crate) fn clear_remaps(&mut self) {
        self.initial_remaps.clear();
    }

    /// Remember changed bytes; untouched and already-built slots require no work.
    pub(crate) fn accept(&mut self, index: usize, bytes: &[u8]) -> bool {
        if self.values[index] == bytes {
            return false;
        }
        self.values[index].clear();
        self.values[index].extend_from_slice(bytes);
        true
    }
}

/// Load the initial effect graphs once while the world is being built.
pub(crate) fn preload_effects(
    vfs: &VirtualFileSystem,
    map: &LegacyMapEffects,
    missiles: &LegacyMissileEffects,
) -> EffectLibrary {
    let mut effects = EffectLibrary::default();
    effects.preload(vfs, effect_assets::STOCK_EFFECTS);
    effects.preload(vfs, map.effect_names().chain(missiles.effect_names()));
    let _ = effects.shader("gfx/effects/saberFlare");
    for shader in effect_assets::CODE_SHADERS {
        let _ = effects.shader(shader);
    }
    let _ = effects.code_primitive_definition();
    for icon in crate::pickups::simple::ICONS
        .iter()
        .chain(crate::pickups::simple::DISABLED_ICONS.iter())
        .filter(|s| !s.is_empty())
    {
        let _ = effects.shader(icon);
    }
    effects
}

impl GpuState {
    /// Drain once per frame. An idle session performs only fixed bitset work.
    pub(crate) fn refresh_config_strings(&mut self, audio: &mut Option<GameAudio>) {
        self.refresh_forced_models();
        if self.pending_map_reload
            || self.world_load_task.is_some()
            || self.world_install_task.is_some()
        {
            return;
        }
        self.refresh_muted_models(audio);
        self.poll_player_loads();
        let mut changes = ConfigStringDirty::default();
        if let Some(session) = &mut self.live_session {
            session.drain_config_string_changes(|index| changes.mark(index));
        } else if let Some(session) = &mut self.demo_session {
            session.drain_config_string_changes(|index| changes.mark(index));
        }
        changes.drain(|index| {
            let Some(target) = consumer(index) else {
                return;
            };
            let game = self
                .live_session
                .as_ref()
                .map(ClientSession::game_state)
                .or_else(|| {
                    self.demo_session
                        .as_ref()
                        .map(demo_playback::Session::game_state)
                });
            let Some(game) = game else { return };
            let bytes = game.config_string(index).unwrap_or_default();
            if !self.config_string_refresh.accept(index, bytes) {
                return;
            }
            match target {
                Consumer::SystemInfo => {
                    self.local_prediction.refresh_roll_rules(game);
                    if let Some(console) = &mut self.console {
                        console.refresh_movement_cvars(game);
                    }
                }
                Consumer::ServerInfo => {
                    self.local_prediction.refresh_roll_rules(game);
                    if let Some(console) = &mut self.console {
                        if let Some(session) = &self.live_session {
                            console.set_server_info(session);
                        } else {
                            console.set_demo_server_info(game);
                        }
                    }
                }
                Consumer::Music => {
                    if let (Some(audio), Some(vfs)) = (audio.as_mut(), &self.vfs) {
                        audio.refresh_music(game, vfs);
                    }
                }
                Consumer::Sound => {
                    if let (Some(audio), Some(vfs)) = (audio.as_mut(), &self.vfs) {
                        audio.refresh_sound_table(index, game, vfs);
                    }
                }
                Consumer::Model(_slot) => {
                    for name in self.missile_effects.refresh_vehicle(index, game) {
                        self.preload_config_effect(&name, audio);
                    }
                    let models: Vec<_> = self
                        .missile_effects
                        .vehicle_model_paths()
                        .map(str::to_owned)
                        .collect();
                    for model in models {
                        if let Err(error) = self.load_config_model(&Appearance {
                            model,
                            variant: String::new(),
                        }) {
                            log::progress(format_args!("vehicle projectile model: {error}"));
                        }
                    }
                }
                Consumer::Player(client) => {
                    if let Some(vfs) = &self.vfs {
                        self.local_prediction.refresh_sabers(game, vfs, client);
                    }
                    if let Err(error) = self.apply_clientinfo(client) {
                        log::progress(format_args!("cs {index}: clientinfo failed: {error}"));
                    }
                }
                Consumer::Effect => {
                    // `*` names are weather commands (`CG_ParseWeatherEffect`).
                    self.weather.refresh_server(game);
                    self.missile_effects.refresh_config_string(index, game);
                    let name = self
                        .map_effects
                        .refresh_config_string(index, game)
                        .map(str::to_owned);
                    if let Some(name) = name {
                        self.preload_config_effect(&name, audio);
                    }
                }
            }
        });
        self.refresh_npc_actors();
        self.refresh_demand_models();
        self.refresh_cosmetics();
        let mode = self.remap_mode();
        let remaps = self
            .live_session
            .as_ref()
            .map(ClientSession::shader_remaps)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::shader_remaps)
            })
            .unwrap_or(&self.config_string_refresh.initial_remaps);
        if let Some(vfs) = self.vfs.as_ref() {
            if let Err(error) = self.world_materials.refresh_remaps(
                &self.device,
                &self.queue,
                vfs,
                &self.shaders,
                remaps,
                mode,
                self.bsp.render().visibility(),
            ) {
                log::progress(format_args!("shader remap failed: {error}"));
            }
            let (world, table) = (&self.world_materials, remaps.table(mode));
            if let Err(error) = self.particle_atlas.refresh_remaps(
                &self.device,
                &self.queue,
                vfs,
                &self.shaders,
                world.remap_generation(),
                |name| {
                    let (target, offset) = world.remap_target(table, name);
                    (target.to_owned(), offset)
                },
            ) {
                log::progress(format_args!("effect shader remap failed: {error}"));
            }
        }
    }

    pub(crate) fn load_config_model(
        &mut self,
        appearance: &Appearance,
    ) -> Result<(), Box<dyn Error>> {
        if appearance.model.starts_with(['*', '$', '@'])
            || sjk_client::legacy_npc_body_model(&appearance.model)
            || self
                .object_meshes
                .iter()
                .any(|mesh| mesh.appearance == *appearance)
        {
            return Ok(());
        }
        let vfs = self.vfs.as_ref().ok_or("no VFS")?;
        let mut scene = FlattenedScene::default();
        let mesh = object_meshes::load_one(vfs, appearance, &mut scene)?;
        self.install_config_model(mesh, scene)
    }

    fn install_config_model(
        &mut self,
        mut mesh: StaticModelMesh,
        scene: FlattenedScene,
    ) -> Result<(), Box<dyn Error>> {
        if self
            .object_meshes
            .iter()
            .any(|loaded| loaded.appearance == mesh.appearance)
        {
            return Ok(());
        }
        let vfs = self.vfs.as_ref().ok_or("no VFS")?;
        let materials = self.world_materials.append_entity_materials(
            &self.device,
            &self.queue,
            vfs,
            &self.shaders,
            &scene.materials,
        )?;
        let placement =
            self.geometry
                .append(&self.device, &self.queue, &scene.vertices, &scene.indices)?;
        shared_geometry::relocate(&mut mesh.draws, &mut [], placement, materials);
        self.world_materials.bind_geometry(&self.geometry);
        self.emitter_model_catalog
            .insert(&mesh.appearance.model, self.object_meshes.len());
        log::progress(format_args!(
            "loaded demanded model {} mid-match",
            mesh.appearance.model
        ));
        self.object_meshes.push(mesh);
        self.object_groups.push(Vec::with_capacity(32));
        Ok(())
    }

    fn preload_config_effect(&mut self, name: &str, audio: &mut Option<GameAudio>) {
        let Some(vfs) = self.vfs.clone() else { return };
        self.effects.preload(&vfs, [name]);
        if let Err(error) = self.refresh_effect_atlas() {
            log::progress(format_args!("effect {name}: texture load failed: {error}"));
        }
        // Recursive EFX dependencies include rigid models and sound assets.
        for model in effect_assets::required_models(&vfs, std::iter::once(name)) {
            let appearance = Appearance {
                model,
                variant: String::new(),
            };
            if let Err(error) = self.load_config_model(&appearance) {
                log::progress(format_args!("effect {name}: model load failed: {error}"));
            }
        }
        if let Some(audio) = audio {
            let mut paths = Vec::new();
            self.effects.append_sound_paths(&mut paths);
            audio.register_effect_paths(&vfs, paths.iter().map(String::as_str));
        }
    }
}
