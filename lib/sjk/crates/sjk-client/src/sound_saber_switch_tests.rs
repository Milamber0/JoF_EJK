//! Pins cgame's saber switch sounds: which transition, which hilt's sound, which
//! source and channel, and that nothing doubles a server sound.

use super::*;
use sjk_protocol::LEGACY_ENTITY_FIELDS;

const LOCAL: u16 = 0;
const DUAL: u16 = 1;
const STAFF: u16 = 2;
const HIDDEN: u16 = 3;
const PISTOL: u8 = 4;
const EV_CHANGE_WEAPON: u32 = 26;
const EV_SABER_UNHOLSTER: u32 = 33;
/// `CS_SOUNDS` slot of a hilt's own `soundOn`, as the server's toggle plays it.
const SLOT_SINGLE2_ON: u8 = 1;

/// A blade skin's sounds (an SJK unlockable), worn in place of the hilts'.
const SKIN: SaberSoundSet<'static> = SaberSoundSet {
    on: "sound/sjk/skin_on.wav",
    off: "sound/sjk/skin_off.wav",
    hum: "sound/sjk/skin_hum.wav",
    swings: [
        "sound/sjk/skin_swing1.wav",
        "sound/sjk/skin_swing2.wav",
        "sound/sjk/skin_swing3.wav",
    ],
};

const SABERS: &str = r#"
Kyle
{
    saberType SABER_SINGLE
    soundOn "sound/weapons/saber/saberon.wav"
    soundOff "sound/weapons/saber/saberoff.wav"
}
single_2
{
    saberType SABER_SINGLE
    soundOn "sound/weapons/saber/single2_on.wav"
    soundOff "sound/weapons/saber/single2_off.wav"
}
dual_2
{
    saberType SABER_SINGLE
    soundOn "sound/weapons/saber/dual2_on.wav"
    soundOff "sound/weapons/saber/dual2_off.wav"
}
staff_1
{
    saberType SABER_STAFF
    twoHanded 1
    soundOn "sound/weapons/saber/staff_on.wav"
    soundOff "sound/weapons/saber/staff_off.wav"
}
hidden
{
    notInMP 1
    soundOn "sound/weapons/saber/hidden_on.wav"
}
plain
{
    saberType SABER_SINGLE
}
"#;

fn player_config(sabers: &str) -> String {
    format!("\\n\\Player\\t\\0\\model\\kyle/default\\{sabers}")
}

struct Fixture {
    game_state: GameState,
    vfs: VirtualFileSystem,
    adapter: LegacySoundAdapter,
    time: i32,
    next_handle: u32,
}

impl Fixture {
    fn new() -> Self {
        let mut vfs = VirtualFileSystem::new();
        let mut files = vec![(
            "ext_data/sabers/test.sab".to_owned(),
            SABERS.as_bytes().to_vec(),
        )];
        for name in [
            "saberon",
            "saberoff",
            "single2_on",
            "single2_off",
            "dual2_on",
            "dual2_off",
            "staff_on",
            "staff_off",
            "hidden_on",
            "enemy_saber_on",
            "enemy_saber_off",
        ] {
            files.push((format!("sound/weapons/saber/{name}.wav"), b"RIFF".to_vec()));
        }
        for path in [SKIN.on, SKIN.off, SKIN.hum].into_iter().chain(SKIN.swings) {
            files.push((path.to_owned(), b"RIFF".to_vec()));
        }
        vfs.mount_memory("base", files).unwrap();
        let mut game_state = GameState::empty_local(i32::from(LOCAL));
        game_state
            .replace_config_string(
                CS_SOUNDS + usize::from(SLOT_SINGLE2_ON),
                b"sound/weapons/saber/single2_on.wav".to_vec(),
            )
            .unwrap();
        for (client, sabers) in [
            (LOCAL, "st\\single_2\\st2\\none"),
            (DUAL, "st\\single_2\\st2\\dual_2"),
            (STAFF, "st\\staff_1\\st2\\dual_2"),
            (HIDDEN, "st\\hidden\\st2\\none"),
        ] {
            game_state
                .replace_config_string(
                    CS_PLAYERS + usize::from(client),
                    player_config(sabers).into_bytes(),
                )
                .unwrap();
        }
        let mut next_handle = 0;
        let mut register = |_: &str, _: &[u8]| {
            next_handle += 1;
            Some(SoundHandle(next_handle))
        };
        let mut adapter = LegacySoundAdapter::new(&game_state, &vfs, &mut register);
        adapter.register_saber_sound_sets(&[SKIN], &vfs, &mut register);
        Self {
            game_state,
            vfs,
            adapter,
            time: 1_000,
            next_handle,
        }
    }

    /// The clients wearing the blade skin, as the viewer gives them every frame.
    fn wear(&mut self, clients: &[u16]) {
        let mut worn = [None; SABER_SOUND_CLIENTS];
        for client in clients {
            worn[usize::from(*client)] = Some(0);
        }
        self.adapter.set_saber_sound_overrides(&worn);
    }

    /// A `CS_PLAYERS` change, delivered as the viewer does before a snapshot.
    fn set_player(&mut self, client: u16, config: &str) {
        self.game_state
            .replace_config_string(CS_PLAYERS + usize::from(client), config.as_bytes().to_vec())
            .unwrap();
        let next_handle = &mut self.next_handle;
        self.adapter
            .refresh_clients(&self.game_state, &self.vfs, |_, _| {
                *next_handle += 1;
                Some(SoundHandle(*next_handle))
            });
    }

    fn snapshot(&mut self, view: LegacySaberView, entities: &[EntityState]) -> Snapshot {
        self.time += 50;
        let mut player = PlayerState::zero();
        player.set_client_num(view.client_num);
        player.stats[0] = 100;
        Snapshot {
            message_sequence: 0,
            reliable_acknowledge: 0,
            server_commands: Vec::new(),
            server_time: self.time,
            delta_from: None,
            flags: 0,
            area_mask: Vec::new(),
            player,
            vehicle_player: None,
            entities: entities.to_vec(),
            consumed_bits: 0,
        }
    }

    /// One new snapshot presented for three rendered frames.
    fn step(&mut self, view: LegacySaberView, entities: &[EntityState]) -> Vec<Played> {
        let snapshot = self.snapshot(view, entities);
        let mut played = Vec::new();
        for _ in 0..3 {
            self.adapter.observe_saber_switches(&snapshot, view);
            played.extend(self.played());
        }
        played
    }

    fn played(&self) -> Vec<Played> {
        self.adapter
            .decisions()
            .iter()
            .map(|decision| Played {
                event: decision.event,
                sound: decision
                    .sound
                    .and_then(|index| self.adapter.sound(index))
                    .map(|sound| {
                        sound
                            .path
                            .trim_start_matches("sound/weapons/saber/")
                            .trim_end_matches(".wav")
                            .to_owned()
                    })
                    .unwrap_or_default(),
                source: decision.request.source.0,
                channel: decision.request.channel.0,
                origin: decision.request.origin,
                additional: decision.additional,
                audible: decision.handle.is_some(),
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Played {
    event: LegacySoundEvent,
    sound: String,
    source: u32,
    channel: u32,
    origin: Option<[f32; 3]>,
    additional: bool,
    audible: bool,
}

fn names(played: &[Played]) -> Vec<(LegacySoundEvent, &str)> {
    played
        .iter()
        .map(|played| (played.event, played.sound.as_str()))
        .collect()
}

fn view(weapon: u8, holstered: u8) -> LegacySaberView {
    LegacySaberView {
        client_num: LOCAL,
        weapon,
        saber_holstered: holstered,
        saber_in_flight: false,
        saber_entity_num: 0,
        entity_flags: 0,
        movement_flags: 0,
        team: 0,
        health: 100,
        origin: [0.0; 3],
    }
}

fn thrown(mut view: LegacySaberView, saber_entity: u16) -> LegacySaberView {
    view.saber_in_flight = true;
    view.saber_entity_num = saber_entity;
    view
}

fn player(number: u16, weapon: u8, holstered: u8) -> EntityState {
    let mut state = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(8, u32::from(ET_PLAYER));
    state.set_raw_field(14, u32::from(weapon));
    state.set_raw_field(71, u32::from(holstered));
    state.set_raw_field(32, u32::from(number));
    // trBase, as `EntityState::trajectory_base` reads it.
    state.set_raw_field(2, 100.0_f32.to_bits());
    state.set_raw_field(1, 200.0_f32.to_bits());
    state.set_raw_field(4, 24.0_f32.to_bits());
    state
}

fn with_field(mut state: EntityState, field: usize, value: u32) -> EntityState {
    state.set_raw_field(field, value);
    state
}

/// `G_Sound`'s temporary entity: `EV_GENERAL_SOUND` with no owner, at `origin`.
fn general_sound(number: u16, slot: u8, origin: [f32; 3]) -> EntityState {
    let mut state = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(8, 18 + 76); // ET_EVENTS + EV_GENERAL_SOUND
    state.set_raw_field(42, u32::from(slot));
    for (field, value) in [2, 1, 4].into_iter().zip(origin) {
        state.set_raw_field(field, value.to_bits());
    }
    state
}

use LegacySoundEvent::{SaberSwitchOff as Off, SaberSwitchOn as On, SaberUnholster};

#[test]
fn hilts_resolve_like_cg_new_client_info() {
    let fixture = Fixture::new();
    let hands = |client| {
        let hands = fixture.adapter.saber_switch.hands(client);
        let path = |sound: Option<u16>| {
            sound.map(|index| fixture.adapter.sound(index).unwrap().path.to_string())
        };
        (
            [Switch::On, Switch::Off].map(|switch| hands.sounds(switch).map(path)),
            hands.second_model,
        )
    };
    let saber = |name: &str| Some(format!("sound/weapons/saber/{name}.wav"));
    // A removed second saber keeps the default `soundOn` and so is heard on
    // ignition, but has no model, so retraction skips it.
    assert_eq!(
        hands(LOCAL),
        (
            [
                [saber("single2_on"), saber("enemy_saber_on")],
                [saber("single2_off"), None]
            ],
            false
        )
    );
    assert_eq!(
        hands(DUAL),
        (
            [
                [saber("single2_on"), saber("dual2_on")],
                [saber("single2_off"), saber("dual2_off")]
            ],
            true
        )
    );
    // A two-handed staff drops the second saber.
    assert_eq!(
        hands(STAFF),
        (
            [
                [saber("staff_on"), saber("enemy_saber_on")],
                [saber("staff_off"), None]
            ],
            false
        )
    );
    // `notInMP` loads Kyle in its place; an empty slot has nothing.
    assert_eq!(
        hands(HIDDEN).0[0],
        [saber("saberon"), saber("enemy_saber_on")]
    );
    assert_eq!(hands(9).0, [[None, None], [None, None]]);
}

#[test]
fn unauthored_hilt_sounds_fall_back_to_the_defaults() {
    let mut vfs = VirtualFileSystem::new();
    vfs.mount_memory("base", [("ext_data/sabers/test.sab", SABERS.as_bytes())])
        .unwrap();
    let definitions = crate::legacy_saber_definitions(&vfs).unwrap();
    // A definition keeps only what its `.sab` authors.
    let plain = &definitions["plain"];
    assert_eq!(
        (plain.sound_on.as_deref(), plain.sound_off.as_deref()),
        (None, None)
    );
    assert!(!plain.two_handed);
    assert!(definitions["staff_1"].two_handed);
    assert_eq!(
        definitions["dual_2"].sound_off.as_deref(),
        Some("sound/weapons/saber/dual2_off.wav")
    );
    let mut paths: Vec<String> = Vec::new();
    let mut resolve = |config: &str| {
        let mut intern = |path: &str| {
            paths.push(path.to_owned());
            (paths.len() - 1) as u16
        };
        let hands = super::hands(config.as_bytes(), &definitions, &mut intern);
        let path = |sound: Option<u16>| sound.map(|index| paths[usize::from(index)].clone());
        [Switch::On, Switch::Off].map(|switch| hands.sounds(switch).map(path))
    };
    let saber = |name: &str| Some(format!("sound/weapons/saber/{name}.wav"));
    // A hilt that authors none plays `WP_SaberSetDefaults`' sounds.
    assert_eq!(
        resolve(&player_config("st\\plain\\st2\\none")),
        [
            [saber("enemy_saber_on"), saber("enemy_saber_on")],
            [saber("enemy_saber_off"), None]
        ]
    );
    // A clientinfo naming no hilt keeps the stock ignition.
    assert_eq!(
        resolve("\\n\\Player\\model\\kyle/default"),
        [
            [Some("sound/weapons/saber/saberon.mp3".to_owned()), None],
            [None, None]
        ]
    );
}

#[test]
fn drawing_the_saber_from_melee_ignites_each_hilt_once() {
    let mut fixture = Fixture::new();
    assert!(fixture.step(view(WP_MELEE, 0), &[]).is_empty());
    assert!(fixture.step(view(WP_MELEE, 0), &[]).is_empty());
    let played = fixture.step(view(WP_SABER, 0), &[]);
    assert_eq!(names(&played), [(On, "single2_on"), (On, "enemy_saber_on")]);
    for (index, played) in played.iter().enumerate() {
        // `S_StartSound(lerpOrigin, ps.clientNum, CHAN_AUTO, ...)` on the
        // listener's own entity: full volume, no position.
        assert_eq!(played.source, u32::from(LOCAL));
        assert_eq!(played.channel, CHAN_AUTO);
        assert_eq!(played.origin, None);
        assert_eq!(played.additional, index == 1);
        assert!(played.audible);
    }
    assert!(fixture.step(view(WP_SABER, 0), &[]).is_empty());
    // From a gun too.
    fixture.step(view(PISTOL, 0), &[]);
    assert_eq!(
        names(&fixture.step(view(WP_SABER, 0), &[])),
        [(On, "single2_on"), (On, "enemy_saber_on")]
    );
}

#[test]
fn putting_a_lit_saber_away_retracts_it_and_a_holstered_one_is_silent() {
    let mut fixture = Fixture::new();
    fixture.step(view(WP_SABER, 0), &[]);
    assert_eq!(
        names(&fixture.step(view(PISTOL, 0), &[])),
        [(Off, "single2_off")]
    );
    fixture.step(view(WP_SABER, 0), &[]);
    fixture.step(view(WP_SABER, 2), &[]);
    // `saberHolstered` as the switch lands: still 2 when the weapon changes.
    assert!(fixture.step(view(WP_MELEE, 2), &[]).is_empty());
}

#[test]
fn remote_dual_and_staff_switches_play_their_hilts_from_the_player() {
    let mut fixture = Fixture::new();
    let local = view(WP_SABER, 0);
    fixture.step(
        local,
        &[player(DUAL, WP_MELEE, 0), player(STAFF, PISTOL, 0)],
    );
    let played = fixture.step(
        local,
        &[player(DUAL, WP_SABER, 0), player(STAFF, WP_SABER, 0)],
    );
    assert_eq!(
        names(&played),
        [
            (On, "single2_on"),
            (On, "dual2_on"),
            (On, "staff_on"),
            (On, "enemy_saber_on")
        ]
    );
    assert_eq!(played[0].source, u32::from(DUAL));
    assert_eq!(played[0].channel, CHAN_AUTO);
    assert_eq!(played[0].origin, Some([100.0, 200.0, 24.0]));
    assert_eq!(played[2].source, u32::from(STAFF));
    let played = fixture.step(
        local,
        &[player(DUAL, WP_MELEE, 0), player(STAFF, PISTOL, 0)],
    );
    assert_eq!(
        names(&played),
        [(Off, "single2_off"), (Off, "dual2_off"), (Off, "staff_off")]
    );
}

#[test]
fn a_player_coming_into_view_with_a_saber_is_silent() {
    let mut fixture = Fixture::new();
    let local = view(WP_SABER, 0);
    fixture.step(local, &[]);
    assert!(fixture.step(local, &[player(DUAL, WP_SABER, 0)]).is_empty());
    assert!(fixture.step(local, &[]).is_empty());
    assert!(fixture.step(local, &[player(DUAL, WP_SABER, 0)]).is_empty());
    // Respawning (the teleport bit toggles) with a saber after dying with a gun.
    fixture.step(local, &[player(DUAL, PISTOL, 0)]);
    let dead = with_field(player(DUAL, PISTOL, 0), 19, EF_DEAD);
    fixture.step(local, &[dead]);
    let respawned = with_field(player(DUAL, WP_SABER, 0), 19, EF_TELEPORT_BIT);
    assert!(fixture.step(local, &[respawned]).is_empty());
    // The quirk the other way: cgame still holds the saber it last bolted on, so
    // a player who left view with it lit and returns with a gun retracts it.
    fixture.step(local, &[]);
    assert_eq!(
        names(&fixture.step(local, &[player(DUAL, PISTOL, 0)])),
        [(Off, "single2_off"), (Off, "dual2_off")]
    );
}

#[test]
fn a_caught_saber_does_not_ignite_again() {
    let mut fixture = Fixture::new();
    let remote = |in_flight: bool| {
        let state = player(DUAL, WP_SABER, 0);
        if in_flight {
            with_field(with_field(state, 81, 1), 37, 200)
        } else {
            state
        }
    };
    fixture.step(view(WP_SABER, 0), &[remote(false)]);
    let mut played = Vec::new();
    for in_flight in [true, true, false, false] {
        let local = if in_flight {
            thrown(view(WP_SABER, 0), 100)
        } else {
            view(WP_SABER, 0)
        };
        played.extend(fixture.step(local, &[remote(in_flight)]));
    }
    assert!(played.is_empty(), "{played:?}");
    // Knocked away, then fists: no retraction for a saber no longer in hand, and
    // `saberWasInFlight` survives melee, so drawing it again is silent as well.
    fixture.step(thrown(view(WP_SABER, 0), 0), &[]);
    assert!(fixture.step(thrown(view(WP_MELEE, 0), 0), &[]).is_empty());
    assert!(fixture.step(view(WP_MELEE, 0), &[]).is_empty());
    assert!(fixture.step(view(WP_SABER, 0), &[]).is_empty());
    // Once a saber is bolted on again the latch clears: the next draw ignites.
    assert_eq!(fixture.step(view(PISTOL, 0), &[]).len(), 1);
    assert_eq!(fixture.step(view(WP_SABER, 0), &[]).len(), 2);
}

#[test]
fn server_toggles_and_unholster_events_are_not_doubled() {
    let mut fixture = Fixture::new();
    let local = view(WP_SABER, 0);
    fixture.step(local, &[player(DUAL, WP_SABER, 2)]);
    // The toggle (`sv_saberswitch`) and an attack's unholster keep the weapon:
    // the server's event voices them, cgame's switch path stays silent.
    let unholster = with_field(player(DUAL, WP_SABER, 0), 28, EV_SABER_UNHOLSTER);
    let snapshot = fixture.snapshot(local, std::slice::from_ref(&unholster));
    fixture.adapter.observe_snapshot(&snapshot);
    let played = fixture.played();
    assert_eq!(
        names(&played),
        [(SaberUnholster, "single2_on"), (SaberUnholster, "dual2_on")]
    );
    assert_eq!(played[0].channel, CHAN_AUTO);
    assert_eq!(fixture.adapter.observe_saber_switches(&snapshot, local), 0);
    // Drawing it from melee: `EV_CHANGE_WEAPON` has no saber select sound, so
    // each hilt ignites exactly once, from cgame.
    fixture.step(local, &[player(DUAL, WP_MELEE, 0)]);
    let change = with_field(
        with_field(player(DUAL, WP_SABER, 0), 28, EV_CHANGE_WEAPON),
        42,
        u32::from(WP_SABER),
    );
    let snapshot = fixture.snapshot(local, std::slice::from_ref(&change));
    fixture.adapter.observe_snapshot(&snapshot);
    assert!(fixture.played().is_empty());
    fixture.adapter.observe_saber_switches(&snapshot, local);
    assert_eq!(
        names(&fixture.played()),
        [(On, "single2_on"), (On, "dual2_on")]
    );
    // An NPC's unholster keeps the stock ignition.
    let npc = with_field(
        with_field(player(40, WP_SABER, 0), 8, 13),
        28,
        EV_SABER_UNHOLSTER,
    );
    let snapshot = fixture.snapshot(local, &[npc]);
    fixture.adapter.observe_snapshot(&snapshot);
    assert_eq!(names(&fixture.played()), [(SaberUnholster, "saberon")]);
}

#[test]
fn the_dead_switch_silently_and_a_spectator_joining_ignites() {
    let mut fixture = Fixture::new();
    fixture.step(view(WP_SABER, 0), &[player(DUAL, WP_SABER, 0)]);
    let mut dead = view(PISTOL, 0);
    dead.health = 0;
    let remote_dead = with_field(player(DUAL, PISTOL, 0), 19, EF_DEAD);
    assert!(fixture.step(dead, &[remote_dead]).is_empty());
    // Respawned holding the saber it died with: nothing to switch.
    assert!(fixture.step(view(WP_SABER, 0), &[]).is_empty());
    // A spectator's centity holds no weapon, so joining with a saber ignites it.
    let mut spectator = view(WP_SABER, 0);
    spectator.team = TEAM_SPECTATOR;
    assert!(fixture.step(spectator, &[]).is_empty());
    assert_eq!(
        names(&fixture.step(view(WP_SABER, 0), &[])),
        [(On, "single2_on"), (On, "enemy_saber_on")]
    );
}

#[test]
fn a_new_map_starts_from_cleared_centities() {
    // The adapter is rebuilt per gamestate, as `CG_Init` clears `cg_entities`:
    // the local view ignites a held saber on its first frame, while another
    // player already holding one comes into view silently.
    let mut fixture = Fixture::new();
    assert_eq!(
        names(&fixture.step(view(WP_SABER, 0), &[player(DUAL, WP_SABER, 0)])),
        [(On, "single2_on"), (On, "enemy_saber_on")]
    );
    let mut fixture = Fixture::new();
    assert!(
        fixture
            .step(view(WP_MELEE, 0), &[player(DUAL, WP_SABER, 0)])
            .is_empty()
    );
}

#[test]
fn a_reused_slot_and_a_new_hilt_refresh_the_bolted_weapon() {
    let mut fixture = Fixture::new();
    let local = view(WP_SABER, 0);
    fixture.step(local, &[player(DUAL, WP_SABER, 0)]);
    // Client 1 leaves and another player takes the slot, holding a saber.
    fixture.set_player(DUAL, "");
    fixture.step(local, &[]);
    fixture.set_player(DUAL, &player_config("st\\staff_1\\st2\\none"));
    assert!(fixture.step(local, &[player(DUAL, WP_SABER, 0)]).is_empty());
    assert_eq!(
        names(&fixture.step(local, &[player(DUAL, WP_MELEE, 0)])),
        [(Off, "staff_off")]
    );
    // The local player picks another hilt with the saber out: cgame bolts the
    // new one on from no weapon, so it ignites; a renamed player does not.
    fixture.set_player(LOCAL, &player_config("st\\dual_2\\st2\\none"));
    assert_eq!(
        names(&fixture.step(local, &[])),
        [(On, "dual2_on"), (On, "enemy_saber_on")]
    );
    fixture.set_player(
        LOCAL,
        &player_config("st\\dual_2\\st2\\none").replace("Player", "Sol"),
    );
    assert!(fixture.step(local, &[]).is_empty());
    // Another player's new hilt is bolted on silently.
    fixture.step(local, &[player(DUAL, WP_SABER, 0)]);
    fixture.set_player(DUAL, &player_config("st\\single_2\\st2\\dual_2"));
    assert!(fixture.step(local, &[player(DUAL, WP_SABER, 0)]).is_empty());
}

#[test]
fn a_followed_player_is_voiced_once_from_its_playerstate() {
    let mut fixture = Fixture::new();
    let follow = |weapon| LegacySaberView {
        client_num: DUAL,
        movement_flags: PMF_FOLLOW,
        ..view(weapon, 0)
    };
    // The followed client's own entity is also in the snapshot; cgame draws it
    // from the playerstate only.
    fixture.step(follow(WP_MELEE), &[player(DUAL, WP_MELEE, 0)]);
    let played = fixture.step(follow(WP_SABER), &[player(DUAL, WP_MELEE, 0)]);
    assert_eq!(names(&played), [(On, "single2_on"), (On, "dual2_on")]);
    assert!(
        played
            .iter()
            .all(|played| played.source == u32::from(DUAL) && played.origin.is_none())
    );
}

#[test]
fn a_blade_skin_plays_once_per_switch_over_the_hilts() {
    let mut fixture = Fixture::new();
    fixture.wear(&[LOCAL, DUAL]);
    let skin = |switch| (switch, "sound/sjk/skin_on");
    fixture.step(
        view(WP_MELEE, 0),
        &[player(DUAL, WP_MELEE, 0), player(STAFF, WP_MELEE, 0)],
    );
    // The wearers ignite with their hilts' sounds and the skin's one sound over them
    // (once, not per hand); the staff has its own hilt only.
    let played = fixture.step(
        view(WP_SABER, 0),
        &[player(DUAL, WP_SABER, 0), player(STAFF, WP_SABER, 0)],
    );
    assert_eq!(
        names(&played),
        [
            (On, "single2_on"),
            (On, "enemy_saber_on"),
            skin(On),
            (On, "single2_on"),
            (On, "dual2_on"),
            skin(On),
            (On, "staff_on"),
            (On, "enemy_saber_on")
        ]
    );
    let sources: Vec<u32> = played.iter().map(|played| played.source).collect();
    assert_eq!(
        sources,
        [LOCAL, LOCAL, LOCAL, DUAL, DUAL, DUAL, STAFF, STAFF].map(u32::from)
    );
    // The skin's sound is the third of each wearer's, added to the hilts' two.
    for index in [2, 5] {
        assert!(played[index].channel == CHAN_AUTO && played[index].additional);
    }
    assert!(!played[0].additional && played[1].additional && !played[3].additional);
    let played = fixture.step(
        view(PISTOL, 0),
        &[player(DUAL, WP_MELEE, 0), player(STAFF, PISTOL, 0)],
    );
    let skin_off = (Off, "sound/sjk/skin_off");
    let heard = names(&played);
    assert_eq!(heard.iter().filter(|name| **name == skin_off).count(), 2);
    assert_eq!(heard.last(), Some(&(Off, "staff_off")));
    // Taken off, the hilts' own sounds alone.
    fixture.wear(&[]);
    assert_eq!(
        names(&fixture.step(view(WP_SABER, 0), &[])),
        [(On, "single2_on"), (On, "enemy_saber_on")]
    );
}

#[test]
fn a_blade_skin_voices_the_unholster_and_toggle_once_and_the_switch_adds_nothing() {
    let mut fixture = Fixture::new();
    fixture.wear(&[DUAL]);
    let local = view(WP_SABER, 0);
    fixture.step(local, &[player(DUAL, WP_SABER, 2)]);
    // An attack's unholster: both hilts' ignitions, the skin's once over them.
    let unholster = with_field(player(DUAL, WP_SABER, 0), 28, EV_SABER_UNHOLSTER);
    let snapshot = fixture.snapshot(local, std::slice::from_ref(&unholster));
    fixture.adapter.observe_snapshot(&snapshot);
    assert_eq!(
        names(&fixture.played()),
        [
            (SaberUnholster, "single2_on"),
            (SaberUnholster, "dual2_on"),
            (SaberUnholster, "sound/sjk/skin_on")
        ]
    );
    assert_eq!(fixture.adapter.observe_saber_switches(&snapshot, local), 0);
    // The server's toggle: the `G_Sound` of a hilt's `soundOn` where the wearer
    // stands plays the stock sound and the skin's over it (`TOGGLE_REACH`), and the
    // held weapon does not change, so cgame's switch path stays silent.
    let lit = player(DUAL, WP_SABER, 0);
    let toggle = general_sound(100, SLOT_SINGLE2_ON, lit.trajectory_base());
    let snapshot = fixture.snapshot(local, &[with_field(lit, 71, 2), toggle]);
    fixture.adapter.observe_snapshot(&snapshot);
    assert_eq!(
        names(&fixture.played()),
        [
            (LegacySoundEvent::General, "single2_on"),
            (LegacySoundEvent::General, "sound/sjk/skin_on")
        ]
    );
    assert_eq!(fixture.adapter.observe_saber_switches(&snapshot, local), 0);
    // A weapon switch has no server sound: only the switch path's one ignition.
    fixture.step(local, &[player(DUAL, WP_MELEE, 0)]);
    let snapshot = fixture.snapshot(local, &[player(DUAL, WP_SABER, 0)]);
    fixture.adapter.observe_snapshot(&snapshot);
    assert!(fixture.played().is_empty());
    fixture.adapter.observe_saber_switches(&snapshot, local);
    assert_eq!(
        names(&fixture.played()),
        [
            (On, "single2_on"),
            (On, "dual2_on"),
            (On, "sound/sjk/skin_on")
        ]
    );
    // A client not wearing it keeps its hilts on the same event.
    fixture.wear(&[]);
    let snapshot = fixture.snapshot(local, std::slice::from_ref(&unholster));
    fixture.adapter.observe_snapshot(&snapshot);
    assert_eq!(
        names(&fixture.played()),
        [(SaberUnholster, "single2_on"), (SaberUnholster, "dual2_on")]
    );
}
