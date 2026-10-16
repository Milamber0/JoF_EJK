//! World shots of movers' second models ([`crate::mover_models`]) without a session:
//! `t2_rancor`'s bar, where Sol noted "tables are missing on this map" (08/10/2026).
//! Its tables are `func_usable` clip brushes with `model2` table md3s; the shot builds
//! the entities as this repository's server spawns them (`use_key::spawn_usable`,
//! `Usable::project`) and shows the bar without them, then with them.
//! Ignored like the other world shots.

use super::*;

/// `CS_MODELS` (`bg_public.h`).
const CS_MODELS: usize = 298;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn rancor_tables() {
    on_big_stack(|| {
        let Some((mut gpu, _profile)) = open("maps/t2_rancor.bsp", [1280, 720], None, &[]) else {
            return;
        };
        gpu.is_menu_world = false;
        // The note's own view, then the bar from above its door.
        let views: [([f32; 3], f32, f32); 2] = [
            ([-2846.0, 2334.0, 1129.0], 277.0, -12.5),
            ([-2700.0, 2250.0, 1200.0], 135.0, -30.0),
        ];
        let mut images = Vec::new();
        for (eye, yaw, pitch) in views {
            aim(&mut gpu, eye, yaw, pitch);
            images.push(frame(&mut gpu, 20));
        }
        let (game, snapshot) = usables(&gpu.bsp);
        for state in &snapshot.entities {
            if let Some(model) = sjk_client::legacy_mover_model2(&game, state) {
                gpu.load_config_model(&model.appearance)
                    .expect("the table model");
            }
        }
        println!(
            "{} func_usable entities sent, {} with a model2",
            snapshot.entities.len(),
            snapshot
                .entities
                .iter()
                .filter(|state| state.model_index2() != 0)
                .count()
        );
        gpu.resident.scenery = Some((game, snapshot));
        for (eye, yaw, pitch) in views {
            aim(&mut gpu, eye, yaw, pitch);
            images.push(frame(&mut gpu, 20));
        }
        println!("{}", sheet(&images, 2, 960, "rancor-tables").display());
    });
}

/// The map's `func_usable`s that are there, as the server sends them: numbered from 100,
/// their `model2` names registered in `CS_MODELS`.
fn usables(bsp: &sjk_bsp::Bsp) -> (sjk_protocol::GameState, sjk_protocol::Snapshot) {
    let mut game = sjk_protocol::GameState::empty_local(0);
    let mut names: Vec<String> = Vec::new();
    let mut entities = Vec::new();
    for entity in sjk_entity::parse_entity_lump(bsp.entities()).unwrap_or_default() {
        let Some(model) = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(bounds) = bsp
            .render()
            .models()
            .get(model)
            .map(|model| (model.minimums, model.maximums))
        else {
            continue;
        };
        let Some(usable) = sjk_game_jka::use_key::spawn_usable(&entity, bounds) else {
            continue;
        };
        if !usable.present {
            continue;
        }
        let model2 = if usable.model2.is_empty() {
            0
        } else {
            let index = match names.iter().position(|name| *name == usable.model2) {
                Some(index) => index + 1,
                None => {
                    names.push(usable.model2.clone());
                    game.replace_config_string(
                        CS_MODELS + names.len(),
                        usable.model2.clone().into_bytes(),
                    )
                    .expect("a model config string");
                    names.len()
                }
            };
            index as u16
        };
        let number = 100 + entities.len() as u16;
        let mut state =
            sjk_protocol::EntityState::zero(number, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        usable.project(&mut state, model2);
        entities.push(state);
    }
    let snapshot = sjk_protocol::Snapshot {
        message_sequence: 0,
        reliable_acknowledge: 0,
        server_commands: Vec::new(),
        server_time: 0,
        delta_from: None,
        flags: 0,
        area_mask: Vec::new(),
        player: Default::default(),
        vehicle_player: None,
        entities,
        consumed_bits: 0,
    };
    (game, snapshot)
}
