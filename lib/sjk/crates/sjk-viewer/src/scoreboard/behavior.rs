//! Classic scoreboard decisions from JoF EJK `codemp/cgame` at
//! `bd5e202dfedd49500f1b7aadd655619bac152167`: `CG_DrawOldScoreboard`,
//! `CG_Draw2D` and `CG_ChatBox_DrawStrings`. No wire or gameplay state is owned here.

use super::style::ScoreboardStyle;

/// `CG_ScoresDown_f`'s two-second request gate; rewind permits a fresh request.
#[derive(Default)]
pub(super) struct Requests(Option<i32>);

impl Requests {
    pub(super) fn due(&mut self, time: i32) -> bool {
        if self
            .0
            .is_some_and(|last| time >= last && time.wrapping_sub(last) <= 2_000)
        {
            return false;
        }
        self.0 = Some(time);
        true
    }
}

/// An explicit request never sends commands or clears recorded scores in a demo.
pub(crate) fn request_scores(gpu: &mut crate::GpuState) {
    if gpu.demo_session.is_some() {
        return;
    }
    let Some(time) = gpu
        .live_session
        .as_ref()
        .or(gpu.resident.session.as_ref())
        .map(|s| s.latest_snapshot().server_time)
    else {
        return;
    };
    if !gpu.scoreboard.requests.due(time) {
        return;
    }
    let classic = ScoreboardStyle::from_console(gpu.console.as_ref()) == ScoreboardStyle::Classic;
    if let Some(session) = gpu.communication_session_mut() {
        if classic {
            session.clear_scores();
        }
        if let Err(error) = session.send_reliable_command(b"score") {
            eprintln!("failed to request scoreboard: {error}");
        }
    }
}

/// Snapshot-backed conditions, separate from the scoreboard's fading presence.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Context {
    pub(super) held: bool,
    pub(super) dead: bool,
    pub(super) intermission: bool,
    pub(super) warmup: bool,
    pub(super) connected: usize,
}

impl Context {
    pub(super) fn requested(self) -> bool {
        // SJK's chosen policy: no automatic scoreboard on death, in either look.
        self.held || self.intermission
    }

    pub(super) fn compact(self) -> bool {
        self.connected > 25 && (self.dead || self.intermission) && !self.held
    }
}

/// Where chat goes while a scoreboard is actually drawn, including its fade.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ChatLayout {
    #[default]
    Normal,
    SideColumn,
    Classic {
        hide_feed: bool,
    },
}

impl ChatLayout {
    pub(crate) fn active(self) -> bool {
        self != Self::Normal
    }

    /// JoF's x=8 includes UI_Set2DRatio's widescreen correction; y=475 is
    /// bottom anchored. Both therefore follow height / 480, not window width.
    pub(crate) fn classic_origin(viewport: [f32; 2]) -> [f32; 2] {
        [8.0 * viewport[1] / 480.0, 475.0 * viewport[1] / 480.0]
    }
}

pub(super) fn context(gpu: &crate::GpuState, intermission: bool) -> Context {
    #[cfg(test)]
    if let Some(context) = gpu.scoreboard.shot_context() {
        return context;
    }
    let held = gpu.gameplay_input.held(crate::input::GameButton::Scores);
    let snapshot_and_game = gpu
        .live_session
        .as_ref()
        .map(|s| (s.latest_snapshot(), s.game_state()))
        .or_else(|| {
            gpu.resident
                .session
                .as_ref()
                .map(|s| (s.latest_snapshot(), s.game_state()))
        })
        .or_else(|| {
            gpu.demo_session
                .as_ref()
                .map(|s| (s.latest_snapshot(), s.game_state()))
        });
    let Some((snapshot, game)) = snapshot_and_game else {
        return Context {
            held,
            intermission,
            ..Context::default()
        };
    };
    let player = &snapshot.player;
    let movement = gpu
        .local_prediction
        .predicted_state()
        .filter(|_| gpu.live_session.is_some())
        .map_or(player.movement_type(), |p| p.movement_type);
    Context {
        held,
        dead: movement == 3,
        intermission: intermission || movement == 6,
        warmup: game
            .config_string(5)
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0)
            != 0,
        connected: (0..32)
            .filter(|client| {
                game.config_string(super::CS_PLAYERS + client)
                    .is_some_and(|bytes| !bytes.is_empty())
            })
            .count(),
    }
}

pub(crate) fn chat_layout(gpu: &crate::GpuState, visible: bool) -> ChatLayout {
    if !visible {
        ChatLayout::Normal
    } else if ScoreboardStyle::from_console(gpu.console.as_ref()) == ScoreboardStyle::Sjk {
        ChatLayout::SideColumn
    } else {
        let context = context(gpu, false);
        ChatLayout::Classic {
            hide_feed: context.held && context.connected > 25,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_score_taps_are_throttled_and_rewinds_reset() {
        let mut requests = Requests::default();
        assert!(requests.due(10_000));
        assert!(!requests.due(10_001));
        assert!(!requests.due(12_000));
        assert!(requests.due(12_001));
        assert!(requests.due(0));
    }
    #[test]
    fn death_warmup_and_explicit_scores_are_distinct() {
        let mut c = Context {
            dead: true,
            ..Context::default()
        };
        assert!(!c.requested());
        c.warmup = true;
        assert!(!c.requested());
        c.held = true;
        assert!(c.requested());
        c.held = false;
        c.intermission = true;
        assert!(c.requested());
    }
    #[test]
    fn classic_chat_uses_jof_bottom_anchor_at_each_aspect_ratio() {
        assert_eq!(ChatLayout::classic_origin([640.0, 480.0]), [8.0, 475.0]);
        for width in [1440.0, 1920.0, 2560.0, 3840.0] {
            assert_eq!(ChatLayout::classic_origin([width, 1080.0]), [18.0, 1068.75]);
        }
    }
    #[test]
    fn automatic_crowded_board_is_compact_until_held() {
        for connected in [25, 26, 32] {
            let mut c = Context {
                dead: true,
                connected,
                ..Context::default()
            };
            assert_eq!(c.compact(), connected > 25);
            c.held = true;
            assert!(!c.compact());
            c.held = false;
            c.dead = false;
            assert!(!c.compact());
            c.intermission = true;
            assert_eq!(c.compact(), connected > 25);
        }
    }
}
