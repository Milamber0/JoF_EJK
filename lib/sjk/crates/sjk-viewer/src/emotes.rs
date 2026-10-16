//! Emotes (`docs/hub-chat.md`, "Emotes in the client"): short animations an SJK
//! player plays, which other SJK players on the same game server see. They travel
//! through the SJK hub, not the game server, so stock servers and clients know
//! nothing of them.
//!
//! This is the groundwork: the catalogue's format and loader, the `sjkemote` command
//! (named apart from the `emote` some server mods have),
//! and [`ActiveEmotes`], which says which player plays which emote right now. The
//! animations, sounds and art are separate work: an emote's torso and legs
//! animations and sound are named in its catalogue file, and the player's model
//! code reads [`active`] to play them. Until then a received emote is written to the
//! console.
//!
//! The catalogue is data in the game's file system (pk3s or the folder):
//! `emotes/<id>.emote`, a JSON object:
//!
//! ```json
//! {"name": "Wave", "torso": "BOTH_...", "legs": "", "length_ms": 2500,
//!  "loop": false, "sound": "sound/emotes/wave.mp3", "clip": "", "local": false,
//!  "moving": false, "icon": "gfx/emotes/wave.png"}
//! ```
//!
//! `clip` names an animation of a rig model (`docs/rig-models.md`): a player wearing
//! a rig that has it plays it on the rig's own skeleton. `rig` names the model folder
//! the clip comes from: other rig models with the same joint names borrow it. `local` keeps the emote on
//! this client: it plays here and is never sent to the hub (an emote made for a
//! model only this player has). `moving` keeps it playing while the player moves.
//! The sound plays on the player and stops when the emote is cut short. With its
//! length (`sound_ms`) a looping emote plays it again each time it ends, and
//! `sound_loop` (its length `sound_loop_ms`) follows the first sound and repeats
//! for as long as the emote plays (music with an intro and a loop). `icon` is a
//! picture (PNG, JPG or TGA) the quick wheel shows for it (`emote_icons.rs`). The id
//! [`STOP`] ends the player's emote.
//!
//! `<id>` is the emote's id at the hub: 1 to 32 of `a` to `z`, `0` to `9` and `_`.
//!
//! [`SYNC`] (`sjkemote sync [player]`) joins another player's emote in step: the hub
//! relays the leader's slot with it, and each client starts the follower at the
//! leader's start as that client saw it ([`ActiveEmotes::sync`]), so the two dance in
//! phase on every screen whatever each one's delay. The follower's music starts
//! where the leader's is, so both play it in step.

use sjk_identity::Emote;
use std::time::{Duration, Instant};

/// Where the catalogue's files are.
pub(crate) const FOLDER: &str = "emotes";
/// The `emote` command.
pub(crate) const COMMAND: &str = "sjkemote";
/// The id that ends the player's emote (`sjkemote stop`), never a catalogue entry's.
pub(crate) const STOP: &str = "stop";
/// The word that joins another player's emote in step (`sjkemote sync [player]`),
/// never a catalogue entry's.
pub(crate) const SYNC: &str = "sync";
/// How far (game units) `sjkemote sync` alone looks for a player to join.
pub(crate) const SYNC_RANGE: f32 = 512.0;
pub(crate) const HELP: &str = "Play an emote the SJK players on this server see (sjkemote alone lists them; sjkemote sync [player] joins one in step)";
/// How loud emote sounds play, times the effects volume (Settings > Sound > Emotes).
pub(crate) const VOLUME_CVAR: &str = "s_emoteVolume";
/// [`VOLUME_CVAR`]'s default: recorded music sits well under the game's effects.
pub(crate) const DEFAULT_VOLUME: f64 = 2.0;
/// How long an emote the catalogue does not know lasts.
const UNKNOWN_LENGTH: Duration = Duration::from_secs(3);
/// How far a player may move (game units) before their emote stops.
const MOVE_LIMIT: f32 = 24.0;
/// Slots the game numbers.
const SLOTS: usize = 64;

/// One emote of the catalogue.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EmoteDef {
    /// Its id at the hub, the file's name.
    pub(crate) id: String,
    /// What it is called.
    pub(crate) name: String,
    /// The torso animation (`BOTH_...` or `TORSO_...`), or empty.
    pub(crate) torso: String,
    /// The legs animation, or empty.
    pub(crate) legs: String,
    /// How long it plays.
    pub(crate) length: Duration,
    /// Whether it loops until the player moves.
    pub(crate) looping: bool,
    /// The sound it plays, or empty.
    pub(crate) sound: String,
    /// The rig model animation it plays, or empty.
    pub(crate) clip: String,
    /// Played on this client only, never sent to the hub.
    pub(crate) local: bool,
    /// Keeps playing while the player moves.
    pub(crate) moving: bool,
    /// Its picture, or empty.
    pub(crate) icon: String,
    /// How long `sound` lasts (zero: unknown, played once).
    pub(crate) sound_length: Duration,
    /// The sound that follows `sound` and repeats while the emote plays, or empty.
    pub(crate) sound_loop: String,
    /// How long `sound_loop` lasts.
    pub(crate) sound_loop_length: Duration,
    /// The rig model folder (under `models/players`) its clip comes from, or empty.
    pub(crate) rig: String,
}

/// Whether `id` may name an emote at the hub.
pub(crate) fn valid_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// The fields a catalogue file may hold.
const FIELDS: [&str; 14] = [
    "name",
    "torso",
    "legs",
    "length_ms",
    "loop",
    "sound",
    "clip",
    "local",
    "moving",
    "icon",
    "rig",
    "sound_ms",
    "sound_loop",
    "sound_loop_ms",
];
/// The longest animation or sound name.
const PATH_MAX: usize = 64;
/// The shortest and longest emote that is not a loop, in milliseconds.
const LENGTH_MS: std::ops::RangeInclusive<u64> = 100..=60_000;

/// Read one catalogue file: `id` from its name, `text` its contents.
pub(crate) fn parse(id: &str, text: &str) -> Result<EmoteDef, String> {
    if !valid_id(id) {
        return Err(format!(
            "{id:?} is not an emote id (1 to 32 of a to z, 0 to 9 and _)"
        ));
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("not JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "not a JSON object".to_owned())?;
    if let Some(unknown) = object.keys().find(|key| !FIELDS.contains(&key.as_str())) {
        return Err(format!("unknown field {unknown:?}"));
    }
    let text_field = |field: &str, max: usize| -> Result<String, String> {
        match object.get(field) {
            None => Ok(String::new()),
            Some(serde_json::Value::String(text))
                if text.chars().count() <= max && !text.chars().any(char::is_control) =>
            {
                Ok(text.clone())
            }
            Some(_) => Err(format!("{field} must be text of at most {max} characters")),
        }
    };
    let name = text_field("name", 32)?;
    if name.trim().is_empty() {
        return Err("name is needed".to_owned());
    }
    let flag = |field: &str| match object.get(field) {
        None => Ok(false),
        Some(serde_json::Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{field} must be true or false")),
    };
    let looping = flag("loop")?;
    let milliseconds = |field: &str| match object.get(field) {
        None => Ok(Duration::ZERO),
        Some(value) => value
            .as_u64()
            .filter(|ms| *ms <= 600_000)
            .map(Duration::from_millis)
            .ok_or_else(|| format!("{field} must be 0 to 600000")),
    };
    let length = match object.get("length_ms") {
        None if looping => 0,
        Some(value) => value
            .as_u64()
            .filter(|ms| LENGTH_MS.contains(ms))
            .ok_or_else(|| "length_ms must be 100 to 60000".to_owned())?,
        None => return Err("length_ms is needed unless the emote loops".to_owned()),
    };
    Ok(EmoteDef {
        id: id.to_owned(),
        name,
        torso: text_field("torso", PATH_MAX)?,
        legs: text_field("legs", PATH_MAX)?,
        length: Duration::from_millis(length),
        looping,
        sound: text_field("sound", PATH_MAX)?,
        clip: text_field("clip", PATH_MAX)?,
        local: flag("local")?,
        moving: flag("moving")?,
        icon: text_field("icon", PATH_MAX)?,
        sound_length: milliseconds("sound_ms")?,
        sound_loop: text_field("sound_loop", PATH_MAX)?,
        sound_loop_length: milliseconds("sound_loop_ms")?,
        rig: {
            let rig = text_field("rig", PATH_MAX)?;
            if rig.contains(['/', '\\']) || rig.contains("..") {
                return Err("rig must be a model folder's name".to_owned());
            }
            rig
        },
    })
}

/// The catalogue in `vfs`: every `emotes/<id>.emote` that reads, by id; the ones that
/// do not are reported to the log by name.
pub(crate) fn load(vfs: &sjk_vfs::VirtualFileSystem) -> Vec<EmoteDef> {
    let mut catalogue = Vec::new();
    for listed in vfs.list_files(FOLDER, ".emote") {
        let path = format!("{FOLDER}/{listed}");
        let id = listed.trim_end_matches(".emote");
        let read = vfs
            .read(&path)
            .map_err(|error| error.to_string())
            .and_then(|asset| asset.ok_or_else(|| "missing".to_owned()))
            .and_then(|asset| String::from_utf8(asset.bytes).map_err(|_| "not UTF-8".to_owned()))
            .and_then(|text| parse(id, &text));
        match read {
            Ok(def) => catalogue.push(def),
            Err(why) => crate::log::progress(format_args!("emotes: {path}: {why}")),
        }
    }
    catalogue.retain(|def| {
        let reserved = def.id == STOP || def.id == SYNC;
        if reserved {
            crate::log::progress(format_args!(
                "emotes: {FOLDER}/{}.emote: the id is reserved",
                def.id
            ));
        }
        !reserved
    });
    catalogue.sort_by(|a, b| a.id.cmp(&b.id));
    catalogue.dedup_by(|a, b| a.id == b.id);
    catalogue
}

/// One emote being played.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActiveEmote {
    /// The emote's id.
    pub(crate) emote: String,
    pub(crate) started: Instant,
    /// When it ends, unless the player moves first; `None` while a loop plays.
    pub(crate) until: Option<Instant>,
    /// Where the player stood when it started.
    pub(crate) origin: [f32; 3],
    /// Its sound, or empty.
    pub(crate) sound: String,
    /// Whether it keeps playing while the player moves.
    pub(crate) moving: bool,
    /// The sound to start next while it plays (its loop, or its sound again), when,
    /// and how long that one lasts.
    pub(crate) next_sound: Option<(Instant, String, Duration)>,
    /// The player it follows in step (`sjkemote sync`): one joining it follows them.
    pub(crate) leader: Option<u8>,
}

/// An emote's music: its first sound, then the one that repeats after it (its loop,
/// or for a looping emote the first again), each with its length.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Music {
    first: String,
    first_length: Duration,
    then: String,
    then_length: Duration,
}

impl Music {
    fn of(def: &EmoteDef) -> Self {
        let (then, then_length) = if def.sound.is_empty() || def.sound_length.is_zero() {
            (String::new(), Duration::ZERO)
        } else if !def.sound_loop.is_empty() {
            (def.sound_loop.clone(), def.sound_loop_length)
        } else if def.looping {
            (def.sound.clone(), def.sound_length)
        } else {
            (String::new(), Duration::ZERO)
        };
        Self {
            first: def.sound.clone(),
            first_length: def.sound_length,
            then,
            then_length,
        }
    }

    /// The sound playing `elapsed` after the start, how far into it, and how long
    /// before `then` starts (again); `None` once it has all played.
    fn at(&self, elapsed: Duration) -> Option<(&str, Duration, Option<Duration>)> {
        if self.first.is_empty() {
            return None;
        }
        // A first sound of unknown length: the mixer plays nothing past its end.
        if self.first_length.is_zero() || elapsed < self.first_length {
            let next = (!self.then.is_empty() && !self.first_length.is_zero())
                .then(|| self.first_length - elapsed);
            return Some((&self.first, elapsed, next));
        }
        if self.then.is_empty() {
            return None;
        }
        let past = elapsed - self.first_length;
        if self.then_length.is_zero() {
            return Some((&self.then, past, None));
        }
        let into = Duration::from_nanos(
            u64::try_from(past.as_nanos() % self.then_length.as_nanos()).unwrap_or(0),
        );
        Some((&self.then, into, Some(self.then_length - into)))
    }
}

/// A sound to start on, or stop from, a player: what the audio does with emotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Cue {
    /// Start `sound` `offset` into it (a synced emote's music joining the leader's),
    /// at the sample `align`'s plays it (the leader's, for a follower).
    Start {
        slot: u8,
        sound: String,
        offset: Duration,
        align: Option<u8>,
    },
    Stop {
        slot: u8,
    },
}

/// Which player plays which emote, by slot.
pub(crate) struct ActiveEmotes {
    slots: [Option<ActiveEmote>; SLOTS],
    /// Sounds to start and stop, taken by the audio each frame.
    cues: Vec<Cue>,
}

impl Default for ActiveEmotes {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            cues: Vec::new(),
        }
    }
}

impl ActiveEmotes {
    /// The emote `slot` plays, if any.
    pub(crate) fn get(&self, slot: u8) -> Option<&ActiveEmote> {
        self.slots.get(usize::from(slot))?.as_ref()
    }

    /// `slot` starts `emote` at `now`, standing at `origin`; `def` is its catalogue
    /// entry if this client has it.
    pub(crate) fn start(
        &mut self,
        slot: u8,
        emote: &str,
        def: Option<&EmoteDef>,
        origin: [f32; 3],
        now: Instant,
    ) {
        let Some(entry) = self.slots.get_mut(usize::from(slot)) else {
            return;
        };
        let until = match def {
            Some(def) if def.looping => None,
            Some(def) => Some(now + def.length),
            None => Some(now + UNKNOWN_LENGTH),
        };
        if entry.as_ref().is_some_and(|old| !old.sound.is_empty()) {
            self.cues.push(Cue::Stop { slot });
        }
        let music = def.map(Music::of).unwrap_or_default();
        let sound = music.first.clone();
        if !sound.is_empty() {
            self.cues.push(Cue::Start {
                slot,
                sound: sound.clone(),
                offset: Duration::ZERO,
                align: None,
            });
        }
        // After the first sound: its loop, or for a looping emote the sound again.
        let next_sound = music
            .at(Duration::ZERO)
            .and_then(|(_, _, next)| next)
            .map(|next| (now + next, music.then.clone(), music.then_length));
        *entry = Some(ActiveEmote {
            emote: emote.to_owned(),
            started: now,
            until,
            origin,
            sound,
            moving: def.is_some_and(|def| def.moving),
            next_sound,
            leader: None,
        });
    }

    /// `slot` joins `leader`'s `emote` in step at `now`, standing at `origin`: it
    /// starts when the leader's started here (the player the leader follows, when the
    /// leader follows someone), and its music from where the leader's is. False,
    /// changing nothing, when the leader is not playing `emote`: the caller starts it
    /// as usual then.
    pub(crate) fn sync(
        &mut self,
        slot: u8,
        leader: u8,
        emote: &str,
        def: Option<&EmoteDef>,
        origin: [f32; 3],
        now: Instant,
    ) -> bool {
        let Some(lead) = self.get(leader).filter(|lead| lead.emote == emote) else {
            return false;
        };
        let (started, lead_until) = (lead.started, lead.until);
        let leader = match lead.leader {
            Some(root) if self.follows(leader, root) => root,
            _ => leader,
        };
        if leader == slot || usize::from(slot) >= SLOTS {
            return false;
        }
        let until = match def {
            Some(def) if def.looping => None,
            Some(def) => Some(started + def.length),
            None => lead_until,
        };
        self.stop(slot);
        let music = def.map(Music::of).unwrap_or_default();
        let mut next_sound = None;
        if let Some((sound, offset, next)) = music.at(now.saturating_duration_since(started)) {
            self.cues.push(Cue::Start {
                slot,
                sound: sound.to_owned(),
                offset,
                align: Some(leader),
            });
            next_sound = next.map(|next| (now + next, music.then.clone(), music.then_length));
        }
        self.slots[usize::from(slot)] = Some(ActiveEmote {
            emote: emote.to_owned(),
            started,
            until,
            origin,
            sound: music.first.clone(),
            moving: def.is_some_and(|def| def.moving),
            next_sound,
            leader: Some(leader),
        });
        true
    }

    /// Whether `slot` plays the emote `leader` plays, from the same start.
    fn follows(&self, slot: u8, leader: u8) -> bool {
        match (self.get(slot), self.get(leader)) {
            (Some(follower), Some(lead)) => {
                follower.emote == lead.emote && follower.started == lead.started
            }
            _ => false,
        }
    }

    /// The players emoting now, by slot.
    pub(crate) fn playing(&self) -> impl Iterator<Item = (u8, &ActiveEmote)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| Some((u8::try_from(slot).ok()?, entry.as_ref()?)))
    }

    /// End `slot`'s emote now, and its sound.
    pub(crate) fn stop(&mut self, slot: u8) {
        if let Some(old) = self.slots.get_mut(usize::from(slot)).and_then(Option::take)
            && !old.sound.is_empty()
        {
            self.cues.push(Cue::Stop { slot });
        }
    }

    /// The sounds to start and stop since the last call.
    pub(crate) fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// End the emotes that are over at `now`, or whose player moved away (unless the
    /// emote keeps playing while moving) or left (`position` says where each slot's
    /// player is; `None` for one that left). An emote cut short stops its sound; one
    /// that ran its length lets the sound finish.
    pub(crate) fn update(&mut self, now: Instant, position: impl Fn(u8) -> Option<[f32; 3]>) {
        for (slot, entry) in self.slots.iter_mut().enumerate() {
            let Some(active) = entry else { continue };
            let Ok(slot) = u8::try_from(slot) else {
                continue;
            };
            if let Some((at, sound, length)) = &mut active.next_sound
                && now >= *at
            {
                self.cues.push(Cue::Start {
                    slot,
                    sound: sound.clone(),
                    offset: Duration::ZERO,
                    align: active.leader,
                });
                if length.is_zero() {
                    active.next_sound = None;
                } else {
                    while *at <= now {
                        *at += *length;
                    }
                }
            }
            let over = active.until.is_some_and(|until| now >= until);
            let gone = match position(slot) {
                None => true,
                Some(_) if active.moving => false,
                Some(at) => {
                    let d: f32 = at
                        .iter()
                        .zip(active.origin)
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum();
                    d > MOVE_LIMIT * MOVE_LIMIT
                }
            };
            if gone && !active.sound.is_empty() {
                self.cues.push(Cue::Stop { slot });
            }
            if over || gone {
                *entry = None;
            }
        }
    }
}

/// The catalogue (once a game's files are mounted) and the emotes playing now.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) catalogue: Option<Vec<EmoteDef>>,
    pub(crate) active: ActiveEmotes,
    /// Whether the catalogue's sounds were handed to the audio to load.
    pub(crate) sounds_loaded: bool,
}

static STATE: std::sync::LazyLock<std::sync::Mutex<State>> =
    std::sync::LazyLock::new(std::sync::Mutex::default);

pub(crate) fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The emote the player in `slot` plays now, for the player's model code.
// The seam the stock animations and sounds plug into; rig clips use `playing`.
#[allow(dead_code)]
pub(crate) fn active(slot: u8) -> Option<ActiveEmote> {
    lock().active.get(slot).cloned()
}

/// A rig clip the player in `slot` plays at `now`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Playing {
    pub(crate) clip: String,
    /// Seconds since it started.
    pub(crate) seconds: f32,
    /// Seconds before it ends, unless the player moves first; `None` for a loop.
    pub(crate) remaining: Option<f32>,
    pub(crate) looping: bool,
}

/// The rig clip the player in `slot` plays at `now`, if their emote names one.
pub(crate) fn playing(slot: u8, now: Instant) -> Option<Playing> {
    let state = lock();
    let active = state.active.get(slot)?;
    let def = state
        .catalogue
        .as_ref()?
        .iter()
        .find(|def| def.id == active.emote)?;
    (!def.clip.is_empty()).then(|| Playing {
        clip: def.clip.clone(),
        seconds: now.saturating_duration_since(active.started).as_secs_f32(),
        remaining: active
            .until
            .map(|until| until.saturating_duration_since(now).as_secs_f32()),
        looping: def.looping,
    })
}

/// What `emote` alone prints: the catalogue, or how emotes come.
pub(crate) fn listing(catalogue: &[EmoteDef]) -> Vec<String> {
    if catalogue.is_empty() {
        return vec![
            "No emotes are installed yet (emotes/<id>.emote in the game's files).".to_owned(),
            "sjkemote <id> still sends any id to the SJK players here.".to_owned(),
        ];
    }
    catalogue
        .iter()
        .map(|def| {
            let length = if def.looping {
                "loops".to_owned()
            } else {
                format!("{:.1} s", def.length.as_secs_f32())
            };
            format!("{} - {}, {length}", def.id, def.name)
        })
        .chain([
            format!("{COMMAND} {STOP} ends yours."),
            format!("{COMMAND} {SYNC} [player] joins another player's emote in step."),
        ])
        .collect()
}

/// `name` without colour codes, lower case: what `sjkemote sync <player>` matches.
pub(crate) fn plain_name(name: &str) -> String {
    plain_name_shown(name).to_lowercase()
}

/// `name` without colour codes, for the console.
pub(crate) fn plain_name_shown(name: &str) -> String {
    let mut chars = name.chars().peekable();
    let mut plain = String::new();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_alphanumeric) {
            chars.next();
        } else {
            plain.push(c);
        }
    }
    plain
}

/// Whether to believe `emote`: the game shows `shown` in its slot, and the claim the
/// hub took the emote's slot from was made under that name (the badges' rule).
pub(crate) fn trusted(emote: &Emote, shown: Option<&str>) -> bool {
    shown.is_some_and(|shown| sjk_identity::names_match(&emote.claimed_name, shown))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAVE: &str = r#"{"name": "Wave", "torso": "BOTH_ENGAGETAUNT", "legs": "",
        "length_ms": 2500, "loop": false, "sound": "sound/emotes/wave.mp3"}"#;

    fn wave() -> EmoteDef {
        parse("wave", WAVE).unwrap()
    }

    #[test]
    fn a_rig_clip_emote_reads_its_clip_and_stays_local() {
        let def = parse(
            "kane_dance",
            r#"{"name": "Dance", "clip": "kane_dance", "length_ms": 3400, "local": true}"#,
        )
        .unwrap();
        assert_eq!((def.clip.as_str(), def.local), ("kane_dance", true));
        assert!(!wave().local && wave().clip.is_empty());
        assert!(parse("x", r#"{"name": "X", "length_ms": 500, "local": 1}"#).is_err());
    }

    #[test]
    fn an_emote_file_parses_and_bad_ones_are_named() {
        let def = wave();
        assert_eq!(def.id, "wave");
        assert_eq!(def.name, "Wave");
        assert_eq!(def.torso, "BOTH_ENGAGETAUNT");
        assert_eq!(def.length, Duration::from_millis(2500));
        assert!(!def.looping);
        assert_eq!(def.sound, "sound/emotes/wave.mp3");
        // Only the name is required; a loop needs no length.
        let sit = parse(
            "sit",
            r#"{"name": "Sit", "legs": "BOTH_SIT1", "loop": true}"#,
        )
        .unwrap();
        assert!(sit.looping && sit.torso.is_empty());
        for (id, text, why) in [
            ("Wave", WAVE, "id"),
            ("wave", "not json", "JSON"),
            ("wave", r#"{"torso": "X"}"#, "name"),
            ("wave", r#"{"name": "Wave"}"#, "length_ms"),
            ("wave", r#"{"name": "Wave", "length_ms": 50}"#, "length_ms"),
            (
                "wave",
                r#"{"name": "Wave", "length_ms": 1000, "extra": 1}"#,
                "extra",
            ),
        ] {
            let error = parse(id, text).unwrap_err();
            assert!(error.contains(why), "{id} {text}: {error}");
        }
    }

    #[test]
    fn the_listing_names_each_emote_or_says_none_is_installed() {
        assert!(listing(&[])[0].starts_with("No emotes are installed"));
        let sit = parse("sit", r#"{"name": "Sit", "loop": true}"#).unwrap();
        assert_eq!(
            listing(&[sit, wave()]),
            [
                "sit - Sit, loops",
                "wave - Wave, 2.5 s",
                "sjkemote stop ends yours.",
                "sjkemote sync [player] joins another player's emote in step."
            ]
        );
    }

    #[test]
    fn emote_ids_are_the_hubs() {
        assert!(valid_id("wave"));
        assert!(valid_id("sit_2"));
        assert!(!valid_id(""));
        assert!(!valid_id("Wave"));
        assert!(!valid_id(&"a".repeat(33)));
    }

    #[test]
    fn known_emotes_last_their_length_and_unknown_ones_three_seconds() {
        let t0 = Instant::now();
        let mut active = ActiveEmotes::default();
        active.start(3, "wave", Some(&wave()), [0.0; 3], t0);
        active.start(4, "dance", None, [0.0; 3], t0);
        let here = |_| Some([0.0; 3]);
        active.update(t0 + Duration::from_millis(2_400), here);
        assert_eq!(active.get(3).map(|e| e.emote.as_str()), Some("wave"));
        active.update(t0 + Duration::from_millis(2_600), here);
        assert!(active.get(3).is_none());
        assert!(active.get(4).is_some());
        active.update(t0 + Duration::from_secs(3), here);
        assert!(active.get(4).is_none());
    }

    #[test]
    fn a_moving_emote_survives_walking_and_sounds_start_and_stop() {
        let t0 = Instant::now();
        let dance = parse(
            "dance",
            r#"{"name": "Dance", "length_ms": 6800, "moving": true,
                "sound": "sound/emotes/dance.wav"}"#,
        )
        .unwrap();
        let mut active = ActiveEmotes::default();
        active.start(5, "dance", Some(&dance), [0.0; 3], t0);
        assert_eq!(
            active.take_cues(),
            vec![Cue::Start {
                slot: 5,
                sound: "sound/emotes/dance.wav".into(),
                offset: Duration::ZERO,
                align: None
            }]
        );
        active.update(t0 + Duration::from_secs(1), |_| Some([500.0, 0.0, 0.0]));
        assert!(active.get(5).is_some(), "it dances on while running");
        // Running its length lets the sound finish.
        active.update(t0 + Duration::from_secs(7), |_| Some([500.0, 0.0, 0.0]));
        assert!(active.get(5).is_none() && active.take_cues().is_empty());
        // Cut short (stopped, or replaced), the sound stops.
        active.start(5, "dance", Some(&dance), [0.0; 3], t0);
        active.start(5, "wave", Some(&wave()), [0.0; 3], t0);
        active.stop(5);
        assert_eq!(
            active.take_cues(),
            vec![
                Cue::Start {
                    slot: 5,
                    sound: "sound/emotes/dance.wav".into(),
                    offset: Duration::ZERO,
                    align: None
                },
                Cue::Stop { slot: 5 },
                Cue::Start {
                    slot: 5,
                    sound: "sound/emotes/wave.mp3".into(),
                    offset: Duration::ZERO,
                    align: None
                },
                Cue::Stop { slot: 5 },
            ]
        );
        assert!(active.get(5).is_none());
    }

    #[test]
    fn moving_or_leaving_ends_an_emote_and_a_loop_lasts_until_then() {
        let t0 = Instant::now();
        let sit = parse("sit", r#"{"name": "Sit", "loop": true}"#).unwrap();
        let mut active = ActiveEmotes::default();
        active.start(1, "sit", Some(&sit), [100.0, 0.0, 0.0], t0);
        active.start(2, "wave", Some(&wave()), [0.0; 3], t0);
        active.start(70, "wave", None, [0.0; 3], t0);
        assert!(active.get(70).is_none(), "no such slot");
        let later = t0 + Duration::from_secs(600);
        active.update(later, |slot| (slot == 1).then_some([110.0, 0.0, 0.0]));
        assert!(active.get(1).is_some(), "a loop, moved a little");
        assert!(active.get(2).is_none(), "left the server");
        active.update(later, |_| Some([130.0, 0.0, 0.0]));
        assert!(active.get(1).is_none(), "walked off");
    }

    #[test]
    fn an_emote_needs_the_slot_name_to_match_the_claim() {
        let emote = Emote {
            id: 1,
            at: 0,
            slot: 3,
            claimed_name: "^2Sol".to_owned(),
            key_id: "0123456789abcdef".to_owned(),
            emote: "wave".to_owned(),
            sync: None,
        };
        assert!(trusted(&emote, Some("Sol")));
        assert!(
            !trusted(&emote, Some("Fox")),
            "someone else in the slot now"
        );
        assert!(!trusted(&emote, None), "nobody in the slot");
    }

    #[test]
    fn music_with_an_intro_then_loops_and_a_looping_emote_repeats_its_sound() {
        let def = parse(
            "heavy",
            r#"{"name": "Heavy", "length_ms": 9000, "sound": "a.wav", "sound_ms": 2000,
                "sound_loop": "b.wav", "sound_loop_ms": 3000}"#,
        )
        .unwrap();
        let mut active = ActiveEmotes::default();
        let start = Instant::now();
        active.start(1, "heavy", Some(&def), [0.0; 3], start);
        let started = |active: &mut ActiveEmotes| {
            active
                .take_cues()
                .into_iter()
                .filter_map(|cue| match cue {
                    Cue::Start { sound, .. } => Some(sound),
                    Cue::Stop { .. } => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(started(&mut active), ["a.wav"]);
        let here = |_: u8| Some([0.0; 3]);
        active.update(start + Duration::from_millis(1900), here);
        assert!(started(&mut active).is_empty());
        active.update(start + Duration::from_millis(2000), here);
        assert_eq!(started(&mut active), ["b.wav"]);
        active.update(start + Duration::from_millis(5000), here);
        assert_eq!(started(&mut active), ["b.wav"]);
        // A looping emote with one sound plays it again when it ends.
        let looped = parse(
            "boom",
            r#"{"name": "Boom", "loop": true, "sound": "c.wav", "sound_ms": 4000}"#,
        )
        .unwrap();
        active.start(2, "boom", Some(&looped), [0.0; 3], start);
        let _ = active.take_cues();
        active.update(start + Duration::from_millis(4000), here);
        assert!(started(&mut active).contains(&"c.wav".to_owned()));
    }

    fn heavy() -> EmoteDef {
        parse(
            "heavy",
            r#"{"name": "Heavy", "length_ms": 9000, "sound": "a.wav", "sound_ms": 2000,
                "sound_loop": "b.wav", "sound_loop_ms": 3000}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_synced_emote_starts_with_its_leader_and_plays_the_music_in_step() {
        let t0 = Instant::now();
        let here = |_: u8| Some([0.0; 3]);
        let mut active = ActiveEmotes::default();
        active.start(1, "heavy", Some(&heavy()), [0.0; 3], t0);
        let _ = active.take_cues();
        let late = t0 + Duration::from_millis(1500);
        assert!(
            !active.sync(2, 1, "wave", Some(&wave()), [0.0; 3], late),
            "not that emote"
        );
        assert!(
            !active.sync(2, 9, "heavy", Some(&heavy()), [0.0; 3], late),
            "nobody there"
        );
        // Joining 1.5 s late (the hub's delay) still lines up with the leader, and
        // its music starts where the leader's is.
        active.update(late, here);
        assert!(active.sync(2, 1, "heavy", Some(&heavy()), [0.0; 3], late));
        let follower = active.get(2).unwrap();
        assert_eq!((follower.started, follower.leader), (t0, Some(1)));
        assert_eq!(follower.until, Some(t0 + Duration::from_millis(9000)));
        assert_eq!(
            active.take_cues(),
            [Cue::Start {
                slot: 2,
                sound: "a.wav".into(),
                offset: Duration::from_millis(1500),
                align: Some(1)
            }]
        );
        // The loop starts on both at the same moment.
        active.update(t0 + Duration::from_millis(2000), here);
        let b = |slot, align| Cue::Start {
            slot,
            sound: "b.wav".into(),
            offset: Duration::ZERO,
            align,
        };
        assert_eq!(active.take_cues(), [b(1, None), b(2, Some(1))]);
        // A third joining the follower follows the first leader.
        assert!(active.sync(3, 2, "heavy", Some(&heavy()), [0.0; 3], late));
        assert_eq!(active.get(3).unwrap().leader, Some(1));
    }

    #[test]
    fn a_follower_keeps_its_music_when_the_leader_stops() {
        let t0 = Instant::now();
        let here = |_: u8| Some([0.0; 3]);
        let mut active = ActiveEmotes::default();
        active.start(1, "heavy", Some(&heavy()), [0.0; 3], t0);
        assert!(active.sync(2, 1, "heavy", Some(&heavy()), [0.0; 3], t0));
        let _ = active.take_cues();
        active.stop(1);
        active.update(t0 + Duration::from_millis(1000), here);
        assert_eq!(active.take_cues(), [Cue::Stop { slot: 1 }]);
        active.stop(2);
        assert_eq!(active.take_cues(), [Cue::Stop { slot: 2 }]);
    }

    #[test]
    fn music_knows_what_plays_when() {
        let music = Music::of(&heavy());
        let at = |ms| music.at(Duration::from_millis(ms));
        assert_eq!(
            at(500),
            Some((
                "a.wav",
                Duration::from_millis(500),
                Some(Duration::from_millis(1500))
            ))
        );
        assert_eq!(
            at(8000),
            Some(("b.wav", Duration::ZERO, Some(Duration::from_millis(3000))))
        );
        let once = Music::of(&wave());
        assert_eq!(
            once.at(Duration::from_secs(9)).map(|(s, ..)| s),
            Some("sound/emotes/wave.mp3")
        );
        let timed = Music::of(
            &parse(
                "t",
                r#"{"name": "T", "length_ms": 3000, "sound": "t.wav", "sound_ms": 1000}"#,
            )
            .unwrap(),
        );
        assert_eq!(timed.at(Duration::from_secs(2)), None, "over");
    }
}
