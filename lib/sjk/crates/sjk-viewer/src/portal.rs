//! The world the client is joining, prepared while it connects.
//!
//! Destination construction overlaps the connection: the menu's loading screen
//! shows its progress, and the actual prepared world is handed to the client
//! for live play. A verified gamestate supersedes speculative browser content;
//! there is no second live-world build when it matches.

#[path = "portal_render.rs"]
mod render;

use crate::gpu_context::Context;
use crate::session_transition::{WorldInstallPoll, WorldInstallTask, WorldLoadPoll, WorldLoadTask};
use crate::{GpuState, GpuWorldInput};
use std::path::Path;
use std::sync::Arc;

/// The preview of the map the client is joining, in whatever state its
/// load is in.
pub(crate) struct Destination {
    session_feed: Option<i32>,
    build: Option<(sjk_protocol::GameState, Option<sjk_protocol::Snapshot>)>,
    /// `maps/<map>.bsp` of the preview in flight or ready.
    map: Option<String>,
    load: Option<WorldLoadTask>,
    install: Option<WorldInstallTask>,
    world: Option<Box<GpuState>>,
    error: Option<String>,
}

impl Destination {
    pub(crate) const fn new() -> Self {
        Self {
            session_feed: None,
            build: None,
            map: None,
            load: None,
            install: None,
            world: None,
            error: None,
        }
    }

    /// Point the preview at `map` (`None` ends it). A different map than the
    /// one in flight restarts the load.
    pub(crate) fn aim(&mut self, map: Option<&str>, vfs: Option<&Arc<sjk_vfs::VirtualFileSystem>>) {
        if self.map.as_deref() == map {
            return;
        }
        crate::log::progress(format_args!(
            "portal: aimed at {map:?} (was {:?})",
            self.map.as_deref()
        ));
        *self = Self::new();
        if let (Some(map), Some(vfs)) = (map, vfs) {
            self.map = Some(map.to_owned());
            self.load = Some(WorldLoadTask::start(Arc::clone(vfs), map.to_owned()));
        }
    }

    /// Replace the pre-join preview when the actual session identifies its content.
    pub(crate) fn aim_session(
        &mut self,
        map: &str,
        root: &Path,
        game: &sjk_protocol::GameState,
        snapshot: Option<&sjk_protocol::Snapshot>,
    ) {
        if self.map.as_deref() == Some(map) && self.session_feed == Some(game.checksum_feed) {
            return;
        }
        *self = Self::new();
        self.map = Some(map.to_owned());
        self.session_feed = Some(game.checksum_feed);
        self.build = Some((game.clone(), snapshot.cloned()));
        match crate::assets::session_content::Selection::from_game(game) {
            Ok(selection) => {
                self.load = Some(WorldLoadTask::start_session(
                    root.to_owned(),
                    map.to_owned(),
                    selection,
                ))
            }
            Err(error) => self.fail(&error.to_string()),
        }
    }

    /// Advance the load: parse on one worker, then build the world against
    /// the shared context on another.
    pub(crate) fn poll(&mut self, context: &Arc<Context>, size: [u32; 2], game_data: &Path) {
        if let Some(task) = &self.load {
            match task.poll() {
                WorldLoadPoll::Pending => {}
                WorldLoadPoll::Failed(error) => self.fail(&error),
                WorldLoadPoll::Ready(loaded) => {
                    self.load = None;
                    // The joined session's snapshot places the camera.
                    let (camera_origin, camera_yaw) =
                        crate::assets::initial_camera(&loaded.bsp).unwrap_or(([0.0; 3], 0.0));
                    let bounds = loaded.bsp.render().models()[0].clone();
                    let input = GpuWorldInput {
                        scene: loaded.scene,
                        bsp: loaded.bsp,

                        vfs: loaded.vfs,
                        shaders: loaded.shaders,
                        world_minimums: bounds.minimums,
                        world_maximums: bounds.maximums,
                        camera_origin,
                        camera_yaw,
                        player_preview: None,
                        live_session: None,
                        demo_session: None,
                        build_game_state: self.build.as_ref().map(|(game, _)| game.clone()),
                        build_snapshot: self
                            .build
                            .as_ref()
                            .and_then(|(_, snapshot)| snapshot.clone()),
                        console: None,
                        client_menu: None,
                        game_data: game_data.to_path_buf(),
                        connect_timeline: None,
                        game_fonts: false,

                        completed_map_changes: 0,
                    };
                    self.install = Some(WorldInstallTask::start(
                        Arc::clone(context),
                        size,
                        input,
                        loaded.map_path,
                    ));
                }
            }
        }
        if let Some(task) = &mut self.install {
            match task.poll() {
                WorldInstallPoll::Pending => {}
                WorldInstallPoll::Failed(error) => self.fail(&error),
                WorldInstallPoll::Ready(world) => {
                    crate::log::progress(format_args!(
                        "portal: {} ready in {:.1} ms",
                        task.map_path,
                        task.started.elapsed().as_secs_f64() * 1_000.0
                    ));
                    self.install = None;
                    self.world = Some(Box::new(world));
                }
            }
        }
    }

    fn fail(&mut self, error: &str) {
        crate::log::progress(format_args!("portal: preview failed: {error}"));
        self.load = None;
        self.install = None;
        self.error = Some(error.to_owned());
    }

    /// Whether the world is fully prepared.
    pub(crate) fn ready(&self) -> bool {
        self.world.is_some()
    }

    /// How far the destination is: parsing, building or built.
    pub(crate) fn stage(&self) -> Option<crate::menu::classic::loading::WorldStage> {
        use crate::menu::classic::loading::WorldStage;
        if self.world.is_some() {
            Some(WorldStage::Ready)
        } else if self.install.is_some() {
            Some(WorldStage::Building)
        } else if self.load.is_some() {
            Some(WorldStage::Parsing)
        } else {
            None
        }
    }

    /// Whether the destination is built from the joined session's own
    /// gamestate (not the browser's guess at the map).
    pub(crate) fn for_session(&self) -> bool {
        self.session_feed.is_some()
    }
}

impl Destination {
    pub(crate) fn take_world(&mut self) -> Option<GpuState> {
        self.world.take().map(|world| *world)
    }

    pub(crate) fn session_error(&mut self) -> Option<String> {
        self.session_feed.and_then(|_| self.error.take())
    }

    pub(crate) fn map(&self) -> Option<&str> {
        self.map.as_deref()
    }
}
