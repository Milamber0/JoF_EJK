//! The background service that keeps the player registered with the hub, repeats
//! their game-server claim while they play, tells the hub the look they wear there,
//! and reads who else on that server is known. Nothing here blocks a frame: the viewer sends [`Command`]s through
//! [`Service`] and reads a [`Snapshot`].
//!
//! Once registered, and only when told where ([`Service::keep_assets`]), it also keeps
//! the hub's asset packs ([`crate::assets`]) in a cache folder.
//!
//! The service is inert while it is disabled or has no hub address: it makes no
//! request of any kind.

use crate::feed::{ChatState, FeedShared, FeedWorker, QueuedLook, QueuedPing, ReceivedLooks};
use crate::hub::{Hub, HubError};
use crate::keys::Identity;
use crate::logs::{LogRequest, LogUpload};
use crate::report::{BugReport, PlayerReport, WorldNote};
use crate::staff::{OpenedText, StaffQuery, StaffRequest, StaffState};
use crate::vitals::{Sharing, VitalsTable, VitalsWorker};
use crate::wire::{
    Achievement, Emote, HolocronState, Look, Ping, Presence, Profile, Unlock, names_match,
};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How often a claim is repeated; a claim lives 90 seconds at the hub.
const CLAIM_EVERY: Duration = Duration::from_secs(45);
/// How often the roster of known players is read.
const POLL_EVERY: Duration = Duration::from_secs(15);
/// How often the player's own profile is read again while registered, so a medal
/// the SJK team gives during a session shows without a restart.
const PROFILE_EVERY: Duration = Duration::from_secs(600);
/// Shortest time between two readings of the own profile brought forward because its
/// unlocks look out of date (a skin refused, or taken back at the hub).
const PROFILE_SOON: Duration = Duration::from_secs(30);
/// How often the progress toward the next holocron is read while registered.
const HOLOCRONS_EVERY: Duration = Duration::from_secs(300);
/// Shortest time between two readings of it brought forward (a drop of the player's
/// own key, or the viewer asking), and the profile's early reading for a drop.
const HOLOCRONS_SOON: Duration = Duration::from_secs(30);
/// How long a hub that refused the reading (from before holocrons) is left alone.
const HOLOCRONS_AGAIN: Duration = Duration::from_secs(3_600);
/// Shortest time between two sendings of the achievement counts.
const ACHIEVEMENTS_EVERY: Duration = Duration::from_secs(60);
/// How long counts the hub held back (over an hourly allowance) wait before they are
/// sent again unchanged, and how long a refusal waits.
const ACHIEVEMENTS_AGAIN: Duration = Duration::from_secs(3_600);
/// How long a refused model list waits before it is sent again unchanged.
const MODELS_AGAIN: Duration = Duration::from_secs(3_600);
/// Model answers kept at most; past it the oldest are forgotten.
const MODEL_CHECKS_MAX: usize = 256;
/// First wait after a failure, doubled up to [`RETRY_MAX`].
const RETRY_MIN: Duration = Duration::from_secs(10);
const RETRY_MAX: Duration = Duration::from_secs(120);
/// Longest the worker sleeps without looking at its commands.
const IDLE_MAX: Duration = Duration::from_secs(60);
/// Shortest time between two looks sent: the hub takes one a second.
const LOOK_EVERY: Duration = Duration::from_secs(1);
/// How long a look waits after the hub said too many (429: `look_quota`, or the
/// address's `rate_limited`), refused the signature (401) or failed.
const LOOK_AGAIN: Duration = Duration::from_secs(10);
/// How often the hub's list of asset packs is read while registered.
const ASSETS_EVERY: Duration = Duration::from_secs(6 * 3_600);
/// First wait after an asset check fell short, doubled up to [`ASSETS_EVERY`].
/// How often the crash folder is looked at again while registered: a report a worker
/// thread's panic left while the game went on goes within the hour.
const CRASHES_EVERY: Duration = Duration::from_secs(3_600);
/// How long reports the hub could not take wait before the next try.
const CRASHES_AGAIN: Duration = Duration::from_secs(600);
const ASSETS_RETRY_MIN: Duration = Duration::from_secs(60);
/// How often the SJK team's requests for the player's logs are looked for.
const LOG_REQUESTS_EVERY: Duration = Duration::from_secs(120);
/// How long a hub that refused the reading (from before log requests) is left alone.
const LOG_REQUESTS_AGAIN: Duration = Duration::from_secs(3_600);

/// What the player has configured.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    /// Whether the identity features are on.
    pub enabled: bool,
    /// The hub's address; empty means no hub, so nothing is sent.
    pub hub_url: String,
}

/// Where the player is: the game server they are connected to and how the game
/// shows them there.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    /// The server's address as connected.
    pub server: SocketAddr,
    /// The player's own client number there.
    pub slot: u8,
    /// The player's name as the game shows it.
    pub name: String,
}

/// How the service is doing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    /// Turned off in the settings.
    Disabled,
    /// On, but there is no hub address.
    NoHub,
    /// Registering with the hub.
    Registering,
    /// Registered; claims and reads work.
    Online,
    /// The last hub request failed (the text says why); the service retries.
    Failed(String),
}

/// Whether the player of a slot has a model (`PROTOCOL.md`, "Models").
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelCheck {
    /// The name the slot's claim was made under, empty when nobody the hub knows
    /// holds it.
    pub claimed_name: String,
    /// Whether they have the model; `None` when they sent no list, or the hub did
    /// not answer.
    pub has: Option<bool>,
}

/// What the viewer may read.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// The service's status.
    pub status: Status,
    /// The player's own id at the hub: this PC's key id until the hub answered the
    /// player's profile, then the id of the person that profile is (another one when
    /// this key is linked to a person's main key). Presence, the feed and the pages
    /// recognise the player by it.
    pub key_id: String,
    /// This PC's key id (the key file's), whatever person it belongs to: for what is
    /// kept per key on this PC (seen lists, counts) and the Identity page.
    pub local_key_id: String,
    /// The player's profile at the hub, once registered.
    pub me: Option<Profile>,
    /// The game server being claimed on, if any.
    pub server: Option<SocketAddr>,
    /// Known players on that server, by slot.
    pub players: Vec<Presence>,
    /// Profiles fetched with [`Service::look_up`].
    pub profiles: HashMap<String, Profile>,
    /// What the hub answered to [`Service::check_model`], by server, slot and model.
    pub model_checks: HashMap<(SocketAddr, u8, String), ModelCheck>,
    /// Counts profiles fetched into [`Snapshot::profiles`] and answers stored in
    /// [`Snapshot::model_checks`], so a reader that derived something from one (the
    /// player card) knows when to read again.
    pub profiles_revision: u64,
    /// The outcome of the last profile change or lookup, for the page to show.
    pub notice: Option<String>,
    /// Counts changes to [`Snapshot::players`], so a reader that caches what it
    /// derived from them (scoreboard tags) knows when to derive again.
    pub revision: u64,
    /// The outcome of the last bug report sent with [`Service::report`].
    pub report: Option<ReportOutcome>,
    /// The outcome of the last world note sent with [`Service::note`].
    pub note: Option<ReportOutcome>,
    /// The outcome of the last player report sent with [`Service::player_report`].
    pub player_report: Option<ReportOutcome>,
    /// The outcome of the last picture change ([`Service::set_avatar`],
    /// [`Service::remove_avatar`]): sent when the hub took it.
    pub avatar: Option<ReportOutcome>,
    /// What became of the last look sent ([`Service::set_look`]).
    pub look_outcome: Option<ReportOutcome>,
    /// Counts the asset packs written into the folder given to
    /// [`Service::keep_assets`], so the viewer knows to mount them again.
    pub packs_revision: u64,
    /// What the last check of the asset packs did, for the log. Asset packs are
    /// cosmetic: a failure shows here, never in [`Snapshot::status`].
    pub assets_note: Option<String>,
    /// Counts crash reports the hub took ([`Service::send_crashes`]).
    pub crashes_sent: u64,
    /// What the last round of crash reports did, for the log, once a round sent,
    /// dropped or kept back anything.
    pub crash_note: Option<String>,
    /// How far the player is from their next holocron, as the hub last said (read at
    /// the registration, every few minutes, after a drop of their own and on
    /// [`Service::refresh_holocrons`]). `None` before the hub answered, or from a hub
    /// older than holocrons. Cosmetic: a failure leaves [`Snapshot::status`] alone.
    pub holocrons: Option<HolocronProgress>,
    /// The odds the hub rolls holocrons with, one table per tier, as it last sent them
    /// with the progress; empty before, or from a hub older than them (14/10/2026).
    pub loot: Vec<crate::wire::LootTier>,
    /// The SJK team's open requests for the player's logs (oldest first), as the hub
    /// last listed them, without the ones the player answered: the game asks the
    /// player about each ([`Service::answer_logs`]).
    pub log_requests: Vec<LogRequest>,
    /// What became of the last answer to a log request.
    pub log_answer: Option<ReportOutcome>,
    /// What became of the last opening of holocrons ([`Service::open_holocrons`]).
    pub opening: Option<OpenOutcome>,
    /// What became of the last reveal of sealed cards ([`Service::reveal_cards`]).
    pub reveal: Option<RevealOutcome>,
}

/// What became of a reveal of sealed cards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevealOutcome {
    /// Counts reveals asked for, so a reader can tell a new outcome from the last one.
    pub serial: u64,
    /// The copies revealed; empty when none could be (or it failed).
    pub revealed: Vec<crate::wire::Revealed>,
    /// Why it failed; empty when it worked.
    pub message: String,
}

/// What became of an opening of holocrons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenOutcome {
    /// Counts openings asked for, so a reader can tell a new outcome from the last one.
    pub serial: u64,
    /// What each holocron brought, in the order asked; empty when nothing was opened.
    pub opened: Vec<crate::wire::Opened>,
    /// Why nothing was opened; empty when it worked.
    pub message: String,
}

/// How far the player's key is from its next holocron (`GET /v1/holocrons`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HolocronProgress {
    /// Seconds of active play carried toward the next holocron, when it was read.
    pub progress_secs: u64,
    /// Seconds of active play for one holocron.
    pub every_secs: u64,
    /// Holocrons dropped in the 24 hours before it was read.
    pub today: u32,
    /// The most a key can earn in 24 hours.
    pub daily_cap: u32,
    /// The hub's clock when it answered, unix seconds.
    pub server_time: i64,
    /// Whether the person may open holocrons now (only staff while opening is tested).
    pub can_open: bool,
    /// When this client read it.
    pub read_at: Instant,
}

impl HolocronProgress {
    fn new(state: HolocronState, read_at: Instant) -> Self {
        Self {
            progress_secs: state.progress_secs,
            every_secs: state.every_secs,
            today: state.today,
            daily_cap: state.daily_cap,
            server_time: state.server_time,
            can_open: state.can_open,
            read_at,
        }
    }

    /// Seconds of active play left to the next holocron as read: the hub's
    /// `every_secs - progress_secs`. It does not count down on its own; the player
    /// may be playing or not.
    pub fn remaining_secs(&self) -> u64 {
        self.every_secs.saturating_sub(self.progress_secs)
    }

    /// Whether the day's cap is reached, so no holocron drops until one is a day old.
    pub fn capped(&self) -> bool {
        self.daily_cap > 0 && self.today >= self.daily_cap
    }
}

/// What became of a bug report or a world note.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportOutcome {
    /// Counts reports sent, so a reader can tell a new outcome from the last one.
    pub serial: u64,
    /// The hub stored it.
    pub sent: bool,
    /// The hub's report number, or why it was not stored.
    pub message: String,
}

impl Snapshot {
    fn new(key_id: String) -> Self {
        Self {
            status: Status::Disabled,
            local_key_id: key_id.clone(),
            key_id,
            me: None,
            server: None,
            players: Vec::new(),
            profiles: HashMap::new(),
            model_checks: HashMap::new(),
            profiles_revision: 0,
            notice: None,
            revision: 0,
            report: None,
            note: None,
            player_report: None,
            avatar: None,
            look_outcome: None,
            packs_revision: 0,
            assets_note: None,
            crashes_sent: 0,
            crash_note: None,
            holocrons: None,
            loot: Vec::new(),
            log_requests: Vec::new(),
            log_answer: None,
            opening: None,
            reveal: None,
        }
    }

    /// Take `profile` as the player's own: the player's id follows the person it is
    /// (a linked key's profile is its person's).
    fn set_me(&mut self, profile: Profile) {
        if self.key_id != profile.key_id {
            self.key_id.clone_from(&profile.key_id);
        }
        self.me = Some(profile);
    }

    /// Forget the own profile (the identity off, another hub, a key unlinked): the
    /// player is this PC's key again until the hub answers.
    fn clear_me(&mut self) {
        self.me = None;
        if self.key_id != self.local_key_id {
            self.key_id.clone_from(&self.local_key_id);
        }
    }

    /// The known player in `slot` whose claimed name matches `shown`, the name the
    /// game shows there. A claim for another name is somebody else's and is ignored.
    pub fn badge(&self, slot: u8, shown: &str) -> Option<&Presence> {
        self.players
            .iter()
            .find(|player| player.slot == slot && names_match(&player.claimed_name, shown))
    }
}

/// A request to the worker.
enum Command {
    Configure(Settings),
    Enter(Location),
    Leave,
    /// The in-game name the player now wears.
    Name(String),
    SetBio(String),
    /// The counts the client keeps for its achievements, by id.
    Achievements(BTreeMap<String, u64>),
    /// A staff request.
    Staff(StaffRequest),
    /// A staff query about logs and crash reports.
    StaffQuery(StaffQuery),
    /// The player's answer to log request `id`: the logs to send, or none to decline.
    AnswerLogs(i64, Option<LogUpload>),
    LookUp(String),
    /// The player models the game has.
    Models(Vec<String>),
    /// Ask whether the player of a slot has a model.
    CheckModel(SocketAddr, u8, String),
    Report(BugReport),
    PlayerReport(PlayerReport),
    /// A world note and the tag its picture will come with.
    Note(u64, Box<WorldNote>),
    /// The picture of the note tagged so.
    NoteImage(u64, Vec<u8>),
    /// An SJK chat message.
    Chat(String),
    /// An emote, for the slot the player claims, in step with another slot's or not.
    Emote(String, Option<u8>),
    /// A ping at a point (whole game units) for a scope, from the slot the player claims.
    Ping([i32; 3], String),
    /// Whether the SJK chat is on.
    SetChat(bool),
    /// A new picture, a PNG already cropped and scaled (`crate::avatar`).
    SetAvatar(Vec<u8>),
    /// Take the picture down.
    RemoveAvatar,
    /// The look the player wears.
    Look(Look),
    /// The feed relayed a look event of the player's own key: its id and blade skin.
    OwnLook(u64, String),
    /// The folder to keep the hub's asset packs in.
    AssetsDir(PathBuf),
    /// The folder crash reports wait in, or none to send none.
    Crashes(Option<PathBuf>),
    /// Whether the player is actively playing (`Service::set_active`).
    Active(bool),
    /// The feed relayed a holocron drop of the player's own key.
    OwnDrop,
    /// The viewer wants fresh holocron progress.
    RefreshHolocrons,
    /// Open these holocrons of the player's.
    OpenHolocrons(Vec<u64>),
    /// Reveal the player's sealed cards.
    RevealCards,
    Stop,
}

fn lock_chat(chat: &Mutex<ChatState>) -> MutexGuard<'_, ChatState> {
    crate::feed::lock(chat)
}

fn lock_feed(feed: &Mutex<FeedShared>) -> MutexGuard<'_, FeedShared> {
    crate::feed::lock(feed)
}

/// Notes whose pictures may still come, kept by tag: the newest few only.
const NOTES_AWAITING_PICTURES: usize = 8;

/// Builds the hub client for an address; `Err` for an address that cannot be used.
pub type HubFactory = Box<dyn Fn(&str) -> Result<Box<dyn Hub>, HubError> + Send>;

/// The worker's logic, advanced by [`Worker::handle`] and [`Worker::tick`] with
/// an explicit clock so it can be tested without waiting.
struct Worker {
    identity: Identity,
    make_hub: HubFactory,
    settings: Settings,
    hub: Option<Box<dyn Hub>>,
    location: Option<Location>,
    /// The server address the hub holds a claim for, to withdraw it.
    claimed: Option<String>,
    registered: bool,
    /// The in-game name the player wears, and the one the hub was last told.
    name: Option<String>,
    name_sent: Option<String>,
    snapshot: Arc<Mutex<Snapshot>>,
    due_register: Instant,
    due_name: Instant,
    due_claim: Instant,
    due_poll: Instant,
    due_profile: Instant,
    backoff: Duration,
    lookups: Vec<String>,
    /// Notes the hub took, as (tag, hub id), while their pictures may come.
    notes_sent: Vec<(u64, i64)>,
    /// The achievement counts the client keeps, the last ones the hub was sent, and
    /// when they may be sent next.
    counts: BTreeMap<String, u64>,
    counts_sent: Option<BTreeMap<String, u64>>,
    due_counts: Instant,
    /// The player models the game has, the last list the hub took, and when the list
    /// may be sent next.
    models: Option<Vec<String>>,
    models_sent: Option<Vec<String>>,
    due_models: Instant,
    /// Model questions waiting for the hub.
    model_checks: Vec<(SocketAddr, u8, String)>,
    /// What staff requests brought back.
    staff: Arc<Mutex<StaffState>>,
    /// Whether the SJK chat is on (`cl_sjkChat`).
    chat_on: bool,
    /// What the feed thread is told to read.
    feed: Arc<Mutex<FeedShared>>,
    /// What the chat shows; the worker writes the outcome of what it sends.
    chat: Arc<Mutex<ChatState>>,
    /// What became of the last ping sent ([`Service::ping_outcome`]).
    ping_outcome: Arc<Mutex<Option<ReportOutcome>>>,
    /// The look the viewer last gave.
    look: Look,
    /// The claim (server, slot and name) the hub accepted last, while it holds it: a
    /// look lives on it.
    look_claim: Option<(String, u8, String)>,
    /// The look the hub holds for the key: on the accepted claim, or, with none
    /// accepted now, on a claim that may still be live.
    look_held: Held,
    /// When the next look may go.
    due_look: Instant,
    /// Blade skins the hub said the key does not hold, and the unlocks the profile
    /// listed then: they are not sent again until those change.
    look_refused: Vec<String>,
    look_refused_for: Vec<Unlock>,
    /// The feed id of the last look the hub took from this worker.
    look_id: Option<u64>,
    /// When the own profile may next be read early ([`Worker::profile_soon`]).
    profile_soon_after: Instant,
    /// The folder the asset packs are kept in; without one none is asked for.
    assets_dir: Option<PathBuf>,
    /// The folder crash reports wait in; without one none is sent.
    crash_dir: Option<PathBuf>,
    /// When the crash folder is next looked at.
    due_crashes: Instant,
    /// When the asset packs are next checked, and the wait after the next shortfall.
    due_assets: Instant,
    assets_backoff: Duration,
    /// Whether the player is actively playing: sent with every claim.
    active: bool,
    /// When the progress toward the next holocron is next read, and when it may next
    /// be read early.
    due_holocrons: Instant,
    holocrons_soon_after: Instant,
    /// When the requests for the player's logs are next looked for.
    due_log_requests: Instant,
    /// Log requests the player answered, kept out of what the hub lists until it
    /// stops listing them.
    logs_answered: Vec<i64>,
}

/// What the hub holds as the player's look.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Held {
    /// Not known: the worker just started (a client that stopped without releasing may
    /// have left a live claim with a look), a claim failed (the last one may still be
    /// live, its look with it) or a release failed. The look goes on the next accepted
    /// claim whatever it is.
    Unknown,
    /// This look; [`Look::default`] for none, as a new claim starts.
    Look(Look),
}

impl Held {
    /// No look: what a new claim starts with.
    fn none() -> Self {
        Self::Look(Look::default())
    }
}

/// The refusal of anything sent before the hub answered the registration.
fn not_connected() -> HubError {
    HubError::Protocol("not connected to the hub (is identity on, cl_identity 1?)".to_owned())
}

/// What the player is told of a report or a note: what the hub stored it as, or why
/// it did not.
/// A hub from before pictures has no `/v1/avatar`: say so in words rather than as the
/// bare status its router answers with.
fn older_hub(error: HubError) -> HubError {
    match error {
        HubError::Protocol(why)
            if why.starts_with("status 404") || why.starts_with("status 405") =>
        {
            HubError::Rejected {
                status: 404,
                code: "no_pictures".to_owned(),
                message: "this SJK hub does not take pictures yet".to_owned(),
            }
        }
        other => other,
    }
}

/// What the player reads when the hub did not open the holocrons.
fn opening_refusal(error: &HubError) -> String {
    match error {
        HubError::Rejected { code, .. } if code == "holocron_opened" => {
            "That holocron was already opened.".to_owned()
        }
        HubError::Rejected { code, .. } if code == "not_found" => {
            "That holocron is not yours any more.".to_owned()
        }
        HubError::Rejected { message, .. } => message.clone(),
        // A hub from before opening has no such route.
        HubError::Protocol(why)
            if why.starts_with("status 404") || why.starts_with("status 405") =>
        {
            "This SJK hub cannot open holocrons yet.".to_owned()
        }
        other => other.to_string(),
    }
}

/// Why a reveal failed, as the Cards tab says it.
fn reveal_refusal(error: &HubError) -> String {
    match error {
        HubError::Rejected { message, .. } => message.clone(),
        // A hub from before the card catalogue has no such route.
        HubError::Protocol(why)
            if why.starts_with("status 404") || why.starts_with("status 405") =>
        {
            "This SJK hub cannot reveal cards yet.".to_owned()
        }
        other => other.to_string(),
    }
}

fn outcome_of(serial: u64, outcome: Result<String, HubError>) -> ReportOutcome {
    match outcome {
        Ok(message) => ReportOutcome {
            serial,
            sent: true,
            message,
        },
        Err(HubError::Rejected { message, .. }) => ReportOutcome {
            serial,
            sent: false,
            message,
        },
        Err(error) => ReportOutcome {
            serial,
            sent: false,
            message: error.to_string(),
        },
    }
}

fn lock(snapshot: &Mutex<Snapshot>) -> MutexGuard<'_, Snapshot> {
    snapshot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Worker {
    fn new(
        identity: Identity,
        make_hub: HubFactory,
        snapshot: Arc<Mutex<Snapshot>>,
        staff: Arc<Mutex<StaffState>>,
        now: Instant,
    ) -> Self {
        Self {
            identity,
            make_hub,
            settings: Settings::default(),
            hub: None,
            location: None,
            claimed: None,
            registered: false,
            name: None,
            name_sent: None,
            snapshot,
            due_register: now,
            due_name: now,
            due_claim: now,
            due_poll: now,
            due_profile: now,
            backoff: RETRY_MIN,
            lookups: Vec::new(),
            notes_sent: Vec::new(),
            counts: BTreeMap::new(),
            counts_sent: None,
            due_counts: now,
            models: None,
            models_sent: None,
            due_models: now,
            model_checks: Vec::new(),
            staff,
            chat_on: false,
            feed: Arc::default(),
            chat: Arc::default(),
            ping_outcome: Arc::default(),
            look: Look::default(),
            look_claim: None,
            look_held: Held::Unknown,
            due_look: now,
            look_refused: Vec::new(),
            look_refused_for: Vec::new(),
            look_id: None,
            profile_soon_after: now,
            assets_dir: None,
            crash_dir: None,
            due_crashes: now,
            due_assets: now,
            assets_backoff: ASSETS_RETRY_MIN,
            active: false,
            due_holocrons: now,
            holocrons_soon_after: now,
            due_log_requests: now,
            logs_answered: Vec::new(),
        }
    }

    /// Read the holocron progress soon, at most once every [`HOLOCRONS_SOON`], and
    /// never later than it was due anyway.
    fn holocrons_soon(&mut self, now: Instant) {
        let at = self.holocrons_soon_after.max(now);
        if at < self.due_holocrons {
            self.due_holocrons = at;
            self.holocrons_soon_after = at + HOLOCRONS_SOON;
        }
    }

    /// A holocron dropped for the player's own key: their profile lists it and their
    /// progress changed, so both are read again soon.
    fn own_drop(&mut self, now: Instant) {
        self.profile_soon(now);
        self.holocrons_soon(now);
    }

    /// Read the own profile soon: its unlocks look out of date. At most once every
    /// [`PROFILE_SOON`], and never later than it was due anyway.
    fn profile_soon(&mut self, now: Instant) {
        let at = self.profile_soon_after.max(now);
        if at < self.due_profile {
            self.due_profile = at;
            self.profile_soon_after = at + PROFILE_SOON;
        }
    }

    /// The feed relayed look `id` of the player's own key, with blade skin `saber`.
    /// One the hub made after the last this worker sent, wearing another skin than
    /// the one to send, is the hub taking the skin back (staff relocked it): the
    /// profile's unlocks are out of date.
    fn own_look_seen(&mut self, id: u64, saber: &str, now: Instant) {
        if self.look_id.is_none_or(|sent| id > sent) && saber != self.look_to_send().saber {
            self.profile_soon(now);
        }
    }

    /// Tell the feed thread what to read: the hub, only while the identity is on and
    /// the hub answered the registration, and then on a game server (for its looks
    /// and emotes, whatever the chat says) or with the chat on; the server played on;
    /// and whether the chat shows. Another hub or server, or none, is a new
    /// generation: the looks read before are dropped.
    fn publish_feed(&self) {
        let reading = self.chat_on || self.location.is_some();
        let url = (self.settings.enabled && self.registered && reading && self.hub.is_some())
            .then(|| self.settings.hub_url.trim().to_owned());
        let wanted = FeedShared {
            url,
            server: self
                .location
                .as_ref()
                .map(|location| location.server.to_string()),
            chat: self.chat_on,
            person: lock(&self.snapshot).key_id.clone(),
            generation: 0,
        };
        let mut shared = lock_feed(&self.feed);
        let generation = if (&shared.url, &shared.server) == (&wanted.url, &wanted.server) {
            shared.generation
        } else {
            shared.generation + 1
        };
        let wanted = FeedShared {
            generation,
            ..wanted
        };
        if *shared != wanted {
            *shared = wanted;
        }
    }

    /// Put what became of a message or an emote where the chat shows it.
    fn chat_outcome(&self, outcome: Result<String, HubError>) {
        let mut chat = lock_chat(&self.chat);
        let serial = chat.outcome.as_ref().map_or(1, |last| last.serial + 1);
        chat.outcome = Some(outcome_of(serial, outcome));
    }

    /// Send an SJK chat message under the rules, once registered.
    fn send_chat(&mut self, text: &str) {
        let outcome = match crate::chat::check(text) {
            Err(error) => Err(HubError::Rejected {
                status: 400,
                code: error.code().to_owned(),
                message: error.message().to_owned(),
            }),
            Ok(text) => match (self.hub.as_mut(), self.registered) {
                (Some(hub), true) => {
                    let name = self.name.clone().unwrap_or_default();
                    hub.chat(&self.identity, &text, &name)
                        .map(|_| String::new())
                }
                _ => Err(not_connected()),
            },
        };
        self.chat_outcome(outcome);
    }

    /// Send an emote for the slot the player claims on their server, in step with
    /// slot `sync`'s when it is set.
    fn send_emote(&mut self, emote: &str, sync: Option<u8>) {
        let well_formed = (1..=32).contains(&emote.len())
            && emote
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        let server = self
            .location
            .as_ref()
            .map(|location| location.server.to_string());
        let outcome = match (self.hub.as_mut(), self.registered, server) {
            _ if !well_formed => Err(HubError::Rejected {
                status: 400,
                code: "bad_emote".to_owned(),
                message: "an emote is 1 to 32 of a to z, 0 to 9 and _".to_owned(),
            }),
            (Some(_), true, None) => Err(HubError::Rejected {
                status: 403,
                code: "not_on_server".to_owned(),
                message: "join a server to emote".to_owned(),
            }),
            // A hub from before 15/10/2026 refuses `sync` (`bad_body`): the emote
            // still goes, out of step.
            (Some(hub), true, Some(server)) => {
                match hub.emote(&self.identity, &server, emote, sync) {
                    Err(HubError::Rejected { code, .. })
                        if sync.is_some() && code == "bad_body" =>
                    {
                        hub.emote(&self.identity, &server, emote, None)
                    }
                    sent => sent,
                }
                .map(|_| String::new())
            }
            _ => Err(not_connected()),
        };
        self.chat_outcome(outcome);
    }

    /// Send a ping from the slot the player claims on their server; what became of it
    /// goes to [`Service::ping_outcome`].
    fn send_ping(&mut self, pos: [i32; 3], scope: &str) {
        let server = self
            .location
            .as_ref()
            .map(|location| location.server.to_string());
        let outcome = match (self.hub.as_mut(), self.registered, server) {
            (Some(_), true, None) => Err(HubError::Rejected {
                status: 403,
                code: "not_on_server".to_owned(),
                message: "join a server to ping".to_owned(),
            }),
            (Some(hub), true, Some(server)) => hub
                .ping(&self.identity, &server, pos, scope)
                .map(|_| String::new()),
            _ => Err(not_connected()),
        };
        let mut last = crate::feed::lock(&self.ping_outcome);
        let serial = last.as_ref().map_or(1, |last| last.serial + 1);
        *last = Some(outcome_of(serial, outcome));
    }

    fn update(&self, change: impl FnOnce(&mut Snapshot)) {
        change(&mut lock(&self.snapshot));
    }

    fn failed(&mut self, error: &HubError, now: Instant) -> Instant {
        self.update(|snapshot| snapshot.status = Status::Failed(error.to_string()));
        let due = now + self.backoff;
        self.backoff = (self.backoff * 2).min(RETRY_MAX);
        due
    }

    /// Withdraw the claim the hub holds, if any, ignoring a failure: it expires. The
    /// look on it goes with it; after a failure the hub may hold both a while longer.
    fn release(&mut self) {
        match (self.claimed.take(), self.hub.as_mut()) {
            (Some(server), Some(hub)) => {
                self.look_held = match hub.release(&self.identity, &server) {
                    Ok(()) => Held::none(),
                    Err(_) => Held::Unknown,
                };
            }
            (Some(_), None) => self.look_held = Held::Unknown,
            (None, _) => {}
        }
        self.look_claim = None;
        self.update(|snapshot| {
            snapshot.server = None;
            snapshot.players.clear();
            snapshot.revision += 1;
        });
    }

    fn handle(&mut self, command: Command, now: Instant) {
        match command {
            Command::Configure(settings) => self.configure(settings, now),
            Command::Enter(location) => {
                if self.location.as_ref() == Some(&location) {
                    return;
                }
                if self.location.as_ref().map(|old| old.server) != Some(location.server) {
                    self.release();
                }
                self.update(|snapshot| snapshot.server = Some(location.server));
                self.location = Some(location);
                self.due_claim = now;
                self.due_poll = now;
            }
            Command::Leave => {
                self.release();
                self.location = None;
            }
            Command::Name(name) => {
                let name = Some(name).filter(|name| !name.trim().is_empty());
                if name != self.name {
                    self.name = name;
                    self.due_name = now;
                }
            }
            Command::SetBio(bio) => self.set_bio(&bio),
            Command::Achievements(counts) => self.counts = counts,
            Command::Staff(request) => self.staff(&request),
            Command::StaffQuery(query) => self.staff_query(&query),
            Command::AnswerLogs(id, upload) => self.answer_logs(id, upload.as_ref()),
            Command::Report(mut report) => {
                if report.name.is_empty() {
                    report.name = self.name.clone().unwrap_or_default();
                }
                self.report(&report);
            }
            Command::PlayerReport(mut report) => {
                if report.name.is_empty() {
                    report.name = self.name.clone().unwrap_or_default();
                }
                self.player_report(&report);
            }
            Command::Note(tag, mut note) => {
                if note.name.is_empty() {
                    note.name = self.name.clone().unwrap_or_default();
                }
                self.note(tag, &note);
            }
            Command::NoteImage(tag, jpeg) => self.note_image(tag, &jpeg),
            Command::LookUp(key_id) => self.lookups.push(key_id),
            Command::Models(models) => {
                if self.models.as_ref() != Some(&models) {
                    self.models = Some(models);
                    self.due_models = now;
                }
            }
            Command::CheckModel(server, slot, model) => {
                let question = (server, slot, model);
                if !self.model_checks.contains(&question) {
                    self.model_checks.push(question);
                }
            }
            Command::Chat(text) => self.send_chat(&text),
            Command::Emote(emote, sync) => self.send_emote(&emote, sync),
            Command::Ping(pos, scope) => self.send_ping(pos, &scope),
            Command::SetChat(on) => self.chat_on = on,
            Command::SetAvatar(png) => self.change_avatar(Some(&png)),
            Command::RemoveAvatar => self.change_avatar(None),
            Command::Look(look) => self.look = look,
            Command::OwnLook(id, saber) => self.own_look_seen(id, &saber, now),
            Command::AssetsDir(dir) => {
                if self.assets_dir.as_ref() != Some(&dir) {
                    self.assets_dir = Some(dir);
                    self.due_assets = now;
                    self.assets_backoff = ASSETS_RETRY_MIN;
                }
            }
            Command::Crashes(dir) => {
                if self.crash_dir != dir {
                    self.crash_dir = dir;
                    self.due_crashes = now;
                }
            }
            Command::Active(active) => self.active = active,
            Command::OwnDrop => self.own_drop(now),
            Command::RefreshHolocrons => self.holocrons_soon(now),
            Command::OpenHolocrons(ids) => self.open_holocrons(&ids, now),
            Command::RevealCards => self.reveal_cards(),
            Command::Stop => self.release(),
        }
        self.publish_feed();
    }

    fn configure(&mut self, settings: Settings, now: Instant) {
        if settings == self.settings {
            return;
        }
        // Anything that changes the hub or switches the feature off ends the claim
        // first, on the hub that holds it.
        self.release();
        self.hub = None;
        self.registered = false;
        self.name_sent = None;
        self.update(Snapshot::clear_me);
        if settings.enabled && !settings.hub_url.trim().is_empty() {
            match (self.make_hub)(settings.hub_url.trim()) {
                Ok(hub) => self.hub = Some(hub),
                Err(error) => {
                    self.update(|snapshot| snapshot.status = Status::Failed(error.to_string()))
                }
            }
        }
        self.settings = settings;
        self.backoff = RETRY_MIN;
        self.due_register = now;
        self.due_claim = now;
        self.due_poll = now;
    }

    fn note(&mut self, tag: u64, note: &WorldNote) {
        let outcome = match (self.hub.as_mut(), self.registered) {
            (Some(hub), true) => hub.note(&self.identity, note),
            _ => Err(HubError::Protocol(
                "not connected to the hub (is identity on, cl_identity 1?)".to_owned(),
            )),
        };
        if let Ok(id) = outcome {
            if self.notes_sent.len() >= NOTES_AWAITING_PICTURES {
                self.notes_sent.remove(0);
            }
            self.notes_sent.push((tag, id));
        }
        self.update(|snapshot| {
            let serial = snapshot.note.as_ref().map_or(1, |last| last.serial + 1);
            snapshot.note = Some(outcome_of(serial, outcome.map(|id| format!("note #{id}"))));
        });
    }

    /// Attach `jpeg` to the note tagged `tag`, if the hub took that note. A picture
    /// that fails leaves the note as it is.
    fn note_image(&mut self, tag: u64, jpeg: &[u8]) {
        let Some(index) = self.notes_sent.iter().position(|(sent, _)| *sent == tag) else {
            return;
        };
        let (_, id) = self.notes_sent.remove(index);
        if let Some(hub) = self.hub.as_mut() {
            let _ = hub.note_image(&self.identity, id, jpeg);
        }
    }

    fn report(&mut self, report: &BugReport) {
        let outcome = match (self.hub.as_mut(), self.registered) {
            (Some(hub), true) => hub.report(&self.identity, report),
            _ => Err(HubError::Protocol(
                "not connected to the hub (is identity on, cl_identity 1?)".to_owned(),
            )),
        };
        self.update(|snapshot| {
            let serial = snapshot.report.as_ref().map_or(1, |last| last.serial + 1);
            snapshot.report = Some(outcome_of(
                serial,
                outcome.map(|id| format!("report #{id}")),
            ));
        });
    }

    /// Send a player report: only once registered, and only with a key the hub's
    /// operator verified (the hub refuses the others too).
    fn player_report(&mut self, report: &PlayerReport) {
        let verified = lock(&self.snapshot)
            .me
            .as_ref()
            .is_some_and(|me| me.verified);
        let outcome = match (self.hub.as_mut(), self.registered, verified) {
            (Some(hub), true, true) => hub.player_report(&self.identity, report),
            (Some(_), true, false) => Err(HubError::Rejected {
                status: 403,
                code: "not_verified".to_owned(),
                message: "only verified SJK players can report players".to_owned(),
            }),
            _ => Err(HubError::Protocol(
                "not connected to the hub (is identity on, cl_identity 1?)".to_owned(),
            )),
        };
        self.update(|snapshot| {
            let serial = snapshot
                .player_report
                .as_ref()
                .map_or(1, |last| last.serial + 1);
            snapshot.player_report = Some(outcome_of(
                serial,
                outcome.map(|id| format!("player report #{id}")),
            ));
        });
    }

    /// Send the player's new picture (`Some`, a PNG) or take it down (`None`), once
    /// registered; the profile the hub answers with replaces the player's own.
    fn change_avatar(&mut self, png: Option<&[u8]>) {
        let outcome = match (self.hub.as_mut(), self.registered, png) {
            (_, _, Some(png)) if png.len() > crate::avatar::UPLOAD_MAX => Err(HubError::Rejected {
                status: 413,
                code: "too_large".to_owned(),
                message: "the picture is larger than the hub takes".to_owned(),
            }),
            (Some(hub), true, Some(png)) => hub.set_avatar(&self.identity, png),
            (Some(hub), true, None) => hub.remove_avatar(&self.identity),
            _ => Err(not_connected()),
        }
        .map_err(older_hub);
        self.update(|snapshot| {
            let serial = snapshot.avatar.as_ref().map_or(1, |last| last.serial + 1);
            let outcome = outcome.map(|profile| {
                let done = if profile.avatar.is_empty() {
                    "removed"
                } else {
                    "saved"
                };
                snapshot.set_me(profile);
                done.to_owned()
            });
            snapshot.avatar = Some(outcome_of(serial, outcome));
        });
    }

    /// Open holocrons `ids` (the first [`crate::wire::OPEN_MAX`]): the outcome goes in
    /// [`Snapshot::opening`], the hub's profile after it in [`Snapshot::me`]. When the hub
    /// refuses (one was opened meanwhile, from another PC of the person's, or is gone),
    /// the own profile is read again soon so the pages show what is really left.
    fn open_holocrons(&mut self, ids: &[u64], now: Instant) {
        let ids = &ids[..ids.len().min(crate::wire::OPEN_MAX)];
        let outcome = match (self.hub.as_mut(), self.registered) {
            (_, _) if ids.is_empty() => Err(HubError::Rejected {
                status: 400,
                code: "bad_ids".to_owned(),
                message: "there is no holocron to open".to_owned(),
            }),
            (Some(hub), true) => hub.open_holocrons(&self.identity, ids),
            _ => Err(not_connected()),
        };
        if let Err(HubError::Rejected { code, .. }) = &outcome
            && (code == "holocron_opened" || code == "not_found")
        {
            self.profile_soon(now);
        }
        self.update(|snapshot| {
            let serial = snapshot.opening.as_ref().map_or(1, |last| last.serial + 1);
            snapshot.opening = Some(match outcome {
                Ok(answer) => {
                    snapshot.set_me(answer.profile);
                    OpenOutcome {
                        serial,
                        opened: answer.opened,
                        message: String::new(),
                    }
                }
                Err(error) => OpenOutcome {
                    serial,
                    opened: Vec::new(),
                    message: opening_refusal(&error),
                },
            });
        });
    }

    /// Reveal the player's sealed cards: the outcome goes in [`Snapshot::reveal`], the
    /// hub's profile after it in [`Snapshot::me`].
    fn reveal_cards(&mut self) {
        let outcome = match (self.hub.as_mut(), self.registered) {
            (Some(hub), true) => hub.reveal_cards(&self.identity),
            _ => Err(not_connected()),
        };
        self.update(|snapshot| {
            let serial = snapshot.reveal.as_ref().map_or(1, |last| last.serial + 1);
            snapshot.reveal = Some(match outcome {
                Ok(answer) => {
                    snapshot.set_me(answer.profile);
                    RevealOutcome {
                        serial,
                        revealed: answer.revealed,
                        message: String::new(),
                    }
                }
                Err(error) => RevealOutcome {
                    serial,
                    revealed: Vec::new(),
                    message: reveal_refusal(&error),
                },
            });
        });
    }

    fn set_bio(&mut self, bio: &str) {
        // The hub's rules, checked here first so the player hears why at once.
        let bio = match crate::bio::check(bio) {
            Ok(bio) => bio,
            Err(error) => {
                self.update(|snapshot| snapshot.notice = Some(error.to_string()));
                return;
            }
        };
        let outcome = match (self.hub.as_mut(), self.registered) {
            (Some(hub), true) => hub.set_bio(&self.identity, &bio),
            _ => {
                self.update(|snapshot| {
                    snapshot.notice = Some("not connected to the hub".to_owned())
                });
                return;
            }
        };
        self.update(|snapshot| match outcome {
            Ok(profile) => {
                snapshot.set_me(profile);
                snapshot.notice = Some("saved".to_owned());
            }
            Err(error) => snapshot.notice = Some(error.to_string()),
        });
    }

    /// Send a staff request: only once registered, and only for a key whose profile
    /// says staff (the hub refuses the others too).
    fn staff(&mut self, request: &StaffRequest) {
        let me = lock(&self.snapshot).me.clone();
        let outcome = match (self.hub.as_mut(), self.registered, &me) {
            (Some(hub), true, Some(me)) if me.staff => hub.staff(&self.identity, request),
            (Some(_), true, _) => Err(HubError::Rejected {
                status: 403,
                code: "not_staff".to_owned(),
                message: "only SJK staff can do this".to_owned(),
            }),
            _ => Err(HubError::Protocol(
                "not connected to the hub (is identity on, cl_identity 1?)".to_owned(),
            )),
        };
        let mut staff = self
            .staff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        staff.serial += 1;
        staff.busy = false;
        match outcome {
            Ok(profiles) => {
                staff.failed = false;
                staff.message = crate::staff::done(request, profiles.len());
                if matches!(request, StaffRequest::Search(_)) {
                    staff.players = profiles;
                } else if let Some(profile) = profiles.first() {
                    // The player merged away is part of the kept one now.
                    if let StaffRequest::Merge { from, .. } = request {
                        staff.remove(from, &profile.key_id);
                    }
                    staff.replace(profile);
                    // A change to the player's own person shows on their pages at once.
                    if Some(profile.key_id.as_str()) == me.as_ref().map(|me| me.key_id.as_str()) {
                        lock(&self.snapshot).set_me(profile.clone());
                    }
                    // This PC's own key unlinked: it starts afresh, registering again.
                    if let StaffRequest::Unlink { key_id } = request
                        && *key_id == self.identity.key_id()
                    {
                        self.registered = false;
                        self.name_sent = None;
                        self.update(Snapshot::clear_me);
                    }
                }
            }
            Err(HubError::Rejected { message, .. }) => {
                staff.failed = true;
                staff.message = message;
            }
            Err(error) => {
                staff.failed = true;
                staff.message = error.to_string();
            }
        }
    }

    /// Send a staff query: only once registered, and only for a key whose profile
    /// says staff (the hub refuses the others too).
    fn staff_query(&mut self, query: &StaffQuery) {
        let staff_key = lock(&self.snapshot).me.as_ref().is_some_and(|me| me.staff);
        let outcome = match (self.hub.as_mut(), self.registered, staff_key) {
            (Some(hub), true, true) => hub.staff_query(&self.identity, query),
            (Some(_), true, false) => Err(HubError::Rejected {
                status: 403,
                code: "not_staff".to_owned(),
                message: "only SJK staff can do this".to_owned(),
            }),
            _ => Err(not_connected()),
        };
        let mut staff = self
            .staff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        staff.serial += 1;
        staff.busy = false;
        match outcome {
            Ok(answer) => {
                staff.failed = false;
                staff.answered(query, answer);
            }
            Err(HubError::Rejected {
                status: 404, code, ..
            }) if code == "not_found" => {
                staff.failed = true;
                staff.message = match query {
                    StaffQuery::RequestLogs { .. } => "The hub does not know that player",
                    _ => "The hub no longer has it",
                }
                .to_owned();
            }
            Err(HubError::Rejected { message, .. }) => {
                staff.failed = true;
                staff.message = message;
            }
            Err(error) => {
                staff.failed = true;
                staff.message = error.to_string();
            }
        }
    }

    /// Look for the SJK team's requests for the player's logs if it is due. Like the
    /// holocrons, a hub that refuses (from before log requests) is left alone a while.
    fn keep_log_requests(&mut self, now: Instant) {
        if now < self.due_log_requests {
            return;
        }
        let Some(hub) = self.hub.as_mut() else {
            return;
        };
        match hub.log_requests(&self.identity) {
            Ok(requests) => {
                self.due_log_requests = now + LOG_REQUESTS_EVERY;
                self.logs_answered
                    .retain(|id| requests.iter().any(|request| request.id == *id));
                let open: Vec<LogRequest> = requests
                    .into_iter()
                    .filter(|request| !self.logs_answered.contains(&request.id))
                    .collect();
                self.update(|snapshot| {
                    if snapshot.log_requests != open {
                        snapshot.log_requests = open;
                    }
                });
            }
            Err(HubError::Network(_)) => self.due_log_requests = now + RETRY_MAX,
            Err(_) => self.due_log_requests = now + LOG_REQUESTS_AGAIN,
        }
    }

    /// Send the player's answer to log request `id`: `upload` (fitted here) or, with
    /// none, a refusal. The request leaves the list whatever the hub says, unless it
    /// could not be reached: a request the hub no longer holds open is done with too.
    fn answer_logs(&mut self, id: i64, upload: Option<&LogUpload>) {
        if !self.logs_answered.contains(&id) {
            self.logs_answered.push(id);
        }
        self.update(|snapshot| snapshot.log_requests.retain(|request| request.id != id));
        let outcome = match (self.hub.as_mut(), self.registered, upload) {
            (Some(hub), true, Some(upload)) => hub
                .send_logs(&self.identity, id, &upload.fitted())
                .map(|()| "logs sent".to_owned()),
            (Some(hub), true, None) => hub
                .decline_logs(&self.identity, id)
                .map(|()| "declined".to_owned()),
            _ => Err(not_connected()),
        };
        // Not reached: the request comes back with the next listing, to answer again.
        if matches!(outcome, Err(HubError::Network(_))) {
            self.logs_answered.retain(|answered| *answered != id);
        }
        self.update(|snapshot| {
            let serial = snapshot
                .log_answer
                .as_ref()
                .map_or(1, |last| last.serial + 1);
            snapshot.log_answer = Some(outcome_of(serial, outcome));
        });
    }

    /// Do whatever is due at `now`; return how long to wait for the next thing.
    fn tick(&mut self, now: Instant) -> Duration {
        let wait = self.tick_due(now);
        self.publish_feed();
        wait
    }

    fn tick_due(&mut self, now: Instant) -> Duration {
        if !self.settings.enabled {
            self.update(|snapshot| snapshot.status = Status::Disabled);
            return IDLE_MAX;
        }
        if self.hub.is_none() {
            if self.settings.hub_url.trim().is_empty() {
                self.update(|snapshot| snapshot.status = Status::NoHub);
            }
            return IDLE_MAX;
        }
        if !self.registered {
            if now < self.due_register {
                return self.due_register - now;
            }
            self.update(|snapshot| {
                if !matches!(snapshot.status, Status::Failed(_)) {
                    snapshot.status = Status::Registering;
                }
            });
            let name = self.name.clone();
            let outcome = self
                .hub
                .as_mut()
                .map(|hub| hub.register(&self.identity, name.as_deref()));
            match outcome {
                Some(Ok(profile)) => {
                    self.registered = true;
                    self.name_sent = name;
                    self.backoff = RETRY_MIN;
                    self.due_profile = now + PROFILE_EVERY;
                    self.due_assets = now;
                    self.due_holocrons = now;
                    self.due_log_requests = now;
                    self.update(|snapshot| {
                        snapshot.set_me(profile);
                        snapshot.status = Status::Online;
                    });
                }
                Some(Err(error)) => {
                    self.due_register = self.failed(&error, now);
                    return self.due_register - now;
                }
                None => {}
            }
        }
        self.run_due(now);
        self.keep_holocrons(now);
        self.keep_log_requests(now);
        self.keep_assets(now);
        self.send_crashes(now);
        let mut wait = IDLE_MAX;
        if self.location.is_some() {
            wait = wait
                .min(self.due_claim.saturating_duration_since(now))
                .min(self.due_poll.saturating_duration_since(now));
        }
        if self.name != self.name_sent {
            wait = wait.min(self.due_name.saturating_duration_since(now));
        }
        wait = wait.min(self.due_profile.saturating_duration_since(now));
        if self.counts_waiting() {
            wait = wait.min(self.due_counts.saturating_duration_since(now));
        }
        if !self.lookups.is_empty() || !self.model_checks.is_empty() {
            wait = Duration::ZERO;
        }
        if self.models_waiting() {
            wait = wait.min(self.due_models.saturating_duration_since(now));
        }
        if self.look_waiting() {
            wait = wait.min(self.due_look.saturating_duration_since(now));
        }
        if self.assets_dir.is_some() {
            wait = wait.min(self.due_assets.saturating_duration_since(now));
        }
        if self.crash_dir.is_some() {
            wait = wait.min(self.due_crashes.saturating_duration_since(now));
        }
        wait.min(self.due_holocrons.saturating_duration_since(now))
            .min(self.due_log_requests.saturating_duration_since(now))
    }

    /// Read the progress toward the next holocron if it is due. Cosmetic, like the asset
    /// packs: the status stays. A hub that refuses (from before holocrons) is asked
    /// again in an hour, one that cannot be reached after [`RETRY_MAX`].
    fn keep_holocrons(&mut self, now: Instant) {
        if now < self.due_holocrons {
            return;
        }
        let Some(hub) = self.hub.as_mut() else {
            return;
        };
        match hub.holocrons(&self.identity) {
            Ok(state) => {
                self.due_holocrons = now + HOLOCRONS_EVERY;
                self.update(|snapshot| {
                    if !state.loot.is_empty() {
                        snapshot.loot.clone_from(&state.loot);
                    }
                    snapshot.holocrons = Some(HolocronProgress::new(state, now));
                });
            }
            Err(HubError::Network(_)) => self.due_holocrons = now + RETRY_MAX,
            Err(_) => self.due_holocrons = now + HOLOCRONS_AGAIN,
        }
    }

    /// Check the hub's asset packs if a folder was given and the check is due: one
    /// reading of the list, and a download of each pack the folder does not hold as
    /// listed. Next time is [`ASSETS_EVERY`] later; a shortfall worth trying again
    /// soon waits [`ASSETS_RETRY_MIN`], doubling, instead. Cosmetic: the status stays.
    fn keep_assets(&mut self, now: Instant) {
        let (Some(dir), Some(hub)) = (self.assets_dir.as_ref(), self.hub.as_mut()) else {
            return;
        };
        if now < self.due_assets {
            return;
        }
        let refresh = crate::assets::refresh(hub.as_mut(), &self.identity, dir);
        let wait = match &refresh.problem {
            Some(crate::assets::Problem::Soon(_)) => {
                let wait = self.assets_backoff;
                self.assets_backoff = (self.assets_backoff * 2).min(ASSETS_EVERY);
                wait
            }
            _ => {
                self.assets_backoff = ASSETS_RETRY_MIN;
                ASSETS_EVERY
            }
        };
        self.due_assets = now + wait;
        let note = format!("{}; next check in {} s", refresh.summary(), wait.as_secs());
        self.update(|snapshot| {
            snapshot.packs_revision += refresh.written.len() as u64;
            snapshot.assets_note = Some(note);
        });
    }

    /// Send the crash reports waiting in the folder given, if it is time: next time
    /// [`CRASHES_EVERY`] later, or [`CRASHES_AGAIN`] when some had to wait.
    fn send_crashes(&mut self, now: Instant) {
        let (Some(dir), Some(hub)) = (self.crash_dir.as_ref(), self.hub.as_mut()) else {
            return;
        };
        if now < self.due_crashes {
            return;
        }
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let name = self.name.clone().unwrap_or_default();
        let round = crate::crash::send_pending(hub.as_mut(), &self.identity, dir, &name, unix);
        self.due_crashes = now
            + if round.waiting > 0 {
                CRASHES_AGAIN
            } else {
                CRASHES_EVERY
            };
        if round == crate::crash::Round::default() {
            return;
        }
        let mut note = format!("crash reports: {} sent", round.sent.len());
        if round.dropped > 0 {
            note.push_str(&format!(", {} dropped", round.dropped));
        }
        if let Some(problem) = &round.problem {
            note.push_str(&format!(", {} waiting ({problem})", round.waiting));
        }
        self.update(|snapshot| {
            snapshot.crashes_sent += round.sent.len() as u64;
            snapshot.crash_note = Some(note);
        });
    }

    /// The look to send: the one the viewer gave, without a blade skin the hub said
    /// the key does not hold while the profile's unlocks are the same.
    fn look_to_send(&mut self) -> Look {
        if !self.look_refused.is_empty() {
            let same = lock(&self.snapshot)
                .me
                .as_ref()
                .is_some_and(|me| me.unlocks == self.look_refused_for);
            if !same {
                self.look_refused.clear();
            }
        }
        let mut look = self.look.clone();
        if self.look_refused.contains(&look.saber) {
            look.saber.clear();
            look.saber_off.clear();
        }
        look
    }

    /// Whether the hub's claim should get another look than the one it holds.
    fn look_waiting(&mut self) -> bool {
        self.look_claim.is_some() && Held::Look(self.look_to_send()) != self.look_held
    }

    /// Send the look for the accepted claim if it changed and one may go.
    fn send_look(&mut self, now: Instant) {
        if now < self.due_look || !self.look_waiting() {
            return;
        }
        let look = self.look_to_send();
        let (Some((server, _, _)), Some(hub)) = (self.look_claim.clone(), self.hub.as_mut()) else {
            return;
        };
        let outcome = hub.look(&self.identity, &server, &look);
        self.due_look = now + LOOK_EVERY;
        match &outcome {
            Ok(id) => {
                self.look_held = Held::Look(look.clone());
                self.look_id = Some(*id);
            }
            // The skin is not the key's (or not the hub's): the rest of the look still
            // goes, without it, until the profile changes. Not the key's while its
            // profile said so: the profile is out of date.
            Err(HubError::Rejected { code, .. })
                if (code == "not_unlocked" || code == "bad_look") && !look.saber.is_empty() =>
            {
                if code == "not_unlocked" {
                    self.profile_soon(now);
                }
                self.look_refused_for = lock(&self.snapshot)
                    .me
                    .as_ref()
                    .map(|me| me.unlocks.clone())
                    .unwrap_or_default();
                self.look_refused.push(look.saber.clone());
            }
            // The claim lapsed at the hub: the next accepted claim sends the look again.
            Err(HubError::Rejected { code, .. }) if code == "not_on_server" => {
                self.look_claim = None;
                self.look_held = Held::none();
            }
            // Too many: the key's look quota, or the address's allowance (429
            // `rate_limited`) shared with the chat and emotes. A signature refused
            // (401, the clock still off after the one retry) is not final either.
            Err(HubError::Rejected {
                status: 401 | 429, ..
            }) => {
                self.due_look = now + LOOK_AGAIN;
            }
            // Any other refusal (an older hub without looks) is not repeated until the
            // look or the claim changes.
            Err(HubError::Rejected { status, .. }) if (400..500).contains(status) => {
                self.look_held = Held::Look(look.clone());
            }
            Err(_) => self.due_look = now + LOOK_AGAIN,
        }
        self.update(|snapshot| {
            let serial = snapshot
                .look_outcome
                .as_ref()
                .map_or(1, |last| last.serial + 1);
            snapshot.look_outcome = Some(outcome_of(serial, outcome.map(|_| String::new())));
        });
    }

    /// Whether the hub should hear the achievement counts: they changed since they
    /// were last sent, or one is above what the hub holds (it was held back by its
    /// hourly allowance) and the last sending is an hour old.
    fn counts_waiting(&self) -> bool {
        if self.counts.is_empty() {
            return false;
        }
        if self.counts_sent.as_ref() != Some(&self.counts) {
            return true;
        }
        let held = lock(&self.snapshot).me.as_ref().map(|me| {
            self.counts.iter().any(|(id, &count)| {
                match me.achievements.iter().find(|held| &held.id == id) {
                    Some(held) => count > held.progress && held.unlocked == 0,
                    None => count > 0,
                }
            })
        });
        held.unwrap_or(false)
    }

    /// Whether the hub should hear the model list: the game listed its models and the
    /// hub has not taken this list.
    fn models_waiting(&self) -> bool {
        self.models.is_some() && self.models != self.models_sent
    }

    /// Send the model list if it is waiting and due. A refusal (an older hub, a
    /// quota) waits [`MODELS_AGAIN`]; a hub out of reach, the backoff.
    fn send_models(&mut self, now: Instant) {
        if now < self.due_models || !self.models_waiting() {
            return;
        }
        let (Some(hub), Some(models)) = (self.hub.as_mut(), self.models.as_ref()) else {
            return;
        };
        match hub.set_models(&self.identity, models) {
            Ok(_) => self.models_sent = self.models.clone(),
            Err(HubError::Rejected { status, .. }) if (400..500).contains(&status) => {
                self.due_models = now + MODELS_AGAIN;
            }
            Err(_) => self.due_models = now + self.backoff,
        }
    }

    /// Ask the hub the model questions waiting. Nobody holding the slot is an answer
    /// with no claimed name; a hub that cannot say leaves `has` unknown.
    fn check_models(&mut self) {
        let Some(hub) = self.hub.as_mut() else { return };
        for (server, slot, model) in std::mem::take(&mut self.model_checks) {
            let check = match hub.model_check(&server.to_string(), slot, &model) {
                Ok((claimed_name, has)) => ModelCheck { claimed_name, has },
                Err(_) => ModelCheck {
                    claimed_name: String::new(),
                    has: None,
                },
            };
            let mut snapshot = lock(&self.snapshot);
            if snapshot.model_checks.len() >= MODEL_CHECKS_MAX {
                snapshot.model_checks.clear();
            }
            snapshot.model_checks.insert((server, slot, model), check);
            // The player card rebuilds on it, as on a profile.
            snapshot.profiles_revision += 1;
        }
    }

    /// Send the achievement counts if they are waiting and due.
    fn send_counts(&mut self, now: Instant) -> Option<HubError> {
        if now < self.due_counts || !self.counts_waiting() {
            return None;
        }
        let hub = self.hub.as_mut()?;
        match hub.set_achievements(&self.identity, &self.counts) {
            Ok(achievements) => {
                set_achievements(&mut lock(&self.snapshot), achievements);
                self.counts_sent = Some(self.counts.clone());
                self.due_counts = now + ACHIEVEMENTS_EVERY;
                // Counts the hub held back wait an hour unless they change.
                if self.counts_waiting() {
                    self.due_counts = now + ACHIEVEMENTS_AGAIN;
                }
                None
            }
            Err(HubError::Rejected { status, .. }) if (400..500).contains(&status) => {
                // A refusal (a quota, an older hub without achievements) is not
                // retried soon; it is not the hub being down either.
                self.counts_sent = Some(self.counts.clone());
                self.due_counts = now + ACHIEVEMENTS_AGAIN;
                None
            }
            Err(failure) => {
                self.due_counts = now + self.backoff;
                Some(failure)
            }
        }
    }

    /// A new name, the claim, the roster read and the pending lookups, where due.
    fn run_due(&mut self, now: Instant) {
        let counts_error = self.send_counts(now);
        let Some(hub) = self.hub.as_mut() else { return };
        let mut error = None;
        // A name worn since registering joins the key's history at the hub (a
        // registration of a known key only adds the name). A failure waits and retries.
        if self.name != self.name_sent && now >= self.due_name {
            match hub.register(&self.identity, self.name.as_deref()) {
                Ok(profile) => {
                    self.name_sent = self.name.clone();
                    self.due_profile = now + PROFILE_EVERY;
                    lock(&self.snapshot).set_me(profile);
                }
                Err(failure) => {
                    self.due_name = now + self.backoff;
                    error = Some(failure);
                }
            }
        }
        // The own profile again from time to time: medals and the verified flag change
        // at the hub, not here.
        if now >= self.due_profile {
            match hub.profile(&self.identity.key_id()) {
                Ok(profile) => {
                    self.due_profile = now + PROFILE_EVERY;
                    lock(&self.snapshot).set_me(profile);
                }
                Err(failure) => {
                    self.due_profile = now + self.backoff;
                    error = Some(failure);
                }
            }
        }
        if let Some(location) = self.location.clone() {
            let server = location.server.to_string();
            if now >= self.due_claim {
                match hub.claim(
                    &self.identity,
                    &server,
                    location.slot,
                    &location.name,
                    self.active,
                ) {
                    Ok(()) => {
                        self.claimed = Some(server.clone());
                        self.due_claim = now + CLAIM_EVERY;
                        // Renewing the same claim keeps its look; another server,
                        // slot or name starts with none. With no claim accepted before
                        // (released, lapsed, failed or none yet), what the hub holds
                        // stays as it was known, or unknown.
                        let claim = (server.clone(), location.slot, location.name.clone());
                        if self.look_claim.as_ref() != Some(&claim)
                            && self.look_claim.replace(claim).is_some()
                        {
                            self.look_held = Held::none();
                        }
                    }
                    Err(failure) => {
                        self.due_claim = now + self.backoff;
                        // The claim may lapse meanwhile, and its look with it, or
                        // still be live with it: what the hub holds is not known.
                        self.look_claim = None;
                        self.look_held = Held::Unknown;
                        error = Some(failure);
                    }
                }
            }
            if now >= self.due_poll {
                match hub.presence(&server) {
                    Ok(players) => {
                        self.due_poll = now + POLL_EVERY;
                        let mut snapshot = lock(&self.snapshot);
                        if snapshot.players != players {
                            snapshot.players = players;
                            snapshot.revision += 1;
                        }
                    }
                    Err(failure) => {
                        self.due_poll = now + self.backoff;
                        error = Some(failure);
                    }
                }
            }
        }
        for key_id in std::mem::take(&mut self.lookups) {
            match hub.profile(&key_id) {
                Ok(profile) => {
                    let mut snapshot = lock(&self.snapshot);
                    snapshot.profiles.insert(profile.key_id.clone(), profile);
                    snapshot.profiles_revision += 1;
                }
                Err(failure) => {
                    lock(&self.snapshot).notice = Some(failure.to_string());
                }
            }
        }
        // Models and looks are cosmetic: they do not change the status.
        self.send_models(now);
        self.check_models();
        // A look is cosmetic: what became of it is in its outcome, not the status.
        self.send_look(now);
        match error.or(counts_error) {
            Some(failure) => {
                self.backoff = (self.backoff * 2).min(RETRY_MAX);
                self.update(|snapshot| snapshot.status = Status::Failed(failure.to_string()));
            }
            None => {
                self.backoff = RETRY_MIN;
                self.update(|snapshot| {
                    if matches!(snapshot.status, Status::Failed(_)) {
                        snapshot.status = Status::Online;
                    }
                });
            }
        }
    }
}

/// Put the hub's answer about the player's achievements into their profile.
fn set_achievements(snapshot: &mut Snapshot, achievements: Vec<Achievement>) {
    if let Some(me) = snapshot.me.as_mut() {
        me.achievements = achievements;
    }
}

/// The running service: commands go in, a [`Snapshot`] comes out.
pub struct Service {
    commands: Sender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    staff: Arc<Mutex<StaffState>>,
    chat: Arc<Mutex<ChatState>>,
    emotes: Arc<Mutex<VecDeque<Emote>>>,
    looks: Arc<Mutex<VecDeque<QueuedLook>>>,
    pings: Arc<Mutex<VecDeque<QueuedPing>>>,
    ping_outcome: Arc<Mutex<Option<ReportOutcome>>>,
    /// What the feed is told to read, for the looks' generation.
    feed: Arc<Mutex<FeedShared>>,
    /// Set to end the feed and vitals threads, which end after their request at the latest.
    feed_stop: Arc<std::sync::atomic::AtomicBool>,
    /// What the player shares on their nameplate, for the vitals thread.
    sharing: Arc<Mutex<Sharing>>,
    /// The sharers of the player's server, as the vitals thread last read them.
    vitals: Arc<Mutex<VitalsTable>>,
    finished: Mutex<Receiver<()>>,
    /// The last tag given to a note ([`Service::note`]).
    note_tags: std::sync::atomic::AtomicU64,
}

impl Service {
    /// Start the worker thread for `identity`. `make_hub` builds a hub client for
    /// the configured address. Without a feed, the SJK chat sends but never reads.
    pub fn start(identity: Identity, make_hub: HubFactory) -> Self {
        Self::start_with_feed(identity, make_hub, None)
    }

    /// [`Service::start`] with the SJK chat's feed on a thread of its own, reading the
    /// hub through clients `make_feed_hub` builds (with a timeout longer than the
    /// hub's wait, [`crate::feed::TIMEOUT`]).
    pub fn start_with_feed(
        identity: Identity,
        make_hub: HubFactory,
        make_feed_hub: Option<HubFactory>,
    ) -> Self {
        Self::start_with_threads(identity, make_hub, make_feed_hub, None)
    }

    /// [`Service::start_with_feed`] with shared vitals on a thread of their own too,
    /// reading the hub through clients `make_vitals_hub` builds (with a short
    /// timeout, [`crate::vitals::TIMEOUT`]).
    pub fn start_with_threads(
        identity: Identity,
        make_hub: HubFactory,
        make_feed_hub: Option<HubFactory>,
        make_vitals_hub: Option<HubFactory>,
    ) -> Self {
        let key_id = identity.key_id();
        let snapshot = Arc::new(Mutex::new(Snapshot::new(key_id.clone())));
        let staff = Arc::new(Mutex::new(StaffState::default()));
        let (commands, inbox) = channel();
        let (done, finished) = channel();
        let feed_identity = identity.clone();
        let vitals_identity = identity.clone();
        let mut worker = Worker::new(
            identity,
            make_hub,
            Arc::clone(&snapshot),
            Arc::clone(&staff),
            Instant::now(),
        );
        let chat = Arc::clone(&worker.chat);
        let ping_outcome = Arc::clone(&worker.ping_outcome);
        let feed = Arc::clone(&worker.feed);
        let own_drops = commands.clone();
        let emotes = Arc::new(Mutex::new(VecDeque::new()));
        let looks = Arc::new(Mutex::new(VecDeque::new()));
        let feed_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut pings = Arc::default();
        if let Some(make_feed_hub) = make_feed_hub {
            let feed = FeedWorker::new(
                feed_identity,
                make_feed_hub,
                Arc::clone(&worker.feed),
                Arc::clone(&chat),
                Arc::clone(&emotes),
                Arc::clone(&looks),
                Box::new(move || {
                    let _ = own_drops.send(Command::OwnDrop);
                }),
                Instant::now(),
            );
            pings = feed.pings();
            let stop = Arc::clone(&feed_stop);
            let spawned = std::thread::Builder::new()
                .name("sjk-hub-feed".to_owned())
                .spawn(move || crate::feed::run(feed, &stop));
            if let Err(error) = spawned {
                lock(&snapshot).status =
                    Status::Failed(format!("cannot start the chat thread: {error}"));
            }
        }
        let sharing = Arc::new(Mutex::new(Sharing::default()));
        let vitals = Arc::new(Mutex::new(VitalsTable::default()));
        if let Some(make_vitals_hub) = make_vitals_hub {
            let worker = VitalsWorker::new(
                vitals_identity,
                make_vitals_hub,
                Arc::clone(&worker.feed),
                Arc::clone(&sharing),
                Arc::clone(&vitals),
                Instant::now(),
            );
            let stop = Arc::clone(&feed_stop);
            let spawned = std::thread::Builder::new()
                .name("sjk-hub-vitals".to_owned())
                .spawn(move || crate::vitals::run(worker, &stop));
            if let Err(error) = spawned {
                lock(&snapshot).status =
                    Status::Failed(format!("cannot start the vitals thread: {error}"));
            }
        }
        let spawned = std::thread::Builder::new()
            .name("sjk-identity".to_owned())
            .spawn(move || {
                loop {
                    let wait = worker.tick(Instant::now());
                    match inbox.recv_timeout(wait) {
                        Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => {
                            worker.handle(Command::Stop, Instant::now());
                            break;
                        }
                        Ok(command) => worker.handle(command, Instant::now()),
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
                let _ = done.send(());
            });
        if let Err(error) = spawned {
            lock(&snapshot).status =
                Status::Failed(format!("cannot start the identity thread: {error}"));
        }
        Self {
            commands,
            snapshot,
            staff,
            chat,
            emotes,
            looks,
            pings,
            ping_outcome,
            feed,
            feed_stop,
            sharing,
            vitals,
            finished: Mutex::new(finished),
            note_tags: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Apply the player's settings; the service starts or stops talking to the hub.
    pub fn configure(&self, settings: Settings) {
        let _ = self.commands.send(Command::Configure(settings));
    }

    /// Keep the hub's asset packs in `directory` (the settings folder's
    /// [`crate::assets::FOLDER`], beside `identity.key`): once registered, at once and
    /// every 6 hours, the worker reads the hub's list and downloads each pack the folder
    /// does not hold as listed, checked against the list's size and SHA-256 and written
    /// in one rename. [`Snapshot::packs_revision`] counts the packs written. Until this
    /// is called no asset is asked for.
    pub fn keep_assets(&self, directory: PathBuf) {
        let _ = self.commands.send(Command::AssetsDir(directory));
    }

    /// Send the crash reports waiting in `directory` (`crate::crash`) once registered,
    /// and look there again every hour; [`Snapshot::crash_note`] says what went.
    /// `None` sends none from now on.
    pub fn send_crashes(&self, directory: Option<PathBuf>) {
        let _ = self.commands.send(Command::Crashes(directory));
    }

    /// Whether the player is actively playing right now: on a live game server, not
    /// spectating, not at the intermission, not in a menu, and with input in the last
    /// two minutes. Every claim says so, and the hub counts the time between claims
    /// toward the player's next holocron only while it does (`PROTOCOL.md`,
    /// "Holocrons"). Defaults to false, which earns nothing.
    pub fn set_active(&self, active: bool) {
        let _ = self.commands.send(Command::Active(active));
    }

    /// Ask for fresh holocron progress ([`Snapshot::holocrons`]); read at most once
    /// every 30 seconds, for a page that shows it as it opens.
    pub fn refresh_holocrons(&self) {
        let _ = self.commands.send(Command::RefreshHolocrons);
    }

    /// Open the player's holocrons `ids` (their own, not opened yet; at most
    /// [`crate::wire::OPEN_MAX`] are taken): what they brought arrives in
    /// [`Snapshot::opening`], and the profile after it.
    pub fn open_holocrons(&self, ids: Vec<u64>) {
        let _ = self.commands.send(Command::OpenHolocrons(ids));
    }

    /// Reveal the player's sealed cards whose rarity has cards at the hub: what they
    /// became arrives in [`Snapshot::reveal`], and the profile after it.
    pub fn reveal_cards(&self) {
        let _ = self.commands.send(Command::RevealCards);
    }

    /// The player is on a game server (call again if `slot` or `name` changes).
    pub fn enter(&self, location: Location) {
        let _ = self.commands.send(Command::Enter(location));
    }

    /// The player left their game server.
    pub fn leave(&self) {
        let _ = self.commands.send(Command::Leave);
    }

    /// The in-game name the player wears: sent with the registration and again
    /// whenever it changes, for the key's name history at the hub.
    pub fn set_name(&self, name: String) {
        let _ = self.commands.send(Command::Name(name));
    }

    /// Change the player's bio at the hub.
    pub fn set_bio(&self, bio: String) {
        let _ = self.commands.send(Command::SetBio(bio));
    }

    /// Make `png` (cropped and scaled to [`crate::avatar::SIZE`] square) the player's
    /// picture; the outcome arrives in [`Snapshot::avatar`], the new version in the
    /// profile's `avatar`.
    pub fn set_avatar(&self, png: Vec<u8>) {
        let _ = self.commands.send(Command::SetAvatar(png));
    }

    /// Take the player's picture down; the outcome arrives in [`Snapshot::avatar`].
    pub fn remove_avatar(&self) {
        let _ = self.commands.send(Command::RemoveAvatar);
    }

    /// The counts the client keeps for the achievements it counts, by id: sent to the
    /// hub once registered, at most once a minute, and again when the hub held part
    /// back. The hub's answer arrives in the profile's `achievements`.
    pub fn set_achievement_counts(&self, counts: BTreeMap<String, u64>) {
        let _ = self.commands.send(Command::Achievements(counts));
    }

    /// Send a staff request; its answer arrives in [`Service::staff_state`].
    pub fn staff(&self, request: StaffRequest) {
        self.staff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .busy = true;
        let _ = self.commands.send(Command::Staff(request));
    }

    /// Send a staff query about logs or crash reports; its answer arrives in
    /// [`Service::staff_state`], and a text opened in [`Service::take_opened`].
    pub fn staff_query(&self, query: StaffQuery) {
        self.staff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .busy = true;
        let _ = self.commands.send(Command::StaffQuery(query));
    }

    /// The logs or crash report a staff query opened, once.
    pub fn take_opened(&self) -> Option<OpenedText> {
        self.staff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .opened
            .take()
    }

    /// Answer the SJK team's log request `id`: send `upload`, or decline with `None`.
    /// The request leaves [`Snapshot::log_requests`] at once; the outcome arrives in
    /// [`Snapshot::log_answer`].
    pub fn answer_logs(&self, id: i64, upload: Option<LogUpload>) {
        lock(&self.snapshot)
            .log_requests
            .retain(|request| request.id != id);
        let _ = self.commands.send(Command::AnswerLogs(id, upload));
    }

    /// What staff requests brought back (without a text opened, which
    /// [`Service::take_opened`] hands over).
    pub fn staff_state(&self) -> StaffState {
        self.staff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Send a bug report; its outcome arrives in [`Snapshot::report`].
    pub fn report(&self, report: BugReport) {
        let _ = self.commands.send(Command::Report(report));
    }

    /// Send a report about another player; its outcome arrives in
    /// [`Snapshot::player_report`]. Only a verified key may.
    pub fn player_report(&self, report: PlayerReport) {
        let _ = self.commands.send(Command::PlayerReport(report));
    }

    /// Send a world note; its outcome arrives in [`Snapshot::note`]. The answer tags
    /// the note for [`Service::note_image`].
    pub fn note(&self, note: WorldNote) -> u64 {
        let tag = self
            .note_tags
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let _ = self.commands.send(Command::Note(tag, Box::new(note)));
        tag
    }

    /// The picture of the note tagged `tag`, a JPEG; sent once the hub took the note.
    pub fn note_image(&self, tag: u64, jpeg: Vec<u8>) {
        let _ = self.commands.send(Command::NoteImage(tag, jpeg));
    }

    /// Fetch a player's profile (their bio) into [`Snapshot::profiles`].
    pub fn look_up(&self, key_id: String) {
        let _ = self.commands.send(Command::LookUp(key_id));
    }

    /// The player model folders the game has (`PROTOCOL.md`, "Models"), sent to the
    /// hub once registered and again whenever the list changes. Names are lower case.
    pub fn set_models(&self, models: Vec<String>) {
        let _ = self.commands.send(Command::Models(models));
    }

    /// Ask the hub whether the player of `slot` on `server` has `model`; the answer
    /// comes in [`Snapshot::model_checks`].
    pub fn check_model(&self, server: SocketAddr, slot: u8, model: String) {
        let _ = self.commands.send(Command::CheckModel(server, slot, model));
    }

    /// A copy of the current state.
    pub fn snapshot(&self) -> Snapshot {
        lock(&self.snapshot).clone()
    }

    /// Read the current state without copying it; keep `read` short, the worker
    /// waits for the lock.
    pub fn with_snapshot<R>(&self, read: impl FnOnce(&Snapshot) -> R) -> R {
        read(&lock(&self.snapshot))
    }

    /// Turn the SJK chat on or off. Off, the feed still reads the hub while the
    /// player is on a game server (for its looks and emotes) but keeps no message; in
    /// the menus it reads only with the chat on.
    pub fn set_chat(&self, on: bool) {
        let _ = self.commands.send(Command::SetChat(on));
    }

    /// Send an SJK chat message; what became of it arrives in [`ChatState::outcome`].
    pub fn chat(&self, text: String) {
        let _ = self.commands.send(Command::Chat(text));
    }

    /// Play an emote for the player's slot on their server, in step with slot `sync`'s
    /// when it is set; what became of it arrives in [`ChatState::outcome`].
    pub fn emote(&self, emote: String, sync: Option<u8>) {
        let _ = self.commands.send(Command::Emote(emote, sync));
    }

    /// Read the chat without copying it; keep `read` short.
    pub fn with_chat<R>(&self, read: impl FnOnce(&ChatState) -> R) -> R {
        read(&lock_chat(&self.chat))
    }

    /// The emotes received since the last call, oldest first.
    pub fn take_emotes(&self) -> Vec<Emote> {
        crate::feed::lock(&self.emotes).drain(..).collect()
    }

    /// Mark `pos` (whole game units) for `scope` (`PROTOCOL.md`, "Pings") from the slot
    /// the player claims on their server; what became of it arrives in
    /// [`Service::ping_outcome`].
    pub fn ping(&self, pos: [i32; 3], scope: String) {
        let _ = self.commands.send(Command::Ping(pos, scope));
    }

    /// What became of the last ping sent; `None` before one.
    pub fn ping_outcome(&self) -> Option<ReportOutcome> {
        crate::feed::lock(&self.ping_outcome).clone()
    }

    /// The pings the feed received for `server`, the game server the player is on,
    /// since the last call, oldest first; those read for another server are dropped.
    pub fn take_pings(&self, server: Option<SocketAddr>) -> Vec<Ping> {
        crate::feed::take_pings(&self.pings, server)
    }

    /// The look the player wears (`PROTOCOL.md`, "Looks"). The worker keeps the
    /// latest and sends it once its claim on a server is accepted, again when it
    /// changes or the claim does (another server, slot or name), at most once a
    /// second; a blade skin the hub says the key does not hold is left out until the
    /// profile changes. What became of it arrives in [`Snapshot::look_outcome`].
    pub fn set_look(&self, look: Look) {
        let _ = self.commands.send(Command::Look(look));
    }

    /// The looks the feed received since the last call, oldest first: those it read
    /// for `server`, the game server the player is on, under the current reading. A
    /// poll still under way when the server changed or the identity went off brings
    /// none, and [`ReceivedLooks::generation`] tells when every look had before is out
    /// of date. The newest look of the player's own key goes to the worker, which reads
    /// the own profile again soon when the hub took a skin back.
    pub fn take_looks(&self, server: Option<SocketAddr>) -> ReceivedLooks {
        let received = crate::feed::take_looks(&self.feed, &self.looks, server);
        if received.events.is_empty() {
            return received;
        }
        // The player's own id follows the person this key belongs to.
        let own_id = lock(&self.snapshot).key_id.clone();
        if let Some(own) = received
            .events
            .iter()
            .rev()
            .find(|event| event.key_id == own_id)
        {
            let _ = self
                .commands
                .send(Command::OwnLook(own.id, own.saber.clone()));
        }
        received
    }

    /// What the player shares on their nameplate (`PROTOCOL.md`, "Vitals"); the vitals
    /// thread sends it with its next request, at once when `share` or `estimates`
    /// changed. Cheap: call it every frame.
    pub fn set_sharing(&self, sharing: Sharing) {
        *crate::feed::lock(&self.sharing) = sharing;
    }

    /// Read the sharers of the player's game server, as the hub last listed them.
    pub fn with_vitals<R>(&self, read: impl FnOnce(&VitalsTable) -> R) -> R {
        read(&crate::feed::lock(&self.vitals))
    }

    /// Withdraw the claim and stop, waiting at most `timeout` for it. The feed and
    /// vitals threads are told to stop and ends after their request, without being waited for.
    pub fn shutdown(&self, timeout: Duration) {
        self.feed_stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.commands.send(Command::Stop);
        let finished = self
            .finished
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = finished.recv_timeout(timeout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{FOLDER, Pack, sha256_hex, write_pack};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A hub that records what it is asked.
    #[derive(Clone, Default)]
    struct Fake {
        log: Arc<Mutex<Vec<String>>>,
        fail: Arc<AtomicBool>,
        /// Refuses an emote's `sync`, as a hub from before 15/10/2026 does.
        old_hub: Arc<AtomicBool>,
        roster: Arc<Mutex<Vec<Presence>>>,
        /// The most of any count the fake hub takes, as its hourly allowance would.
        cap: Arc<Mutex<Option<u64>>>,
        /// The achievements the fake hub holds, as its profiles list them.
        held: Arc<Mutex<Vec<Achievement>>>,
        /// The unlocks the fake hub holds for the player's key.
        owned: Arc<Mutex<Vec<String>>>,
        /// Refusals the next looks get, in order.
        look_refusals: Arc<Mutex<VecDeque<HubError>>>,
        /// The asset packs the fake hub serves, by name.
        packs: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
        /// Bytes every pack download gets instead of the pack's own.
        served: Arc<Mutex<Option<Vec<u8>>>>,
        /// Refusals the next readings of the asset list get, in order.
        asset_refusals: Arc<Mutex<VecDeque<HubError>>>,
        /// The holocron progress the fake hub reports; none is a hub from before
        /// holocrons, which refuses the reading.
        progress: Arc<Mutex<Option<HolocronState>>>,
        /// How many times the holocron progress was asked for.
        holocron_reads: Arc<Mutex<usize>>,
        /// The open log requests the fake hub lists; none is a hub from before log
        /// requests, which refuses the reading.
        log_asks: Arc<Mutex<Option<Vec<LogRequest>>>>,
        /// How many times the log requests were looked for.
        log_ask_reads: Arc<Mutex<usize>>,
    }

    impl Fake {
        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }

        fn unlocks(&self) -> Vec<Unlock> {
            self.owned
                .lock()
                .unwrap()
                .iter()
                .map(|id| Unlock {
                    id: id.clone(),
                    granted: 1,
                    note: String::new(),
                    count: 1,
                    medal: None,
                })
                .collect()
        }

        /// The asset list readings and pack downloads asked for.
        fn asset_requests(&self) -> Vec<String> {
            self.log()
                .into_iter()
                .filter(|line| line.starts_with("asset"))
                .collect()
        }

        fn serve(&self, name: &str, bytes: &[u8]) {
            self.packs
                .lock()
                .unwrap()
                .insert(name.to_owned(), bytes.to_vec());
        }

        fn looks(&self) -> Vec<String> {
            self.log()
                .into_iter()
                .filter(|line| line.starts_with("look "))
                .collect()
        }

        fn record(&self, line: String) -> Result<(), HubError> {
            self.log.lock().unwrap().push(line);
            if self.fail.load(Ordering::SeqCst) {
                Err(HubError::Network("down".to_owned()))
            } else {
                Ok(())
            }
        }
    }

    fn profile(name: &str) -> Profile {
        Profile {
            key_id: "0123456789abcdef".to_owned(),
            key: String::new(),
            name: name.to_owned(),
            bio: String::new(),
            verified: false,
            staff: true,
            created: 0,
            names: Vec::new(),
            medals: Vec::new(),
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: crate::wire::HolocronCounts::default(),
            holocrons: Vec::new(),
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
        }
    }

    impl Hub for Fake {
        fn register(&mut self, _: &Identity, name: Option<&str>) -> Result<Profile, HubError> {
            let unlocks = self.unlocks();
            match name {
                Some(name) => self.record(format!("register {name}")).map(|()| Profile {
                    unlocks,
                    ..profile(name)
                }),
                None => self.record("register".to_owned()).map(|()| Profile {
                    unlocks,
                    ..profile("")
                }),
            }
        }
        fn set_bio(&mut self, _: &Identity, bio: &str) -> Result<Profile, HubError> {
            self.record(format!("bio {bio}")).map(|()| profile(""))
        }
        fn set_avatar(&mut self, _: &Identity, png: &[u8]) -> Result<Profile, HubError> {
            Fake::record(self, format!("avatar {} bytes", png.len())).map(|()| Profile {
                avatar: "0123456789abcdef".to_owned(),
                ..profile("Sol")
            })
        }
        fn remove_avatar(&mut self, _: &Identity) -> Result<Profile, HubError> {
            Fake::record(self, "avatar removed".to_owned()).map(|()| profile("Sol"))
        }
        fn profile(&mut self, key_id: &str) -> Result<Profile, HubError> {
            self.record(format!("lookup {key_id}")).map(|()| Profile {
                achievements: self.held.lock().unwrap().clone(),
                unlocks: self.unlocks(),
                ..profile("Other")
            })
        }
        fn claim(
            &mut self,
            _: &Identity,
            server: &str,
            slot: u8,
            name: &str,
            active: bool,
        ) -> Result<(), HubError> {
            let tail = if active { " active" } else { "" };
            self.record(format!("claim {server} {slot} {name}{tail}"))
        }
        fn open_holocrons(
            &mut self,
            _: &Identity,
            ids: &[u64],
        ) -> Result<crate::wire::OpenAnswer, HubError> {
            let list: Vec<String> = ids.iter().map(u64::to_string).collect();
            self.record(format!("open {}", list.join(",")))?;
            if ids.contains(&99) {
                return Err(refused(409, "holocron_opened"));
            }
            let opened: Vec<crate::wire::Opened> = ids
                .iter()
                .map(|&id| crate::wire::Opened {
                    holocron: id,
                    tier: "rare".to_owned(),
                    card: Some(crate::wire::OpenedCard {
                        id: 1_000 + id,
                        card: None,
                        rarity: "common".to_owned(),
                        finish: "holo".to_owned(),
                        edition: "alpha".to_owned(),
                    }),
                    credits: 40,
                    unlock: None,
                })
                .collect();
            let mut profile = profile("Sol");
            profile.credits = 40 * ids.len() as u64;
            Ok(crate::wire::OpenAnswer { opened, profile })
        }
        fn reveal_cards(&mut self, _: &Identity) -> Result<crate::wire::RevealAnswer, HubError> {
            self.record("reveal".to_owned())?;
            Ok(crate::wire::RevealAnswer {
                revealed: vec![crate::wire::Revealed {
                    id: 1_007,
                    card: "sol_lore_001".to_owned(),
                    rarity: "legendary".to_owned(),
                    finish: "holo".to_owned(),
                    edition: "alpha".to_owned(),
                }],
                profile: profile("Sol"),
            })
        }
        fn holocrons(&mut self, _: &Identity) -> Result<HolocronState, HubError> {
            // Counted apart from the log, which the other tests read whole.
            *self.holocron_reads.lock().unwrap() += 1;
            if self.fail.load(Ordering::SeqCst) {
                return Err(HubError::Network("down".to_owned()));
            }
            let state = self.progress.lock().unwrap().clone();
            state.ok_or_else(|| refused(404, "not_found"))
        }
        fn log_requests(&mut self, _: &Identity) -> Result<Vec<LogRequest>, HubError> {
            // Counted apart from the log, which the other tests read whole.
            *self.log_ask_reads.lock().unwrap() += 1;
            if self.fail.load(Ordering::SeqCst) {
                return Err(HubError::Network("down".to_owned()));
            }
            let asks = self.log_asks.lock().unwrap().clone();
            asks.ok_or_else(|| refused(404, "not_found"))
        }
        fn send_logs(&mut self, _: &Identity, id: i64, upload: &LogUpload) -> Result<(), HubError> {
            self.record(format!("logs {id} {} {}", upload.build, upload.logs))?;
            if let Some(asks) = self.log_asks.lock().unwrap().as_mut() {
                asks.retain(|ask| ask.id != id);
            }
            Ok(())
        }
        fn decline_logs(&mut self, _: &Identity, id: i64) -> Result<(), HubError> {
            self.record(format!("decline {id}"))?;
            if let Some(asks) = self.log_asks.lock().unwrap().as_mut() {
                asks.retain(|ask| ask.id != id);
            }
            Ok(())
        }
        fn staff_query(
            &mut self,
            _: &Identity,
            query: &StaffQuery,
        ) -> Result<crate::staff::StaffAnswer, HubError> {
            use crate::logs::LogRequestRow;
            use crate::staff::StaffAnswer;
            self.record(format!("staff {query:?}"))?;
            Ok(match query {
                StaffQuery::RequestLogs { key_id, note } => {
                    StaffAnswer::LogRequest(Box::new(LogRequestRow {
                        id: 12,
                        key_id: key_id.clone(),
                        name: "Friend".into(),
                        note: note.clone(),
                        state: "open".into(),
                        ..LogRequestRow::default()
                    }))
                }
                StaffQuery::Log(id) => StaffAnswer::LogRequest(Box::new(LogRequestRow {
                    id: *id,
                    name: "Friend".into(),
                    state: "sent".into(),
                    logs: "the lines".into(),
                    ..LogRequestRow::default()
                })),
                StaffQuery::Crash(_) => return Err(refused(404, "not_found")),
                _ => StaffAnswer::LogRequests(Vec::new()),
            })
        }
        fn release(&mut self, _: &Identity, server: &str) -> Result<(), HubError> {
            self.record(format!("release {server}"))
        }
        fn set_models(&mut self, _: &Identity, models: &[String]) -> Result<u64, HubError> {
            self.record(format!("models {}", models.join(",")))
                .map(|()| models.len() as u64)
        }
        fn model_check(
            &mut self,
            server: &str,
            slot: u8,
            model: &str,
        ) -> Result<(String, Option<bool>), HubError> {
            self.record(format!("model {server} {slot} {model}"))
                .map(|()| ("^2Sol".to_owned(), Some(model == "kyle")))
        }
        fn presence(&mut self, server: &str) -> Result<Vec<Presence>, HubError> {
            self.record(format!("presence {server}"))
                .map(|()| self.roster.lock().unwrap().clone())
        }
        fn report(&mut self, _: &Identity, report: &BugReport) -> Result<i64, HubError> {
            self.record(format!("report {}", report.text))?;
            self.record(format!("report name {}", report.name))
                .map(|()| 7)
        }
        fn player_report(&mut self, _: &Identity, report: &PlayerReport) -> Result<i64, HubError> {
            Fake::record(
                self,
                format!(
                    "player {} {} {} by {}",
                    report.category.code(),
                    report.slot,
                    report.target_name,
                    report.name
                ),
            )
            .map(|()| 11)
        }
        fn note(&mut self, _: &Identity, note: &WorldNote) -> Result<i64, HubError> {
            Fake::record(self, format!("note {} {}", note.text, note.shader))?;
            Fake::record(self, format!("note name {}", note.name)).map(|()| 9)
        }
        fn note_image(&mut self, _: &Identity, id: i64, jpeg: &[u8]) -> Result<(), HubError> {
            Fake::record(self, format!("image {id} {} bytes", jpeg.len()))
        }
        fn chat(&mut self, _: &Identity, text: &str, name: &str) -> Result<u64, HubError> {
            Fake::record(self, format!("chat {text} as {name}")).map(|()| 12)
        }
        fn emote(
            &mut self,
            _: &Identity,
            server: &str,
            emote: &str,
            sync: Option<u8>,
        ) -> Result<u64, HubError> {
            if sync.is_some() && self.old_hub.load(Ordering::SeqCst) {
                return Err(HubError::Rejected {
                    status: 400,
                    code: "bad_body".to_owned(),
                    message: "unknown field `sync`".to_owned(),
                });
            }
            let with = sync.map_or_else(String::new, |slot| format!(" with {slot}"));
            Fake::record(self, format!("emote {emote} on {server}{with}")).map(|()| 13)
        }
        fn ping(
            &mut self,
            _: &Identity,
            server: &str,
            pos: [i32; 3],
            scope: &str,
        ) -> Result<u64, HubError> {
            Fake::record(self, format!("ping {pos:?} {scope} on {server}")).map(|()| 14)
        }
        fn look(&mut self, _: &Identity, server: &str, look: &Look) -> Result<u64, HubError> {
            Fake::record(
                self,
                format!("look {server} {:?} {}", look.saber, look.illuminate),
            )?;
            if let Some(refusal) = self.look_refusals.lock().unwrap().pop_front() {
                return Err(refusal);
            }
            if !look.saber.is_empty() && !self.owned.lock().unwrap().contains(&look.saber) {
                return Err(HubError::Rejected {
                    status: 403,
                    code: "not_unlocked".to_owned(),
                    message: "this key does not hold that unlock".to_owned(),
                });
            }
            Ok(15)
        }
        fn assets(&mut self, _: &Identity) -> Result<Vec<Pack>, HubError> {
            Fake::record(self, "assets".to_owned())?;
            if let Some(refusal) = self.asset_refusals.lock().unwrap().pop_front() {
                return Err(refusal);
            }
            let packs = self.packs.lock().unwrap();
            Ok(packs
                .iter()
                .map(|(name, bytes)| Pack {
                    name: name.clone(),
                    size: bytes.len() as u64,
                    sha256: sha256_hex(bytes),
                })
                .collect())
        }
        fn asset(&mut self, _: &Identity, name: &str) -> Result<Vec<u8>, HubError> {
            Fake::record(self, format!("asset {name}"))?;
            if let Some(bytes) = self.served.lock().unwrap().clone() {
                return Ok(bytes);
            }
            self.packs
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .ok_or_else(|| refused(404, "not_found"))
        }
        fn set_achievements(
            &mut self,
            _: &Identity,
            progress: &BTreeMap<String, u64>,
        ) -> Result<Vec<Achievement>, HubError> {
            let line: Vec<String> = progress.iter().map(|(id, n)| format!("{id}={n}")).collect();
            Fake::record(self, format!("achievements {}", line.join(",")))?;
            let cap = *self.cap.lock().unwrap();
            let held: Vec<Achievement> = progress
                .iter()
                .map(|(id, &count)| Achievement {
                    id: id.clone(),
                    progress: cap.map_or(count, |cap| count.min(cap)),
                    goal: 100,
                    unlocked: 0,
                })
                .collect();
            *self.held.lock().unwrap() = held.clone();
            Ok(held)
        }
        fn staff(
            &mut self,
            _: &Identity,
            request: &StaffRequest,
        ) -> Result<Vec<Profile>, HubError> {
            Fake::record(self, format!("staff {request:?}"))?;
            Ok(match request {
                StaffRequest::Search(_) => vec![profile("Found"), profile("Other")],
                StaffRequest::Award { key_id, medal, .. } => vec![Profile {
                    key_id: key_id.clone(),
                    medals: vec![crate::wire::Medal {
                        id: medal.clone(),
                        count: 1,
                        awarded: 1,
                        note: String::new(),
                    }],
                    ..profile("Target")
                }],
                StaffRequest::Unlock { key_id, unlock, .. } => {
                    self.owned.lock().unwrap().push(unlock.clone());
                    vec![Profile {
                        key_id: key_id.clone(),
                        unlocks: self.unlocks(),
                        ..profile("Target")
                    }]
                }
                StaffRequest::Relock { key_id, unlock } => {
                    self.owned.lock().unwrap().retain(|id| id != unlock);
                    vec![Profile {
                        key_id: key_id.clone(),
                        unlocks: self.unlocks(),
                        ..profile("Target")
                    }]
                }
                StaffRequest::HolocronGive { key_id, tier, note } => vec![Profile {
                    key_id: key_id.clone(),
                    holocron_counts: crate::wire::HolocronCounts {
                        legendary: u32::from(tier == "legendary"),
                        ..Default::default()
                    },
                    holocrons: vec![crate::wire::Holocron {
                        id: 41,
                        tier: tier.clone(),
                        dropped: 1,
                        source: "staff".to_owned(),
                        note: note.clone(),
                        opened: None,
                    }],
                    ..profile("Target")
                }],
                StaffRequest::HolocronRemove { key_id, .. } => vec![Profile {
                    key_id: key_id.clone(),
                    ..profile("Target")
                }],
                StaffRequest::Unaward { key_id, .. }
                | StaffRequest::ClearAchievements { key_id, .. }
                | StaffRequest::AvatarRemove { key_id }
                | StaffRequest::AvatarBlock { key_id, .. } => vec![Profile {
                    key_id: key_id.clone(),
                    ..profile("Target")
                }],
                StaffRequest::ChatDelete { .. } | StaffRequest::ChatMute { .. } => Vec::new(),
                StaffRequest::Verify { key_id, verified } => vec![Profile {
                    key_id: key_id.clone(),
                    verified: *verified,
                    ..profile("Target")
                }],
                // The kept person, with the merged-away key linked to it.
                StaffRequest::Merge { key_id, from } => vec![Profile {
                    key_id: key_id.clone(),
                    keys: vec![key_id.clone(), from.clone()],
                    ..profile("Target")
                }],
                // The person the key was linked to, without it.
                StaffRequest::Unlink { .. } => vec![Profile {
                    keys: vec!["0123456789abcdef".to_owned()],
                    ..profile("Sol")
                }],
            })
        }
    }

    fn worker(fake: &Fake, now: Instant) -> (Worker, Arc<Mutex<Snapshot>>) {
        let identity = Identity::from_seed([1; 32]);
        let snapshot = Arc::new(Mutex::new(Snapshot::new(identity.key_id())));
        let factory = fake.clone();
        let make: HubFactory = Box::new(move |url| {
            if url.starts_with("https://") {
                Ok(Box::new(factory.clone()))
            } else {
                Err(HubError::Network("bad address".to_owned()))
            }
        });
        (
            Worker::new(
                identity,
                make,
                Arc::clone(&snapshot),
                Arc::new(Mutex::new(StaffState::default())),
                now,
            ),
            snapshot,
        )
    }

    fn on(url: &str) -> Settings {
        Settings {
            enabled: true,
            hub_url: url.to_owned(),
        }
    }

    fn here(slot: u8, name: &str) -> Location {
        Location {
            server: "1.2.3.4:29070".parse().unwrap(),
            slot,
            name: name.to_owned(),
        }
    }

    fn counts(pairs: &[(&str, u64)]) -> BTreeMap<String, u64> {
        pairs.iter().map(|(id, n)| ((*id).to_owned(), *n)).collect()
    }

    fn sent_counts(fake: &Fake) -> Vec<String> {
        fake.log()
            .into_iter()
            .filter(|line| line.starts_with("achievements"))
            .collect()
    }

    fn holocron_reads(fake: &Fake) -> usize {
        *fake.holocron_reads.lock().unwrap()
    }

    fn progress(secs: u64) -> HolocronState {
        HolocronState {
            progress_secs: secs,
            every_secs: 1_800,
            today: 2,
            daily_cap: 8,
            server_time: 1_791_000_000,
            can_open: true,
            loot: Vec::new(),
        }
    }

    fn claims(fake: &Fake) -> Vec<String> {
        fake.log()
            .into_iter()
            .filter(|line| line.starts_with("claim"))
            .collect()
    }

    #[test]
    fn claims_say_active_only_while_the_player_is() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        let at = |seconds| t0 + Duration::from_secs(seconds);
        // Telling the service sends nothing by itself; the next claim carries it.
        worker.handle(Command::Active(true), at(10));
        assert_eq!(claims(&fake).len(), 1);
        worker.tick(at(46));
        worker.handle(Command::Active(false), at(50));
        worker.tick(at(91));
        worker.handle(Command::Active(true), at(100));
        worker.tick(at(136));
        assert_eq!(
            claims(&fake),
            [
                "claim 1.2.3.4:29070 3 Sol",
                "claim 1.2.3.4:29070 3 Sol active",
                "claim 1.2.3.4:29070 3 Sol",
                "claim 1.2.3.4:29070 3 Sol active",
            ]
        );
    }

    #[test]
    fn holocron_progress_is_read_after_registering_and_every_few_minutes() {
        let fake = Fake::default();
        *fake.progress.lock().unwrap() = Some(progress(600));
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        assert!(lock(&snapshot).holocrons.is_none());
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!(holocron_reads(&fake), 1);
        let read = lock(&snapshot).holocrons.expect("the progress");
        assert_eq!(
            (
                read.progress_secs,
                read.every_secs,
                read.today,
                read.daily_cap
            ),
            (600, 1_800, 2, 8)
        );
        assert_eq!(read.remaining_secs(), 1_200);
        assert!(!read.capped());
        assert_eq!(read.server_time, 1_791_000_000);
        worker.tick(t0 + HOLOCRONS_EVERY - Duration::from_secs(1));
        assert_eq!(holocron_reads(&fake), 1, "not yet");
        *fake.progress.lock().unwrap() = Some(HolocronState {
            today: 8,
            ..progress(1_790)
        });
        worker.tick(t0 + HOLOCRONS_EVERY);
        assert_eq!(holocron_reads(&fake), 2);
        let again = lock(&snapshot).holocrons.expect("the progress");
        assert_eq!(again.remaining_secs(), 10);
        assert!(again.capped(), "8 of 8 today");
        assert_eq!(lock(&snapshot).status, Status::Online);
    }

    #[test]
    fn a_hub_without_holocrons_is_asked_again_in_an_hour_and_the_status_stays() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!(holocron_reads(&fake), 1);
        assert!(lock(&snapshot).holocrons.is_none());
        assert_eq!(lock(&snapshot).status, Status::Online);
        worker.tick(t0 + HOLOCRONS_AGAIN - Duration::from_secs(1));
        assert_eq!(holocron_reads(&fake), 1);
        // Asking for it (a page opening) reads it now, once the half minute is out.
        worker.handle(Command::RefreshHolocrons, t0 + Duration::from_secs(10));
        worker.tick(t0 + Duration::from_secs(10));
        assert_eq!(holocron_reads(&fake), 2);
        *fake.progress.lock().unwrap() = Some(progress(0));
        worker.tick(t0 + Duration::from_secs(10) + HOLOCRONS_AGAIN);
        assert!(lock(&snapshot).holocrons.is_some());
    }

    #[test]
    fn a_failing_hub_is_asked_again_soon_without_a_failed_status_from_the_reading() {
        let fake = Fake::default();
        *fake.progress.lock().unwrap() = Some(progress(0));
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!(holocron_reads(&fake), 1);
        fake.fail.store(true, Ordering::SeqCst);
        worker.tick(t0 + HOLOCRONS_EVERY);
        let reads = holocron_reads(&fake);
        assert_eq!(reads, 2);
        worker.tick(t0 + HOLOCRONS_EVERY + Duration::from_secs(1));
        assert_eq!(holocron_reads(&fake), reads, "waits for the retry");
        fake.fail.store(false, Ordering::SeqCst);
        worker.tick(t0 + HOLOCRONS_EVERY + RETRY_MAX);
        assert_eq!(holocron_reads(&fake), reads + 1);
        assert!(lock(&snapshot).holocrons.is_some());
    }

    #[test]
    fn a_drop_of_the_own_key_reads_the_profile_and_progress_soon_at_most_twice_a_minute() {
        let fake = Fake::default();
        *fake.progress.lock().unwrap() = Some(progress(0));
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!((profile_reads(&fake), holocron_reads(&fake)), (0, 1));
        let at = |seconds| t0 + Duration::from_secs(seconds);
        worker.handle(Command::OwnDrop, at(5));
        worker.tick(at(5));
        assert_eq!((profile_reads(&fake), holocron_reads(&fake)), (1, 2));
        // Another within the half minute waits for it.
        worker.handle(Command::OwnDrop, at(15));
        worker.tick(at(15));
        worker.tick(at(34));
        assert_eq!((profile_reads(&fake), holocron_reads(&fake)), (1, 2));
        worker.tick(at(35));
        assert_eq!((profile_reads(&fake), holocron_reads(&fake)), (2, 3));
        // Asking for the progress alone is throttled the same way.
        worker.handle(Command::RefreshHolocrons, at(40));
        worker.tick(at(40));
        assert_eq!(holocron_reads(&fake), 3);
        worker.tick(at(65));
        assert_eq!(holocron_reads(&fake), 4);
        assert_eq!(
            profile_reads(&fake),
            2,
            "a refresh is not a profile reading"
        );
    }

    #[test]
    fn log_requests_are_listed_answered_once_and_older_hubs_left_alone() {
        let fake = Fake::default();
        let ask = |id| LogRequest {
            id,
            from: "Sol".into(),
            note: "the freeze".into(),
            created: 1,
        };
        *fake.log_asks.lock().unwrap() = Some(vec![ask(3), ask(4)]);
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!(*fake.log_ask_reads.lock().unwrap(), 1);
        assert_eq!(lock(&snapshot).log_requests, vec![ask(3), ask(4)]);
        // Sent: it leaves the list, and the hub hears the logs fitted.
        worker.handle(
            Command::AnswerLogs(
                3,
                Some(LogUpload {
                    logs: "line\r\nnext\u{0}".into(),
                    build: "2026.1011.2".into(),
                    ..LogUpload::default()
                }),
            ),
            t0,
        );
        assert!(
            fake.log()
                .contains(&"logs 3 2026.1011.2 line\nnext".to_owned())
        );
        assert_eq!(lock(&snapshot).log_requests, vec![ask(4)]);
        assert_eq!(
            lock(&snapshot).log_answer.clone().unwrap().message,
            "logs sent"
        );
        // Declined, then a listing that still has it (read before the answer) keeps it out.
        worker.handle(Command::AnswerLogs(4, None), t0);
        assert!(fake.log().contains(&"decline 4".to_owned()));
        fake.log_asks.lock().unwrap().as_mut().unwrap().push(ask(4));
        worker.tick(t0 + LOG_REQUESTS_EVERY);
        assert!(lock(&snapshot).log_requests.is_empty());
        // A new one shows at the next look.
        fake.log_asks.lock().unwrap().as_mut().unwrap().push(ask(5));
        worker.tick(t0 + LOG_REQUESTS_EVERY * 2);
        assert_eq!(lock(&snapshot).log_requests, vec![ask(5)]);
        // A hub from before log requests is asked again hours later only.
        *fake.log_asks.lock().unwrap() = None;
        let t1 = t0 + LOG_REQUESTS_EVERY * 3;
        worker.tick(t1);
        let reads = *fake.log_ask_reads.lock().unwrap();
        worker.tick(t1 + LOG_REQUESTS_EVERY);
        assert_eq!(*fake.log_ask_reads.lock().unwrap(), reads);
        worker.tick(t1 + LOG_REQUESTS_AGAIN);
        assert_eq!(*fake.log_ask_reads.lock().unwrap(), reads + 1);
    }

    #[test]
    fn an_answer_the_hub_could_not_hear_comes_back_to_answer_again() {
        let fake = Fake::default();
        let ask = LogRequest {
            id: 8,
            from: "Sol".into(),
            note: String::new(),
            created: 1,
        };
        *fake.log_asks.lock().unwrap() = Some(vec![ask.clone()]);
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        fake.fail.store(true, Ordering::SeqCst);
        worker.handle(Command::AnswerLogs(8, None), t0);
        assert!(!lock(&snapshot).log_answer.clone().unwrap().sent);
        assert!(lock(&snapshot).log_requests.is_empty());
        fake.fail.store(false, Ordering::SeqCst);
        worker.tick(t0 + RETRY_MAX + LOG_REQUESTS_EVERY);
        assert_eq!(lock(&snapshot).log_requests, vec![ask]);
    }

    #[test]
    fn staff_queries_go_for_staff_only_and_opened_logs_are_handed_over() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        let staff = Arc::clone(&worker.staff);
        worker.handle(
            Command::StaffQuery(StaffQuery::RequestLogs {
                key_id: "0123456789abcdef".into(),
                note: "your freeze".into(),
            }),
            t0,
        );
        assert_eq!(staff.lock().unwrap().message, "Asked Friend for their logs");
        assert_eq!(staff.lock().unwrap().log_requests[0].id, 12);
        worker.handle(Command::StaffQuery(StaffQuery::Log(12)), t0);
        let opened = staff.lock().unwrap().opened.take().unwrap();
        assert_eq!(opened.text, "the lines");
        assert_eq!(staff.lock().unwrap().log_requests[0].state, "sent");
        worker.handle(Command::StaffQuery(StaffQuery::Crash(1)), t0);
        assert!(staff.lock().unwrap().failed);
        assert_eq!(staff.lock().unwrap().message, "The hub no longer has it");
        // A plain key is refused here, before the hub hears it.
        lock(&snapshot).me.as_mut().unwrap().staff = false;
        let before = fake.log().len();
        worker.handle(Command::StaffQuery(StaffQuery::CrashGroups), t0);
        assert_eq!(fake.log().len(), before);
        assert_eq!(staff.lock().unwrap().message, "only SJK staff can do this");
    }

    #[test]
    fn staff_give_and_remove_a_holocron_and_the_answer_replaces_the_profile() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        let staff = Arc::clone(&worker.staff);
        let me = lock(&snapshot).me.clone().unwrap().key_id;
        worker.handle(
            Command::Staff(StaffRequest::HolocronGive {
                key_id: me.clone(),
                tier: "legendary".into(),
                note: "For the fog bug".into(),
            }),
            t0,
        );
        assert_eq!(staff.lock().unwrap().message, "Gave a legendary holocron");
        let own = lock(&snapshot).me.clone().unwrap();
        assert_eq!(own.holocron_counts.legendary, 1);
        assert_eq!(own.holocrons[0].note, "For the fog bug");
        assert_eq!(staff.lock().unwrap().players[0].holocrons[0].id, 41);
        worker.handle(
            Command::Staff(StaffRequest::HolocronRemove { key_id: me, id: 41 }),
            t0,
        );
        assert_eq!(staff.lock().unwrap().message, "Took back holocron #41");
        assert!(lock(&snapshot).me.clone().unwrap().holocrons.is_empty());
        // Only staff may: a plain key is refused here, before the hub hears it.
        lock(&snapshot).me.as_mut().unwrap().staff = false;
        let before = fake.log().len();
        worker.handle(
            Command::Staff(StaffRequest::HolocronGive {
                key_id: "0123456789abcdef".into(),
                tier: "rare".into(),
                note: String::new(),
            }),
            t0,
        );
        assert_eq!(fake.log().len(), before);
        assert!(staff.lock().unwrap().failed);
    }

    fn chat_outcome(worker: &Worker) -> ReportOutcome {
        lock_chat(&worker.chat).outcome.clone().unwrap()
    }

    #[test]
    fn a_hub_without_pictures_is_said_in_words() {
        let older = older_hub(HubError::Protocol("status 404, not JSON".to_owned()));
        assert!(
            matches!(&older, HubError::Rejected { message, .. } if message.contains("does not take pictures")),
            "{older:?}"
        );
        assert!(matches!(
            older_hub(HubError::Protocol("status 405, not JSON".to_owned())),
            HubError::Rejected { .. }
        ));
        // Other answers keep what they say.
        assert!(matches!(
            older_hub(HubError::Protocol("status 500, not JSON".to_owned())),
            HubError::Protocol(_)
        ));
        assert!(matches!(
            older_hub(HubError::Network("down".to_owned())),
            HubError::Network(_)
        ));
    }

    /// Revealing waits for the registration, then keeps what the copies became and the
    /// profile after; each reveal counts up.
    #[test]
    fn sealed_cards_reveal_once_registered() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::RevealCards, t0);
        let first = lock(&snapshot).reveal.clone().unwrap();
        assert!(first.revealed.is_empty() && first.message.contains("not connected"));
        assert!(
            fake.log().is_empty(),
            "nothing goes before the registration"
        );
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::RevealCards, t0);
        let revealed = lock(&snapshot).reveal.clone().unwrap();
        assert_eq!(revealed.serial, 2);
        assert!(revealed.message.is_empty());
        assert_eq!(revealed.revealed[0].card, "sol_lore_001");
        assert!(fake.log().iter().any(|line| line == "reveal"));
    }

    /// Opening waits for the registration, sends at most twenty ids, keeps what they
    /// brought and the profile after; a refusal says why in words and reads the profile
    /// again soon.
    #[test]
    fn holocrons_open_once_registered_and_what_they_brought_is_kept() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::OpenHolocrons(vec![7]), t0);
        let first = lock(&snapshot).opening.clone().unwrap();
        assert!(first.opened.is_empty() && first.message.contains("not connected"));
        assert!(
            fake.log().is_empty(),
            "nothing goes before the registration"
        );
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        let ids: Vec<u64> = (1..=25).collect();
        worker.handle(Command::OpenHolocrons(ids), t0);
        let opened = lock(&snapshot).opening.clone().unwrap();
        assert_eq!(opened.serial, 2);
        assert!(opened.message.is_empty());
        assert_eq!(opened.opened.len(), crate::wire::OPEN_MAX, "twenty at most");
        assert_eq!(opened.opened[0].card.as_ref().unwrap().id, 1_001);
        assert_eq!(lock(&snapshot).me.as_ref().unwrap().credits, 800);
        assert!(
            fake.log()
                .iter()
                .any(|line| line == "open 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20")
        );
        worker.handle(Command::OpenHolocrons(vec![99]), t0);
        let refused = lock(&snapshot).opening.clone().unwrap();
        assert_eq!(refused.serial, 3);
        assert!(refused.opened.is_empty());
        assert_eq!(refused.message, "That holocron was already opened.");
        assert!(
            worker.due_profile <= t0 + PROFILE_SOON,
            "the profile is read again soon"
        );
        worker.handle(Command::OpenHolocrons(Vec::new()), t0);
        assert_eq!(
            lock(&snapshot).opening.clone().unwrap().message,
            "there is no holocron to open"
        );
    }

    #[test]
    fn a_picture_is_sent_once_registered_and_its_profile_kept() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::SetAvatar(vec![1; 300]), t0);
        let first = lock(&snapshot).avatar.clone().unwrap();
        assert!(!first.sent && first.message.contains("not connected"));
        assert!(
            fake.log().is_empty(),
            "nothing goes before the registration"
        );
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::SetAvatar(vec![1; 300]), t0);
        let saved = lock(&snapshot).avatar.clone().unwrap();
        assert_eq!(
            (saved.serial, saved.sent, saved.message.as_str()),
            (2, true, "saved")
        );
        assert_eq!(
            lock(&snapshot).me.as_ref().unwrap().avatar,
            "0123456789abcdef"
        );
        // Larger than the hub takes: refused here, nothing sent.
        worker.handle(
            Command::SetAvatar(vec![0; crate::avatar::UPLOAD_MAX + 1]),
            t0,
        );
        let big = lock(&snapshot).avatar.clone().unwrap();
        assert!(!big.sent && big.serial == 3);
        worker.handle(Command::RemoveAvatar, t0);
        let removed = lock(&snapshot).avatar.clone().unwrap();
        assert_eq!((removed.sent, removed.message.as_str()), (true, "removed"));
        assert!(lock(&snapshot).me.as_ref().unwrap().avatar.is_empty());
        let sent: Vec<String> = fake
            .log()
            .into_iter()
            .filter(|line| line.starts_with("avatar"))
            .collect();
        assert_eq!(sent, ["avatar 300 bytes", "avatar removed"]);
        // A refusal is the hub's message.
        fake.fail.store(true, Ordering::SeqCst);
        worker.handle(Command::SetAvatar(vec![1; 10]), t0);
        let failed = lock(&snapshot).avatar.clone().unwrap();
        assert!(!failed.sent && failed.message.contains("down"));
    }

    #[test]
    fn chat_needs_the_rules_and_a_registration() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Chat("hello".into()), t0);
        let first = chat_outcome(&worker);
        assert!(!first.sent && first.message.contains("not connected"));
        worker.handle(Command::Name("^2Sol".into()), t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        // The rules are checked here first; nothing goes.
        worker.handle(Command::Chat("hi a\u{200D}b".into()), t0);
        let refused = chat_outcome(&worker);
        assert_eq!(
            (refused.serial, refused.sent, refused.message.as_str()),
            (2, false, crate::chat::ChatError::Characters.message())
        );
        worker.handle(Command::Chat("  gg   wp ".into()), t0);
        assert_eq!(fake.log().last().unwrap(), "chat gg wp as ^2Sol");
        let sent = chat_outcome(&worker);
        assert!(sent.sent);
        assert_eq!(sent.serial, 3);
    }

    #[test]
    fn emotes_need_a_server_and_a_well_formed_id() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::Emote("wave".into(), None), t0);
        let nowhere = chat_outcome(&worker);
        assert!(!nowhere.sent && nowhere.message.contains("server"));
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        worker.handle(Command::Emote("Wave!".into(), None), t0);
        assert!(!chat_outcome(&worker).sent);
        assert!(!fake.log().iter().any(|line| line.starts_with("emote")));
        worker.handle(Command::Emote("wave".into(), None), t0);
        assert_eq!(fake.log().last().unwrap(), "emote wave on 1.2.3.4:29070");
        assert!(chat_outcome(&worker).sent);
        worker.handle(Command::Emote("wave".into(), Some(5)), t0);
        assert_eq!(
            fake.log().last().unwrap(),
            "emote wave on 1.2.3.4:29070 with 5"
        );
        // A hub that does not know `sync` still gets the emote, out of step.
        fake.old_hub.store(true, Ordering::SeqCst);
        worker.handle(Command::Emote("bow".into(), Some(5)), t0);
        assert_eq!(fake.log().last().unwrap(), "emote bow on 1.2.3.4:29070");
        assert!(chat_outcome(&worker).sent);
    }

    #[test]
    fn pings_need_a_server() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::Ping([1, 2, 3], "all".into()), t0);
        let outcome = || crate::feed::lock(&worker.ping_outcome).clone().unwrap();
        let nowhere = outcome();
        assert!(!nowhere.sent && nowhere.message.contains("server"));
        assert!(!fake.log().iter().any(|line| line.starts_with("ping")));
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        worker.handle(Command::Ping([1, -2, 3], "red".into()), t0);
        assert_eq!(
            fake.log().last().unwrap(),
            "ping [1, -2, 3] red on 1.2.3.4:29070"
        );
        let sent = crate::feed::lock(&worker.ping_outcome).clone().unwrap();
        assert!(sent.sent && sent.serial == nowhere.serial + 1);
    }

    #[test]
    fn the_feed_runs_when_registered_with_chat_on_or_on_a_server() {
        let fake = Fake::default();
        fake.fail.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        let shared = Arc::clone(&worker.feed);
        let url = || lock_feed(&shared).url.clone();
        worker.handle(Command::SetChat(true), t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!(url(), None, "not registered yet");
        fake.fail.store(false, Ordering::SeqCst);
        worker.tick(t0 + Duration::from_secs(30));
        assert_eq!(url().as_deref(), Some("https://hub"));
        assert_eq!(lock_feed(&shared).server, None);
        assert!(lock_feed(&shared).chat);
        // In the menus the chat off stops the reading.
        worker.handle(Command::SetChat(false), t0);
        assert_eq!(url(), None);
        // On a server it reads anyway, for the looks and emotes, the chat hidden.
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        assert_eq!(url().as_deref(), Some("https://hub"));
        assert_eq!(lock_feed(&shared).server.as_deref(), Some("1.2.3.4:29070"));
        assert!(!lock_feed(&shared).chat);
        worker.handle(Command::SetChat(true), t0);
        assert!(url().is_some() && lock_feed(&shared).chat);
        worker.handle(Command::SetChat(false), t0);
        worker.handle(Command::Leave, t0);
        assert_eq!(url(), None, "back in the menus with the chat off");
        worker.handle(Command::SetChat(true), t0);
        assert!(url().is_some());
        worker.handle(Command::Configure(Settings::default()), t0);
        assert_eq!(url(), None, "identity off");
    }

    #[test]
    fn another_server_or_the_identity_off_is_a_new_reading_for_the_looks() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        let shared = Arc::clone(&worker.feed);
        let generation = || lock_feed(&shared).generation;
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        let first = generation();
        // The chat, a slot or a name change nothing of what is read.
        worker.handle(Command::SetChat(true), t0);
        worker.handle(Command::Enter(here(4, "Sol")), t0);
        assert_eq!(generation(), first);
        let elsewhere = Location {
            server: "5.6.7.8:29070".parse().unwrap(),
            ..here(4, "Sol")
        };
        worker.handle(Command::Enter(elsewhere), t0);
        let second = generation();
        assert!(second > first);
        worker.handle(Command::Configure(Settings::default()), t0);
        assert!(generation() > second, "identity off");
    }

    fn lit(saber: &str) -> Look {
        Look {
            saber: saber.to_owned(),
            illuminate: true,
            saber_off: Vec::new(),
        }
    }

    #[test]
    fn the_look_goes_once_the_claim_is_accepted_and_at_most_once_a_second() {
        let fake = Fake::default();
        fake.owned.lock().unwrap().push("saber_sun".to_owned());
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        // Without a claim there is nowhere to wear it.
        worker.handle(Command::Look(lit("")), t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert!(fake.looks().is_empty());
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        assert_eq!(
            fake.log()[1..],
            [
                "claim 1.2.3.4:29070 3 Sol",
                "presence 1.2.3.4:29070",
                "look 1.2.3.4:29070 \"\" true",
            ]
        );
        assert!(lock(&snapshot).look_outcome.as_ref().unwrap().sent);
        // Changes within the second wait, and only the latest goes.
        let ms = |ms| t0 + Duration::from_millis(ms);
        worker.handle(Command::Look(lit("saber_sun")), ms(200));
        assert_eq!(worker.tick(ms(200)), Duration::from_millis(800));
        worker.handle(Command::Look(Look::default()), ms(500));
        worker.tick(ms(500));
        worker.handle(Command::Look(lit("saber_sun")), ms(700));
        worker.tick(ms(700));
        assert_eq!(fake.looks().len(), 1);
        worker.tick(ms(1_000));
        assert_eq!(
            fake.looks(),
            [
                "look 1.2.3.4:29070 \"\" true",
                "look 1.2.3.4:29070 \"saber_sun\" true"
            ]
        );
        // The same look is not sent again, nor with the claim renewed.
        worker.handle(Command::Look(lit("saber_sun")), ms(2_000));
        worker.tick(ms(2_000));
        worker.tick(t0 + Duration::from_secs(46));
        assert_eq!(fake.looks().len(), 2);
    }

    #[test]
    fn a_new_claim_gets_the_look_again_and_no_look_sends_nothing() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        // A client that stopped without releasing may have left a live claim with a
        // look: the first accepted claim gets the look, even none.
        assert_eq!(fake.looks(), ["look 1.2.3.4:29070 \"\" false"]);
        // Another slot: a new claim, which starts with no look, so none goes.
        let t1 = t0 + Duration::from_secs(5);
        worker.handle(Command::Enter(here(4, "Sol")), t1);
        worker.tick(t1);
        assert_eq!(fake.log().last().unwrap(), "presence 1.2.3.4:29070");
        assert_eq!(fake.looks().len(), 1);
        worker.handle(Command::Look(lit("")), t1);
        worker.tick(t1);
        assert_eq!(fake.looks().len(), 2);
        // Another slot again: the new claim gets the look again.
        let t2 = t1 + Duration::from_secs(5);
        worker.handle(Command::Enter(here(5, "Sol")), t2);
        worker.tick(t2);
        assert_eq!(fake.log().last().unwrap(), "look 1.2.3.4:29070 \"\" true");
        assert_eq!(fake.looks().len(), 3);
        // Another server: the old claim is withdrawn and the new one gets it.
        let elsewhere = Location {
            server: "5.6.7.8:29070".parse().unwrap(),
            ..here(5, "Sol")
        };
        let t3 = t2 + Duration::from_secs(5);
        worker.handle(Command::Enter(elsewhere), t3);
        worker.tick(t3);
        assert_eq!(fake.log().last().unwrap(), "look 5.6.7.8:29070 \"\" true");
        // A claim that failed may have lapsed: the next accepted one sends it again.
        fake.fail.store(true, Ordering::SeqCst);
        let t4 = t3 + Duration::from_secs(46);
        worker.tick(t4);
        fake.fail.store(false, Ordering::SeqCst);
        worker.tick(t4 + Duration::from_secs(30));
        assert_eq!(fake.looks().len(), 5);
        // Leaving needs nothing: the release drops it.
        worker.handle(Command::Leave, t4 + Duration::from_secs(31));
        assert_eq!(fake.log().last().unwrap(), "release 5.6.7.8:29070");
        // Back on the server: a new claim after a release starts with no look.
        let t5 = t4 + Duration::from_secs(40);
        worker.handle(Command::Look(Look::default()), t5);
        worker.handle(Command::Enter(here(5, "Sol")), t5);
        worker.tick(t5);
        assert_eq!(fake.log().last().unwrap(), "presence 1.2.3.4:29070");
        assert_eq!(fake.looks().len(), 5);
    }

    #[test]
    fn after_a_failed_claim_the_look_goes_whatever_it_is() {
        let fake = Fake::default();
        fake.owned.lock().unwrap().push("saber_sun".to_owned());
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.handle(Command::Look(lit("saber_sun")), t0);
        worker.tick(t0);
        assert_eq!(fake.looks(), ["look 1.2.3.4:29070 \"saber_sun\" true"]);
        // The renewal fails (it timed out, say) while the hub still holds the claim and
        // its look; meanwhile the look goes back to none.
        fake.fail.store(true, Ordering::SeqCst);
        let t1 = t0 + Duration::from_secs(46);
        worker.tick(t1);
        worker.handle(Command::Look(Look::default()), t1);
        fake.fail.store(false, Ordering::SeqCst);
        // The same claim is accepted again: the hub may still hold the Sun, so none goes.
        let t2 = t1 + Duration::from_secs(30);
        worker.tick(t2);
        assert_eq!(
            fake.looks().last().unwrap(),
            "look 1.2.3.4:29070 \"\" false"
        );
        assert_eq!(fake.looks().len(), 2);
        // Known again: not repeated.
        worker.tick(t2 + Duration::from_secs(46));
        assert_eq!(fake.looks().len(), 2);
    }

    fn refused(status: u16, code: &str) -> HubError {
        HubError::Rejected {
            status,
            code: code.to_owned(),
            message: String::new(),
        }
    }

    #[test]
    fn a_look_refused_for_too_many_or_the_signature_goes_again_later() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        let sent = fake.looks().len();
        for (status, code) in [(429, "rate_limited"), (429, "look_quota"), (401, "clock")] {
            fake.look_refusals
                .lock()
                .unwrap()
                .push_back(refused(status, code));
        }
        let mut at = t0 + LOOK_EVERY;
        worker.handle(Command::Look(lit("")), at);
        for (n, code) in ["rate_limited", "look_quota", "clock"].iter().enumerate() {
            worker.tick(at);
            assert_eq!(fake.looks().len(), sent + n + 1, "{code}");
            assert!(!lock(&snapshot).look_outcome.clone().unwrap().sent);
            // Not again within the wait, then again.
            worker.tick(at + LOOK_AGAIN - Duration::from_millis(1));
            assert_eq!(fake.looks().len(), sent + n + 1, "{code}");
            at += LOOK_AGAIN;
        }
        worker.tick(at);
        assert_eq!(fake.looks().len(), sent + 4);
        assert!(lock(&snapshot).look_outcome.clone().unwrap().sent);
        // Another refusal (an older hub) is not repeated.
        fake.look_refusals
            .lock()
            .unwrap()
            .push_back(refused(404, "not_found"));
        worker.handle(Command::Look(Look::default()), at);
        worker.tick(at + LOOK_AGAIN);
        worker.tick(at + LOOK_AGAIN * 3);
        assert_eq!(fake.looks().len(), sent + 5);
    }

    fn profile_reads(fake: &Fake) -> usize {
        fake.log()
            .iter()
            .filter(|line| line.starts_with("lookup"))
            .count()
    }

    #[test]
    fn a_refused_skin_reads_the_own_profile_soon_at_most_twice_a_minute() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        // The profile listed the Sun, but staff took it back: the hub refuses it.
        worker.handle(Command::Look(lit("saber_sun")), t0);
        assert_eq!(
            worker.tick(t0),
            Duration::ZERO,
            "the profile is due at once"
        );
        assert_eq!(fake.looks(), ["look 1.2.3.4:29070 \"saber_sun\" true"]);
        assert_eq!(profile_reads(&fake), 0);
        let ms = |ms| t0 + Duration::from_millis(ms);
        worker.tick(ms(10));
        assert_eq!(profile_reads(&fake), 1);
        // Refused again: not before 30 seconds from the last early reading.
        worker.handle(Command::Look(lit("saber_moon")), ms(2_000));
        worker.tick(ms(2_000));
        assert_eq!(
            fake.looks().last().unwrap(),
            "look 1.2.3.4:29070 \"saber_moon\" true"
        );
        worker.tick(ms(29_000));
        assert_eq!(profile_reads(&fake), 1);
        worker.tick(ms(30_010));
        assert_eq!(profile_reads(&fake), 2);
    }

    #[test]
    fn the_hub_taking_the_own_skin_back_reads_the_own_profile_soon() {
        let fake = Fake::default();
        fake.owned.lock().unwrap().push("saber_sun".to_owned());
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.handle(Command::Look(lit("saber_sun")), t0);
        worker.tick(t0);
        // The fake hub numbers it 15. The feed brings it back: nothing to do.
        let t1 = t0 + Duration::from_secs(1);
        worker.handle(Command::OwnLook(15, "saber_sun".into()), t1);
        // An older one, or a newer one with the same skin, is not news either.
        worker.handle(Command::OwnLook(12, String::new()), t1);
        worker.handle(Command::OwnLook(16, "saber_sun".into()), t1);
        worker.tick(t1);
        assert_eq!(profile_reads(&fake), 0);
        // Staff relock it: the hub's event takes the skin off.
        worker.handle(Command::OwnLook(20, String::new()), t1);
        worker.tick(t1);
        assert_eq!(profile_reads(&fake), 1);
        // Another within the 30 seconds waits for them.
        worker.handle(Command::OwnLook(21, String::new()), t1);
        worker.tick(t1 + Duration::from_secs(29));
        assert_eq!(profile_reads(&fake), 1);
        worker.tick(t1 + Duration::from_secs(30));
        assert_eq!(profile_reads(&fake), 2);
    }

    #[test]
    fn a_blade_skin_the_key_lacks_is_left_out_until_the_profile_changes() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.handle(Command::Look(lit("saber_sun")), t0);
        worker.tick(t0);
        assert_eq!(fake.looks(), ["look 1.2.3.4:29070 \"saber_sun\" true"]);
        let outcome = lock(&snapshot).look_outcome.clone().unwrap();
        assert!(!outcome.sent);
        // A second later the Illuminate still goes, without the skin, and then nothing.
        let t1 = t0 + Duration::from_secs(1);
        worker.tick(t1);
        worker.tick(t1 + Duration::from_secs(5));
        assert_eq!(
            fake.looks()[1..],
            ["look 1.2.3.4:29070 \"\" true".to_owned()]
        );
        // Another skin is tried and refused; back to the first, which stays out: the
        // hub already holds the look without a skin, so nothing goes.
        worker.handle(
            Command::Look(lit("saber_moon")),
            t1 + Duration::from_secs(5),
        );
        worker.tick(t1 + Duration::from_secs(5));
        worker.handle(Command::Look(lit("saber_sun")), t1 + Duration::from_secs(7));
        worker.tick(t1 + Duration::from_secs(7));
        worker.tick(t1 + Duration::from_secs(8));
        assert_eq!(
            fake.looks()[2..],
            ["look 1.2.3.4:29070 \"saber_moon\" true".to_owned()]
        );
        // Staff unlock it on the player's own key: the profile changes, it goes.
        let me = lock(&snapshot).me.clone().unwrap().key_id;
        worker.handle(
            Command::Staff(StaffRequest::Unlock {
                key_id: me,
                unlock: "saber_sun".into(),
                note: String::new(),
            }),
            t1 + Duration::from_secs(9),
        );
        worker.tick(t1 + Duration::from_secs(9));
        assert_eq!(
            fake.looks().last().unwrap(),
            "look 1.2.3.4:29070 \"saber_sun\" true"
        );
        assert!(lock(&snapshot).look_outcome.clone().unwrap().sent);
    }

    #[test]
    fn staff_unlock_and_relock_change_the_profile_they_answer() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        let staff = Arc::clone(&worker.staff);
        let me = lock(&snapshot).me.clone().unwrap().key_id;
        worker.handle(
            Command::Staff(StaffRequest::Unlock {
                key_id: me.clone(),
                unlock: "saber_sun".into(),
                note: "Thanks".into(),
            }),
            t0,
        );
        assert_eq!(staff.lock().unwrap().message, "Unlocked saber_sun");
        let unlocks = |snapshot: &Mutex<Snapshot>| lock(snapshot).me.clone().unwrap().unlocks;
        assert_eq!(unlocks(&snapshot)[0].id, "saber_sun");
        worker.handle(
            Command::Staff(StaffRequest::Relock {
                key_id: me,
                unlock: "saber_sun".into(),
            }),
            t0,
        );
        assert_eq!(staff.lock().unwrap().message, "Took back saber_sun");
        assert!(unlocks(&snapshot).is_empty());
        assert_eq!(staff.lock().unwrap().players[0].unlocks, []);
    }

    #[test]
    fn achievement_counts_go_to_the_hub_at_most_once_a_minute() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Achievements(counts(&[("kills_100", 3)])), t0);
        worker.tick(t0);
        assert_eq!(sent_counts(&fake), ["achievements kills_100=3"]);
        let me = lock(&snapshot).me.clone().unwrap();
        assert_eq!(me.achievements[0].progress, 3);
        // The same counts are not sent again; new ones wait for the minute.
        worker.tick(t0 + Duration::from_secs(30));
        worker.handle(
            Command::Achievements(counts(&[("kills_100", 4)])),
            t0 + Duration::from_secs(30),
        );
        worker.tick(t0 + Duration::from_secs(40));
        assert_eq!(sent_counts(&fake).len(), 1);
        worker.tick(t0 + Duration::from_secs(61));
        assert_eq!(
            sent_counts(&fake).last().unwrap(),
            "achievements kills_100=4"
        );
        worker.tick(t0 + Duration::from_secs(5_000));
        assert_eq!(sent_counts(&fake).len(), 2, "nothing waits");
    }

    #[test]
    fn counts_the_hub_held_back_are_sent_again_an_hour_later() {
        let fake = Fake::default();
        *fake.cap.lock().unwrap() = Some(2);
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Achievements(counts(&[("kills_100", 5)])), t0);
        worker.tick(t0);
        worker.tick(t0 + Duration::from_secs(120));
        assert_eq!(sent_counts(&fake).len(), 1, "held back: waits the hour");
        *fake.cap.lock().unwrap() = None;
        worker.tick(t0 + Duration::from_secs(3_601));
        assert_eq!(sent_counts(&fake).len(), 2);
        worker.tick(t0 + Duration::from_secs(8_000));
        assert_eq!(sent_counts(&fake).len(), 2, "all taken: nothing waits");
    }

    #[test]
    fn a_bio_breaking_the_rules_is_refused_before_the_hub_hears_it() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::SetBio("hi a\u{200D}b".to_owned()), t0);
        assert!(!fake.log().iter().any(|line| line.starts_with("bio")));
        assert_eq!(
            lock(&snapshot).notice.as_deref(),
            Some(crate::bio::BioError::Character.message())
        );
        worker.handle(Command::SetBio("  hello   there ".to_owned()), t0);
        assert_eq!(fake.log().last().unwrap(), "bio hello there");
    }

    #[test]
    fn the_own_id_follows_the_person_the_key_belongs_to() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        let local = Identity::from_seed([1; 32]).key_id();
        assert_eq!(
            lock(&snapshot).key_id,
            local,
            "this PC's key until the hub answers"
        );
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        // The fake hub answers for person 0123456789abcdef, as for a linked key.
        let read = lock(&snapshot).clone();
        assert_eq!(read.key_id, "0123456789abcdef");
        assert_eq!(read.local_key_id, local, "the key file's id stays");
        // The feed is told, so the player's own drops are still theirs.
        assert_eq!(lock_feed(&worker.feed).person, "0123456789abcdef");
        // The identity off: this PC's key again.
        worker.handle(
            Command::Configure(Settings {
                enabled: false,
                ..on("https://hub")
            }),
            t0,
        );
        let read = lock(&snapshot).clone();
        assert!(read.me.is_none());
        assert_eq!(read.key_id, local);
        assert_eq!(lock_feed(&worker.feed).person, local);
    }

    #[test]
    fn staff_verify_merge_and_unlink_update_the_list_they_answer() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        let staff = Arc::clone(&worker.staff);
        let state = || staff.lock().unwrap().clone();
        {
            let mut staff = staff.lock().unwrap();
            staff.players = vec![
                Profile {
                    key_id: "aaaaaaaaaaaaaaaa".into(),
                    ..profile("Kept")
                },
                Profile {
                    key_id: "bbbbbbbbbbbbbbbb".into(),
                    keys: vec!["bbbbbbbbbbbbbbbb".into(), "cccccccccccccccc".into()],
                    ..profile("Reset")
                },
                Profile {
                    key_id: "dddddddddddddddd".into(),
                    ..profile("Else")
                },
            ];
        }
        worker.handle(
            Command::Staff(StaffRequest::Verify {
                key_id: "aaaaaaaaaaaaaaaa".into(),
                verified: true,
            }),
            t0,
        );
        assert_eq!(state().message, "Verified");
        assert!(state().players[0].verified);
        // Merging by a linked key of the merged-away person takes that person out.
        worker.handle(
            Command::Staff(StaffRequest::Merge {
                key_id: "aaaaaaaaaaaaaaaa".into(),
                from: "cccccccccccccccc".into(),
            }),
            t0,
        );
        let merged = state();
        assert_eq!(merged.message, "Merged cccccccccccccccc into this player");
        let ids: Vec<&str> = merged.players.iter().map(|p| p.key_id.as_str()).collect();
        assert_eq!(ids, ["aaaaaaaaaaaaaaaa", "dddddddddddddddd"]);
        assert_eq!(
            merged.players[0].keys,
            ["aaaaaaaaaaaaaaaa", "cccccccccccccccc"]
        );
        // Unlinking another key leaves this PC registered.
        let registers = || {
            fake.log()
                .iter()
                .filter(|l| l.starts_with("register"))
                .count()
        };
        let before = registers();
        worker.handle(
            Command::Staff(StaffRequest::Unlink {
                key_id: "cccccccccccccccc".into(),
            }),
            t0,
        );
        assert_eq!(state().message, "Unlinked cccccccccccccccc");
        worker.tick(t0);
        assert_eq!(registers(), before);
        // Unlinking this PC's own key: it starts afresh and registers again.
        let local = Identity::from_seed([1; 32]).key_id();
        worker.handle(
            Command::Staff(StaffRequest::Unlink {
                key_id: local.clone(),
            }),
            t0,
        );
        assert!(lock(&snapshot).me.is_none());
        assert_eq!(lock(&snapshot).key_id, local);
        worker.tick(t0);
        assert_eq!(registers(), before + 1);
        assert!(lock(&snapshot).me.is_some());
    }

    #[test]
    fn staff_requests_need_a_staff_profile_and_update_what_they_change() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        let staff = Arc::clone(&worker.staff);
        let state = || staff.lock().unwrap().clone();
        worker.handle(Command::Staff(StaffRequest::Search("so".into())), t0);
        let found = state();
        assert_eq!(found.players.len(), 2);
        assert_eq!(found.message, "2 players found");
        assert!(!found.failed);
        // A change to another key replaces it in the list; to the player's own key,
        // their profile too.
        let me = lock(&snapshot).me.clone().unwrap().key_id;
        worker.handle(
            Command::Staff(StaffRequest::Award {
                key_id: me.clone(),
                medal: "bug_hunter".into(),
                note: String::new(),
            }),
            t0,
        );
        assert_eq!(lock(&snapshot).me.as_ref().unwrap().medals.len(), 1);
        assert_eq!(state().message, "Gave bug_hunter");
        // A key that is not staff is refused here, before the hub hears it.
        lock(&snapshot).me.as_mut().unwrap().staff = false;
        let before = fake.log().len();
        worker.handle(Command::Staff(StaffRequest::Search(String::new())), t0);
        assert_eq!(fake.log().len(), before);
        assert!(state().failed);
        assert_eq!(state().message, "only SJK staff can do this");
    }

    /// A worker told to keep the asset packs in a fresh folder and switched on; the
    /// folder's parent goes when the [`tempfile::TempDir`] does.
    fn keeping_assets(
        fake: &Fake,
        now: Instant,
    ) -> (Worker, Arc<Mutex<Snapshot>>, tempfile::TempDir, PathBuf) {
        let (mut worker, snapshot) = worker(fake, now);
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(FOLDER);
        worker.handle(Command::AssetsDir(dir.clone()), now);
        worker.handle(Command::Configure(on("https://hub")), now);
        (worker, snapshot, root, dir)
    }

    #[test]
    fn asset_packs_are_downloaded_once_registered_and_again_only_when_they_change() {
        let fake = Fake::default();
        fake.serve("sjk_skins", b"sun v1");
        let t0 = Instant::now();
        let (mut worker, snapshot, _root, dir) = keeping_assets(&fake, t0);
        worker.tick(t0);
        assert_eq!(fake.log()[0], "register");
        assert_eq!(fake.asset_requests(), ["assets", "asset sjk_skins"]);
        assert_eq!(std::fs::read(dir.join("sjk_skins.pk3")).unwrap(), b"sun v1");
        assert_eq!(lock(&snapshot).packs_revision, 1);
        assert!(
            lock(&snapshot)
                .assets_note
                .as_ref()
                .unwrap()
                .contains("sjk_skins")
        );
        // Not again for six hours.
        worker.tick(t0 + Duration::from_secs(3_600));
        assert_eq!(fake.asset_requests().len(), 2);
        // Then the list again; the pack matches, so it is not downloaded.
        let t1 = t0 + ASSETS_EVERY;
        worker.tick(t1);
        assert_eq!(
            fake.asset_requests(),
            ["assets", "asset sjk_skins", "assets"]
        );
        assert_eq!(lock(&snapshot).packs_revision, 1);
        // A new version of the pack is downloaded and replaces the old.
        fake.serve("sjk_skins", b"sun v2");
        let t2 = t1 + ASSETS_EVERY;
        worker.tick(t2);
        assert_eq!(fake.asset_requests()[3..], ["assets", "asset sjk_skins"]);
        assert_eq!(std::fs::read(dir.join("sjk_skins.pk3")).unwrap(), b"sun v2");
        assert_eq!(lock(&snapshot).packs_revision, 2);
        // A pack the hub no longer lists stays for offline use.
        fake.packs.lock().unwrap().clear();
        worker.tick(t2 + ASSETS_EVERY);
        assert_eq!(fake.asset_requests().len(), 6);
        assert!(dir.join("sjk_skins.pk3").exists());
        assert_eq!(lock(&snapshot).packs_revision, 2);
        assert_eq!(lock(&snapshot).status, Status::Online);
    }

    #[test]
    fn a_pack_unlike_its_listing_is_refused_and_the_cached_one_kept() {
        let fake = Fake::default();
        fake.serve("sjk_skins", b"sun v2");
        let t0 = Instant::now();
        let (mut worker, snapshot, _root, dir) = keeping_assets(&fake, t0);
        write_pack(&dir, "sjk_skins", b"sun v1").unwrap();
        // The same length, other bytes: the hash differs.
        *fake.served.lock().unwrap() = Some(b"sun v3".to_vec());
        worker.tick(t0);
        assert_eq!(fake.asset_requests(), ["assets", "asset sjk_skins"]);
        assert_eq!(std::fs::read(dir.join("sjk_skins.pk3")).unwrap(), b"sun v1");
        assert!(!dir.join(".sjk_skins.pk3.part").exists());
        assert_eq!(lock(&snapshot).packs_revision, 0);
        let note = lock(&snapshot).assets_note.clone().unwrap();
        assert!(note.contains("SHA-256"), "{note}");
        assert_eq!(lock(&snapshot).status, Status::Online, "cosmetic");
        // Tried again a minute later; another length is refused too.
        worker.tick(t0 + Duration::from_secs(59));
        assert_eq!(fake.asset_requests().len(), 2);
        *fake.served.lock().unwrap() = Some(b"sun v2, longer".to_vec());
        let t1 = t0 + ASSETS_RETRY_MIN;
        worker.tick(t1);
        assert_eq!(fake.asset_requests().len(), 4);
        assert!(
            lock(&snapshot)
                .assets_note
                .as_ref()
                .unwrap()
                .contains("14 bytes")
        );
        assert_eq!(std::fs::read(dir.join("sjk_skins.pk3")).unwrap(), b"sun v1");
        // The right bytes, two minutes later, are written.
        *fake.served.lock().unwrap() = None;
        worker.tick(t1 + ASSETS_RETRY_MIN * 2);
        assert_eq!(std::fs::read(dir.join("sjk_skins.pk3")).unwrap(), b"sun v2");
        assert_eq!(lock(&snapshot).packs_revision, 1);
    }

    #[test]
    fn asset_check_failures_back_off_and_success_resets_the_wait() {
        let fake = Fake::default();
        fake.serve("sjk_skins", b"sun");
        for refusal in [
            HubError::Network("down".to_owned()),
            refused(401, "bad_signature"),
            refused(503, "error"),
            refused(429, "rate_limited"),
        ] {
            fake.asset_refusals.lock().unwrap().push_back(refusal);
        }
        let t0 = Instant::now();
        let (mut worker, snapshot, _root, _dir) = keeping_assets(&fake, t0);
        worker.tick(t0);
        assert_eq!(fake.asset_requests(), ["assets"]);
        assert_eq!(lock(&snapshot).status, Status::Online, "cosmetic");
        // The worker wakes for the next check.
        assert!(worker.tick(t0 + Duration::from_secs(30)) <= Duration::from_secs(30));
        let mut at = t0;
        for (n, wait) in [60, 120, 240, 480].into_iter().enumerate() {
            worker.tick(at + Duration::from_secs(wait - 1));
            assert_eq!(fake.asset_requests().len(), n + 1, "not before {wait} s");
            at += Duration::from_secs(wait);
            worker.tick(at);
        }
        assert_eq!(
            fake.asset_requests()[4..],
            ["assets", "asset sjk_skins"],
            "the fifth reading succeeds"
        );
        assert_eq!(lock(&snapshot).packs_revision, 1);
        // Success: the next check is six hours on, and the next failure waits a minute.
        fake.asset_refusals
            .lock()
            .unwrap()
            .push_back(HubError::Network("down".to_owned()));
        worker.tick(at + ASSETS_EVERY - Duration::from_secs(1));
        assert_eq!(fake.asset_requests().len(), 6);
        at += ASSETS_EVERY;
        worker.tick(at);
        assert_eq!(fake.asset_requests().len(), 7);
        worker.tick(at + ASSETS_RETRY_MIN);
        assert_eq!(fake.asset_requests().len(), 8);
        assert_eq!(lock(&snapshot).status, Status::Online);
    }

    #[test]
    fn an_older_hub_without_assets_is_asked_again_in_six_hours() {
        let fake = Fake::default();
        fake.asset_refusals
            .lock()
            .unwrap()
            .push_back(refused(404, "not_found"));
        let t0 = Instant::now();
        let (mut worker, snapshot, _root, dir) = keeping_assets(&fake, t0);
        worker.tick(t0);
        worker.tick(t0 + ASSETS_RETRY_MIN);
        worker.tick(t0 + Duration::from_secs(3_600));
        assert_eq!(fake.asset_requests(), ["assets"]);
        assert!(
            lock(&snapshot)
                .assets_note
                .as_ref()
                .unwrap()
                .contains("not_found")
        );
        worker.tick(t0 + ASSETS_EVERY);
        assert_eq!(fake.asset_requests(), ["assets", "assets"]);
        assert!(crate::assets::cached_packs(&dir).is_empty());
        assert_eq!(lock(&snapshot).status, Status::Online);
    }

    #[test]
    fn no_asset_is_asked_for_while_off_unregistered_or_without_a_folder() {
        let fake = Fake::default();
        fake.serve("sjk_skins", b"sun");
        let t0 = Instant::now();
        // Registered, but told no folder.
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.tick(t0 + ASSETS_EVERY);
        assert!(fake.log().contains(&"register".to_owned()));
        assert!(fake.asset_requests().is_empty());
        // A folder, with the identity off: nothing at all.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(FOLDER);
        let t1 = t0 + ASSETS_EVERY * 2;
        worker.handle(
            Command::Configure(Settings {
                enabled: false,
                hub_url: "https://hub".into(),
            }),
            t1,
        );
        worker.handle(Command::AssetsDir(dir.clone()), t1);
        let before = fake.log().len();
        worker.tick(t1);
        worker.tick(t1 + ASSETS_EVERY);
        assert_eq!(fake.log().len(), before);
        // On, but the registration fails: no asset request either.
        fake.fail.store(true, Ordering::SeqCst);
        let t2 = t1 + ASSETS_EVERY * 2;
        worker.handle(Command::Configure(on("https://hub")), t2);
        worker.tick(t2);
        assert!(fake.asset_requests().is_empty());
        assert!(!dir.exists());
        // Registered: checked at once.
        fake.fail.store(false, Ordering::SeqCst);
        let t3 = t2 + Duration::from_secs(30);
        worker.tick(t3);
        assert_eq!(fake.asset_requests(), ["assets", "asset sjk_skins"]);
        assert_eq!(lock(&snapshot).packs_revision, 1);
        // Off and on again: checked again at once; the pack matches.
        worker.handle(Command::Configure(Settings::default()), t3);
        worker.tick(t3);
        worker.handle(Command::Configure(on("https://hub")), t3);
        worker.tick(t3);
        assert_eq!(
            fake.asset_requests(),
            ["assets", "asset sjk_skins", "assets"]
        );
        assert_eq!(lock(&snapshot).packs_revision, 1);
    }

    #[test]
    fn nothing_is_sent_while_disabled_or_without_a_hub() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        assert_eq!(lock(&snapshot).status, Status::Disabled);
        worker.handle(Command::Configure(on("")), t0);
        worker.tick(t0);
        assert_eq!(lock(&snapshot).status, Status::NoHub);
        worker.handle(
            Command::Configure(Settings {
                enabled: false,
                hub_url: "https://h".into(),
            }),
            t0,
        );
        worker.tick(t0);
        assert!(fake.log().is_empty());
    }

    #[test]
    fn it_registers_before_it_claims_and_repeats_the_claim() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        assert_eq!(
            fake.log(),
            [
                "register",
                "claim 1.2.3.4:29070 3 Sol",
                "presence 1.2.3.4:29070",
                // What the hub held for the key before the start is not known.
                "look 1.2.3.4:29070 \"\" false"
            ]
        );
        assert_eq!(lock(&snapshot).status, Status::Online);
        // Nothing is due for a while, then the roster, then the claim.
        worker.tick(t0 + Duration::from_secs(10));
        assert_eq!(fake.log().len(), 4);
        worker.tick(t0 + Duration::from_secs(16));
        assert_eq!(fake.log().last().unwrap(), "presence 1.2.3.4:29070");
        worker.tick(t0 + Duration::from_secs(46));
        assert!(
            fake.log()
                .iter()
                .filter(|line| line.starts_with("claim"))
                .count()
                == 2
        );
    }

    #[test]
    fn leaving_or_switching_off_withdraws_the_claim() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        worker.handle(Command::Leave, t0);
        assert_eq!(fake.log().last().unwrap(), "release 1.2.3.4:29070");
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        worker.handle(Command::Configure(Settings::default()), t0);
        assert_eq!(fake.log().last().unwrap(), "release 1.2.3.4:29070");
        let before = fake.log().len();
        worker.tick(t0 + Duration::from_secs(500));
        assert_eq!(fake.log().len(), before);
    }

    #[test]
    fn a_new_name_or_slot_is_claimed_at_once_and_the_same_one_is_not_repeated() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, _) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        let claims = |fake: &Fake| fake.log().iter().filter(|l| l.starts_with("claim")).count();
        worker.handle(Command::Enter(here(3, "Sol")), t0);
        worker.tick(t0);
        assert_eq!(claims(&fake), 1);
        worker.handle(Command::Enter(here(3, "Fox")), t0);
        worker.tick(t0);
        assert_eq!(claims(&fake), 2);
        assert_eq!(
            fake.log()
                .iter()
                .filter(|l| l.starts_with("release"))
                .count(),
            0
        );
    }

    #[test]
    fn failures_back_off_and_recovery_clears_the_status() {
        let fake = Fake::default();
        fake.fail.store(true, Ordering::SeqCst);
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        let wait = worker.tick(t0);
        assert!(matches!(lock(&snapshot).status, Status::Failed(_)));
        assert_eq!(wait, RETRY_MIN);
        worker.tick(t0 + Duration::from_secs(5));
        assert_eq!(fake.log().len(), 1, "no retry before the backoff ends");
        let wait = worker.tick(t0 + Duration::from_secs(11));
        assert_eq!(wait, RETRY_MIN * 2);
        fake.fail.store(false, Ordering::SeqCst);
        worker.tick(t0 + Duration::from_secs(40));
        assert_eq!(lock(&snapshot).status, Status::Online);
    }

    #[test]
    fn a_bad_address_is_reported_not_used() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("http://hub.example")), t0);
        worker.tick(t0);
        assert!(matches!(lock(&snapshot).status, Status::Failed(_)));
        assert!(fake.log().is_empty());
    }

    #[test]
    fn badges_match_by_slot_and_name_only() {
        let fake = Fake::default();
        *fake.roster.lock().unwrap() = vec![Presence {
            slot: 3,
            claimed_name: "^1Sol".to_owned(),
            key_id: "0123456789abcdef".to_owned(),
            name: "Sol".to_owned(),
            verified: true,
            medals: Vec::new(),
            avatar: String::new(),
            look: None,
        }];
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.handle(Command::Enter(here(1, "Me")), t0);
        worker.tick(t0);
        let snapshot = lock(&snapshot);
        assert!(snapshot.badge(3, "sol").unwrap().verified);
        assert!(
            snapshot.badge(3, "Fox").is_none(),
            "another name in the slot"
        );
        assert!(snapshot.badge(4, "Sol").is_none(), "another slot");
    }

    #[test]
    fn bio_changes_and_lookups_go_through_the_hub() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::SetBio("x".into()), t0);
        assert_eq!(
            lock(&snapshot).notice.as_deref(),
            Some("not connected to the hub")
        );
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::SetBio("hello".into()), t0);
        assert_eq!(fake.log().last().unwrap(), "bio hello");
        assert_eq!(lock(&snapshot).notice.as_deref(), Some("saved"));
        worker.handle(Command::LookUp("0123456789abcdef".into()), t0);
        worker.tick(t0);
        assert!(lock(&snapshot).profiles.contains_key("0123456789abcdef"));
    }

    #[test]
    fn the_model_list_goes_once_registered_and_checks_come_back() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        let list = vec!["kyle".to_owned(), "wolfie".to_owned()];
        worker.handle(Command::Models(list.clone()), t0);
        worker.tick(t0);
        assert!(fake.log().is_empty(), "nothing before the identity is on");
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!(
            fake.log()
                .iter()
                .filter(|line| line.starts_with("models"))
                .count(),
            1
        );
        // The same list again is not sent again; a new one is.
        worker.handle(Command::Models(list), t0);
        worker.tick(t0);
        worker.handle(Command::Models(vec!["kyle".to_owned()]), t0);
        worker.tick(t0);
        let sent: Vec<String> = fake
            .log()
            .into_iter()
            .filter(|line| line.starts_with("models"))
            .collect();
        assert_eq!(sent, ["models kyle,wolfie", "models kyle"]);
        let server: SocketAddr = "127.0.0.1:29070".parse().unwrap();
        worker.handle(Command::CheckModel(server, 3, "kyle".to_owned()), t0);
        worker.handle(Command::CheckModel(server, 3, "reborn".to_owned()), t0);
        worker.tick(t0);
        let checks = lock(&snapshot).model_checks.clone();
        let has = |model: &str| checks[&(server, 3, model.to_owned())].has;
        assert_eq!((has("kyle"), has("reborn")), (Some(true), Some(false)));
        assert_eq!(
            checks[&(server, 3, "kyle".to_owned())].claimed_name,
            "^2Sol"
        );
    }

    #[test]
    fn notes_go_to_the_hub_and_their_pictures_follow() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        let note = WorldNote {
            text: "too shiny".to_owned(),
            shader: "textures/vjun/newfloor_vjun".to_owned(),
            ..WorldNote::default()
        };
        // Before registering, a note is refused and its picture goes nowhere.
        worker.handle(Command::Note(1, Box::new(note.clone())), t0);
        worker.handle(Command::NoteImage(1, vec![0; 4]), t0);
        assert!(!lock(&snapshot).note.clone().unwrap().sent);
        assert!(fake.log().is_empty());
        worker.handle(Command::Name("^1Sol".to_owned()), t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::Note(2, Box::new(note)), t0);
        let outcome = lock(&snapshot).note.clone().unwrap();
        assert!(outcome.sent);
        assert_eq!((outcome.serial, outcome.message.as_str()), (2, "note #9"));
        // A picture for an unknown tag is dropped; the note's goes to its hub id once.
        worker.handle(Command::NoteImage(5, vec![0; 4]), t0);
        worker.handle(Command::NoteImage(2, vec![0; 4]), t0);
        worker.handle(Command::NoteImage(2, vec![0; 4]), t0);
        let log = fake.log();
        assert_eq!(
            log.iter()
                .filter(|line| line.starts_with("image"))
                .collect::<Vec<_>>(),
            ["image 9 4 bytes"]
        );
        assert!(log.contains(&"note too shiny textures/vjun/newfloor_vjun".to_owned()));
        // The note carries the name the player wears.
        assert!(log.contains(&"note name ^1Sol".to_owned()), "{log:?}");
    }

    #[test]
    fn bug_reports_go_to_the_hub_once_registered() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        let report = BugReport {
            text: "The door flickers".into(),
            ..BugReport::default()
        };
        // Before registering, nothing is sent and the outcome says why.
        worker.handle(Command::Report(report.clone()), t0);
        let first = lock(&snapshot).report.clone().unwrap();
        assert!(!first.sent && first.message.contains("not connected"));
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        worker.handle(Command::Report(report), t0);
        assert!(fake.log().contains(&"report The door flickers".to_owned()));
        let outcome = lock(&snapshot).report.clone().unwrap();
        assert_eq!((outcome.serial, outcome.sent), (first.serial + 1, true));
        assert_eq!(outcome.message, "report #7");
    }

    #[test]
    fn player_reports_go_only_from_a_verified_key() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        let report = PlayerReport {
            category: crate::report::Category::Cheating,
            text: "Speed hacking all round".into(),
            slot: 5,
            target_name: "^1Troll".into(),
            ..PlayerReport::default()
        };
        worker.handle(Command::PlayerReport(report.clone()), t0);
        let first = lock(&snapshot).player_report.clone().unwrap();
        assert!(!first.sent && first.message.contains("not connected"));
        worker.handle(Command::Name("^2Sol".into()), t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        // Registered but not verified: nothing goes, and the outcome says why.
        worker.handle(Command::PlayerReport(report.clone()), t0);
        let unverified = lock(&snapshot).player_report.clone().unwrap();
        assert!(!unverified.sent && unverified.message.contains("verified"));
        assert!(!fake.log().iter().any(|line| line.starts_with("player")));
        lock(&snapshot).me.as_mut().unwrap().verified = true;
        worker.handle(Command::PlayerReport(report), t0);
        let sent = lock(&snapshot).player_report.clone().unwrap();
        assert_eq!(
            (sent.serial, sent.sent, sent.message.as_str()),
            (3, true, "player report #11")
        );
        // It carries the name the reporter wears.
        assert!(
            fake.log()
                .contains(&"player cheating 5 ^1Troll by ^2Sol".to_owned()),
            "{:?}",
            fake.log()
        );
    }

    #[test]
    fn the_worn_name_goes_with_the_registration_and_again_when_it_changes() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Name("^1Sol".into()), t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        worker.tick(t0);
        assert_eq!(fake.log(), ["register ^1Sol"]);
        assert_eq!(lock(&snapshot).me.as_ref().unwrap().name, "^1Sol");
        // The same name again sends nothing; an empty one is no name.
        worker.handle(Command::Name("^1Sol".into()), t0);
        worker.tick(t0);
        assert_eq!(fake.log().len(), 1);
        worker.handle(Command::Name("Fox".into()), t0);
        let wait = worker.tick(t0);
        assert_eq!(fake.log().last().unwrap(), "register Fox");
        assert_eq!(wait, IDLE_MAX, "nothing else is due");
        // A failure retries after the backoff, not at once.
        fake.fail.store(true, Ordering::SeqCst);
        worker.handle(Command::Name("Wolf".into()), t0);
        worker.tick(t0);
        let count = fake.log().len();
        worker.tick(t0 + Duration::from_secs(1));
        assert_eq!(fake.log().len(), count);
        fake.fail.store(false, Ordering::SeqCst);
        worker.tick(t0 + Duration::from_secs(30));
        assert_eq!(fake.log().last().unwrap(), "register Wolf");
    }

    #[test]
    fn the_own_profile_is_read_again_now_and_then() {
        let fake = Fake::default();
        let t0 = Instant::now();
        let (mut worker, snapshot) = worker(&fake, t0);
        worker.handle(Command::Configure(on("https://hub")), t0);
        let wait = worker.tick(t0);
        assert_eq!(fake.log(), ["register"]);
        assert!(wait <= PROFILE_EVERY);
        worker.tick(t0 + PROFILE_EVERY - Duration::from_secs(1));
        assert_eq!(fake.log().len(), 1, "not yet");
        worker.tick(t0 + PROFILE_EVERY);
        let own = format!("lookup {}", Identity::from_seed([1; 32]).key_id());
        assert_eq!(fake.log().last(), Some(&own));
        assert_eq!(lock(&snapshot).me.as_ref().unwrap().name, "Other");
        worker.tick(t0 + PROFILE_EVERY + Duration::from_secs(1));
        assert_eq!(fake.log().len(), 2, "and then not for a while");
    }
}
