//! Map-lifetime CPU assets and session inputs for background GPU installation.
use crate::*;

pub(crate) struct GpuWorldInput {
    pub(crate) scene: StaticWorld,
    pub(crate) bsp: Bsp,

    pub(crate) vfs: Arc<VirtualFileSystem>,
    pub(crate) shaders: ShaderCatalog,
    pub(crate) world_minimums: [f32; 3],
    pub(crate) world_maximums: [f32; 3],
    pub(crate) camera_origin: [f32; 3],
    pub(crate) camera_yaw: f32,
    pub(crate) player_preview: Option<PlayerPreview>,
    pub(crate) live_session: Option<ClientSession>,
    pub(crate) demo_session: Option<demo_playback::Session>,
    pub(crate) build_game_state: Option<GameState>,
    pub(crate) build_snapshot: Option<Snapshot>,
    pub(crate) console: Option<console::ViewerConsole>,
    pub(crate) client_menu: Option<menu::ClientMenu>,
    pub(crate) game_data: PathBuf,
    pub(crate) connect_timeline: Option<log::ConnectTimeline>,
    /// Load the optional menu and chat game fonts while installing the world.
    pub(crate) game_fonts: bool,

    pub(crate) completed_map_changes: u32,
}
