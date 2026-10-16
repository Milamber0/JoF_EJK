//! Native and demo-playback launch orchestration.

use super::*;

pub(super) fn run() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let demo_request = demo_playback::request(&arguments)?;
    let (
        game_data,
        map_override,
        player_directory,
        connect_address,
        open_main_menu,
        mut demo_session,
    ) = if let Some(request) = demo_request {
        let mut session = demo_playback::Session::open(&request.demo, request.camera, request.fps)?;
        if request.from_millis != 0 {
            session.advance_to(request.from_millis, |_, _| {})?;
        }
        (request.game_data, None, None, None, false, Some(session))
    } else {
        let configured = platform::per_user_config_file()
            .ok()
            .and_then(|path| launch::saved_game_data(&path));
        let launch = launch::resolve(arguments.into_iter(), configured)?;
        (
            launch.game_data,
            launch.map_path,
            launch.player_directory,
            launch.connect_address,
            launch.open_main_menu,
            None,
        )
    };
    platform::initialize_storage(&game_data)?;
    let mut console = console::ViewerConsole::new(platform::user_config_file()?)?;
    // A player's first start is on Vanilla; the SJK renderer (alpha) is theirs to
    // turn on in Settings.
    if console.new_profile() {
        crate::graphics_quality::apply_new_profile(&mut console);
    }
    for line in notice::LINES {
        log::progress(format_args!("{line}"));
        console.push_log_quiet(line);
    }
    console.check_for_updates_at_launch();
    // The build's commit and date too, so a log names exactly what ran.
    log::progress(format_args!("build: {}", crate::build_info::label()));
    assets::search_paths::initialize(&console)?;
    // SJK's packs from the hub, cached beside identity.key, before any world mounts.
    crate::sjk_packs::mount_at_start(console.config_directory());
    // The material maps sjk-materialgen wrote beside the settings, above all packs.
    assets::generated::locate_at_start(console.config_directory());
    // The menu goes up first so its master-server fetch runs while the map
    // loads: the browser has servers by the time the main menu is on screen.
    let mut client_menu = menu::ClientMenu::new(open_main_menu, console.master_server()?);
    if open_main_menu {
        client_menu.prefetch_servers();
    }
    let mut live_session: Option<ClientSession> = None;
    let connect_timeline: Option<log::ConnectTimeline> = None;
    let map_path = if let Some(path) = map_override {
        path
    } else if let Some(session) = &demo_session {
        map_path_from_gamestate(session.game_state())?
    } else {
        menu::style::MenuStyle::from_cvar(console.text_value(menu::style::CVAR))
            .boot_map()
            .to_owned()
    };
    let (bsp, vfs) = if let Some(session) = &demo_session {
        let selection = assets::session_content::Selection::from_game(session.game_state())?;
        let vfs = selection.mount(&game_data)?;
        (selection.load_bsp(&vfs, &map_path)?, vfs)
    } else {
        assets::load_bsp(&game_data, &map_path)?
    };
    let scene = StaticWorld::build(&bsp, MeshBuildOptions::default().with_sky_surfaces())?;
    let shaders = assets::load_shaders(&vfs);
    let (mut camera_origin, mut camera_yaw) = assets::initial_camera(&bsp)?;
    if let Some(session) = &live_session {
        camera_origin = session.latest_snapshot().player.origin();
        camera_origin[2] += session.latest_snapshot().player.view_height() as f32;
        camera_yaw = session.latest_snapshot().player.view_angles()[1].to_radians();
        log::progress(format_args!(
            "joined {} as client {}; live camera starts at {:?}",
            session.server(),
            session.game_state().client_num,
            camera_origin
        ));
    } else if let Some(session) = &demo_session {
        let player = &session.latest_snapshot().player;
        camera_origin = player.origin();
        camera_origin[2] += player.view_height() as f32;
        camera_yaw = player.view_angles()[1].to_radians();
    }
    let player_preview = player_directory
        .as_deref()
        .map(|directory| load_player_preview(&vfs, directory, camera_origin, camera_yaw))
        .transpose()?;
    let world_bounds = bsp.render().models()[0].clone();
    log::progress(format_args!(
        "loaded {map_path}: {} triangles in {} batches; {} scripted materials",
        scene.triangle_count(),
        scene.batches().len(),
        shaders.len()
    ));
    console.push_log(format!("Loaded {map_path}"));
    if open_main_menu {
        log::progress(format_args!("client state: main menu"));
    }
    let event_loop = EventLoop::new()?;
    let mut application = viewer_app::ViewerApplication {
        scene: Some(scene),
        bsp: Some(bsp),

        vfs: Some(std::sync::Arc::new(vfs)),
        shaders: Some(shaders),
        world_minimums: world_bounds.minimums,
        world_maximums: world_bounds.maximums,
        camera_origin,
        camera_yaw,
        player_preview,
        live_session: live_session.take(),
        demo_session: demo_session.take(),
        console: Some(console),
        client_menu: Some(client_menu),
        game_data,
        connect_timeline,

        completed_map_changes: 0,
        game_audio: None,
        initial_connect: connect_address,
        gpu: None,
        menu_world: crate::menu_world::Parked::new(),
        boot_map: map_path,
    };
    event_loop.run_app(&mut application)?;
    Ok(())
}

pub(super) fn map_path_from_gamestate(game_state: &GameState) -> Result<String, Box<dyn Error>> {
    let server_info = game_state
        .config_string(0)
        .ok_or("gamestate has no CS_SERVERINFO")?;
    let server_info = InfoString::parse(std::str::from_utf8(server_info)?)?;
    Ok(format!(
        "maps/{}.bsp",
        server_info
            .get("mapname")
            .ok_or("CS_SERVERINFO has no mapname")?
    ))
}
