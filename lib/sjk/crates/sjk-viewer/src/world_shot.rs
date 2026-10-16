//! Off-screen frames of a map, for reviewing what a camera sees without a
//! window: a [`GpuState`] built windowless renders into an image of its own
//! ([`GpuState::headless_frame`]) that is read back to a PNG in
//! `target/world-shots`. The tests here are ignored: they need a GPU adapter
//! and the installed game data named by `JKA_GAME_DATA`.

use crate::*;

/// Run `shots` on a thread with the stack a whole client needs (a test
/// thread's 2 MiB overflows building one).
pub(crate) fn on_big_stack(shots: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name("world shots".to_owned())
        .stack_size(256 << 20)
        .spawn(shots)
        .expect("the world shot thread")
        .join()
        .expect("the world shots ran");
}

thread_local! {
    /// Keep `GpuState::movers` from frame to frame without a snapshot.
    pub(crate) static MOVERS_PINNED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Where the shots are written.
pub(crate) fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/world-shots")
}

/// A windowless client on `map` (`maps/mp/duel6.bsp`) at `size`, with the
/// main menu up when `menu` is given; `None` without a GPU adapter.
pub(crate) fn open(
    map: &str,
    size: [u32; 2],
    menu: Option<menu::ClientMenu>,
    cvars: &[(&str, &str)],
) -> Option<(GpuState, tempfile::TempDir)> {
    let (input, directory) = world_input(map, menu, cvars);
    let mut gpu = match pollster::block_on(GpuState::new_for_target(None, size, input)) {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("no windowless GPU state ({error}); skipped");
            return None;
        }
    };
    gpu.is_menu_world = true;
    attach_target(&mut gpu, size);
    Some((gpu, directory))
}

/// What [`open`] builds a client from: `map` loaded from `JKA_GAME_DATA`, a throwaway
/// profile (kept alive by the returned directory) with `cvars` set, and `menu`.
pub(crate) fn world_input(
    map: &str,
    menu: Option<menu::ClientMenu>,
    cvars: &[(&str, &str)],
) -> (GpuWorldInput, tempfile::TempDir) {
    let game_data = PathBuf::from(
        std::env::var_os("JKA_GAME_DATA").expect("JKA_GAME_DATA names the GameData directory"),
    );
    let directory = tempfile::tempdir().expect("a profile directory");
    let mut console =
        console::ViewerConsole::new(directory.path().join("config.cfg")).expect("a console");
    // A throwaway profile must not register an identity with the hub or look
    // for updates.
    console.set_cvar("cl_identity", "0");
    console.set_cvar("cl_autoUpdate", "0");
    // Shots for the site leave out the build label.
    if std::env::var_os("WORLD_SHOT_NO_VERSION").is_some() {
        console.set_cvar(crate::version_overlay::CVAR, "0");
    }
    for (name, value) in cvars {
        console.set_cvar(name, value);
    }
    let (bsp, vfs) = assets::load_bsp(&game_data, map).expect("the map");
    let scene = StaticWorld::build(&bsp, MeshBuildOptions::default().with_sky_surfaces())
        .expect("the map's meshes");
    let shaders = assets::load_shaders(&vfs);
    let (camera_origin, camera_yaw) = assets::initial_camera(&bsp).expect("a camera");
    let bounds = bsp.render().models()[0].clone();
    let input = GpuWorldInput {
        scene,
        bsp,
        vfs: Arc::new(vfs),
        shaders,
        world_minimums: bounds.minimums,
        world_maximums: bounds.maximums,
        camera_origin,
        camera_yaw,
        player_preview: None,
        live_session: None,
        demo_session: None,
        build_game_state: None,
        build_snapshot: None,
        console: Some(console),
        client_menu: menu,
        game_data,
        connect_timeline: None,
        game_fonts: false,
        completed_map_changes: 0,
    };
    (input, directory)
}

/// Give `gpu` the image its frames render into at `size` (a world built after
/// [`open`], as a graphics reload builds one, has none).
pub(crate) fn attach_target(gpu: &mut GpuState, size: [u32; 2]) {
    let format = gpu.context.format;
    gpu.headless_frame = Some(gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("world shot"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &ui_target::surface_view_formats(format, gpu.context.ui_direct),
    }));
}

/// Render `frames` frames (letting what loads on workers arrive) and save the
/// last as `name`.png; returns its path.
pub(crate) fn shoot(gpu: &mut GpuState, frames: u32, name: &str) -> PathBuf {
    for _ in 0..frames {
        let _ = gpu.render(&mut None);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    let texture = gpu.headless_frame.as_ref().expect("a world shot target");
    let image = read_back(&gpu.device, &gpu.queue, texture);
    std::fs::create_dir_all(directory()).expect("the shot directory");
    let path = directory().join(format!("{name}.png"));
    image.save(&path).expect("write the shot");
    path
}

/// Put the camera at `origin`, turned `yaw` degrees counter-clockwise from
/// +X and `pitch` degrees up (the backdrop's convention, not `viewpos`'s).
pub(crate) fn aim(gpu: &mut GpuState, origin: [f32; 3], yaw: f32, pitch: f32) {
    gpu.camera_position = Vec3::from_array(origin);
    gpu.camera_yaw = yaw.to_radians();
    gpu.camera_pitch = pitch.to_radians();
}

/// Render `frames` frames and return the image.
pub(crate) fn frame(gpu: &mut GpuState, frames: u32) -> image::RgbaImage {
    for _ in 0..frames {
        let _ = gpu.render(&mut None);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    let texture = gpu.headless_frame.as_ref().expect("a world shot target");
    read_back(&gpu.device, &gpu.queue, texture)
}

/// `images` side by side, `columns` to a row, each scaled to `cell` pixels
/// wide, saved as `name`.png: a contact sheet to review many views at once.
pub(crate) fn sheet(images: &[image::RgbaImage], columns: u32, cell: u32, name: &str) -> PathBuf {
    let first = images.first().expect("a shot");
    let cell_height = cell * first.height() / first.width();
    let rows = (images.len() as u32).div_ceil(columns);
    let mut out = image::RgbaImage::new(columns * cell, rows * cell_height);
    for (index, shot) in images.iter().enumerate() {
        let small = image::imageops::resize(
            shot,
            cell,
            cell_height,
            image::imageops::FilterType::Triangle,
        );
        let (x, y) = (index as u32 % columns, index as u32 / columns);
        image::imageops::overlay(
            &mut out,
            &small,
            i64::from(x * cell),
            i64::from(y * cell_height),
        );
    }
    std::fs::create_dir_all(directory()).expect("the shot directory");
    let path = directory().join(format!("{name}.png"));
    out.save(&path).expect("write the sheet");
    path
}

/// The yaw and pitch (degrees, the backdrop's convention) from `from` to `at`.
pub(crate) fn look(from: [f32; 3], at: [f32; 3]) -> (f32, f32) {
    let direction = (Vec3::from_array(at) - Vec3::from_array(from)).normalize_or_zero();
    (
        direction.y.atan2(direction.x).to_degrees(),
        direction.z.clamp(-1.0, 1.0).asin().to_degrees(),
    )
}

/// Render each camera of `views` (name, origin, a point it looks at) and save
/// them as contact sheet `name`, printing each one's index, numbers and
/// whether it starts inside a wall.
pub(crate) fn sweep(gpu: &mut GpuState, views: &[(&str, [f32; 3], [f32; 3])], name: &str) {
    let _ = frame(gpu, 30);
    let mut images = Vec::new();
    for (index, (label, origin, at)) in views.iter().enumerate() {
        let (yaw, pitch) = look(*origin, *at);
        aim(gpu, *origin, yaw, pitch);
        images.push(frame(gpu, 6));
        let solid = gpu.bsp.point_contents(*origin, 1) != 0;
        println!(
            "{index:2}: {label} {origin:?} yaw {yaw:.1} pitch {pitch:.1}{}",
            if solid { " IN A WALL" } else { "" }
        );
    }
    println!("{}", sheet(&images, 4, 480, name).display());
}

/// The texture's pixels.
pub(crate) fn read_back(
    device: &wgpu::Device,
    queue: &frame_queue::FrameQueue,
    texture: &wgpu::Texture,
) -> image::RgbaImage {
    let (width, height) = (texture.width(), texture.height());
    let row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("world shot readback"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("world shot readback"),
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("map the readback")
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().expect("the mapped readback");
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for line in data.chunks(row as usize).take(height as usize) {
        pixels.extend_from_slice(&line[..(width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();
    image::RgbaImage::from_raw(width, height, pixels).expect("the shot's pixels")
}

#[cfg(test)]
#[path = "world_shot_notes.rs"]
mod notes;

#[cfg(test)]
#[path = "world_shot_movers.rs"]
mod movers;

#[path = "world_shot_illuminate.rs"]
mod holocron;

#[path = "world_shot_saber_skins.rs"]
mod saber_skins;

#[path = "world_shot_animation_profile.rs"]
mod animation_profile;
#[path = "world_shot_card_outline.rs"]
mod card_outline_shots;
#[path = "world_shot_rig_model.rs"]
mod rig_model_shots;
#[path = "world_shot_team_outline.rs"]
mod team_outline_shots;

#[path = "world_shot_collection.rs"]
mod collection;

#[path = "world_shot_holocrons.rs"]
mod holocrons_tab;

#[path = "world_shot_cards.rs"]
mod cards_shots;

#[path = "world_shot_create_game.rs"]
mod create_game;

#[path = "world_shot_graphics_reload.rs"]
mod graphics_reload;

#[path = "world_shot_fps_help.rs"]
mod fps_help_shots;

#[path = "world_shot_chat_gifs.rs"]
mod chat_gifs_shots;

#[path = "world_shot_map_change.rs"]
mod map_change;

#[path = "world_shot_brushes.rs"]
mod brushes;

#[path = "world_shot_log_requests.rs"]
mod log_requests;

#[path = "world_shot_whats_new.rs"]
mod whats_new;

#[cfg(test)]
#[path = "world_shot_footprints.rs"]
mod footprints_shots;
#[cfg(test)]
#[path = "world_shot_mover_models.rs"]
mod mover_models_shots;
#[path = "world_shot_pings.rs"]
mod pings;

#[cfg(test)]
mod tests {
    use super::*;

    /// The world from the duel6 camera the menu starts on, to check the
    /// harness itself.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_from_its_start() {
        on_big_stack(|| {
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[])
            else {
                return;
            };
            let path = shoot(&mut gpu, 40, "duel6-start");
            println!("{}", path.display());
        });
    }

    /// The console's looks over duel6, half open on made-up scrollback with a
    /// command being typed (its unique completion ghosted) and two rows
    /// selected: the SJK UI's deck and the classic console; the deck
    /// full height and at 4K, and scrolled back.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_console_styles() {
        use sjk_ui::{InputEvent, PointerButton, Vec2};
        fn shots(size: [u32; 2], looks: &[(&'static str, &'static str, &'static str)]) {
            let cvars = [("cl_identity", "0")];
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, None, &cvars) else {
                return;
            };
            let tour = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let shot = &tour[0];
            let (yaw, pitch) = look(shot.from, shot.at);
            aim(&mut gpu, shot.from, yaw, pitch);
            let s = size[1] as f32 / 1080.0;
            for (name, style, height) in looks {
                if let Some(console) = gpu.console.as_mut() {
                    console.set_cvar("con_style", style);
                    console.set_cvar("con_height", height);
                    console.set_open(false);
                    console.console_for_shot("cl_maxpa");
                }
                let _ = frame(&mut gpu, 30);
                // Drag across two rows of the status table.
                if let Some(console) = gpu.console.as_mut() {
                    console.handle_pointer(InputEvent::PointerPress {
                        position: Vec2::new(60.0 * s, 300.0 * s),
                        button: PointerButton::Primary,
                    });
                }
                let _ = frame(&mut gpu, 1);
                if let Some(console) = gpu.console.as_mut() {
                    console
                        .handle_pointer(InputEvent::PointerMove(Vec2::new(420.0 * s, 322.0 * s)));
                }
                let _ = frame(&mut gpu, 1);
                if let Some(console) = gpu.console.as_mut() {
                    console.handle_pointer(InputEvent::PointerRelease {
                        position: Vec2::new(420.0 * s, 322.0 * s),
                        button: PointerButton::Primary,
                    });
                }
                println!("{}", shoot(&mut gpu, 2, name).display());
            }
        }
        on_big_stack(|| {
            shots(
                [1920, 1080],
                &[
                    ("duel6-console-sjk", "sjk", "0.5"),
                    ("duel6-console-classic", "classic", "0.5"),
                    ("duel6-console-sjk-full", "sjk", "1"),
                ],
            );
            shots([3840, 2160], &[("duel6-console-sjk-4k", "sjk", "0.5")]);
        });
    }

    /// The command browser (F3) in the SJK UI's look over duel6: a search for
    /// `viewmodel`, its first entry chosen and part of its description selected.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_console_browser_sjk() {
        use sjk_ui::Vec2;
        on_big_stack(|| {
            let cvars = [("con_style", "sjk")];
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], None, &cvars)
            else {
                return;
            };
            let tour = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let (yaw, pitch) = look(tour[0].from, tour[0].at);
            aim(&mut gpu, tour[0].from, yaw, pitch);
            if let Some(console) = gpu.console.as_mut() {
                console.set_cvar("cg_fov", "110");
                console.console_for_shot("");
                console.open_browser_on("viewmodel");
            }
            let _ = frame(&mut gpu, 20);
            if let Some(console) = gpu.console.as_mut() {
                console.browser_drag_for_shot(Vec2::new(1452.0, 390.0), Vec2::new(1580.0, 418.0));
            }
            println!(
                "{}",
                shoot(&mut gpu, 3, "duel6-console-browser-sjk").display()
            );
        });
    }

    /// A plan of duel6 from above: every upward-facing surface coloured by its
    /// height (dark low, light high), a grid every 256 units (brighter every
    /// 1024, the axes brightest), spawns red and the intermission green. One
    /// pixel is 4 units, x to the right, y up; the image's centre is (0, 0).
    #[test]
    #[ignore = "reads the installed game data named by JKA_GAME_DATA"]
    fn duel6_plan() {
        let game_data = PathBuf::from(std::env::var_os("JKA_GAME_DATA").expect("JKA_GAME_DATA"));
        let (bsp, _) = assets::load_bsp(&game_data, "maps/mp/duel6.bsp").expect("the map");
        let scene = StaticWorld::build(&bsp, MeshBuildOptions::default()).expect("meshes");
        const UNITS: f32 = 4.0;
        const SIDE: u32 = 1600;
        let half = SIDE as f32 * 0.5;
        let pixel = |x: f32, y: f32| (half + x / UNITS, half - y / UNITS);
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for batch in scene.batches() {
            for vertex in &batch.vertices {
                low = low.min(vertex.position[2]);
                high = high.max(vertex.position[2]);
            }
        }
        let mut height = vec![f32::MIN; (SIDE * SIDE) as usize];
        let mut image = image::RgbaImage::from_pixel(SIDE, SIDE, image::Rgba([12, 16, 28, 255]));
        for batch in scene.batches() {
            for triangle in batch.indices.chunks_exact(3) {
                let [a, b, c] = [0, 1, 2].map(|i| batch.vertices[triangle[i] as usize].position);
                let normal = Vec3::from(b) - Vec3::from(a);
                let normal = normal
                    .cross(Vec3::from(c) - Vec3::from(a))
                    .normalize_or_zero();
                if normal.z < 0.6 {
                    continue;
                }
                let points = [a, b, c].map(|p| pixel(p[0], p[1]));
                let (x0, x1) = (
                    points
                        .iter()
                        .map(|p| p.0)
                        .fold(f32::MAX, f32::min)
                        .floor()
                        .max(0.0) as u32,
                    points
                        .iter()
                        .map(|p| p.0)
                        .fold(f32::MIN, f32::max)
                        .ceil()
                        .min(SIDE as f32 - 1.0) as u32,
                );
                let (y0, y1) = (
                    points
                        .iter()
                        .map(|p| p.1)
                        .fold(f32::MAX, f32::min)
                        .floor()
                        .max(0.0) as u32,
                    points
                        .iter()
                        .map(|p| p.1)
                        .fold(f32::MIN, f32::max)
                        .ceil()
                        .min(SIDE as f32 - 1.0) as u32,
                );
                let edge = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| {
                    (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
                };
                let area = edge(points[0], points[1], points[2]);
                if area.abs() < 1e-6 {
                    continue;
                }
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let p = (x as f32 + 0.5, y as f32 + 0.5);
                        let w = [
                            edge(points[1], points[2], p) / area,
                            edge(points[2], points[0], p) / area,
                            edge(points[0], points[1], p) / area,
                        ];
                        if w.iter().any(|w| *w < 0.0) {
                            continue;
                        }
                        let z = w[0] * a[2] + w[1] * b[2] + w[2] * c[2];
                        let slot = (y * SIDE + x) as usize;
                        if z > height[slot] {
                            height[slot] = z;
                            let t = ((z - low) / (high - low)).clamp(0.0, 1.0);
                            let shade = (40.0 + 200.0 * t) as u8;
                            image.put_pixel(
                                x,
                                y,
                                image::Rgba([shade, shade, (shade as f32 * 0.85) as u8, 255]),
                            );
                        }
                    }
                }
            }
        }
        for step in (-12..=12).map(|i| i as f32 * 256.0) {
            let strength = if step == 0.0 {
                160
            } else if step % 1024.0 == 0.0 {
                90
            } else {
                40
            };
            let (gx, _) = pixel(step, 0.0);
            let (_, gy) = pixel(0.0, step);
            for t in 0..SIDE {
                for (x, y) in [(gx as u32, t), (t, gy as u32)] {
                    if x < SIDE && y < SIDE {
                        let mut p = *image.get_pixel(x, y);
                        p.0[2] = p.0[2].saturating_add(strength);
                        image.put_pixel(x, y, p);
                    }
                }
            }
        }
        let entities = sjk_entity::parse_entity_lump(bsp.entities()).expect("entities");
        for entity in &entities {
            let colour = match entity.classname() {
                Some(class) if class.starts_with("info_player_intermission") => [60, 255, 90, 255],
                Some(class) if class.starts_with("info_player") => [255, 60, 60, 255],
                _ => continue,
            };
            let Ok(Some(origin)) = entity.vector("origin") else {
                continue;
            };
            let (cx, cy) = pixel(origin[0], origin[1]);
            for dy in -4..=4 {
                for dx in -4..=4 {
                    let (x, y) = (cx as i64 + dx, cy as i64 + dy);
                    if (0..SIDE as i64).contains(&x) && (0..SIDE as i64).contains(&y) {
                        image.put_pixel(x as u32, y as u32, image::Rgba(colour));
                    }
                }
            }
        }
        std::fs::create_dir_all(directory()).expect("the shot directory");
        let path = directory().join("duel6-plan.png");
        image.save(&path).expect("write the plan");
        println!("heights {low} to {high}; {}", path.display());
    }

    /// The menu's camera tour on duel6 as authored: each shot's start and end
    /// side by side, a row a shot, checked to start and end in open air with
    /// nothing solid between.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_tour() {
        on_big_stack(|| {
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [960, 540], None, &[]) else {
                return;
            };
            let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let _ = frame(&mut gpu, 30);
            let mut images = Vec::new();
            for (index, shot) in shots.iter().enumerate() {
                for origin in [shot.from, shot.to] {
                    let (yaw, pitch) = look(origin, shot.at);
                    aim(&mut gpu, origin, yaw, pitch);
                    images.push(frame(&mut gpu, 6));
                }
                let trace = gpu.bsp.trace_box(
                    shot.from,
                    shot.to,
                    sjk_bsp::Aabb::new([-8.0; 3], [8.0; 3]).expect("a box"),
                    1,
                );
                println!(
                    "{index:2}: from {:?} to {:?} at {:?}{}",
                    shot.from,
                    shot.to,
                    shot.at,
                    if trace.fraction < 1.0 || trace.start_solid {
                        " BLOCKED"
                    } else {
                        ""
                    }
                );
            }
            println!("{}", sheet(&images, 4, 480, "duel6-tour").display());
        });
    }

    /// Every tour shot halfway through its glide, full size and without a
    /// menu: the site's home page plays a few of them behind its own ring
    /// (`site/screenshots/README.md`).
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_site_backdrops() {
        on_big_stack(|| {
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], None, &[])
            else {
                return;
            };
            let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let _ = frame(&mut gpu, 30);
            for (index, shot) in shots.iter().enumerate() {
                let origin = std::array::from_fn(|axis| (shot.from[axis] + shot.to[axis]) / 2.0);
                let (yaw, pitch) = look(origin, shot.at);
                aim(&mut gpu, origin, yaw, pitch);
                println!(
                    "{}",
                    shoot(&mut gpu, 8, &format!("site-tour-{index}")).display()
                );
            }
        });
    }

    /// The SJK UI over the live duel6, as a player sees it: the main page on
    /// the tour's first shot, later shots of the tour (the menu's clock moved
    /// on), and Settings over the tour (Interface, and Network with the SJK identity
    /// key's row).
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_menu() {
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            // The donation goal as the hub would answer it: 80 of 200 euros this month.
            crate::donation_goal::set_for_shot(Some(sjk_identity::DonationGoal {
                url: sjk_identity::donations::DEFAULT_URL.to_owned(),
                currency: "EUR".to_owned(),
                goal_cents: 20_000,
                raised_cents: 8_000,
                monthly: true,
            }));
            // Past the first shot's fade-in.
            let _ = frame(&mut gpu, 20);
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            shoot(&mut gpu, 4, "duel6-menu");
            for later in 1..3 {
                // To the next shot, then past its fade-in.
                gpu.ui_epoch -= std::time::Duration::from_millis(15_000);
                let _ = frame(&mut gpu, 1);
                gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
                shoot(&mut gpu, 4, &format!("duel6-menu-{later}"));
            }
            if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
                menu.open_sjk_settings(console, 7, crate::player_menu::ReturnTarget::MainMenu);
            }
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            shoot(&mut gpu, 4, "duel6-menu-settings");
            // Network, with the SJK identity key's row, the key itself hidden.
            if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
                menu.open_sjk_settings(console, 12, crate::player_menu::ReturnTarget::MainMenu);
            }
            shoot(&mut gpu, 4, "duel6-menu-settings-network");
            if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
                menu.open_sjk_settings(console, 7, crate::player_menu::ReturnTarget::MainMenu);
            }
            // Key bindings, then one of them awaiting its key.
            for (capture, name) in [
                (false, "duel6-menu-keys"),
                (true, "duel6-menu-keys-capture"),
            ] {
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    menu.sjk_keys_for_shot(console, "+attack", capture);
                }
                shoot(&mut gpu, 8, name);
            }
        });
    }

    /// Settings' changed dots: Gameplay with several rows off their defaults
    /// (short and long names) at 1080p, at 4K, and with the player's larger,
    /// wider-spaced menu text; then Key bindings with a few keys rebound.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_settings_changed_dots() {
        for (size, style, suffix) in [
            ([1920, 1080], None, "1080p"),
            ([3840, 2160], None, "4k"),
            ([1920, 1080], Some(("1.2", "0.1")), "styled"),
        ] {
            on_big_stack(move || {
                let menu = menu::ClientMenu::new(true, String::new());
                let mut cvars = vec![
                    ("ui_menuStyle", "sjk"),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                    ("cg_simpleItems", "1"),
                    ("cg_forceModel", "1"),
                    ("cg_saberTrail", "0"),
                    ("cg_shieldBrightness", "3"),
                    ("cg_spProtAbsColor", "1"),
                    ("cg_remaps", "0"),
                    ("cg_thirdPersonTargetDamp", "0.75"),
                    ("cg_errorDecay", "250"),
                ];
                if let Some((scale, tracking)) = style {
                    cvars.push((crate::text::style::SCALE_CVAR, scale));
                    cvars.push((crate::text::style::TRACKING_CVAR, tracking));
                }
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                else {
                    return;
                };
                let _ = frame(&mut gpu, 4);
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    menu.open_sjk_settings(console, 6, crate::player_menu::ReturnTarget::MainMenu);
                }
                gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
                let name = format!("settings-changed-dots-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                if let Some(console) = gpu.console.as_mut() {
                    console.rebind_action("+back", 0, "K");
                    console.rebind_action("+movedown", 1, "J");
                    console.rebind_action("+speed", 1, "N");
                    console.rebind_action("+left", 1, "H");
                }
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    menu.sjk_keys_for_shot(console, "+forward", false);
                }
                let name = format!("keys-changed-dots-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
            });
        }
    }

    /// A new profile's first start: no menu style saved, so the SJK UI, and
    /// First setup opening by itself on its Menu style row; then the style
    /// switched to classic from there, First setup staying.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_first_setup() {
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &[])
            else {
                return;
            };
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            let path = shoot(&mut gpu, 20, "duel6-first-setup-sjk");
            println!("{}", path.display());
            // The row that imports another client's config.
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.select_setting_for_shot(crate::settings::IMPORT_ROW);
            }
            let path = shoot(&mut gpu, 6, "duel6-first-setup-import");
            println!("{}", path.display());
            if let Some(console) = gpu.console.as_mut() {
                console.set_cvar(crate::menu::style::CVAR, "classic");
            }
            let path = shoot(&mut gpu, 20, "duel6-first-setup-classic");
            println!("{}", path.display());
            // The Import page that row opens (without its file dialog, which a shot
            // cannot show), the SJK UI's card in both menu styles: empty, with a
            // file's parts, and after a file that cannot be read.
            if let Some(console) = gpu.console.as_mut() {
                console.open_config_import(None);
            }
            let path = shoot(&mut gpu, 6, "duel6-import-page-classic");
            println!("{}", path.display());
            if let Some(console) = gpu.console.as_mut() {
                console.set_cvar(crate::menu::style::CVAR, "sjk");
            }
            let path = shoot(&mut gpu, 6, "duel6-import-page");
            println!("{}", path.display());
            let directory = tempfile::tempdir().expect("a temporary folder");
            let config = directory.path().join("jampconfig.cfg");
            std::fs::write(
                &config,
                "unbindall\nbind w \"+forward\"\nbind NOSUCHKEY \"+use\"\n\
                 seta name \"^1Sol^7Fox\"\nseta model \"kyle/default\"\nseta cg_fov \"100\"\n\
                 seta com_maxfps \"333\"\n",
            )
            .expect("write the config");
            if let Some(console) = gpu.console.as_mut() {
                console.open_config_import(Some(&config));
            }
            let path = shoot(&mut gpu, 6, "duel6-import-page-loaded");
            println!("{}", path.display());
            // Imported: the gameplay FPS step with the file's 333 picked, then on its
            // own (help_gameplay_fps) with Other typed.
            if let Some(console) = gpu.console.as_mut() {
                console.import_for_shot();
            }
            let path = shoot(&mut gpu, 6, "duel6-gameplay-fps-imported");
            println!("{}", path.display());
            if let Some(console) = gpu.console.as_mut() {
                console.config_import_done_for_shot();
            }
            let path = shoot(&mut gpu, 6, "duel6-import-page-done");
            println!("{}", path.display());
            if let Some(console) = gpu.console.as_mut() {
                console.open_gameplay_fps();
            }
            let path = shoot(&mut gpu, 6, "duel6-gameplay-fps");
            println!("{}", path.display());
            if let Some(console) = gpu.console.as_mut() {
                console.type_gameplay_fps_for_shot("400");
            }
            let path = shoot(&mut gpu, 6, "duel6-gameplay-fps-other");
            println!("{}", path.display());
            // Opened by itself: the inputs were not going through.
            if let Some(console) = gpu.console.as_mut() {
                console.set_frame_cap(0);
                console.open_gameplay_fps_alert();
            }
            let path = shoot(&mut gpu, 6, "duel6-gameplay-fps-alert");
            println!("{}", path.display());
            if let Some(console) = gpu.console.as_mut() {
                console.open_config_import(Some(std::path::Path::new(
                    "C:/no/such/folder/jampconfig.cfg",
                )));
            }
            let path = shoot(&mut gpu, 6, "duel6-import-page-failed");
            println!("{}", path.display());
        });
    }

    /// A 4K monitor's video modes, for the resolution list's shots.
    fn shot_monitor() -> crate::settings::MonitorModes {
        crate::settings::MonitorModes {
            sizes: vec![
                [3840, 2160],
                [2560, 1600],
                [2560, 1440],
                [1920, 1200],
                [1920, 1080],
                [1680, 1050],
                [1600, 900],
                [1440, 900],
                [1366, 768],
                [1280, 1024],
                [1280, 960],
                [1280, 800],
                [1280, 720],
                [1024, 768],
                [800, 600],
                [640, 480],
            ],
            desktop: Some([3840, 2160]),
            exclusive: true,
        }
    }

    /// The settings in the SJK UI's look, at 1080p and 4K with Sol's
    /// larger menu text (`ui_textScale 1.2`): the resolution list as the SJK
    /// UI's card over its Settings and over a classic+ panel, then the
    /// classic Graphics page's renderer route, which opens the SJK UI's
    /// Settings on Graphics.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_settings_sjk_only() {
        for (size, suffix) in [([1920, 1080], "1080p"), ([3840, 2160], "4k")] {
            on_big_stack(move || {
                let menu = menu::ClientMenu::new(true, String::new());
                let cvars = [
                    ("ui_menuStyle", "sjk"),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                    (crate::text::style::SCALE_CVAR, "1.2"),
                ];
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                else {
                    return;
                };
                // A 4K monitor's desktop size in use: more sizes than the card
                // shows, the longest tags.
                if let Some(console) = gpu.console.as_mut() {
                    console.set_cvar("r_resolution", "3840x2160");
                }
                let _ = frame(&mut gpu, 4);
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    menu.open_sjk_settings(console, 1, crate::player_menu::ReturnTarget::MainMenu);
                    menu.set_monitor_modes(shot_monitor(), console);
                    menu.resolutions_for_shot(console);
                }
                gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
                let name = format!("resolution-list-sjk-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                if let Some(console) = gpu.console.as_mut() {
                    console.set_cvar(crate::menu::style::CVAR, "classic");
                }
                let _ = frame(&mut gpu, 4);
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    menu.open_classic_panel(
                        console,
                        crate::menu::classic::layout::Page::Setup,
                        crate::menu::classic::layout::Entry::Video,
                        crate::menu::classic::panel::Frame::Main,
                        crate::player_menu::ReturnTarget::MainMenu,
                    );
                    menu.set_monitor_modes(shot_monitor(), console);
                    menu.resolutions_for_shot(console);
                }
                let name = format!("resolution-list-classic-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    menu.classic_renderer_for_shot(
                        console,
                        crate::menu::classic::panel::Frame::Main,
                    );
                }
                gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
                let name = format!("classic-renderer-route-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
            });
        }
    }

    /// The player screen on duel6's stage: the model standing where the
    /// route puts it, seen from the route's camera, in the style `style`.
    fn duel6_player(style: &'static str, name: &'static str) {
        on_big_stack(move || {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", style),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 4);
            if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
                menu.open_player(console, crate::player_menu::ReturnTarget::MainMenu);
            }
            // Through the cut to the stage.
            let _ = frame(&mut gpu, 2);
            gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
            let path = shoot(&mut gpu, 6, name);
            println!("{}", path.display());
            // The Saber page (the saber thrown out to its shot), the Force page.
            // Saber's row 4 is the first blade's colour (Style, Search, Hilt, Blade).
            for (page, row, suffix) in [(1, 4, "saber"), (2, 4, "force")] {
                if let Some(menu) = gpu.client_menu.as_mut() {
                    menu.player_page_for_shot(page, row);
                }
                let _ = frame(&mut gpu, 2);
                gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
                let path = shoot(&mut gpu, 30, &format!("{name}-{suffix}"));
                println!("{}", path.display());
            }
            // The Force page with the pointer on Heal's second level: its marks
            // and channel charged, the spark on its way, the box on Heal.
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.player_hover_level_for_shot(0, 2);
            }
            let path = shoot(&mut gpu, 4, &format!("{name}-force-hover"));
            println!("{}", path.display());
            // The Force page on a server with rules: rank, free saber, powers
            // off (the longest list a row shows) and the team powers' note.
            if let Some(menu) = gpu.client_menu.as_mut() {
                let longest = [5, 7, 9, 11, 12, 15, 16, 17].map(|bit| 1_u32 << bit);
                menu.player_server_rules_for_shot(sjk_client::ForceLegalizeRules {
                    max_rank: 5,
                    free_saber: true,
                    team_side: None,
                    gametype: 0,
                    disabled_mask: longest.iter().fold(0, |mask, bit| mask | bit),
                });
                menu.player_page_for_shot(2, 4);
            }
            let _ = frame(&mut gpu, 2);
            let path = shoot(&mut gpu, 6, &format!("{name}-force-server"));
            println!("{}", path.display());
            // Dual sabers: the two hands' hilt lists side by side.
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.player_dual_for_shot();
                menu.player_page_for_shot(1, 2);
            }
            let _ = frame(&mut gpu, 2);
            gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
            let path = shoot(&mut gpu, 30, &format!("{name}-saber-dual"));
            println!("{}", path.display());
        });
    }

    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_player_sjk() {
        duel6_player("sjk", "duel6-player-sjk");
    }

    /// SJK's pages in the SJK UI over the live duel6: What's new, Update
    /// (a newer release pretended out) and Identity (switched off: the shots'
    /// profile never registers).
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_pages() {
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            crate::update::pretend_available("2026.1008.1");
            type Opens = fn(&mut console::ViewerConsole);
            let pages: [(&str, Opens); 3] = [
                (
                    "duel6-page-whats-new",
                    console::ViewerConsole::open_changelog,
                ),
                (
                    "duel6-page-update",
                    console::ViewerConsole::open_update_panel,
                ),
                (
                    "duel6-page-identity",
                    console::ViewerConsole::open_identity_panel,
                ),
            ];
            for (name, open_page) in pages {
                if let Some(console) = gpu.console.as_mut() {
                    open_page(console);
                }
                let path = shoot(&mut gpu, 6, name);
                println!("{}", path.display());
            }
        });
    }

    /// The debug panel's test list (`debug_panel`), drawn in the SJK UI's look in
    /// every menu style: the first entry with notes chosen on the All tab, a tick's
    /// outcome bottom left. At 1080p with the plain text style and at 4K with Sol's
    /// (`ui_textScale 1.2`), with `ui_menuStyle sjk` and `classic`.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_debug_panel() {
        for style in ["sjk", "classic"] {
            for (size, scale, suffix) in [
                ([1920, 1080], None, "1080p"),
                ([3840, 2160], Some("1.2"), "4k-styled"),
            ] {
                on_big_stack(move || {
                    let menu = menu::ClientMenu::new(true, String::new());
                    let mut cvars = vec![
                        ("ui_menuStyle", style),
                        (crate::settings::quick::HIDE_CVAR, "1"),
                    ];
                    if let Some(scale) = scale {
                        cvars.push((crate::text::style::SCALE_CVAR, scale));
                    }
                    let Some((mut gpu, _profile)) =
                        open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                    else {
                        return;
                    };
                    let _ = frame(&mut gpu, 10);
                    if let Some(console) = gpu.console.as_mut() {
                        console.debug_panel_for_shot();
                    }
                    let name = format!("duel6-debug-panel-{style}-{suffix}");
                    println!("{}", shoot(&mut gpu, 6, &name).display());
                });
            }
        }
    }

    /// A made-up profile with medals, worn names, a record and half the achievements,
    /// for the Profile page's shots.
    pub(super) fn profile_preview() -> crate::console::profile_panel::Preview {
        use crate::console::profile_panel::Preview;
        let medal = |id: &str, count, note: &str| sjk_identity::Medal {
            id: id.to_owned(),
            count,
            awarded: 1_791_336_225,
            note: note.to_owned(),
        };
        let me = sjk_identity::Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "44f3d0b36c9b2510".to_owned(),
            key: String::new(),
            name: "^1Sol^7Vulpes".to_owned(),
            bio: "Saber duelist, mostly staff and yellow. On JoF most evenings.

Found the fog bug, ask me about it!"
                .to_owned(),
            verified: true,
            staff: false,
            created: 1_791_250_000,
            names: ["^1Sol^7Vulpes", "^4Fox", "Sol"]
                .iter()
                .map(|name| sjk_identity::WornName {
                    name: (*name).to_owned(),
                    first_seen: 0,
                    last_seen: 0,
                })
                .collect(),
            medals: vec![
                medal("early_tester", 1, "Tested the first alphas with us"),
                medal(
                    "bug_hunter",
                    2,
                    "The fog that followed the camera, and the drain",
                ),
            ],
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: vec![sjk_identity::Unlock {
                id: "saber_sun".to_owned(),
                granted: 1_791_336_225,
                note: String::new(),
                count: 1,
                medal: None,
            }],
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        };
        let snapshot = sjk_identity::Snapshot {
            loot: Vec::new(),
            opening: None,
            reveal: None,
            status: sjk_identity::Status::Online,
            key_id: me.key_id.clone(),
            local_key_id: me.key_id.clone(),
            me: Some(me),
            server: None,
            players: Vec::new(),
            profiles: std::collections::HashMap::new(),
            model_checks: std::collections::HashMap::new(),
            profiles_revision: 0,
            notice: None,
            revision: 0,
            report: None,
            note: None,
            player_report: None,
            avatar: None,
            look_outcome: None,
            packs_revision: 0,
            assets_note: None,
            crashes_sent: 0,
            crash_note: None,
            holocrons: None,
            log_requests: Vec::new(),
            log_answer: None,
        };
        let standings = crate::achievements::ALL
            .iter()
            .enumerate()
            .map(|(index, kind)| crate::achievements::Standing {
                kind,
                progress: match index % 3 {
                    0 => kind.goal,
                    1 => kind.goal * 2 / 5,
                    _ => 0,
                },
                unlocked: (index % 3 == 0).then_some(1_791_336_225 + index as i64 * 3_600),
            })
            .collect();
        let record = vec![
            ("Players defeated", "412".to_owned()),
            ("Saber kills", "388".to_owned()),
            ("Best streak", "9".to_owned()),
            ("Duels won", "57".to_owned()),
            ("Flags captured", "6".to_owned()),
            ("Maps played", "14".to_owned()),
            ("Servers played", "3".to_owned()),
            ("Time played", "23 h 40 min".to_owned()),
        ];
        Preview {
            snapshot,
            standings,
            record,
        }
    }

    /// The Profile page over the live duel6: a made-up profile with medals, worn names, a
    /// record and half the achievements, the bio being written, the page with the
    /// identity off (the shots' own), and a 4:3 window.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_profile() {
        let preview = profile_preview;
        on_big_stack(move || {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            for size in [[1920, 1080], [1440, 1080]] {
                let Some((mut gpu, _profile)) = open(
                    "maps/mp/duel6.bsp",
                    size,
                    Some(menu::ClientMenu::new(true, String::new())),
                    &cvars,
                ) else {
                    return;
                };
                let _ = frame(&mut gpu, 10);
                let suffix = if size[0] == 1920 { "" } else { "-4x3" };
                if let Some(console) = gpu.console.as_mut() {
                    console.open_profile_panel();
                    console.preview_profile(preview());
                }
                for _ in 0..240 {
                    let _ = frame(&mut gpu, 1);
                    if crate::medals::art::decoded().is_some() {
                        break;
                    }
                }
                println!(
                    "{}",
                    shoot(&mut gpu, 6, &format!("duel6-profile{suffix}")).display()
                );
                if let Some(console) = gpu.console.as_mut() {
                    console.profile_type_bio(
                        "Writing my bio here, with ^3colour^7 and a second line
like this one.",
                    );
                }
                println!(
                    "{}",
                    shoot(&mut gpu, 6, &format!("duel6-profile-typing{suffix}")).display()
                );
            }
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            if let Some(console) = gpu.console.as_mut() {
                console.open_profile_panel();
            }
            println!("{}", shoot(&mut gpu, 6, "duel6-profile-off").display());
        });
    }

    /// The Staff page over the live duel6: players found, the player's own medals and
    /// achievements, another player chosen, and Clear all waiting for its second press.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_staff() {
        let profile = |key: &str, name: &str| sjk_identity::Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: key.to_owned(),
            key: String::new(),
            name: name.to_owned(),
            bio: String::new(),
            verified: false,
            staff: false,
            created: 1_791_250_000,
            names: Vec::new(),
            medals: Vec::new(),
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        };
        let medal = |id: &str, count, note: &str| sjk_identity::Medal {
            id: id.to_owned(),
            count,
            awarded: 1_791_336_225,
            note: note.to_owned(),
        };
        let me = sjk_identity::Profile {
            staff: true,
            verified: true,
            medals: vec![
                medal("early_tester", 1, "Tested the first alphas with us"),
                medal(
                    "bug_hunter",
                    2,
                    "The fog that followed the camera, and the drain",
                ),
            ],
            achievements: crate::achievements::ALL
                .iter()
                .step_by(2)
                .map(|kind| sjk_identity::Achievement {
                    id: kind.id.to_owned(),
                    progress: kind.goal,
                    goal: kind.goal,
                    unlocked: 1_791_336_225,
                })
                .collect(),
            ..profile("44f3d0b36c9b2510", "^1Sol^7Vulpes")
        };
        let players = vec![
            me.clone(),
            sjk_identity::Profile {
                verified: true,
                medals: vec![medal("early_contributor", 1, "")],
                unlocks: vec![sjk_identity::Unlock {
                    id: "saber_sun".to_owned(),
                    granted: 1_791_336_225,
                    note: String::new(),
                    count: 1,
                    medal: None,
                }],
                keys: vec!["9a0c51e2b7d34f80".to_owned(), "5d4c3b2a19087f6e".to_owned()],
                ..profile("9a0c51e2b7d34f80", "^5Creyon")
            },
            profile("1f2e3d4c5b6a7980", "^3Lumaya"),
            profile("0b1c2d3e4f5a6b7c", "Padawan^1Fox"),
        ];
        let staff = sjk_identity::StaffState {
            serial: 3,
            players,
            message: "4 players seen lately".to_owned(),
            failed: false,
            busy: false,
            ..sjk_identity::StaffState::default()
        };
        on_big_stack(move || {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            if let Some(console) = gpu.console.as_mut() {
                console.preview_staff(me.clone(), staff.clone());
            }
            for _ in 0..240 {
                let _ = frame(&mut gpu, 1);
                if crate::medals::art::decoded().is_some() {
                    break;
                }
            }
            println!("{}", shoot(&mut gpu, 6, "duel6-staff").display());
            // Another player, holding the Sun blade: Relock offered, Unlock not.
            if let Some(console) = gpu.console.as_mut() {
                console.staff_choose("9a0c51e2b7d34f80");
            }
            println!("{}", shoot(&mut gpu, 6, "duel6-staff-other").display());
            // Their keys, one linked, and Fox's key ready to merge in.
            if let Some(console) = gpu.console.as_mut() {
                console.staff_merge("0b1c2d3e4f5a6b7c", false);
            }
            println!("{}", shoot(&mut gpu, 6, "duel6-staff-keys").display());
            // The confirmation: who is kept, who goes, for good.
            if let Some(console) = gpu.console.as_mut() {
                console.staff_merge("0b1c2d3e4f5a6b7c", true);
            }
            println!("{}", shoot(&mut gpu, 6, "duel6-staff-merge").display());
        });
    }

    /// Credits in the SJK UI over the live duel6: the top (the sun, the rail,
    /// Sol's panel), Creyon's panel with its folds open and his medal, Lumaya's
    /// panel, and the end of the page (the cards in twos and the notice).
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_credits() {
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            if let Some(console) = gpu.console.as_mut() {
                console.open_credits();
                console.credits_mut().settle();
            }
            // The emblem's layers and the medals' pictures decode on workers.
            let _ = frame(&mut gpu, 40);
            println!("{}", shoot(&mut gpu, 4, "duel6-page-credits").display());
            for (person, name) in [
                (1, "duel6-page-credits-creyon"),
                (2, "duel6-page-credits-lumaya"),
            ] {
                if let Some(console) = gpu.console.as_mut() {
                    console.credits_mut().unfold(person);
                    console.credits_mut().settle();
                }
                let _ = frame(&mut gpu, 2);
                if let Some(console) = gpu.console.as_mut() {
                    console.credits_mut().scroll_to_person(person);
                }
                println!("{}", shoot(&mut gpu, 6, name).display());
            }
            // Creyon's work unfolded: the days, his pull requests with their
            // tags, and the commits of the first with several.
            if let Some(console) = gpu.console.as_mut() {
                console.credits_mut().scroll_to_person(1);
                console.credits_mut().scroll_on(700.0);
            }
            println!(
                "{}",
                shoot(&mut gpu, 6, "duel6-page-credits-creyon-work").display()
            );
            if let Some(console) = gpu.console.as_mut() {
                console.open_credits();
                console.credits_mut().settle();
            }
            let _ = frame(&mut gpu, 2);
            if let Some(console) = gpu.console.as_mut() {
                console.credits_mut().scroll_to(100_000.0);
            }
            println!("{}", shoot(&mut gpu, 6, "duel6-page-credits-end").display());
        });
    }

    /// The SJK UI's server browser over the live duel6, on made-up servers:
    /// the list with the first server's map, numbers and players; a search
    /// with the left column's game type taking the keys; the password prompt.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_browser() {
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.browser_for_shot();
            }
            // The chosen server's levelshot decodes on its worker.
            for _ in 0..240 {
                let _ = frame(&mut gpu, 1);
                if gpu
                    .client_menu
                    .as_ref()
                    .is_some_and(menu::ClientMenu::browser_picture_settled)
                {
                    break;
                }
            }
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            println!("{}", shoot(&mut gpu, 4, "duel6-browser").display());
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.browser_search_for_shot("duel", 5);
            }
            println!("{}", shoot(&mut gpu, 8, "duel6-browser-search").display());
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.browser_search_for_shot("", 0);
                menu.browser_password_for_shot("saber");
            }
            println!("{}", shoot(&mut gpu, 4, "duel6-browser-password").display());
        });
    }

    /// The SJK UI's scoreboard over the live duel6, on made-up matches (no
    /// server): capture the flag with the classic menus and the look chosen on
    /// its own (`cg_scoreboardStyle sjk`, which loads the UI's families), then
    /// with the SJK UI's menus and the default `auto`: free for all, a full
    /// server, a duel and a power duel.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_scoreboard() {
        use crate::scoreboard::shot::Match;
        on_big_stack(|| {
            let cvars = [("ui_menuStyle", "classic"), ("cg_scoreboardStyle", "sjk")];
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], None, &cvars)
            else {
                return;
            };
            // The menu tour's first view: down the west wing at the tower.
            let tour = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let (yaw, pitch) = look(tour[0].from, tour[0].at);
            aim(&mut gpu, tour[0].from, yaw, pitch);
            let _ = frame(&mut gpu, 20);
            gpu.scoreboard.show_for_shot(Match::Capture);
            println!("{}", shoot(&mut gpu, 16, "duel6-scoreboard-ctf").display());
            if let Some(console) = gpu.console.as_mut() {
                console.set_cvar("ui_menuStyle", "sjk");
                console.set_cvar("cg_scoreboardStyle", "auto");
            }
            for (game, name) in [
                (Match::Free, "duel6-scoreboard-ffa"),
                (Match::Crowd, "duel6-scoreboard-full"),
                (Match::Duel, "duel6-scoreboard-duel"),
                (Match::PowerDuel, "duel6-scoreboard-power-duel"),
            ] {
                gpu.scoreboard.show_for_shot(game);
                println!("{}", shoot(&mut gpu, 16, name).display());
            }
            // The full server without compact rows: two lists side by side.
            if let Some(console) = gpu.console.as_mut() {
                console.set_cvar(crate::scoreboard::style::COMPACT_CVAR, "0");
            }
            gpu.scoreboard.show_for_shot(Match::Crowd);
            println!(
                "{}",
                shoot(&mut gpu, 16, "duel6-scoreboard-full-split").display()
            );
            gpu.scoreboard.end_shot();
            drop(gpu);
            // A 4:3 window: the frame scales down to its width.
            let cvars = [("ui_menuStyle", "sjk")];
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1440, 1080], None, &cvars)
            else {
                return;
            };
            aim(&mut gpu, tour[0].from, yaw, pitch);
            let _ = frame(&mut gpu, 20);
            gpu.scoreboard.show_for_shot(Match::Crowd);
            println!("{}", shoot(&mut gpu, 16, "duel6-scoreboard-4x3").display());
        });
    }

    /// The new medal pop-up over the main page on the live duel6, in the SJK UI's look
    /// at moments of its ceremony (the flight, the landing, the burst, the band of
    /// light, standing still, a soft glint, leaving), each held still, then in the
    /// classic+ look; the whole medal's picture decoded on its worker and uploaded the
    /// first time it is drawn.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_medal_popup() {
        use crate::medal_popup::MedalPopup;
        use crate::medal_popup::award::{ENTRANCE, EXIT};
        on_big_stack(|| {
            let awards = crate::medals::awards(&[sjk_identity::Medal {
                id: "bug_hunter".to_owned(),
                count: 2,
                awarded: 1_791_336_225,
                note: "The fog that followed the camera floor.".to_owned(),
            }]);
            let moments = [
                (0.3, None, "flight"),
                (0.55, None, "landing"),
                (0.8, None, "burst"),
                (1.3, None, "sweep"),
                (ENTRANCE, None, "still"),
                (ENTRANCE + 1.95, None, "glint"),
                (ENTRANCE + 2.0, Some(EXIT * 0.5), "leaving"),
            ];
            for style in ["sjk", "classic"] {
                let menu = menu::ClientMenu::new(true, String::new());
                let cvars = [
                    ("ui_menuStyle", style),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                let Some((mut gpu, _profile)) =
                    open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
                else {
                    return;
                };
                let _ = frame(&mut gpu, 10);
                crate::medals::art::request();
                for _ in 0..240 {
                    let _ = frame(&mut gpu, 1);
                    if crate::medals::art::decoded().is_some() {
                        break;
                    }
                }
                let shown: &[_] = if style == "sjk" {
                    &moments
                } else {
                    &moments[2..5]
                };
                for (at, leaving, name) in shown {
                    gpu.medal_popup = MedalPopup::preview(awards.clone(), *at, *leaving);
                    let name = format!("duel6-medal-popup-{style}-{name}");
                    println!("{}", shoot(&mut gpu, 6, &name).display());
                }
            }
        });
    }

    /// The unlock pop-up over play on the live duel6 (no menu), at 1920x1080 and
    /// 1440x1080: an achievement's card coming in (the ring sweeping, the burst, the
    /// glint), held and leaving, each moment held still, and a sheet of the top of the
    /// screen at every moment; a medal's, each holocron tier's and an update's card,
    /// coming in and held, on a sheet; then an achievement's over the SJK UI's main page.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_unlock_toast() {
        use crate::unlock_toast::{ENTER, HOLD, LEAVE, Unlock, UnlockToast};
        on_big_stack(|| {
            let streak =
                Unlock::Achievement(crate::achievements::find("streak_5").expect("Rampage"));
            let lightning = Unlock::Achievement(
                crate::achievements::find("dark_side_25").expect("Unlimited Power"),
            );
            let mut others = vec![Unlock::Medal {
                medal: crate::medals::Medal::BugHunter,
                count: 2,
            }];
            for (index, tier) in crate::holocrons::TIERS.iter().enumerate() {
                others.push(Unlock::Holocron {
                    tier,
                    id: index as u64,
                    gift: index == 0,
                });
            }
            others.push(Unlock::Update {
                version: crate::update::Version::new("2026.1011.1").expect("a version"),
                manual: false,
            });
            let moments = [
                (0.1, "entering"),
                (0.25, "landing"),
                (0.45, "sweep"),
                (0.7, "burst"),
                (0.95, "glint"),
                (2.5, "held"),
                (ENTER + HOLD + LEAVE * 0.4, "leaving"),
            ];
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            for (size, prefix) in [
                ([1920, 1080], "duel6-achievement"),
                ([1440, 1080], "duel6-achievement-4x3"),
            ] {
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, None, &cvars)
                else {
                    return;
                };
                let tour = menu_backdrop::tour_for("Yavin Training Grounds").expect("the tour");
                let (yaw, pitch) = look(tour[0].from, tour[0].at);
                aim(&mut gpu, tour[0].from, yaw, pitch);
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                let _ = frame(&mut gpu, 60);
                std::fs::create_dir_all(directory()).expect("the shot directory");
                let mut tops = Vec::new();
                for (at, name) in moments {
                    gpu.unlock_toast = UnlockToast::preview(&[streak, lightning], at);
                    let image = frame(&mut gpu, 3);
                    let path = directory().join(format!("{prefix}-{name}.png"));
                    image.save(&path).expect("write the shot");
                    println!("{}", path.display());
                    let width = image.width().min(760);
                    let x = (image.width() - width) / 2;
                    tops.push(image::imageops::crop_imm(&image, x, 50, width, 240).to_image());
                }
                println!(
                    "{}",
                    sheet(&tops, 2, 760, &format!("{prefix}-moments")).display()
                );
                let mut cards = Vec::new();
                for unlock in &others {
                    for at in [0.7, 2.5] {
                        gpu.unlock_toast = UnlockToast::preview(&[*unlock], at);
                        let image = frame(&mut gpu, 3);
                        let width = image.width().min(760);
                        let x = (image.width() - width) / 2;
                        cards.push(image::imageops::crop_imm(&image, x, 50, width, 240).to_image());
                    }
                }
                println!(
                    "{}",
                    sheet(&cards, 2, 760, &format!("{prefix}-medal-holocrons")).display()
                );
            }
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            gpu.unlock_toast = UnlockToast::preview(&[lightning], 2.5);
            println!("{}", shoot(&mut gpu, 6, "duel6-achievement-menu").display());
        });
    }

    /// Every scoreboard look with the SJK emblem and medal ribbon bars after the names
    /// of the made-up players the hub knows: classic on a free for all, the SJK UI's on
    /// a free for all and a duel (the bars on a duelist's card).
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_scoreboard_medals() {
        use crate::scoreboard::shot::Match;
        on_big_stack(|| {
            for style in ["classic", "sjk"] {
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], None, &[])
                else {
                    return;
                };
                // Set once the profile's start has moved old defaults on.
                if let Some(console) = gpu.console.as_mut() {
                    console.set_cvar("cg_scoreboardStyle", style);
                }
                let tour = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
                let (yaw, pitch) = look(tour[0].from, tour[0].at);
                aim(&mut gpu, tour[0].from, yaw, pitch);
                let _ = frame(&mut gpu, 20);
                let games: &[(Match, &str)] = if style == "sjk" {
                    &[(Match::Free, "ffa"), (Match::Duel, "duel")]
                } else {
                    &[(Match::Free, "ffa")]
                };
                for (game, name) in games {
                    gpu.scoreboard.show_for_shot(*game);
                    let name = format!("duel6-medals-scoreboard-{style}-{name}");
                    println!("{}", shoot(&mut gpu, 16, &name).display());
                }
                gpu.scoreboard.end_shot();
            }
        });
    }

    /// The SJK UI's loading screen over the live duel6, on a made-up join of
    /// the JoF server: before the map is known (the tour behind), loading
    /// mp/ffa3 (its levelshot over the screen), and a failed join with and
    /// without the map known.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_loading() {
        use menu::classic::loading::{Stage, WorldStage};
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            // Past the tour's first fade-in.
            let _ = frame(&mut gpu, 20);
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            type State = (
                &'static str,
                Stage,
                bool,
                Option<WorldStage>,
                bool,
                Option<&'static str>,
            );
            let states: [State; 4] = [
                (
                    "duel6-loading-joining",
                    Stage::Challenging,
                    false,
                    None,
                    false,
                    None,
                ),
                (
                    "duel6-loading-map",
                    Stage::Loading,
                    true,
                    Some(WorldStage::Building),
                    true,
                    None,
                ),
                (
                    "duel6-loading-failed",
                    Stage::Loading,
                    true,
                    None,
                    false,
                    Some("server is full"),
                ),
                (
                    "duel6-loading-failed-early",
                    Stage::Connecting,
                    false,
                    None,
                    false,
                    Some("no answer from the server after 5 seconds"),
                ),
            ];
            for (name, stage, map, world, joined, error) in states {
                if let Some(menu) = gpu.client_menu.as_mut() {
                    menu.loading_for_shot(stage, map, world, joined, error);
                }
                // The levelshot decodes on its worker.
                for _ in 0..240 {
                    let _ = frame(&mut gpu, 1);
                    if gpu
                        .client_menu
                        .as_ref()
                        .is_some_and(menu::ClientMenu::loading_picture_settled)
                    {
                        break;
                    }
                }
                println!("{}", shoot(&mut gpu, 4, name).display());
            }
            // A server's change of map, its world (not the menu's) behind:
            // the navy ground instead, until the new map is named.
            gpu.is_menu_world = false;
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.loading_for_shot(Stage::Loading, true, None, true, None);
                menu.map_change_for_shot();
            }
            println!("{}", shoot(&mut gpu, 4, "duel6-loading-next-map").display());
        });
    }

    /// The SJK UI's player screen opened from a game, over duel6 as a match
    /// would show it (there is no server): no menu-map stage, so the model
    /// stands in its live preview right of the form; its three pages.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_character_in_game() {
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(false, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let (yaw, pitch) = look(shots[3].from, shots[3].at);
            aim(&mut gpu, shots[3].from, yaw, pitch);
            gpu.is_menu_world = false;
            if let Some(console) = gpu.console.as_mut() {
                console.close_for_connection();
            }
            let _ = frame(&mut gpu, 20);
            // As the in-game menu's Character opens it.
            gpu.open_player_menu_from_game();
            // The model and its preview load.
            let _ = frame(&mut gpu, 60);
            println!(
                "{}",
                shoot(&mut gpu, 4, "duel6-character-in-game").display()
            );
            for (page, row, suffix) in [(1, 2, "saber"), (2, 4, "force")] {
                if let Some(menu) = gpu.client_menu.as_mut() {
                    menu.player_page_for_shot(page, row);
                }
                let path = shoot(&mut gpu, 30, &format!("duel6-character-in-game-{suffix}"));
                println!("{}", path.display());
            }
        });
    }

    /// The Profile screen's tabs and the main page at Sol's text style
    /// (`ui_textScale 1.2`): every tab's name whole, on Character and on
    /// Achievements, and the main page's Profile line clear of the servers.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_profile_tabs_styled() {
        use crate::profile_hub::Tab;
        on_big_stack(|| {
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
                ("name", "^1Sol^7Vulpes"),
                (crate::text::style::SCALE_CVAR, "1.2"),
            ];
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            let _ = frame(&mut gpu, 20);
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.sjk_home_entry_for_shot(1);
            }
            println!(
                "{}",
                shoot(&mut gpu, 6, "duel6-menu-profile-styled").display()
            );
            if let Some(console) = gpu.console.as_mut() {
                console.preview_profile(profile_preview());
            }
            gpu.open_profile_hub(Tab::Character);
            let _ = frame(&mut gpu, 30);
            println!(
                "{}",
                shoot(&mut gpu, 4, "duel6-profile-tabs-styled-character").display()
            );
            gpu.profile_hub_show_for_shot(Tab::Profile);
            println!(
                "{}",
                shoot(&mut gpu, 6, "duel6-profile-tabs-styled-sjk-profile").display()
            );
        });
    }

    /// The SJK UI's Character creator over duel6, one shot per category: models,
    /// hats and capes of a plain model, then a species' head, torso, legs and skin.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_creator() {
        on_big_stack(|| {
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
                ("name", "^1Sol^7Vulpes"),
            ];
            let menu = menu::ClientMenu::new(false, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let (yaw, pitch) = look(shots[3].from, shots[3].at);
            aim(&mut gpu, shots[3].from, yaw, pitch);
            gpu.is_menu_world = false;
            gpu.game_menu = true;
            if let Some(console) = gpu.console.as_mut() {
                console.close_for_connection();
                console.preview_profile(profile_preview());
            }
            let _ = frame(&mut gpu, 20);
            gpu.open_profile_hub_from_game(None);
            let _ = frame(&mut gpu, 80);
            for (name, species, index) in [
                ("hats", false, 1),
                ("capes", false, 2),
                ("models", false, 0),
                ("head", true, 1),
                ("torso", true, 2),
                ("legs", true, 3),
                ("skin", true, 4),
            ] {
                if let Some(menu) = gpu.client_menu.as_mut() {
                    menu.creator_for_shot(species, index);
                }
                let _ = frame(&mut gpu, 40);
                println!(
                    "{}",
                    shoot(&mut gpu, 4, &format!("duel6-creator-{name}")).display()
                );
            }
        });
    }

    /// The SJK UI's Profile screen over duel6: opened from the game menu on its
    /// Character tab (the model in its live preview), then Ctrl+Tab through Saber,
    /// Force and SJK Profile (a made-up profile) and round to Character; the game menu's Profile opening again on the
    /// tab shown last; the picture panel with Browse...; a 4:3 window; and from the main
    /// page's Profile, on the menu map's stage. No canvas runs out of room.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_profile_screen() {
        use crate::profile_hub::Tab;
        on_big_stack(|| {
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
                ("name", "^1Sol^7Vulpes"),
            ];
            let sun = || sjk_identity::Unlock {
                id: "saber_sun".to_owned(),
                granted: 1_791_336_225,
                note: "Thanks for testing the blades with us".to_owned(),
                count: 1,
                medal: None,
            };
            let overflowed = |gpu: &GpuState| {
                gpu.console
                    .as_ref()
                    .is_some_and(crate::console::ViewerConsole::profile_pages_overflowed)
                    || gpu
                        .client_menu
                        .as_ref()
                        .is_some_and(menu::ClientMenu::player_overflowed)
            };
            for size in [[1920, 1080], [1440, 1080]] {
                let suffix = if size[0] == 1920 { "" } else { "-4x3" };
                let menu = menu::ClientMenu::new(false, String::new());
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                else {
                    return;
                };
                let shots =
                    menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
                let (yaw, pitch) = look(shots[3].from, shots[3].at);
                aim(&mut gpu, shots[3].from, yaw, pitch);
                gpu.is_menu_world = false;
                gpu.game_menu = true;
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                    console.preview_profile(profile_preview());
                    console.preview_collection(Some(vec![sun()]), 0.8);
                }
                let _ = frame(&mut gpu, 20);
                // As the game menu's Profile opens it the first time.
                gpu.open_profile_hub_from_game(None);
                assert_eq!(gpu.profile_hub_tab(), Some(Tab::Character));
                let _ = frame(&mut gpu, 60);
                let name = format!("duel6-profile-screen-character{suffix}");
                println!("{}", shoot(&mut gpu, 4, &name).display());
                assert!(!overflowed(&gpu), "Character{suffix}");
                for tab in &crate::profile_hub::Screen::Profile.tabs()[1..] {
                    gpu.profile_hub_next_for_shot();
                    assert_eq!(gpu.profile_hub_tab(), Some(*tab));
                    let part = tab.label().to_lowercase().replace(' ', "-");
                    let name = format!("duel6-profile-screen-{part}{suffix}");
                    println!("{}", shoot(&mut gpu, 6, &name).display());
                    assert!(!overflowed(&gpu), "{tab:?}{suffix}");
                }
                // Round to Character: the console's page closes, the game menu waits
                // under it on Profile.
                gpu.profile_hub_next_for_shot();
                assert_eq!(gpu.profile_hub_tab(), Some(Tab::Character));
                assert!(!gpu.game_menu);
                if size[0] != 1920 {
                    continue;
                }
                // Leave on SJK Profile: the game menu's Profile opens on it again.
                gpu.profile_hub_show_for_shot(Tab::Profile);
                assert_eq!(gpu.profile_hub_tab(), Some(Tab::Profile));
                let _ = frame(&mut gpu, 2);
                if let Some(console) = gpu.console.as_mut() {
                    console.close_profile_hub_page();
                }
                assert_eq!(gpu.profile_hub_tab(), None);
                assert!(gpu.game_menu, "back on the game menu");
                gpu.open_profile_hub_from_game(None);
                assert_eq!(gpu.profile_hub_tab(), Some(Tab::Profile));
                // The picture panel, with Browse..., on the SJK Profile tab.
                gpu.profile_hub_show_for_shot(Tab::Profile);
                if let Some(console) = gpu.console.as_mut() {
                    console.preview_profile(profile_preview());
                    console.profile_show_picture();
                }
                println!(
                    "{}",
                    shoot(&mut gpu, 6, "duel6-profile-screen-picture").display()
                );
                assert!(!overflowed(&gpu), "the picture panel");
            }
            // From the main page's Profile: the Character tab on the menu map's stage.
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
                menu.open_player_hub(console, crate::player_menu::ReturnTarget::MainMenu);
            }
            assert_eq!(gpu.profile_hub_tab(), Some(Tab::Character));
            println!(
                "{}",
                shoot(&mut gpu, 150, "duel6-profile-screen-main").display()
            );
        });
    }

    /// The SJK UI's in-game menu over duel6 as a match would show it, on a
    /// made-up match (there is no server): the main page over an FFA without a vote
    /// and with one, the keyboard in the row of icons, the card's side buttons in a
    /// CTF, Staff tools shown, a spectator in an FFA, a Siege, Team (the J key's) in a
    /// CTF, the call-vote maps (the installed ones), Leave with Quit chosen, a 4:3
    /// window, and Settings opened from the menu.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_ingame() {
        use crate::ingame_menu::sjk_focus::{Control, Focus, Icon};
        use crate::ingame_menu::{Page, ShotView, sjk_view::Card};
        use sjk_client::LegacyTeamChoice;
        on_big_stack(|| {
            for size in [[1920, 1080], [1440, 1080]] {
                // The main menu stays closed: the client is "in the match".
                let menu = menu::ClientMenu::new(false, String::new());
                let cvars = [
                    ("ui_menuStyle", "sjk"),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                else {
                    return;
                };
                // A player's view down the south-west path towards the tower.
                let shots =
                    menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
                let shot = &shots[0];
                let (yaw, pitch) = look(shot.from, shot.at);
                aim(&mut gpu, shot.from, yaw, pitch);
                // Without a server or a menu the console drops: the game menu is
                // up from the start, as in a match.
                gpu.game_menu = true;
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                let _ = frame(&mut gpu, 60);
                let ffa = ShotView {
                    team: 0,
                    team_game: false,
                    red_players: 0,
                    blue_players: 0,
                    vote_active: false,
                    staff: false,
                    siege: false,
                };
                let ffa_vote = ShotView {
                    vote_active: true,
                    ..ffa
                };
                let ctf = ShotView {
                    team: 2,
                    team_game: true,
                    red_players: 4,
                    blue_players: 3,
                    vote_active: false,
                    staff: false,
                    siege: false,
                };
                let staff = ShotView {
                    vote_active: true,
                    staff: true,
                    ..ctf
                };
                let watching = ShotView { team: 3, ..ffa };
                let siege = ShotView {
                    team: 1,
                    siege: true,
                    ..ctf
                };
                let vfs = gpu.vfs.clone();
                gpu.in_game_menu.refresh_callvote(None, vfs.as_deref());
                let red = Control::Team(LegacyTeamChoice::Red);
                let join = Control::Team(LegacyTeamChoice::Free);
                let pages: Vec<(&str, Page, usize, Card, ShotView, Focus)> = if size[0] == 1920 {
                    vec![
                        (
                            "duel6-ingame",
                            Page::Main,
                            0,
                            Card::for_shot(false, false),
                            ffa,
                            Focus::List,
                        ),
                        (
                            "duel6-ingame-vote",
                            Page::Main,
                            1,
                            Card::for_shot(false, false),
                            ffa_vote,
                            Focus::List,
                        ),
                        (
                            "duel6-ingame-row",
                            Page::Main,
                            0,
                            Card::for_shot(false, false),
                            ffa,
                            Focus::Row(Icon::WhatsNew),
                        ),
                        (
                            "duel6-ingame-card",
                            Page::Main,
                            0,
                            Card::for_shot(true, false),
                            ctf,
                            Focus::Card(red),
                        ),
                        (
                            "duel6-ingame-staff",
                            Page::Main,
                            0,
                            Card::for_shot(true, false),
                            staff,
                            Focus::Row(Icon::Staff),
                        ),
                        (
                            "duel6-ingame-watching",
                            Page::Main,
                            0,
                            Card::for_shot(false, true),
                            watching,
                            Focus::Card(join),
                        ),
                        (
                            "duel6-ingame-siege",
                            Page::Main,
                            0,
                            Card::for_siege_shot(),
                            siege,
                            Focus::Card(Control::SiegeClass),
                        ),
                        (
                            "duel6-ingame-team",
                            Page::Team,
                            1,
                            Card::for_shot(true, false),
                            ctf,
                            Focus::List,
                        ),
                        (
                            "duel6-ingame-maps",
                            Page::VoteMap,
                            3,
                            Card::for_shot(false, false),
                            ffa,
                            Focus::List,
                        ),
                        (
                            "duel6-ingame-leave",
                            Page::Leave,
                            1,
                            Card::for_shot(false, false),
                            ffa,
                            Focus::List,
                        ),
                    ]
                } else {
                    vec![(
                        "duel6-ingame-4x3",
                        Page::Main,
                        0,
                        Card::for_shot(true, false),
                        staff,
                        Focus::Card(Control::VoteYes),
                    )]
                };
                for (name, page, row, card, view, focus) in pages {
                    gpu.in_game_menu.sjk_for_shot(card, view);
                    gpu.game_menu_page = page;
                    gpu.game_menu_row = row;
                    gpu.in_game_menu.focus = focus;
                    // Long enough for the gold mark to settle.
                    println!("{}", shoot(&mut gpu, 16, name).display());
                    assert_eq!(gpu.in_game_menu.focus, focus, "{name}: the keyboard stays");
                }
                if size[0] != 1920 {
                    continue;
                }
                // Settings, opened from the menu: its way back is the game menu.
                gpu.game_menu = false;
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    menu.open_sjk_settings_from_game(console);
                }
                println!("{}", shoot(&mut gpu, 8, "duel6-ingame-settings").display());
            }
        });
    }

    /// The SJK chat docked on the main page and in the in-game menu, on made-up messages
    /// (a long one wrapping onto rows of its own, the newest too tall for the box at the
    /// larger text style): the main page, then something typed in its field; the in-game
    /// menu over an FFA, the sender card of a name under the pointer, then the chat's
    /// field typing with a vote on and Staff tools shown. At 1080p, 4:3 and 4K, with the
    /// plain text style and Sol's (`ui_textScale 1.2`). No canvas runs out of room.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_chat_dock() {
        use crate::ingame_menu::sjk_focus::Focus;
        use crate::ingame_menu::sjk_view::{CHAT_TOKENS, Card};
        use crate::ingame_menu::{Page, ShotView};
        const LINES: [(&str, &str, bool); 5] = [
            ("^5JoF^7 Jedi", "anyone up for duels on ffa3?", true),
            ("^1Fox", "in 5 min, finishing a CTF", false),
            (
                "Kyle",
                "the new HUD looks great, but the force bar in the corner feels a bit too \
                 small at 4K and the clock could move a little to the left",
                false,
            ),
            ("^3Lumaya", "gg", true),
            (
                "^2Sol",
                "the chat dock wraps now: a long message goes on over as many lines as it \
                 needs instead of being cut at the edge of the column",
                true,
            ),
        ];
        for (size, scale, suffix) in [
            ([1920, 1080], None, "1080p"),
            ([1920, 1080], Some("1.2"), "1080p-styled"),
            ([1440, 1080], None, "4x3"),
            ([1440, 1080], Some("1.2"), "4x3-styled"),
            ([3840, 2160], Some("1.2"), "4k-styled"),
        ] {
            let cvars = move || {
                let mut cvars = vec![
                    ("ui_menuStyle", "sjk"),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                    ("name", "^1Sol^7Vulpes"),
                ];
                if let Some(scale) = scale {
                    cvars.push((crate::text::style::SCALE_CVAR, scale));
                }
                cvars
            };
            on_big_stack(move || {
                let menu = menu::ClientMenu::new(true, String::new());
                let Some((mut gpu, _profile)) =
                    open("maps/mp/duel6.bsp", size, Some(menu), &cvars())
                else {
                    return;
                };
                gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
                let _ = frame(&mut gpu, 20);
                for (draft, name) in [
                    (None, format!("duel6-chat-dock-{suffix}")),
                    (
                        Some("on my way, save me a spot on the JoF server tonight"),
                        format!("duel6-chat-dock-{suffix}-typing"),
                    ),
                ] {
                    if let Some(menu) = gpu.client_menu.as_mut() {
                        menu.sjk_home_chat_for_shot(&LINES, 12, draft);
                    }
                    println!("{}", shoot(&mut gpu, 6, &name).display());
                    let menu = gpu.client_menu.as_ref().expect("the menu");
                    assert!(!menu.sjk_home_overflowed(), "{name}");
                }
            });
            on_big_stack(move || {
                // The main menu stays closed: the client is "in the match".
                let menu = menu::ClientMenu::new(false, String::new());
                let Some((mut gpu, _profile)) =
                    open("maps/mp/duel6.bsp", size, Some(menu), &cvars())
                else {
                    return;
                };
                let shots =
                    menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
                let shot = &shots[0];
                let (yaw, pitch) = look(shot.from, shot.at);
                aim(&mut gpu, shot.from, yaw, pitch);
                gpu.game_menu = true;
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                let _ = frame(&mut gpu, 60);
                let ffa = ShotView {
                    team: 0,
                    team_game: false,
                    red_players: 0,
                    blue_players: 0,
                    vote_active: false,
                    staff: false,
                    siege: false,
                };
                let busy = ShotView {
                    vote_active: true,
                    staff: true,
                    ..ffa
                };
                gpu.game_menu_page = Page::Main;
                gpu.game_menu_row = 0;
                gpu.in_game_menu
                    .sjk_for_shot(Card::for_shot(false, false), ffa);
                gpu.in_game_menu.chat_for_shot(&LINES, 12, None);
                gpu.in_game_menu.focus = Focus::List;
                let name = format!("duel6-ingame-chat-{suffix}");
                println!("{}", shoot(&mut gpu, 16, &name).display());
                assert!(!gpu.in_game_menu.overflowed(), "{name}");
                // The pointer on the newest message's sender: their card.
                if let Some(rect) = gpu.in_game_menu.rect_for(CHAT_TOKENS.name(1)) {
                    let _ = gpu.in_game_menu.pointer(sjk_ui::InputEvent::PointerMove(
                        sjk_ui::Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                    ));
                    let name = format!("duel6-ingame-chat-{suffix}-card");
                    println!("{}", shoot(&mut gpu, 4, &name).display());
                    assert!(!gpu.in_game_menu.overflowed(), "{name}");
                    let _ = gpu
                        .in_game_menu
                        .pointer(sjk_ui::InputEvent::PointerMove(sjk_ui::Vec2::new(2.0, 2.0)));
                }
                gpu.in_game_menu
                    .sjk_for_shot(Card::for_shot(false, false), busy);
                gpu.in_game_menu.chat_for_shot(
                    &LINES,
                    12,
                    Some("gg, rematch on ffa3 after this one?"),
                );
                gpu.in_game_menu.focus = Focus::Chat;
                let name = format!("duel6-ingame-chat-{suffix}-typing");
                println!("{}", shoot(&mut gpu, 8, &name).display());
                assert!(!gpu.in_game_menu.overflowed(), "{name}");
                assert_eq!(gpu.in_game_menu.focus, Focus::Chat, "{name}");
            });
        }
    }

    /// Camera control over duel6 as a match would show it, on a made-up match
    /// (there is no server): the game menu with its camera icon focused (the
    /// SJK UI's row of icons), then the panel on its Camera page; in the SJK UI also the Sun
    /// page, a number being typed and the Sun page where the sun cannot be
    /// set, and a 4:3 window; the classic bar and its panel, opened by F8.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_camera_control() {
        use crate::ingame_menu::shot::SUN;
        use crate::ingame_menu::{
            Page, ShotView,
            sjk_focus::{Focus, Icon},
            sjk_view::Card,
        };
        on_big_stack(|| {
            for (style, size, prefix) in [
                ("sjk", [1920, 1080], "duel6-camera"),
                ("sjk", [1440, 1080], "duel6-camera-4x3"),
                ("classic", [1920, 1080], "duel6-camera-classic"),
            ] {
                let menu = menu::ClientMenu::new(false, String::new());
                let cvars = [
                    ("ui_menuStyle", style),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                else {
                    return;
                };
                let shots =
                    menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
                let (yaw, pitch) = look(shots[0].from, shots[0].at);
                aim(&mut gpu, shots[0].from, yaw, pitch);
                gpu.game_menu = true;
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                let _ = frame(&mut gpu, 60);
                gpu.game_menu_page = Page::Main;
                if style == "sjk" {
                    let ffa = ShotView {
                        team: 0,
                        team_game: false,
                        red_players: 0,
                        blue_players: 0,
                        vote_active: false,
                        staff: false,
                        siege: false,
                    };
                    gpu.in_game_menu
                        .sjk_for_shot(Card::for_shot(false, false), ffa);
                    // Its icon in the row under the emblem, with the keyboard.
                    gpu.game_menu_row = 0;
                    gpu.in_game_menu.focus = Focus::Row(Icon::Camera);
                } else {
                    // The classic bar has no entry: F8 opens the panel there.
                    gpu.game_menu_row = 0;
                }
                let entry = format!("{prefix}-entry");
                println!("{}", shoot(&mut gpu, 16, &entry).display());
                // As the entry (or F8) opens it.
                gpu.open_shot_panel();
                let panel = format!("{prefix}-panel");
                println!("{}", shoot(&mut gpu, 8, &panel).display());
                if prefix != "duel6-camera" {
                    continue;
                }
                let sun_available = gpu.in_game_menu.shot.sun_available;
                println!("the sun can be set here: {sun_available}");
                gpu.in_game_menu.shot.activate(SUN);
                println!("{}", shoot(&mut gpu, 4, "duel6-camera-sun").display());
                // Back on Camera, Distance chosen and a number typed on it.
                let shot = &mut gpu.in_game_menu.shot;
                shot.sun_tab = false;
                shot.selected = 2;
                assert!(shot.begin_typed("12"));
                println!("{}", shoot(&mut gpu, 4, "duel6-camera-typing").display());
                let shot = &mut gpu.in_game_menu.shot;
                shot.activate(SUN);
                shot.sun_available = !sun_available;
                println!("{}", shoot(&mut gpu, 4, "duel6-camera-sun-other").display());
            }
        });
    }

    /// The quick wheel over duel6 as a player holding its key sees it, without a
    /// server: General with a choice highlighted, the change to Toys caught
    /// half-way, Toys settled with the mouse on Illuminate (lit), then Weather
    /// settled with the mouse on Rain, the pointer still in the middle; a 4:3
    /// window; and in Inter, with the classic menus (whose style does not load the
    /// SJK UI's families).
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_quick_wheel() {
        use std::time::{Duration, Instant};
        on_big_stack(|| {
            for (size, style, prefix) in [
                ([1920, 1080], "sjk", "duel6-wheel"),
                ([1440, 1080], "sjk", "duel6-wheel-4x3"),
                ([1920, 1080], "classic", "duel6-wheel-inter"),
            ] {
                // No client menu: a disconnected game client would show its
                // console full screen.
                let cvars = [
                    ("ui_menuStyle", style),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, None, &cvars)
                else {
                    return;
                };
                let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("the tour");
                let (yaw, pitch) = look(shots[0].from, shots[0].at);
                aim(&mut gpu, shots[0].from, yaw, pitch);
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                gpu.quick_wheel.shot = true;
                let _ = frame(&mut gpu, 60);
                // As Q's bind opens it: the page, then the key's number and time.
                let bound = |page: &str| [page.to_owned(), "81".to_owned(), "1000".to_owned()];
                gpu.open_quick_wheel(&bound("general"))
                    .expect("the wheel opens");
                gpu.quick_wheel.moved([50.0, -50.0]);
                println!(
                    "{}",
                    shoot(&mut gpu, 4, &format!("{prefix}-general")).display()
                );
                if prefix != "duel6-wheel" {
                    continue;
                }
                gpu.quick_wheel
                    .turn(1, Instant::now() - Duration::from_millis(45));
                println!("{}", shoot(&mut gpu, 1, "duel6-wheel-switching").display());
                // Toys, with the holocron lit: its one choice wears the gold dot.
                gpu.illuminate.set_on(true);
                gpu.quick_wheel.cancel();
                gpu.open_quick_wheel(&bound("toys"))
                    .expect("the wheel opens");
                gpu.quick_wheel.moved([0.0, -90.0]);
                println!("{}", shoot(&mut gpu, 4, "duel6-wheel-toys").display());
                gpu.illuminate.set_on(false);
                gpu.quick_wheel.cancel();
                gpu.open_quick_wheel(&bound("weather"))
                    .expect("the wheel opens");
                gpu.quick_wheel.moved([70.0, 50.0]);
                println!("{}", shoot(&mut gpu, 12, "duel6-wheel-weather").display());
                gpu.quick_wheel.cancel();
                gpu.open_quick_wheel(&bound("weather"))
                    .expect("the wheel opens");
                println!("{}", shoot(&mut gpu, 4, "duel6-wheel-middle").display());
                // The second board's icons on three full pages, the last with
                // a custom command and three of the first board's beside them.
                gpu.quick_wheel.cancel();
                let settled = Instant::now() - Duration::from_secs(1);
                gpu.quick_wheel.open(second_board_pages(), 0, settled);
                gpu.quick_wheel.moved([60.0, -60.0]);
                for page in 1..=3 {
                    let name = format!("duel6-wheel-icons-{page}");
                    println!("{}", shoot(&mut gpu, 4, &name).display());
                    gpu.quick_wheel.turn(1, settled);
                }
            }
        });
    }

    /// The quick wheel's Force page over duel6 without a server, the powers set
    /// for the shot: a light-side build (Protect selected, the mouse on Push), a
    /// dark-side one, every power (twelve on the ring, the rest on Force 2), no
    /// power at all, and the light build at 4K with Sol's menu text size
    /// (`ui_textScale 1.2`). Illuminate is a toy: no build shows it.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_quick_wheel_force() {
        use std::time::{Duration, Instant};
        const NEUTRAL: u32 = (1 << 2) | (1 << 3) | (1 << 4) | (1 << 14);
        const LIGHT: u32 = NEUTRAL | 1 | (1 << 5) | (1 << 9) | (1 << 10) | (1 << 11);
        const DARK: u32 = NEUTRAL | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 12) | (1 << 13);
        // Jump and the saber powers known too, as every build has them.
        const PASSIVE: u32 = (1 << 1) | (7 << 15);
        on_big_stack(|| {
            for (size, scale, prefix) in [
                ([1920, 1080], "1", "duel6-wheel-force"),
                ([3840, 2160], "1.2", "duel6-wheel-force-4k"),
            ] {
                let cvars = [
                    ("ui_menuStyle", "sjk"),
                    ("ui_textScale", scale),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, None, &cvars)
                else {
                    return;
                };
                let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("the tour");
                let (yaw, pitch) = look(shots[0].from, shots[0].at);
                aim(&mut gpu, shots[0].from, yaw, pitch);
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                gpu.quick_wheel.shot = true;
                let _ = frame(&mut gpu, 60);
                let bound = [String::from("force"), "81".to_owned(), "1000".to_owned()];
                let builds: &[(&str, u32, u8, [f32; 2])] = if prefix == "duel6-wheel-force" {
                    &[
                        ("light", LIGHT | PASSIVE, 9, [0.0, -90.0]),
                        ("dark", DARK | PASSIVE, 7, [70.0, 50.0]),
                        ("every", u32::MAX, 3, [-60.0, -60.0]),
                        ("none", PASSIVE, 0, [0.0, -90.0]),
                    ]
                } else {
                    &[("light", LIGHT | PASSIVE, 9, [0.0, -90.0])]
                };
                for &(build, known, selected, pointer) in builds {
                    gpu.quick_wheel.force_for_shot = Some((known, selected));
                    gpu.quick_wheel.cancel();
                    gpu.open_quick_wheel(&bound).expect("the wheel opens");
                    gpu.quick_wheel.moved(pointer);
                    let name = format!("{prefix}-{build}");
                    println!("{}", shoot(&mut gpu, 4, &name).display());
                    if build == "every" {
                        let settled = Instant::now() - Duration::from_secs(1);
                        gpu.quick_wheel.turn(1, settled);
                        let name = format!("{prefix}-every-2");
                        println!("{}", shoot(&mut gpu, 4, &name).display());
                    }
                }
                // Out of a game: the page says so.
                gpu.quick_wheel.force_for_shot = None;
                gpu.quick_wheel.cancel();
                gpu.open_quick_wheel(&bound).expect("the wheel opens");
                println!(
                    "{}",
                    shoot(&mut gpu, 4, &format!("{prefix}-no-game")).display()
                );
            }
        });
    }

    /// The GIF wheel (`+gifwheel`) on twelve favourite test GIFs of several shapes,
    /// moving, the pointer on the first, then on its second page; at 1080p and 4K.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_gif_wheel() {
        use crate::chat_gifs::{GifId, decode};
        use std::time::{Duration, Instant};
        on_big_stack(|| {
            for (size, suffix) in [([1920, 1080], "1080p"), ([3840, 2160], "4k")] {
                let cvars = [
                    ("ui_menuStyle", "sjk"),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, None, &cvars)
                else {
                    return;
                };
                let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("the tour");
                let (yaw, pitch) = look(shots[0].from, shots[0].at);
                aim(&mut gpu, shots[0].from, yaw, pitch);
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                gpu.quick_wheel.shot = true;
                let _ = frame(&mut gpu, 60);
                // After the first frames, which read the (empty) favourites file.
                let shapes = [[200, 112], [200, 200], [200, 150], [200, 260], [200, 90]];
                for n in (0..12).rev() {
                    let id = GifId::new(&format!("ShotWheel{n:02}X")).unwrap();
                    crate::chat_gifs::insert_preview_for_shot(
                        &id,
                        &decode::test_animation(shapes[n % shapes.len()], 4 + n % 4, 90),
                        250,
                    );
                    crate::chat_gifs::favourites::set(&id, true);
                }
                gpu.quick_wheel.cancel();
                gpu.open_gif_wheel().expect("the wheel opens");
                gpu.quick_wheel.moved([0.0, -90.0]);
                let name = format!("duel6-gif-wheel-{suffix}");
                println!("{}", shoot(&mut gpu, 8, &name).display());
                gpu.quick_wheel
                    .turn(1, Instant::now() - Duration::from_secs(1));
                let name = format!("duel6-gif-wheel-2-{suffix}");
                println!("{}", shoot(&mut gpu, 8, &name).display());
                for n in 0..12 {
                    let id = GifId::new(&format!("ShotWheel{n:02}X")).unwrap();
                    crate::chat_gifs::favourites::set(&id, false);
                }
            }
        });
    }

    /// Three pages holding every action of the wheel's second icon board, the
    /// custom command's icon and, to compare, three of the first board's.
    fn second_board_pages() -> Vec<crate::quick_wheel::ShownPage> {
        use crate::quick_wheel::pages::Slot;
        use crate::quick_wheel::{ShownChoice, ShownPage, catalog};
        let pages: [(&str, [&str; 10]); 3] = [
            (
                "Interface",
                [
                    "game_menu",
                    "first_setup",
                    "free_camera",
                    "timer",
                    "fps",
                    "speedometer",
                    "lagometer",
                    "console",
                    "whats_new",
                    "team_menu",
                ],
            ),
            (
                "Player",
                [
                    "spectate",
                    "respawn",
                    "inspect",
                    "duel",
                    "saber_style",
                    "saber_toggle",
                    "taunt",
                    "bow",
                    "meditate",
                    "flourish",
                ],
            ),
            (
                "Server",
                [
                    "gloat",
                    "vote_yes",
                    "vote_no",
                    "record",
                    "stop_record",
                    "reconnect",
                    "",
                    "third_person",
                    "rain",
                    "day",
                ],
            ),
        ];
        pages
            .iter()
            .map(|(name, ids)| ShownPage {
                id: name.to_lowercase(),
                name: (*name).to_owned(),
                choices: ids
                    .iter()
                    .map(|id| {
                        let slot = catalog::action_index(id).map_or_else(
                            || Slot::Custom {
                                label: "Ready".to_owned(),
                                command: "ready".to_owned(),
                            },
                            Slot::Action,
                        );
                        ShownChoice {
                            label: slot.label().to_owned(),
                            command: slot.command().to_owned(),
                            icon: slot.texture(),
                            on: false,
                            gif: None,
                        }
                    })
                    .collect(),
                force: false,
                gifs: false,
            })
            .collect()
    }

    /// The quick wheel over duel6 with a page of the installed emotes (`emotes/` of
    /// the game data) and Stop emote: each emote wears its picture when its file
    /// names one (`emote_icons`), the plain emote icon otherwise.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_quick_wheel_emotes() {
        use crate::quick_wheel::pages::Slot;
        use crate::quick_wheel::{ShownChoice, ShownPage, catalog};
        use std::time::{Duration, Instant};
        on_big_stack(|| {
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], None, &cvars)
            else {
                return;
            };
            let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("the tour");
            let (yaw, pitch) = look(shots[0].from, shots[0].at);
            aim(&mut gpu, shots[0].from, yaw, pitch);
            if let Some(console) = gpu.console.as_mut() {
                console.close_for_connection();
            }
            gpu.quick_wheel.shot = true;
            // The frames load the catalogue and put its pictures into the atlas.
            let _ = frame(&mut gpu, 30);
            let installed: Vec<Slot> = crate::emotes::lock()
                .catalogue
                .iter()
                .flatten()
                .take(9)
                .map(|def| Slot::Custom {
                    label: def.name.clone(),
                    command: format!("{} {}", crate::emotes::COMMAND, def.id),
                })
                .collect();
            println!("installed emotes: {}", installed.len());
            let slots = installed
                .into_iter()
                .chain(catalog::action_index("emote_stop").map(Slot::Action));
            let page = ShownPage {
                id: "emotes".to_owned(),
                name: "Emotes".to_owned(),
                choices: slots
                    .map(|slot| {
                        println!("{} -> {:?}", slot.command(), slot.texture());
                        ShownChoice {
                            label: slot.label().to_owned(),
                            command: slot.command().to_owned(),
                            icon: slot.texture(),
                            on: false,
                            gif: None,
                        }
                    })
                    .collect(),
                force: false,
                gifs: false,
            };
            let settled = Instant::now() - Duration::from_secs(1);
            gpu.quick_wheel.open(vec![page], 0, settled);
            gpu.quick_wheel.moved([60.0, -60.0]);
            println!("{}", shoot(&mut gpu, 4, "duel6-wheel-emotes").display());
        });
    }

    /// Settings > Quick wheel in the SJK UI over duel6: the pages with General
    /// focused, a choice focused (the preview highlighting it), the catalogue
    /// changing it, a custom choice's form, a new page being named, then the
    /// page filled; and the same editor opened from the classic+ settings'
    /// Interface row.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_quick_wheel_settings() {
        use winit::keyboard::KeyCode::*;
        /// Keys pressed in the editor, each with what it types.
        type Keys = &'static [(winit::keyboard::KeyCode, Option<&'static str>)];
        on_big_stack(|| {
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 20);
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            let steps: [(&str, Keys); 8] = [
                // The Force page, the first: a note instead of choices, an
                // example ring.
                ("duel6-wheel-settings", &[]),
                // The Toys page: Illuminate, from the catalogue's Toys.
                (
                    "duel6-wheel-settings-toys",
                    &[(ArrowDown, None), (ArrowDown, None)],
                ),
                // Down past the pages, Add a page and Restore to the sounds,
                // switched off; switched on again before the next step, then
                // Down wraps back to Force and on to General.
                (
                    "duel6-wheel-settings-sounds",
                    &[
                        (ArrowDown, None),
                        (ArrowDown, None),
                        (ArrowDown, None),
                        (ArrowDown, None),
                        (Enter, None),
                    ],
                ),
                (
                    "duel6-wheel-settings-choice",
                    &[
                        (Enter, None),
                        (ArrowDown, None),
                        (ArrowDown, None),
                        (ArrowRight, None),
                        (ArrowDown, None),
                    ],
                ),
                ("duel6-wheel-settings-catalogue", &[(Enter, None)]),
                (
                    "duel6-wheel-settings-custom",
                    &[
                        (Escape, None),
                        (ArrowUp, None),
                        (ArrowUp, None),
                        (Enter, None),
                        (End, None),
                        (Enter, None),
                        (KeyA, Some("Ready")),
                        (Enter, None),
                        (KeyA, Some("say ready; ready")),
                    ],
                ),
                (
                    "duel6-wheel-settings-new-page",
                    &[
                        (Enter, None),
                        (ArrowLeft, None),
                        (ArrowDown, None),
                        (ArrowDown, None),
                        (ArrowDown, None),
                        (Enter, None),
                        (KeyA, Some("Duels")),
                    ],
                ),
                (
                    "duel6-wheel-settings-filled",
                    &[
                        (Enter, None),
                        (Enter, None),
                        (Enter, None),
                        (Enter, None),
                        (ArrowDown, None),
                        (Enter, None),
                        (ArrowDown, None),
                        (ArrowDown, None),
                        (Enter, None),
                    ],
                ),
            ];
            for (name, keys) in steps {
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_mut())
                {
                    menu.sjk_wheel_for_shot(console, keys);
                }
                println!("{}", shoot(&mut gpu, 6, name).display());
            }
            if let Some(console) = gpu.console.as_mut() {
                console.set_cvar(crate::menu::style::CVAR, "classic");
            }
            let _ = frame(&mut gpu, 2);
            if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
                menu.wheel_overlay_for_shot(console);
            }
            let path = shoot(&mut gpu, 6, "duel6-wheel-settings-classic");
            println!("{}", path.display());
        });
    }

    /// The game menu's Players page (a small scoreboard) and its Report page, in the
    /// SJK UI and the classic menus, for a verified player and for one who is not, and
    /// the report's dialog: on a made-up roster over duel6.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_ingame_players() {
        use crate::ingame_menu::players::{Gate, State};
        use crate::ingame_menu::{Page, ShotView, sjk_view::Card};
        on_big_stack(|| {
            for style in ["sjk", "classic"] {
                let menu = menu::ClientMenu::new(false, String::new());
                let cvars = [
                    ("ui_menuStyle", style),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                let Some((mut gpu, _profile)) =
                    open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
                else {
                    return;
                };
                let shots =
                    menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
                let (yaw, pitch) = look(shots[0].from, shots[0].at);
                aim(&mut gpu, shots[0].from, yaw, pitch);
                gpu.game_menu = true;
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                let _ = frame(&mut gpu, 60);
                if style == "sjk" {
                    let ctf = ShotView {
                        team: 2,
                        team_game: true,
                        red_players: 6,
                        blue_players: 6,
                        vote_active: false,
                        staff: false,
                        siege: false,
                    };
                    gpu.in_game_menu
                        .sjk_for_shot(Card::for_shot(true, false), ctf);
                }
                let pages: [(&str, Gate, Page, usize); 4] = [
                    ("players", Gate::Open, Page::Players, 3),
                    ("players-unverified", Gate::NotVerified, Page::Players, 0),
                    ("report", Gate::Open, Page::ReportPlayer, 1),
                    (
                        "report-unverified",
                        Gate::NotVerified,
                        Page::ReportPlayer,
                        7,
                    ),
                ];
                for (name, gate, page, row) in pages {
                    let mut roster = State::for_shot(12, true, gate);
                    roster.choose(3);
                    gpu.in_game_menu.players = roster;
                    gpu.game_menu_page = page;
                    gpu.game_menu_row = row;
                    let name = format!("duel6-ingame-{style}-{name}");
                    println!("{}", shoot(&mut gpu, 16, &name).display());
                }
                // The SJK UI's card has its own shots (`duel6_sjk_report`), without
                // the menu under it, as in a match.
                if style == "sjk" {
                    continue;
                }
                // In a match the menu closes under the dialog; without a server the
                // console would drop, so the menu stays up under it here.
                gpu.text_dialog
                    .open(crate::text_dialog::Kind::PlayerReport {
                        subject: "Kyle: Cheating".to_owned(),
                        category: sjk_identity::Category::Cheating,
                    });
                gpu.text_dialog.preview(
                    "Speed hacking and flying through walls all match",
                    true,
                    "",
                );
                let name = format!("duel6-ingame-{style}-report-dialog");
                println!("{}", shoot(&mut gpu, 8, &name).display());
            }
        });
    }

    /// A made-up profile picture for a world shot, [`sjk_identity::avatar::SIZE`]
    /// square: a head and shoulders in a light tone over a gradient between `top` and
    /// `bottom` (RGB).
    fn shot_picture(top: [u8; 3], bottom: [u8; 3], figure: [u8; 3]) -> Vec<u8> {
        let size = sjk_identity::avatar::SIZE;
        let mut rgba = Vec::with_capacity((size * size * 4) as usize);
        let edge = size as f32;
        for y in 0..size {
            for x in 0..size {
                let t = y as f32 / edge;
                let mut colour = [0u8; 3];
                for channel in 0..3 {
                    colour[channel] = (f32::from(top[channel]) * (1.0 - t)
                        + f32::from(bottom[channel]) * t)
                        as u8;
                }
                let (fx, fy) = (x as f32 / edge - 0.5, y as f32 / edge);
                let head = fx * fx + (fy - 0.4) * (fy - 0.4) < 0.17 * 0.17;
                let shoulders =
                    fy > 0.68 && fx * fx / 0.16 + (fy - 1.02) * (fy - 1.02) / 0.12 < 1.0;
                if head || shoulders {
                    colour = figure;
                }
                rgba.extend_from_slice(&[colour[0], colour[1], colour[2], 255]);
            }
        }
        rgba
    }

    /// The SJK chat page over the live duel6 with its who-is-online window of faces, on
    /// made-up players, some with pictures (made up too, put in the picture cache) and
    /// some with only their initial: a hub that lists them (some playing, a long name,
    /// the recently seen), then the card of a player in the window under the pointer,
    /// the card of a recent one, a chosen message under the window, a crowd too big for
    /// the window (its last face "+N"), and a hub that does not list them (the count
    /// and the last senders). At 1080p with the plain text style and at 4K with Sol's
    /// (`ui_textScale 1.2`).
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_chat_page() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() as i64);
        let lines: [(&str, &str, &str, bool); 6] = [
            (
                "9a0c51e2b7d34f80",
                "^5Creyon",
                "anyone up for duels on ffa3?",
                true,
            ),
            (
                "0b1c2d3e4f5a6b7c",
                "Padawan^1Fox",
                "in 5 min, finishing a CTF",
                false,
            ),
            ("1f2e3d4c5b6a7980", "^3Lumaya", "gg", true),
            (
                "44f3d0b36c9b2510",
                "^1Sol^7Vulpes",
                "the chat page shows who is online now, on the right",
                true,
            ),
            (
                "7c6b5a4f3e2d1c0b",
                "Kyle",
                "nice, I can see who is around",
                false,
            ),
            (
                "0b1c2d3e4f5a6b7c",
                "Padawan^1Fox",
                "back, who wants a duel?",
                false,
            ),
        ];
        let messages = lines
            .iter()
            .enumerate()
            .map(
                |(index, (key, name, text, verified))| sjk_identity::ChatMessage {
                    id: index as u64 + 1,
                    at: now - (lines.len() - index) as i64 * 140,
                    key_id: (*key).to_owned(),
                    name: (*name).to_owned(),
                    verified: *verified,
                    staff: index == 3,
                    text: (*text).to_owned(),
                    holocron: None,
                },
            )
            .collect();
        let person =
            |key: &str, name: &str, ago: i64, playing: bool, verified: bool| sjk_identity::Person {
                key_id: key.to_owned(),
                name: name.to_owned(),
                verified,
                seen: now - ago,
                playing,
                ..sjk_identity::Person::default()
            };
        // Pictures for Sol, Creyon, Mara and Lumaya; the others show their initial.
        let pictures = [
            (
                "44f3d0b36c9b2510",
                [0x2a, 0x1c, 0x3c],
                [0xb0, 0x4a, 0x2a],
                [0xf2, 0xc9, 0x8a],
            ),
            (
                "9a0c51e2b7d34f80",
                [0x12, 0x3a, 0x52],
                [0x1c, 0x8a, 0x9a],
                [0xe6, 0xf2, 0xf6],
            ),
            (
                "3e4f5a6b7c8d9e0f",
                [0x3c, 0x24, 0x10],
                [0x9a, 0x72, 0x2c],
                [0x2a, 0x1a, 0x10],
            ),
            (
                "1f2e3d4c5b6a7980",
                [0x46, 0x3a, 0x08],
                [0xe0, 0xb8, 0x3a],
                [0x3a, 0x22, 0x40],
            ),
        ];
        for (key, top, bottom, figure) in pictures {
            crate::avatars::insert_for_shot(
                key,
                "0a1b2c3d4e5f6a7b",
                &shot_picture(top, bottom, figure),
            );
        }
        let pictured = |mut person: sjk_identity::Person| {
            if pictures.iter().any(|(key, ..)| *key == person.key_id) {
                person.avatar = "0a1b2c3d4e5f6a7b".to_owned();
            }
            person
        };
        let mut listed = sjk_identity::ChatState {
            messages,
            revision: 1,
            online: 7,
            people: Some(sjk_identity::People {
                online: vec![
                    person("44f3d0b36c9b2510", "^1Sol^7Vulpes", 5, false, true),
                    person("0b1c2d3e4f5a6b7c", "Padawan^1Fox", 10, true, false),
                    person("9a0c51e2b7d34f80", "^5Creyon", 20, true, true),
                    person("7c6b5a4f3e2d1c0b", "Kyle", 30, false, false),
                    person(
                        "2d3e4f5a6b7c8d9e",
                        "^4The^7Longest^4Name^7On^4The^7Hub^4Today",
                        15,
                        false,
                        false,
                    ),
                    person("3e4f5a6b7c8d9e0f", "Mara", 40, false, false),
                    person("4f5a6b7c8d9e0f1a", "Bastila", 25, false, false),
                ],
                recent: vec![
                    person("1f2e3d4c5b6a7980", "^3Lumaya", 600, false, true),
                    person("5a6b7c8d9e0f1a2b", "Jan^2Ors", 3 * 3_600, false, false),
                    person("6b7c8d9e0f1a2b3c", "Rosh", 30 * 3_600, false, false),
                    person("7a8b9c0d1e2f3a4b", "^6Visas", 4 * 86_400, false, false),
                ],
            }),
            live: true,
            outcome: None,
            loaded: Some(1),
        };
        if let Some(people) = listed.people.as_mut() {
            for person in people.online.iter_mut().chain(people.recent.iter_mut()) {
                *person = pictured(std::mem::take(person));
            }
        }
        let unlisted = sjk_identity::ChatState {
            people: None,
            ..listed.clone()
        };
        // More than the window holds: its last face counts the rest.
        let mut crowd = listed.clone();
        crowd.online = 31;
        if let Some(people) = crowd.people.as_mut() {
            let names = [
                "Revan",
                "^3Bastila",
                "Carth",
                "^1Malak",
                "Mission",
                "Zaalbar",
                "Jolee",
                "^5Juhani",
                "HK-47",
                "Canderous",
                "T3",
                "^2Atton",
                "Kreia",
                "Mira",
                "^6Brianna",
                "Mical",
                "Bao-Dur",
                "Visas",
                "^4Mandalore",
                "Kaah",
                "Tavion",
                "Desann",
                "Galak",
                "Reelo",
            ];
            for (index, name) in names.iter().enumerate() {
                people.online.push(person(
                    &format!("{:016x}", 0xc0de_0000 + index),
                    name,
                    10,
                    index % 5 == 0,
                    false,
                ));
            }
        }
        for (size, scale, suffix) in [
            ([1920, 1080], None, "1080p"),
            ([3840, 2160], Some("1.2"), "4k-styled"),
        ] {
            let (listed, unlisted, crowd) = (listed.clone(), unlisted.clone(), crowd.clone());
            on_big_stack(move || {
                let menu = menu::ClientMenu::new(true, String::new());
                let mut cvars = vec![
                    ("ui_menuStyle", "sjk"),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                ];
                if let Some(scale) = scale {
                    cvars.push((crate::text::style::SCALE_CVAR, scale));
                }
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                else {
                    return;
                };
                let _ = frame(&mut gpu, 10);
                if let Some(console) = gpu.console.as_mut() {
                    console.preview_sjk_chat(listed.clone(), None);
                }
                let name = format!("duel6-sjk-chat-online-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                // Creyon's card, from the window (playing first, then by name: the first
                // face), with a picture.
                if let Some(console) = gpu.console.as_mut() {
                    console.sjk_chat_hover_person(Some(0));
                }
                let name = format!("duel6-sjk-chat-online-card-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                // Lumaya's, the first recent face: when she was seen.
                if let Some(console) = gpu.console.as_mut() {
                    console.sjk_chat_hover_person(Some(7));
                }
                let name = format!("duel6-sjk-chat-online-card-recent-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                if let Some(console) = gpu.console.as_mut() {
                    console.preview_sjk_chat(listed.clone(), Some(4));
                    console.sjk_chat_hover_person(None);
                }
                let name = format!("duel6-sjk-chat-online-chosen-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                if let Some(console) = gpu.console.as_mut() {
                    console.preview_sjk_chat(crowd.clone(), None);
                }
                let name = format!("duel6-sjk-chat-online-crowd-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
                if let Some(console) = gpu.console.as_mut() {
                    console.preview_sjk_chat(unlisted.clone(), None);
                }
                let name = format!("duel6-sjk-chat-online-unlisted-{suffix}");
                println!("{}", shoot(&mut gpu, 6, &name).display());
            });
        }
    }

    /// Report a bug and the dialogs sharing it as the SJK UI's card. Over duel6 from a
    /// player's view with no menu up, as in a match (no server, so no HUD): the empty
    /// report, a long text typed, a text refused, sending, sent, not sent (the real
    /// path: the shots' identity is off, so nothing reaches the hub), a player report,
    /// a world note; a 4:3 window; and over the menu map with the main page under it.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_sjk_report() {
        use crate::text_dialog::{Kind, Report};
        const TYPED: &str = "The door by the tower's foot flickers when I walk through it, and the light behind it goes black for a second. I expected it to open smoothly, as it does on ffa3. It happens every time on this server.";
        on_big_stack(|| {
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            // No client menu: a disconnected game client would show its console
            // full screen.
            let in_match = |size: [u32; 2]| {
                let (mut gpu, profile) = open("maps/mp/duel6.bsp", size, None, &cvars)?;
                let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("the tour");
                let (yaw, pitch) = look(shots[0].from, shots[0].at);
                aim(&mut gpu, shots[0].from, yaw, pitch);
                if let Some(console) = gpu.console.as_mut() {
                    console.close_for_connection();
                }
                let _ = frame(&mut gpu, 60);
                Some((gpu, profile))
            };
            let Some((mut gpu, _profile)) = in_match([1920, 1080]) else {
                return;
            };
            // Nothing may reach the hub: the shots' profile never starts the service.
            assert!(crate::player_identity::snapshot().is_none());
            let report = |gpu: &mut GpuState, name: &str, text: &str, message: &str| {
                gpu.text_dialog.open(Kind::Report);
                gpu.text_dialog.preview(text, false, message);
                gpu.text_dialog.caret_for_shot();
                println!("{}", shoot(gpu, 6, name).display());
            };
            report(&mut gpu, "duel6-report", "", "");
            report(&mut gpu, "duel6-report-typed", TYPED, "");
            report(
                &mut gpu,
                "duel6-report-refused",
                "door",
                "a report needs at least 10 characters",
            );
            // Sent: the card waits, then shows what the hub stored it as.
            gpu.text_dialog.preview(TYPED, true, "");
            assert!(matches!(
                gpu.text_dialog.send_for_shot(),
                crate::text_dialog::Action::Send(..)
            ));
            println!("{}", shoot(&mut gpu, 6, "duel6-report-sending").display());
            assert!(
                gpu.text_dialog
                    .answer(Report::Bug, Ok("report #12".to_owned()))
            );
            println!("{}", shoot(&mut gpu, 6, "duel6-report-sent").display());
            // Not sent, through the client's own path: the identity is off.
            gpu.text_dialog.open(Kind::Report);
            gpu.text_dialog.preview(TYPED, true, "");
            let action = gpu.text_dialog.send_for_shot();
            gpu.apply_dialog_action(action);
            assert!(gpu.text_dialog.is_open(), "the card shows why");
            println!("{}", shoot(&mut gpu, 6, "duel6-report-failed").display());
            gpu.text_dialog.open(Kind::PlayerReport {
                subject: "Kyle: Cheating".to_owned(),
                category: sjk_identity::Category::Cheating,
            });
            gpu.text_dialog
                .preview("Speed hacking and flying through walls all match", true, "");
            println!("{}", shoot(&mut gpu, 6, "duel6-report-player").display());
            gpu.text_dialog.open(Kind::Note {
                subject: "Wall: textures/yavin/wall_3 (lightmapped, BSP surface 812) on mp/duel6"
                    .to_owned(),
                model: None,
            });
            gpu.text_dialog.caret_for_shot();
            println!("{}", shoot(&mut gpu, 6, "duel6-report-note").display());
            gpu.text_dialog.open(Kind::Note {
                subject: concat!(
                    "misc_model_static models/map_objects/yavin/tree09_b.md3 ",
                    "(models/map_objects/yavin/tree09_b remapped to textures/video/raven, ",
                    "1 more shader), 412 units"
                )
                .to_owned(),
                model: Some("models/map_objects/yavin/tree09_b.md3".to_owned()),
            });
            gpu.text_dialog.preview(
                &"The bark looks too flat in this light. ".repeat(14),
                false,
                "",
            );
            gpu.text_dialog.caret_for_shot();
            println!(
                "{}",
                shoot(&mut gpu, 6, "duel6-report-note-model").display()
            );
            drop(gpu);
            let Some((mut gpu, _profile)) = in_match([1440, 1080]) else {
                return;
            };
            report(&mut gpu, "duel6-report-4x3", TYPED, "");
            drop(gpu);
            // Over the menu map, the main page under the card.
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 20);
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            report(&mut gpu, "duel6-report-menu", TYPED, "");
        });
    }

    /// Hand-placed candidates for the menu's camera tour on duel6, for review.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_tour_candidates() {
        on_big_stack(|| {
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [960, 540], None, &[]) else {
                return;
            };
            let views: &[(&str, [f32; 3], [f32; 3])] = &[
                (
                    "west wing (intermission)",
                    [-2224.0, 0.0, 888.0],
                    [-640.0, 0.0, 709.0],
                ),
                ("east wing", [2224.0, 0.0, 888.0], [640.0, 0.0, 709.0]),
                ("north wing", [0.0, 2224.0, 888.0], [0.0, 640.0, 709.0]),
                ("south wing", [0.0, -2224.0, 888.0], [0.0, -640.0, 709.0]),
                (
                    "high corner sw",
                    [-1100.0, -1100.0, 700.0],
                    [0.0, 0.0, 300.0],
                ),
                (
                    "high corner se",
                    [1100.0, -1100.0, 700.0],
                    [0.0, 0.0, 300.0],
                ),
                ("high corner ne", [1100.0, 1100.0, 700.0], [0.0, 0.0, 300.0]),
                (
                    "high corner nw",
                    [-1100.0, 1100.0, 700.0],
                    [0.0, 0.0, 300.0],
                ),
                (
                    "obelisk from below",
                    [-260.0, -260.0, 400.0],
                    [0.0, 0.0, 700.0],
                ),
                ("north bridge", [0.0, 1960.0, 760.0], [0.0, 0.0, 300.0]),
                (
                    "over the octagon wall",
                    [-560.0, 0.0, 700.0],
                    [0.0, 0.0, 300.0],
                ),
                (
                    "along the west wing",
                    [-2000.0, 600.0, 520.0],
                    [-2000.0, -600.0, 450.0],
                ),
                (
                    "quarter garden",
                    [-700.0, -1100.0, 460.0],
                    [-1400.0, -1400.0, 380.0],
                ),
                (
                    "south from the octagon",
                    [0.0, -1150.0, 330.0],
                    [0.0, -2000.0, 520.0],
                ),
                (
                    "down a diagonal arm",
                    [-380.0, -380.0, 470.0],
                    [-1000.0, -1000.0, 380.0],
                ),
                (
                    "east sunken court",
                    [1280.0, 200.0, 110.0],
                    [1280.0, -600.0, 160.0],
                ),
            ];
            sweep(&mut gpu, views, "duel6-tour-candidates");
        });
    }

    /// Every spawn and intermission point of duel6 at eye height, facing its
    /// own angle and its opposite, as a contact sheet: the raw material for
    /// the menu's camera tour.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn duel6_spawn_views() {
        on_big_stack(|| {
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [960, 540], None, &[]) else {
                return;
            };
            let entities = sjk_entity::parse_entity_lump(gpu.bsp.entities()).expect("entities");
            let mut views = Vec::new();
            for entity in &entities {
                let Some(class) = entity.classname() else {
                    continue;
                };
                if !class.starts_with("info_player") {
                    continue;
                }
                let Ok(Some(origin)) = entity.vector("origin") else {
                    continue;
                };
                let angle = entity.number("angle").ok().flatten().unwrap_or(0.0);
                for turn in [0.0, 180.0] {
                    views.push((class.to_owned(), origin, angle + turn));
                }
            }
            let _ = frame(&mut gpu, 30);
            let mut images = Vec::new();
            for (index, (class, origin, yaw)) in views.iter().enumerate() {
                aim(
                    &mut gpu,
                    [origin[0], origin[1], origin[2] + 40.0],
                    *yaw,
                    -4.0,
                );
                images.push(frame(&mut gpu, 6));
                println!("{index:2}: {class} {origin:?} yaw {yaw}");
            }
            println!("{}", sheet(&images, 4, 480, "duel6-spawns").display());
        });
    }
}
