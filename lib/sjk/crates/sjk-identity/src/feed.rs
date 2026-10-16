//! The hub's feed (`PROTOCOL.md`, "The feed"): a thread of its own that holds a long
//! poll open at the hub and keeps the SJK chat's last messages and online count, and
//! the emotes, pings and looks received, so the identity worker's claims and reports
//! never wait behind it.
//!
//! The worker tells it, through [`FeedShared`], which hub to read (only while the
//! identity is on and the hub answered the registration, and then while the player
//! is on a game server or the chat is on), which game server's emotes and looks to ask
//! for, and whether the chat shows. With the chat off the feed still reads on a game
//! server, for the looks and emotes, but keeps no message: [`ChatState`] stays as an
//! idle feed leaves it. With no hub to read it makes no request.
//!
//! A holocron drop (`PROTOCOL.md`, "The feed") joins the chat's messages in the feed's
//! order as a [`ChatMessage`] whose [`ChatMessage::holocron`] is set, so the game's
//! chat, the docked chat and the chat page show it where it was said. A drop of the
//! player's own key also tells the identity worker ([`OwnDrop`]), which reads the own
//! profile and holocron progress again soon.
//!
//! Another game server starts the reading again from `after` 0, whose answer carries
//! that server's looks of the last minute. The looks queued for the viewer carry the
//! server they were read for and the reading's generation ([`FeedShared::generation`],
//! which changes with the hub, the server or the identity going off or on), and
//! [`take_looks`] hands over only those of the current generation and the viewer's
//! server, so a poll still under way when either changed cannot bring old looks back.

use crate::hub::Hub;
use crate::keys::Identity;
use crate::service::{HubFactory, ReportOutcome};
use crate::wire::{ChatMessage, DropEvent, DropMark, Emote, Feed, LookEvent, People, Ping};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Messages kept, as many as the hub keeps.
pub const MESSAGES_KEPT: usize = 200;
/// How long the hub may hold a poll when nothing is new, in seconds.
pub const WAIT: u64 = 25;
/// The feed's HTTP timeout: the hub's wait and room for a slow answer.
pub const TIMEOUT: Duration = Duration::from_secs(40);
/// Shortest time between the starts of two polls.
const POLL_GAP: Duration = Duration::from_secs(2);
/// First wait after a failure, doubled up to [`RETRY_MAX`].
const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);
/// How often an idle feed looks at what it is told.
pub(crate) const IDLE: Duration = Duration::from_secs(1);
/// Emotes waiting for the viewer, the newest kept.
const EMOTES_WAITING: usize = 64;
/// Looks waiting for the viewer, the newest kept.
const LOOKS_WAITING: usize = 64;
/// Pings waiting for the viewer, the newest kept.
const PINGS_WAITING: usize = 64;

/// What the feed thread calls when a holocron dropped for the player's own key.
pub(crate) type OwnDrop = Box<dyn Fn() + Send>;

/// What the chat shows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChatState {
    /// The last messages, oldest first.
    pub messages: VecDeque<ChatMessage>,
    /// Counts changes to `messages`, `online`, `people` and `live`, so a reader that derives from
    /// them knows when to derive again.
    pub revision: u64,
    /// Keys that read the chat in the last minute, as the hub last said.
    pub online: u32,
    /// Who reads the chat now and who read it last, as the hub last listed them;
    /// `None` until it does (hubs before the list never do).
    pub people: Option<People>,
    /// The last poll reached the hub.
    pub live: bool,
    /// What became of the last message or emote sent.
    pub outcome: Option<ReportOutcome>,
    /// `None` until the hub being read answered once; then a number that changes with
    /// each hub (or each time the reading starts again), so a reader can tell the
    /// backlog that came with that first answer from what is said afterwards.
    pub loaded: Option<u64>,
}

/// What the identity worker tells the feed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FeedShared {
    /// The hub to read; `None` while there is none to read.
    pub(crate) url: Option<String>,
    /// The game server whose emotes and looks to ask for.
    pub(crate) server: Option<String>,
    /// Whether the SJK chat shows: without it no message is kept.
    pub(crate) chat: bool,
    /// The player's own id at the hub ([`crate::Snapshot::key_id`]), to tell their
    /// drops from others'; empty for this PC's key.
    pub(crate) person: String,
    /// Changes whenever `url` or `server` does: looks read before belong to another
    /// reading (another hub or server, or the identity since turned off).
    pub(crate) generation: u64,
}

/// A look event queued for the viewer, with what it was read for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedLook {
    /// [`FeedShared::generation`] when the poll that brought it began.
    generation: u64,
    /// The game server that poll asked for.
    server: Option<SocketAddr>,
    event: LookEvent,
}

/// The look events the feed received for the viewer's server under the current
/// reading ([`crate::Service::take_looks`]).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReceivedLooks {
    /// The reading's generation: when it changes (the identity went off or on, another
    /// hub or server), every look received before is out of date, the roster's too.
    pub generation: u64,
    /// The events, oldest first.
    pub events: Vec<LookEvent>,
}

/// Take the queued looks: those read under the current generation for `server`, the
/// game server the viewer is on (it may be ahead of the worker); the rest are dropped.
pub(crate) fn take_looks(
    shared: &Mutex<FeedShared>,
    queue: &Mutex<VecDeque<QueuedLook>>,
    server: Option<SocketAddr>,
) -> ReceivedLooks {
    // Under the shared lock, so no new generation's looks are dropped as old ones.
    let shared = lock(shared);
    let mut queue = lock(queue);
    let events = queue
        .drain(..)
        .filter(|queued| {
            queued.generation == shared.generation && server.is_some() && queued.server == server
        })
        .map(|queued| queued.event)
        .collect();
    ReceivedLooks {
        generation: shared.generation,
        events,
    }
}

/// A ping queued for the viewer, with the game server the poll that brought it asked
/// about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedPing {
    server: Option<SocketAddr>,
    ping: Ping,
}

/// Take the queued pings read for `server`, the game server the viewer is on; those read
/// for another server (a poll under way when the player changed servers) are dropped.
pub(crate) fn take_pings(
    queue: &Mutex<VecDeque<QueuedPing>>,
    server: Option<SocketAddr>,
) -> Vec<Ping> {
    let mut queue = lock(queue);
    if server.is_none() {
        queue.clear();
        return Vec::new();
    }
    queue
        .drain(..)
        .filter(|queued| queued.server == server)
        .map(|queued| queued.ping)
        .collect()
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The feed's logic, advanced by [`FeedWorker::step`] with an explicit clock.
pub(crate) struct FeedWorker {
    identity: Identity,
    make_hub: HubFactory,
    shared: Arc<Mutex<FeedShared>>,
    state: Arc<Mutex<ChatState>>,
    emotes: Arc<Mutex<VecDeque<Emote>>>,
    looks: Arc<Mutex<VecDeque<QueuedLook>>>,
    /// The pings received, for [`take_pings`].
    pings: Arc<Mutex<VecDeque<QueuedPing>>>,
    hub: Option<Box<dyn Hub>>,
    url: Option<String>,
    /// The player's own id at the hub, to tell their drops from others': this PC's key
    /// id, or the person's id the worker tells ([`FeedShared::person`]).
    key_id: String,
    /// This PC's key id.
    local_key_id: String,
    /// Called when a drop of the player's own key arrives.
    own_drop: OwnDrop,
    /// The newest own drop told of, so a replay of the feed's last minute is not news.
    own_drop_told: u64,
    /// The game server read for, as last told.
    server: Option<String>,
    /// Whether the chat shows, as last told.
    chat: bool,
    after: u64,
    due: Instant,
    backoff: Duration,
    /// Counts the hubs read (`ChatState::loaded`).
    epoch: u64,
}

impl FeedWorker {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        identity: Identity,
        make_hub: HubFactory,
        shared: Arc<Mutex<FeedShared>>,
        state: Arc<Mutex<ChatState>>,
        emotes: Arc<Mutex<VecDeque<Emote>>>,
        looks: Arc<Mutex<VecDeque<QueuedLook>>>,
        own_drop: OwnDrop,
        now: Instant,
    ) -> Self {
        Self {
            key_id: identity.key_id(),
            local_key_id: identity.key_id(),
            own_drop,
            own_drop_told: 0,
            identity,
            make_hub,
            shared,
            state,
            emotes,
            looks,
            pings: Arc::default(),
            hub: None,
            url: None,
            server: None,
            chat: false,
            after: 0,
            due: now,
            backoff: RETRY_MIN,
            epoch: 0,
        }
    }

    /// The queue the pings received go to ([`take_pings`]).
    pub(crate) fn pings(&self) -> Arc<Mutex<VecDeque<QueuedPing>>> {
        Arc::clone(&self.pings)
    }

    /// Poll the hub if it is time to; return how long to wait before the next step.
    /// A poll blocks for as long as the hub holds it (at most [`WAIT`] seconds).
    pub(crate) fn step(&mut self, now: Instant) -> Duration {
        let shared = lock(&self.shared).clone();
        let person = if shared.person.is_empty() {
            &self.local_key_id
        } else {
            &shared.person
        };
        if *person != self.key_id {
            self.key_id.clone_from(person);
        }
        if shared.url != self.url {
            self.chat = shared.chat;
            self.switch(shared.url.clone(), now);
        } else if shared.chat != self.chat {
            self.show_chat(shared.chat, now);
        }
        // Another server: its looks of the last minute come with `after` 0, and the
        // old server's ids say nothing of it.
        if shared.server != self.server {
            self.server.clone_from(&shared.server);
            self.after = 0;
        }
        if self.url.is_none() {
            return IDLE;
        }
        if now < self.due {
            return self.due - now;
        }
        let Some(hub) = self.hub.as_mut() else {
            return IDLE;
        };
        match hub.feed(&self.identity, self.after, shared.server.as_deref(), WAIT) {
            Ok(feed) => {
                self.apply(feed, &shared);
                self.backoff = RETRY_MIN;
                self.due = now + POLL_GAP;
            }
            Err(_) => {
                self.set_live(false);
                self.due = now + self.backoff;
                self.backoff = (self.backoff * 2).min(RETRY_MAX);
            }
        }
        self.due.saturating_duration_since(now)
    }

    /// Read another hub, or none: start again from its backlog.
    fn switch(&mut self, url: Option<String>, now: Instant) {
        self.hub = url.as_deref().and_then(|url| (self.make_hub)(url).ok());
        self.url = url;
        self.after = 0;
        self.own_drop_told = 0;
        self.due = now;
        self.backoff = RETRY_MIN;
        self.clear_chat();
    }

    /// The chat was turned on or off while the same hub is read (on a game server).
    /// Off, the messages go; on, the reading starts again from the hub's backlog, as
    /// it would have had the feed been idle.
    fn show_chat(&mut self, on: bool, now: Instant) {
        self.chat = on;
        if on {
            self.after = 0;
            self.due = now;
        }
        self.clear_chat();
    }

    /// Forget the messages: a new backlog, under a new epoch, is due.
    fn clear_chat(&mut self) {
        self.epoch += 1;
        let mut state = lock(&self.state);
        state.loaded = None;
        state.messages.clear();
        state.live = false;
        state.online = 0;
        state.people = None;
        state.revision += 1;
    }

    fn set_live(&self, live: bool) {
        if !self.chat {
            return;
        }
        let mut state = lock(&self.state);
        if state.live != live {
            state.live = live;
            state.revision += 1;
        }
    }

    /// Take an answer to a poll made as `polled` says into the state: its messages
    /// only while the chat shows, its looks marked with the reading they belong to.
    fn apply(&mut self, mut feed: Feed, polled: &FeedShared) {
        self.tell_own_drops(&feed.drops);
        if self.chat {
            self.apply_chat(&mut feed);
        }
        queue(&self.emotes, feed.emotes, EMOTES_WAITING);
        let server = polled
            .server
            .as_deref()
            .and_then(|server| server.parse().ok());
        let pings = feed
            .pings
            .into_iter()
            .map(|ping| QueuedPing { server, ping });
        queue(&self.pings, pings, PINGS_WAITING);
        let looks = feed.looks.into_iter().map(|event| QueuedLook {
            generation: polled.generation,
            server,
            event,
        });
        queue(&self.looks, looks, LOOKS_WAITING);
        self.after = feed.next;
    }

    /// Tell the worker of the newest drop for the player's own key not told of yet.
    fn tell_own_drops(&mut self, drops: &[DropEvent]) {
        let newest = drops
            .iter()
            .filter(|drop| drop.key_id == self.key_id)
            .map(|drop| drop.id)
            .max();
        if let Some(id) = newest.filter(|id| *id > self.own_drop_told) {
            self.own_drop_told = id;
            (self.own_drop)();
        }
    }

    /// Take an answer's messages, deletions, online count and people into the chat.
    fn apply_chat(&self, feed: &mut Feed) {
        let mut state = lock(&self.state);
        let mut changed =
            !state.live || state.online != feed.online || state.loaded != Some(self.epoch);
        state.live = true;
        state.online = feed.online;
        // The hub lists them now and then: an answer without the list keeps the last.
        if let Some(people) = feed.people.take()
            && state.people.as_ref() != Some(&people)
        {
            state.people = Some(people);
            changed = true;
        }
        // Ids that go backwards mean the hub restarted: its backlog starts afresh.
        if feed.next < self.after {
            changed |= !state.messages.is_empty();
            state.messages.clear();
        }
        if !feed.deleted.is_empty() {
            let before = state.messages.len();
            state
                .messages
                .retain(|message| !feed.deleted.contains(&message.id));
            changed |= state.messages.len() != before;
        }
        // Drops share the messages' ids: merged, the chat keeps the order they came in.
        let mut incoming = std::mem::take(&mut feed.chat);
        if !feed.drops.is_empty() {
            let own = &self.key_id;
            incoming.extend(feed.drops.drain(..).map(|drop| drop_message(drop, own)));
            incoming.sort_by_key(|message| message.id);
        }
        for message in incoming {
            if state
                .messages
                .back()
                .is_some_and(|last| message.id <= last.id)
            {
                continue;
            }
            if state.messages.len() == MESSAGES_KEPT {
                state.messages.pop_front();
            }
            state.messages.push_back(message);
            changed = true;
        }
        state.loaded = Some(self.epoch);
        if changed {
            state.revision += 1;
        }
    }
}

/// The chat entry for `drop`, as `own` (the player's key id) sees it. Its text is left
/// empty: the viewer words the line from its tier.
fn drop_message(drop: DropEvent, own: &str) -> ChatMessage {
    ChatMessage {
        id: drop.id,
        at: drop.at,
        holocron: Some(DropMark {
            own: drop.key_id == own,
            tier: drop.tier,
        }),
        key_id: drop.key_id,
        name: drop.name,
        verified: drop.verified,
        staff: false,
        text: String::new(),
    }
}

/// Add `received` to `waiting`, keeping the newest `kept`.
fn queue<T>(waiting: &Mutex<VecDeque<T>>, received: impl IntoIterator<Item = T>, kept: usize) {
    let mut received = received.into_iter().peekable();
    if received.peek().is_none() {
        return;
    }
    let mut waiting = lock(waiting);
    for item in received {
        if waiting.len() == kept {
            waiting.pop_front();
        }
        waiting.push_back(item);
    }
}

/// Run the feed on this thread until `stop` is set.
pub(crate) fn run(mut worker: FeedWorker, stop: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    while !stop.load(Ordering::Relaxed) {
        let wait = worker.step(Instant::now());
        // Short sleeps, so a change of hub or a stop is seen within a second.
        std::thread::sleep(wait.min(IDLE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::HubError;
    use crate::wire::Profile;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Something that happens while a poll is under way.
    type Meanwhile = Box<dyn FnOnce() + Send>;

    /// A hub whose feed answers come from a script.
    #[derive(Clone, Default)]
    struct Scripted {
        answers: Arc<Mutex<VecDeque<Result<Feed, HubError>>>>,
        asked: Arc<Mutex<Vec<String>>>,
        down: Arc<AtomicBool>,
        meanwhile: Arc<Mutex<Option<Meanwhile>>>,
    }

    impl Hub for Scripted {
        fn register(&mut self, _: &Identity, _: Option<&str>) -> Result<Profile, HubError> {
            unreachable!()
        }
        fn set_bio(&mut self, _: &Identity, _: &str) -> Result<Profile, HubError> {
            unreachable!()
        }
        fn profile(&mut self, _: &str) -> Result<Profile, HubError> {
            unreachable!()
        }
        fn claim(
            &mut self,
            _: &Identity,
            _: &str,
            _: u8,
            _: &str,
            _: bool,
        ) -> Result<(), HubError> {
            unreachable!()
        }
        fn release(&mut self, _: &Identity, _: &str) -> Result<(), HubError> {
            unreachable!()
        }
        fn presence(&mut self, _: &str) -> Result<Vec<crate::Presence>, HubError> {
            unreachable!()
        }
        fn feed(
            &mut self,
            _: &Identity,
            after: u64,
            server: Option<&str>,
            wait: u64,
        ) -> Result<Feed, HubError> {
            self.asked
                .lock()
                .unwrap()
                .push(format!("after={after} server={server:?} wait={wait}"));
            if let Some(meanwhile) = self.meanwhile.lock().unwrap().take() {
                meanwhile();
            }
            if self.down.load(Ordering::SeqCst) {
                return Err(HubError::Network("down".to_owned()));
            }
            self.answers
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Ok(Feed::default()))
        }
    }

    fn message(id: u64, text: &str) -> ChatMessage {
        ChatMessage {
            id,
            at: 0,
            key_id: "aa".to_owned(),
            name: "Sol".to_owned(),
            verified: false,
            staff: false,
            text: text.to_owned(),
            holocron: None,
        }
    }

    fn answer(next: u64, chat: Vec<ChatMessage>) -> Result<Feed, HubError> {
        Ok(Feed {
            next,
            chat,
            online: 3,
            ..Feed::default()
        })
    }

    struct Rig {
        hub: Scripted,
        shared: Arc<Mutex<FeedShared>>,
        state: Arc<Mutex<ChatState>>,
        emotes: Arc<Mutex<VecDeque<Emote>>>,
        looks: Arc<Mutex<VecDeque<QueuedLook>>>,
        worker: FeedWorker,
        made: Arc<Mutex<Vec<String>>>,
        /// How many times the worker was told of a drop for the player's own key.
        told: Arc<std::sync::atomic::AtomicUsize>,
    }

    fn rig(now: Instant) -> Rig {
        let hub = Scripted::default();
        let shared = Arc::new(Mutex::new(FeedShared::default()));
        let state = Arc::new(Mutex::new(ChatState::default()));
        let emotes = Arc::new(Mutex::new(VecDeque::new()));
        let looks = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(Vec::new()));
        let told = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (factory, log) = (hub.clone(), Arc::clone(&made));
        let counter = Arc::clone(&told);
        let make: HubFactory = Box::new(move |url| {
            log.lock().unwrap().push(url.to_owned());
            Ok(Box::new(factory.clone()))
        });
        let worker = FeedWorker::new(
            Identity::from_seed([1; 32]),
            make,
            Arc::clone(&shared),
            Arc::clone(&state),
            Arc::clone(&emotes),
            Arc::clone(&looks),
            Box::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            now,
        );
        Rig {
            hub,
            shared,
            state,
            emotes,
            looks,
            worker,
            made,
            told,
        }
    }

    impl Rig {
        fn script(&self, answer: Result<Feed, HubError>) {
            self.hub.answers.lock().unwrap().push_back(answer);
        }
        fn asked(&self) -> Vec<String> {
            self.hub.asked.lock().unwrap().clone()
        }
        fn texts(&self) -> Vec<String> {
            lock(&self.state)
                .messages
                .iter()
                .map(|m| m.text.clone())
                .collect()
        }
        fn read(&self, url: &str, server: Option<&str>) {
            let mut shared = lock(&self.shared);
            *shared = FeedShared {
                url: Some(url.to_owned()),
                server: server.map(str::to_owned),
                chat: true,
                person: String::new(),
                generation: shared.generation + 1,
            };
        }
        /// The looks waiting for a viewer on `server`.
        fn take_looks(&self, server: &str) -> ReceivedLooks {
            take_looks(&self.shared, &self.looks, server.parse().ok())
        }
        fn show_chat(&self, on: bool) {
            lock(&self.shared).chat = on;
        }
    }

    fn drop_event(id: u64, key_id: &str, tier: &str) -> DropEvent {
        DropEvent {
            id,
            at: 0,
            key_id: key_id.to_owned(),
            name: "^2Sol".to_owned(),
            tier: tier.to_owned(),
            verified: true,
        }
    }

    #[test]
    fn drops_join_the_chat_in_the_feeds_order() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        let own = Identity::from_seed([1; 32]).key_id();
        rig.script(Ok(Feed {
            next: 5,
            chat: vec![message(3, "gg"), message(5, "wp")],
            drops: vec![
                drop_event(4, "ff", "legendary"),
                drop_event(2, &own, "rare"),
            ],
            online: 1,
            ..Feed::default()
        }));
        rig.worker.step(t0);
        let state = lock(&rig.state).clone();
        let ids: Vec<u64> = state.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, [2, 3, 4, 5], "one sequence, drops among messages");
        let marks: Vec<Option<(&str, bool)>> = state
            .messages
            .iter()
            .map(|m| m.holocron.as_ref().map(|d| (d.tier.as_str(), d.own)))
            .collect();
        assert_eq!(
            marks,
            [Some(("rare", true)), None, Some(("legendary", false)), None]
        );
        let first = &state.messages[0];
        assert_eq!(
            (first.name.as_str(), first.text.as_str(), first.verified),
            ("^2Sol", "", true)
        );
        // A replay of what was shown is skipped like any message.
        rig.script(Ok(Feed {
            next: 5,
            drops: vec![drop_event(4, "ff", "legendary")],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP);
        assert_eq!(lock(&rig.state).messages.len(), 4);
    }

    #[test]
    fn a_drop_for_the_own_key_tells_the_worker_once_even_with_the_chat_off() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        rig.show_chat(false);
        let own = Identity::from_seed([1; 32]).key_id();
        rig.script(Ok(Feed {
            next: 8,
            drops: vec![
                drop_event(7, "ff", "mythical"),
                drop_event(8, &own, "uncommon"),
            ],
            ..Feed::default()
        }));
        rig.worker.step(t0);
        assert_eq!(rig.told.load(Ordering::SeqCst), 1, "the own key's only");
        assert!(lock(&rig.state).messages.is_empty(), "the chat is off");
        // The same drop again (a reader that started over) is not news; a newer one is.
        rig.script(Ok(Feed {
            next: 8,
            drops: vec![drop_event(8, &own, "uncommon")],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP);
        assert_eq!(rig.told.load(Ordering::SeqCst), 1);
        rig.script(Ok(Feed {
            next: 9,
            drops: vec![drop_event(9, &own, "rare")],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP * 2);
        assert_eq!(rig.told.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn nothing_is_asked_while_there_is_no_hub_to_read() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        assert_eq!(rig.worker.step(t0), IDLE);
        assert!(rig.asked().is_empty());
        assert!(rig.made.lock().unwrap().is_empty());
        assert!(!lock(&rig.state).live);
    }

    #[test]
    fn the_people_listed_stay_until_the_hub_lists_others_or_the_chat_goes_off() {
        use crate::wire::Person;
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        let people = |names: &[&str]| People {
            online: names
                .iter()
                .map(|name| Person {
                    key_id: format!("{name:0>16}"),
                    name: (*name).to_owned(),
                    ..Person::default()
                })
                .collect(),
            recent: Vec::new(),
        };
        let with = |next, listed: Option<People>| {
            Ok(Feed {
                next,
                online: 3,
                people: listed,
                ..Feed::default()
            })
        };
        rig.script(answer(1, vec![message(1, "a")]));
        rig.worker.step(t0);
        assert_eq!(lock(&rig.state).people, None, "a hub that lists nobody");
        rig.script(with(1, Some(people(&["Sol", "Fox"]))));
        rig.worker.step(t0 + POLL_GAP);
        let state = lock(&rig.state).clone();
        assert_eq!(state.people, Some(people(&["Sol", "Fox"])));
        // An answer without the list keeps it; the same list changes nothing.
        rig.script(with(1, None));
        rig.script(with(1, Some(people(&["Sol", "Fox"]))));
        rig.worker.step(t0 + POLL_GAP * 2);
        rig.worker.step(t0 + POLL_GAP * 3);
        assert_eq!(*lock(&rig.state), state);
        rig.script(with(1, Some(people(&["Fox"]))));
        rig.worker.step(t0 + POLL_GAP * 4);
        let state = lock(&rig.state).clone();
        assert_eq!(state.people, Some(people(&["Fox"])));
        // The chat off: nobody shows.
        rig.show_chat(false);
        rig.worker.step(t0 + POLL_GAP * 5);
        assert_eq!(lock(&rig.state).people, None);
    }

    #[test]
    fn the_feed_appends_in_order_and_skips_repeats() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", Some("1.2.3.4:29070"));
        rig.script(answer(2, vec![message(1, "a"), message(2, "b")]));
        rig.script(answer(3, vec![message(2, "b"), message(3, "c")]));
        rig.worker.step(t0);
        let revision = lock(&rig.state).revision;
        rig.worker.step(t0 + Duration::from_secs(2));
        assert_eq!(rig.texts(), ["a", "b", "c"]);
        assert_eq!(
            rig.asked(),
            [
                "after=0 server=Some(\"1.2.3.4:29070\") wait=25",
                "after=2 server=Some(\"1.2.3.4:29070\") wait=25"
            ]
        );
        let state = lock(&rig.state).clone();
        assert!(state.live);
        assert_eq!(state.online, 3);
        assert!(state.revision > revision);
        // An answer with nothing new changes nothing.
        rig.script(answer(3, Vec::new()));
        rig.worker.step(t0 + Duration::from_secs(4));
        assert_eq!(lock(&rig.state).revision, state.revision);
    }

    #[test]
    fn the_last_200_messages_are_kept() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        let batch: Vec<ChatMessage> = (1..=250).map(|id| message(id, &id.to_string())).collect();
        rig.script(answer(250, batch));
        rig.worker.step(t0);
        let texts = rig.texts();
        assert_eq!(texts.len(), MESSAGES_KEPT);
        assert_eq!(texts[0], "51");
    }

    #[test]
    fn deleted_ids_leave_the_state() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        rig.script(answer(2, vec![message(1, "spam"), message(2, "hi")]));
        rig.script(Ok(Feed {
            next: 3,
            deleted: vec![1],
            ..Feed::default()
        }));
        rig.worker.step(t0);
        rig.worker.step(t0 + Duration::from_secs(2));
        assert_eq!(rig.texts(), ["hi"]);
    }

    #[test]
    fn pings_wait_for_the_viewer_of_their_server() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub.example", Some("1.2.3.4:29070"));
        let mark = |id| Ping {
            id,
            at: 0,
            slot: 3,
            claimed_name: "^2Sol".to_owned(),
            key_id: "aa".to_owned(),
            pos: [1, 2, 3],
            scope: "all".to_owned(),
        };
        rig.script(Ok(Feed {
            next: 2,
            pings: vec![mark(1), mark(2)],
            ..Feed::default()
        }));
        rig.worker.step(t0);
        let queue = rig.worker.pings();
        let here = Some("1.2.3.4:29070".parse().unwrap());
        // Another server takes none and drops them: they were not read for it.
        assert!(take_pings(&queue, Some("5.6.7.8:29070".parse().unwrap())).is_empty());
        assert!(take_pings(&queue, here).is_empty());
        rig.script(Ok(Feed {
            next: 3,
            pings: vec![mark(3)],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP);
        let taken = take_pings(&queue, here);
        assert_eq!(taken.iter().map(|ping| ping.id).collect::<Vec<_>>(), [3]);
        assert!(take_pings(&queue, here).is_empty(), "taken once");
        // Out of a game, the queue empties.
        rig.script(Ok(Feed {
            next: 4,
            pings: vec![mark(4)],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP * 2);
        assert!(take_pings(&queue, None).is_empty());
        assert!(take_pings(&queue, here).is_empty());
    }

    #[test]
    fn emotes_wait_for_the_viewer() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", Some("1.2.3.4:29070"));
        let wave = Emote {
            id: 4,
            at: 0,
            slot: 3,
            claimed_name: "Sol".to_owned(),
            key_id: "aa".to_owned(),
            emote: "wave".to_owned(),
            sync: None,
        };
        rig.script(Ok(Feed {
            next: 4,
            emotes: vec![wave.clone()],
            ..Feed::default()
        }));
        rig.worker.step(t0);
        assert_eq!(
            lock(&rig.emotes).iter().cloned().collect::<Vec<_>>(),
            [wave]
        );
    }

    fn look_event(id: u64, illuminate: bool) -> LookEvent {
        LookEvent {
            id,
            at: 0,
            slot: 3,
            claimed_name: "Sol".to_owned(),
            key_id: "aa".to_owned(),
            saber: "saber_sun".to_owned(),
            illuminate,
            saber_off: Vec::new(),
        }
    }

    #[test]
    fn looks_wait_for_the_viewer_and_the_newest_are_kept() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", Some("1.2.3.4:29070"));
        let batch: Vec<LookEvent> = (1..=70).map(|id| look_event(id, id % 2 == 0)).collect();
        rig.script(Ok(Feed {
            next: 70,
            looks: batch,
            ..Feed::default()
        }));
        rig.worker.step(t0);
        let waiting = rig.take_looks("1.2.3.4:29070").events;
        assert_eq!(waiting.len(), LOOKS_WAITING);
        assert_eq!(waiting.first().map(|look| look.id), Some(7));
        assert_eq!(waiting.last(), Some(&look_event(70, true)));
        assert!(lock(&rig.looks).is_empty(), "taken");
    }

    #[test]
    fn another_server_is_read_from_the_start_for_its_looks() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", Some("1.2.3.4:29070"));
        rig.script(answer(9, vec![message(9, "hi")]));
        rig.worker.step(t0);
        rig.read("https://hub", Some("5.6.7.8:29070"));
        rig.script(Ok(Feed {
            next: 9,
            chat: vec![message(9, "hi")],
            looks: vec![look_event(8, true)],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP);
        assert_eq!(
            rig.asked()[1],
            "after=0 server=Some(\"5.6.7.8:29070\") wait=25",
            "the hub's answer to 0 carries the server's looks of the last minute"
        );
        assert_eq!(rig.texts(), ["hi"], "the same hub: the chat goes on");
        assert_eq!(
            rig.take_looks("5.6.7.8:29070").events,
            [look_event(8, true)]
        );
        rig.worker.step(t0 + POLL_GAP * 2);
        assert_eq!(
            rig.asked()[2],
            "after=9 server=Some(\"5.6.7.8:29070\") wait=25"
        );
    }

    #[test]
    fn looks_of_a_poll_under_way_when_the_reading_changed_are_dropped() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        let old = "1.2.3.4:29070";
        rig.read("https://hub", Some(old));
        // The player moves to another server while the poll waits at the hub.
        let shared = Arc::clone(&rig.shared);
        *rig.hub.meanwhile.lock().unwrap() = Some(Box::new(move || {
            let mut shared = lock(&shared);
            shared.server = Some("5.6.7.8:29070".to_owned());
            shared.generation += 1;
        }));
        rig.script(Ok(Feed {
            next: 5,
            looks: vec![look_event(5, true)],
            ..Feed::default()
        }));
        rig.worker.step(t0);
        let taken = rig.take_looks("5.6.7.8:29070");
        assert_eq!(taken.generation, lock(&rig.shared).generation);
        assert!(taken.events.is_empty(), "read for the old server");
        // The viewer is still on the old server, but the reading changed: dropped too.
        rig.read("https://hub", Some(old));
        rig.script(Ok(Feed {
            next: 6,
            looks: vec![look_event(6, true)],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP);
        let shared = Arc::clone(&rig.shared);
        *rig.hub.meanwhile.lock().unwrap() = Some(Box::new(move || {
            // The identity is turned off.
            let mut shared = lock(&shared);
            *shared = FeedShared {
                generation: shared.generation + 1,
                ..FeedShared::default()
            };
        }));
        rig.script(Ok(Feed {
            next: 7,
            looks: vec![look_event(7, false)],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP * 2);
        let taken = rig.take_looks(old);
        assert!(taken.events.is_empty(), "{:?}", taken.events);
        // Without a change, the same server's looks are handed over.
        rig.read("https://hub", Some(old));
        rig.script(Ok(Feed {
            next: 8,
            looks: vec![look_event(8, true)],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP * 3);
        assert!(rig.take_looks("5.6.7.8:29070").events.is_empty());
        rig.script(Ok(Feed {
            next: 9,
            looks: vec![look_event(9, true)],
            ..Feed::default()
        }));
        rig.worker.step(t0 + POLL_GAP * 4);
        assert_eq!(rig.take_looks(old).events, [look_event(9, true)]);
    }

    #[test]
    fn with_the_chat_off_the_feed_keeps_looks_and_emotes_but_no_message() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", Some("1.2.3.4:29070"));
        rig.show_chat(false);
        rig.script(Ok(Feed {
            next: 5,
            chat: vec![message(4, "hidden")],
            looks: vec![look_event(5, true)],
            online: 9,
            ..Feed::default()
        }));
        rig.worker.step(t0);
        assert_eq!(
            rig.asked(),
            ["after=0 server=Some(\"1.2.3.4:29070\") wait=25"]
        );
        assert_eq!(lock(&rig.looks).len(), 1);
        // The chat is as an idle feed leaves it: nothing the page or the game shows.
        let idle = |state: &ChatState| ChatState {
            revision: state.revision,
            ..ChatState::default()
        };
        let state = lock(&rig.state).clone();
        assert_eq!(state, idle(&state));
        // A failure says nothing either.
        rig.hub.down.store(true, Ordering::SeqCst);
        rig.worker.step(t0 + POLL_GAP);
        assert_eq!(*lock(&rig.state), state);
        rig.hub.down.store(false, Ordering::SeqCst);
        // On again: the backlog is read from the start, under a new epoch.
        rig.show_chat(true);
        rig.script(answer(6, vec![message(4, "hidden"), message(6, "hello")]));
        rig.worker.step(t0 + POLL_GAP * 2);
        assert_eq!(
            rig.asked()[2],
            "after=0 server=Some(\"1.2.3.4:29070\") wait=25"
        );
        assert_eq!(rig.texts(), ["hidden", "hello"]);
        let first = lock(&rig.state).loaded.expect("loaded");
        // Off once more: the messages go, and the reading goes on from where it was.
        rig.show_chat(false);
        rig.worker.step(t0 + POLL_GAP * 3);
        assert!(rig.texts().is_empty());
        assert_eq!(lock(&rig.state).loaded, None);
        assert_eq!(
            rig.asked()[3],
            "after=6 server=Some(\"1.2.3.4:29070\") wait=25"
        );
        rig.show_chat(true);
        rig.worker.step(t0 + POLL_GAP * 4);
        assert_ne!(lock(&rig.state).loaded, Some(first));
    }

    #[test]
    fn a_new_hub_starts_from_the_backlog() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://one", None);
        rig.script(answer(9, vec![message(9, "old hub")]));
        rig.worker.step(t0);
        rig.read("https://two", None);
        rig.script(answer(1, vec![message(1, "new hub")]));
        rig.worker.step(t0 + Duration::from_secs(2));
        assert_eq!(*rig.made.lock().unwrap(), ["https://one", "https://two"]);
        assert_eq!(rig.asked()[1], "after=0 server=None wait=25");
        assert_eq!(rig.texts(), ["new hub"]);
    }

    #[test]
    fn the_backlog_is_marked_loaded_once_the_hub_answered() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        assert_eq!(lock(&rig.state).loaded, None);
        rig.read("https://one", None);
        rig.hub.down.store(true, Ordering::SeqCst);
        rig.worker.step(t0);
        assert_eq!(lock(&rig.state).loaded, None, "no answer yet");
        rig.hub.down.store(false, Ordering::SeqCst);
        rig.script(answer(2, vec![message(1, "a"), message(2, "b")]));
        rig.worker.step(t0 + RETRY_MIN);
        let first = lock(&rig.state).loaded.expect("loaded");
        // Another hub: not loaded until it answers, then under another epoch.
        rig.read("https://two", None);
        rig.hub.down.store(true, Ordering::SeqCst);
        rig.worker.step(t0 + RETRY_MIN * 2);
        assert_eq!(lock(&rig.state).loaded, None);
        rig.hub.down.store(false, Ordering::SeqCst);
        rig.worker.step(t0 + RETRY_MIN * 4);
        let second = lock(&rig.state).loaded.expect("loaded again");
        assert_ne!(first, second);
    }

    #[test]
    fn a_hub_that_restarted_starts_from_its_backlog() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        rig.script(answer(90, vec![message(90, "before")]));
        // The hub restarted: its ids begin again, below what was asked.
        rig.script(answer(
            2,
            vec![message(1, "after a"), message(2, "after b")],
        ));
        rig.worker.step(t0);
        rig.worker.step(t0 + Duration::from_secs(2));
        assert_eq!(rig.texts(), ["after a", "after b"]);
        rig.worker.step(t0 + Duration::from_secs(4));
        assert_eq!(rig.asked()[2], "after=2 server=None wait=25");
    }

    #[test]
    fn feed_stops_when_chat_turns_off() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        rig.script(answer(1, vec![message(1, "a")]));
        rig.worker.step(t0);
        *lock(&rig.shared) = FeedShared::default();
        assert_eq!(rig.worker.step(t0 + Duration::from_secs(2)), IDLE);
        assert_eq!(rig.asked().len(), 1);
        let state = lock(&rig.state).clone();
        assert!(!state.live);
        assert!(
            state.messages.is_empty(),
            "the chat is off: nothing to show"
        );
    }

    #[test]
    fn polls_are_two_seconds_apart() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        assert_eq!(rig.worker.step(t0), POLL_GAP);
        assert_eq!(
            rig.worker.step(t0 + Duration::from_millis(500)),
            Duration::from_millis(1_500)
        );
        assert_eq!(rig.asked().len(), 1);
        rig.worker.step(t0 + POLL_GAP);
        assert_eq!(rig.asked().len(), 2);
    }

    #[test]
    fn failures_back_off() {
        let t0 = Instant::now();
        let mut rig = rig(t0);
        rig.read("https://hub", None);
        rig.hub.down.store(true, Ordering::SeqCst);
        assert_eq!(rig.worker.step(t0), RETRY_MIN);
        assert!(!lock(&rig.state).live);
        assert_eq!(rig.worker.step(t0 + RETRY_MIN), RETRY_MIN * 2);
        rig.hub.down.store(false, Ordering::SeqCst);
        rig.worker.step(t0 + RETRY_MIN * 3);
        assert!(lock(&rig.state).live);
        assert_eq!(rig.asked().len(), 3);
    }
}
