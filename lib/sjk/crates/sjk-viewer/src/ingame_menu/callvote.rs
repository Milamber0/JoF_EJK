//! Fixed-capacity call-vote catalogue and page controller.

use super::Page;
use sjk_protocol::{GameState, InfoString};
use sjk_vfs::VirtualFileSystem;

const CS_SERVERINFO: usize = 0;
const CS_PLAYERS: usize = 1_131;
pub(super) const PAGE_ITEMS: usize = 16;
const MAP_LIMIT: usize = 128;
const PLAYER_LIMIT: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Open(Page),
    Send(String),
    Back,
    None,
}

/// Retained map/player catalogues used by call-vote sub-screens.
pub(crate) struct State {
    maps: [String; MAP_LIMIT],
    map_len: usize,
    players: [Player; PLAYER_LIMIT],
    player_len: usize,
    map_offset: usize,
    kick_offset: usize,
}

#[derive(Clone)]
struct Player {
    client_num: u8,
    name: String,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            maps: std::array::from_fn(|_| String::with_capacity(64)),
            map_len: 0,
            players: std::array::from_fn(|index| Player {
                client_num: index as u8,
                name: String::with_capacity(64),
            }),
            player_len: 0,
            map_offset: 0,
            kick_offset: 0,
        }
    }

    /// Refresh catalogues when the call-vote screen is opened, never per frame.
    pub(crate) fn refresh(
        &mut self,
        game_state: Option<&GameState>,
        vfs: Option<&VirtualFileSystem>,
    ) {
        self.map_len = 0;
        self.player_len = 0;
        self.map_offset = 0;
        self.kick_offset = 0;
        if let Some(game_state) = game_state {
            self.read_server_maps(game_state);
            self.read_players(game_state);
        }
        if self.map_len == 0
            && let Some(vfs) = vfs
        {
            self.read_vfs_maps(vfs);
        }
    }

    pub(crate) fn row_count(&self, page: Page) -> usize {
        match page {
            Page::CallVote => 10,
            Page::VoteMap => self.page_len(self.map_len, self.map_offset),
            Page::VoteGameType => 10,
            Page::VoteKick | Page::VoteClientKick => {
                self.page_len(self.player_len, self.kick_offset)
            }
            Page::VoteWarmup => 3,
            Page::VoteTimeLimit => 7,
            Page::VoteFragLimit => 7,
            _ => 0,
        }
    }

    pub(crate) fn prepare_rows(&self, page: Page, rows: &mut [String]) -> usize {
        for row in rows.iter_mut() {
            row.clear();
        }
        match page {
            Page::CallVote => fill(
                rows,
                &[
                    "Restart map",
                    "Next map",
                    "Change map...",
                    "Change game type...",
                    "Kick player by name...",
                    "Kick client number...",
                    "Warmup...",
                    "Time limit...",
                    "Frag limit...",
                    "Back",
                ],
            ),
            Page::VoteMap => self.map_rows(rows),
            Page::VoteGameType => fill(
                rows,
                &[
                    "Free for all  /  0",
                    "Holocron free for all  /  1",
                    "Jedi master  /  2",
                    "Duel  /  3",
                    "Power duel  /  4",
                    "Team free for all  /  6",
                    "Siege  /  7",
                    "Capture the flag  /  8",
                    "Capture the Ysalamiri  /  9",
                    "Back",
                ],
            ),
            Page::VoteKick | Page::VoteClientKick => self.player_rows(page, rows),
            Page::VoteWarmup => fill(rows, &["Enable warmup", "Disable warmup", "Back"]),
            Page::VoteTimeLimit => fill(rows, &["0", "5", "10", "15", "20", "30", "Back"]),
            Page::VoteFragLimit => fill(rows, &["0", "10", "20", "30", "40", "50", "Back"]),
            _ => 0,
        }
    }

    pub(crate) fn activate(&mut self, page: Page, row: usize) -> Action {
        match page {
            Page::CallVote => self.activate_root(row),
            Page::VoteMap => self.activate_map(row),
            Page::VoteGameType => self.activate_gametype(row),
            Page::VoteKick => self.activate_player(row, false),
            Page::VoteClientKick => self.activate_player(row, true),
            Page::VoteWarmup => self.activate_bool(row),
            Page::VoteTimeLimit => self.activate_number(row, true),
            Page::VoteFragLimit => self.activate_number(row, false),
            _ => Action::None,
        }
    }

    pub(crate) fn scroll(&mut self, page: Page, lines: isize) {
        let (offset, total) = match page {
            Page::VoteMap => (&mut self.map_offset, self.map_len),
            Page::VoteKick | Page::VoteClientKick => (&mut self.kick_offset, self.player_len),
            _ => return,
        };
        let last = total.saturating_sub(PAGE_ITEMS);
        *offset = (*offset as isize + lines).clamp(0, last as isize) as usize;
    }

    pub(crate) fn scroll_to(&mut self, page: Page, ratio: f32) {
        let (offset, total) = match page {
            Page::VoteMap => (&mut self.map_offset, self.map_len),
            Page::VoteKick | Page::VoteClientKick => (&mut self.kick_offset, self.player_len),
            _ => return,
        };
        let last = total.saturating_sub(PAGE_ITEMS);
        *offset = (ratio.clamp(0.0, 1.0) * last as f32).round() as usize;
    }

    pub(crate) fn scroll_metrics(&self, page: Page) -> Option<(usize, usize)> {
        match page {
            Page::VoteMap if self.map_len > PAGE_ITEMS => Some((self.map_offset, self.map_len)),
            Page::VoteKick | Page::VoteClientKick if self.player_len > PAGE_ITEMS => {
                Some((self.kick_offset, self.player_len))
            }
            _ => None,
        }
    }

    fn read_server_maps(&mut self, game_state: &GameState) {
        let Some(text) = game_state
            .config_string(CS_SERVERINFO)
            .and_then(|value| std::str::from_utf8(value).ok())
        else {
            return;
        };
        let Ok(info) = InfoString::parse(text) else {
            return;
        };
        let Some(list) = info.get("sv_maplist").or_else(|| info.get("maplist")) else {
            return;
        };
        for map in list.split_ascii_whitespace() {
            self.push_map(map);
        }
    }

    fn read_vfs_maps(&mut self, vfs: &VirtualFileSystem) {
        for path in vfs.paths() {
            let Some(map) = path
                .as_str()
                .strip_prefix("maps/")
                .and_then(|value| value.strip_suffix(".bsp"))
            else {
                continue;
            };
            self.push_map(map);
        }
    }

    fn push_map(&mut self, map: &str) {
        if self.map_len >= MAP_LIMIT
            || self.maps[..self.map_len]
                .iter()
                .any(|known| known.eq_ignore_ascii_case(map))
        {
            return;
        }
        // Slots keep their storage between refreshes: replace the old name, never extend it.
        self.maps[self.map_len].clear();
        self.maps[self.map_len].push_str(map);
        self.map_len += 1;
    }

    fn read_players(&mut self, game_state: &GameState) {
        for client in 0..PLAYER_LIMIT {
            let Some(text) = game_state
                .config_string(CS_PLAYERS + client)
                .and_then(|value| std::str::from_utf8(value).ok())
            else {
                continue;
            };
            let Ok(info) = InfoString::parse(text) else {
                continue;
            };
            let Some(name) = info.get("n").or_else(|| info.get("name")) else {
                continue;
            };
            self.players[self.player_len].client_num = client as u8;
            self.players[self.player_len].name.clear();
            self.players[self.player_len].name.push_str(name);
            self.player_len += 1;
        }
    }

    fn page_len(&self, total: usize, offset: usize) -> usize {
        total.saturating_sub(offset).min(PAGE_ITEMS) + 1 + usize::from(total > offset + PAGE_ITEMS)
    }

    fn map_rows(&self, rows: &mut [String]) -> usize {
        let end = (self.map_offset + PAGE_ITEMS).min(self.map_len);
        let mut count = 0;
        for map in &self.maps[self.map_offset..end] {
            rows[count].push_str(map);
            count += 1;
        }
        if end < self.map_len {
            rows[count].push_str("More maps...");
            count += 1;
        }
        rows[count].push_str("Back");
        count + 1
    }

    fn player_rows(&self, page: Page, rows: &mut [String]) -> usize {
        let end = (self.kick_offset + PAGE_ITEMS).min(self.player_len);
        let mut count = 0;
        for player in &self.players[self.kick_offset..end] {
            if page == Page::VoteClientKick {
                use std::fmt::Write as _;
                let _ = write!(
                    rows[count],
                    "{}  /  client {}",
                    player.name, player.client_num
                );
            } else {
                rows[count].push_str(&player.name);
            }
            count += 1;
        }
        if end < self.player_len {
            rows[count].push_str("More players...");
            count += 1;
        }
        rows[count].push_str("Back");
        count + 1
    }

    fn activate_root(&self, row: usize) -> Action {
        match row {
            0 => send(sjk_client::LegacyCallVote::MapRestart),
            1 => send(sjk_client::LegacyCallVote::NextMap),
            2 => Action::Open(Page::VoteMap),
            3 => Action::Open(Page::VoteGameType),
            4 => Action::Open(Page::VoteKick),
            5 => Action::Open(Page::VoteClientKick),
            6 => Action::Open(Page::VoteWarmup),
            7 => Action::Open(Page::VoteTimeLimit),
            8 => Action::Open(Page::VoteFragLimit),
            9 => Action::Back,
            _ => Action::None,
        }
    }

    fn activate_map(&mut self, row: usize) -> Action {
        let remaining = self.map_len.saturating_sub(self.map_offset).min(PAGE_ITEMS);
        if row < remaining {
            return send(sjk_client::LegacyCallVote::Map(
                &self.maps[self.map_offset + row],
            ));
        }
        if self.map_offset + remaining < self.map_len && row == remaining {
            self.map_offset += remaining;
            return Action::Open(Page::VoteMap);
        }
        Action::Back
    }

    fn activate_gametype(&self, row: usize) -> Action {
        [0, 1, 2, 3, 4, 6, 7, 8, 9]
            .get(row)
            .copied()
            .map_or(Action::Back, |value| {
                send(sjk_client::LegacyCallVote::GameType(value))
            })
    }

    fn activate_player(&mut self, row: usize, numeric: bool) -> Action {
        let remaining = self
            .player_len
            .saturating_sub(self.kick_offset)
            .min(PAGE_ITEMS);
        if row < remaining {
            let player = &self.players[self.kick_offset + row];
            return if numeric {
                send(sjk_client::LegacyCallVote::ClientKick(player.client_num))
            } else {
                send(sjk_client::LegacyCallVote::Kick(&player.name))
            };
        }
        if self.kick_offset + remaining < self.player_len && row == remaining {
            self.kick_offset += remaining;
            return Action::Open(if numeric {
                Page::VoteClientKick
            } else {
                Page::VoteKick
            });
        }
        Action::Back
    }

    fn activate_bool(&self, row: usize) -> Action {
        match row {
            0 => send(sjk_client::LegacyCallVote::Warmup(true)),
            1 => send(sjk_client::LegacyCallVote::Warmup(false)),
            _ => Action::Back,
        }
    }

    fn activate_number(&self, row: usize, time: bool) -> Action {
        let values = if time {
            [0, 5, 10, 15, 20, 30]
        } else {
            [0, 10, 20, 30, 40, 50]
        };
        values.get(row).copied().map_or(Action::Back, |value| {
            if time {
                send(sjk_client::LegacyCallVote::TimeLimit(value))
            } else {
                send(sjk_client::LegacyCallVote::FragLimit(value))
            }
        })
    }
}

/// The row of the call-vote page that opens `page`, one of its lists (the
/// SJK UI's Escape returns to it).
pub(super) fn opening_row(page: Page) -> Option<usize> {
    match page {
        Page::VoteMap => Some(2),
        Page::VoteGameType => Some(3),
        Page::VoteKick => Some(4),
        Page::VoteClientKick => Some(5),
        Page::VoteWarmup => Some(6),
        Page::VoteTimeLimit => Some(7),
        Page::VoteFragLimit => Some(8),
        _ => None,
    }
}

fn fill(rows: &mut [String], values: &[&str]) -> usize {
    for (row, value) in rows.iter_mut().zip(values) {
        row.push_str(value);
    }
    values.len()
}

fn send(vote: sjk_client::LegacyCallVote<'_>) -> Action {
    let mut command = String::with_capacity(96);
    match sjk_client::write_legacy_callvote(&mut command, vote) {
        Ok(()) => Action::Send(command),
        Err(_) => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_list_opens_from_its_row() {
        let state = State::new();
        for row in 0..state.row_count(Page::CallVote) {
            if let Action::Open(page) = state.activate_root(row) {
                assert_eq!(opening_row(page), Some(row), "{page:?}");
            }
        }
        assert_eq!(opening_row(Page::CallVote), None);
    }
}
