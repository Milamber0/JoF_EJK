//! The hub's wire format (`PROTOCOL.md` in Sol-Vulpes/SJK-hub): signed
//! requests, the JSON the hub answers with, and how a claim's name is matched
//! to the name the game shows.

use crate::keys::Identity;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A player's public profile as the hub serves it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Profile {
    /// First 16 hex digits of the SHA-256 of the public key.
    pub key_id: String,
    /// The public key, unpadded base64url.
    pub key: String,
    /// Display name: the name the operator gave the key, else the in-game name it
    /// last wore; empty while the hub has seen neither.
    pub name: String,
    /// Free text the player wrote about themselves.
    pub bio: String,
    /// Whether the hub's operator vouches for this key.
    pub verified: bool,
    /// Whether the hub's operator made this key staff (absent from older hubs).
    #[serde(default)]
    pub staff: bool,
    /// Registration time, unix seconds.
    pub created: i64,
    /// In-game names the key has worn, most recent first (absent from hubs older
    /// than the name history).
    #[serde(default)]
    pub names: Vec<WornName>,
    /// Medals the SJK team gave the key, in the hub's catalogue order (absent from
    /// hubs older than medals).
    #[serde(default)]
    pub medals: Vec<Medal>,
    /// Achievements with a count above 0, in the hub's catalogue order (absent from
    /// hubs older than achievements).
    #[serde(default)]
    pub achievements: Vec<Achievement>,
    /// Unlockables the key holds, in the hub's catalogue order (absent from hubs
    /// older than unlocks).
    #[serde(default)]
    pub unlocks: Vec<Unlock>,
    /// The version of the key's picture (`crate::avatar`), empty for none (and from hubs
    /// older than pictures).
    #[serde(default)]
    pub avatar: String,
    /// How many holocrons of each tier the key has earned (absent from hubs older
    /// than holocrons, which count as none).
    #[serde(default)]
    pub holocron_counts: HolocronCounts,
    /// The key's recent holocrons, newest first, at most [`HOLOCRONS_LISTED`] (absent
    /// from hubs older than holocrons).
    #[serde(default)]
    pub holocrons: Vec<Holocron>,
    /// The person's credits (`PROTOCOL.md`, "Holocrons", "Opening"): what opening
    /// holocrons brought, never below 0 (absent from hubs older than opening).
    #[serde(default)]
    pub credits: u64,
    /// The person's cards, one entry per kind of copy held, the best first (absent from
    /// hubs older than opening).
    #[serde(default)]
    pub cards: Vec<CardStack>,
    /// The person's keys: the main key first, then the keys linked to it (absent from
    /// hubs older than linked keys). A linked key signs as its person, so the hub
    /// answers the person's [`Profile::key_id`] for it everywhere.
    #[serde(default)]
    pub keys: Vec<String>,
}

impl Profile {
    /// The keys linked to the person's main key: every one of [`Profile::keys`] but the
    /// main key (the first, which is also [`Profile::key_id`]).
    pub fn linked_keys(&self) -> impl Iterator<Item = &str> {
        self.keys
            .iter()
            .map(String::as_str)
            .filter(move |key| *key != self.key_id)
    }

    /// Whether `key_id` is this person's: their main key or one linked to it.
    pub fn has_key(&self, key_id: &str) -> bool {
        self.key_id == key_id || self.keys.iter().any(|key| key == key_id)
    }
}

/// The most holocrons a profile lists, as the hub caps them.
pub const HOLOCRONS_LISTED: usize = 100;

/// How many holocrons of each tier a key holds unopened (`PROTOCOL.md`, "Holocrons"):
/// what it can still open. All four are always present on the wire; a count the client
/// does not know is none.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct HolocronCounts {
    /// Uncommon holocrons.
    #[serde(default)]
    pub uncommon: u32,
    /// Rare holocrons.
    #[serde(default)]
    pub rare: u32,
    /// Legendary holocrons.
    #[serde(default)]
    pub legendary: u32,
    /// Mythical holocrons.
    #[serde(default)]
    pub mythical: u32,
}

impl HolocronCounts {
    /// The count of the tier whose wire id is `tier` (`uncommon`, `rare`, `legendary`,
    /// `mythical`); 0 for an id this client does not know.
    pub fn of(&self, tier: &str) -> u32 {
        match tier {
            "uncommon" => self.uncommon,
            "rare" => self.rare,
            "legendary" => self.legendary,
            "mythical" => self.mythical,
            _ => 0,
        }
    }

    /// Every holocron the key has earned.
    pub fn total(&self) -> u32 {
        self.uncommon
            .saturating_add(self.rare)
            .saturating_add(self.legendary)
            .saturating_add(self.mythical)
    }
}

/// One holocron a key earned by playing, or that staff gave. Opening it
/// ([`crate::Hub::open_holocrons`]) brings a card and credits; an opened one stays
/// listed, with [`Holocron::opened`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Holocron {
    /// Its number at the hub, which only rises: a client remembers the highest it has
    /// shown.
    pub id: u64,
    /// Its tier's wire id; a client shows only the ids it knows.
    pub tier: String,
    /// When it dropped, unix seconds.
    #[serde(default)]
    pub dropped: i64,
    /// `play`, or `staff` for a gift.
    #[serde(default)]
    pub source: String,
    /// Public plain text from the team, only on a gift; often empty. Never markup.
    #[serde(default)]
    pub note: String,
    /// When it was opened, unix seconds; `None` while it can still be opened (and from
    /// hubs older than opening).
    #[serde(default)]
    pub opened: Option<i64>,
}

impl Holocron {
    /// Whether it can still be opened.
    pub fn is_sealed(&self) -> bool {
        self.opened.is_none()
    }
}

/// The most holocrons one opening takes (`POST /v1/holocrons/open`).
pub const OPEN_MAX: usize = 20;

/// Copies of one kind a person holds (`PROTOCOL.md`, "Holocrons", "Opening"): a card
/// (or a sealed one), its rarity, finish and edition, and how many.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CardStack {
    /// The card's id in the catalogue; `None` for a sealed card, whose face is chosen
    /// when it is revealed.
    #[serde(default)]
    pub card: Option<String>,
    /// `common`, `uncommon`, `rare` or `legendary`; a client shows only the ids it knows.
    pub rarity: String,
    /// `normal`, `holo` or `solar`.
    pub finish: String,
    /// The era it was opened in (`alpha`).
    #[serde(default)]
    pub edition: String,
    /// How many such copies.
    #[serde(default)]
    pub count: u32,
    /// When the first of them was obtained, unix seconds.
    #[serde(default)]
    pub first: i64,
    /// When the newest of them was obtained, unix seconds.
    #[serde(default)]
    pub last: i64,
}

/// The copy one opening brought.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct OpenedCard {
    /// The copy's number at the hub.
    pub id: u64,
    /// The card it is, picked from the hub's catalogue; `None` while sealed (no card of
    /// its rarity yet, or a hub from before the catalogue).
    #[serde(default)]
    pub card: Option<String>,
    /// The card's rarity (as [`CardStack::rarity`]).
    pub rarity: String,
    /// The copy's finish (as [`CardStack::finish`]).
    pub finish: String,
    /// The copy's edition (as [`CardStack::edition`]).
    #[serde(default)]
    pub edition: String,
}

/// What opening one holocron brought.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Opened {
    /// The holocron opened.
    pub holocron: u64,
    /// Its tier's wire id.
    pub tier: String,
    /// The (sealed) card it held; `None` when it held a shader instead.
    #[serde(default)]
    pub card: Option<OpenedCard>,
    /// The credits it held.
    #[serde(default)]
    pub credits: u64,
    /// The shader it held instead of a card (14/10/2026).
    #[serde(default)]
    pub unlock: Option<OpenedUnlock>,
}

/// A shader an opening brought.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct OpenedUnlock {
    /// The unlockable's id (`saber_tidal`).
    pub id: String,
    /// Its name ("Tidal blade").
    #[serde(default)]
    pub name: String,
    /// How many of it the key holds now, this one included.
    #[serde(default = "one")]
    pub count: u32,
}

/// A sealed copy the hub revealed (`POST /v1/cards/reveal`): the card it turned out to be.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Revealed {
    /// The copy's number at the hub.
    pub id: u64,
    /// The card it is now.
    pub card: String,
    /// Its rarity, as it was sealed.
    pub rarity: String,
    /// Its finish, as it was sealed.
    pub finish: String,
    /// Its edition, as it was sealed.
    #[serde(default)]
    pub edition: String,
}

/// The hub's answer to `POST /v1/cards/reveal`: the copies revealed (none when no sealed
/// copy has a card of its rarity yet) and the person's profile after.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct RevealAnswer {
    /// The copies revealed, oldest first.
    pub revealed: Vec<Revealed>,
    /// The person's profile after the reveal.
    pub profile: Profile,
}

/// The hub's answer to `POST /v1/holocrons/open`: what each holocron brought, in the
/// order asked, and the person's profile after.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct OpenAnswer {
    /// What each holocron brought.
    pub opened: Vec<Opened>,
    /// The person's profile after the opening.
    pub profile: Profile,
}

/// The hub's answer to `GET /v1/holocrons` (`PROTOCOL.md`, "Holocrons"): how far the
/// own key is from its next holocron, and the odds the hub rolls with.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HolocronState {
    /// Seconds of active play carried toward the next holocron.
    pub progress_secs: u64,
    /// Seconds of active play for one holocron.
    pub every_secs: u64,
    /// Holocrons dropped in the last 24 hours.
    #[serde(default)]
    pub today: u32,
    /// The most holocrons a key can earn in 24 hours.
    #[serde(default)]
    pub daily_cap: u32,
    /// The hub's clock, unix seconds.
    #[serde(default)]
    pub server_time: i64,
    /// Whether the person may open holocrons now: only staff while opening is tested.
    /// Hubs before it leave it out, read as `false`.
    #[serde(default)]
    pub can_open: bool,
    /// Every odds table the hub rolls with, one per holocron tier in the tiers' order
    /// (14/10/2026); empty from hubs before.
    #[serde(default)]
    pub loot: Vec<LootTier>,
}

/// One holocron tier's odds as the hub rolls them (`PROTOCOL.md`, "Holocrons"), every
/// share per mille.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct LootTier {
    /// The tier's wire id.
    pub tier: String,
    /// How often a drop is this tier.
    pub drop: u32,
    /// The card's rarity (common to legendary).
    pub rarity: Vec<u32>,
    /// The card's finish (normal, holo, solar).
    pub finish: Vec<u32>,
    /// The credits a card opening gives: the least and the most.
    pub credits: [u64; 2],
    /// How often an opening brings a saber shader instead of the card and credits.
    #[serde(default)]
    pub shader: u32,
    /// The shader's tier then (common to mythical).
    #[serde(default)]
    pub shader_tier: Vec<u32>,
}

/// A holocron dropping for a key, as the feed relays it (`PROTOCOL.md`, "The feed").
/// Legendary and mythical drops reach every reader; uncommon and rare ones only the
/// reader whose key it is.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DropEvent {
    /// Its id in the feed's sequence.
    pub id: u64,
    /// When it dropped, unix seconds.
    pub at: i64,
    /// The key it dropped for.
    pub key_id: String,
    /// That key's hub display name, with colour codes. Show it only through
    /// [`crate::chat::for_display`].
    pub name: String,
    /// The tier's wire id.
    pub tier: String,
    /// Whether the hub's operator vouches for the key.
    #[serde(default)]
    pub verified: bool,
}

/// One unlockable a key holds (`PROTOCOL.md`, "Unlocks"): granted by the hub's
/// operator or staff, or brought by a medal the key holds. A client shows and wears only the ids it knows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Unlock {
    /// The unlockable's id (`saber_sun`).
    pub id: String,
    /// When it was granted, unix seconds.
    #[serde(default)]
    pub granted: i64,
    /// Public plain text from the team, often empty. Never markup.
    #[serde(default)]
    pub note: String,
    /// How many the key holds (14/10/2026: a holocron may drop the same shader again);
    /// 1 from hubs before.
    #[serde(default = "one")]
    pub count: u32,
    /// The medal that brings it (`bug_hunter`), when the key holds it only through that
    /// medal: it goes with the medal. `None` for a grant by hand, and from hubs before
    /// 10/10/2026.
    #[serde(default)]
    pub medal: Option<String>,
}

/// What a player wears that others draw (`PROTOCOL.md`, "Looks"). The default is
/// the stock blade with the holocron out, which is also what a claim without a
/// look means.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct Look {
    /// A blade-skin unlock id, or empty for the stock blade.
    #[serde(default)]
    pub saber: String,
    /// Whether the Illuminate holocron is lit.
    #[serde(default)]
    pub illuminate: bool,
    /// The blade skin's options its wearer switched off (since 11/10/2026; absent, and
    /// empty, from older hubs and from wearers with every part on).
    #[serde(default)]
    pub saber_off: Vec<String>,
}

/// A look worn on a game server, as the feed relays it when it changes: the slot and
/// name come from the wearer's claim, as an emote's do.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct LookEvent {
    /// Its id in the feed's sequence.
    pub id: u64,
    /// When the hub took it, unix seconds.
    pub at: i64,
    /// The wearer's slot on the server, from their claim.
    pub slot: u8,
    /// The name the wearer claimed the game shows there.
    pub claimed_name: String,
    /// The wearer's key id.
    pub key_id: String,
    /// The blade-skin unlock id, or empty for the stock blade.
    #[serde(default)]
    pub saber: String,
    /// Whether the Illuminate holocron is lit.
    #[serde(default)]
    pub illuminate: bool,
    /// The blade skin's options its wearer switched off; empty when absent.
    #[serde(default)]
    pub saber_off: Vec<String>,
}

impl LookEvent {
    /// The look it carries.
    pub fn look(&self) -> Look {
        Look {
            saber: self.saber.clone(),
            illuminate: self.illuminate,
            saber_off: self.saber_off.clone(),
        }
    }
}

/// A milestone of the player's own play (`PROTOCOL.md`, "Achievements"): how far its
/// count is and when it was unlocked. Recognition only, it grants nothing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Achievement {
    /// The achievement's id (`first_blood`, `kills_100`, ...).
    pub id: String,
    /// The count the hub took, at most `goal`.
    pub progress: u64,
    /// The count that unlocks it.
    #[serde(default)]
    pub goal: u64,
    /// When the count first reached the goal, unix seconds; 0 while it has not.
    #[serde(default)]
    pub unlocked: i64,
}

/// The hub's answer to `POST /v1/staff/search`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Players {
    /// The players found, most recently seen first.
    pub players: Vec<Profile>,
}

/// The hub's answer to `PUT /v1/achievements`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Achievements {
    /// The key's achievements, as a profile lists them.
    pub achievements: Vec<Achievement>,
}

/// One medal the SJK team gave a key: recognition, and some bring a saber blade (an
/// [`Unlock`] with `medal`) while held.
///
/// A profile carries every field; a presence entry only `id` and `count`, the rest
/// keeping their defaults. A client shows only the ids it knows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Medal {
    /// The medal's id (`early_tester`, `bug_hunter`, ...).
    pub id: String,
    /// How many times it was given: 1 unless the medal is repeatable.
    #[serde(default = "one")]
    pub count: u32,
    /// When it was last given, unix seconds; 0 when not sent (presence).
    #[serde(default)]
    pub awarded: i64,
    /// Public plain text from the team about why, often empty. Never markup.
    #[serde(default)]
    pub note: String,
}

fn one() -> u32 {
    1
}

/// One in-game name a key has worn, as the hub saw it in registrations and claims.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct WornName {
    /// The name with its colour codes.
    pub name: String,
    /// When the hub first saw it, unix seconds.
    pub first_seen: i64,
    /// When the hub last saw it, unix seconds.
    pub last_seen: i64,
}

/// One live claim on a game server and who made it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Presence {
    /// The game slot the claimant says they hold.
    pub slot: u8,
    /// The name the claimant says the game shows for them.
    pub claimed_name: String,
    /// The claimant's key id.
    pub key_id: String,
    /// The claimant's display name.
    pub name: String,
    /// Whether the hub's operator vouches for the claimant.
    pub verified: bool,
    /// The claimant's medals, ids and counts only (absent from hubs older than
    /// medals), so the scoreboard shows them without a request per player.
    #[serde(default)]
    pub medals: Vec<Medal>,
    /// The version of the claimant's picture, empty for none (and from hubs older than
    /// pictures), so a client fetches it without a profile per player.
    #[serde(default)]
    pub avatar: String,
    /// What the claimant wears, when their claim has a look (absent otherwise, and
    /// from hubs older than looks).
    #[serde(default)]
    pub look: Option<Look>,
}

/// One SJK chat message (`PROTOCOL.md`, "Chat"), as the feed carries it. Show its
/// `name` and `text` only through [`crate::chat::for_display`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ChatMessage {
    /// Its id in the feed's sequence.
    pub id: u64,
    /// When the hub took it, unix seconds.
    pub at: i64,
    /// The sender's key id.
    pub key_id: String,
    /// The in-game name the sender wore, else their hub name.
    pub name: String,
    /// Whether the hub's operator vouches for the sender.
    #[serde(default)]
    pub verified: bool,
    /// Whether the sender is SJK staff.
    #[serde(default)]
    pub staff: bool,
    /// What they said.
    pub text: String,
    /// Set when this "message" is a holocron drop the feed relayed (never read from
    /// the wire): `text` is empty and the viewer words the line.
    #[serde(skip)]
    pub holocron: Option<DropMark>,
}

/// What marks a [`ChatMessage`] as a holocron drop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DropMark {
    /// The tier's wire id.
    pub tier: String,
    /// The drop is the reader's own.
    pub own: bool,
}

/// One emote on a game server (`PROTOCOL.md`, "Emotes"): the slot and name come from
/// the sender's claim, so they say who the hub believes is in that slot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Emote {
    /// Its id in the feed's sequence.
    pub id: u64,
    /// When the hub took it, unix seconds.
    pub at: i64,
    /// The sender's slot on the server, from their claim.
    pub slot: u8,
    /// The name the sender claimed the game shows there.
    pub claimed_name: String,
    /// The sender's key id.
    pub key_id: String,
    /// The emote's id (`wave`); a client plays the ids it has.
    pub emote: String,
    /// The slot whose emote the sender joined in step (`sjkemote sync`), if any;
    /// absent from hubs before 15/10/2026.
    #[serde(default)]
    pub sync: Option<u8>,
}

/// One ping on a game server (`PROTOCOL.md`, "Pings"): a point a player marked in the
/// world. The slot and name come from the sender's claim; `scope` is the sender's own
/// word for who should see it, which the reader checks against what its game shows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Ping {
    /// Its id in the feed's sequence.
    pub id: u64,
    /// When the hub took it, unix seconds.
    pub at: i64,
    /// The sender's slot on the server, from their claim.
    pub slot: u8,
    /// The name the sender claimed the game shows there.
    pub claimed_name: String,
    /// The sender's key id.
    pub key_id: String,
    /// The point, in whole game units.
    pub pos: [i32; 3],
    /// Who the sender meant it for (`all`, `red`, `blue`, `lone`, `double`, `spec`).
    pub scope: String,
}

/// The values a player shares on their nameplate (`PROTOCOL.md`, "Vitals"): their
/// game's own numbers, as the server sends them to that player.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct Vitals {
    /// The game server's time (milliseconds) of the snapshot they come from.
    pub t: i32,
    /// Health (`STAT_HEALTH`).
    pub health: i32,
    /// Shield (`STAT_ARMOR`).
    pub armor: i32,
    /// Force (`fd.forcePower`).
    pub force: i32,
    /// `STAT_MAX_HEALTH`.
    pub max_health: i32,
    /// `fd.forcePowerMax`.
    pub max_force: i32,
}

/// One player in the hub's answer to `POST /v1/vitals`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SharedVitals {
    /// Their slot on the server, from their claim.
    pub slot: u8,
    /// The name they claimed the game shows there.
    pub claimed_name: String,
    /// Their id at the hub.
    pub key_id: String,
    /// They share their values.
    pub share: bool,
    /// They see everyone's estimated bars (which makes them share).
    pub estimates: bool,
    /// Seconds since the hub took the entry.
    #[serde(default)]
    pub age: i64,
    /// Their values; `None` while they have none (dead, spectating).
    #[serde(default)]
    pub vitals: Option<Vitals>,
}

/// The hub's answer to `GET /v1/feed` (`PROTOCOL.md`, "The feed").
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct Feed {
    /// The `after` of the next request.
    pub next: u64,
    /// New messages, oldest first.
    #[serde(default)]
    pub chat: Vec<ChatMessage>,
    /// New emotes on the server asked about.
    #[serde(default)]
    pub emotes: Vec<Emote>,
    /// New pings on the server asked about (absent from hubs older than pings).
    #[serde(default)]
    pub pings: Vec<Ping>,
    /// New looks on the server asked about (absent from hubs older than looks).
    #[serde(default)]
    pub looks: Vec<LookEvent>,
    /// New holocron drops this reader is told of (absent from hubs older than
    /// holocrons).
    #[serde(default)]
    pub drops: Vec<DropEvent>,
    /// Messages staff deleted.
    #[serde(default)]
    pub deleted: Vec<u64>,
    /// Keys that read the feed in the last minute.
    #[serde(default)]
    pub online: u32,
    /// Who reads the chat now and who read it last (absent from hubs that do not
    /// list them, and from answers the hub leaves them out of).
    #[serde(default)]
    pub people: Option<People>,
}

/// The feed's `people` (`PROTOCOL.md`, "The feed"): the keys reading the feed now and
/// the ones that read it most recently.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct People {
    /// Keys that read the feed in the last minute, the same ones `online` counts.
    #[serde(default)]
    pub online: Vec<Person>,
    /// Keys that read it before that, most recently seen first.
    #[serde(default)]
    pub recent: Vec<Person>,
}

/// One SJK player of [`People`].
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct Person {
    /// Their key id.
    pub key_id: String,
    /// Their display name, as the hub's profiles say.
    #[serde(default)]
    pub name: String,
    /// Whether the hub's operator vouches for them.
    #[serde(default)]
    pub verified: bool,
    /// Whether they are SJK staff.
    #[serde(default)]
    pub staff: bool,
    /// Their picture's version, `""` for none.
    #[serde(default)]
    pub avatar: String,
    /// When they last read the feed, unix seconds.
    #[serde(default)]
    pub seen: i64,
    /// They hold a live claim marked active: they are playing a match now.
    #[serde(default)]
    pub playing: bool,
}

/// The text a request's signature covers.
pub fn signed_text(
    method: &str,
    path_and_query: &str,
    ts: i64,
    nonce: &str,
    body: &[u8],
) -> String {
    let hex: String = Sha256::digest(body)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("SJK-HUB-v1\n{method}\n{path_and_query}\n{ts}\n{nonce}\n{hex}")
}

/// The `Authorization` header value for a request signed by `identity`.
pub fn authorization(
    identity: &Identity,
    method: &str,
    path_and_query: &str,
    body: &[u8],
    ts: i64,
    nonce_bytes: [u8; 12],
) -> String {
    let nonce = B64.encode(nonce_bytes);
    let signature = identity.sign_text(&signed_text(method, path_and_query, ts, &nonce, body));
    format!(
        "SJK-Sig key={}, ts={ts}, nonce={nonce}, sig={signature}",
        identity.public_key_text()
    )
}

/// The form two player names are compared in: lower case, Quake colour codes
/// removed, only letters and digits kept (the hub uses the same rule for
/// display names).
pub fn normal_form(name: &str) -> String {
    let mut out = String::new();
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' {
            // `Q_IsColorString`: `^` plus any character but another `^`.
            if chars.peek().is_some_and(|&next| next != '^') {
                let _ = chars.next();
            }
        } else if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Whether a claim's name and the name the game shows are one player's name.
///
/// The game server cleans names (length, spaces, duplicates), so the exact text
/// can differ; the normal forms must agree and must not be empty.
pub fn names_match(claimed: &str, shown: &str) -> bool {
    let claimed = normal_form(claimed);
    !claimed.is_empty() && claimed == normal_form(shown)
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_opening_may_hold_a_shader_instead_of_a_card() {
        let opened: Opened = serde_json::from_str(
            r#"{"holocron":8,"tier":"uncommon","credits":0,
                "unlock":{"id":"saber_tidal","name":"Tidal blade","count":2}}"#,
        )
        .unwrap();
        assert_eq!(opened.card, None);
        assert_eq!(
            opened.unlock,
            Some(OpenedUnlock {
                id: "saber_tidal".into(),
                name: "Tidal blade".into(),
                count: 2
            })
        );
        // A card opening has no shader, and an unlock without a count is one.
        let card: Opened = serde_json::from_str(
            r#"{"holocron":9,"tier":"rare","credits":40,
                "card":{"id":3,"card":null,"rarity":"common","finish":"normal","edition":"alpha"}}"#,
        )
        .unwrap();
        assert!(card.card.is_some() && card.unlock.is_none());
        let unlock: Unlock = serde_json::from_str(r#"{"id":"saber_sun"}"#).unwrap();
        assert_eq!(unlock.count, 1);
    }

    use super::*;

    /// A request every implementation of `PROTOCOL.md` must sign to this exact
    /// header; the hub's tests carry the same vector.
    #[test]
    fn the_known_answer_header_matches_the_protocol() {
        let identity = Identity::from_seed([7; 32]);
        assert_eq!(
            authorization(
                &identity,
                "POST",
                "/v1/claim",
                br#"{"server":"1.2.3.4:29070","slot":3,"name":"Sol"}"#,
                1_000_000,
                [1; 12]
            ),
            KNOWN_ANSWER
        );
    }

    const KNOWN_ANSWER: &str = "SJK-Sig key=6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw, ts=1000000, nonce=AQEBAQEBAQEBAQEB, sig=IKLlpqaSWIXqXK7Q77SsGJiE4BIdUTjkSWv2zyr0951WKByhEShSL-Ow70Rgc3znokl0Sf4rQK2VjNkPEm9uCQ";

    #[test]
    fn a_feed_answer_parses() {
        let feed: Feed = serde_json::from_str(
            r#"{"next":14,"chat":[{"id":12,"at":5,"key_id":"aa","name":"^2Sol",
                "verified":true,"staff":false,"text":"gg"}],
                "emotes":[{"id":13,"at":6,"slot":3,"claimed_name":"^2Sol","key_id":"aa",
                "emote":"wave"}],"deleted":[9],"online":7}"#,
        )
        .unwrap();
        assert_eq!(feed.next, 14);
        assert_eq!(feed.chat[0].text, "gg");
        assert!(feed.chat[0].verified);
        assert_eq!(
            (feed.emotes[0].slot, feed.emotes[0].emote.as_str()),
            (3, "wave")
        );
        assert_eq!((feed.deleted.as_slice(), feed.online), (&[9][..], 7));
        // Lists a hub leaves out are empty.
        let quiet: Feed = serde_json::from_str(r#"{"next":3}"#).unwrap();
        assert!(quiet.chat.is_empty() && quiet.emotes.is_empty() && quiet.deleted.is_empty());
        assert_eq!(quiet.people, None, "a hub that does not list them");
    }

    #[test]
    fn a_feed_answer_with_people_parses() {
        let feed: Feed = serde_json::from_str(
            r#"{"next":3,"online":2,"people":{"online":[{"key_id":"aa","name":"^2Sol",
                "verified":true,"staff":true,"avatar":"0123456789abcdef","seen":100,
                "playing":true},{"key_id":"bb","name":"Fox","seen":90}],
                "recent":[{"key_id":"cc","name":"Kyle","seen":40}]}}"#,
        )
        .unwrap();
        let people = feed.people.expect("people");
        assert_eq!(people.online.len(), 2);
        let sol = &people.online[0];
        assert!(sol.verified && sol.staff && sol.playing);
        assert_eq!((sol.avatar.as_str(), sol.seen), ("0123456789abcdef", 100));
        // Fields a hub leaves out are off or empty.
        let fox = &people.online[1];
        assert!(!fox.verified && !fox.playing && fox.avatar.is_empty());
        assert_eq!(people.recent[0].name, "Kyle");
        let empty: Feed = serde_json::from_str(r#"{"next":3,"people":{}}"#).unwrap();
        assert_eq!(empty.people, Some(People::default()));
    }

    #[test]
    fn unlocks_and_looks_parse_and_older_hubs_send_none() {
        let profile: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5,
                "unlocks":[{"id":"saber_sun","granted":1791000000,"note":"Thanks"},
                           {"id":"from_the_future","granted":7},
                           {"id":"saber_glitch","granted":8,"note":"","medal":"bug_hunter"}]}"#,
        )
        .unwrap();
        assert_eq!(
            profile.unlocks,
            [
                Unlock {
                    id: "saber_sun".to_owned(),
                    granted: 1_791_000_000,
                    note: "Thanks".to_owned(),
                    count: 1,
                    medal: None,
                },
                Unlock {
                    id: "from_the_future".to_owned(),
                    granted: 7,
                    note: String::new(),
                    count: 1,
                    medal: None,
                },
                Unlock {
                    id: "saber_glitch".to_owned(),
                    granted: 8,
                    note: String::new(),
                    count: 1,
                    medal: Some("bug_hunter".to_owned()),
                },
            ]
        );
        let old: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5}"#,
        )
        .unwrap();
        assert!(old.unlocks.is_empty(), "an older hub sends no unlocks");
        let presence: Presence = serde_json::from_str(
            r#"{"slot":3,"claimed_name":"x","key_id":"aa","name":"Sol","verified":false,
                "look":{"saber":"saber_sun","illuminate":true}}"#,
        )
        .unwrap();
        assert_eq!(
            presence.look,
            Some(Look {
                saber: "saber_sun".to_owned(),
                illuminate: true,
                saber_off: Vec::new(),
            })
        );
        let options: Presence = serde_json::from_str(
            r#"{"slot":3,"claimed_name":"x","key_id":"aa","name":"Sol","verified":false,
                "look":{"saber":"saber_sun","illuminate":false,"saber_off":["glint","haze"]}}"#,
        )
        .unwrap();
        assert_eq!(
            options.look.unwrap().saber_off,
            ["glint", "haze"],
            "the options switched off, since 11/10/2026"
        );
        let bare: Presence = serde_json::from_str(
            r#"{"slot":3,"claimed_name":"x","key_id":"aa","name":"Sol","verified":false}"#,
        )
        .unwrap();
        assert_eq!(bare.look, None, "a claim without a look, or an older hub");
        let feed: Feed = serde_json::from_str(
            r#"{"next":15,"looks":[{"id":15,"at":9,"slot":3,"claimed_name":"^2Sol",
                "key_id":"aa","saber":"","illuminate":true}]}"#,
        )
        .unwrap();
        let event = &feed.looks[0];
        assert_eq!((event.id, event.slot), (15, 3));
        assert_eq!(event.claimed_name, "^2Sol");
        assert_eq!(
            event.look(),
            Look {
                saber: String::new(),
                illuminate: true,
                saber_off: Vec::new(),
            }
        );
        let options: Feed = serde_json::from_str(
            r#"{"next":16,"looks":[{"id":16,"at":9,"slot":3,"claimed_name":"^2Sol",
                "key_id":"aa","saber":"saber_sun","illuminate":false,"saber_off":["wisps"]}]}"#,
        )
        .unwrap();
        assert_eq!(options.looks[0].look().saber_off, ["wisps"]);
        let quiet: Feed = serde_json::from_str(r#"{"next":3,"emotes":[]}"#).unwrap();
        assert!(quiet.looks.is_empty(), "an older hub sends no looks");
    }

    #[test]
    fn a_feed_answer_with_pings_parses() {
        let feed: Feed = serde_json::from_str(
            r#"{"next":17,"pings":[{"id":17,"at":9,"slot":3,"claimed_name":"^2Sol",
                "key_id":"aa","pos":[10,-20,30],"scope":"red"}]}"#,
        )
        .unwrap();
        let ping = &feed.pings[0];
        assert_eq!(
            (ping.slot, ping.pos, ping.scope.as_str()),
            (3, [10, -20, 30], "red")
        );
        let quiet: Feed = serde_json::from_str(r#"{"next":3}"#).unwrap();
        assert!(quiet.pings.is_empty(), "an older hub sends no pings");
    }

    #[test]
    fn holocrons_parse_in_profiles_and_older_hubs_send_none() {
        let profile: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5,
                "holocron_counts":{"uncommon":4,"rare":2,"legendary":1,"mythical":0},
                "holocrons":[{"id":9,"tier":"legendary","dropped":1791000000,"source":"play","note":""},
                             {"id":7,"tier":"rare","dropped":1790990000,"source":"staff","note":"Thanks"},
                             {"id":3,"tier":"from_the_future","dropped":1}]}"#,
        )
        .unwrap();
        assert_eq!(
            profile.holocron_counts,
            HolocronCounts {
                uncommon: 4,
                rare: 2,
                legendary: 1,
                mythical: 0
            }
        );
        assert_eq!(profile.holocron_counts.total(), 7);
        assert_eq!(profile.holocron_counts.of("rare"), 2);
        assert_eq!(profile.holocron_counts.of("from_the_future"), 0);
        let ids: Vec<u64> = profile
            .holocrons
            .iter()
            .map(|holocron| holocron.id)
            .collect();
        assert_eq!(ids, [9, 7, 3]);
        assert_eq!(profile.holocrons[1].source, "staff");
        assert_eq!(profile.holocrons[1].note, "Thanks");
        assert_eq!(profile.holocrons[2].source, "", "a missing field is empty");
        let old: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5}"#,
        )
        .unwrap();
        assert_eq!(old.holocron_counts, HolocronCounts::default());
        assert!(old.holocrons.is_empty(), "an older hub sends none");
    }

    #[test]
    fn a_persons_keys_parse_and_older_hubs_send_none() {
        let person: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5,
                "keys":["aa","cc","dd"]}"#,
        )
        .unwrap();
        assert_eq!(person.keys, ["aa", "cc", "dd"]);
        assert_eq!(person.linked_keys().collect::<Vec<_>>(), ["cc", "dd"]);
        assert!(person.has_key("aa") && person.has_key("dd"));
        assert!(!person.has_key("ee"));
        let old: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5}"#,
        )
        .unwrap();
        assert!(old.keys.is_empty(), "an older hub sends no keys");
        assert_eq!(old.linked_keys().count(), 0);
        assert!(old.has_key("aa"));
    }

    #[test]
    fn the_holocron_state_and_drop_events_parse() {
        let state: HolocronState = serde_json::from_str(
            r#"{"progress_secs":600,"every_secs":1800,"today":3,"daily_cap":8,"server_time":1791000000}"#,
        )
        .unwrap();
        assert_eq!(
            state,
            HolocronState {
                progress_secs: 600,
                every_secs: 1800,
                today: 3,
                daily_cap: 8,
                server_time: 1_791_000_000,
                can_open: false,
                loot: Vec::new(),
            }
        );
        let odds: HolocronState = serde_json::from_str(
            r#"{"progress_secs":0,"every_secs":1800,"server_time":1,"loot":[{"tier":"uncommon",
                "drop":600,"rarity":[700,240,55,5],"finish":[970,30,0],"credits":[10,20],
                "shader":300,"shader_tier":[750,220,30,0,0]}]}"#,
        )
        .unwrap();
        assert_eq!(odds.loot[0].tier, "uncommon");
        assert_eq!((odds.loot[0].credits, odds.loot[0].shader), ([10, 20], 300));
        let staff: HolocronState = serde_json::from_str(
            r#"{"progress_secs":0,"every_secs":1800,"today":0,"daily_cap":8,"server_time":1,"can_open":true}"#,
        )
        .unwrap();
        assert!(staff.can_open);
        let feed: Feed = serde_json::from_str(
            r#"{"next":21,"drops":[{"id":20,"at":1791000001,"key_id":"aa","name":"^2Sol",
                "tier":"legendary","verified":true},
                {"id":21,"at":1791000002,"key_id":"bb","name":"Fox","tier":"rare"}]}"#,
        )
        .unwrap();
        assert_eq!(feed.drops.len(), 2);
        assert_eq!(
            (feed.drops[0].tier.as_str(), feed.drops[0].verified),
            ("legendary", true)
        );
        assert!(!feed.drops[1].verified, "absent is not verified");
        let quiet: Feed = serde_json::from_str(r#"{"next":3}"#).unwrap();
        assert!(quiet.drops.is_empty(), "an older hub sends no drops");
        // A chat message is never a drop, whatever the wire says.
        let message: ChatMessage = serde_json::from_str(
            r#"{"id":1,"at":2,"key_id":"aa","name":"x","text":"y","holocron":{"tier":"rare","own":true}}"#,
        )
        .unwrap();
        assert_eq!(message.holocron, None);
    }

    #[test]
    fn names_match_through_colours_and_case() {
        assert!(names_match("^1Sol", "sol"));
        assert!(names_match("S.o.l", "^2SOL"));
        assert!(!names_match("Sol", "Fox"));
        assert!(!names_match("^1^2", "^3"));
    }

    #[test]
    fn profiles_and_presence_parse() {
        let profile: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5}"#,
        )
        .unwrap();
        assert!(profile.verified);
        assert!(profile.names.is_empty(), "an older hub sends no history");
        let profile: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":false,"created":5,
                "names":[{"name":"^1Sol","first_seen":1,"last_seen":9}]}"#,
        )
        .unwrap();
        assert_eq!(profile.names[0].name, "^1Sol");
        let presence: Presence = serde_json::from_str(
            r#"{"slot":3,"claimed_name":"x","key_id":"aa","name":"Sol","verified":false}"#,
        )
        .unwrap();
        assert_eq!(presence.slot, 3);
        assert!(presence.medals.is_empty(), "an older hub sends no medals");
        assert!(presence.avatar.is_empty(), "an older hub sends no picture");
        assert!(profile.avatar.is_empty());
        let pictured: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":false,"created":5,
                "avatar":"0123456789abcdef"}"#,
        )
        .unwrap();
        assert_eq!(pictured.avatar, "0123456789abcdef");
        let presence: Presence = serde_json::from_str(
            r#"{"slot":3,"claimed_name":"x","key_id":"aa","name":"Sol","verified":false,
                "avatar":"fedcba9876543210"}"#,
        )
        .unwrap();
        assert_eq!(presence.avatar, "fedcba9876543210");
    }

    #[test]
    fn medals_parse_in_profiles_and_presence() {
        let profile: Profile = serde_json::from_str(
            r#"{"key_id":"aa","key":"bb","name":"Sol","bio":"","verified":true,"created":5,
                "medals":[{"id":"early_tester","count":1,"awarded":1791000000,"note":""},
                          {"id":"bug_hunter","count":2,"awarded":1791300000,"note":"The ^1fog^7 bug"},
                          {"id":"from_the_future","count":1,"awarded":1,"note":""}]}"#,
        )
        .unwrap();
        assert!(profile.names.is_empty());
        let ids: Vec<&str> = profile
            .medals
            .iter()
            .map(|medal| medal.id.as_str())
            .collect();
        assert_eq!(ids, ["early_tester", "bug_hunter", "from_the_future"]);
        assert_eq!(profile.medals[1].count, 2);
        assert_eq!(profile.medals[1].awarded, 1_791_300_000);
        assert_eq!(profile.medals[1].note, "The ^1fog^7 bug");
        let presence: Presence = serde_json::from_str(
            r#"{"slot":3,"claimed_name":"x","key_id":"aa","name":"Sol","verified":false,
                "medals":[{"id":"early_tester","count":1},{"id":"jof_clan"}]}"#,
        )
        .unwrap();
        assert_eq!(
            presence.medals,
            [
                Medal {
                    id: "early_tester".to_owned(),
                    count: 1,
                    awarded: 0,
                    note: String::new(),
                },
                Medal {
                    id: "jof_clan".to_owned(),
                    count: 1,
                    awarded: 0,
                    note: String::new(),
                },
            ]
        );
    }
}
