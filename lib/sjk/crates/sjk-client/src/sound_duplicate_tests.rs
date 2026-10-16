//! Snapshot-only pistol events must not play through both the PS and entity paths.
use super::*;
use sjk_protocol::{LEGACY_ENTITY_FIELDS, PlayerState};

fn adapter() -> LegacySoundAdapter {
    let mut vfs = VirtualFileSystem::new();
    vfs.mount_memory(
        "test",
        [("sound/weapons/bryar/fire.wav", b"fixture".to_vec())],
    )
    .unwrap();
    LegacySoundAdapter::new(&GameState::empty_local(0), &vfs, |_, _| {
        Some(SoundHandle(1))
    })
}

fn pistol(number: u16, raw_event: u32) -> EntityState {
    let mut entity = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
    entity.set_raw_field(8, 1); // ET_PLAYER
    entity.set_raw_field(28, raw_event);
    entity.set_raw_field(14, 4); // WP_BRYAR_PISTOL
    entity.set_raw_field(32, u32::from(number)); // clientNum
    entity
}

fn snapshot(sequence: u32, entities: Vec<EntityState>) -> Snapshot {
    let mut player = PlayerState::zero();
    player.set_raw_field(19, sequence);
    player.set_raw_field(27 + ((sequence - 1) & 1) as usize, 27); // EV_FIRE_WEAPON
    player.set_raw_field(47, 4); // WP_BRYAR_PISTOL
    Snapshot {
        message_sequence: sequence as i32,
        reliable_acknowledge: 0,
        server_commands: Vec::new(),
        server_time: sequence as i32 * 50,
        delta_from: None,
        flags: 0,
        area_mask: Vec::new(),
        player,
        vehicle_player: None,
        entities,
        consumed_bits: 0,
    }
}

#[test]
fn unpredicted_local_pistol_fire_plays_once_and_remote_fire_is_independent() {
    for step in [8, 7, 4, 3] {
        let mut adapter = adapter();
        for sequence in 1..=5 {
            let raw_event = 27 | (((sequence - 1) & 3) << 8);
            let mut snapshot = snapshot(sequence, vec![pistol(0, raw_event), pistol(1, raw_event)]);
            snapshot.server_time = sequence as i32 * step;
            adapter.observe_snapshot(&snapshot);
            let fires: Vec<_> = adapter
                .decisions()
                .iter()
                .filter(|d| d.event == LegacySoundEvent::FireWeapon)
                .collect();
            assert_eq!(fires.len(), 2, "one local and one remote shot at {step} ms");
            assert_eq!(
                fires
                    .iter()
                    .filter(|d| d.request.source == SourceId(0))
                    .count(),
                1
            );
            assert_eq!(
                fires
                    .iter()
                    .filter(|d| d.request.source == SourceId(1))
                    .count(),
                1
            );
            assert!(fires.iter().all(|d| d.handle.is_some()));
            adapter.observe_snapshot(&snapshot);
            assert!(
                adapter.decisions().is_empty(),
                "same snapshot must not replay"
            );
        }
    }
}

#[test]
fn an_external_local_pistol_event_is_not_replayed_by_its_entity() {
    let mut adapter = adapter();
    let mut snapshot = snapshot(1, vec![pistol(0, 27 | 0x100)]);
    snapshot.player.set_raw_field(19, 0);
    snapshot.player.set_raw_field(27, 0);
    snapshot.player.set_raw_field(56, 27 | 0x100);
    adapter.observe_snapshot(&snapshot);
    assert_eq!(adapter.decisions().len(), 1);
    assert_eq!(adapter.decisions()[0].event, LegacySoundEvent::FireWeapon);
}

#[test]
fn predicted_pistol_fire_is_not_replayed_on_authoritative_confirmation() {
    let mut adapter = adapter();
    let snapshot = snapshot(1, vec![pistol(0, 27)]);
    adapter.observe_predicted_event(
        crate::predicted_events::PredictedEvent {
            weapon: 4,
            event: 27,
            sequence: 0,
            ..Default::default()
        },
        &snapshot,
    );
    assert_eq!(adapter.decisions().len(), 1);
    adapter.observe_snapshot(&snapshot);
    assert!(adapter.decisions().is_empty());
}
