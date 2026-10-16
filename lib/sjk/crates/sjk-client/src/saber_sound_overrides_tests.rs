use super::*;
use crate::LegacyLoopKind;
use sjk_protocol::{EntityState, GameState, LEGACY_ENTITY_FIELDS, PlayerState};

const CS_SOUNDS: usize = 811;
const CS_PLAYERS: usize = 1_131;
const SKINNED: u16 = 2;
const STOCK: u16 = 3;
const LOCAL: u16 = 0;

/// A blade skin's sounds, as a pack would name them.
const SET: SaberSoundSet<'static> = SaberSoundSet {
    on: "sound/test/blade/on.wav",
    off: "sound/test/blade/off.wav",
    hum: "sound/test/blade/hum.wav",
    swings: [
        "sound/test/blade/swing1.wav",
        "sound/test/blade/swing2.wav",
        "sound/test/blade/swing3.wav",
    ],
};

fn vfs() -> VirtualFileSystem {
    let mut vfs = VirtualFileSystem::new();
    let mut files: Vec<(String, Vec<u8>)> = [
        "sound/weapons/saber/saberon.wav",
        "sound/weapons/saber/saberoff.wav",
        "sound/weapons/saber/saberhum1.wav",
        "sound/weapons/explosion.wav",
        "sound/custom/ignite.wav",
        SET.on,
        SET.off,
        SET.hum,
        SET.swings[0],
        SET.swings[1],
        SET.swings[2],
    ]
    .into_iter()
    .map(|path| (path.to_owned(), b"wav".to_vec()))
    .collect();
    files.extend((1..=8).map(|n| {
        (
            format!("sound/weapons/saber/saberhup{n}.wav"),
            b"wav".to_vec(),
        )
    }));
    files.push((
        "ext_data/sabers/custom.sab".to_owned(),
        b"custom\n{\n\tname \"Custom\"\n\tsoundOn \"sound/custom/ignite.wav\"\n\tsoundOff \"sound/null.wav\"\n}\n".to_vec(),
    ));
    vfs.mount_memory("test", files).unwrap();
    vfs
}

fn game_state() -> GameState {
    let mut game = GameState::empty_local(i32::from(LOCAL));
    for (slot, path) in [
        (1, "sound/weapons/saber/saberon.wav"),
        // The game names the stock sounds .wav; the client also finds .mp3.
        (2, "sound/weapons/saber/saberoff.mp3"),
        (3, "sound/custom/ignite.wav"),
        (4, "sound/weapons/explosion.wav"),
    ] {
        game.replace_config_string(CS_SOUNDS + slot, path.as_bytes().to_vec())
            .unwrap();
    }
    for client in [LOCAL, SKINNED, STOCK] {
        game.replace_config_string(
            CS_PLAYERS + usize::from(client),
            b"n\\Player\\model\\kyle/default".to_vec(),
        )
        .unwrap();
    }
    game
}

fn adapter() -> LegacySoundAdapter {
    let vfs = vfs();
    let mut next = 0;
    let mut register = |_: &str, _: &[u8]| {
        next += 1;
        Some(SoundHandle(next))
    };
    let mut adapter = LegacySoundAdapter::new(&game_state(), &vfs, &mut register);
    adapter.register_saber_sound_sets(&[SET], &vfs, &mut register);
    adapter
}

fn player_entity(number: u16, origin: [f32; 3]) -> EntityState {
    let mut state = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(8, 1); // ET_PLAYER
    state.set_raw_field(32, u32::from(number)); // clientNum
    state.set_raw_field(14, 3); // WP_SABER
    for (field, value) in [2, 1, 4].into_iter().zip(origin) {
        state.set_raw_field(field, value.to_bits());
    }
    state
}

fn with_event(mut state: EntityState, event: u16, parameter: u8) -> EntityState {
    state.set_raw_field(28, u32::from(event));
    state.set_raw_field(42, u32::from(parameter));
    state
}

/// `G_Sound`'s temporary entity: `EV_GENERAL_SOUND` with no owner at `origin`.
fn general_sound(number: u16, slot: u8, origin: [f32; 3]) -> EntityState {
    let mut state = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(8, 18 + 76); // ET_EVENTS + EV_GENERAL_SOUND
    state.set_raw_field(42, u32::from(slot));
    for (field, value) in [2, 1, 4].into_iter().zip(origin) {
        state.set_raw_field(field, value.to_bits());
    }
    state
}

fn snapshot(entities: Vec<EntityState>) -> Snapshot {
    let mut player = PlayerState::zero();
    player.set_client_num(LOCAL);
    player.set_origin([-500.0, 0.0, 0.0]);
    Snapshot {
        message_sequence: 1,
        reliable_acknowledge: 0,
        server_commands: Vec::new(),
        server_time: 1_000,
        delta_from: None,
        flags: 0,
        area_mask: Vec::new(),
        player,
        vehicle_player: None,
        entities,
        consumed_bits: 0,
    }
}

const AT_SKINNED: [f32; 3] = [0.0, 0.0, 0.0];
const AT_STOCK: [f32; 3] = [300.0, 0.0, 0.0];

/// The paths the snapshot's events play, by event source, in order.
fn played(adapter: &mut LegacySoundAdapter, entities: Vec<EntityState>) -> Vec<(u16, String)> {
    adapter.observe_snapshot(&snapshot(entities));
    adapter
        .decisions()
        .iter()
        .map(|decision| {
            let path = decision
                .sound
                .and_then(|sound| adapter.sound(sound))
                .map_or_else(String::new, |sound| sound.path.to_string());
            (decision.request.source.0 as u16, path)
        })
        .collect()
}

fn wear(adapter: &mut LegacySoundAdapter, client: u16) {
    let mut clients = [None; SABER_SOUND_CLIENTS];
    clients[usize::from(client)] = Some(0);
    adapter.set_saber_sound_overrides(&clients);
}

#[test]
fn without_a_set_every_saber_sound_is_stock() {
    let mut adapter = adapter();
    let sounds = played(
        &mut adapter,
        vec![
            with_event(player_entity(SKINNED, AT_SKINNED), 33, 0),
            with_event(player_entity(STOCK, AT_STOCK), 29, 0),
            general_sound(100, 1, AT_SKINNED),
        ],
    );
    assert_eq!(sounds.len(), 3);
    assert_eq!(
        sounds[0],
        (SKINNED, "sound/weapons/saber/saberon.wav".to_owned())
    );
    assert_eq!(sounds[1].0, STOCK);
    assert!(sounds[1].1.starts_with("sound/weapons/saber/saberhup"));
    assert_eq!(
        sounds[2],
        (100, "sound/weapons/saber/saberon.wav".to_owned())
    );
}

#[test]
fn only_the_wearer_ignites_and_swings_with_the_set_over_the_stock_sounds() {
    let mut adapter = adapter();
    wear(&mut adapter, SKINNED);
    let stock_on = "sound/weapons/saber/saberon.wav".to_owned();
    let sounds = played(
        &mut adapter,
        vec![
            with_event(player_entity(SKINNED, AT_SKINNED), 33, 0),
            with_event(player_entity(STOCK, AT_STOCK), 33, 0),
        ],
    );
    // The wearer's stock ignition stays, the set's on top of it.
    assert_eq!(
        sounds,
        [
            (SKINNED, stock_on.clone()),
            (SKINNED, SET.on.to_owned()),
            (STOCK, stock_on),
        ]
    );
    let decisions = adapter.decisions();
    assert!(!decisions[0].additional && decisions[1].additional && !decisions[2].additional);
    // Swings: every parameter adds one of the set's three over the stock swing, for the
    // wearer only.
    let mut heard = std::collections::BTreeSet::new();
    for round in 0..24_u16 {
        // The event's two sequence bits tell each round's event from the last.
        let event = 29 | (round % 4) << 8;
        let sounds = played(
            &mut adapter,
            vec![
                with_event(player_entity(SKINNED, AT_SKINNED), event, round as u8),
                with_event(player_entity(STOCK, AT_STOCK), event, round as u8),
            ],
        );
        assert_eq!(sounds.len(), 3, "{sounds:?}");
        assert!(sounds[0].1.starts_with("sound/weapons/saber/saberhup"));
        assert!(SET.swings.contains(&sounds[1].1.as_str()), "{sounds:?}");
        assert!(sounds[2].1.starts_with("sound/weapons/saber/saberhup"));
        // The set's swing is on a channel of its own, so the stock swing is not cut.
        let decisions = adapter.decisions();
        assert_ne!(decisions[0].request.channel, decisions[1].request.channel);
        heard.insert(sounds[1].1.clone());
    }
    assert_eq!(heard.len(), 3);
}

#[test]
fn a_general_saber_toggle_follows_the_nearest_player() {
    let mut adapter = adapter();
    wear(&mut adapter, SKINNED);
    let near = [10.0, 20.0, 0.0];
    let sounds = played(
        &mut adapter,
        vec![
            player_entity(SKINNED, AT_SKINNED),
            player_entity(STOCK, AT_STOCK),
            // Stock on and off, a .sab's own soundOn, a sound that is no toggle.
            general_sound(100, 1, near),
            general_sound(101, 2, near),
            general_sound(102, 3, near),
            general_sound(103, 4, near),
            // At the unskinned player, and far from everyone.
            general_sound(104, 1, [290.0, 0.0, 0.0]),
            general_sound(105, 1, [150.0, 0.0, 0.0]),
        ],
    );
    let paths: Vec<&str> = sounds.iter().map(|(_, path)| path.as_str()).collect();
    // The wearer's toggles keep the stock sound and add the set's after it.
    assert_eq!(
        paths,
        [
            "sound/weapons/saber/saberon.wav",
            SET.on,
            "sound/weapons/saber/saberoff.wav",
            SET.off,
            "sound/custom/ignite.wav",
            SET.on,
            "sound/weapons/explosion.wav",
            "sound/weapons/saber/saberon.wav",
            "sound/weapons/saber/saberon.wav",
        ]
    );
}

#[test]
fn the_local_player_is_found_by_its_own_origin() {
    let mut adapter = adapter();
    wear(&mut adapter, LOCAL);
    let sounds = played(
        &mut adapter,
        vec![general_sound(100, 2, [-490.0, 0.0, 0.0])],
    );
    assert_eq!(sounds.last().unwrap().1, SET.off);
    assert_eq!(sounds.len(), 2);
}

#[test]
fn the_wearer_hums_with_the_set_over_the_stock_hum() {
    let mut adapter = adapter();
    let hums = |adapter: &mut LegacySoundAdapter| {
        let snapshot = snapshot(vec![
            player_entity(SKINNED, AT_SKINNED),
            player_entity(STOCK, AT_STOCK),
        ]);
        adapter.observe_loops(&snapshot, 1_000, [0.0; 3], |state| state.trajectory_base());
        adapter
            .loop_decisions()
            .iter()
            .filter(|decision| {
                matches!(
                    decision.kind,
                    LegacyLoopKind::SaberHumPrimary | LegacyLoopKind::SaberHumSkin
                )
            })
            .map(|decision| {
                (
                    decision.request.source.0 as u16,
                    adapter
                        .loop_sound(decision.sound.unwrap())
                        .unwrap()
                        .path
                        .to_string(),
                )
            })
            .collect::<Vec<_>>()
    };
    let stock = "sound/weapons/saber/saberhum1.wav".to_owned();
    assert_eq!(
        hums(&mut adapter),
        [(SKINNED, stock.clone()), (STOCK, stock.clone())]
    );
    wear(&mut adapter, SKINNED);
    assert_eq!(
        adapter.saber_sound_overrides()[usize::from(SKINNED)],
        Some(0)
    );
    assert_eq!(
        hums(&mut adapter),
        [
            (SKINNED, stock.clone()),
            (SKINNED, SET.hum.to_owned()),
            (STOCK, stock.clone())
        ]
    );
    adapter.set_saber_sound_overrides(&[None; SABER_SOUND_CLIENTS]);
    assert_eq!(
        hums(&mut adapter),
        [(SKINNED, stock.clone()), (STOCK, stock)]
    );
}

/// A thrown saber (`ET_GENERAL`, `WP_SABER`, its owner in `genericenemyindex`) humming
/// with the game's `loopSound` in sound slot 5.
fn thrown_saber(number: u16, owner: u16) -> EntityState {
    let mut state = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(14, 3); // WP_SABER
    state.set_raw_field(18, 1_024 + u32::from(owner));
    state.set_raw_field(55, 5); // loopSound
    state
}

#[test]
fn a_thrown_saber_hums_with_its_owners_set_over_its_own() {
    let mut game = game_state();
    game.replace_config_string(CS_SOUNDS + 5, b"sound/weapons/saber/saberhum1.wav".to_vec())
        .unwrap();
    let vfs = vfs();
    let mut next = 0;
    let mut register = |_: &str, _: &[u8]| {
        next += 1;
        Some(SoundHandle(next))
    };
    let mut adapter = LegacySoundAdapter::new(&game, &vfs, &mut register);
    adapter.register_saber_sound_sets(&[SET], &vfs, &mut register);
    let flying = |adapter: &mut LegacySoundAdapter| {
        let snapshot = snapshot(vec![thrown_saber(200, SKINNED), thrown_saber(201, STOCK)]);
        adapter.observe_loops(&snapshot, 1_000, [0.0; 3], |state| state.trajectory_base());
        adapter
            .loop_decisions()
            .iter()
            .map(|decision| {
                (
                    decision.request.source.0 as u16,
                    adapter
                        .loop_sound(decision.sound.unwrap())
                        .unwrap()
                        .path
                        .to_string(),
                )
            })
            .collect::<Vec<_>>()
    };
    let stock = "sound/weapons/saber/saberhum1.wav".to_owned();
    assert_eq!(
        flying(&mut adapter),
        [(200, stock.clone()), (201, stock.clone())]
    );
    wear(&mut adapter, SKINNED);
    assert_eq!(
        flying(&mut adapter),
        [
            (200, stock.clone()),
            (200, SET.hum.to_owned()),
            (201, stock)
        ]
    );
}

#[test]
fn a_set_registered_mid_session_is_worn_once_the_clients_are_given_again() {
    // The tables were built before the skin's pack came: no set.
    let vfs = vfs();
    let mut next = 0;
    let mut register = |_: &str, _: &[u8]| {
        next += 1;
        Some(SoundHandle(next))
    };
    let mut adapter = LegacySoundAdapter::new(&game_state(), &vfs, &mut register);
    adapter.register_saber_sound_sets(&[], &vfs, &mut register);
    wear(&mut adapter, SKINNED);
    assert_eq!(
        adapter.saber_sound_overrides(),
        &[None; SABER_SOUND_CLIENTS]
    );
    // The pack arrives: its set is registered, and the viewer gives the clients again.
    adapter.register_saber_sound_sets(&[SET], &vfs, &mut register);
    wear(&mut adapter, SKINNED);
    assert_eq!(
        adapter.saber_sound_overrides()[usize::from(SKINNED)],
        Some(0)
    );
    let sounds = played(
        &mut adapter,
        vec![with_event(player_entity(SKINNED, AT_SKINNED), 33, 0)],
    );
    assert_eq!(
        sounds,
        [
            (SKINNED, "sound/weapons/saber/saberon.wav".to_owned()),
            (SKINNED, SET.on.to_owned())
        ]
    );
}

#[test]
fn an_unregistered_set_is_not_worn() {
    let mut adapter = adapter();
    let mut clients = [None; SABER_SOUND_CLIENTS];
    clients[usize::from(SKINNED)] = Some(5);
    adapter.set_saber_sound_overrides(&clients);
    assert_eq!(
        adapter.saber_sound_overrides(),
        &[None; SABER_SOUND_CLIENTS]
    );
    let sounds = played(
        &mut adapter,
        vec![with_event(player_entity(SKINNED, AT_SKINNED), 33, 0)],
    );
    assert_eq!(sounds[0].1, "sound/weapons/saber/saberon.wav");
}

#[test]
fn toggles_are_told_by_stem_whatever_the_extension_or_case() {
    let mut overrides = SaberSoundOverrides::default();
    overrides
        .toggles
        .push(("sound/custom/ignite".into(), Toggle::On));
    assert_eq!(
        overrides.toggle("sound/weapons/saber/saberon.mp3"),
        Some(Toggle::On)
    );
    assert_eq!(
        overrides.toggle("Sound/Weapons/Saber/SaberOffQuick.wav"),
        Some(Toggle::Off)
    );
    assert_eq!(
        overrides.toggle("sound/custom/IGNITE.mp3"),
        Some(Toggle::On)
    );
    assert_eq!(overrides.toggle("sound/weapons/saber/saberhum1.wav"), None);
    assert_eq!(overrides.toggle("sound/other/saberon.wav"), None);
}
