mod achievements;
mod actor_instance;
mod actor_load;
mod actor_mesh;
mod actor_pose;
mod actor_tint;
mod actor_world_submission;
mod am_color;
mod animation_timing;
mod app_launch;
mod assets;
mod audio;
mod audio_mute;
mod audio_output;
mod auto_switch;
mod avatar_command;
mod avatars;
mod bolt;
mod brush_view;
mod build_info;
mod camera;
mod camera_uniform;
mod capture;
mod card_outline;
mod cards;
mod cgame_options;
mod chat;
mod chat_gifs;
mod chat_mutes;
mod cinematic_roq;
mod client_guid;
mod client_state;
mod clientinfo_refresh;
mod debug_collectibles;
mod editions;
mod entity_view;
mod loot_odds;

mod shader_image;

mod clock_trace;
mod combat_effects;
mod config_import;
mod config_string_refresh;
mod connection;
mod connection_commands;
mod console;
mod console_backdrop;
mod console_overlay;
mod console_runtime;
mod cosmetics;
mod crash_report;
mod crosshair_scan;
mod cut_trace;
mod damage_feedback;
mod decal_marks;
mod decal_store;
mod decoded_image_cache;
mod demo_playback;
mod disintegration;
mod dismember;
mod donation_goal;
mod dust_motes;
mod dynamic_lights;
mod effect_assets;
mod effect_aux;
mod effect_blend;
mod effect_debug;
mod effect_electricity;
mod effect_emitter;
mod effect_envelope;
mod effect_geometry;
mod effect_geometry_gpu;
mod vehicle_assets;
mod vehicle_pose;

mod effect_library;
mod effect_runtime;
mod effect_shapes;
mod effect_submission;
mod effect_texcoords;
mod effect_wave;
mod entity_instance;
mod entity_lighting;
mod entity_materials;
mod first_person_view;
mod first_person_weapon;
mod fog_frame;
mod fog_volumes;
mod force_overlay_submission;
mod frame_pacing;
mod frame_queue;
mod frame_split;

mod charge_flash;
mod fake_noclip;
mod fps_help;
mod frame_target;
mod free_camera;
mod game_font;
mod game_menu_actions;
mod gameplay_fps;
mod ghost_replay;
mod glow_pass;
mod gpu_context;
mod gpu_phases;
mod gpu_texture;
mod graphics_quality;
mod graphics_reload;
mod ground_hud;
mod hitch_log;
mod holocron_popup;
mod holocrons;
mod hud;
mod hud_runtime;
mod impact_spawn;
mod impacts;
mod ingame_menu;
mod input;
mod input_health;
mod jof_tag;
mod keybind_editor;
mod lamp_lights;
mod launch;
mod load_profile;
mod local_actor_state;
mod local_prediction;
mod localization;
mod log;
mod log_request_card;
mod main_scene_pass;
mod medal_popup;
mod medals;
mod menu;
mod menu_backdrop;
mod menu_hud;
#[cfg(test)]
mod menu_snapshot;
mod menu_stage;
mod menu_widgets;
mod menu_world;
mod mimic;
mod missile_trails;
mod model_materials;
mod mods;
mod movement_collision;
mod mover_models;
mod movers;
mod muted_players;
mod muted_players_frame;
mod muzzle_effects;
mod muzzle_flash;
mod notice;
mod npc_refresh;
mod object_meshes;
mod oblique_clip;
mod particle_atlas;
mod particle_draw;
mod peek;
mod portal;
mod prediction_preview;
mod scene_views;
mod scope;
mod sender_card;
mod server_caps;
mod surface_tables;
mod trip_mine_lasers;
mod viewer_app;
use object_meshes::StaticModelMesh;
mod achievements_frame;
mod blade_skin_file;
mod bug_report;
mod electrify;
mod emote_icons;
mod emotes;
mod emotes_frame;
mod footprints;
mod gi_voxels;
mod holocrons_frame;
mod identity_command;
mod identity_frame;
mod illuminate;
mod inline_code;
mod installed_models;
mod jetpack;
mod live_session;
mod looks;
mod looks_frame;
mod net_timing;
mod particle_motion;
mod particle_physics;
mod particle_room;
mod particle_spawn;
mod particle_types;
mod pickups;
mod platform;
mod player_animation;
mod player_assets;
mod player_identity;
mod player_ids;
mod player_menu;
#[cfg(test)]
mod player_model_scan;
mod player_mutes;
mod player_report;
mod player_shadows;
mod player_skin;
mod pointer_input;
mod presentation_clock;
mod profile_card;
mod profile_hub;
mod projectiles;
mod quick_wheel;
mod rarity_fx;
mod remap_blocked_maps;
mod render_helpers;
mod rig_model;
mod runtime_settings;
mod saber;
mod saber_clash_flare;
mod saber_defs;
mod saber_ghosts;
mod saber_gpu;
mod saber_hilts;
mod saber_persona;
mod saber_rgb;
mod saber_skin_command;
mod saber_skin_options;
mod saber_skins;
mod saber_submission;
mod saber_trail;
mod saber_trail_gpu;
mod scene_flatten;
mod scoreboard;
mod screenshot;
mod server_browser;
mod server_commands;
mod server_passwords;
mod session_log;
mod session_transition;
mod settings;
mod settings_icons;
mod settings_usage;
mod shared_geometry;
mod sjk_chat_field;
mod sjk_chat_frame;
mod sjk_chat_look;
mod sjk_packs;
mod sky_stage;
mod snapshot_presentation;
mod static_models;
mod symbols;
mod text;
mod text_dialog;
mod text_entry;
mod text_select;
mod ui_renderer;
mod ui_scale;
mod ui_target;
mod unlock_toast;
mod unlockables;
mod update;
mod version_overlay;
mod wall_hold_pose;
mod weapon_select;
mod weapon_view;
mod weather;
mod wgsl_source;
mod whats_new_card;
mod window_icon;
mod world_materials;
mod world_notes;
mod world_ping;
mod world_ping_frame;
#[cfg(test)]
mod world_shot;
/// SJK's renderer inside another engine (JoF EJK's `rd-sjk`).
#[allow(unsafe_code)]
pub(crate) mod host;
mod world_stage;
use actor_instance::ActorInstance;
use actor_mesh::ActorMesh;
use animation_timing::animation_frame;
use camera_uniform::CameraUniform;
use effect_runtime::{EffectLibrary, Particle};
use entity_instance::EntityInstance;
use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
use glam::{Mat4, Quat, Vec3};
use gpu_context::FrameStatus;
use gpu_texture::{create_rgba8_texture, decode_image, load_shader_texture};
use ingame_menu::Page as GameMenuPage;
use local_prediction::LocalPrediction;
use localization::Localization;
use particle_types::ParticleBlend;
use player_animation::{GpuPlayerAnimation, PreviewVertexRange};
use player_assets::{PlayerPreview, load_player_preview};
use render_helpers::{angle_to_short, append_instance_group, mesh_center, texture_layout_entry};
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_client::{
    ClientSession, LegacyMapEffects, LegacyMissileEffects, LegacySaberClashFlare,
    LegacyWorldAdapter, ServerClock,
};
use sjk_effect::ComponentKind;
use sjk_model::{AnimationConfig, AnimationSequence, Gla, Glm, Md3, Md3Tag, Skin, SkinnedVertex};
use sjk_protocol::{GameState, InfoString, Snapshot};
use sjk_runtime::{Appearance, EntityId, EntityKind, World, WorldId};
use sjk_scene::{MeshBuildOptions, StaticWorld};
use sjk_shader::{ShaderCatalog, StageBlend, WaveForm};
use sjk_vfs::VirtualFileSystem;

use scene_flatten::{
    ActorDraw, DrawBatch, FlattenedScene, ViewerMaterial, append_actor_mesh, append_md3_mesh,
    append_static_glm_mesh, preview_gpu_vertex,
};
use shared_geometry::SharedGeometry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use text::{MAX_TEXT_VERTICES, TextVertex, UiFont, append_text};
use ui_renderer::ShapeRenderer;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};
#[cfg_attr(not(test), allow(dead_code))]
fn main() {
    log::init();
    crash_report::install();
    if let Err(error) = app_launch::run() {
        log::progress(format_args!("sjk: {error}"));
        crash_report::fatal_error(&error.to_string());
        std::process::exit(1);
    }
    crash_report::clean_exit();
    // Keep the last achievement counts, then withdraw the identity claim on the hub.
    achievements::sync(None, true);
    player_identity::shutdown();
    // After the settings were saved on the way out, start an installed update.
    update::restart_if_requested();
}
mod gpu_vertex;
use gpu_vertex::GpuVertex;

use audio::GameAudio;
use hud_runtime::HudUniform;

struct GpuState {
    context: Arc<gpu_context::Context>,
    /// The context the world loading now is built on, when it is newer than this
    /// world's: a graphics reload on a server ([`graphics_reload`]).
    next_context: Option<Arc<gpu_context::Context>>,
    /// The graphics reload's card and rebuild ([`graphics_reload`]).
    graphics_reload: graphics_reload::State,
    /// The low-FPS check of this world's map ([`fps_help`]).
    fps_help: fps_help::State,
    /// The low-FPS card (`help_fps`).
    fps_card: fps_help::card::Card,
    /// The SJK team's request for the player's logs ([`log_request_card`]).
    log_request_card: log_request_card::Card,
    /// What the update brought ([`whats_new_card`]).
    whats_new_card: whats_new_card::Card,
    window: Option<Arc<Window>>,
    post_aa: Option<frame_target::aa::Runtime>,
    render_scale: Option<frame_target::scale::Runtime>,
    screenshots: screenshot::Manager,
    /// The image frames render into without a window: only the off-screen
    /// world shots set it (`world_shot`); without one a windowless frame is
    /// skipped.
    headless_frame: Option<wgpu::Texture>,
    device: wgpu::Device,
    queue: frame_queue::FrameQueue,
    configuration: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    adapter_name: String,
    adapter_backend: String,
    size: PhysicalSize<u32>,
    world_materials: world_materials::Runtime,
    scene_views: scene_views::Runtime,
    entity_pipeline: wgpu::RenderPipeline,
    particle_pipelines: particle_draw::Pipelines,
    particle_atlas: ParticleAtlas,
    effect_geometry: effect_geometry_gpu::Runtime,
    hud_pipeline: wgpu::RenderPipeline,
    ui_shapes: ShapeRenderer,
    text_pipeline: wgpu::RenderPipeline,
    /// Text pipeline for the retail fonts' distance-field atlases.
    sdf_text_pipeline: wgpu::RenderPipeline,
    saber_gpu: saber_gpu::Runtime,
    dust_motes: dust_motes::Runtime,
    /// The quick wheel open while its key is held (`quick_wheel.rs`).
    quick_wheel: quick_wheel::QuickWheel,
    /// The map's rain, snow and mist (`weather.rs`).
    weather: weather::Runtime,
    geometry: SharedGeometry,
    entity_instance_buffer: wgpu::Buffer,
    actor_instance_buffer: wgpu::Buffer,
    actor_meshes: Vec<ActorMesh>,
    actor_workers: actor_pose::evaluation::Pool,
    object_meshes: Vec<StaticModelMesh>,
    emitter_model_catalog: effect_emitter::ModelCatalog,
    mover_catalog: movers::Catalog,
    pickup_catalog: pickups::Catalog,
    first_person_weapon: first_person_weapon::Catalog,
    first_person_view: first_person_view::Tracker,
    model_material_overrides: model_materials::Overrides,
    saber_hilts: Option<saber::HiltCatalog>,
    /// Who wears which blade skin (`saber_skins.rs`).
    saber_skins: saber_skins::SaberSkins,
    /// The blade skins the packs brought (`sjk_packs.rs`), as uploaded to `saber_gpu`.
    blade_skins: std::sync::Arc<saber_skins::LoadedSkins>,
    saber_states: saber_trail::StateSlab,
    saber_trail_segments: saber_trail::SegmentPool,
    speed_trails: actor_world_submission::speed_trail::Trails,
    /// Illuminate's holocron, the client's own toy (`toy_illuminate`).
    illuminate: illuminate::Holocron,
    /// The other players' holocrons, lit by their looks.
    illuminate_others: illuminate::Others,
    /// The Holocrons tab's 3D holocron ([`holocrons::stage`]).
    holocron_stage: holocrons::stage::Stage,
    /// The time its turn and bob are drawn at, instead of the clock's, for a world shot.
    #[cfg(test)]
    holocron_stage_seconds: Option<f32>,
    /// What each player wears that SJK draws (blade skin, Illuminate).
    looks: looks::Looks,
    trick_fades: sjk_client::LegacyTrickFades,
    projectiles: Vec<projectiles::Presented>,
    missile_effects: LegacyMissileEffects,
    dynamic_lights: dynamic_lights::PointLightList,
    effect_aux: effect_aux::Runtime,
    muzzle_effects: sjk_client::LegacyMuzzleEffects,
    force_overlays: sjk_client::LegacyForceOverlayTracker,
    /// The jetpack model and its jet bolts.
    jetpack: jetpack::Catalog,
    /// Which entities' jets have sounded their start.
    jetpack_sounds: jetpack::Sounds,
    force_overlays_last: usize,
    actor_groups: Vec<Vec<ActorInstance>>,
    object_groups: Vec<Vec<ActorInstance>>,
    static_models: static_models::StaticModels,
    pickup_override_instances: Vec<entity_materials::OverrideInstance>,
    mover_groups: Vec<Vec<ActorInstance>>,
    movers: Vec<movers::Presented>,
    pickups: Vec<pickups::Presented>,
    entity_instances: Vec<EntityInstance>,
    saber_instances: Vec<saber::Instance>,
    particle_groups: [Vec<EntityInstance>; effect_blend::PIPELINE_COUNT],
    actor_instances: Vec<ActorInstance>,
    actor_instance_ranges: Vec<Range<u32>>,
    object_instance_ranges: Vec<Range<u32>>,
    pickup_override_ranges: Vec<entity_materials::OverrideRange>,
    entity_draw_queue: entity_materials::Queue,
    mover_instance_ranges: Vec<Range<u32>>,
    /// Shared with the weather's cover survey thread.
    bsp: Arc<Bsp>,
    trace_scratch: TraceScratch,
    /// `worldnote`: the selection and the note being written.
    world_notes: world_notes::Notes,
    /// Map entity markers (`r_showEntities`).
    entity_view: entity_view::EntityView,
    /// `+ping`: the pings on show and their limits (`world_ping`).
    world_pings: world_ping::WorldPings,
    /// The note and bug report panel, and the Report a bug button (`text_dialog`).
    text_dialog: text_dialog::TextDialog,
    /// The new medal pop-up (`medal_popup`).
    medal_popup: medal_popup::MedalPopup,
    /// The holocron drop pop-up (`holocron_popup`).
    holocron_popup: holocron_popup::HolocronPopup,
    /// The unlock pop-up over play and the menus (`unlock_toast`): achievements, and
    /// medals and holocrons that arrive in a match.
    unlock_toast: unlock_toast::UnlockToast,
    /// A bug report is on its way to the hub (`bug_report`).
    bug_report_waiting: bool,
    /// The outcome last shown, so the next one is told apart.
    bug_report_serial: u64,
    entity_lighting: entity_lighting::EntityLighting,
    /// Live player model behind the Player screen.
    menu_stage: menu_stage::MenuStage,
    clientinfo_watch: clientinfo_refresh::ClientInfoWatch,
    config_string_refresh: config_string_refresh::ConfigStringRefresh,
    /// Parsed shader catalogue, kept for materials compiled after map load.
    /// Shared with the workers that load players' models (`player_loads.rs`).
    shaders: Arc<ShaderCatalog>,
    decal_surfaces: decal_marks::DecalSurfaces,
    player_shadows: player_shadows::State,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// The camera uniform's layout, for views with a camera of their own
    /// (the classic profile's model preview).
    camera_layout: wgpu::BindGroupLayout,
    hud_buffer: wgpu::Buffer,
    /// Where this frame's HUD program can draw (`hud_runtime::HudScissors`).
    hud_scissors: hud_runtime::HudScissors,
    hud_bind_group: wgpu::BindGroup,
    text_vertex_buffer: wgpu::Buffer,
    classic_text_vertex_buffer: wgpu::Buffer,
    text_bind_group: wgpu::BindGroup,
    text_layout: wgpu::BindGroupLayout,
    text_sampler: wgpu::Sampler,
    /// Glyphs beyond the code-page atlases, bound with every text atlas.
    wide_text: text::wide_texture::WideTexture,
    /// The retail `¬` logo spliced into the Inter atlas, kept for DPI rebuilds.
    logo_glyph: Option<text::LogoGlyph>,
    classic_text_bind_group: Option<wgpu::BindGroup>,
    /// The classic HUD atlas is a distance field (see `text::sdf::for_atlas`).
    classic_text_sdf: bool,
    ui_font: UiFont,
    classic_hud_font: Option<UiFont>,
    game_fonts: game_font::GameFonts,
    text_vertices: Vec<TextVertex>,
    classic_text_vertices: Vec<TextVertex>,
    hud: hud::HudOverlay,
    scoreboard: scoreboard::Scoreboard,
    chat: chat::ChatOverlay,
    /// Players muted on this PC who are on the server being played.
    muted_players: muted_players::MutedPlayers,
    game_menu: bool,
    in_game_menu: ingame_menu::InGameMenu,
    game_menu_page: GameMenuPage,
    game_menu_row: usize,
    auto_opened_team_menu: bool,
    depth: DepthTarget,
    camera_position: Vec3,
    /// The rendered view's origin (`cg.refdef.vieworg`), as last uploaded.
    view_origin: Vec3,
    camera_yaw: f32,
    camera_pitch: f32,
    mouse_look: pointer_input::MouseLook,
    field_of_view: f32,
    scope: scope::Zoom,
    scope_mask: Option<scope::Mask>,
    /// The player outlines' mask and rings, the card's and the teams' (`card_outline.rs`).
    card_outline: card_outline::Gpu,
    /// The classic console's background, bar and text, drawn over all other 2D.
    console_layer: console_backdrop::ConsoleLayer,
    /// The game's menu-file status HUD (`cg_hudStyle game`).
    menu_hud: menu_hud::MenuHud,
    ground_hud: ground_hud::GroundHud,
    /// Invisible brushes (`r_showBrushes`).
    brush_view: brush_view::BrushView,
    /// Display mode last applied to the window; `None` forces a reapply.
    applied_display: Option<settings::DisplayMode>,
    /// Exclusive fullscreen is paused while the window lacks focus, so Alt+Tab
    /// and Win+D reach the desktop (`runtime_settings.rs`).
    display_suspended: bool,
    /// When the window's display mode last changed.
    display_changed_at: Instant,
    /// The world is not drawn this frame: the classic menu style covers the
    /// screen with retail's opaque menu or loading screen.
    world_hidden: bool,
    applied_resolution: [u32; 2],
    /// Monitor refresh rate behind `com_maxfps -1`, re-read at most once a second.
    refresh_cap: std::cell::Cell<Option<(Instant, u32)>>,
    frame_pacer: frame_pacing::FramePacer,
    /// Storage for the upper right's "125fps" line (`console_overlay`).
    upper_right_rate: String,
    /// Always-on gameplay hitch lines (`hitch_log.rs`).
    hitches: hitch_log::Recorder,
    /// Optional per-pass GPU timing printed with the frame-budget report.
    gpu_phases: Option<gpu_phases::Profiler>,
    third_person_camera: camera::State,
    far_plane: f32,
    gameplay_input: input::GameplayInput,
    /// Windows Alt code being typed into a text field.
    alt_code: input::alt_code::AltCode,
    /// The SJK UI's Profile screen: which tab shows, switched here.
    profile_hub: profile_hub::Hub,
    /// The last frame's view, for placing 2D over the world outside the HUD pass (the
    /// Collection's nameplate over the stage model).
    frame_view: Option<hud::identification::Camera>,
    pointer_captured: bool,
    cursor_policy: pointer_input::CursorPolicy,
    cursor_position: [f32; 2],
    ui_epoch: Instant,
    last_frame: Instant,
    last_cluster: i32,
    player_animation: Option<GpuPlayerAnimation>,
    live_session: Option<ClientSession>,
    demo_session: Option<demo_playback::Session>,

    live_world: World,
    legacy_world_adapter: Option<LegacyWorldAdapter>,
    presentation_clock: presentation_clock::SnapshotPresentationClock,
    net_timing: net_timing::NetTiming,
    /// The 125 Hz user-command clock and the `cl_maxpackets` packet pace.
    command_schedule: sjk_client::command_rate::CommandSchedule,
    packet_pacer: sjk_client::command_rate::PacketPacer,
    /// Server-time estimate that stamps outgoing user commands.
    server_clock: ServerClock,
    clock_trace: clock_trace::ClockTrace,
    cut_trace: cut_trace::CutTrace,
    /// Armed trip mines' traced beam ends (`trip_mine_lasers`).
    trip_mine_beams: trip_mine_lasers::Beams,
    local_prediction: LocalPrediction,
    local_actor_state: local_actor_state::Tracker,
    third_person: bool,
    /// The player's camera choice (the camera key, a demo's camera); `third_person`
    /// is derived from it every frame by [`GpuState::update_zoom_view`] and is what
    /// the camera, HUD and scope read.
    third_person_choice: bool,
    /// A zoom forces first person this frame (`ZoomView::forces_first_person`).
    zoom_first_person: bool,
    /// Evidence cameras (spectate/look-at/orbit) leave the local actor at
    /// its entity transform instead of pinning it under the camera.
    detached_camera: bool,
    peek: peek::State,
    /// `ghost`: recording and replaying the player's own run.
    ghost: ghost_replay::Ghost,
    /// `mimic`: copying another player's moves live.
    mimic: mimic::Mimic,

    selected_weapon: Option<u8>,
    weapon_selected_at: Option<Instant>,
    pending_generic_command: u8,
    vfs: Option<Arc<VirtualFileSystem>>,
    pending_map_reload: bool,
    world_load_task: Option<session_transition::WorldLoadTask>,
    world_install_task: Option<session_transition::WorldInstallTask>,
    world_load_state: session_transition::LoadStateMachine,
    world_load_started: Option<Instant>,
    world_load_map: String,
    /// Whether `world_load_map` is in `cg_remapsBlockedMaps`.
    remap_blocked_maps: remap_blocked_maps::Cache,
    snapshot_observations: snapshot_presentation::Counters,
    obituaries: sjk_client::ObituaryTracker,
    /// What the player does in matches, for the achievements (`achievements.rs`).
    achievement_tracker: achievements::tracker::Tracker,
    lagometer: sjk_client::LagometerSamples,
    crosshair_scan: crosshair_scan::State,
    auto_switch: auto_switch::Tracker,
    previous_particle_events: HashMap<u16, u16>,
    particles: Vec<Particle>,
    /// Frees the old end of the effect pool for each frame's new particles.
    particle_room: particle_room::Room,
    pending_particle_effects: particle_physics::PendingEffects,
    impacts: impacts::Pool,
    /// `cg.lastFPFlashPoint`: the view gun's flash point of the last frame.
    last_first_person_flash: Option<[f32; 3]>,
    saber_clash_flare: LegacySaberClashFlare,
    damage_feedback: damage_feedback::Feedback,
    effects: EffectLibrary,
    /// Map music read on the install thread, handed to the audio at the cut.
    sound_prefetch: audio::SoundPrefetch,
    map_effects: LegacyMapEffects,
    localization: Localization,
    console: Option<console::ViewerConsole>,
    client_menu: Option<menu::ClientMenu>,
    join_task: Option<connection::JoinTask>,
    last_connect_address: Option<String>,
    game_data: PathBuf,
    connect_timeline: Option<log::ConnectTimeline>,
    load_event_gap: log::EventGap,
    transition_report_pending: bool,

    completed_map_changes: u32,
    live_map_installed: bool,

    quit_requested: bool,
    /// The boot world the standalone menu lives in (see `menu_world`).
    is_menu_world: bool,
    /// The map being joined, seen through the menu's gate.
    portal: portal::Destination,
    resident: session_transition::resident::State,
}
use assets::GpuWorldInput;

impl GpuState {
    async fn new(window: Arc<Window>, input: GpuWorldInput) -> Result<Self, Box<dyn Error>> {
        let size = window.inner_size();
        Self::new_for_target(Some(window), [size.width, size.height], input).await
    }

    async fn new_for_target(
        window: Option<Arc<Window>>,
        target_size: [u32; 2],
        input: GpuWorldInput,
    ) -> Result<Self, Box<dyn Error>> {
        let context = gpu_context::Context::new(window, input.console.as_ref()).await?;
        Self::new_with_context(context, target_size, input, None).await
    }

    async fn new_with_context(
        context: Arc<gpu_context::Context>,
        target_size: [u32; 2],
        input: GpuWorldInput,
        cancelled: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, Box<dyn Error>> {
        let input = assets::prepare_instances(input)?;
        let GpuWorldInput {
            scene,
            bsp,
            vfs,
            shaders,
            world_minimums,
            world_maximums,
            camera_origin,
            camera_yaw,
            player_preview,
            live_session,
            demo_session,
            build_game_state,
            build_snapshot,
            mut console,
            mut client_menu,
            game_data,
            mut connect_timeline,
            game_fonts: preload_game_fonts,
            completed_map_changes,
        } = input;
        // The Import page looks for other clients' configs beside SJK's.
        if let Some(console) = &mut console {
            console.set_game_data(&game_data);
        }
        let mut load_profile = load_profile::LoadProfile::start(cancelled);
        let session_active =
            live_session.is_some() || demo_session.is_some() || build_game_state.is_some();
        let active_game_state = live_session
            .as_ref()
            .map(ClientSession::game_state)
            .or_else(|| {
                demo_session
                    .as_ref()
                    .map(demo_playback::Session::game_state)
            })
            .or(build_game_state.as_ref());
        let active_snapshot = live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            })
            .or(build_snapshot.as_ref());
        let world_id = WorldId::new(1);
        let mut live_world = World::new(world_id);
        let mut legacy_world_adapter = active_game_state.map(|_| LegacyWorldAdapter::new(world_id));
        let latest_scene_server_time = active_snapshot.map_or(0, |snapshot| snapshot.server_time);
        let presentation_clock = presentation_clock::SnapshotPresentationClock::new(
            latest_scene_server_time,
            Instant::now(),
        );
        if let (Some(adapter), Some(snapshot), Some(game_state)) = (
            &mut legacy_world_adapter,
            active_snapshot,
            active_game_state,
        ) {
            adapter.apply_snapshot(snapshot, game_state, &mut live_world);
        }
        let server_clock = ServerClock::unanchored(Instant::now());
        let animation_config = player_preview
            .as_ref()
            .map(|preview| preview.config.as_ref());
        let local_prediction =
            LocalPrediction::new(active_snapshot, animation_config, active_game_state, &vfs);
        let config_string_refresh =
            config_string_refresh::ConfigStringRefresh::new(active_game_state);
        let map_effects = active_game_state
            .map_or_else(LegacyMapEffects::empty, LegacyMapEffects::from_game_state);
        let missile_effects = missile_trails::load(active_game_state, &vfs);
        let size = PhysicalSize::new(target_size[0].max(1), target_size[1].max(1));
        let localization = Localization::load(&vfs);
        menu::attach_world(&mut client_menu, Arc::clone(&vfs), &bsp, console.as_ref());
        if let Some(console) = &mut console {
            console.attach_script_vfs(Arc::clone(&vfs));
        }
        let gpu_phases = gpu_phases::Profiler::new(&context.device, &context.queue);
        let device = &context.device;
        let queue = &context.queue;
        let window = &context.window;
        let surface = &context.surface;
        let format = context.format;
        let vsync = console
            .as_ref()
            .and_then(|console| console.bool_cvar("r_vsync"))
            .unwrap_or(false);
        let present_mode = context.present_mode(vsync);
        let screenshot_setup = screenshot::setup(&context, console.as_ref(), &game_data);
        let configuration = wgpu::SurfaceConfiguration {
            usage: screenshot_setup.surface_usage,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: context.alpha_mode,
            view_formats: ui_target::surface_view_formats(format, context.ui_direct),
        };
        if console.is_some()
            && let Some(surface) = &surface
        {
            surface.configure(&device, &configuration);
        }

        let mut flattened = scene_flatten::prepare(&scene, &bsp, &shaders)?;
        load_profile.mark("flatten-bsp")?;
        let player_animation = player_preview
            .map(|preview| GpuPlayerAnimation::append(&mut flattened, preview))
            .transpose()?;
        let mut actor_meshes = if session_active {
            let active_world = demo_session
                .as_ref()
                .map_or(&live_world, demo_playback::Session::world);
            actor_load::load_actor_meshes(&vfs, active_game_state, active_world, &mut flattened)?
        } else {
            Vec::new()
        };
        load_profile.mark("player-models")?;
        let mover_catalog = movers::build_catalog(&bsp, &flattened);
        let decal_surfaces = decal_marks::DecalSurfaces::from_flattened(&flattened, &bsp);
        let mut object_meshes = object_meshes::load(
            &vfs,
            &bsp,
            demo_session
                .as_ref()
                .map_or(&live_world, demo_playback::Session::world),
            active_game_state,
            map_effects
                .effect_names()
                .chain(missile_effects.effect_names()),
            missile_effects.vehicle_model_paths(),
            &mut flattened,
        );
        let saber_hilts = if session_active {
            saber::load_hilts(
                &vfs,
                actor_meshes
                    .iter()
                    .flat_map(|mesh| mesh.saber_names.iter().flatten().map(String::as_str)),
                &mut flattened,
                &mut object_meshes,
            )
            .map_err(|error| eprintln!("could not load saber hilts: {error}"))
            .ok()
        } else {
            None
        };
        let pickup_catalog = pickups::Catalog::build(&object_meshes);
        let first_person_weapon = first_person_weapon::Catalog::build(
            &vfs,
            &object_meshes,
            actor_meshes
                .first()
                .map(|mesh| mesh.preview.config.as_ref()),
        );
        let emitter_model_catalog = effect_emitter::ModelCatalog::build(&object_meshes);
        let static_models = static_models::StaticModels::build(&bsp, &object_meshes);
        let model_material_overrides = model_materials::append_overrides(&mut flattened.materials);
        let jetpack = jetpack::Catalog::build(&vfs, &object_meshes);
        if let Some(timeline) = &mut connect_timeline {
            timeline.mark(log::TimelinePhase::Models);
        }
        load_profile.mark("object-models")?;
        let geometry = actor_pose::gpu_skinning::upload(&device, &mut actor_meshes, &flattened)?;
        load_profile.mark("geometry-upload")?;
        let world_minimums = Vec3::from_array(world_minimums);
        let world_maximums = Vec3::from_array(world_maximums);
        let far_plane = (world_maximums - world_minimums).length().max(4096.0) * 2.0;
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK camera"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_projection: Mat4::IDENTITY.to_cols_array_2d(),
                camera_position: camera_origin,
                view_forward: [camera_yaw.cos(), camera_yaw.sin(), 0.0],
                _padding: 0.0,
                shader_time: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let hud_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK HUD state"),
            contents: bytemuck::bytes_of(&HudUniform {
                crosshair_color: cgame_options::crosshair_color(None),
                health_ratio: 1.0,
                armor_ratio: 0.0,
                force_ratio: 1.0,
                menu_open: 0.0,
                inverse_width: 1.0 / configuration.width as f32,
                inverse_height: 1.0 / configuration.height as f32,
                menu_row: 0.0,
                menu_row_count: 1.0,
                crosshair: 1.0,
                hud_visible: 1.0,
                status_visible: 1.0,
                menu_kind: 0.0,
                menu_phase: 0.0,
                damage_x: 0.0,
                damage_y: 0.0,
                damage_alpha: 0.0,
                damage_strength: 0.0,
                health_bar: [0.0; 4],
                armor_bar: [0.0; 4],
                force_bar: [0.0; 4],
                _padding: [0.0; 3],
                crosshair_parameters: [0.0, 0.0, 640.0, 480.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let hud_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK HUD layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let hud_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK HUD bind group"),
            layout: &hud_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: hud_buffer.as_entire_binding(),
            }],
        });
        let logo_glyph = text::LogoGlyph::bundled()
            .inspect_err(|error| log::progress(format_args!("warning: no logo glyph: {error}")))
            .ok();
        let modern_atlas = text::load_modern(
            window.as_ref().map_or(1.0, |window| window.scale_factor()),
            logo_glyph.as_ref(),
        )?;
        let ui_font = modern_atlas.font;
        let font_view = gpu_texture::create_rgba8_texture_mipmapped(
            &device,
            &queue,
            "SJK UI font atlas",
            &modern_atlas.image,
            true,
            text::ATLAS_MIP_LEVELS,
        );
        // The font's atlas, its sampler and the wide-glyph and emoji atlases
        // (`text::wide_texture`).
        let text_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK text layout"),
            entries: &[
                texture_layout_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_layout_entry(2),
                texture_layout_entry(3),
            ],
        });
        let wide_text = text::wide_texture::WideTexture::new(&device);
        let text_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK text sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let text_bind_group = text::wide_texture::bind_group(
            &device,
            "SJK text bind group",
            &text_layout,
            &font_view,
            &text_sampler,
            &wide_text,
        );
        let classic_atlas = text::load_classic()
            .inspect_err(|error| {
                log::progress(format_args!("warning: no classic HUD font: {error}"))
            })
            .ok();
        let classic_text_sdf = classic_atlas
            .as_ref()
            .is_some_and(|atlas| atlas.distance_field);
        let (classic_hud_font, classic_text_bind_group) =
            classic_atlas.map_or((None, None), |atlas| {
                let view = gpu_texture::create_rgba8_texture_mipmapped(
                    &device,
                    &queue,
                    "SJK classic HUD font atlas",
                    &atlas.image,
                    true,
                    game_font::mip_levels(atlas.image.width(), atlas.image.height()),
                );
                let bind_group = text::wide_texture::bind_group(
                    &device,
                    "SJK classic HUD text bind group",
                    &text_layout,
                    &view,
                    &text_sampler,
                    &wide_text,
                );
                (Some(atlas.font), Some(bind_group))
            });
        let game_fonts = game_font::GameFonts::preload(
            preload_game_fonts || game_font::enabled(console.as_ref()),
            console.is_some(),
            &game_font::Device {
                device: &device,
                queue: &queue,
                layout: &text_layout,
                sampler: &text_sampler,
                wide: &wide_text,
            },
        );
        let (mut world_materials, resolved_world_stages) =
            world_materials::create_filtered_runtime(
                &device,
                &queue,
                &camera_layout,
                context.scene_format(),
                &bsp,
                &vfs,
                &shaders,
                &flattened.materials,
                &flattened.draws,
                (&flattened.vertices, &flattened.indices),
                &mover_catalog.meshes,
                context.filtering,
                context.sun_shadows.day.enabled,
                context.sun_shadows.enabled,
                context.material_maps,
            )?;
        world_materials.bind_geometry(&geometry);
        world_materials.install_gi(
            &device,
            &flattened,
            &mover_catalog.meshes,
            context.sun_shadows.day.enabled,
        );
        let scene_views = scene_views::Runtime::new(
            &device,
            &camera_layout,
            (context.scene_format(), [size.width, size.height]),
            &flattened,
            &bsp,
            &shaders,
            &|material| world_materials.floor_maps(material).cloned(),
        );
        load_profile.mark("world-materials")?;
        if let Some(timeline) = &mut connect_timeline {
            timeline.mark(log::TimelinePhase::Materials);
        }
        log::progress(format_args!(
            "resolved {resolved_world_stages} shared world/entity stages; uploaded {} lightmaps",
            bsp.render().lightmap_count()
        ));
        let particle_layout = particle_atlas::layout(&device);
        let required_effect_shaders = effect_assets::required_shaders(
            &vfs,
            map_effects
                .effect_names()
                .chain(missile_effects.effect_names()),
        );
        let particle_atlas = particle_atlas::create(
            &device,
            &queue,
            &particle_layout,
            &vfs,
            &shaders,
            &required_effect_shaders,
        )?;
        let effect_geometry = effect_geometry_gpu::Runtime::new(
            &device,
            &camera_layout,
            &particle_layout,
            frame_target::aa::effects::FORMAT,
        );
        load_profile.mark("effect-atlas")?;
        let (entity_pipeline, particle_pipelines) = particle_draw::create(
            device,
            &camera_layout,
            &particle_layout,
            context.scene_format(),
        );
        let saber_gpu = saber_gpu::Runtime::new(
            &device,
            &queue,
            &vfs,
            &shaders,
            &camera_layout,
            frame_target::aa::effects::FORMAT,
        )?;
        let dust_motes = dust_motes::Runtime::new(&device, &camera_layout, context.scene_format());
        let weather = weather::Runtime::new(active_game_state, &bsp);
        // Every 2D pipeline writes display values through a UNORM view (`ui_target.rs`).
        let ui_format = ui_target::format(format);
        let hud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK HUD shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("hud.wgsl").into()),
        });
        let hud_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK HUD pipeline layout"),
            bind_group_layouts: &[Some(&hud_layout)],
            immediate_size: 0,
        });
        let hud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK HUD pipeline"),
            layout: Some(&hud_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &hud_shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &hud_shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ui_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK text shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text.wgsl").into()),
        });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK text pipeline layout"),
            bind_group_layouts: &[Some(&text_layout)],
            immediate_size: 0,
        });
        // Inter's coverage atlas and the retail fonts' distance fields share one
        // layout and vertex format; only the fragment entry differs.
        let create_text_pipeline = |label, fragment_entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&text_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &text_shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(TextVertex::layout())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &text_shader,
                    entry_point: Some(fragment_entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: ui_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let text_pipeline = create_text_pipeline("SJK text pipeline", "fragment_main");
        let sdf_text_pipeline =
            create_text_pipeline("SJK distance-field text pipeline", "fragment_sdf");
        let text_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK dynamic text vertices"),
            size: (MAX_TEXT_VERTICES * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let classic_text_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK dynamic classic HUD text vertices"),
            size: (MAX_TEXT_VERTICES * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entity_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK dynamic entity instances"),
            size: (particle_types::INSTANCE_CAPACITY * std::mem::size_of::<EntityInstance>())
                as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let actor_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK actor instances"),
            size: (actor_instance::CAPACITY * std::mem::size_of::<ActorInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth = DepthTarget::new(&device, configuration.width, configuration.height);
        let (ui_shapes, hud) = hud::icons::install(&device, &queue, ui_format, &vfs, &shaders);
        let emojis = chat::emoji::Emojis::load(&vfs, &shaders, |id, rgba| {
            ui_shapes.upload_icon(queue, id, rgba)
        });
        let menu_hud = menu_hud::MenuHud::new(&device, ui_format);
        let console_layer = console_backdrop::ConsoleLayer::new(
            device,
            queue,
            ui_format,
            &vfs,
            &shaders,
            game_font::classic_console(console.as_ref()),
        );
        let card_outline = card_outline::Gpu::new(device, ui_format);
        let (scope_mask, ground_hud) = (
            scope::Mask::new(&device, &queue, ui_format, &vfs, &shaders),
            ground_hud::GroundHud::new(device, context.scene_format(), &text_layout),
        );
        load_profile.mark("pipelines-ui")?;

        let third_person = live_session.is_some()
            || build_game_state.is_some()
            || demo_session
                .as_ref()
                .is_some_and(|session| session.camera().third_person());
        let game_menu = active_snapshot.is_some_and(|snapshot| snapshot.player.is_spectator());
        let actor_groups = (0..actor_meshes.len())
            .map(|_| Vec::with_capacity(4))
            .collect();
        let object_groups = object_meshes::instance_groups(&object_meshes);
        let mover_groups = movers::instance_groups(&mover_catalog.meshes);
        let effects = config_string_refresh::preload_effects(&vfs, &map_effects, &missile_effects);
        load_profile.mark("effect-graphs")?;
        let sound_prefetch = audio::SoundPrefetch::read(&vfs, build_game_state.as_ref(), &bsp);
        load_profile.mark("sound-prefetch")?;
        if let Some(timeline) = &mut connect_timeline {
            timeline.mark(log::TimelinePhase::Sounds);
        }
        let resident = session_transition::resident::State::new(active_game_state, active_snapshot);
        let live_map_installed = live_session.is_some();
        let trace_scratch = bsp.trace_scratch();
        let entity_lighting = entity_lighting::EntityLighting::from_world(&bsp);
        Ok(Self {
            render_scale: None,
            post_aa: frame_target::aa::Runtime::for_context(
                &context,
                [configuration.width, configuration.height],
            ),
            window: context.window.clone(),
            device: context.device.clone(),
            queue: context.queue.clone(),
            present_modes: context.present_modes.clone(),
            adapter_name: context.adapter_name.clone(),
            adapter_backend: context.adapter_backend.clone(),
            context,
            next_context: None,
            graphics_reload: graphics_reload::State::default(),
            fps_help: fps_help::State::default(),
            fps_card: fps_help::card::Card::default(),
            log_request_card: log_request_card::Card::default(),
            whats_new_card: whats_new_card::Card::default(),

            screenshots: screenshot_setup.manager,
            headless_frame: None,
            configuration,
            size,
            world_materials,
            scene_views,
            entity_pipeline,
            particle_pipelines,
            particle_atlas,
            effect_geometry,
            hud_pipeline,
            ui_shapes,
            text_pipeline,
            sdf_text_pipeline,
            saber_gpu,
            dust_motes,
            quick_wheel: quick_wheel::QuickWheel::default(),
            weather,
            geometry,
            entity_instance_buffer,
            actor_instance_buffer,
            actor_meshes,
            actor_workers: actor_pose::evaluation::Pool::new(),
            object_meshes,
            emitter_model_catalog,
            mover_catalog,
            pickup_catalog,
            first_person_weapon,
            first_person_view: first_person_view::Tracker::default(),

            model_material_overrides,
            saber_hilts,
            saber_skins: Default::default(),
            blade_skins: Default::default(),
            saber_states: saber_trail::StateSlab::default(),
            saber_trail_segments: saber_trail::SegmentPool::default(),
            speed_trails: Default::default(),
            illuminate: Default::default(),
            illuminate_others: Default::default(),
            holocron_stage: Default::default(),
            #[cfg(test)]
            holocron_stage_seconds: None,
            looks: Default::default(),
            trick_fades: Default::default(),
            projectiles: Vec::with_capacity(sjk_protocol::MAX_LEGACY_ENTITIES),
            missile_effects,
            dynamic_lights: dynamic_lights::PointLightList::default(),
            effect_aux: effect_aux::Runtime::default(),
            muzzle_effects: sjk_client::LegacyMuzzleEffects::new(),
            force_overlays: sjk_client::LegacyForceOverlayTracker::default(),
            jetpack,
            jetpack_sounds: Default::default(),
            force_overlays_last: 0,
            actor_groups,
            object_groups,
            static_models,
            pickup_override_instances: Vec::with_capacity(force_overlay_submission::CAPACITY),
            mover_groups,
            movers: Vec::with_capacity(sjk_protocol::MAX_LEGACY_ENTITIES),
            pickups: Vec::with_capacity(sjk_protocol::MAX_LEGACY_ENTITIES),
            entity_instances: Vec::with_capacity(particle_types::INSTANCE_CAPACITY),
            saber_instances: Vec::with_capacity(saber::MAX_BLADE_INSTANCES),
            particle_groups: std::array::from_fn(|_| {
                Vec::with_capacity(particle_types::PARTICLE_POOL)
            }),
            actor_instances: Vec::with_capacity(actor_instance::CAPACITY),
            actor_instance_ranges: Vec::with_capacity(64),
            object_instance_ranges: Vec::with_capacity(256),
            pickup_override_ranges: Vec::with_capacity(sjk_protocol::MAX_LEGACY_ENTITIES),
            entity_draw_queue: entity_materials::Queue::new(),
            mover_instance_ranges: Vec::with_capacity(bsp.render().models().len()),
            crosshair_scan: crosshair_scan::State::new(&bsp),
            bsp: Arc::new(bsp),
            trace_scratch,
            world_notes: world_notes::Notes::default(),
            entity_view: Default::default(),
            world_pings: world_ping::WorldPings::default(),
            text_dialog: text_dialog::TextDialog::default(),
            medal_popup: medal_popup::MedalPopup::default(),
            holocron_popup: holocron_popup::HolocronPopup::default(),
            unlock_toast: unlock_toast::UnlockToast::default(),
            bug_report_waiting: false,
            bug_report_serial: 0,
            entity_lighting,
            menu_stage: menu_stage::MenuStage::default(),
            clientinfo_watch: clientinfo_refresh::ClientInfoWatch::new(),
            config_string_refresh,
            shaders: Arc::new(shaders),
            decal_surfaces,
            player_shadows: player_shadows::State::default(),
            camera_buffer,
            camera_bind_group,
            camera_layout: camera_layout.clone(),
            hud_buffer,
            hud_scissors: None,
            hud_bind_group,
            text_vertex_buffer,
            classic_text_vertex_buffer,
            text_bind_group,
            text_layout,
            text_sampler,
            wide_text,
            logo_glyph,
            classic_text_bind_group,
            classic_text_sdf,
            ui_font,
            classic_hud_font,
            game_fonts,
            text_vertices: Vec::with_capacity(MAX_TEXT_VERTICES),
            classic_text_vertices: Vec::with_capacity(4_096),
            hud,
            scoreboard: scoreboard::Scoreboard::new(),
            chat: chat::ChatOverlay::with_emojis(emojis),
            muted_players: muted_players::MutedPlayers::default(),
            game_menu,
            in_game_menu: ingame_menu::InGameMenu::new(),
            game_menu_page: if game_menu {
                GameMenuPage::Team
            } else {
                GameMenuPage::Main
            },
            game_menu_row: 0,
            auto_opened_team_menu: game_menu,
            depth,
            camera_position: Vec3::from_array(camera_origin),
            view_origin: Vec3::from_array(camera_origin),
            camera_yaw,
            camera_pitch: 0.0,
            mouse_look: pointer_input::MouseLook::default(),
            field_of_view: 90.0,
            scope: scope::Zoom::default(),
            scope_mask,
            card_outline,
            console_layer,
            menu_hud,
            ground_hud,
            brush_view: Default::default(),
            applied_display: Some(settings::DisplayMode::Windowed),
            display_suspended: false,
            display_changed_at: Instant::now(),
            world_hidden: false,
            applied_resolution: [size.width, size.height],
            refresh_cap: std::cell::Cell::new(None),
            frame_pacer: frame_pacing::FramePacer::new(),
            upper_right_rate: String::with_capacity(16),
            hitches: hitch_log::Recorder::default(),
            gpu_phases,
            third_person_camera: camera::State::default(),
            far_plane,
            gameplay_input: input::GameplayInput::default(),
            alt_code: input::alt_code::AltCode::default(),
            profile_hub: profile_hub::Hub::default(),
            frame_view: None,
            pointer_captured: false,
            cursor_policy: pointer_input::CursorPolicy::new(),
            cursor_position: [0.0; 2],
            ui_epoch: Instant::now(),
            last_frame: Instant::now(),
            last_cluster: i32::MIN,
            player_animation,
            live_session,
            demo_session,

            live_world,
            legacy_world_adapter,
            presentation_clock,
            net_timing: net_timing::NetTiming::default(),
            command_schedule: Default::default(),
            packet_pacer: Default::default(),
            server_clock,
            clock_trace: clock_trace::ClockTrace::new(),
            cut_trace: cut_trace::CutTrace::new(),
            trip_mine_beams: trip_mine_lasers::Beams::default(),
            local_prediction,
            local_actor_state: local_actor_state::Tracker::default(),
            third_person,
            third_person_choice: third_person,
            zoom_first_person: false,
            detached_camera: false,
            peek: peek::State::default(),
            ghost: ghost_replay::Ghost::default(),
            mimic: mimic::Mimic::default(),

            selected_weapon: None,
            weapon_selected_at: None,
            pending_generic_command: 0,
            effects,
            sound_prefetch,
            vfs: Some(vfs),
            pending_map_reload: false,
            world_load_task: None,
            world_install_task: None,
            world_load_state: session_transition::LoadStateMachine::new(),
            world_load_started: None,
            world_load_map: String::with_capacity(64),
            remap_blocked_maps: Default::default(),
            snapshot_observations: snapshot_presentation::Counters::default(),
            obituaries: sjk_client::ObituaryTracker::new(),
            achievement_tracker: achievements::tracker::Tracker::default(),
            lagometer: sjk_client::LagometerSamples::new(),
            auto_switch: auto_switch::Tracker::default(),
            previous_particle_events: HashMap::new(),
            particles: Vec::with_capacity(particle_types::PARTICLE_POOL),
            particle_room: particle_room::Room::default(),
            pending_particle_effects: particle_physics::PendingEffects::new(),
            impacts: impacts::Pool::new(),
            last_first_person_flash: None,
            saber_clash_flare: LegacySaberClashFlare::default(),
            damage_feedback: damage_feedback::Feedback::default(),
            map_effects,
            localization,
            console,
            client_menu,
            join_task: None,
            last_connect_address: None,
            game_data,
            connect_timeline,
            load_event_gap: log::EventGap::default(),
            transition_report_pending: false,

            completed_map_changes,
            live_map_installed,

            quit_requested: false,
            is_menu_world: false,
            portal: portal::Destination::new(),
            resident,
        }
        .with_render_scale()
        .with_sun_shadows())
    }

    fn rebuild_modern_text(&mut self, dpi_scale: f64) -> Result<(), Box<dyn Error>> {
        let atlas = text::load_modern(dpi_scale, self.logo_glyph.as_ref())?;
        let view = gpu_texture::create_rgba8_texture_mipmapped(
            &self.device,
            &self.queue,
            "SJK Inter UI font atlas",
            &atlas.image,
            true,
            text::ATLAS_MIP_LEVELS,
        );
        self.text_bind_group = text::wide_texture::bind_group(
            &self.device,
            "SJK text bind group",
            &self.text_layout,
            &view,
            &self.text_sampler,
            &self.wide_text,
        );
        self.ui_font = atlas.font;
        Ok(())
    }

    fn render_inner(
        &mut self,
        game_audio: &mut Option<GameAudio>,
        timing: &mut frame_pacing::budget::Timer,
    ) -> FrameStatus {
        use frame_pacing::budget::Phase;
        timing.mark(Phase::Shell);
        self.prepare_timed_frame(game_audio);
        // Before anything spawns this frame: trail puffs and impacts always fit.
        self.particle_room.make_room(
            &mut self.particles,
            Instant::now(),
            particle_room::take_refused(),
        );
        let now = Instant::now();
        let delta_seconds = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        let visual_now = now;
        self.local_prediction
            .begin_frame(visual_now.duration_since(self.ui_epoch).as_millis() as i32);
        self.update_live_session(game_audio, visual_now, timing);
        self.update_identity();
        timing.mark(Phase::Preview);
        self.update_demo_playback(game_audio, visual_now);
        // Hosted (`rd-sjk`): the engine's refdef clock drives shader time and effects.
        let host_view = crate::host::view();
        let presentation_time = match host_view {
            Some(view) => i64::from(view.time_ms),
            None => self.demo_session.as_ref().map_or_else(
                || self.presentation_clock.sample(Instant::now()),
                demo_playback::Session::current_server_time,
            ),
        };
        prediction_preview::tick(self, game_audio, Instant::now(), presentation_time as i32);
        timing.mark(Phase::Config);
        self.refresh_config_strings(game_audio);
        timing.mark(Phase::View);
        menu_backdrop::drive(self, visual_now);
        // The backdrop keeps the view while a joined map loads behind it.
        let backdrop_view = menu_backdrop::standalone_menu_visible(self);
        self.world_hidden = menu_backdrop::classic_hides_world(self);
        let world_hidden = self.world_hidden;
        if let Some(menu) = &mut self.client_menu {
            menu.set_world_hidden(world_hidden);
        }
        self.update_menu_stage(visual_now);
        self.prepare_eye_adaptation(delta_seconds, backdrop_view);
        let local_view = self
            .demo_session
            .as_ref()
            .is_none_or(|session| !matches!(session.camera(), demo_playback::Camera::Spectate(_)));
        self.update_zoom_view(presentation_time as i32);
        let intermission_view = self
            .live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            })
            .and_then(|snapshot| sjk_client::IntermissionView::from_player_state(&snapshot.player))
            .filter(|_| !backdrop_view);
        let forward = Vec3::new(
            self.camera_yaw.cos() * self.camera_pitch.cos(),
            self.camera_yaw.sin() * self.camera_pitch.cos(),
            self.camera_pitch.sin(),
        );
        // Quake's `AngleVectors` right axis (+Y is the listener's left).
        let right = Vec3::new(self.camera_yaw.sin(), -self.camera_yaw.cos(), 0.0);
        self.finish_audio_frame(game_audio, forward, right);
        self.advance_resident_movement(delta_seconds);
        net_timing::frame(self, presentation_time);
        clock_trace::tick(self, presentation_time, visual_now);
        let first_person = (!backdrop_view)
            .then(|| first_person_view::camera(self, presentation_time as i32))
            .flatten();
        // cg_view.c:1597-1608: the decaying prediction error shifts only the
        // view origin; the model root keeps the predicted origin.
        let error_offset = self.local_prediction.view_offset();
        let mut view_up = Vec3::Z;
        let peek_view = (!backdrop_view && intermission_view.is_none())
            .then(|| peek::camera(self, presentation_time))
            .flatten();
        let detached_before_peek = self.detached_camera;
        if self.live_session.is_some() && peek_view.is_some() {
            self.detached_camera = true;
        }
        let (branch, (view_position, view_target)) = if let Some(view) = host_view {
            // The hosting engine's own view (`refdef_t`), exactly as its cgame set it.
            self.third_person_camera = camera::State::default();
            view_up = view.up;
            ("host", (view.origin, view.origin + view.forward * 256.0))
        } else if let Some(view) = intermission_view {
            self.third_person_camera = camera::State::default();
            let pitch = -view.angles[0].to_radians();
            let yaw = view.angles[1].to_radians();
            let direction = Vec3::new(
                yaw.cos() * pitch.cos(),
                yaw.sin() * pitch.cos(),
                pitch.sin(),
            );
            let origin = Vec3::from_array(view.origin);
            ("intermission", (origin, origin + direction * 256.0))
        } else if let Some(view) = peek_view {
            self.third_person_camera = camera::State::default();
            ("peek", view)
        } else if (self.live_session.is_some() || self.demo_session.is_some())
            && self.third_person
            && !self.detached_camera
            && !backdrop_view
        {
            (
                "third-person",
                camera::damped_third_person(self, error_offset, delta_seconds, presentation_time),
            )
        } else if let Some(camera) = first_person {
            self.third_person_camera = camera::State::default();
            let (position, target, up) = camera.look();
            view_up = up;
            ("first-person", (position, target))
        } else {
            self.third_person_camera = camera::State::default();
            let free = (self.camera_position, self.camera_position + forward * 256.0);
            (if backdrop_view { "backdrop" } else { "free" }, free)
        };
        cut_trace::tick(self, branch, (view_position, view_target), visual_now);
        // `CG_DoCameraShake` measures from the rendered view (`cg.refdef.vieworg`),
        // so an effect's short-range shake reaches a first-person eye but not the
        // third-person camera behind the player.
        self.effect_aux.resolve_shakes(
            view_position,
            visual_now,
            local_view && cgame_options::screen_shake(self.console.as_ref()),
        );
        // The third-person camera already traced from the shifted origin.
        let error_offset = if branch == "third-person" || branch == "peek" {
            Vec3::ZERO
        } else {
            error_offset
        };
        let (view_position, view_target) = effect_aux::apply_camera_offset(
            view_position + error_offset,
            view_target + error_offset,
            self.effect_aux.camera_offset(visual_now),
        );
        self.update_fog_setting();
        let view = look_at_mat4(view_position, view_target, view_up);
        let aspect = self.configuration.width as f32 / self.configuration.height as f32;
        let fov = match host_view {
            Some(view) => view.fov_y,
            None => self.scope_fov(presentation_time as i32, game_audio),
        };
        let projection = perspective(fov.to_radians(), aspect, 2.0, self.far_plane);
        // rd-vanilla `tr_shade.cpp:367` derives tess.shaderTime from the
        // frame/refdef time; materials apply their own remap time offsets.
        self.upload_scene_camera(CameraUniform {
            view_projection: (projection * view).to_cols_array_2d(),
            camera_position: view_position.to_array(),
            view_forward: (view_target - view_position).normalize().to_array(),
            _padding: if self.scene_views.sky_only_fog {
                2.0
            } else {
                0.0
            },
            shader_time: presentation_time as f32 * 0.001,
        });
        timing.mark(Phase::Hud);
        let damage = self
            .damage_feedback
            .marker(presentation_time as i32, self.console.as_ref());
        self.update_auto_switch();
        let mut hud_visibility =
            ground_hud::frame(self, intermission_view.is_some(), presentation_time as i32);
        brush_view::frame(self);
        // The quick wheel's middle shows its names where the crosshair would be.
        if self.quick_wheel.is_open()
            || (self.scope.mode != 0 && cgame_options::scope_style(self.console.as_ref()) != 1)
        {
            hud_visibility.crosshair = false;
            hud_visibility.crosshair_names = false;
        }
        let viewport = [
            self.configuration.width as f32,
            self.configuration.height as f32,
        ];
        let text_scale = ui_scale::height_scale(viewport[1]).max(0.85);
        menu_hud::MenuHud::sync(self);
        hud_visibility.menu_hud = self.menu_hud.active();
        self.frame_view = Some(hud::identification::Camera {
            eye: view_position,
            target: view_target,
            up: view_up,
            fov,
            viewport,
        });
        hud_runtime::update(
            self,
            view_position,
            (view_target, view_up, fov),
            presentation_time as i32,
            intermission_view.is_some(),
        );
        // `CG_DrawRocketLocking`'s tick and lock, at the target (`CHAN_AUTO`).
        if let Some((sound, origin)) = self.hud.rocket_lock.take_sound()
            && let Some(audio) = game_audio.as_mut()
        {
            audio.play_on_channel(
                sound,
                1.0,
                origin,
                sjk_audio::SourceId(1022),
                sjk_audio::ChannelId(0),
            );
        }
        // A vehicle that hides its rider takes the player's status and weapon HUD.
        let hud_visibility = self.hud.vehicle.apply(hud_visibility);
        let hud_style = menu_hud::HudStyle::read(self.console.as_ref());
        // The radial look is laid out for the bundled font (Inter), whatever
        // `cg_classicHudFont` says: its numbers are aligned to that font's metrics.
        let classic_hud = !matches!(hud_style, menu_hud::HudStyle::Radial)
            && self
                .console
                .as_ref()
                .and_then(|console| console.bool_cvar("cg_classicHudFont"))
                .unwrap_or(false);
        let hud_font = if classic_hud {
            self.classic_hud_font.as_ref().unwrap_or(&self.ui_font)
        } else {
            &self.ui_font
        };
        self.hud.weapon_select.shown = self.sample_weapon_select(intermission_view.is_some());
        // JoF EJK's Force wheel (the retail icon bar) with the retail-looking HUDs.
        self.hud
            .set_force_wheel_bar(hud_style != menu_hud::HudStyle::Radial);
        let hud_layout = self.hud.layout(
            hud_font,
            match hud_style {
                menu_hud::HudStyle::Radial => hud::HudLook::Radial,
                menu_hud::HudStyle::Classic => hud::HudLook::Classic,
                menu_hud::HudStyle::Game => hud::HudLook::Game,
            },
            viewport,
            runtime_settings::hud_scale(self.console.as_ref()),
            hud_visibility,
            presentation_time.max(0) as u64,
        );
        if hud_style == menu_hud::HudStyle::Game {
            // The game-data HUD draws its meters as pictures, not one of the layouts.
            self.hud.nameplate.set_hud_colors(None, None, None);
        }
        // The shader's own bars hold one maximum; the layouts draw the overflow.
        let [health_ratio, armor_ratio, force_ratio] = self
            .hud
            .displayed_ratios()
            .map(|ratio| ratio.clamp(0.0, 1.0));
        let (menu_kind, visual_menu_row) = self
            .client_menu
            .as_ref()
            .filter(|menu| menu.is_visible())
            .map_or(
                (0.0, self.game_menu_row as f32),
                menu::ClientMenu::visual_selection,
            );
        let hud_uniform = HudUniform {
            crosshair_color: self.hud.targeting.color,
            health_ratio,
            armor_ratio,
            force_ratio,
            menu_open: menu_backdrop::shader_menu_state(self),
            inverse_width: 1.0 / self.configuration.width as f32,
            inverse_height: 1.0 / self.configuration.height as f32,
            menu_row: visual_menu_row,
            menu_row_count: self.game_menu_row_count() as f32,
            // The classic picture replaces the procedural cross when it loaded.
            crosshair: hud::options::crosshair_size(
                self.console.as_ref(),
                hud_visibility.crosshair && !self.hud.crosshair_picture_drawn,
            ) * self.hud.vehicle_crosshair_factor(),
            hud_visible: f32::from(hud_visibility.hud),
            status_visible: 0.0,
            menu_kind,
            menu_phase: now.duration_since(self.ui_epoch).as_secs_f32(),
            damage_x: damage.map_or(0.0, |sample| sample.x),
            damage_y: damage.map_or(0.0, |sample| sample.y),
            damage_alpha: damage.map_or(0.0, |sample| sample.alpha),
            damage_strength: damage.map_or(0.0, |sample| sample.strength),
            health_bar: hud_layout.health_bar,
            armor_bar: hud_layout.armor_bar,
            force_bar: hud_layout.force_bar,
            _padding: [0.0; 3],
            crosshair_parameters: self.hud.targeting.parameters(viewport),
        };
        self.queue
            .write_buffer(&self.hud_buffer, 0, bytemuck::bytes_of(&hud_uniform));
        self.hud_scissors =
            hud_uniform.scissors(self.configuration.width, self.configuration.height);
        let console_covers_frame = self.console_covers_frame();
        // A new medal, or a holocron drop after it, shows over the menus, which are not
        // drawn under it.
        let medal_shown = self.prepare_medal_popup(console_covers_frame);
        let holocron_shown = self.prepare_holocron_popup(console_covers_frame || medal_shown);
        // The graphics reload card waits for the others.
        let reload_shown = self.prepare_reload_card(
            console_covers_frame
                || medal_shown
                || holocron_shown
                || self.medal_popup.pending()
                || self.holocron_popup.pending(),
        );
        // The low-FPS card too.
        self.update_fps_help();
        let fps_shown = self.prepare_fps_card(
            console_covers_frame
                || medal_shown
                || holocron_shown
                || reload_shown
                || self.medal_popup.pending()
                || self.holocron_popup.pending(),
        );
        // And the SJK team's request for the player's logs.
        let logs_shown = self.prepare_log_request_card(
            console_covers_frame
                || medal_shown
                || holocron_shown
                || reload_shown
                || fps_shown
                || self.medal_popup.pending()
                || self.holocron_popup.pending(),
        );
        // And what the update brought, last.
        let news_shown = self.prepare_whats_new_card(
            console_covers_frame
                || medal_shown
                || holocron_shown
                || reload_shown
                || fps_shown
                || logs_shown
                || self.medal_popup.pending()
                || self.holocron_popup.pending(),
        );
        let medal_popup =
            medal_shown || holocron_shown || reload_shown || fps_shown || logs_shown || news_shown;
        // Nor under the SJK UI's report card, as under its other cards.
        let report_card = self.text_dialog.is_open() && self.text_dialog.is_sjk();
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        game_font::prepare(self);
        let information_visible = (self.live_session.is_some() || self.demo_session.is_some())
            && self
                .console
                .as_ref()
                .and_then(|c| c.bool_cvar("cg_draw2D"))
                .unwrap_or(true)
            && !self.game_menu
            && !self
                .console
                .as_ref()
                .is_some_and(|console| console.is_open())
            && !self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible());
        let scoreboard_held = self.gameplay_input.held(input::GameButton::Scores);
        self.append_world_pings(
            viewport,
            information_visible && intermission_view.is_none() && !scoreboard_held,
        );
        self.append_entity_view(
            viewport,
            information_visible && intermission_view.is_none() && !scoreboard_held,
        );
        if information_visible && intermission_view.is_none() {
            hud_runtime::append(
                self,
                classic_hud,
                hud_visibility,
                hud_layout,
                text_scale,
                viewport,
            );
        }
        quick_wheel::append(
            self,
            viewport,
            information_visible && intermission_view.is_none(),
        );
        let hud_scale = runtime_settings::hud_scale(self.console.as_ref());
        let menu_readout = (information_visible && intermission_view.is_none())
            .then(|| menu_hud::readout(self, presentation_time as i32))
            .flatten();
        self.menu_hud
            .prepare(&self.queue, menu_readout.as_ref(), viewport, hud_scale);
        if classic_hud && let Some(font) = &self.classic_hud_font {
            self.menu_hud
                .append_text(&mut self.classic_text_vertices, font, viewport, hud_scale);
        } else {
            self.menu_hud
                .append_text(&mut self.text_vertices, &self.ui_font, viewport, hud_scale);
        }
        scoreboard::update_context(self, intermission_view.is_some());
        let scores_requested = scoreboard::requested(self, intermission_view.is_some());
        let scores_allowed = information_visible
            && self
                .console
                .as_ref()
                .and_then(|c| c.bool_cvar("cg_drawScores"))
                .unwrap_or(true);
        let scoreboard_visible =
            self.scoreboard
                .present(self.console.as_ref(), scores_requested, scores_allowed);
        let chat_visible = !self.console_covers_frame()
            && scoreboard::chat_visible(
                information_visible,
                self.chat.wants_history(self.console.as_ref()),
            );
        if chat_visible {
            self.append_configured_chat(viewport, text_scale, scoreboard_visible);
        }
        self.append_weapon_select_name(viewport);
        if self.game_menu && !self.console_covers_frame() && !medal_popup {
            self.refresh_game_menu_players();
            let team_sizes = self.live_session.as_ref().map_or([0, 0], |session| {
                ingame_menu::team_sizes(session.game_state())
            });
            let view = ingame_menu::View {
                page: self.game_menu_page,
                selected_row: self.game_menu_row,
                team: self
                    .live_session
                    .as_ref()
                    .map_or(3, |session| session.latest_snapshot().player.team()),
                team_game: self.is_team_game(),
                siege: self.is_siege_game(),
                red_players: team_sizes[0],
                blue_players: team_sizes[1],
                vote_active: self.vote_active(),
                staff: profile_card::with(|summary| summary.staff),
                _frame: std::marker::PhantomData,
            };
            if view.page == GameMenuPage::Shot && !self.in_game_menu.is_sjk() {
                // Camera control has only the SJK UI's look, in every menu style.
                let target = ingame_menu::sjk_view::text_target(
                    &mut self.game_fonts,
                    &mut self.text_vertices,
                    &self.ui_font,
                );
                self.in_game_menu.append_sjk(view, target, viewport);
            } else if self.in_game_menu.is_sjk() {
                // Camera control has no match card.
                if view.page != GameMenuPage::Shot {
                    self.refresh_game_menu_card();
                }
                let chat_on = self
                    .console
                    .as_ref()
                    .is_some_and(|console| console.bool_cvar("cl_sjkChat") != Some(false));
                self.in_game_menu.sync_chat(chat_on);
                let target = ingame_menu::sjk_view::text_target(
                    &mut self.game_fonts,
                    &mut self.text_vertices,
                    &self.ui_font,
                );
                self.in_game_menu.append_sjk(view, target, viewport);
            } else {
                let (vertices, font) = self.game_fonts.menu(&mut self.text_vertices, &self.ui_font);
                self.in_game_menu.append(view, vertices, font, viewport);
            }
        }
        if scoreboard_visible {
            scoreboard::append_overlay(self, viewport, text_scale * 1.05);
        }
        if let Some(menu) = self
            .client_menu
            .as_mut()
            .filter(|_| !console_covers_frame && !medal_popup && !report_card)
        {
            if menu.sjk_screen() {
                // The SJK UI draws in its own families once they are loaded.
                let target = if self.game_fonts.has_sjk() {
                    let style = self.ui_font.style();
                    match self.game_fonts.sjk() {
                        Some(fonts) => menu::sjk::TextTarget::Families(fonts, style),
                        None => unreachable!("checked above"),
                    }
                } else {
                    let (vertices, font) =
                        self.game_fonts.menu(&mut self.text_vertices, &self.ui_font);
                    menu::sjk::TextTarget::Inter(vertices, font)
                };
                menu.append_sjk_screen(target, self.console.as_ref(), viewport);
            } else {
                let (vertices, font) = self.game_fonts.menu(&mut self.text_vertices, &self.ui_font);
                menu.append_overlay(vertices, font, viewport);
            }
        }
        self.append_console_overlay(viewport, text_scale);
        self.append_version_overlay(viewport, text_scale);
        let launcher = self.game_menu
            && self.game_menu_page != GameMenuPage::Shot
            && !self.in_game_menu.is_sjk()
            && !console_covers_frame
            && !medal_popup
            && !self.text_dialog.is_open();
        if launcher {
            let (vertices, font) = self.game_fonts.menu(&mut self.text_vertices, &self.ui_font);
            self.text_dialog.append_launcher(vertices, font, viewport);
        }
        if self.text_dialog.is_open() {
            self.append_text_dialog(viewport);
        } else {
            self.world_notes.composer_closed();
        }
        if medal_shown {
            self.append_medal_popup(viewport);
        } else if holocron_shown {
            self.append_holocron_popup(viewport);
        } else if reload_shown {
            self.append_reload_card(viewport);
        } else if fps_shown {
            self.append_fps_card(viewport);
        } else if logs_shown {
            self.append_log_request_card(viewport);
        } else if news_shown {
            self.append_whats_new_card(viewport);
        }
        // An achievement unlocked, or a medal or holocron in a match: its card over play or
        // the menus, never input-taking.
        let unlock_toast = self.append_unlock_toast(viewport, console_covers_frame || medal_popup);
        self.world_notes.draw_highlight(viewport);
        // The menu camera tour's fades, over the menu world only.
        let world_fade = menu_backdrop::standalone_menu_visible(self)
            && self
                .client_menu
                .as_mut()
                .is_some_and(|menu| menu.prepare_world_fade(viewport));
        let layers = [
            self.client_menu
                .as_ref()
                .filter(|_| world_fade)
                .map(menu::ClientMenu::world_fade),
            self.world_notes.fill(),
            self.world_notes.highlight(),
            self.entity_view.active().then(|| self.entity_view.list()),
            Some(&self.world_pings.list),
            information_visible.then(|| &self.hud.team_logo.list),
            information_visible.then(|| &self.hud.nameplate.list),
            information_visible.then(|| &self.hud.identification.list),
            information_visible.then(|| &self.hud.card.list),
            self.quick_wheel
                .is_open()
                .then(|| self.quick_wheel.draw_list()),
            information_visible.then(|| self.hud.draw_list()),
            chat_visible.then(|| self.chat.draw_list()),
            scoreboard_visible.then(|| self.scoreboard.draw_list()),
            (self.game_menu && !console_covers_frame && !medal_popup)
                .then(|| self.in_game_menu.draw_list()),
            self.client_menu
                .as_ref()
                .filter(|_| !console_covers_frame)
                .and_then(|menu| menu.backdrop_draw_list()),
            self.client_menu
                .as_ref()
                .filter(|_| !console_covers_frame && !medal_popup && !report_card)
                .and_then(|menu| menu.draw_list()),
            self.console.as_ref().map(|console| console.draw_list()),
            launcher.then(|| self.text_dialog.launcher_draw_list()),
            self.text_dialog
                .is_open()
                .then(|| self.text_dialog.draw_list()),
            medal_shown.then(|| self.medal_popup.draw_list()),
            holocron_shown.then(|| self.holocron_popup.draw_list()),
            reload_shown.then(|| self.graphics_reload.card.draw_list()),
            fps_shown.then(|| self.fps_card.draw_list()),
            logs_shown.then(|| self.log_request_card.draw_list()),
            news_shown.then(|| self.whats_new_card.draw_list()),
            unlock_toast.then(|| self.unlock_toast.draw_list()),
        ];
        self.ui_shapes
            .prepare_layers(&self.queue, layers.into_iter().flatten(), viewport);
        // Wide glyphs this frame asked for first draw next frame.
        self.wide_text.update(&self.queue);
        let text_vertex_count = u32::try_from(self.text_vertices.len()).unwrap_or(0);
        if !self.text_vertices.is_empty() {
            self.queue.write_buffer(
                &self.text_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.text_vertices),
            );
        }
        self.game_fonts.upload(&self.queue);
        let classic_text_vertex_count =
            u32::try_from(self.classic_text_vertices.len()).unwrap_or(0);
        if !self.classic_text_vertices.is_empty() {
            self.queue.write_buffer(
                &self.classic_text_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.classic_text_vertices),
            );
        }

        timing.mark(Phase::Pose);
        {
            self.assign_corpse_meshes(presentation_time);
            self.update_limbs(presentation_time);
        }
        {
            if let Err(error) = self.update_actor_animations(presentation_time, game_audio) {
                eprintln!("remote actor animation stopped: {error}");
            }
        }

        timing.mark(Phase::World);
        // PVS and the area mask belong to where the picture is taken from (`refdef.vieworg`),
        // not to the player's eye: a third-person camera sits behind and above it, often in
        // another cluster, and the eye's visible set then culled walls in plain view.
        let leaf_index = self.bsp.leaf_at(view_position.to_array());
        let camera_cluster = self.bsp.leaves()[leaf_index].cluster;
        let source_cluster = usize::try_from(camera_cluster).ok();
        if camera_cluster != self.last_cluster {
            self.last_cluster = camera_cluster;
        }

        let local_entity_id = self
            .live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            })
            .map(|snapshot| u64::from(snapshot.player.client_num()) + 1);
        let fallback_mesh = self
            .actor_meshes
            .iter()
            .position(|mesh| !mesh.corpse_pool && mesh.entity_id.is_none());
        self.actor_groups.iter_mut().for_each(Vec::clear);
        self.object_groups.iter_mut().for_each(Vec::clear);
        self.pickup_override_instances.clear();
        self.mover_groups.iter_mut().for_each(Vec::clear);
        self.entity_instances.clear();
        self.begin_saber_instances();
        self.particle_groups.iter_mut().for_each(Vec::clear);
        self.dynamic_lights.clear();
        self.submit_illuminate(presentation_time, visual_now);
        self.submit_holocron_stage(visual_now);
        self.sync_saber_skins(game_audio, presentation_time);
        let debug_missiles = effect_debug::sync(self.console.as_ref());
        let active_snapshot = first_person_view::presented_snapshot(
            self.live_session.as_ref(),
            self.demo_session.as_ref(),
            presentation_time as i32,
        );
        if let Some(snapshot) = active_snapshot {
            let active_world = self
                .demo_session
                .as_ref()
                .map_or(&self.live_world, demo_playback::Session::world);
            if let Some(audio) = game_audio {
                let predicted = self
                    .local_prediction
                    .predicted_state()
                    .filter(|_| self.live_session.is_some());
                audio.observe_saber_switches(snapshot, predicted);
                audio.update_frame_loops(
                    snapshot,
                    presentation_time as i32,
                    self.camera_position.to_array(),
                    active_world,
                    &self.bsp,
                );
            }
            self.map_effects.update(snapshot, presentation_time as i32);
            effect_runtime::spawn_map_effect_requests(
                &mut self.particles,
                &mut self.effect_aux,
                &self.map_effects,
                self.vfs
                    .as_ref()
                    .expect("live sessions retain their mounted VFS"),
                &mut self.effects,
                game_audio,
                visual_now,
            );
            let missile_metrics = missile_trails::update_and_spawn(
                &mut self.missile_effects,
                snapshot,
                presentation_time as i32,
                &mut self.particles,
                &mut self.effect_aux,
                &mut self.effects,
                self.vfs
                    .as_ref()
                    .expect("live sessions retain their mounted VFS"),
                game_audio,
                &mut self.dynamic_lights,
                visual_now,
            );
            if debug_missiles {
                missile_trails::debug_report(
                    &self.missile_effects,
                    missile_metrics,
                    snapshot,
                    &mut self.effects,
                    self.vfs
                        .as_ref()
                        .expect("live sessions retain their mounted VFS"),
                    presentation_time as i32,
                );
            }
            projectiles::collect(snapshot, presentation_time as i32, &mut self.projectiles);
            movers::collect(snapshot, presentation_time as i32, &mut self.movers);
            self.local_prediction
                .pickups(snapshot, presentation_time as i32, &mut self.pickups);
            trip_mine_lasers::spawn(
                &mut self.trip_mine_beams,
                snapshot,
                &self.bsp,
                &mut self.trace_scratch,
                self.camera_position,
                &mut self.particles,
                &mut self.effect_aux,
                &mut self.effects,
                self.vfs
                    .as_ref()
                    .expect("live sessions retain their mounted VFS"),
                game_audio,
                visual_now,
                presentation_time as i32,
            );
            pickups::simple::prepare(&mut self.pickups, self.console.as_ref());
            pickups::spawn_cones(
                &self.pickups,
                &mut self.particles,
                &mut self.effect_aux,
                &mut self.effects,
                self.vfs
                    .as_ref()
                    .expect("live sessions retain their mounted VFS"),
                game_audio,
                presentation_time as i32,
                visual_now,
            );
            self.muzzle_effects
                .update(snapshot, presentation_time as i32);
        } else {
            self.projectiles.clear();
            if !self.resident.exploring() && !world_shot_movers_pinned() {
                self.movers.clear();
            }
            self.pickups.clear();
        }
        let view_weapon = first_person_weapon::frame_inputs(
            self,
            active_snapshot,
            first_person,
            local_entity_id,
            presentation_time,
        );
        let first_person_charge = view_weapon.and(active_snapshot).and_then(|snapshot| {
            let predicted = self
                .live_session
                .as_ref()
                .and_then(|_| self.local_prediction.predicted_state());
            charge_flash::Charges::collect(Some(snapshot), predicted)
                .of(snapshot.player.client_num())
        });
        if let Some(inputs) = view_weapon {
            first_person_weapon::submit(&self.first_person_weapon, inputs, &mut self.object_groups);
            // `CG_AddPlayerWeapon` records the view gun's `tag_flash` for
            // trace-weapon beams (`cg_weapons.c:579`) and plays the
            // first-person muzzle effect there (`cg_weapons.c:704-762`).
            let socket = first_person_weapon::flash_socket(&self.first_person_weapon, inputs);
            self.last_first_person_flash = socket.map(|socket| socket.origin);
            if let Some(request) = active_snapshot
                .and_then(|snapshot| self.muzzle_effects.request(snapshot.player.client_num()))
                && let Some(socket) = socket
            {
                muzzle_effects::spawn(
                    &mut self.particles,
                    &mut self.effect_aux,
                    &mut self.effects,
                    self.vfs
                        .as_ref()
                        .expect("sessions retain their mounted VFS"),
                    game_audio,
                    request,
                    socket,
                    true,
                    visual_now,
                    presentation_time as i32,
                );
            }
        }
        self.prepare_scene_views(view, projection, presentation_time as i32);
        movers::append_frame(self, presentation_time);
        self.static_models.append_instances(&mut self.object_groups);
        pickups::simple::append_frame(self, visual_now);
        // The charge glow on the view gun's muzzle (`cg_weapons.c` charge bits), after
        // this frame's sprites were cleared.
        if let (Some(origin), Some(charge)) = (
            view_weapon.and(self.last_first_person_flash),
            first_person_charge,
        ) {
            charge_flash::push(
                &mut self.particles,
                &mut self.effects,
                charge,
                Vec3::from_array(origin),
                presentation_time as i32,
                visual_now,
            );
        }
        self.force_overlays_last = actor_world_submission::submit(
            self,
            local_entity_id,
            fallback_mesh,
            presentation_time,
            visual_now,
            game_audio,
        );
        // A hosting engine's entities (`crate::host`), beside SJK's own.
        if crate::host::active() {
            crate::host::entities::submit(self, presentation_time);
        }
        let depth_size = self.depth.view.texture().size();
        self.card_outline.prepare(
            &self.device,
            &self.queue,
            self.hud.card.outline().map(|(_, frame)| frame),
            self.hud.team_outline.areas(),
            [depth_size.width, depth_size.height],
            [self.configuration.width, self.configuration.height],
        );
        timing.mark(Phase::Acquire);
        let target = match frame_target::prepare(self) {
            Ok(target) => target,
            Err(status) => {
                self.detached_camera = detached_before_peek;
                return status;
            }
        };
        let target_view = target.scene.clone();
        timing.mark(Phase::Effects);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("SJK frame encoder"),
            });
        if let Some(phases) = &self.gpu_phases {
            phases.begin(&mut encoder);
        }
        dynamic_lights::entities::finish(self, presentation_time, visual_now, game_audio);
        let detached_flight = self.free_camera_active();
        let object_groups = &mut self.object_groups;
        let mover_groups = &mut self.mover_groups;
        let actor_groups = &mut self.actor_groups;
        let entity_instances = &mut self.entity_instances;
        let saber_instances = &mut self.saber_instances;
        let particle_groups = &mut self.particle_groups;
        let active_world = self
            .demo_session
            .as_ref()
            .map_or(&self.live_world, demo_playback::Session::world);
        projectiles::append_models(
            &self.projectiles,
            &self.missile_effects,
            self.live_session
                .as_ref()
                .map(ClientSession::game_state)
                .or_else(|| {
                    self.demo_session
                        .as_ref()
                        .map(demo_playback::Session::game_state)
                }),
            &self.object_meshes,
            object_groups,
        );
        saber_clash_flare::append(
            &self.saber_clash_flare,
            presentation_time as i32,
            view_position,
            (view_target - view_position).normalize_or(forward),
            projection * view,
            // A perspective projection's y scale over its x scale is the aspect.
            projection.y_axis.y / projection.x_axis.x,
            &self.bsp,
            &mut self.trace_scratch,
            &self.particle_atlas,
            &mut particle_groups[1],
        );
        self.context.dynamic_light_settings.upload(
            &self.world_materials,
            &self.queue,
            &mut self.dynamic_lights,
        );
        if let Some(vfs) = self.vfs.as_deref() {
            self.world_materials
                .update_videos(&self.queue, vfs, visual_now);
        }
        let particle_now = Instant::now();
        particle_physics::update_and_spawn(
            &mut self.particles,
            &mut self.pending_particle_effects,
            &mut self.effect_aux,
            &mut self.effects,
            self.vfs
                .as_ref()
                .expect("particle effects retain their VFS"),
            game_audio,
            particle_now,
            &self.bsp,
            &mut self.trace_scratch,
        );
        effect_emitter::append_model_instances(
            &mut self.effect_aux.emitters,
            &self.emitter_model_catalog,
            object_groups,
        );
        self.world_materials.fogged_entities = entity_lighting::apply_frame(
            &self.entity_lighting,
            &self.bsp,
            &self.dynamic_lights,
            actor_groups,
            object_groups,
            &mut self.pickup_override_instances,
            self.world_materials.fogs(),
        );
        // Saber shaders taking the colour of the light where they are read the grid too.
        saber_persona::light_instances(saber_instances, &self.blade_skins, |at| {
            let light = self.entity_lighting.sample(&self.bsp, at, &[]);
            std::array::from_fn(|i| light.ambient[i] + 0.5 * light.directed[i])
        });
        let global_particle_seconds = presentation_time as f32 * 0.001;
        player_shadows::request_all(
            &self.player_shadows,
            player_shadows::Inputs {
                world: active_world,
                presentation_time,
                local_entity: local_entity_id.filter(|_| !detached_flight),
                local_root: camera::local_actor_root(
                    self.camera_position,
                    self.local_prediction.view_height(),
                    self.camera_yaw,
                )
                .0,
                bsp: &self.bsp,
                scratch: &mut self.trace_scratch,
                decals: &mut self.effect_aux.decals,
            },
            player_shadows::mesh_legs_yaw(&self.actor_meshes),
        );
        let atlas = &self.particle_atlas;
        self.effect_aux.decals.project_pending(
            &self.decal_surfaces,
            &self.bsp,
            particle_now,
            |shader| {
                effect_blend::slot(
                    atlas
                        .layers_for(shader, 0.0)
                        .iter()
                        .next()
                        .map_or(ParticleBlend::Add, |layer| layer.blend),
                )
            },
        );
        timing.mark(Phase::Encode);
        let particle_ranges = effect_submission::prepare(
            timing,
            effect_submission::Inputs {
                geometry: &mut self.effect_geometry,
                queue: &self.queue,
                encoder: &mut encoder,
                particles: &mut self.particles,
                decals: &mut self.effect_aux.decals,
                atlas: &self.particle_atlas,
                now: particle_now,
                global_seconds: global_particle_seconds,
                camera: self.camera_position,
                field_of_view: self.field_of_view,
                bsp: &self.bsp,
                trace_scratch: &mut self.trace_scratch,

                entity_instances,
                blended: particle_groups,
            },
        );
        timing.mark(Phase::EncodeInstances);
        Self::pack_frame_instances(
            &mut self.actor_instances,
            &mut self.actor_instance_ranges,
            &mut self.object_instance_ranges,
            &mut self.mover_instance_ranges,
            actor_groups,
            object_groups,
            mover_groups,
        );
        let actor_instances = &mut self.actor_instances;
        let actor_instance_ranges = &mut self.actor_instance_ranges;
        let object_instance_ranges = &mut self.object_instance_ranges;
        entity_materials::append_override_ranges(
            actor_instances,
            &self.pickup_override_instances,
            &mut self.pickup_override_ranges,
        );
        self.entity_draw_queue.rebuild(
            &self.world_materials,
            &self.actor_meshes,
            actor_instance_ranges,
            &self.object_meshes,
            object_instance_ranges,
            &self.pickup_override_ranges,
            actor_instances,
            self.camera_position,
            crate::card_outline::Group::ALL
                .map(|group| self.model_material_overrides.outline(group)),
        );
        self.saber_gpu.prepare(
            &self.queue,
            saber_instances,
            &mut self.saber_trail_segments,
            presentation_time,
        );
        if !entity_instances.is_empty() {
            self.queue.write_buffer(
                &self.entity_instance_buffer,
                0,
                bytemuck::cast_slice(entity_instances),
            );
        }
        if !actor_instances.is_empty() {
            self.queue.write_buffer(
                &self.actor_instance_buffer,
                0,
                bytemuck::cast_slice(actor_instances),
            );
        }
        let has_entity_instances = !entity_instances.is_empty();
        timing.mark(Phase::EncodeViews);
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "uploads");
        }
        if !self.world_hidden {
            // Walls stop dynamic lights: their shadow tiles before any view lights with them.
            self.world_materials
                .trace_dynamic_light_shadows(&mut encoder, self.gpu_phases.as_ref());
            self.draw_scene_views(&mut encoder, &particle_ranges);
        }
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "views");
        }
        timing.mark(Phase::EncodeWorld);
        if self.world_hidden {
            self.encode_cleared_scene(&mut encoder, &target_view);
        } else {
            self.encode_world_scene(
                &mut encoder,
                &target_view,
                source_cluster,
                &particle_ranges,
                has_entity_instances,
            );
        }
        self.encode_stage_preview(&mut encoder);
        let (output, mut encoder) = match target.finish(self, encoder, timing) {
            Ok(output) => output,
            Err(status) => {
                self.detached_camera = detached_before_peek;
                return status;
            }
        };
        let visibility = self.bsp.render().visibility();
        timing.mark(Phase::EncodeOverlays);
        self.draw_frame_overlays(
            &mut encoder,
            &target_view,
            &output.view,
            &output.ui,
            source_cluster,
            visibility,
            text_vertex_count,
            classic_text_vertex_count,
        );
        timing.mark(Phase::EncodeCopy);
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "overlays+post+hud");
        }
        self.encode_console_screenshot(&mut encoder, output.frame.as_ref());
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "capture");
            phases.finish(&mut encoder);
        }
        timing.mark(Phase::Submit);
        self.frame_pacer
            .split
            .submit(&self.queue, encoder, output.frame, timing);
        timing.mark(Phase::Present);
        timing.mark(Phase::Other);
        self.complete_render_transition(game_audio);
        self.detached_camera = detached_before_peek;
        FrameStatus::Rendered
    }
}

struct ParticleAtlas {
    bind_group: wgpu::BindGroup,
    animations: HashMap<String, Vec<ParticleAtlasAnimation>>,
    fallback: [f32; 4],
    /// Some stage is a dynamic glow stage, so effects sort glowing layers apart.
    any_glow: bool,
    remaps: effect_remaps::State,
}

#[derive(Clone)]
struct ParticleAtlasAnimation {
    frames: Vec<[f32; 4]>,
    frequency: f32,
    one_shot: bool,
    blend: ParticleBlend,
    rgb_wave: Option<WaveForm>,
    alpha_wave: Option<WaveForm>,
    tc_scale: [f32; 2],
    tc_scroll: [f32; 2],
    /// The stage is drawn into the dynamic glow image too.
    glow: bool,
    /// The stage asks for the effect's colour (`rgbGen vertex`, `exactVertex` or
    /// `entity`); see [`ParticleLayerSample::tint`].
    tinted: bool,
    /// Remap destination clock, subtracted from the sampled shader time.
    time_offset: f32,
}

#[derive(Clone, Copy)]
struct ParticleLayerSample {
    uv_rect: [f32; 4],
    blend: ParticleBlend,
    rgb: f32,
    alpha: f32,
    uv_transform: [f32; 4],
    glow: bool,
    tinted: bool,
}

mod effect_remaps;
mod particle_atlas_sampling;

mod depth_target;
use depth_target::DepthTarget;

/// World shots place movers by hand (`world_shot::movers`); nothing else keeps them
/// without a snapshot.
#[cfg(test)]
fn world_shot_movers_pinned() -> bool {
    world_shot::MOVERS_PINNED.with(std::cell::Cell::get)
}
#[cfg(not(test))]
fn world_shot_movers_pinned() -> bool {
    false
}
