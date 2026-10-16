//! Achievements: milestones of the player's own play, shown on the Profile page's
//! board (`docs/identity.md`, "Achievements"). Like medals they grant nothing.
//!
//! Most are counted here, from what the client sees in its matches on servers
//! ([`tracker`]): players defeated, duels won, flags captured, maps and servers played,
//! time played. The counts are kept in `achievements.json` in the settings folder, so
//! they work with the identity off, and are sent to the SJK hub with it on, which keeps
//! them with the player's profile (`PROTOCOL.md`, "Achievements"). A few the hub counts
//! itself, from what the player did there (a bio, bug reports, world notes, medals).
//!
//! The hub cannot see a match, so what the client counts is the player's own record:
//! the hub bounds how fast each count may rise, nothing more. Adding an achievement is
//! one entry in [`ALL`] and the same id in the hub's catalogue.

pub(crate) mod medallion;
pub(crate) mod tracker;

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The file the counts are kept in, in the settings folder.
const FILE: &str = "achievements.json";
/// Most maps and servers remembered (their counts' goals are far below).
const SET_MAX: usize = 512;
/// Longest map name or server address kept.
const NAME_MAX: usize = 64;
/// Shortest time between two writes of the file while counts change.
const SAVE_EVERY: Duration = Duration::from_secs(20);

/// What the client counts: each achievement it counts reads one of these.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Counter {
    /// Players defeated.
    Kills,
    /// Players defeated with a saber.
    SaberKills,
    /// The most players defeated in one life.
    BestStreak,
    /// Players defeated by Force lightning or drain.
    DarkSideKills,
    /// Players a fall, a pit or a hazard finished after the player hit or pushed them.
    LedgeKills,
    /// Kinds of weapon the player defeated someone with.
    Weapons,
    /// Duels won.
    DuelWins,
    /// Flags captured.
    Captures,
    /// Different maps played.
    Maps,
    /// Different servers played on.
    Servers,
    /// Minutes played on servers.
    Minutes,
}

impl Counter {
    const COUNT: usize = 11;
    const ALL: [Self; Self::COUNT] = [
        Self::Kills,
        Self::SaberKills,
        Self::BestStreak,
        Self::DarkSideKills,
        Self::LedgeKills,
        Self::Weapons,
        Self::DuelWins,
        Self::Captures,
        Self::Maps,
        Self::Servers,
        Self::Minutes,
    ];

    /// Its name in `achievements.json`.
    const fn key(self) -> &'static str {
        match self {
            Self::Kills => "kills",
            Self::SaberKills => "saber_kills",
            Self::BestStreak => "best_streak",
            Self::DarkSideKills => "dark_side_kills",
            Self::LedgeKills => "ledge_kills",
            Self::Weapons => "weapons",
            Self::DuelWins => "duel_wins",
            Self::Captures => "captures",
            Self::Maps => "maps",
            Self::Servers => "servers",
            Self::Minutes => "minutes",
        }
    }
}

/// Where a board shows an achievement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Category {
    Combat,
    Duels,
    Journeys,
    Community,
}

impl Category {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [Self::Combat, Self::Duels, Self::Journeys, Self::Community];

    /// The name a board reads.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Combat => "Combat",
            Self::Duels => "Duels and flags",
            Self::Journeys => "Journeys",
            Self::Community => "Community",
        }
    }
}

/// Who counts an achievement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    /// The client, from its matches.
    Client(Counter),
    /// The hub, from what the player did there.
    Hub,
}

/// One achievement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Kind {
    /// The hub's id.
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    /// What to do, in a sentence.
    pub(crate) description: &'static str,
    /// The count that unlocks it.
    pub(crate) goal: u64,
    pub(crate) category: Category,
    pub(crate) source: Source,
    /// The edition it belongs to ([`crate::editions`]).
    pub(crate) edition: &'static str,
}

const fn kind(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    goal: u64,
    category: Category,
    source: Source,
) -> Kind {
    Kind {
        id,
        name,
        description,
        goal,
        category,
        source,
        edition: crate::editions::ALPHA,
    }
}

use Category::{Combat, Community, Duels, Journeys};
use Counter as C;
use Source::{Client, Hub};

/// Every achievement, in the hub's catalogue order.
pub(crate) const ALL: [Kind; 21] = [
    kind(
        "first_blood",
        "First Blood",
        "Defeat another player on a server.",
        1,
        Combat,
        Client(C::Kills),
    ),
    kind(
        "kills_100",
        "Centurion",
        "Defeat 100 players.",
        100,
        Combat,
        Client(C::Kills),
    ),
    kind(
        "kills_1000",
        "Legend of the Arena",
        "Defeat 1000 players.",
        1000,
        Combat,
        Client(C::Kills),
    ),
    kind(
        "saber_kills_100",
        "Blademaster",
        "Defeat 100 players with your saber.",
        100,
        Combat,
        Client(C::SaberKills),
    ),
    kind(
        "streak_5",
        "Rampage",
        "Defeat 5 players without falling.",
        5,
        Combat,
        Client(C::BestStreak),
    ),
    kind(
        "streak_10",
        "Unstoppable",
        "Defeat 10 players without falling.",
        10,
        Combat,
        Client(C::BestStreak),
    ),
    kind(
        "dark_side_25",
        "Unlimited Power",
        "Defeat 25 players with Force lightning or drain.",
        25,
        Combat,
        Client(C::DarkSideKills),
    ),
    kind(
        "ledge_10",
        "Watch Your Step",
        "Send 10 players to a fall, a pit or a hazard.",
        10,
        Combat,
        Client(C::LedgeKills),
    ),
    kind(
        "arsenal",
        "Arsenal",
        "Defeat players with 8 different weapons.",
        8,
        Combat,
        Client(C::Weapons),
    ),
    kind(
        "duel_wins_10",
        "Duelist",
        "Win 10 duels.",
        10,
        Duels,
        Client(C::DuelWins),
    ),
    kind(
        "duel_wins_100",
        "Duel Master",
        "Win 100 duels.",
        100,
        Duels,
        Client(C::DuelWins),
    ),
    kind(
        "captures_10",
        "Flag Runner",
        "Capture 10 flags.",
        10,
        Duels,
        Client(C::Captures),
    ),
    kind(
        "maps_10",
        "Traveller",
        "Play on 10 different maps.",
        10,
        Journeys,
        Client(C::Maps),
    ),
    kind(
        "maps_25",
        "Galaxy Tour",
        "Play on 25 different maps.",
        25,
        Journeys,
        Client(C::Maps),
    ),
    kind(
        "servers_5",
        "Server Hopper",
        "Play on 5 different servers.",
        5,
        Journeys,
        Client(C::Servers),
    ),
    kind(
        "hours_10",
        "Regular",
        "Play 10 hours on servers.",
        600,
        Journeys,
        Client(C::Minutes),
    ),
    kind(
        "hours_100",
        "Veteran",
        "Play 100 hours on servers.",
        6000,
        Journeys,
        Client(C::Minutes),
    ),
    kind(
        "storyteller",
        "Storyteller",
        "Write a bio on your profile.",
        1,
        Community,
        Hub,
    ),
    kind(
        "bug_reporter",
        "Bug Reporter",
        "Send a bug report to the SJK team.",
        1,
        Community,
        Hub,
    ),
    kind(
        "surveyor",
        "Surveyor",
        "Pin 5 world notes for the SJK team.",
        5,
        Community,
        Hub,
    ),
    kind(
        "decorated",
        "Decorated",
        "Receive a medal from the SJK team.",
        1,
        Community,
        Hub,
    ),
];

/// The achievement called `id`, if the client knows it.
pub(crate) fn find(id: &str) -> Option<&'static Kind> {
    ALL.iter().find(|kind| kind.id == id)
}

impl Kind {
    /// `count` as the board writes it: hours for time played, a plain number otherwise.
    pub(crate) fn amount(&self, count: u64) -> String {
        if self.source == Client(C::Minutes) {
            let hours = count as f64 / 60.0;
            if hours < 10.0 && !count.is_multiple_of(60) {
                format!("{hours:.1} h")
            } else {
                format!("{} h", count / 60)
            }
        } else {
            count.to_string()
        }
    }
}

/// An achievement as a board shows it: its count (at most the goal) and when it was
/// unlocked (unix seconds), if it was.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Standing {
    pub(crate) kind: &'static Kind,
    pub(crate) progress: u64,
    pub(crate) unlocked: Option<i64>,
}

impl Standing {
    /// How far towards the goal, from 0 to 1.
    pub(crate) fn fraction(&self) -> f32 {
        (self.progress as f32 / self.kind.goal.max(1) as f32).clamp(0.0, 1.0)
    }
}

/// The counts the client keeps, as `achievements.json` holds them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Record {
    counts: [u64; Counter::COUNT],
    /// Milliseconds played beyond the whole minutes in `counts`.
    play_ms: u64,
    /// Maps and servers played, lower case.
    maps: BTreeSet<String>,
    servers: BTreeSet<String>,
    /// Kinds of weapon (bits of [`tracker::weapon_kind`]).
    weapons: u32,
    /// What the hub held for the counts kept as sets (maps, servers, weapons), when
    /// that was more: after a reinstall the hub's count stands until the set passes it.
    floors: [u64; Counter::COUNT],
    /// When each achievement the client counts was unlocked here, unix seconds.
    unlocked: BTreeMap<String, i64>,
}

impl Record {
    /// The count `counter` stands at.
    pub(crate) fn count(&self, counter: Counter) -> u64 {
        let raw = match counter {
            Counter::Maps => self.maps.len() as u64,
            Counter::Servers => self.servers.len() as u64,
            Counter::Weapons => u64::from(self.weapons.count_ones()),
            _ => self.counts[counter as usize],
        };
        raw.max(self.floors[counter as usize])
    }

    fn add(&mut self, counter: Counter, amount: u64) {
        let slot = &mut self.counts[counter as usize];
        *slot = slot.saturating_add(amount);
    }

    fn raise(&mut self, counter: Counter, value: u64) {
        let slot = &mut self.counts[counter as usize];
        *slot = (*slot).max(value);
    }

    /// Add `ms` of play to the minutes played.
    fn play(&mut self, ms: u64) {
        let total = self.play_ms + ms;
        self.add(Counter::Minutes, total / 60_000);
        self.play_ms = total % 60_000;
    }

    /// Remember `name` in `set` (lower case, bounded); whether it was new.
    fn remember(set: &mut BTreeSet<String>, name: &str) -> bool {
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || name.len() > NAME_MAX || set.len() >= SET_MAX {
            return false;
        }
        set.insert(name)
    }

    /// Forget achievement `kind` as cleared: its counter goes just below its goal, so
    /// the next count unlocks it again, and every achievement on that counter that the
    /// lower count no longer reaches is unlocked no more. A hub-counted one only loses
    /// its local mark.
    fn forget(&mut self, kind: &Kind) {
        self.unlocked.remove(kind.id);
        let Client(counter) = kind.source else {
            return;
        };
        let below = kind.goal.saturating_sub(1);
        let index = counter as usize;
        self.floors[index] = self.floors[index].min(below);
        match counter {
            Counter::Maps | Counter::Servers => {
                let set = if counter == Counter::Maps {
                    &mut self.maps
                } else {
                    &mut self.servers
                };
                while set.len() as u64 > below {
                    set.pop_last();
                }
            }
            Counter::Weapons => {
                while u64::from(self.weapons.count_ones()) > below {
                    self.weapons &= self.weapons - 1;
                }
            }
            _ => {
                self.counts[index] = self.counts[index].min(below);
                if counter == Counter::Minutes {
                    self.play_ms = 0;
                }
            }
        }
        for other in &ALL {
            if other.source == kind.source && other.goal > below {
                self.unlocked.remove(other.id);
            }
        }
    }

    /// The counts of the achievements the client counts, by id, as the hub takes them.
    pub(crate) fn hub_counts(&self) -> BTreeMap<String, u64> {
        ALL.iter()
            .filter_map(|kind| match kind.source {
                Client(counter) => Some((kind.id.to_owned(), self.count(counter))),
                Hub => None,
            })
            .filter(|(_, count)| *count > 0)
            .collect()
    }

    /// Take in what the hub holds: a count above the one kept here (another PC, a
    /// reinstall) raises it.
    pub(crate) fn merge_hub(&mut self, held: &[sjk_identity::Achievement]) -> bool {
        let before = self.clone();
        for achievement in held {
            let Some(Kind {
                source: Client(counter),
                ..
            }) = find(&achievement.id)
            else {
                continue;
            };
            match counter {
                Counter::Maps | Counter::Servers | Counter::Weapons => {
                    let floor = &mut self.floors[*counter as usize];
                    *floor = (*floor).max(achievement.progress);
                }
                _ => self.raise(*counter, achievement.progress),
            }
            if achievement.unlocked > 0 {
                self.unlocked
                    .entry(achievement.id.clone())
                    .or_insert(achievement.unlocked);
            }
        }
        *self != before
    }

    /// Mark the achievements the client counts that reached their goal at `now`
    /// (unix seconds); the ones newly unlocked.
    fn unlock(&mut self, now: i64) -> Vec<&'static Kind> {
        let mut new = Vec::new();
        for kind in &ALL {
            let Client(counter) = kind.source else {
                continue;
            };
            if self.count(counter) >= kind.goal && !self.unlocked.contains_key(kind.id) {
                self.unlocked.insert(kind.id.to_owned(), now);
                new.push(kind);
            }
        }
        new
    }

    /// Every achievement as a board shows it, the hub's word taken for those it counts
    /// and for any it says are further along.
    pub(crate) fn standings(&self, held: &[sjk_identity::Achievement]) -> Vec<Standing> {
        ALL.iter()
            .map(|kind| {
                let hub = held.iter().find(|held| held.id == kind.id);
                let local = match kind.source {
                    Client(counter) => self.count(counter),
                    Hub => 0,
                };
                let progress = local.max(hub.map_or(0, |hub| hub.progress)).min(kind.goal);
                let unlocked = self
                    .unlocked
                    .get(kind.id)
                    .copied()
                    .or(hub.map(|hub| hub.unlocked).filter(|at| *at > 0))
                    .or((progress >= kind.goal).then_some(0));
                Standing {
                    kind,
                    progress,
                    unlocked,
                }
            })
            .collect()
    }

    /// The file's JSON.
    fn to_json(&self) -> Value {
        let counts: serde_json::Map<String, Value> = Counter::ALL
            .iter()
            .filter(|counter| {
                !matches!(counter, Counter::Maps | Counter::Servers | Counter::Weapons)
            })
            .map(|counter| {
                (
                    counter.key().to_owned(),
                    json!(self.counts[*counter as usize]),
                )
            })
            .collect();
        let floors: serde_json::Map<String, Value> = Counter::ALL
            .iter()
            .filter(|counter| self.floors[**counter as usize] > 0)
            .map(|counter| {
                (
                    counter.key().to_owned(),
                    json!(self.floors[*counter as usize]),
                )
            })
            .collect();
        json!({
            "version": 1,
            "counts": counts,
            "play_ms": self.play_ms,
            "maps": self.maps,
            "servers": self.servers,
            "weapons": self.weapons,
            "floors": floors,
            "unlocked": self.unlocked,
        })
    }

    /// The record `json` holds; anything missing or malformed counts as nothing.
    fn from_json(json: &Value) -> Self {
        let mut record = Self::default();
        let number = |value: Option<&Value>| value.and_then(Value::as_u64).unwrap_or(0);
        for counter in Counter::ALL {
            record.counts[counter as usize] = number(json["counts"].get(counter.key()));
            record.floors[counter as usize] = number(json["floors"].get(counter.key()));
        }
        record.play_ms = number(json.get("play_ms")).min(59_999);
        for (field, set) in [("maps", &mut record.maps), ("servers", &mut record.servers)] {
            if let Some(names) = json[field].as_array() {
                for name in names.iter().filter_map(Value::as_str) {
                    Self::remember(set, name);
                }
            }
        }
        record.weapons = u32::try_from(number(json.get("weapons"))).unwrap_or(0);
        if let Some(unlocked) = json["unlocked"].as_object() {
            for (id, at) in unlocked {
                if let (Some(kind), Some(at)) = (find(id), at.as_i64()) {
                    record.unlocked.insert(kind.id.to_owned(), at);
                }
            }
        }
        record
    }
}

/// The shared state: the record, where it is kept, and the unlocks waiting to be told.
#[derive(Default)]
struct Shared {
    record: Record,
    file: Option<PathBuf>,
    dirty: bool,
    saved_at: Option<Instant>,
    /// Counts as the identity service was last given them.
    sent: Option<BTreeMap<String, u64>>,
    /// Unlocked in a match, not yet announced.
    announce: Vec<&'static Kind>,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    record: Record {
        counts: [0; Counter::COUNT],
        play_ms: 0,
        maps: BTreeSet::new(),
        servers: BTreeSet::new(),
        weapons: 0,
        floors: [0; Counter::COUNT],
        unlocked: BTreeMap::new(),
    },
    file: None,
    dirty: false,
    saved_at: None,
    sent: None,
    announce: Vec::new(),
});

fn lock() -> MutexGuard<'static, Shared> {
    SHARED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

/// Read `achievements.json` from the settings folder `directory`, once.
pub(crate) fn load(directory: &Path) {
    let mut shared = lock();
    if shared.file.is_some() {
        return;
    }
    let file = directory.join(FILE);
    if let Ok(text) = std::fs::read_to_string(&file) {
        match serde_json::from_str::<Value>(&text) {
            Ok(json) => shared.record = Record::from_json(&json),
            Err(error) => crate::log::progress(format_args!("achievements: {FILE}: {error}")),
        }
    }
    shared.file = Some(file);
}

/// Change the record with `change` (the tracker's counts), then unlock what reached
/// its goal; newly unlocked achievements wait for [`take_announcements`].
pub(crate) fn update(change: impl FnOnce(&mut Record) -> bool) {
    let mut shared = lock();
    if shared.file.is_none() {
        // Not loaded yet: counting now would overwrite the file with less.
        return;
    }
    if !change(&mut shared.record) {
        return;
    }
    shared.dirty = true;
    let new = shared.record.unlock(unix_now());
    shared.announce.extend(new);
}

/// Achievements unlocked since the last call, to announce.
pub(crate) fn take_announcements() -> Vec<&'static Kind> {
    std::mem::take(&mut lock().announce)
}

/// Twice a second: take in what the hub holds, give the identity service the counts
/// when they changed, and write the file when it is due (`force` on the way out).
pub(crate) fn sync(held: Option<&[sjk_identity::Achievement]>, force: bool) {
    let mut shared = lock();
    if shared.file.is_none() {
        return;
    }
    if let Some(held) = held
        && shared.record.merge_hub(held)
    {
        shared.dirty = true;
        // Unlocked elsewhere: nothing to announce here.
        let _ = shared.record.unlock(unix_now());
    }
    let counts = shared.record.hub_counts();
    if shared.sent.as_ref() != Some(&counts)
        && crate::player_identity::set_achievement_counts(&counts)
    {
        shared.sent = Some(counts);
    }
    let due = shared
        .saved_at
        .is_none_or(|saved| saved.elapsed() >= SAVE_EVERY);
    if shared.dirty && (due || force) {
        shared.dirty = false;
        shared.saved_at = Some(Instant::now());
        if let Some(file) = shared.file.clone() {
            save(&file, &shared.record);
        }
    }
}

/// Write `record` to `file` through a temporary file, so a crash cannot leave half.
fn save(file: &Path, record: &Record) {
    let text = match serde_json::to_string_pretty(&record.to_json()) {
        Ok(text) => text,
        Err(_) => return,
    };
    let temporary = file.with_extension("json.tmp");
    let written = std::fs::write(&temporary, text).and_then(|()| std::fs::rename(&temporary, file));
    if let Err(error) = written {
        crate::log::progress(format_args!("achievements: cannot save {FILE}: {error}"));
    }
}

/// Every achievement as the board shows it, with what the hub holds (`held`).
pub(crate) fn standings(held: &[sjk_identity::Achievement]) -> Vec<Standing> {
    lock().record.standings(held)
}

/// The achievements that clearing `id` takes with it on this client: `id`, and for
/// one the client counts, every one on the same counter with a goal at least as high
/// (their count goes below `id`'s goal). Empty for an unknown id.
pub(crate) fn cleared_with(id: &str) -> Vec<&'static str> {
    let Some(kind) = find(id) else {
        return Vec::new();
    };
    match kind.source {
        Hub => vec![kind.id],
        Client(_) => ALL
            .iter()
            .filter(|other| other.source == kind.source && other.goal >= kind.goal)
            .map(|other| other.id)
            .collect(),
    }
}

/// Forget achievements on this client, after the player's own were cleared at the
/// hub: `Some(id)` as [`Record::forget`] does, `None` every count and unlock.
pub(crate) fn forget(id: Option<&str>) {
    let mut shared = lock();
    if shared.file.is_none() {
        return;
    }
    match id.and_then(find) {
        Some(kind) => shared.record.forget(kind),
        None if id.is_none() => shared.record = Record::default(),
        None => return,
    }
    shared.dirty = true;
}

/// The record's counts, for the profile's numbers.
pub(crate) fn count(counter: Counter) -> u64 {
    lock().record.count(counter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(id: &str, progress: u64, unlocked: i64) -> sjk_identity::Achievement {
        sjk_identity::Achievement {
            id: id.to_owned(),
            progress,
            goal: find(id).map_or(1, |kind| kind.goal),
            unlocked,
        }
    }

    #[test]
    fn the_catalogue_has_unique_ids_and_sensible_goals() {
        let mut ids = BTreeSet::new();
        for kind in &ALL {
            assert!(ids.insert(kind.id), "{} twice", kind.id);
            assert!(kind.goal > 0);
            assert!(kind.description.ends_with('.'));
            assert!(
                kind.id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            );
            assert_eq!(find(kind.id), Some(kind));
        }
        assert!(find("nope").is_none());
        for category in Category::ALL {
            assert!(ALL.iter().any(|kind| kind.category == category));
        }
    }

    #[test]
    fn counts_unlock_once_and_go_to_the_hub_by_id() {
        let mut record = Record::default();
        record.add(Counter::Kills, 1);
        let new: Vec<&str> = record.unlock(100).iter().map(|kind| kind.id).collect();
        assert_eq!(new, ["first_blood"]);
        assert!(record.unlock(200).is_empty());
        record.add(Counter::Kills, 99);
        assert_eq!(
            record.unlock(300).iter().map(|k| k.id).collect::<Vec<_>>(),
            ["kills_100"]
        );
        let counts = record.hub_counts();
        assert_eq!(counts["first_blood"], 100);
        assert_eq!(counts["kills_100"], 100);
        assert_eq!(counts["kills_1000"], 100);
        assert!(
            !counts.contains_key("storyteller"),
            "the hub counts its own"
        );
        assert!(!counts.contains_key("captures_10"), "nothing to send at 0");
    }

    #[test]
    fn time_played_counts_whole_minutes() {
        let mut record = Record::default();
        record.play(59_000);
        assert_eq!(record.count(Counter::Minutes), 0);
        record.play(2_000);
        assert_eq!(record.count(Counter::Minutes), 1);
        assert_eq!(record.play_ms, 1_000);
        record.play(600_000);
        assert_eq!(record.count(Counter::Minutes), 11);
        let hours = find("hours_10").unwrap();
        assert_eq!(hours.amount(90), "1.5 h");
        assert_eq!(hours.amount(600), "10 h");
        assert_eq!(find("kills_100").unwrap().amount(37), "37");
    }

    #[test]
    fn maps_and_servers_are_counted_once_each_and_bounded() {
        let mut record = Record::default();
        assert!(Record::remember(&mut record.maps, "mp/FFA1"));
        assert!(!Record::remember(&mut record.maps, "mp/ffa1"));
        assert!(!Record::remember(&mut record.maps, "  "));
        assert!(!Record::remember(
            &mut record.maps,
            &"x".repeat(NAME_MAX + 1)
        ));
        assert_eq!(record.count(Counter::Maps), 1);
        for index in 0..SET_MAX + 10 {
            Record::remember(&mut record.servers, &format!("1.2.3.4:{index}"));
        }
        assert_eq!(record.count(Counter::Servers), SET_MAX as u64);
    }

    #[test]
    fn the_hub_restores_counts_after_a_reinstall() {
        let mut record = Record::default();
        assert!(record.merge_hub(&[
            held("kills_100", 40, 0),
            held("first_blood", 1, 1_000),
            held("maps_10", 7, 0),
            held("storyteller", 1, 5),
            held("unknown", 9, 9),
        ]));
        assert_eq!(record.count(Counter::Kills), 40);
        assert_eq!(record.count(Counter::Maps), 7);
        assert_eq!(record.unlocked["first_blood"], 1_000);
        // A new map does not count until the set passes the hub's count.
        Record::remember(&mut record.maps, "mp/ffa1");
        assert_eq!(record.count(Counter::Maps), 7);
        assert!(
            !record.merge_hub(&[held("kills_100", 12, 0)]),
            "lower changes nothing"
        );
    }

    #[test]
    fn standings_show_the_furthest_count_and_the_hubs_own() {
        let mut record = Record::default();
        record.add(Counter::Kills, 3);
        let _ = record.unlock(50);
        let standings = record.standings(&[held("kills_100", 10, 0), held("storyteller", 1, 77)]);
        let of = |id: &str| *standings.iter().find(|s| s.kind.id == id).unwrap();
        assert_eq!(of("kills_100").progress, 10);
        assert_eq!(of("kills_100").unlocked, None);
        assert_eq!(of("first_blood").unlocked, Some(50));
        assert_eq!(of("first_blood").progress, 1, "capped at the goal");
        assert_eq!(of("storyteller").unlocked, Some(77));
        assert_eq!(of("surveyor").progress, 0);
        assert!((of("kills_100").fraction() - 0.1).abs() < 1e-6);
        assert_eq!(standings.len(), ALL.len());
    }

    #[test]
    fn forgetting_sets_the_count_just_below_the_goal() {
        let mut record = Record::default();
        record.add(Counter::Kills, 150);
        for name in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k"] {
            Record::remember(&mut record.maps, name);
        }
        record.weapons = 0b1111_1111;
        record.play(700 * 60_000 + 5);
        let _ = record.unlock(1);
        assert!(record.unlocked.contains_key("kills_100"));
        record.forget(find("kills_100").unwrap());
        assert_eq!(record.count(Counter::Kills), 99);
        assert!(!record.unlocked.contains_key("kills_100"));
        assert!(record.unlocked.contains_key("first_blood"), "still reached");
        // The next count unlocks it again.
        record.add(Counter::Kills, 1);
        assert_eq!(
            record.unlock(2).iter().map(|k| k.id).collect::<Vec<_>>(),
            ["kills_100"]
        );
        record.forget(find("first_blood").unwrap());
        assert_eq!(record.count(Counter::Kills), 0);
        assert!(
            !record.unlocked.contains_key("kills_100"),
            "no longer reached"
        );
        record.forget(find("maps_10").unwrap());
        assert_eq!(record.count(Counter::Maps), 9);
        record.forget(find("arsenal").unwrap());
        assert_eq!(record.count(Counter::Weapons), 7);
        record.forget(find("hours_10").unwrap());
        assert_eq!((record.count(Counter::Minutes), record.play_ms), (599, 0));
        record.unlocked.insert("storyteller".into(), 5);
        record.forget(find("storyteller").unwrap());
        assert!(!record.unlocked.contains_key("storyteller"));
    }

    #[test]
    fn clearing_takes_the_higher_goals_on_the_same_counter() {
        assert_eq!(
            cleared_with("first_blood"),
            ["first_blood", "kills_100", "kills_1000"]
        );
        assert_eq!(cleared_with("kills_100"), ["kills_100", "kills_1000"]);
        assert_eq!(cleared_with("saber_kills_100"), ["saber_kills_100"]);
        assert_eq!(cleared_with("maps_10"), ["maps_10", "maps_25"]);
        assert_eq!(cleared_with("storyteller"), ["storyteller"]);
        assert!(cleared_with("nope").is_empty());
    }

    #[test]
    fn the_file_round_trips_and_survives_damage() {
        let mut record = Record::default();
        record.add(Counter::Kills, 12);
        record.raise(Counter::BestStreak, 4);
        record.play(90_500);
        Record::remember(&mut record.maps, "mp/duel6");
        Record::remember(&mut record.servers, "1.2.3.4:29070");
        record.weapons = 0b101;
        record.floors[Counter::Maps as usize] = 3;
        let _ = record.unlock(42);
        let back = Record::from_json(&record.to_json());
        assert_eq!(back, record);
        // Wrong types and unknown ids count as nothing.
        let damaged = json!({
            "counts": {"kills": "many", "duel_wins": 3},
            "maps": [1, "mp/ffa3"],
            "unlocked": {"nope": 5, "first_blood": "x"},
            "play_ms": 999_999_999,
        });
        let read = Record::from_json(&damaged);
        assert_eq!(read.count(Counter::Kills), 0);
        assert_eq!(read.count(Counter::DuelWins), 3);
        assert_eq!(read.count(Counter::Maps), 1);
        assert!(read.unlocked.is_empty());
        assert!(read.play_ms < 60_000);
        assert_eq!(Record::from_json(&Value::Null), Record::default());
    }
}
