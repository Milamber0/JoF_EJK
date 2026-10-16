//! CPU-only world preparation. One selected BSP supplies both scene and collision data.
use crate::assets::{self, session_content::Selection};
use sjk_scene::{MeshBuildOptions, StaticWorld};
use std::path::PathBuf;
use std::sync::{
    Arc,
    mpsc::{self, Receiver, TryRecvError},
};
use std::time::{Duration, Instant};

/// Complete preparation result for the existing GPU installer.
pub(crate) struct PreparedWorld {
    pub(crate) scene: StaticWorld,
    pub(crate) bsp: sjk_bsp::Bsp,
    pub(crate) vfs: Arc<sjk_vfs::VirtualFileSystem>,
    pub(crate) shaders: sjk_shader::ShaderCatalog,
    pub(crate) map_path: String,

    pub(super) elapsed: Duration,
}

/// A background CPU preparation, with no GPU or prediction work.
pub(crate) struct WorldLoadTask {
    receiver: Receiver<Result<PreparedWorld, String>>,
}

/// Nonblocking result of polling the preparation worker.
pub(crate) enum WorldLoadPoll {
    Pending,
    Ready(PreparedWorld),
    Failed(String),
}

impl WorldLoadTask {
    /// Preview an explicitly supplied world; never add ambient cached downloads.
    pub(crate) fn start(vfs: Arc<sjk_vfs::VirtualFileSystem>, map: String) -> Self {
        Self::spawn(map, Selection::default(), move || Ok(vfs))
    }

    /// Mount the installed game data afresh, as at start, for the menu map.
    pub(crate) fn start_installed(root: PathBuf, map: String) -> Self {
        Self::spawn(map, Selection::default(), move || {
            assets::mount_game_data(&root)
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
    }

    /// Rebuild session mounts from scratch, not from the previously displayed world.
    pub(crate) fn start_session(root: PathBuf, map: String, selection: Selection) -> Self {
        let request = selection.clone();
        Self::spawn(map, selection, move || {
            request
                .mount(&root)
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
    }

    fn spawn(
        map_path: String,
        selection: Selection,
        source: impl FnOnce() -> Result<Arc<sjk_vfs::VirtualFileSystem>, String> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("sjk-map-loader".into())
            .spawn(move || {
                let started = Instant::now();
                let result = (|| {
                    let vfs = source()?;
                    let bsp = selection
                        .load_bsp(&vfs, &map_path)
                        .map_err(|e| e.to_string())?;
                    let scene =
                        StaticWorld::build(&bsp, MeshBuildOptions::default().with_sky_surfaces())
                            .map_err(|e| e.to_string())?;
                    let shaders = assets::load_shaders(&vfs);
                    Ok(PreparedWorld {
                        scene,
                        bsp,
                        vfs,
                        shaders,
                        map_path,

                        elapsed: started.elapsed(),
                    })
                })();
                let _ = sender.send(result);
            })
            .expect("map loader thread creation failed");
        Self { receiver }
    }

    /// Poll without blocking the render/event thread.
    pub(crate) fn poll(&self) -> WorldLoadPoll {
        match self.receiver.try_recv() {
            Ok(Ok(world)) => WorldLoadPoll::Ready(world),
            Ok(Err(error)) => WorldLoadPoll::Failed(error),
            Err(TryRecvError::Empty) => WorldLoadPoll::Pending,
            Err(TryRecvError::Disconnected) => WorldLoadPoll::Failed("map loader stopped".into()),
        }
    }
}
