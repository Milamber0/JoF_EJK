//! The players muted on this PC (`docs/hub-chat.md`, "Muting a player"): who they
//! are, kept in `chat-mutes.txt` beside `config.cfg`, and which game slots that names
//! on the server the player is on. Muting is local: nothing is sent anywhere.
//!
//! A mute names an SJK player by their key (from an SJK chat line or the hub's claim on
//! a slot) and keeps the name they were last seen with; a player the hub does not know
//! is muted by name alone. A slot is matched as the scoreboard's badges are
//! (`docs/identity.md`): the hub's claim on it, made under the name the game shows
//! there. A slot no claim covers is matched by name, colour codes, case and symbols
//! ignored ([`sjk_identity::names_match`]), which is weaker: anyone wearing the name
//! is muted, and the card says so.

use sjk_identity::names_match;
use std::io::{self, Read, Write};
use std::path::Path;

/// The list's file, beside `config.cfg`.
pub(crate) const FILE: &str = "chat-mutes.txt";
/// The most players the list keeps.
const MAX_MUTES: usize = 1024;
/// The longest name kept, in bytes.
const MAX_NAME_BYTES: usize = 256;
/// The file's largest size: every line full.
const MAX_FILE_BYTES: u64 = (MAX_MUTES * (MAX_NAME_BYTES + 20)) as u64;
/// Game slots of the legacy protocol.
pub(crate) const SLOTS: usize = 32;

/// One muted player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Mute {
    /// Their SJK key, when they have one.
    pub(crate) key_id: Option<String>,
    /// The name they were last seen with, colour codes kept.
    pub(crate) name: String,
}

/// Every muted player, in the order they were muted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MuteList {
    entries: Vec<Mute>,
}

/// A key as the hub writes it: 16 lower-case hex digits.
fn valid_key(key: &str) -> bool {
    key.len() == 16
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// `name` as the list keeps it: no control characters (they would break its line),
/// at most [`MAX_NAME_BYTES`].
fn kept_name(name: &str) -> String {
    let mut kept = String::with_capacity(name.len().min(MAX_NAME_BYTES));
    for c in name.chars().filter(|c| !c.is_control()) {
        if kept.len() + c.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        kept.push(c);
    }
    kept
}

impl MuteList {
    /// An empty list.
    pub(crate) const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn entries(&self) -> &[Mute] {
        &self.entries
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether the key `key_id` is muted.
    pub(crate) fn contains_key(&self, key_id: &str) -> bool {
        self.entries
            .iter()
            .any(|mute| mute.key_id.as_deref() == Some(key_id))
    }

    /// Whether the player with key `key_id` (by key), or without one named `name`
    /// (by name), is muted.
    pub(crate) fn contains(&self, key_id: Option<&str>, name: &str) -> bool {
        match key_id {
            Some(key) => self.contains_key(key),
            None => self
                .entries
                .iter()
                .any(|mute| mute.key_id.is_none() && names_match(&mute.name, name)),
        }
    }

    /// The muted key or name `name` matches, colour codes ignored: who a slot no claim
    /// covers is taken to be.
    pub(crate) fn matches_name(&self, name: &str) -> bool {
        self.entries
            .iter()
            .any(|mute| names_match(&mute.name, name))
    }

    /// Mute or unmute the player with key `key_id` (or, without one, named `name`).
    /// Muting a key again keeps its newest name. Returns whether the list changed.
    pub(crate) fn set(&mut self, key_id: Option<&str>, name: &str, muted: bool) -> bool {
        let key_id = key_id.filter(|key| valid_key(key));
        let name = kept_name(name);
        let at = self.entries.iter().position(|mute| match key_id {
            Some(key) => mute.key_id.as_deref() == Some(key),
            None => mute.key_id.is_none() && names_match(&mute.name, &name),
        });
        match (at, muted) {
            (Some(at), true) => {
                let changed = self.entries[at].name != name && !name.is_empty();
                if changed {
                    self.entries[at].name = name;
                }
                changed
            }
            (Some(at), false) => {
                self.entries.remove(at);
                true
            }
            (None, true) => {
                // A mute by name alone needs a name something can match.
                let unnamed = key_id.is_none() && !names_match(&name, &name);
                if self.entries.len() >= MAX_MUTES || unnamed {
                    return false;
                }
                self.entries.push(Mute {
                    key_id: key_id.map(str::to_owned),
                    name,
                });
                true
            }
            (None, false) => false,
        }
    }

    /// Read the list's text: one player a line, their key (or `-`), a tab and their
    /// name. Lines that are not so are left out.
    pub(crate) fn parse(text: &str) -> Self {
        let mut list = Self::default();
        for line in text.lines() {
            let Some((key, name)) = line.split_once('\t') else {
                continue;
            };
            let key = (key != "-").then_some(key);
            if key.is_some_and(|key| !valid_key(key)) {
                continue;
            }
            let _ = list.set(key, name, true);
        }
        list
    }

    /// The list's text, as [`Self::parse`] reads it.
    pub(crate) fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.entries.len() * 40);
        for mute in &self.entries {
            text.push_str(mute.key_id.as_deref().unwrap_or("-"));
            text.push('\t');
            text.push_str(&mute.name);
            text.push('\n');
        }
        text
    }
}

/// Read the list from `path`; a missing file is an empty list.
pub(crate) fn load(path: &Path) -> io::Result<MuteList> {
    let mut bytes = Vec::new();
    match std::fs::File::open(path) {
        Ok(file) => {
            // Read as bytes: a hand edit or the size limit cutting a name can leave
            // text that is not UTF-8, and refusing it would have the next mute save
            // over the whole list.
            file.take(MAX_FILE_BYTES).read_to_end(&mut bytes)?;
            Ok(MuteList::parse(&String::from_utf8_lossy(&bytes)))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(MuteList::default()),
        Err(error) => Err(error),
    }
}

/// Write the list to `path` through a temporary file, so a failed write leaves the
/// old one whole.
pub(crate) fn save(path: &Path, list: &MuteList) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pending = path.with_extension("txt.pending");
    let result = (|| {
        let mut file = std::fs::File::create(&pending)?;
        file.write_all(list.to_text().as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&pending, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result
}

/// A claim the hub holds for a slot of this server: the name it was made under and
/// the key that made it (`sjk_identity::Presence`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Claim<'a> {
    pub(crate) slot: u8,
    pub(crate) claimed_name: &'a str,
    pub(crate) key_id: &'a str,
}

/// Which slots of the server the list names, as bits (slot `n` is bit `n`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Slots {
    /// Every muted slot.
    pub(crate) muted: u32,
    /// Those matched by name alone: no claim at the hub says who is there.
    pub(crate) by_name: u32,
}

impl Slots {
    /// Whether slot `slot` is muted.
    pub(crate) fn contains(self, slot: usize) -> bool {
        slot < SLOTS && self.muted & (1 << slot) != 0
    }
}

/// The slots whose claim names what the game shows there, as bits: those the hub says
/// whose they are.
fn claimed(claims: &[Claim<'_>], shown: &[&str; SLOTS]) -> u32 {
    claims
        .iter()
        .filter(|claim| trusted(claim, shown))
        .fold(0, |bits, claim| bits | 1 << claim.slot)
}

/// Whether `claim` names what the game shows in its slot (the badges' rule).
fn trusted(claim: &Claim<'_>, shown: &[&str; SLOTS]) -> bool {
    let slot = usize::from(claim.slot);
    slot < SLOTS && names_match(claim.claimed_name, shown[slot])
}

/// Where the player with key `key_id` (or, without one, named `name`) is: the slot
/// their claim names, else (`true`: by name) the first slot no claim covers that shows
/// their name; `None` when neither.
pub(crate) fn locate(
    key_id: Option<&str>,
    name: &str,
    claims: &[Claim<'_>],
    shown: &[&str; SLOTS],
) -> Option<(usize, bool)> {
    let claimed = claimed(claims, shown);
    if let Some(key) = key_id
        && let Some(claim) = claims
            .iter()
            .find(|claim| claim.key_id == key && trusted(claim, shown))
    {
        return Some((usize::from(claim.slot), false));
    }
    shown
        .iter()
        .enumerate()
        .find(|(slot, shown)| claimed & (1 << slot) == 0 && names_match(name, shown))
        .map(|(slot, _)| (slot, true))
}

/// The slots `list` names, with the game showing `shown[n]` in slot `n` (empty for a
/// free slot) and the hub holding `claims`; `own` is the player's own slot, never
/// muted. A slot whose claim names what the game shows there belongs to that key; any
/// other slot is matched by name.
pub(crate) fn slots(
    list: &MuteList,
    claims: &[Claim<'_>],
    shown: &[&str; SLOTS],
    own: Option<usize>,
) -> Slots {
    let mut found = Slots::default();
    if list.is_empty() {
        return found;
    }
    let claimed = claimed(claims, shown);
    for claim in claims {
        if trusted(claim, shown) && list.contains_key(claim.key_id) {
            found.muted |= 1 << claim.slot;
        }
    }
    for (slot, name) in shown.iter().enumerate() {
        if claimed & (1 << slot) != 0 || name.is_empty() {
            continue;
        }
        if list.matches_name(name) {
            found.muted |= 1 << slot;
            found.by_name |= 1 << slot;
        }
    }
    if let Some(own) = own.filter(|own| *own < SLOTS) {
        found.muted &= !(1 << own);
        found.by_name &= !(1 << own);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOL: &str = "0123456789abcdef";
    const FOX: &str = "fedcba9876543210";

    /// A list with a byte that is not UTF-8 (a hand edit, a name cut by the size
    /// limit) loses that one name's bytes, not the other mutes.
    #[test]
    fn a_list_that_is_not_utf8_still_loads() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        let mut bytes = format!("{SOL}\tSol\n-\tR\u{e9}mi\n").into_bytes();
        bytes.extend_from_slice(b"-\tBad\xffName\n");
        bytes.extend_from_slice(format!("{FOX}\tFox\n").as_bytes());
        std::fs::write(&path, bytes).unwrap();
        let list = load(&path).expect("read as bytes");
        assert!(list.contains(Some(SOL), "Sol"));
        assert!(list.contains(Some(FOX), "Fox"));
        assert_eq!(list.entries().len(), 4);
    }

    #[test]
    fn players_are_muted_and_unmuted_by_key_or_by_name() {
        let mut list = MuteList::default();
        assert!(list.set(Some(SOL), "^2Sol", true));
        assert!(!list.set(Some(SOL), "^2Sol", true), "already muted");
        assert!(list.contains(Some(SOL), "anything"));
        assert!(
            !list.contains(Some(FOX), "^2Sol"),
            "a key is matched by key"
        );
        // A player the hub does not know, by name: colour codes and case ignored.
        assert!(list.set(None, "^1Dark^7Lord", true));
        assert!(list.contains(None, "darklord"));
        assert!(list.set(None, "DARKLORD", false));
        assert!(!list.contains(None, "^1Dark^7Lord"));
        // A newer name for a muted key is kept.
        assert!(list.set(Some(SOL), "^5Sol", true));
        assert_eq!(list.entries()[0].name, "^5Sol");
        assert!(list.set(Some(SOL), "", false));
        assert!(list.is_empty());
        // Nothing to match: no mute.
        assert!(!list.set(None, "^1^2", true));
    }

    #[test]
    fn the_list_survives_its_file() {
        let mut list = MuteList::default();
        list.set(Some(SOL), "^2Sol\tthe\nFox", true);
        list.set(None, "Troll", true);
        let text = list.to_text();
        assert_eq!(text, format!("{SOL}\t^2SoltheFox\n-\tTroll\n"));
        assert_eq!(MuteList::parse(&text), list);
        // Lines that are not a mute are left out.
        let read = MuteList::parse("nonsense\nshort\tName\n-\tKept\n");
        assert_eq!(read.entries().len(), 1);
        let directory = std::env::temp_dir().join(format!("sjk-mutes-{}", std::process::id()));
        let path = directory.join(FILE);
        assert_eq!(load(&path).expect("no file"), MuteList::default());
        save(&path, &list).expect("saved");
        assert_eq!(load(&path).expect("read back"), list);
        let _ = std::fs::remove_dir_all(directory);
    }

    fn shown(names: &[(usize, &'static str)]) -> [&'static str; SLOTS] {
        let mut shown = [""; SLOTS];
        for (slot, name) in names {
            shown[*slot] = name;
        }
        shown
    }

    #[test]
    fn a_claim_names_the_slot_as_the_badges_do() {
        let mut list = MuteList::default();
        list.set(Some(SOL), "Sol", true);
        let claims = [Claim {
            slot: 3,
            claimed_name: "^2Sol",
            key_id: SOL,
        }];
        let found = slots(
            &list,
            &claims,
            &shown(&[(3, "^5S^2ol"), (4, "Fox")]),
            Some(0),
        );
        assert_eq!(found.muted, 1 << 3);
        assert_eq!(found.by_name, 0, "matched by the claim");
        // A claim for another name in the slot is somebody else's, and the name
        // shown there is not the muted one either.
        let found = slots(&list, &claims, &shown(&[(3, "Imposter")]), Some(0));
        assert_eq!(found.muted, 0);
    }

    #[test]
    fn without_a_claim_the_name_matches_whatever_its_colours() {
        let mut list = MuteList::default();
        list.set(Some(SOL), "^2Sol", true);
        list.set(None, "Troll", true);
        let names = shown(&[(1, "^1S^3o^5l"), (2, "^7TROLL"), (5, "Solo")]);
        let found = slots(&list, &[], &names, None);
        assert_eq!(found.muted, 1 << 1 | 1 << 2);
        assert_eq!(found.by_name, 1 << 1 | 1 << 2);
        assert!(found.contains(1) && found.contains(2) && !found.contains(5));
    }

    #[test]
    fn another_keys_claim_keeps_its_slot_from_a_name_match() {
        let mut list = MuteList::default();
        list.set(Some(SOL), "Sol", true);
        // The hub says slot 2 is Fox's key, though Fox wears "Sol" now.
        let claims = [Claim {
            slot: 2,
            claimed_name: "Sol",
            key_id: FOX,
        }];
        let found = slots(&list, &claims, &shown(&[(2, "Sol"), (6, "Sol")]), None);
        assert_eq!(found.muted, 1 << 6);
    }

    #[test]
    fn a_player_is_located_by_claim_then_by_name() {
        let claims = [Claim {
            slot: 3,
            claimed_name: "Sol",
            key_id: SOL,
        }];
        let names = shown(&[(3, "^2Sol"), (7, "^1Fox")]);
        assert_eq!(
            locate(Some(SOL), "anything", &claims, &names),
            Some((3, false))
        );
        assert_eq!(locate(Some(FOX), "^7fox", &claims, &names), Some((7, true)));
        assert_eq!(
            locate(None, "Sol", &claims, &names),
            None,
            "slot 3 is claimed"
        );
        assert_eq!(locate(None, "Kyle", &claims, &names), None);
    }

    #[test]
    fn the_players_own_slot_is_never_muted() {
        let mut list = MuteList::default();
        list.set(None, "Sol", true);
        let found = slots(&list, &[], &shown(&[(0, "Sol"), (1, "Sol")]), Some(0));
        assert_eq!(found.muted, 1 << 1);
        let none = slots(&MuteList::default(), &[], &shown(&[(1, "Sol")]), None);
        assert_eq!(none, Slots::default());
    }
}
