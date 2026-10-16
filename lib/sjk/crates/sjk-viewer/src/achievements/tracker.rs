//! What the client sees the player do in a match, for the achievements it counts
//! (`achievements.rs`): players defeated (and how, and in a duel), the player's own
//! deaths for streaks, flags captured, the maps and servers played and the time
//! played. Only in a live match on a server: never in a demo, a game this client
//! hosts, or while following someone else.
//!
//! Kills come from the obituaries the client already decodes (`EV_OBITUARY`, its
//! attacker, victim and means of death); captures from the player's own
//! `persistant[PERS_CAPTURES]` rising; duels from the player state's private duel
//! (`duelInProgress`, `duelIndex`) or a duel game type.

use super::{Counter, Record};
use sjk_client::{LegacyClientInfo, ObituaryEvent};
use sjk_protocol::{GameState, Snapshot};
use std::net::SocketAddr;
use std::time::Instant;

/// `persistant[PERS_CAPTURES]`.
const PERS_CAPTURES: usize = 14;
/// `PMF_FOLLOW`: the view is someone else's.
const PMF_FOLLOW: u16 = 0x1000;
/// `PM_INTERMISSION`.
const PM_INTERMISSION: u8 = 7;
/// `CS_PLAYERS`.
const CS_PLAYERS: usize = 1131;
/// `GT_DUEL` and `GT_POWERDUEL`; from `GT_TEAM` on, players have teams.
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_TEAM: i32 = 6;
/// Longest gap between two snapshots counted as play (a stall is not play).
const PLAY_STEP_MAX_MS: u64 = 1_000;
/// Most captures one snapshot may add (a jump past it is a reset, not play).
const CAPTURE_STEP_MAX: u32 = 3;

/// `meansOfDeath_t` values (`sjk_game_jka::means_of_death`).
mod means {
    pub(super) const SABER: u8 = 3;
    pub(super) const FORCE_DARK: u8 = 31;
    pub(super) const WATER: u8 = 33;
    pub(super) const SLIME: u8 = 34;
    pub(super) const LAVA: u8 = 35;
    pub(super) const CRUSH: u8 = 36;
    pub(super) const FALLING: u8 = 38;
    pub(super) const TRIGGER_HURT: u8 = 41;
}

/// The kind of weapon a means of death belongs to, for Arsenal: one bit each.
pub(crate) fn weapon_kind(means: u8) -> Option<u32> {
    Some(match means {
        1 => 0,        // stun baton
        2 => 1,        // melee
        3 => 2,        // saber
        4 | 5 => 3,    // bryar pistol
        6 | 7 => 4,    // blaster (and the turret's)
        8..=10 => 5,   // disruptor
        11 => 6,       // bowcaster
        12..=14 => 7,  // repeater
        15 | 16 => 8,  // DEMP 2
        17 | 18 => 9,  // flechette
        19..=22 => 10, // rocket launcher
        23 | 24 => 11, // thermal detonator
        25 | 26 => 12, // trip mine
        27 => 13,      // detonation pack
        29 | 30 => 14, // concussion rifle
        _ => return None,
    })
}

/// What a kill by the player counts towards.
fn count_kill(record: &mut Record, means: u8, duel: bool, streak: u64) {
    record.add(Counter::Kills, 1);
    record.raise(Counter::BestStreak, streak);
    match means {
        means::SABER => record.add(Counter::SaberKills, 1),
        means::FORCE_DARK => record.add(Counter::DarkSideKills, 1),
        means::WATER
        | means::SLIME
        | means::LAVA
        | means::CRUSH
        | means::FALLING
        | means::TRIGGER_HURT => record.add(Counter::LedgeKills, 1),
        _ => {}
    }
    if let Some(bit) = weapon_kind(means) {
        record.weapons |= 1 << bit;
    }
    if duel {
        record.add(Counter::DuelWins, 1);
    }
}

/// Where the player is, to tell one match from the next.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Place {
    server: SocketAddr,
    map: String,
    client: u16,
}

/// The tracker's state between snapshots.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    place: Option<Place>,
    /// Players defeated since the player last fell.
    streak: u64,
    /// The opponent of the private duel the last snapshot showed.
    duel_with: Option<u16>,
    /// The captures and team the last snapshot showed, to count rises.
    captures: Option<(u32, u8)>,
    /// When the last snapshot that counted as play was seen.
    last_play: Option<Instant>,
    /// The player has been in the game (not spectating) on this map.
    joined: bool,
}

/// One live snapshot as the tracker needs it.
pub(crate) struct Seen<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) game_state: &'a GameState,
    pub(crate) server: SocketAddr,
    /// A game this client hosts: nothing counts.
    pub(crate) local: bool,
    /// The obituaries this snapshot brought, oldest first.
    pub(crate) obituaries: &'a [ObituaryEvent],
    pub(crate) now: Instant,
}

impl Tracker {
    /// The session ended: the next one starts afresh.
    pub(crate) fn leave(&mut self) {
        *self = Self::default();
    }

    /// Count what `seen` shows; whether the record changed.
    pub(crate) fn observe(&mut self, seen: &Seen<'_>, record: &mut Record) -> bool {
        if seen.local {
            self.leave();
            return false;
        }
        let game = seen.game_state;
        let Ok(me) = u16::try_from(game.client_num) else {
            return false;
        };
        let info = game.config_string(0).map(LegacyClientInfo::new);
        let map = info
            .and_then(|info| info.bytes("mapname"))
            .map(|bytes| String::from_utf8_lossy(bytes).to_ascii_lowercase())
            .unwrap_or_default();
        let gametype = info
            .and_then(|info| info.integer("g_gametype"))
            .unwrap_or(0);
        let place = Place {
            server: seen.server,
            map,
            client: me,
        };
        if self.place.as_ref() != Some(&place) {
            // A new map, server or slot: streaks, baselines and the join start again.
            *self = Self {
                place: Some(place.clone()),
                ..Self::default()
            };
        }
        let player = &seen.snapshot.player;
        let own_view = player.client_num() == me && player.movement_flags() & PMF_FOLLOW == 0;
        let playing = own_view && !player.is_spectator();
        let mut changed = false;

        for event in seen.obituaries {
            if event.target == me {
                self.streak = 0;
                continue;
            }
            if event.attacker != me || usize::from(event.target) >= 64 {
                continue;
            }
            if gametype >= GT_TEAM && same_team(game, me, event.target) {
                continue;
            }
            let duel = matches!(gametype, GT_DUEL | GT_POWERDUEL)
                || self.duel_with == Some(event.target)
                || (player.duel_in_progress() && player.duel_index() == event.target);
            self.streak += 1;
            count_kill(record, event.means_of_death, duel, self.streak);
            changed = true;
        }
        self.duel_with = (own_view && player.duel_in_progress()).then(|| player.duel_index());

        if own_view {
            let captures = player.persistent[PERS_CAPTURES] & 0xffff;
            let team = player.team();
            if let Some((before, before_team)) = self.captures
                && before_team == team
                && captures > before
                && captures - before <= CAPTURE_STEP_MAX
            {
                record.add(Counter::Captures, u64::from(captures - before));
                changed = true;
            }
            self.captures = Some((captures, team));
        }

        let in_match = playing && player.movement_type() != PM_INTERMISSION;
        if in_match && !self.joined {
            self.joined = true;
            changed |= Record::remember(&mut record.maps, &place.map);
            changed |= Record::remember(&mut record.servers, &place.server.to_string());
        }
        if in_match {
            if let Some(last) = self.last_play {
                let ms = seen.now.saturating_duration_since(last).as_millis() as u64;
                if ms > 0 {
                    record.play(ms.min(PLAY_STEP_MAX_MS));
                    changed = true;
                }
            }
            self.last_play = Some(seen.now);
        } else {
            self.last_play = None;
        }
        changed
    }
}

/// Whether players `a` and `b` are on the same team, as the server publishes it.
fn same_team(game: &GameState, a: u16, b: u16) -> bool {
    let team = |client: u16| {
        game.config_string(CS_PLAYERS + usize::from(client))
            .and_then(|bytes| LegacyClientInfo::new(bytes).integer("t"))
    };
    matches!((team(a), team(b)), (Some(x), Some(y)) if x == y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_protocol::PlayerState;
    use std::time::Duration;

    fn snapshot(player: PlayerState) -> Snapshot {
        Snapshot {
            message_sequence: 0,
            reliable_acknowledge: 0,
            server_commands: Vec::new(),
            server_time: 0,
            delta_from: None,
            flags: 0,
            area_mask: Vec::new(),
            player,
            vehicle_player: None,
            entities: Vec::new(),
            consumed_bits: 0,
        }
    }

    fn set_duel(player: &mut PlayerState, on: bool, with: u16) {
        player.set_raw_field(119, u32::from(on));
        player.set_raw_field(44, u32::from(with));
    }

    fn game(map: &str, gametype: i32, me: i32, teams: &[(usize, i32)]) -> GameState {
        let mut game = GameState::empty_local(me);
        game.replace_config_string(
            0,
            format!("\\mapname\\{map}\\g_gametype\\{gametype}").into_bytes(),
        )
        .unwrap();
        for (client, team) in teams {
            game.replace_config_string(
                CS_PLAYERS + client,
                format!("n\\P{client}\\t\\{team}").into_bytes(),
            )
            .unwrap();
        }
        game
    }

    fn kill(attacker: u16, target: u16, means: u8) -> ObituaryEvent {
        ObituaryEvent {
            target,
            attacker,
            means_of_death: means,
            message: "",
            attacker_message: None,
            local_fragged: false,
            local_was_killed: false,
            server_time: 0,
            attacker_force: 0,
        }
    }

    struct Bench {
        tracker: Tracker,
        record: Record,
        game: GameState,
        snapshot: Snapshot,
        now: Instant,
        server: SocketAddr,
        local: bool,
    }

    impl Bench {
        fn new(game: GameState) -> Self {
            let mut player = PlayerState::default();
            player.set_client_num(game.client_num as u16);
            player.persistent[3] = 1; // red: in the game
            let snapshot = snapshot(player);
            Self {
                tracker: Tracker::default(),
                record: Record::default(),
                game,
                snapshot,
                now: Instant::now(),
                server: "1.2.3.4:29070".parse().unwrap(),
                local: false,
            }
        }

        fn step(&mut self, obituaries: &[ObituaryEvent]) -> bool {
            self.now += Duration::from_millis(50);
            let seen = Seen {
                snapshot: &self.snapshot,
                game_state: &self.game,
                server: self.server,
                local: self.local,
                obituaries,
                now: self.now,
            };
            self.tracker.observe(&seen, &mut self.record)
        }
    }

    #[test]
    fn kills_count_by_how_and_streaks_end_with_a_death() {
        let mut bench = Bench::new(game("mp/ffa3", 0, 2, &[]));
        bench.step(&[kill(2, 5, means::SABER), kill(2, 6, means::FORCE_DARK)]);
        bench.step(&[kill(2, 7, means::FALLING), kill(9, 8, means::SABER)]);
        assert_eq!(bench.record.count(Counter::Kills), 3);
        assert_eq!(bench.record.count(Counter::SaberKills), 1);
        assert_eq!(bench.record.count(Counter::DarkSideKills), 1);
        assert_eq!(bench.record.count(Counter::LedgeKills), 1);
        assert_eq!(bench.record.count(Counter::BestStreak), 3);
        // Falling ends the streak; the best stays.
        bench.step(&[kill(5, 2, means::SABER), kill(2, 5, 12)]);
        assert_eq!(bench.record.count(Counter::BestStreak), 3);
        assert_eq!(bench.tracker.streak, 1);
        // A suicide is not a kill and ends the streak too.
        bench.step(&[kill(2, 2, means::FALLING)]);
        assert_eq!(bench.record.count(Counter::Kills), 4);
        assert_eq!(bench.tracker.streak, 0);
        assert_eq!(
            bench.record.count(Counter::Weapons),
            2,
            "saber and repeater; lightning and falls are no weapon"
        );
        assert_eq!(bench.record.count(Counter::DuelWins), 0);
    }

    #[test]
    fn teammates_do_not_count_in_team_games() {
        let mut bench = Bench::new(game("mp/ctf1", 8, 2, &[(2, 1), (5, 1), (6, 2)]));
        bench.step(&[kill(2, 5, means::SABER), kill(2, 6, means::SABER)]);
        assert_eq!(bench.record.count(Counter::Kills), 1);
    }

    #[test]
    fn duels_are_won_in_a_private_duel_or_a_duel_game() {
        let mut bench = Bench::new(game("mp/ffa3", 0, 2, &[]));
        set_duel(&mut bench.snapshot.player, true, 5);
        bench.step(&[]);
        // The duel ends in the same snapshot as the kill.
        set_duel(&mut bench.snapshot.player, false, 0);
        bench.step(&[kill(2, 5, means::SABER)]);
        bench.step(&[kill(2, 6, means::SABER)]);
        assert_eq!(bench.record.count(Counter::DuelWins), 1);
        let mut duel = Bench::new(game("mp/duel6", GT_DUEL, 2, &[]));
        duel.step(&[kill(2, 4, means::SABER)]);
        assert_eq!(duel.record.count(Counter::DuelWins), 1);
    }

    #[test]
    fn captures_count_their_rise_only() {
        let mut bench = Bench::new(game("mp/ctf1", 8, 2, &[]));
        bench.snapshot.player.persistent[PERS_CAPTURES] = 4;
        bench.step(&[]);
        assert_eq!(bench.record.count(Counter::Captures), 0, "the baseline");
        bench.snapshot.player.persistent[PERS_CAPTURES] = 5;
        bench.step(&[]);
        assert_eq!(bench.record.count(Counter::Captures), 1);
        // A team change re-baselines; a reset to 0 and a jump count nothing.
        bench.snapshot.player.persistent[3] = 2;
        bench.snapshot.player.persistent[PERS_CAPTURES] = 6;
        bench.step(&[]);
        bench.snapshot.player.persistent[PERS_CAPTURES] = 0;
        bench.step(&[]);
        bench.snapshot.player.persistent[PERS_CAPTURES] = 50;
        bench.step(&[]);
        assert_eq!(bench.record.count(Counter::Captures), 1);
    }

    #[test]
    fn maps_servers_and_time_count_while_playing() {
        let mut bench = Bench::new(game("mp/FFA3", 0, 2, &[]));
        for _ in 0..21 {
            bench.step(&[]);
        }
        assert_eq!(bench.record.count(Counter::Maps), 1);
        assert_eq!(bench.record.count(Counter::Servers), 1);
        assert!(bench.record.maps.contains("mp/ffa3"));
        assert_eq!(bench.record.play_ms, 1_000);
        // Spectating and a stall add no time; following someone counts nothing.
        bench.snapshot.player.persistent[3] = 3;
        bench.step(&[]);
        bench.snapshot.player.persistent[3] = 1;
        bench.now += Duration::from_secs(30);
        bench.step(&[]);
        bench.step(&[]);
        assert_eq!(bench.record.play_ms, 1_050);
        bench.game = game("mp/ffa1", 0, 2, &[]);
        bench.snapshot.player.set_client_num(7);
        bench.step(&[]);
        assert_eq!(bench.record.count(Counter::Maps), 1, "watching someone");
        bench.snapshot.player.set_client_num(2);
        bench.step(&[]);
        assert_eq!(bench.record.count(Counter::Maps), 2);
    }

    #[test]
    fn nothing_counts_in_a_local_game() {
        let mut bench = Bench::new(game("mp/ffa3", 0, 2, &[]));
        bench.local = true;
        assert!(!bench.step(&[kill(2, 5, means::SABER)]));
        assert_eq!(bench.record, Record::default());
    }

    #[test]
    fn every_weapon_has_its_own_kind() {
        let kinds: std::collections::BTreeSet<u32> = (0..=42).filter_map(weapon_kind).collect();
        assert_eq!(kinds.len(), 15);
        assert_eq!(weapon_kind(means::FALLING), None);
        assert_eq!(weapon_kind(means::FORCE_DARK), None);
    }
}
