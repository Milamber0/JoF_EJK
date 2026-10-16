//! Where entity event sounds are placed (`CG_SetEntitySoundPosition`).
use super::*;
use sjk_protocol::{LEGACY_ENTITY_FIELDS, PlayerState};

/// Entity netfields (`codemp/qcommon/msg.cpp`).
const TR_BASE: [usize; 3] = [2, 1, 4];
const E_TYPE: usize = 8;
const SOLID: usize = 26;
const EVENT: usize = 28;
const EVENT_PARM: usize = 42;
const MODEL_INDEX: usize = 46;
const SOUND_SET_INDEX: usize = 78;
const ET_GENERAL: u32 = 0;
const ET_MOVER: u32 = 6;
/// `entity_event_t` (`codemp/game/bg_public.h`): 71 is what `G_PlayDoorSound` raises,
/// 73 the script and `func_breakable` bmodel sound.
const EV_PLAYDOORSOUND: u32 = 71;
const EV_BMODEL_SOUND: u32 = 73;
const EV_EVENT_BIT1: u32 = 0x100;
const EV_EVENT_BIT2: u32 = 0x200;
const BMS_START: u32 = 0;
const BMS_END: u32 = 2;
const DOOR: u16 = 100;
/// The door's inline model, `*1`, centred well away from the world origin as
/// most doors are: their entity origin stays at (0, 0, 0).
const DOOR_MIDPOINT: [f32; 3] = [1_536.0, -768.0, 96.0];

fn adapter() -> LegacySoundAdapter {
    let mut vfs = VirtualFileSystem::new();
    vfs.mount_memory(
        "test",
        [
            (
                "sound/sound.txt",
                b"bmodelSet door1\nsubWaves movers/doors start mid end\n".to_vec(),
            ),
            ("sound/movers/doors/start.wav", b"start".to_vec()),
            ("sound/movers/doors/mid.wav", b"mid".to_vec()),
            ("sound/movers/doors/end.wav", b"end".to_vec()),
        ],
    )
    .unwrap();
    let mut game = GameState::empty_local(0);
    game.replace_config_string(CS_AMBIENT_SET + 1, b"door1".to_vec())
        .unwrap();
    let mut next = 0;
    let mut adapter = LegacySoundAdapter::new(&game, &vfs, |_, _| {
        next += 1;
        Some(SoundHandle(next))
    });
    adapter.set_inline_model_midpoints(Box::new([[0.0; 3], DOOR_MIDPOINT]));
    adapter
}

/// A `func_door` without an origin brush: a brush model whose origin is the
/// world origin, moved by its trajectory.
fn door(event: u32, parameter: u32) -> EntityState {
    let mut state = EntityState::zero(DOOR, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(E_TYPE, ET_MOVER);
    state.set_raw_field(SOLID, SOLID_BMODEL);
    state.set_raw_field(MODEL_INDEX, 1);
    state.set_raw_field(SOUND_SET_INDEX, 1);
    state.set_raw_field(EVENT, event);
    state.set_raw_field(EVENT_PARM, parameter);
    state
}

fn snapshot(server_time: i32, entities: Vec<EntityState>) -> Snapshot {
    Snapshot {
        message_sequence: 0,
        reliable_acknowledge: 0,
        server_commands: Vec::new(),
        server_time,
        delta_from: None,
        flags: 0,
        area_mask: Vec::new(),
        player: PlayerState::zero(),
        vehicle_player: None,
        entities,
        consumed_bits: 0,
    }
}

#[test]
fn a_door_sounds_from_the_middle_of_its_model() {
    let mut adapter = adapter();
    adapter.observe_snapshot(&snapshot(1_000, vec![door(0, 0)]));
    assert!(adapter.decisions().is_empty());

    // `Use_BinaryMover_Go`'s `G_PlayDoorSound(ent, BMS_START)`, then
    // `Reached_BinaryMover`'s `BMS_END` (`g_mover.c:88-97,679,761`).
    for (time, bits, stage) in [
        (1_050, EV_EVENT_BIT1, BMS_START),
        (2_050, EV_EVENT_BIT2, BMS_END),
    ] {
        adapter.observe_snapshot(&snapshot(time, vec![door(bits | EV_PLAYDOORSOUND, stage)]));
        let [decision] = adapter.decisions() else {
            panic!("one door sound: {:?}", adapter.decisions());
        };
        assert_eq!(decision.event, LegacySoundEvent::PlayDoorSound);
        assert!(decision.handle.is_some());
        assert_eq!(decision.request.source, SourceId(u32::from(DOOR)));
        assert_eq!(decision.request.origin, Some(DOOR_MIDPOINT));
    }
}

#[test]
fn a_bmodel_sound_and_a_door_without_a_soundset() {
    let mut adapter = adapter();
    adapter.observe_snapshot(&snapshot(1_000, vec![door(0, 0)]));
    adapter.observe_snapshot(&snapshot(
        1_050,
        vec![door(EV_EVENT_BIT1 | EV_BMODEL_SOUND, BMS_END)],
    ));
    let [decision] = adapter.decisions() else {
        panic!("one bmodel sound: {:?}", adapter.decisions());
    };
    assert_eq!(decision.event, LegacySoundEvent::Bmodel);
    // `CG_PlayDoorSound` plays nothing for a mover with no soundset.
    let mut silent = door(EV_EVENT_BIT2 | EV_PLAYDOORSOUND, BMS_START);
    silent.set_raw_field(SOUND_SET_INDEX, 0);
    adapter.observe_snapshot(&snapshot(1_100, vec![silent]));
    assert!(adapter.decisions().is_empty());
}

#[test]
fn only_brush_models_add_their_midpoint() {
    let adapter = adapter();
    let mut moved = door(0, 0);
    for (field, value) in TR_BASE.into_iter().zip([8.0_f32, 16.0, 32.0]) {
        moved.set_raw_field(field, value.to_bits());
    }
    assert_eq!(adapter.sound_origin(&moved), [1_544.0, -752.0, 128.0]);

    let mut general = moved.clone();
    general.set_raw_field(E_TYPE, ET_GENERAL);
    general.set_raw_field(SOLID, 0);
    assert_eq!(adapter.sound_origin(&general), [8.0, 16.0, 32.0]);

    // A model the map does not have keeps the origin rather than panicking.
    let mut unknown = moved;
    unknown.set_raw_field(MODEL_INDEX, 9);
    assert_eq!(adapter.sound_origin(&unknown), [8.0, 16.0, 32.0]);
}

/// A `.wav` asked for that only exists as `.mp3` (retail's `enemy_saber_on`) is
/// registered once however often it is asked for.
#[test]
fn a_wav_that_resolves_to_an_mp3_is_registered_once() {
    let mut vfs = VirtualFileSystem::new();
    vfs.mount_memory(
        "test",
        [("sound/weapons/saber/saberon.mp3", b"on".to_vec())],
    )
    .unwrap();
    let mut sounds = Vec::new();
    let mut registered = 0;
    let mut register = |_: &str, _: &[u8]| {
        registered += 1;
        Some(sjk_audio::SoundHandle(registered))
    };
    let first = super::intern_sound(
        &mut sounds,
        &vfs,
        "sound/weapons/saber/saberon.wav",
        &mut register,
    );
    for _ in 0..5 {
        let again = super::intern_sound(
            &mut sounds,
            &vfs,
            "sound/weapons/saber/SABERON.wav",
            &mut register,
        );
        assert_eq!(again, first);
    }
    assert_eq!(sounds.len(), 1);
    assert_eq!(&*sounds[0].path, "sound/weapons/saber/saberon.mp3");
    assert_eq!(registered, 1);
}
