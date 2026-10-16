//! The viewer's mute list (`docs/hub-chat.md`, "Muting a player"): the players muted
//! on this PC, read from `chat-mutes.txt` beside `config.cfg` at start and written back
//! at every change ([`crate::chat_mutes`]), and the server's players they are matched
//! against. The chat feed, the dock, the SJK chat page and the profile cards read it;
//! the game frame turns it into muted slots ([`crate::muted_players`]).
//!
//! It lives apart from the identity service: muting works with the identity off, for
//! players the hub does not know, by name.

use crate::chat_mutes::{self, MuteList, Slots};
use crate::sender_card::Place;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

struct State {
    list: MuteList,
    /// Where the list is kept, once read.
    path: Option<PathBuf>,
    /// Counts changes to the list, so what is derived from it can be kept.
    revision: u64,
    /// What the game shows in each slot of the server being played, empty without one.
    roster: Vec<String>,
    /// The player's own slot there.
    own: Option<usize>,
}

static STATE: Mutex<State> = Mutex::new(State {
    list: MuteList::new(),
    path: None,
    revision: 0,
    roster: Vec::new(),
    own: None,
});

fn lock() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Read the list from `directory` (the settings folder), once.
pub(crate) fn load(directory: &Path) {
    let mut state = lock();
    if state.path.is_some() {
        return;
    }
    let path = directory.join(chat_mutes::FILE);
    match chat_mutes::load(&path) {
        Ok(list) => {
            if !list.is_empty() {
                crate::log::progress(format_args!(
                    "mutes: {} player(s) muted on this PC",
                    list.entries().len()
                ));
            }
            state.list = list;
            state.revision += 1;
        }
        Err(error) => crate::log::progress(format_args!("mutes: {error}")),
    }
    state.path = Some(path);
}

/// The muted SJK keys.
pub(crate) fn muted_keys() -> Vec<String> {
    lock()
        .list
        .entries()
        .iter()
        .filter_map(|mute| mute.key_id.clone())
        .collect()
}

/// Whether the player with key `key_id` (or, without one, named `name`) is muted.
pub(crate) fn is_muted(key_id: Option<&str>, name: &str) -> bool {
    lock().list.contains(key_id, name)
}

/// Mute or unmute the player with key `key_id` (or, without one, named `name`) on this
/// PC, and keep the list. Nothing is sent anywhere.
pub(crate) fn set_muted(key_id: Option<&str>, name: &str, muted: bool) {
    let mut state = lock();
    if !state.list.set(key_id, name, muted) {
        return;
    }
    state.revision += 1;
    if let Some(path) = &state.path
        && let Err(error) = chat_mutes::save(path, &state.list)
    {
        crate::log::progress(format_args!("mutes: could not save: {error}"));
    }
}

/// Counts changes to the list.
pub(crate) fn revision() -> u64 {
    lock().revision
}

/// The server's players changed: `names` is what the game shows in each slot, in slot
/// order (empty without a server), and `own` the player's own slot.
pub(crate) fn set_roster<'a>(names: impl Iterator<Item = &'a str>, own: Option<usize>) {
    let mut state = lock();
    state.roster.clear();
    state.roster.extend(names.map(str::to_owned));
    state.own = own;
}

/// The roster as the matching reads it.
fn shown(roster: &[String]) -> [&str; chat_mutes::SLOTS] {
    std::array::from_fn(|slot| roster.get(slot).map_or("", String::as_str))
}

/// The slots of the server being played that the list names.
pub(crate) fn slots() -> Slots {
    let state = lock();
    if state.list.is_empty() || state.roster.is_empty() {
        return Slots::default();
    }
    let shown = shown(&state.roster);
    crate::player_identity::with_claims(|claims| {
        chat_mutes::slots(&state.list, claims, &shown, state.own)
    })
}

/// Where the player with key `key_id` (or, without one, named `name`) is on the server
/// being played.
pub(crate) fn place(key_id: Option<&str>, name: &str) -> Place {
    let state = lock();
    if state.roster.is_empty() {
        return Place::Unknown;
    }
    let shown = shown(&state.roster);
    let found = crate::player_identity::with_claims(|claims| {
        chat_mutes::locate(key_id, name, claims, &shown)
    });
    match found {
        Some((slot, false)) => Place::Slot(slot as u8),
        Some((slot, true)) => Place::SlotByName(slot as u8),
        None => Place::Away,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutes_are_kept_by_key_and_counted() {
        // A key no other test uses: the list is the process's own.
        let key = "feedfacecafe0001";
        let before = revision();
        assert!(!is_muted(Some(key), "Tester"));
        set_muted(Some(key), "^3Tester", true);
        assert!(is_muted(Some(key), "whatever they wear now"));
        assert!(muted_keys().iter().any(|muted| muted == key));
        assert!(revision() > before);
        set_muted(Some(key), "", false);
        assert!(!is_muted(Some(key), "Tester"));
        assert!(!muted_keys().iter().any(|muted| muted == key));
    }
}
