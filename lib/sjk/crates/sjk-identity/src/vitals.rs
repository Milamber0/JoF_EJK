//! Shared vitals (`PROTOCOL.md`, "Vitals"): a thread of its own that tells the hub the
//! health, shield and Force the player shares on their nameplate, and reads those of
//! the other SJK players of their game server, so the nameplates show real bars for
//! them. A request goes every half second while the player shares or somebody there
//! does, every [`SLOW`] otherwise, and never waits behind the identity worker's claims
//! or the feed's long poll.
//!
//! It reads the hub and the game server from [`FeedShared`] (set only while the
//! identity is on, registered and on a server), and what to share from [`Sharing`],
//! which the viewer sets every frame. The hub needs a live claim on the server, so the
//! first answers after joining are refusals until the claim is in.

use crate::feed::{FeedShared, lock};
use crate::hub::{Hub, HubError};
use crate::keys::Identity;
use crate::service::HubFactory;
use crate::wire::{SharedVitals, Vitals};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Time between two requests while the player shares or somebody on the server does.
pub const FAST: Duration = Duration::from_millis(500);
/// Time between two requests while nobody shares: only to notice a sharer.
pub const SLOW: Duration = Duration::from_secs(2);
/// The HTTP timeout: an answer later than this is out of date anyway.
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// First wait after a failure, doubled up to [`RETRY_MAX`].
const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(30);
/// How often an idle worker looks at what it is told.
const IDLE: Duration = Duration::from_millis(250);

/// What the player shares, as the viewer last set it ([`crate::Service::set_sharing`]).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sharing {
    /// The player shares their values.
    pub share: bool,
    /// The player sees everyone's estimated bars, which makes them share.
    pub estimates: bool,
    /// The values, while the player has some (alive in a match).
    pub vitals: Option<Vitals>,
}

/// The sharers of the player's game server, as the hub last listed them.
#[derive(Clone, Debug, Default)]
pub struct VitalsTable {
    /// The game server they were read for.
    pub server: Option<SocketAddr>,
    /// The players listed, the player's own entry included, by slot.
    pub players: Vec<SharedVitals>,
    /// When the hub answered.
    pub read_at: Option<Instant>,
    /// Counts the answers, so a reader knows a new one came.
    pub revision: u64,
}

/// The worker's logic, advanced by [`VitalsWorker::step`] with an explicit clock.
pub(crate) struct VitalsWorker {
    identity: Identity,
    make_hub: HubFactory,
    shared: Arc<Mutex<FeedShared>>,
    sharing: Arc<Mutex<Sharing>>,
    table: Arc<Mutex<VitalsTable>>,
    hub: Option<Box<dyn Hub>>,
    url: Option<String>,
    server: Option<String>,
    /// `share` and `estimates` as the hub last took them: a change is sent at once.
    sent: Option<(bool, bool)>,
    due: Instant,
    backoff: Duration,
}

impl VitalsWorker {
    pub(crate) fn new(
        identity: Identity,
        make_hub: HubFactory,
        shared: Arc<Mutex<FeedShared>>,
        sharing: Arc<Mutex<Sharing>>,
        table: Arc<Mutex<VitalsTable>>,
        now: Instant,
    ) -> Self {
        Self {
            identity,
            make_hub,
            shared,
            sharing,
            table,
            hub: None,
            url: None,
            server: None,
            sent: None,
            due: now,
            backoff: RETRY_MIN,
        }
    }

    /// Post if it is time to; return how long to wait before the next step.
    pub(crate) fn step(&mut self, now: Instant) -> Duration {
        let shared = lock(&self.shared).clone();
        if shared.url != self.url {
            self.hub = shared
                .url
                .as_deref()
                .and_then(|url| (self.make_hub)(url).ok());
            self.url = shared.url;
            self.restart(now);
        }
        if shared.server != self.server {
            self.server = shared.server;
            self.restart(now);
        }
        let (Some(server), Some(hub)) = (self.server.as_deref(), self.hub.as_mut()) else {
            return IDLE;
        };
        let sharing = *lock(&self.sharing);
        if self.sent != Some((sharing.share, sharing.estimates)) {
            self.due = self.due.min(now);
        }
        if now < self.due {
            return (self.due - now).min(IDLE);
        }
        match hub.vitals(&self.identity, server, &sharing) {
            Ok(players) => {
                let busy = sharing.share
                    || players
                        .iter()
                        .any(|player| player.share && player.vitals.is_some());
                let mut table = lock(&self.table);
                *table = VitalsTable {
                    server: server.parse().ok(),
                    players,
                    read_at: Some(now),
                    revision: table.revision + 1,
                };
                self.sent = Some((sharing.share, sharing.estimates));
                self.backoff = RETRY_MIN;
                self.due = now + if busy { FAST } else { SLOW };
            }
            // The claim is not in yet (just joined), or the key is not registered.
            Err(HubError::Rejected { status: 403, .. }) => {
                lock(&self.table).players.clear();
                self.due = now + SLOW;
            }
            Err(_) => {
                self.due = now + self.backoff;
                self.backoff = (self.backoff * 2).min(RETRY_MAX);
            }
        }
        self.due.saturating_duration_since(now).min(IDLE)
    }

    /// Another hub or server, or none: forget the table and post at once.
    fn restart(&mut self, now: Instant) {
        let mut table = lock(&self.table);
        table.players.clear();
        table.server = None;
        table.read_at = None;
        self.sent = None;
        self.due = now;
        self.backoff = RETRY_MIN;
    }
}

/// Run `worker` until `stop` is set.
pub(crate) fn run(mut worker: VitalsWorker, stop: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    while !stop.load(Ordering::Relaxed) {
        let wait = worker.step(Instant::now());
        std::thread::sleep(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Profile;

    type Answer = Result<Vec<SharedVitals>, HubError>;

    /// A hub whose vitals answers are scripted, recording what it was sent.
    #[derive(Clone, Default)]
    struct Fake {
        sent: Arc<Mutex<Vec<(String, Sharing)>>>,
        answers: Arc<Mutex<Vec<Answer>>>,
    }

    impl Hub for Fake {
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
        fn vitals(
            &mut self,
            _identity: &Identity,
            server: &str,
            sharing: &Sharing,
        ) -> Result<Vec<SharedVitals>, HubError> {
            lock(&self.sent).push((server.to_owned(), *sharing));
            let mut answers = lock(&self.answers);
            if answers.is_empty() {
                Ok(Vec::new())
            } else {
                answers.remove(0)
            }
        }
    }

    fn sharer(slot: u8, health: i32) -> SharedVitals {
        SharedVitals {
            slot,
            claimed_name: "Fox".to_owned(),
            key_id: "k".to_owned(),
            share: true,
            estimates: false,
            age: 0,
            vitals: Some(Vitals {
                t: 1000,
                health,
                armor: 0,
                force: 100,
                max_health: 100,
                max_force: 100,
            }),
        }
    }

    struct Setup {
        fake: Fake,
        shared: Arc<Mutex<FeedShared>>,
        sharing: Arc<Mutex<Sharing>>,
        table: Arc<Mutex<VitalsTable>>,
        worker: VitalsWorker,
    }

    fn setup(now: Instant) -> Setup {
        let fake = Fake::default();
        let made = fake.clone();
        let shared = Arc::new(Mutex::new(FeedShared::default()));
        let sharing = Arc::new(Mutex::new(Sharing::default()));
        let table = Arc::new(Mutex::new(VitalsTable::default()));
        let worker = VitalsWorker::new(
            Identity::from_seed([7; 32]),
            Box::new(move |_| Ok(Box::new(made.clone()) as Box<dyn Hub>)),
            Arc::clone(&shared),
            Arc::clone(&sharing),
            Arc::clone(&table),
            now,
        );
        Setup {
            fake,
            shared,
            sharing,
            table,
            worker,
        }
    }

    fn on_server(shared: &Mutex<FeedShared>) {
        *lock(shared) = FeedShared {
            url: Some("https://hub.example".to_owned()),
            server: Some("1.2.3.4:29070".to_owned()),
            ..FeedShared::default()
        };
    }

    #[test]
    fn nothing_goes_without_a_hub_and_a_server() {
        let t0 = Instant::now();
        let mut setup = setup(t0);
        setup.worker.step(t0);
        lock(&setup.shared).url = Some("https://hub.example".to_owned());
        setup.worker.step(t0);
        assert!(lock(&setup.fake.sent).is_empty());
        on_server(&setup.shared);
        setup.worker.step(t0);
        assert_eq!(lock(&setup.fake.sent).len(), 1);
    }

    #[test]
    fn readers_poll_slowly_until_somebody_shares_and_a_change_goes_at_once() {
        let t0 = Instant::now();
        let mut setup = setup(t0);
        on_server(&setup.shared);
        setup.worker.step(t0);
        // Nobody shares: the next request waits SLOW.
        setup.worker.step(t0 + FAST);
        assert_eq!(lock(&setup.fake.sent).len(), 1);
        lock(&setup.fake.answers).push(Ok(vec![sharer(3, 80)]));
        setup.worker.step(t0 + SLOW);
        assert_eq!(lock(&setup.fake.sent).len(), 2);
        assert_eq!(lock(&setup.table).players[0].slot, 3);
        assert_eq!(lock(&setup.table).revision, 2);
        // Somebody shares: FAST.
        setup.worker.step(t0 + SLOW + FAST);
        assert_eq!(lock(&setup.fake.sent).len(), 3);
        // Turning sharing on goes at once.
        *lock(&setup.sharing) = Sharing {
            share: true,
            ..Sharing::default()
        };
        setup
            .worker
            .step(t0 + SLOW + FAST + Duration::from_millis(10));
        let sent = lock(&setup.fake.sent);
        assert_eq!(sent.len(), 4);
        assert!(sent[3].1.share);
    }

    #[test]
    fn a_refusal_clears_the_table_and_another_server_starts_afresh() {
        let t0 = Instant::now();
        let mut setup = setup(t0);
        on_server(&setup.shared);
        lock(&setup.fake.answers).push(Ok(vec![sharer(3, 80)]));
        setup.worker.step(t0);
        assert_eq!(lock(&setup.table).players.len(), 1);
        lock(&setup.fake.answers).push(Err(HubError::Rejected {
            status: 403,
            code: "not_on_server".to_owned(),
            message: String::new(),
        }));
        setup.worker.step(t0 + FAST);
        assert!(lock(&setup.table).players.is_empty());
        lock(&setup.fake.answers).push(Ok(vec![sharer(3, 80)]));
        setup.worker.step(t0 + FAST + SLOW);
        lock(&setup.shared).server = Some("5.6.7.8:29070".to_owned());
        setup
            .worker
            .step(t0 + FAST + SLOW + Duration::from_millis(1));
        let sent = lock(&setup.fake.sent);
        assert_eq!(sent.last().unwrap().0, "5.6.7.8:29070");
        assert_eq!(
            lock(&setup.table).server,
            Some("5.6.7.8:29070".parse().unwrap())
        );
    }
}
