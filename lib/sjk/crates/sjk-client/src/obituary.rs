//! BaseJKA obituary decoding and fixed-capacity kill-feed state.

use sjk_protocol::{EntityState, GameState, Snapshot};

const ET_EVENTS: u8 = 18;
const EVENT_MASK: u16 = 0xff;
const MAX_ENTITIES: usize = 1_024;
const MAX_CLIENTS: u16 = 64;
const CS_PLAYERS: usize = 1_131;

/// `EV_OBITUARY` in `codemp/game/bg_public.h:927`.
pub const EV_OBITUARY: u16 = 93;

/// Gender used by the gendered self-kill strings in `CG_Obituary`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Gender {
    Female,
    Neuter,
    #[default]
    Male,
}

/// One decoded obituary, preserving the localization keys selected by codemp.
///
/// `message` is the victim-only/self-kill key and `attacker_message` is the
/// three-part `victim message attacker` key. See
/// `codemp/cgame/cg_event.c:124-454`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObituaryEvent {
    /// Client number of the victim.
    pub target: u16,
    /// Client number of the attacker, or the legacy world entity number.
    pub attacker: u16,
    /// `meansOfDeath_t` ordinal carried by `eventParm`.
    pub means_of_death: u8,
    /// Localization key for world and self kills.
    pub message: &'static str,
    /// Localization key placed between victim and attacker for player kills.
    pub attacker_message: Option<&'static str>,
    /// Whether the current client killed another player.
    pub local_fragged: bool,
    /// Whether another player killed the current client.
    pub local_was_killed: bool,
    /// Snapshot server time at which the event was accepted.
    pub server_time: i32,
    /// The attacker's `forcePowersActive` bits in that snapshot (0 when the
    /// snapshot does not hold the attacker), which tell Force Grip from Force
    /// Lightning, as `MOD_FORCE_DARK` covers both.
    pub attacker_force: u32,
}

/// Eight-entry retained kill-feed ring. Oldest entries are overwritten.
pub struct KillFeed {
    entries: [Option<ObituaryEvent>; 8],
    next: usize,
    len: usize,
}

impl KillFeed {
    /// Construct an empty fixed-capacity feed.
    pub const fn new() -> Self {
        Self {
            entries: [None; 8],
            next: 0,
            len: 0,
        }
    }

    /// Append an event, overwriting the oldest entry once full.
    pub fn push(&mut self, event: ObituaryEvent) {
        self.entries[self.next] = Some(event);
        self.next = (self.next + 1) % self.entries.len();
        self.len = (self.len + 1).min(self.entries.len());
    }

    /// Number of retained entries.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Read an entry, where offset zero is newest.
    pub fn newest(&self, offset: usize) -> Option<ObituaryEvent> {
        if offset >= self.len {
            return None;
        }
        let index = (self.next + self.entries.len() - 1 - offset) % self.entries.len();
        self.entries[index]
    }
}

impl Default for KillFeed {
    fn default() -> Self {
        Self::new()
    }
}

/// Allocation-free entity-event observer for obituary events.
pub struct ObituaryTracker {
    signatures: [u16; MAX_ENTITIES],
    present: [bool; MAX_ENTITIES],
    feed: KillFeed,
    decoded: u64,
}

impl ObituaryTracker {
    /// Construct an empty event-signature tracker and feed.
    pub fn new() -> Self {
        Self {
            signatures: [0; MAX_ENTITIES],
            present: [false; MAX_ENTITIES],
            feed: KillFeed::new(),
            decoded: 0,
        }
    }

    /// Observe every entity event in one accepted snapshot exactly once.
    pub fn observe(&mut self, snapshot: &Snapshot, game: &GameState) {
        self.present.fill(false);
        for entity in &snapshot.entities {
            let index = usize::from(entity.number());
            if index >= MAX_ENTITIES {
                continue;
            }
            self.present[index] = true;
            let signature = raw_event(entity);
            if signature == 0 || signature == self.signatures[index] {
                continue;
            }
            self.signatures[index] = signature;
            if event_number(entity, signature) != EV_OBITUARY {
                continue;
            }
            let gender = client_gender(game, entity.other_entity_num());
            let mut event = legacy_obituary(
                entity,
                snapshot.player.client_num(),
                gender,
                snapshot.server_time,
            );
            event.attacker_force = attacker_force(snapshot, event.attacker);
            self.feed.push(event);
            self.decoded += 1;
        }
        for (index, present) in self.present.iter().copied().enumerate() {
            if !present {
                self.signatures[index] = 0;
            }
        }
    }

    /// Access the retained feed without copying it.
    pub const fn feed(&self) -> &KillFeed {
        &self.feed
    }

    /// Total obituary events decoded by this tracker.
    pub const fn decoded(&self) -> u64 {
        self.decoded
    }
}

impl Default for ObituaryTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// A client's name as the kill messages show it: the `n` key of its `CS_PLAYERS`
/// string with its colour codes, read from the string's bytes
/// ([`crate::LegacyClientInfo::name`]), or `noname` for a slot with no name at all,
/// where `CG_Obituary` would print an empty one.
pub fn obituary_name(game: &GameState, client: u16) -> std::borrow::Cow<'_, str> {
    game.config_string(CS_PLAYERS + usize::from(client))
        .and_then(|info| crate::LegacyClientInfo::new(info).name())
        .unwrap_or(std::borrow::Cow::Borrowed("noname"))
}

/// Select the exact localization key used by `CG_Obituary`.
pub fn legacy_obituary(
    entity: &EntityState,
    local_client: u16,
    gender: Gender,
    server_time: i32,
) -> ObituaryEvent {
    let target = entity.other_entity_num();
    let attacker = entity.other_entity_num2();
    let means = entity.event_parameter();
    let self_kill = target == attacker;
    let (message, attacker_message) = if self_kill {
        (self_message(means, gender), None)
    } else if target < MAX_CLIENTS && attacker < MAX_CLIENTS {
        ("", Some(kill_message(means)))
    } else if let Some(message) = single_message(means) {
        (message, None)
    } else {
        ("DIED_GENERIC", None)
    };
    ObituaryEvent {
        target,
        attacker,
        means_of_death: means,
        message,
        attacker_message,
        local_fragged: attacker == local_client && target != attacker,
        local_was_killed: target == local_client && attacker != target,
        server_time,
        attacker_force: 0,
    }
}

/// The `forcePowersActive` bits of client `attacker` in `snapshot`, 0 when it
/// holds no such client.
fn attacker_force(snapshot: &Snapshot, attacker: u16) -> u32 {
    if attacker >= MAX_CLIENTS {
        return 0;
    }
    if attacker == snapshot.player.client_num() {
        return snapshot.player.force_powers_active();
    }
    snapshot
        .entities
        .iter()
        .find(|entity| entity.number() == attacker)
        .map_or(0, EntityState::force_powers_active)
}

/// `cg_event.c:160-175`: water, slime, lava, crush, falling, suicide and
/// trigger_hurt all share `DIED_GENERIC`; only the target laser differs.
fn single_message(means: u8) -> Option<&'static str> {
    match means {
        33..=36 | 38 | 39 | 41 => Some("DIED_GENERIC"),
        40 => Some("DIED_LASER"),
        _ => None,
    }
}

fn self_message(means: u8, gender: Gender) -> &'static str {
    let stem = match means {
        4..=13 | 17 => "SUICIDE_SHOT",
        14 | 18..=30 => "SUICIDE_EXPLOSIVES",
        15 => "SUICIDE_ELECTROCUTED",
        38 => "SUICIDE_FALLDEATH",
        _ => "SUICIDE_GENERICDEATH",
    };
    match (stem, gender) {
        ("SUICIDE_SHOT", Gender::Female) => "SUICIDE_SHOT_FEMALE",
        ("SUICIDE_SHOT", Gender::Neuter) => "SUICIDE_SHOT_GENDERLESS",
        ("SUICIDE_SHOT", Gender::Male) => "SUICIDE_SHOT_MALE",
        ("SUICIDE_EXPLOSIVES", Gender::Female) => "SUICIDE_EXPLOSIVES_FEMALE",
        ("SUICIDE_EXPLOSIVES", Gender::Neuter) => "SUICIDE_EXPLOSIVES_GENDERLESS",
        ("SUICIDE_EXPLOSIVES", Gender::Male) => "SUICIDE_EXPLOSIVES_MALE",
        ("SUICIDE_ELECTROCUTED", Gender::Female) => "SUICIDE_ELECTROCUTED_FEMALE",
        ("SUICIDE_ELECTROCUTED", Gender::Neuter) => "SUICIDE_ELECTROCUTED_GENDERLESS",
        ("SUICIDE_ELECTROCUTED", Gender::Male) => "SUICIDE_ELECTROCUTED_MALE",
        ("SUICIDE_FALLDEATH", Gender::Female) => "SUICIDE_FALLDEATH_FEMALE",
        ("SUICIDE_FALLDEATH", Gender::Neuter) => "SUICIDE_FALLDEATH_GENDERLESS",
        ("SUICIDE_FALLDEATH", Gender::Male) => "SUICIDE_FALLDEATH_MALE",
        (_, Gender::Female) => "SUICIDE_GENERICDEATH_FEMALE",
        (_, Gender::Neuter) => "SUICIDE_GENERICDEATH_GENDERLESS",
        (_, Gender::Male) => "SUICIDE_GENERICDEATH_MALE",
    }
}

fn kill_message(means: u8) -> &'static str {
    match means {
        1 => "KILLED_STUN",
        2 => "KILLED_MELEE",
        3 => "KILLED_SABER",
        4 | 5 => "KILLED_BRYAR",
        6 | 7 => "KILLED_BLASTER",
        8 | 9 => "KILLED_DISRUPTOR",
        10 => "KILLED_DISRUPTORSNIPE",
        11 => "KILLED_BOWCASTER",
        12 => "KILLED_REPEATER",
        13 | 14 => "KILLED_REPEATERALT",
        15 | 16 => "KILLED_DEMP2",
        17 => "KILLED_FLECHETTE",
        18 => "KILLED_FLECHETTE_MINE",
        19 | 20 => "KILLED_ROCKET",
        21 | 22 => "KILLED_ROCKET_HOMING",
        23 | 24 => "KILLED_THERMAL",
        25 => "KILLED_TRIPMINE",
        26 => "KILLED_TRIPMINE_TIMED",
        27 => "KILLED_DETPACK",
        31 => "KILLED_DARKFORCE",
        32 => "KILLED_SENTRY",
        37 => "KILLED_TELEFRAG",
        38 => "KILLED_FORCETOSS",
        _ => "KILLED_GENERIC",
    }
}

fn client_gender(game: &GameState, client: u16) -> Gender {
    let Some(info) = game.config_string(CS_PLAYERS + usize::from(client)) else {
        return Gender::Male;
    };
    // Read from the bytes, not a UTF-8 view of the whole string: a name with a
    // Latin-1 letter (`é` is byte 0xE9) must not cost the player their gender.
    let info = crate::LegacyClientInfo::new(info);
    let Some(sex) = info.bytes("ds").or_else(|| info.bytes("sex")) else {
        return Gender::Male;
    };
    match sex.first().copied().map(|value| value.to_ascii_lowercase()) {
        Some(b'f') => Gender::Female,
        Some(b'n') => Gender::Neuter,
        _ => Gender::Male,
    }
}

fn raw_event(entity: &EntityState) -> u16 {
    if entity.entity_type() >= ET_EVENTS {
        u16::from(entity.entity_type())
    } else {
        entity.event()
    }
}

fn event_number(entity: &EntityState, raw: u16) -> u16 {
    if entity.entity_type() >= ET_EVENTS {
        u16::from(entity.entity_type() - ET_EVENTS)
    } else {
        raw & EVENT_MASK
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_protocol::LEGACY_ENTITY_FIELDS;

    /// A self-kill by `client` with `means`, as the event entity carries it.
    fn self_kill(client: u16, means: u8) -> EntityState {
        let mut entity = EntityState::zero(200, &LEGACY_ENTITY_FIELDS);
        entity.set_raw_field(8, u32::from(ET_EVENTS) + u32::from(EV_OBITUARY));
        entity.set_raw_field(59, u32::from(client));
        entity.set_raw_field(39, u32::from(client));
        entity.set_raw_field(42, u32::from(means));
        entity
    }

    #[test]
    fn a_latin1_name_keeps_the_players_gender() {
        let mut game = GameState::empty_local(1);
        game.replace_config_string(
            CS_PLAYERS + 4,
            b"n\\Zo\xe9\\t\\0\\model\\jan\\ds\\f".to_vec(),
        )
        .unwrap();
        let gender = client_gender(&game, 4);
        assert_eq!(gender, Gender::Female);
        let event = legacy_obituary(&self_kill(4, 38), 0, gender, 0);
        assert_eq!(event.message, "SUICIDE_FALLDEATH_FEMALE");
        assert_eq!(event.attacker_message, None);
    }
}
