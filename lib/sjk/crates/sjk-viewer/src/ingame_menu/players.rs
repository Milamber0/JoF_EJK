//! The game menu's Players page, a small scoreboard of everyone on the server, and its
//! Report page, why a chosen player is reported (`docs/identity.md`, "Player reports").
//!
//! The roster is read from the session when the page opens and once a second while it
//! shows ([`State::refresh`]): each connected client's name, team and bot flag from its
//! config string, its score and ping from the last `scores` the server sent, and what
//! the SJK hub knows of it (its key, verified or not) from the identity service's
//! presence list. A player chosen for a report is copied ([`State::choose`]), so the
//! report names them as they were even if they leave or rename before it is sent.
//!
//! Only a verified SJK player may report ([`Gate`]); the others see the list, and the
//! Report page's reasons dimmed with why.

use super::Page;
use sjk_client::{LegacyClientInfo, ScoreEntry};
use sjk_identity::Category;
use sjk_protocol::GameState;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// Players a page lists; More players... pages on.
pub(crate) const PAGE_ITEMS: usize = 16;
/// Clients the legacy protocol numbers.
const MAX_CLIENTS: u8 = 32;
/// `CS_PLAYERS`: the first client's config string.
const CS_PLAYERS: usize = 1_131;
/// How often the roster is read again while the page shows, and the scores asked for.
const READ_EVERY: Duration = Duration::from_secs(1);
const SCORES_EVERY: Duration = Duration::from_secs(2);

/// Whether the local player may send player reports, and if not, why.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Gate {
    /// Their key is verified: they may report.
    Open,
    /// No SJK identity: `cl_identity` is off, or the key file is unusable.
    #[default]
    NoIdentity,
    /// The identity is on but the hub has not answered yet.
    Offline,
    /// The key is not verified by the SJK team.
    NotVerified,
    /// A game on this PC: there is no hub to tell.
    Local,
}

impl Gate {
    /// Why reporting is closed, in a line; `None` when it is open.
    pub(crate) const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Open => None,
            Self::NoIdentity => Some("Reporting needs the SJK identity on (cl_identity 1)"),
            Self::Offline => Some("Not connected to the SJK hub yet"),
            Self::NotVerified => Some("Only verified SJK players can report players"),
            Self::Local => Some("Players can be reported on online servers only"),
        }
    }
}

/// What the SJK hub knows of a player: a live claim on their slot under their name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HubMark {
    pub(crate) key_id: String,
    /// Their hub display name.
    pub(crate) name: String,
    pub(crate) verified: bool,
    /// The medals the SJK team gave them.
    pub(crate) medals: crate::medals::Medals,
    /// Their picture's version, empty for none.
    pub(crate) avatar: String,
}

/// One client on the server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Player {
    pub(crate) slot: u8,
    /// As the game shows it, colour codes kept.
    pub(crate) name: String,
    /// 0 free, 1 red, 2 blue, 3 spectator.
    pub(crate) team: u8,
    /// From the server's last scores; `None` before it listed the client.
    pub(crate) score: Option<i32>,
    pub(crate) ping: Option<i32>,
    pub(crate) bot: bool,
    /// The local player.
    pub(crate) you: bool,
    pub(crate) hub: Option<HubMark>,
}

impl Player {
    /// Why this player cannot be reported, if they cannot.
    pub(crate) const fn unreportable(&self) -> Option<&'static str> {
        if self.you {
            Some("This is you")
        } else if self.bot {
            Some("Bots cannot be reported")
        } else {
            None
        }
    }

    /// Their side in words: "red team", "spectating", or nothing in a free game.
    pub(crate) const fn side(&self, team_game: bool) -> &'static str {
        match (self.team, team_game) {
            (3, _) => "spectating",
            (1, true) => "red team",
            (2, true) => "blue team",
            _ => "",
        }
    }
}

/// What a row of the Players page does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// The player at this place in the roster was chosen: open the Report page.
    Report,
    /// The next page of players.
    More,
    Back,
    None,
}

/// What the Players and Report pages show, kept between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    players: Vec<Player>,
    team_game: bool,
    /// The first player the page lists.
    offset: usize,
    /// The player chosen for a report, as they were then, and their row.
    target: Option<(Player, usize)>,
    gate: Gate,
    /// The read-only lines over the classic pages.
    info: Vec<String>,
    read_at: Option<Instant>,
    scores_at: Option<Instant>,
    /// A report was handed to the identity service; its outcome is awaited.
    pub(crate) waiting: bool,
    /// The serial of the last outcome shown.
    pub(crate) serial: u64,
    /// A made-up roster the tests set stays as it is.
    #[cfg(test)]
    held: bool,
}

impl State {
    /// Whether the roster should be read again at `now` (once a second).
    pub(crate) fn due(&mut self, now: Instant) -> bool {
        if self
            .read_at
            .is_some_and(|read| now.duration_since(read) < READ_EVERY)
        {
            return false;
        }
        self.read_at = Some(now);
        true
    }

    /// Whether to ask the server for fresh scores at `now` (every two seconds while the
    /// page shows, as the stock scoreboard does).
    pub(crate) fn scores_due(&mut self, now: Instant) -> bool {
        if self
            .scores_at
            .is_some_and(|asked| now.duration_since(asked) < SCORES_EVERY)
        {
            return false;
        }
        self.scores_at = Some(now);
        true
    }

    /// The pages open afresh: read at once, from the first player.
    pub(crate) fn reset(&mut self) {
        self.read_at = None;
        self.scores_at = None;
        self.offset = 0;
    }

    /// Read the roster: every connected client of `game` in the order of the server's
    /// `scores` (its ranking), then those it does not list yet by slot; `you` is the
    /// local client, `hub` what the hub knows of a slot under a name.
    pub(crate) fn refresh(
        &mut self,
        game: &GameState,
        scores: &[ScoreEntry],
        you: Option<u8>,
        gate: Gate,
        hub: impl Fn(u8, &str) -> Option<HubMark>,
    ) {
        #[cfg(test)]
        if self.held {
            return;
        }
        self.gate = gate;
        self.team_game = team_game(game);
        self.players.clear();
        let listed = scores
            .iter()
            .map(|score| (score.client_num, Some(score)))
            .chain((0..MAX_CLIENTS).map(|slot| (slot, None)));
        for (slot, score) in listed {
            if slot >= MAX_CLIENTS || self.players.iter().any(|p| p.slot == slot) {
                continue;
            }
            let Some(bytes) = game
                .config_string(CS_PLAYERS + usize::from(slot))
                .filter(|bytes| !bytes.is_empty())
            else {
                continue;
            };
            let info = LegacyClientInfo::new(bytes);
            let name = crate::player_identity::shown_name(game, usize::from(slot))
                .unwrap_or_else(|| format!("Client {slot}"));
            self.players.push(Player {
                slot,
                hub: hub(slot, &name),
                name,
                team: info
                    .integer("t")
                    .and_then(|team| u8::try_from(team).ok())
                    .unwrap_or(3),
                score: score.map(|score| score.score),
                ping: score.map(|score| score.ping),
                bot: info.bytes("skill").is_some(),
                you: you == Some(slot),
            });
        }
        if self.offset >= self.players.len() {
            self.offset = 0;
        }
        self.write_info();
    }

    /// No match: nobody to list.
    pub(crate) fn clear(&mut self, gate: Gate) {
        #[cfg(test)]
        if self.held {
            return;
        }
        self.gate = gate;
        self.players.clear();
        self.offset = 0;
        self.write_info();
    }

    /// Whether the local player may report, and why not.
    pub(crate) fn gate(&self) -> Gate {
        self.gate
    }

    /// Whether the server plays in teams.
    pub(crate) fn team_game(&self) -> bool {
        self.team_game
    }

    /// The players the page lists now.
    pub(crate) fn shown(&self) -> &[Player] {
        let end = (self.offset + PAGE_ITEMS).min(self.players.len());
        &self.players[self.offset.min(end)..end]
    }

    /// Whether a More players... row follows the shown players.
    fn more(&self) -> bool {
        self.players.len() > self.offset + PAGE_ITEMS
    }

    /// Every player on the server.
    pub(crate) fn count(&self) -> usize {
        self.players.len()
    }

    /// The player chosen for a report.
    pub(crate) fn target(&self) -> Option<&Player> {
        self.target.as_ref().map(|(player, _)| player)
    }

    /// The Players page's row the chosen player was on.
    pub(crate) fn target_row(&self) -> usize {
        self.target.as_ref().map_or(0, |(_, row)| *row)
    }

    /// Why the chosen player cannot be reported now, if they cannot.
    pub(crate) fn blocked(&self) -> Option<&'static str> {
        self.gate.reason().or_else(|| {
            self.target()
                .map_or(Some("No player chosen"), Player::unreportable)
        })
    }

    /// Rows of `page`: the shown players, More players... when there are more, and
    /// Back; or the report's reasons and Back.
    pub(crate) fn row_count(&self, page: Page) -> usize {
        match page {
            Page::Players => self.shown().len() + usize::from(self.more()) + 1,
            Page::ReportPlayer => Category::ALL.len() + 1,
            _ => 0,
        }
    }

    /// What row `row` of the Players page does.
    pub(crate) fn action(&self, row: usize) -> Action {
        let shown = self.shown().len();
        match row {
            row if row < shown => Action::Report,
            row if row == shown && self.more() => Action::More,
            row if row + 1 == self.row_count(Page::Players) => Action::Back,
            _ => Action::None,
        }
    }

    /// The next page of players, back to the first after the last.
    pub(crate) fn next_page(&mut self) {
        self.offset = if self.more() {
            self.offset + PAGE_ITEMS
        } else {
            0
        };
    }

    /// Choose the player on row `row` of the Players page for a report; false when the
    /// row is not a player.
    pub(crate) fn choose(&mut self, row: usize) -> bool {
        let Some(player) = self.shown().get(row).cloned() else {
            return false;
        };
        self.target = Some((player, row));
        self.write_info();
        true
    }

    /// The category on row `row` of the Report page.
    pub(crate) fn category(row: usize) -> Option<Category> {
        Category::ALL.get(row).copied()
    }

    /// The first row the Report page may take: the first reason, or Back when the
    /// reasons are closed.
    pub(crate) fn first_report_row(&self) -> usize {
        if self.blocked().is_some() {
            Category::ALL.len()
        } else {
            0
        }
    }

    /// Fill `rows`, their `hints` and `enabled` for `page` (rows read "label  /  detail"
    /// where a row has a detail; the SJK UI moves the detail to the hint).
    pub(crate) fn prepare(
        &self,
        page: Page,
        rows: &mut [String],
        hints: &mut [String],
        enabled: &mut [bool],
    ) -> usize {
        match page {
            Page::Players => {
                let shown = self.shown();
                for (row, player) in shown.iter().enumerate() {
                    rows[row].push_str(&player.name);
                    // White after the name: its last colour code would carry on.
                    rows[row].push_str("^7  /  ");
                    self.detail(player, &mut rows[row]);
                }
                let mut count = shown.len();
                if self.more() {
                    rows[count].push_str("More players...");
                    let _ = write!(
                        hints[count],
                        "{} more on the server",
                        self.players.len() - self.offset - shown.len()
                    );
                    count += 1;
                }
                rows[count].push_str("Back");
                count + 1
            }
            Page::ReportPlayer => {
                let blocked = self.blocked();
                for (row, category) in Category::ALL.iter().enumerate() {
                    rows[row].push_str(category.label());
                    hints[row].push_str(category.hint());
                    enabled[row] = blocked.is_none();
                }
                let back = Category::ALL.len();
                rows[back].push_str("Back");
                hints[back].push_str(blocked.unwrap_or("Back to the players"));
                back + 1
            }
            _ => 0,
        }
    }

    /// A player's row detail: side, score, ping and what the hub knows.
    fn detail(&self, player: &Player, out: &mut String) {
        let mut parts: Vec<std::borrow::Cow<'static, str>> = Vec::with_capacity(5);
        if player.you {
            parts.push("you".into());
        }
        if player.bot {
            parts.push("bot".into());
        }
        let side = player.side(self.team_game);
        if !side.is_empty() {
            parts.push(side.into());
        }
        if let Some(score) = player.score.filter(|_| player.team != 3) {
            parts.push(format!("{score} points").into());
        }
        if let Some(ping) = player.ping.filter(|_| !player.bot) {
            parts.push(format!("{ping} ms").into());
        }
        match &player.hub {
            Some(hub) if hub.verified => parts.push("verified SJK player".into()),
            Some(_) => parts.push("SJK player".into()),
            None => {}
        }
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(part);
        }
    }

    /// The read-only lines over the classic pages ("label  /  value", as the
    /// classic about pop-up lays them out).
    pub(crate) fn info(&self, page: Page) -> &[String] {
        match page {
            Page::Players => &self.info[..self.info.len().min(1)],
            Page::ReportPlayer => self.info.get(1..).unwrap_or(&[]),
            _ => &[],
        }
    }

    /// Write [`State::info`]'s lines: the gate's first, then the chosen player's.
    fn write_info(&mut self) {
        self.info.clear();
        self.info.push(match self.gate.reason() {
            Some(reason) => format!("Reports  /  {reason}"),
            None => "Reports  /  Choose a player to report".to_owned(),
        });
        let Some((player, _)) = &self.target else {
            return;
        };
        self.info.push(format!("Player  /  {}", player.name));
        let mut place = format!("slot {}", player.slot);
        let side = player.side(self.team_game);
        if !side.is_empty() {
            let _ = write!(place, ", {side}");
        }
        self.info.push(format!("Place  /  {place}"));
        let key = player.hub.as_ref().map(|hub| {
            crate::player_identity::printable_key_id(&hub.key_id)
                .map_or_else(|| "your key".to_owned(), |key| format!("key {key}"))
        });
        self.info.push(match (&player.hub, key) {
            (Some(hub), Some(key)) if hub.verified => format!("SJK hub  /  verified, {key}"),
            (Some(_), Some(key)) => format!("SJK hub  /  {key}"),
            _ => "SJK hub  /  not known to the hub".to_owned(),
        });
        if let Some(reason) = self.blocked() {
            self.info.push(format!("Report  /  {reason}"));
        }
    }
}

/// Whether `game`'s server plays in teams (`g_gametype` 6 and up).
fn team_game(game: &GameState) -> bool {
    game.config_string(0)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|text| sjk_protocol::InfoString::parse(text).ok())
        .and_then(|info| info.get_i32("g_gametype"))
        .is_some_and(|gametype| gametype >= 6)
}

/// Made-up medals for the world shots, a repeatable one given twice.
#[cfg(test)]
fn shot_medals(ids: &[&str]) -> crate::medals::Medals {
    let list: Vec<sjk_identity::Medal> = ids
        .iter()
        .map(|id| sjk_identity::Medal {
            id: (*id).to_owned(),
            count: 2,
            awarded: 0,
            note: String::new(),
        })
        .collect();
    crate::medals::Medals::from_wire(&list)
}

#[cfg(test)]
impl State {
    /// A made-up roster of `count` players for the world shots and tests, the local
    /// player in slot 2, under `gate`; it stays as set.
    pub(crate) fn for_shot(count: u8, team_game: bool, gate: Gate) -> Self {
        const NAMES: [&str; 8] = [
            "^1Darth ^7Vulpes",
            "Padawan",
            "^2Sol",
            "^5Kyle",
            "^3Jan Ors",
            "Rosh^1Penin",
            "^6Tavion",
            "Luke",
        ];
        let players = (0..count)
            .map(|slot| Player {
                slot,
                name: NAMES[usize::from(slot) % NAMES.len()].to_owned(),
                team: if team_game { 1 + slot % 2 } else { 0 },
                score: Some(30 - i32::from(slot) * 3),
                ping: Some(20 + i32::from(slot) * 7),
                bot: slot == 6,
                you: slot == 2,
                hub: match slot {
                    0 => Some(HubMark {
                        key_id: "0123456789abcdef".to_owned(),
                        name: "Vulpes".to_owned(),
                        verified: false,
                        medals: shot_medals(&["bug_hunter"]),
                        avatar: String::new(),
                    }),
                    2 | 3 => Some(HubMark {
                        key_id: "fedcba9876543210".to_owned(),
                        name: "Sol".to_owned(),
                        verified: true,
                        medals: shot_medals(&["early_tester", "early_contributor", "bug_hunter"]),
                        avatar: String::new(),
                    }),
                    _ => None,
                },
            })
            .collect();
        let mut state = Self {
            players,
            team_game,
            gate,
            held: true,
            ..Self::default()
        };
        state.write_info();
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(state: &State, page: Page) -> (Vec<String>, Vec<String>, Vec<bool>, usize) {
        let mut labels = vec![String::new(); 24];
        let mut hints = vec![String::new(); 24];
        let mut enabled = vec![true; 24];
        let count = state.prepare(page, &mut labels, &mut hints, &mut enabled);
        (labels, hints, enabled, count)
    }

    #[test]
    fn the_roster_follows_the_scores_then_the_slots() {
        let mut game = GameState::empty_local(2);
        let mut set = |index: usize, value: &str| {
            game.replace_config_string(index, value.as_bytes().to_vec())
                .expect("a config string");
        };
        set(0, "\\g_gametype\\8");
        set(CS_PLAYERS, "\\n\\^1Troll\\t\\1");
        set(CS_PLAYERS + 2, "\\n\\^2Sol\\t\\2");
        set(CS_PLAYERS + 4, "\\n\\Bot\\t\\2\\skill\\3");
        set(CS_PLAYERS + 9, "\\n\\Late\\t\\3");
        let score = |client_num, score, ping| ScoreEntry {
            client_num,
            score,
            ping,
            time_minutes: 0,
            flags: 0,
            powerups: 0,
            defends: 0,
            assists: 0,
            captures: 0,
        };
        let scores = [score(2, 30, 40), score(0, 12, 80), score(4, 3, 0)];
        let mut state = State::default();
        state.refresh(&game, &scores, Some(2), Gate::Open, |slot, _| {
            (slot == 0).then(|| HubMark {
                key_id: "0123456789abcdef".to_owned(),
                name: "Troll".to_owned(),
                verified: false,
                medals: crate::medals::Medals::default(),
                avatar: String::new(),
            })
        });
        let slots: Vec<u8> = state.shown().iter().map(|p| p.slot).collect();
        assert_eq!(slots, [2, 0, 4, 9]);
        assert!(state.team_game());
        let (labels, _, _, count) = rows(&state, Page::Players);
        assert_eq!(count, 5);
        assert_eq!(labels[0], "^2Sol^7  /  you, blue team, 30 points, 40 ms");
        assert_eq!(
            labels[1],
            "^1Troll^7  /  red team, 12 points, 80 ms, SJK player"
        );
        assert_eq!(labels[2], "Bot^7  /  bot, blue team, 3 points");
        assert_eq!(labels[3], "Late^7  /  spectating");
        assert_eq!(labels[4], "Back");
        assert_eq!(state.action(4), Action::Back);
        // Yourself and bots cannot be reported; another player can.
        assert!(state.choose(0));
        assert_eq!(state.blocked(), Some("This is you"));
        assert!(state.choose(2));
        assert_eq!(state.blocked(), Some("Bots cannot be reported"));
        assert!(state.choose(1));
        assert_eq!(state.blocked(), None);
        assert_eq!(state.target_row(), 1);
        assert_eq!(
            state.info(Page::ReportPlayer),
            [
                "Player  /  ^1Troll",
                "Place  /  slot 0, red team",
                "SJK hub  /  key 0123456789abcdef"
            ]
        );
        assert!(!state.choose(4), "Back is not a player");
    }

    #[test]
    fn an_unverified_player_sees_the_reasons_closed_and_why() {
        let mut state = State::for_shot(5, false, Gate::NotVerified);
        assert!(state.choose(0));
        let (labels, hints, enabled, count) = rows(&state, Page::ReportPlayer);
        assert_eq!(count, Category::ALL.len() + 1);
        assert_eq!(labels[0], "Cheating");
        assert!(enabled[..Category::ALL.len()].iter().all(|on| !on));
        assert_eq!(
            hints[Category::ALL.len()],
            "Only verified SJK players can report players"
        );
        assert_eq!(state.first_report_row(), Category::ALL.len());
        assert_eq!(
            state.info(Page::Players),
            ["Reports  /  Only verified SJK players can report players"]
        );
        assert!(
            state
                .info(Page::ReportPlayer)
                .last()
                .is_some_and(|line| line.contains("Only verified"))
        );
        let open = State::for_shot(5, false, Gate::Open);
        let (_, _, enabled, _) = rows(&open, Page::ReportPlayer);
        assert!(!enabled[0], "no player chosen yet: the reasons stay closed");
        assert_eq!(State::category(0), Some(Category::Cheating));
        assert_eq!(State::category(Category::ALL.len()), None);
    }

    #[test]
    fn a_full_server_pages_through_sixteen_at_a_time() {
        let mut state = State::for_shot(32, true, Gate::Open);
        assert_eq!(state.count(), 32);
        assert_eq!(state.row_count(Page::Players), PAGE_ITEMS + 2);
        let (labels, hints, _, _) = rows(&state, Page::Players);
        assert_eq!(labels[PAGE_ITEMS], "More players...");
        assert_eq!(hints[PAGE_ITEMS], "16 more on the server");
        assert_eq!(state.action(PAGE_ITEMS), Action::More);
        assert_eq!(state.action(PAGE_ITEMS + 1), Action::Back);
        state.next_page();
        assert_eq!(state.shown()[0].slot, 16);
        assert_eq!(state.row_count(Page::Players), PAGE_ITEMS + 1);
        assert!(state.choose(0));
        assert_eq!(state.target().map(|p| p.slot), Some(16));
        state.next_page();
        assert_eq!(state.shown()[0].slot, 0, "after the last page, the first");
    }

    #[test]
    fn the_gates_say_why() {
        for gate in [
            Gate::NoIdentity,
            Gate::Offline,
            Gate::NotVerified,
            Gate::Local,
        ] {
            assert!(gate.reason().is_some_and(|reason| reason.len() < 60));
        }
        assert_eq!(Gate::Open.reason(), None);
    }
}
