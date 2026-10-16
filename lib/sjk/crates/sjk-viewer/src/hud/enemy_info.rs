//! Independent opponent/master/leader readout, TaystJK cg_draw.c:4301-4470.
use super::*;
use sjk_protocol::Snapshot;
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_ui::{DrawCommand, FontWeight, Rect, TextAlign, TextOverflow};
use std::fmt::Write;

const TOP: f32 = 32.0;

/// Register only the option consumed by this panel.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        "cg_drawEnemyInfo",
        true,
        CvarFlags::ARCHIVE,
        "Show duel opponent, Jedi Master or leader",
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Server-selected panel source and the values which invalidate its retained text.
pub(crate) struct Choice {
    /// Client slot, or no slot while the Jedi Master saber is unclaimed.
    pub(crate) client: Option<u16>,
    kind: u8,
    score: i32,
    duel: Option<[i32; 4]>,
    health: Option<i32>,
}

fn number(game: &GameState, index: usize) -> i32 {
    game.config_string(index)
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1)
}

/// Choose the source specified by stock; never infer a leader from crosshair targeting.
pub(crate) fn choose(game: &GameState, snapshot: &Snapshot) -> Option<Choice> {
    let info = sjk_client::LegacyClientInfo::new(game.config_string(0).unwrap_or_default());
    let mode = info.integer("g_gametype").unwrap_or(0);
    let player = &snapshot.player;
    let japro = info.text("gamename").is_some_and(|s| {
        s.as_bytes()
            .windows(5)
            .any(|w| w.eq_ignore_ascii_case(b"japro"))
    });
    if player.health() <= 0 || mode == 4 || (japro && player.stats[11] != 0) {
        return None;
    }
    let (client, kind) = if mode == 2 {
        (number(game, 28), 1)
    } else if player.duel_in_progress() {
        (i32::from(player.duel_index()), 2)
    } else if mode == 3 && player.team() != 3 {
        let mut duelists = game
            .config_string(30)
            .and_then(|b| std::str::from_utf8(b).ok())?
            .split('|')
            .filter_map(|s| s.parse::<i32>().ok());
        let first = duelists.next()?;
        let second = duelists.next()?;
        let third = duelists.next().unwrap_or(-1);
        let local = i32::from(player.client_num());
        if local == first {
            (second, 2)
        } else if local == second || local == third {
            (first, 2)
        } else {
            return None;
        }
    } else {
        (number(game, 29), 3)
    };
    if kind == 1 && client < 0 {
        return Some(Choice {
            client: None,
            kind,
            score: 0,
            duel: None,
            health: None,
        });
    }
    if !(0..32).contains(&client) || game.config_string(1131 + client as usize).is_none() {
        return None;
    }
    let ci = sjk_client::LegacyClientInfo::new(game.config_string(1131 + client as usize)?);
    let duel = (mode == 3 && player.team() != 3).then(|| {
        [
            info.integer("fraglimit").unwrap_or(0),
            ci.integer("w").unwrap_or(0),
            ci.integer("l").unwrap_or(0),
            i32::MIN,
        ]
    });
    let health = if info.integer("g_showDuelHealths").unwrap_or(0) >= 2 {
        let clients = game
            .config_string(30)
            .and_then(|b| std::str::from_utf8(b).ok());
        let healths = game
            .config_string(31)
            .and_then(|b| std::str::from_utf8(b).ok());
        clients.zip(healths).and_then(|(clients, healths)| {
            clients
                .split('|')
                .zip(healths.split('|'))
                .take(2)
                .find(|(number, _)| number.parse::<i32>().ok() == Some(client))
                .and_then(|(_, value)| value.parse::<i32>().ok())
                .filter(|v| *v >= 0)
        })
    } else {
        None
    };
    Some(Choice {
        client: Some(client as u16),
        kind,
        score: number(game, 6),
        duel,
        health,
    })
}

/// Bounded text rebuilt only when the selected metadata or roster name changes.
pub(crate) struct State {
    portrait: super::portrait::Portrait,
    choice: Option<Choice>,
    visible: bool,
    /// Bounded display name borrowed from the shared roster on change.
    pub(super) name: String,
    /// Bounded role, score and health label, formatted only on metadata changes.
    pub(super) detail: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            portrait: Default::default(),
            choice: None,
            visible: false,
            name: String::with_capacity(256),
            detail: String::with_capacity(128),
        }
    }
}

impl State {
    /// Sample visibility and borrow the same roster used by overhead labels.
    pub(crate) fn update(
        &mut self,
        game: &GameState,
        snapshot: &Snapshot,
        console: Option<&ViewerConsole>,
        roster: &crate::chat::ChatOverlay,
        hidden: bool,
        scores: &[sjk_client::ScoreEntry],
    ) {
        let enabled = |name, fallback| console.and_then(|c| c.bool_cvar(name)).unwrap_or(fallback);
        self.visible = !hidden
            && enabled("cg_drawenemyinfo", true)
            && enabled("cg_drawupperright", true)
            && !(enabled("cg_drawradar", false) && snapshot.player.vehicle_entity_num() != 0)
            && !(enabled("cg_spechud", false)
                && (snapshot.player.team() == 3 || snapshot.player.movement_flags() & 0x1000 != 0));
        let mut choice = choose(game, snapshot);
        if let Some(choice) = &mut choice
            && let Some(duel) = &mut choice.duel
            && let Some(score) = scores
                .iter()
                .find(|s| Some(u16::from(s.client_num)) == choice.client)
        {
            duel[3] = score.score;
        }
        let name = choice
            .and_then(|c| c.client)
            .map(|c| roster.player_label(c))
            .unwrap_or("");
        self.assign(choice, name);
    }

    /// Update cached text; repeated draws of unchanged snapshots do no formatting.
    pub(crate) fn assign(&mut self, choice: Option<Choice>, name: &str) {
        let mut end = name.len().min(252);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        let name = &name[..end];
        if self.choice == choice && self.name == name {
            return;
        }
        self.choice = choice;
        self.name.clear();
        self.name.push_str(name);
        self.detail.clear();
        if let Some(choice) = choice {
            match (choice.kind, choice.client) {
                (1, None) => self.detail.push_str("GET SABER"),
                (1, _) => self.detail.push_str("JEDI MASTER"),
                (2, _) => self.detail.push_str("DUELING"),
                _ => {
                    let _ = write!(self.detail, "LEADER: {}", choice.score);
                }
            }
            if let Some([limit, wins, losses, score]) = choice.duel {
                if limit == 1 {
                    let _ = write!(self.detail, "  W/L {wins}/{losses}");
                } else if score != i32::MIN {
                    let _ = write!(self.detail, "  SCORE {score}");
                    if limit > 1 {
                        let _ = write!(self.detail, "/{limit}");
                    }
                }
            }
            if let Some(health) = choice.health {
                let _ = write!(self.detail, "  HEALTH {health}");
            }
        }
    }

    /// Resolve actual client model art on changes, in the existing HUD texture atlas.
    pub(crate) fn update_portrait(
        &mut self,
        game: &GameState,
        vfs: &sjk_vfs::VirtualFileSystem,
        shaders: &sjk_shader::ShaderCatalog,
        renderer: &ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        if !self.visible {
            return;
        }
        self.portrait.update(
            game,
            self.choice.and_then(|c| c.client),
            vfs,
            shaders,
            renderer,
            queue,
        );
    }

    /// Bottom of the visible corner block in 1080p HUD units, or zero if hidden.
    pub(super) fn bottom(&self) -> f32 {
        if !self.visible || self.choice.is_none() {
            return 0.0;
        }
        TOP + if self.portrait.drawn() {
            super::portrait::Portrait::HEIGHT
        } else {
            0.0
        } + 56.0
    }

    /// Hero text and actual model portrait, without placeholders or a tinted panel.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], theme: Theme) {
        if !self.visible || self.choice.is_none() {
            return;
        }
        self.portrait.emit(list, viewport, TOP);
        let s = crate::ui_scale::height_scale(viewport[1]);
        // The portrait heads the block; the text keeps its place when no image resolved.
        let text_top = TOP
            + if self.portrait.drawn() {
                super::portrait::Portrait::HEIGHT
            } else {
                0.0
            };
        for (row, id) in [318, 319].into_iter().enumerate() {
            let _ = list.push(DrawCommand::Text {
                rect: Rect::new(
                    viewport[0] - 390.0 * s,
                    (text_top + row as f32 * 28.0) * s,
                    350.0 * s,
                    28.0 * s,
                ),
                text: TextId(id),
                size: 22.0 * s,
                color: theme.foreground,
                align: TextAlign::End,
                overflow: TextOverflow::Ellipsis,
                weight: FontWeight::Regular,
                letter_spacing: 0.0,
            });
        }
    }
}
