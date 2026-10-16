//! Made-up matches for the off-screen world shots of the scoreboard
//! (`world_shot::tests::duel6_sjk_scoreboard`, `duel6_scoreboard_medals`): rows and
//! match facts set directly, so the board draws over a map without a server, in the
//! look `cg_scoreboardStyle` picks.

use super::classic::{FlagIcons, LocalStatus};
use super::sjk::{self, Duelists, Limits, Measure, SjkHeader};
use super::{ScoreRow, Scoreboard};
use crate::player_identity::Tag;
use crate::text::TextStyle;

/// A match to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Match {
    /// Free for all: fourteen players (you third, three bots, two tied), two
    /// watching.
    Free,
    /// Free for all on a full server: thirty playing, two watching, and you
    /// just killed.
    Crowd,
    /// Capture the flag, six a side, each side's flag carried.
    Capture,
    /// A duel: you against one, five waiting.
    Duel,
    /// A power duel: you alone against a pair, three waiting.
    PowerDuel,
}

/// The match facts behind a shot's rows.
pub(crate) struct Shot {
    held: bool,
    gametype: i32,
    limits: Limits,
    elapsed: i32,
    team_scores: [i32; 2],
    local: LocalStatus,
    killer: Option<&'static str>,
    duel: Duelists,
}

/// You, in every shot.
const YOU: u8 = 5;
const YOUR_NAME: &str = "^1Sol^7Fox";

const NAMES: [&str; 31] = [
    "^1Kael^7Varn",
    "^1{JoF}^3Mira",
    "^5Tor^7ren",
    "jof.Ashla",
    "^2Vex",
    "^6Nyx",
    "^4Daro",
    "Kyle",
    "Tavion",
    "^1Rax",
    "Jaden",
    "^3Ilum^7Fox",
    "Bastila",
    "^5Quin",
    "^2Sable",
    "Rosh",
    "^6Lumen",
    "^4Orin",
    "^3Talon",
    "Desann",
    "^1Ember",
    "^5Hollow",
    "Juno",
    "^2Kestrel",
    "^6Wren",
    "^4Corvin",
    "Alora",
    "^3Brask",
    "^1Siv",
    "^5Tamsin",
    "^6Pell",
];

/// A row for `client` named `name` on `team` with `score`.
fn row(client: u8, name: &str, team: u8, score: i32) -> ScoreRow {
    let bot = matches!(name, "Kyle" | "Tavion" | "Rosh" | "Desann");
    ScoreRow {
        client_num: client,
        name: name.to_owned(),
        team,
        score,
        deaths: None,
        ping: if bot {
            0
        } else {
            [18, 34, 52, 71, 96, 124, 160, 230][usize::from(client) % 8]
        },
        time: 4 + i32::from(client * 3 % 17),
        has_score: true,
        bot,
        record: None,
        powerups: 0,
        defends: i32::from(client % 4),
        assists: i32::from(client % 3),
        captures: i32::from(client % 2),
        // A few players the hub knows, two of them vouched for, three with medals.
        identity: match client {
            0 => Some(Tag {
                verified: true,
                medals: medals(&["early_tester", "early_contributor", "bug_hunter"], 3),
            }),
            11 => Some(Tag {
                verified: true,
                medals: medals(&["early_contributor"], 1),
            }),
            3 => Some(Tag {
                verified: false,
                medals: medals(&["early_tester", "bug_hunter"], 2),
            }),
            _ => None,
        },
    }
}

/// Made-up medals as the hub's presence list gives them, a repeatable one `count` times.
fn medals(ids: &[&str], count: u32) -> crate::medals::Medals {
    let list: Vec<sjk_identity::Medal> = ids
        .iter()
        .map(|id| sjk_identity::Medal {
            id: (*id).to_owned(),
            count,
            awarded: 0,
            note: String::new(),
        })
        .collect();
    crate::medals::Medals::from_wire(&list)
}

/// The client slot of the `index`th made-up player, keeping yours free.
fn slot(index: usize) -> u8 {
    let slot = index as u8;
    if slot >= YOU { slot + 1 } else { slot }
}

impl Scoreboard {
    /// Show `game` until the shot is done, fading in as when Tab is pressed.
    pub(crate) fn show_for_shot(&mut self, game: Match) {
        self.rows.clear();
        let local = |team, rank, score| LocalStatus {
            client: u16::from(YOU),
            team,
            rank,
            score,
            ready: 0,
            intermission: false,
        };
        let limits = Limits {
            frags: 30,
            captures: 8,
            duel_wins: 0,
            minutes: 20,
        };
        let shot = match game {
            Match::Free | Match::Crowd => {
                let players = if game == Match::Free { 13 } else { 29 };
                let scores = [27, 24, 19, 19, 16, 14, 12, 11, 9, 7, 5, 3, 1];
                for (index, name) in NAMES.iter().enumerate().take(players) {
                    // You are third, after the first two.
                    if index == 2 {
                        self.rows.push(row(YOU, YOUR_NAME, 0, 21));
                    }
                    let score = scores.get(index).copied().unwrap_or(1) - (index / 13) as i32;
                    self.rows.push(row(slot(index), name, 0, score));
                }
                for (index, name) in NAMES.iter().enumerate().skip(players).take(2) {
                    self.rows.push(row(slot(index), name, 3, 0));
                }
                Shot {
                    held: false,
                    gametype: 0,
                    limits,
                    elapsed: 7 * 60_000 + 48_000,
                    team_scores: [0, 0],
                    local: local(0, 2, 21),
                    killer: (game == Match::Crowd).then_some("^1Kael^7Varn"),
                    duel: Duelists::default(),
                }
            }
            Match::Capture => {
                for (index, name) in NAMES.iter().enumerate().take(11) {
                    let team = if index % 2 == 0 { 1 } else { 2 };
                    // In score order, as the server sends them.
                    if index == 5 {
                        let mut you = row(YOU, YOUR_NAME, 2, 14);
                        you.captures = 2;
                        you.assists = 1;
                        you.defends = 4;
                        self.rows.push(you);
                    }
                    let mut player = row(slot(index), name, team, 22 - index as i32 * 2);
                    // A red player carries the blue flag, a blue one the red.
                    player.powerups = match index {
                        2 => 1 << 5,
                        5 => 1 << 4,
                        _ => 0,
                    };
                    self.rows.push(player);
                }
                self.rows.push(row(slot(11), NAMES[11], 3, 0));
                Shot {
                    held: false,
                    gametype: 8,
                    limits,
                    elapsed: 12 * 60_000 + 5_000,
                    team_scores: [3, 5],
                    local: local(2, 0, 14),
                    killer: None,
                    duel: Duelists::default(),
                }
            }
            Match::Duel | Match::PowerDuel => {
                let power = game == Match::PowerDuel;
                let mut you = row(YOU, YOUR_NAME, 0, 4);
                you.record = Some((3, 1));
                self.rows.push(you);
                let opponents = if power { 2 } else { 1 };
                for (index, name) in NAMES.iter().enumerate().take(opponents) {
                    let mut opponent = row(slot(index), name, 0, 2 - index as i32);
                    opponent.record = Some((5 - index as i32, 2));
                    self.rows.push(opponent);
                }
                let waiting = if power { 3 } else { 5 };
                for (index, name) in NAMES.iter().enumerate().skip(opponents).take(waiting) {
                    let mut player = row(slot(index), name, 3, 0);
                    player.record = Some((index as i32 % 3, 1 + index as i32 % 2));
                    self.rows.push(player);
                }
                Shot {
                    held: false,
                    gametype: if power { 4 } else { 3 },
                    limits: Limits {
                        frags: 5,
                        captures: 0,
                        duel_wins: 10,
                        minutes: 0,
                    },
                    elapsed: 3 * 60_000 + 12_000,
                    team_scores: [0, 0],
                    local: local(0, 0, 4),
                    killer: None,
                    duel: Duelists {
                        clients: [Some(YOU), Some(slot(0)), power.then(|| slot(1))],
                        healths: [Some(87), Some(64)],
                        local: Some((87, 25)),
                    },
                }
            }
        };
        self.shot = Some(shot);
        self.motion = super::motion::Motion::default();
    }

    /// Whether a made-up match is on show.
    pub(crate) fn showing_shot(&self) -> bool {
        self.shot.is_some()
    }

    pub(super) fn shot_context(&self) -> Option<super::behavior::Context> {
        self.shot.as_ref().map(|shot| super::behavior::Context {
            held: shot.held,
            connected: self.rows.len(),
            dead: shot.killer.is_some(),
            intermission: shot.local.intermission,
            ..Default::default()
        })
    }

    /// Back to the server's scores.
    pub(crate) fn end_shot(&mut self) {
        self.shot = None;
        self.rows.clear();
    }
}

/// Draw the shot's match in the SJK look, in the UI's families.
pub(super) fn append(gpu: &mut crate::GpuState, viewport: [f32; 2]) {
    let flags = FlagIcons {
        red: gpu.hud.icons.powerup(4),
        blue: gpu.hud.icons.powerup(5),
        neutral: gpu.hud.icons.powerup(6),
    };
    let Scoreboard {
        ui,
        rows,
        motion,
        shot,
        style,
        icons,
        ..
    } = &mut gpu.scoreboard;
    let Some(shot) = shot.as_ref() else {
        return;
    };
    match style {
        super::style::ScoreboardStyle::Classic => {
            let header = super::classic::ClassicHeader {
                hostname: "^5JoF^7 Jedi of Freedom",
                max_clients: 32,
                gametype: shot.gametype,
                fraglimit: shot.limits.frags,
                team_scores: shot.team_scores,
                local: shot.local,
                killer: shot.killer,
                context: super::behavior::Context {
                    held: shot.held,
                    connected: rows.len(),
                    dead: shot.killer.is_some(),
                    intermission: shot.local.intermission,
                    ..Default::default()
                },
                duel_result: None,
            };
            let options = super::style::ClassicOptions::from_console(gpu.console.as_ref());
            super::classic::build(ui, rows, &header, options, icons, flags, motion, viewport);
            ui.finish(u16::MAX);
            ui.append_text_routed(
                &mut gpu.game_fonts,
                |_, text| Some(super::retail_font(text)),
                &mut gpu.text_vertices,
                &gpu.ui_font,
                viewport,
            );
            return;
        }
        super::style::ScoreboardStyle::Sjk => {}
    }
    let header = SjkHeader {
        map: "mp/duel6",
        gametype: shot.gametype,
        limits: shot.limits,
        elapsed: Some(shot.elapsed),
        team_scores: shot.team_scores,
        local: shot.local,
        killer: shot.killer,
        duel: shot.duel,
        compact: super::style::compact(gpu.console.as_ref()),
        fps: super::style::fps(gpu.console.as_ref()).then_some(240),
    };
    let measure = match gpu.game_fonts.sjk_metrics() {
        Some((display, body)) => Measure {
            display: Some(display),
            body: Some(body),
        },
        None => Measure {
            display: Some(&gpu.ui_font),
            body: Some(&gpu.ui_font),
        },
    };
    sjk::build(ui, rows, &header, flags, measure, motion, viewport);
    ui.finish(u16::MAX);
    match gpu.game_fonts.sjk() {
        Some(families) => ui.append_text_families(families, viewport, TextStyle::NEUTRAL),
        None => ui.append_text_styled(
            &mut gpu.text_vertices,
            &gpu.ui_font,
            viewport,
            TextStyle::NEUTRAL,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "renders with the GPU and installed game data named by JKA_GAME_DATA"]
    fn classic_scoreboard_chat_and_crowded_states() {
        crate::world_shot::on_big_stack(|| {
            let (mut gpu, _profile) = crate::world_shot::open(
                "maps/mp/duel6.bsp",
                [1920, 1080],
                None,
                &[("cg_scoreboardStyle", "classic"), ("cg_chatbox", "60000")],
            )
            .expect("GPU startup for the scoreboard check");
            gpu.chat.for_shot = true;
            for text in [
                "^2The scoreboard should leave this chat visible.",
                "^7Holding scores on a full server hides the feed.",
                "^5Releasing scores restores chat beside the compact list.",
            ] {
                gpu.chat.receive(
                    sjk_client::ServerEventKind::Chat,
                    text.to_owned(),
                    None,
                    std::time::Instant::now(),
                );
            }
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/parity-reports");
            std::fs::create_dir_all(&directory).unwrap();
            for (game, held, name) in [
                (Match::Free, true, "ordinary"),
                (Match::Crowd, false, "automatic"),
                (Match::Crowd, true, "held"),
            ] {
                gpu.scoreboard.show_for_shot(game);
                gpu.scoreboard.shot.as_mut().unwrap().held = held;
                if game == Match::Crowd {
                    gpu.scoreboard.shot.as_mut().unwrap().local.intermission = true;
                }
                let frame = crate::world_shot::frame(&mut gpu, 16);
                frame
                    .save(directory.join(format!("classic-scoreboard-{name}.png")))
                    .unwrap();
                let layout = super::super::behavior::chat_layout(&gpu, true);
                assert_eq!(
                    layout,
                    super::super::behavior::ChatLayout::Classic {
                        hide_feed: game == Match::Crowd && held
                    }
                );
                assert!(!gpu.scoreboard.draw_list().commands().is_empty());
                assert_eq!(
                    gpu.chat.draw_list().commands().is_empty(),
                    game == Match::Crowd && held
                );
            }
        });
    }

    /// The shot's match facts, compact or not.
    fn header(shot: &Shot, compact: bool) -> SjkHeader<'static> {
        SjkHeader {
            map: "mp/duel6",
            gametype: shot.gametype,
            limits: shot.limits,
            elapsed: Some(shot.elapsed),
            team_scores: shot.team_scores,
            local: shot.local,
            killer: shot.killer,
            duel: shot.duel,
            compact,
            fps: Some(240),
        }
    }

    /// Every made-up match fits the board's canvas at 1080 lines and 4K, 5:4,
    /// 4:3, 21:9 and 32:9.
    #[test]
    fn every_shot_fits_the_canvas() {
        for game in [
            Match::Free,
            Match::Crowd,
            Match::Capture,
            Match::Duel,
            Match::PowerDuel,
        ] {
            let mut board = Scoreboard::new();
            board.show_for_shot(game);
            assert!(board.rows.len() <= 32, "{game:?}");
            let shot = board.shot.as_ref().expect("a shot");
            let headers = [header(shot, false), header(shot, true)];
            for header in &headers {
                for viewport in [
                    [1920.0, 1080.0],
                    [3840.0, 2160.0],
                    [1280.0, 1024.0],
                    [1440.0, 1080.0],
                    [2560.0, 1080.0],
                    [3440.0, 1440.0],
                    [3840.0, 1080.0],
                    [5120.0, 1440.0],
                ] {
                    sjk::build(
                        &mut board.ui,
                        &board.rows,
                        header,
                        FlagIcons::default(),
                        Measure::default(),
                        &mut board.motion,
                        viewport,
                    );
                    board.ui.finish(u16::MAX);
                    assert!(
                        !board.ui.overflowed(),
                        "{game:?} at {viewport:?}, compact {}",
                        header.compact
                    );
                }
            }
        }
    }

    /// The lists' left and right edges at 1080 lines (frame pixels): the
    /// span of the thin rules under their labels.
    fn edges(board: &mut Scoreboard, measure: Measure<'_>, compact: bool) -> [f32; 2] {
        let header = header(board.shot.as_ref().expect("a shot"), compact);
        sjk::build(
            &mut board.ui,
            &board.rows,
            &header,
            FlagIcons::default(),
            measure,
            &mut board.motion,
            [1920.0, 1080.0],
        );
        board.ui.finish(u16::MAX);
        let rule = crate::menu::sjk::color::alpha(crate::menu::sjk::color::HOLO, 0.22);
        board
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                sjk_ui::DrawCommand::SolidRect { rect, color } if *color == rule => Some(*rect),
                _ => None,
            })
            .fold([f32::MAX, f32::MIN], |[left, right], rect| {
                [left.min(rect.x), right.max(rect.x + rect.width)]
            })
    }

    /// Measured in the UI's own families at 1080 lines, the compact board
    /// hugs its names: a free-for-all of 8 or of 32 is centred and under
    /// half the full board's 1144 pixels, a team game is narrower than it
    /// (`--nocapture` prints them all).
    #[test]
    fn compact_widths_in_the_ui_families() {
        let load = |family| {
            crate::text::load_family(family, 1.0, None)
                .expect("a bundled family")
                .font
        };
        let display = load(&crate::text::DISPLAY);
        let body = load(&crate::text::BODY);
        let measure = Measure {
            display: Some(&display),
            body: Some(&body),
        };
        let mut board = Scoreboard::new();
        for (name, game, players, deaths, team) in [
            ("free for all of 8", Match::Free, 8, false, None),
            (
                "free for all of 8, deaths counted",
                Match::Free,
                8,
                true,
                None,
            ),
            ("free for all of 32", Match::Crowd, 32, false, None),
            (
                "free for all of 32, deaths counted",
                Match::Crowd,
                32,
                true,
                None,
            ),
            ("team FFA, 6 a side", Match::Capture, 12, false, Some(6)),
            (
                "capture the flag, 6 a side",
                Match::Capture,
                12,
                false,
                None,
            ),
        ] {
            board.show_for_shot(game);
            if game == Match::Capture {
                board.rows.retain(|row| row.team != 3);
            } else {
                // The ones watching play too, for 32.
                for row in &mut board.rows {
                    row.team = 0;
                }
            }
            board.rows.truncate(players);
            assert_eq!(board.rows.len(), players, "{name}");
            for row in &mut board.rows {
                row.deaths = deaths.then_some(i32::from(row.client_num % 7));
            }
            if let (Some(gametype), Some(shot)) = (team, board.shot.as_mut()) {
                shot.gametype = gametype;
            }
            let [full_left, full_right] = edges(&mut board, measure, false);
            let [left, right] = edges(&mut board, measure, true);
            println!(
                "{name}: full {} ({full_left} to {full_right}), compact {:.1} ({left:.1} to {right:.1})",
                full_right - full_left,
                right - left,
            );
            assert!((full_right - full_left - 1144.0).abs() < 0.5, "{name}");
            if game == Match::Capture {
                assert!(right - left < 1144.0, "{name}: {left} {right}");
                assert!(left >= 680.0 - 0.5, "{name}: {left}");
            } else {
                assert!(right - left < 560.0, "{name}: {left} {right}");
                assert!(((left + right) * 0.5 - 960.0).abs() < 0.5, "{name}");
            }
        }
    }
}
