//! World shots of the player's own world notes (`world_notes.rs`): each note in
//! `<GameData>/SJK/notes.jsonl` seen again from where it was written, to check a
//! reported surface without starting the game. Ignored like the other world shots.
//!
//! `SJK_NOTES` picks notes by their line number (from 0) or by part of a map name, a
//! shader name or the note's time (`SJK_NOTES=12,13`, `SJK_NOTES=ffa1` or
//! `SJK_NOTES=09:41:20`, which stays right when earlier notes are removed); without it
//! every note is shot. Each
//! note gives one sheet of four views: as written, then with a red point light (a
//! saber's) a little in front of the noted point; and the same two from halfway
//! closer, so a surface that dynamic light does not reach stands out. The player's
//! own renderer settings (`r_*` in `<GameData>/SJK/config.cfg`) apply, and
//! `SJK_NOTES_CVARS` (`name=value,name=value`) changes some for a comparison;
//! `SJK_NOTES_REMAP` (`old=new;old=new`) remaps shaders as a server would. A note that
//! recorded the remaps active when it was taken (`remaps`, since 13/10/2026) has its
//! server and console ones made again for its views; the map's own apply anyway. The
//! player's generated packs (`<GameData>/SJK/generated`) are mounted as the game mounts
//! them; `SJK_NOTES_GENERATED` names another client folder, whose `generated` folder
//! holds the packs to use instead (to compare an older pack).

use super::*;
use crate::dynamic_lights::PointLight;

/// One note's view: where it was written from and what it hit.
struct NoteView {
    line: usize,
    map: String,
    /// When it was written (`dd/mm/yyyy hh:mm:ss UTC`).
    time: String,
    eye: [f32; 3],
    yaw: f32,
    /// Degrees down, as the note's pose writes it.
    pitch_down: f32,
    hit: [f32; 3],
    normal: [f32; 3],
    shader: String,
    /// The server and console remaps the note recorded: (shader, drawn instead).
    remaps: Vec<(String, String)>,
    /// The model the note is about, if any.
    model: String,
}

fn read_notes(game_data: &std::path::Path) -> Vec<NoteView> {
    let path = game_data.join("SJK").join("notes.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("no notes at {}", path.display());
        return Vec::new();
    };
    let vector = |value: &serde_json::Value| -> Option<[f32; 3]> {
        let items = value.as_array()?;
        Some([
            items.first()?.as_f64()? as f32,
            items.get(1)?.as_f64()? as f32,
            items.get(2)?.as_f64()? as f32,
        ])
    };
    let mut notes = Vec::new();
    for (line, row) in text.lines().enumerate() {
        let Ok(note) = serde_json::from_str::<serde_json::Value>(row) else {
            continue;
        };
        // "(x y z) : yaw pitch"
        let pose = note["pose"].as_str().unwrap_or_default();
        let numbers: Vec<f32> = pose
            .split(|c: char| c == '(' || c == ')' || c == ':' || c.is_whitespace())
            .filter_map(|part| part.parse().ok())
            .collect();
        let (Some(hit), Some(normal), [x, y, z, yaw, pitch_down]) = (
            vector(&note["hit"]),
            vector(&note["normal"]),
            numbers.as_slice(),
        ) else {
            continue;
        };
        notes.push(NoteView {
            line,
            map: note["map"].as_str().unwrap_or_default().to_owned(),
            time: note["time"].as_str().unwrap_or_default().to_owned(),
            eye: [*x, *y, *z],
            yaw: *yaw,
            pitch_down: *pitch_down,
            hit,
            normal,
            shader: note["surface"]["shader"]
                .as_str()
                .or_else(|| note["model"]["shaders"][0]["shader"].as_str())
                .unwrap_or_default()
                .to_owned(),
            remaps: note["remaps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|remap| remap["source"].as_str() != Some("map"))
                .filter_map(|remap| {
                    Some((
                        remap["from"].as_str()?.to_owned(),
                        remap["to"].as_str()?.to_owned(),
                    ))
                })
                .collect(),
            model: note["model"]["path"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        });
    }
    notes
}

/// The player's renderer settings (`seta r_* "value"` in `<GameData>/SJK/config.cfg`),
/// so the shots are lit as the notes were seen.
fn renderer_settings(game_data: &std::path::Path) -> Vec<(String, String)> {
    let text =
        std::fs::read_to_string(game_data.join("SJK").join("config.cfg")).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            if words.next()? != "seta" {
                return None;
            }
            let name = words.next()?;
            let value = words.next()?.trim_matches('"');
            // Window and display settings stay the shot's own.
            let window = [
                "r_mode",
                "r_fullscreen",
                "r_custom",
                "r_resolution",
                "r_superSample",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix));
            (name.starts_with("r_") && !window).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// The notes `SJK_NOTES` picks and the renderer settings to shoot them with: the
/// player's own, then `SJK_NOTES_CVARS`.
fn chosen_notes() -> (Vec<NoteView>, Vec<(String, String)>) {
    let game_data = PathBuf::from(
        std::env::var_os("JKA_GAME_DATA").expect("JKA_GAME_DATA names the GameData directory"),
    );
    let filter = std::env::var("SJK_NOTES").unwrap_or_default();
    let notes = read_notes(&game_data)
        .into_iter()
        .filter(|note| wanted(note, &filter))
        .collect();
    // The player's generated packs (`generated_packs`), as the game mounts them: the
    // throwaway profile's own folder has none, so without this the shots showed an
    // older pack left in `base` instead of the installed one. `SJK_NOTES_GENERATED`
    // names another client folder (whose `generated` holds the packs) to compare.
    let client_folder = std::env::var_os("SJK_NOTES_GENERATED")
        .map_or_else(|| game_data.join("SJK"), PathBuf::from);
    crate::assets::generated::locate_at_start(&client_folder);
    let mut settings = renderer_settings(&game_data);
    // `SJK_NOTES_CVARS=r_weather=1,r_weatherFog=2` overrides settings for a comparison.
    for pair in std::env::var("SJK_NOTES_CVARS")
        .unwrap_or_default()
        .split(',')
    {
        if let Some((name, value)) = pair.split_once('=') {
            settings.retain(|(known, _)| !known.eq_ignore_ascii_case(name));
            settings.push((name.to_owned(), value.to_owned()));
        }
    }
    (notes, settings)
}

fn wanted(note: &NoteView, filter: &str) -> bool {
    filter.is_empty()
        || filter.split(',').any(|part| {
            let part = part.trim();
            part.parse::<usize>().map_or_else(
                |_| {
                    let part = part.to_ascii_lowercase();
                    [&note.map, &note.shader, &note.time]
                        .iter()
                        .any(|text| text.to_ascii_lowercase().contains(&part))
                },
                |line| line == note.line,
            )
        })
}

/// Every chosen note's four views, one sheet per note in `target/world-shots`.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn world_notes() {
    on_big_stack(|| {
        let (notes, settings) = chosen_notes();
        let cvars: Vec<(&str, &str)> = settings
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let mut maps: Vec<&str> = notes.iter().map(|note| note.map.as_str()).collect();
        maps.dedup();
        for map in maps {
            let Some((mut gpu, _profile)) = open(map, [1280, 720], None, &cvars) else {
                return;
            };
            // `SJK_NOTES_REMAP=old=new;old=new`: shader remaps as a server sends them,
            // made as the console's `remapShader` makes them.
            let asked: Vec<(String, String)> = std::env::var("SJK_NOTES_REMAP")
                .unwrap_or_default()
                .split(';')
                .filter_map(|pair| pair.split_once('='))
                .map(|(old, new)| (old.to_owned(), new.to_owned()))
                .collect();
            let _ = frame(&mut gpu, 30);
            for note in notes.iter().filter(|note| note.map == map) {
                // The note's own remaps, then the asked ones; the frame's own remap
                // refresh applies them, as after the console command.
                gpu.world_materials.clear_console_remaps();
                let vfs = gpu.vfs.clone().expect("a map's files");
                for (old, new) in note.remaps.iter().chain(&asked) {
                    if let Err(error) =
                        gpu.world_materials
                            .local_remap(&vfs, &gpu.shaders, old, new)
                    {
                        eprintln!("remap {old} -> {new}: {error}");
                    }
                }
                let mut images = Vec::new();
                aim(&mut gpu, note.eye, note.yaw, -note.pitch_down);
                gpu.effect_aux.held_lights.clear();
                images.push(frame(&mut gpu, 8));
                // A saber's light (codemp `CG_AddSaberBlade`: red, twice the blade's
                // length) 48 units off the noted point.
                let origin: [f32; 3] =
                    std::array::from_fn(|axis| note.hit[axis] + note.normal[axis] * 48.0);
                gpu.effect_aux.held_lights.push(PointLight {
                    origin,
                    radius: 200.0,
                    color: [1.0, 0.2, 0.2],
                });
                images.push(frame(&mut gpu, 8));
                // Closer: halfway from the eye to the point.
                let near: [f32; 3] =
                    std::array::from_fn(|axis| (note.eye[axis] + note.hit[axis]) * 0.5);
                aim(&mut gpu, near, note.yaw, -note.pitch_down);
                images.push(frame(&mut gpu, 8));
                gpu.effect_aux.held_lights.clear();
                images.push(frame(&mut gpu, 8));
                let name = format!("note-{:02}", note.line);
                println!(
                    "{}: {} {}{} ({} remaps) {}",
                    note.line,
                    map,
                    note.shader,
                    if note.model.is_empty() {
                        String::new()
                    } else {
                        format!(" on {}", note.model)
                    },
                    note.remaps.len(),
                    sheet(&images, 2, 960, &name).display()
                );
            }
        }
    });
}

/// A model noted through the client's own path, over duel6 as in a match with the SJK
/// UI: aimed at one of its rocks (`misc_model_static` props), the first `worldnote`
/// press selects the prop, outlined and shaded; the second opens the note card naming
/// it, with Clear model; Clear model then notes the ground behind it.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_note_model() {
    on_big_stack(|| {
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
        ];
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], None, &cvars)
        else {
            return;
        };
        if let Some(console) = gpu.console.as_mut() {
            console.close_for_connection();
        }
        // duel6's three rocks (`models/map_objects/desert/rock.md3`), each seen from a
        // few places round it: the first view that selects a model is shot.
        let rocks = [
            [324.0, -1088.0, 150.0],
            [1352.0, 388.0, 46.0],
            [-1740.0, 380.0, 182.0],
        ];
        let offsets = [
            [-200.0, 40.0, 60.0],
            [200.0, -40.0, 60.0],
            [40.0, 200.0, 60.0],
            [-40.0, -200.0, 60.0],
        ];
        let mut found = None;
        'search: for rock in rocks {
            for offset in offsets {
                let from: [f32; 3] = std::array::from_fn(|axis| rock[axis] + offset[axis]);
                let (yaw, pitch) = look(from, rock);
                aim(&mut gpu, from, yaw, pitch);
                let _ = frame(&mut gpu, 4);
                gpu.world_note_press();
                let summary = gpu.world_notes.waiting().unwrap_or_default();
                println!("{from:?}: {summary}");
                if summary.contains(".md3") {
                    found = Some(summary);
                    break 'search;
                }
                gpu.world_notes.cancel_selection();
            }
        }
        let summary = found.expect("a rock under the crosshair");
        assert!(summary.contains("rock.md3"), "{summary}");
        println!(
            "{}",
            shoot(&mut gpu, 10, "duel6-note-model-selected").display()
        );
        gpu.world_note_press();
        assert!(gpu.text_dialog.is_open(), "the note card");
        gpu.text_dialog.preview(
            "The rock's edges look too sharp against the grass.",
            false,
            "",
        );
        gpu.text_dialog.caret_for_shot();
        println!("{}", shoot(&mut gpu, 6, "duel6-note-model-card").display());
        gpu.world_note_clear_model();
        println!(
            "{}",
            shoot(&mut gpu, 6, "duel6-note-model-cleared").display()
        );
    });
}

/// Flicker hunt for the chosen notes. From the note's eye (or `SJK_NOTES_BACK` units
/// behind it along the view), six frames held still and a map of what changed between
/// them (`note-NN-still`: white where a pixel moved by more than a few levels), then
/// eight views `SJK_NOTES_STEP` units apart (3 by default) to the right and up
/// (`note-NN-steps`; with `SJK_NOTES_CROP` only the middle quarter of each, round the
/// noted point). Z-fighting stays still while the camera does and changes from step to
/// step. `SJK_NOTES_SWEEP` shoots instead the view from 0 to 90 units behind the eye
/// (`note-NN-sweep`), the room a third-person camera has.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn world_note_flicker() {
    on_big_stack(|| {
        let (notes, settings) = chosen_notes();
        let cvars: Vec<(&str, &str)> = settings
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let number = |name: &str, default: f32| -> f32 {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default)
        };
        let back = number("SJK_NOTES_BACK", 0.0);
        let stride = number("SJK_NOTES_STEP", 3.0);
        let crop = std::env::var_os("SJK_NOTES_CROP").is_some();
        for note in &notes {
            let Some((mut gpu, _profile)) = open(&note.map, [1280, 720], None, &cvars) else {
                return;
            };
            let _ = frame(&mut gpu, 30);
            let (yaw, pitch) = (note.yaw.to_radians(), (-note.pitch_down).to_radians());
            let forward = [
                yaw.cos() * pitch.cos(),
                yaw.sin() * pitch.cos(),
                pitch.sin(),
            ];
            let behind = |distance: f32| -> [f32; 3] {
                std::array::from_fn(|axis| note.eye[axis] - forward[axis] * distance)
            };
            let name = format!("note-{:02}", note.line);
            if std::env::var_os("SJK_NOTES_SWEEP").is_some() {
                let views: Vec<(String, [f32; 3], [f32; 3])> = (0..16)
                    .map(|step| {
                        let distance = step as f32 * 6.0;
                        let from = behind(distance);
                        let at = std::array::from_fn(|axis| from[axis] + forward[axis] * 100.0);
                        (format!("back {distance}"), from, at)
                    })
                    .collect();
                let views: Vec<(&str, [f32; 3], [f32; 3])> = views
                    .iter()
                    .map(|(label, from, at)| (label.as_str(), *from, *at))
                    .collect();
                sweep(&mut gpu, &views, &format!("{name}-sweep"));
                continue;
            }
            let eye = behind(back);
            aim(&mut gpu, eye, note.yaw, -note.pitch_down);
            let still: Vec<image::RgbaImage> = (0..6).map(|_| frame(&mut gpu, 1)).collect();
            let mut changed =
                image::RgbaImage::from_fn(still[0].width(), still[0].height(), |x, y| {
                    let grey = still[0].get_pixel(x, y)[0] / 4;
                    image::Rgba([grey, grey, grey, 255])
                });
            for pair in still.windows(2) {
                for (x, y, pixel) in changed.enumerate_pixels_mut() {
                    let (a, b) = (pair[0].get_pixel(x, y), pair[1].get_pixel(x, y));
                    if (0..3).any(|c| a[c].abs_diff(b[c]) > 6) {
                        *pixel = image::Rgba([255, 255, 255, 255]);
                    }
                }
            }
            let right = [yaw.sin(), -yaw.cos(), 0.0];
            let steps: Vec<image::RgbaImage> = (0..8)
                .map(|step| {
                    let offset = step as f32 * stride;
                    let origin: [f32; 3] = std::array::from_fn(|axis| {
                        eye[axis]
                            + right[axis] * offset
                            + if axis == 2 { offset * 0.5 } else { 0.0 }
                    });
                    aim(&mut gpu, origin, note.yaw, -note.pitch_down);
                    let shot = frame(&mut gpu, 2);
                    if crop {
                        let (width, height) = (shot.width() / 4, shot.height() / 4);
                        image::imageops::crop_imm(
                            &shot,
                            width * 3 / 2,
                            height * 3 / 2,
                            width,
                            height,
                        )
                        .to_image()
                    } else {
                        shot
                    }
                })
                .collect();
            println!(
                "{}: still {} steps {}",
                note.line,
                sheet(
                    &[still[0].clone(), changed],
                    2,
                    960,
                    &format!("{name}-still")
                )
                .display(),
                sheet(&steps, 4, 480, &format!("{name}-steps")).display()
            );
        }
    });
}
