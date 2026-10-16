//! Details for the selected browser row: a debounced background `getstatus`
//! whose reply is preformatted once so the panel renders without allocating.

use sjk_network::{ServerStatus, query_server_status};
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

/// Wait for the selection to settle before querying the server.
const DEBOUNCE: Duration = Duration::from_millis(250);
const STATUS_TIMEOUT: Duration = Duration::from_millis(900);
/// Server cvars shown on the details panel, in order, with their labels.
const FACTS: [(&str, &str); 10] = [
    ("version", "Version"),
    ("gamename", "Mod"),
    ("g_gametype", "Gametype"),
    ("fraglimit", "Frag limit"),
    ("timelimit", "Time limit"),
    ("duel_fraglimit", "Duel limit"),
    ("capturelimit", "Capture limit"),
    ("g_needpass", "Password"),
    ("sv_privateClients", "Private slots"),
    ("bot_minplayers", "Bot fill"),
];

/// One `label: value` line of the facts list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fact {
    pub(crate) label: &'static str,
    pub(crate) value: String,
}

/// One connected client, colour codes kept for the text renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetailPlayer {
    pub(crate) score: i32,
    pub(crate) ping: i32,
    pub(crate) name: String,
}

/// A queried server's status, shaped for the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetailsView {
    pub(crate) address: SocketAddr,
    pub(crate) hostname: String,
    pub(crate) map: String,
    pub(crate) facts: Vec<Fact>,
    pub(crate) players: Vec<DetailPlayer>,
    /// Bots reported by the server (ping 0), for the player-count caption.
    pub(crate) bots: usize,
}

impl DetailsView {
    pub(crate) fn from_status(address: SocketAddr, status: &ServerStatus) -> Self {
        let info = &status.info;
        let facts = FACTS
            .iter()
            .filter_map(|(key, label)| {
                let value = info.get(key)?.trim();
                if value.is_empty() {
                    return None;
                }
                let value = match *key {
                    "g_gametype" => super::gametype_name(value.parse().ok()).to_owned(),
                    "g_needpass" => if value == "0" { "no" } else { "yes" }.to_owned(),
                    _ => value.to_owned(),
                };
                Some(Fact { label, value })
            })
            .collect();
        let mut players: Vec<DetailPlayer> = status
            .players
            .iter()
            .map(|player| DetailPlayer {
                score: player.score,
                ping: player.ping,
                name: player.name.clone(),
            })
            .collect();
        players.sort_by(|left, right| right.score.cmp(&left.score));
        let bots = players.iter().filter(|player| player.ping == 0).count();
        Self {
            address,
            hostname: info
                .get("sv_hostname")
                .or_else(|| info.get("hostname"))
                .unwrap_or("Unnamed server")
                .to_owned(),
            map: info.get("mapname").unwrap_or("?").to_owned(),
            facts,
            players,
            bots,
        }
    }
}

/// What the details panel should show right now.
pub(crate) enum DetailsState<'a> {
    /// No row is selected.
    Nothing,
    /// The selected server has been asked and has not answered yet.
    Querying,
    /// The selected server did not answer.
    Failed(&'a str),
    Ready(&'a DetailsView),
}

/// Debounced status lookup for whichever server is selected.
pub(crate) struct ServerDetails {
    wanted: Option<(SocketAddr, Instant)>,
    pending: Option<(SocketAddr, Receiver<Result<ServerStatus, String>>)>,
    shown: Option<DetailsView>,
    failed: Option<(SocketAddr, String)>,
    offline: bool,
    /// The first of the shown server's players in view, wheeled through its list.
    player_scroll: usize,
}

impl ServerDetails {
    pub(crate) fn new() -> Self {
        Self {
            wanted: None,
            pending: None,
            shown: None,
            failed: None,
            offline: false,
            player_scroll: 0,
        }
    }

    /// Note the current selection and drive the query; call once per frame.
    pub(crate) fn tick(&mut self, selected: Option<SocketAddr>) {
        self.tick_at(selected, Instant::now());
    }

    fn tick_at(&mut self, selected: Option<SocketAddr>, now: Instant) {
        match (selected, self.wanted) {
            (Some(address), Some((wanted, _))) if wanted == address => {}
            (Some(address), _) => {
                self.wanted = Some((address, now));
                self.player_scroll = 0;
            }
            (None, _) => self.wanted = None,
        }
        self.poll();
        if self.offline {
            return;
        }
        let Some((address, since)) = self.wanted else {
            return;
        };
        let known = self
            .shown
            .as_ref()
            .is_some_and(|view| view.address == address)
            || self
                .failed
                .as_ref()
                .is_some_and(|(failed, _)| *failed == address);
        if known || self.pending.is_some() || now.duration_since(since) < DEBOUNCE {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result =
                query_server_status(address, STATUS_TIMEOUT).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.pending = Some((address, receiver));
    }

    fn poll(&mut self) {
        let Some((address, receiver)) = &self.pending else {
            return;
        };
        let address = *address;
        let outcome = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("status worker stopped".to_owned()),
        };
        self.pending = None;
        match outcome {
            Ok(status) => {
                self.failed = None;
                self.shown = Some(DetailsView::from_status(address, &status));
                self.player_scroll = 0;
            }
            Err(error) => self.failed = Some((address, error)),
        }
    }

    /// Forget a failed answer so the next `tick` asks again (Refresh).
    pub(crate) fn retry(&mut self) {
        self.failed = None;
        self.shown = None;
    }

    /// Take `view` as the answer for its server, asked for now.
    #[cfg(test)]
    pub(crate) fn answer_for_test(&mut self, view: DetailsView) {
        self.wanted = Some((view.address, Instant::now()));
        self.pending = None;
        self.failed = None;
        self.shown = Some(view);
        self.player_scroll = 0;
    }

    /// The first player in view on the details panel.
    pub(crate) fn player_scroll(&self) -> usize {
        self.player_scroll
    }

    /// Wheel the shown server's players `by` rows, `page` of them in view: kept so the
    /// last row stays at the bottom.
    pub(crate) fn scroll_players(&mut self, by: i32, page: usize) {
        let Some(view) = self.shown.as_ref() else {
            return;
        };
        let last = view.players.len().saturating_sub(page);
        self.player_scroll = self
            .player_scroll
            .saturating_add_signed(by as isize)
            .min(last);
    }

    pub(crate) fn state(&self) -> DetailsState<'_> {
        let Some((address, _)) = self.wanted else {
            return DetailsState::Nothing;
        };
        if let Some(view) = self.shown.as_ref().filter(|view| view.address == address) {
            return DetailsState::Ready(view);
        }
        if let Some((_, error)) = self
            .failed
            .as_ref()
            .filter(|(failed, _)| *failed == address)
        {
            return DetailsState::Failed(error);
        }
        DetailsState::Querying
    }
}
