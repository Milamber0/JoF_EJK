//! World pings (`docs/client.md#world-pings`): a player marks the point under the
//! crosshair (`+ping`, `worldping`), and every SJK player who may see it draws a gold
//! marker there for a few seconds, through walls, with the pinger's name and the
//! distance, and hears a short sound.
//!
//! Pings travel through the SJK hub as emotes do (`PROTOCOL.md`, "Pings"): the hub
//! takes the sender's slot and name from their claim on the server and relays the ping
//! to the SJK players reading that server. The hub knows no teams, so a ping carries
//! the sender's [`Scope`] and each reader checks it against what its own game shows:
//! the sender's slot must show that team, and the reader must be one who sees it
//! ([`Scope::sees`]). Game servers and players on other clients never see anything.
//!
//! This module holds the rules, the pings on show and their drawing; the frame glue is
//! `world_ping_frame.rs`.

use crate::hud::identification::Camera;
use glam::Vec3;
use sjk_ui::{Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextId, TextOverflow};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// `cg_worldPings`: draw other players' pings (0 hides them; the player's own still show
/// and are still sent).
pub(crate) const SHOW_CVAR: &str = "cg_worldPings";
/// `cg_worldPingSound`: a short sound when a ping appears.
pub(crate) const SOUND_CVAR: &str = "cg_worldPingSound";
/// The held bind: a ping on the press, nothing on the release.
pub(crate) const PRESS_COMMAND: &str = "+ping";
/// The release of [`PRESS_COMMAND`], recognised so it never reaches the server.
pub(crate) const RELEASE_COMMAND: &str = "-ping";
/// The plain command, for the console and quick wheel choices (`ping` is the engine's
/// server query).
pub(crate) const COMMAND: &str = "worldping";

/// How long a ping shows.
pub(crate) const LIFE: Duration = Duration::from_millis(7000);
/// The end of [`LIFE`] over which it fades out.
const FADE: Duration = Duration::from_millis(1500);
/// The ring that grows out of a new ping.
const PULSE: Duration = Duration::from_millis(800);
/// The marker's pop as it appears.
const POP: Duration = Duration::from_millis(180);
/// Most pings on show; a new one past it replaces the oldest.
pub(crate) const MAX_MARKS: usize = 24;
/// Most pings of one player on show; a new one replaces their oldest.
pub(crate) const PER_SENDER: usize = 2;
/// The player's own pings: at most one a second and [`SEND_PER_WINDOW`] in
/// [`SEND_WINDOW`], under the hub's 5 in 10 seconds so a ping is never refused there.
const SEND_GAP: Duration = Duration::from_millis(1000);
const SEND_WINDOW: Duration = Duration::from_secs(10);
const SEND_PER_WINDOW: usize = 4;
/// Pings received from one slot: at most one every half second and
/// [`RECEIVE_PER_WINDOW`] in [`RECEIVE_WINDOW`]; more are dropped, whatever the hub let
/// through.
const RECEIVE_GAP: Duration = Duration::from_millis(500);
const RECEIVE_WINDOW: Duration = Duration::from_secs(10);
const RECEIVE_PER_WINDOW: usize = 5;
/// Shortest time between two ping sounds, so a burst plays once.
const SOUND_GAP: Duration = Duration::from_millis(300);
/// How far the crosshair's ray reaches for a point to ping.
pub(crate) const REACH: f32 = 16_384.0;
/// Game units in a metre for the distance shown (a unit is an inch, as Quake-family
/// tools take it).
const UNITS_PER_METRE: f32 = 39.37;
/// Legacy client slots.
const SLOTS: usize = 32;
/// `g_gametype` of Power Duel and of the first team game (`gametype_t`).
const GT_POWERDUEL: i32 = 4;
const GT_TEAM: i32 = 6;
/// `team_t` in a player's configstring (`t`).
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
const TEAM_SPECTATOR: i32 = 3;
/// `duelTeam_t` in a player's configstring (`ds`).
const DUELTEAM_LONE: i32 = 1;
const DUELTEAM_DOUBLE: i32 = 2;

/// Who a ping is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Scope {
    /// A player of a game without teams: everyone on the server, spectators included.
    All,
    /// A red team player.
    Red,
    /// A blue team player.
    Blue,
    /// The lone duelist of Power Duel.
    Lone,
    /// The pair of Power Duel.
    Double,
    /// A spectator: other spectators only.
    Spec,
}

impl Scope {
    const ALL: [Self; 6] = [
        Self::All,
        Self::Red,
        Self::Blue,
        Self::Lone,
        Self::Double,
        Self::Spec,
    ];

    /// The word the hub carries (`PROTOCOL.md`, "Pings").
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Red => "red",
            Self::Blue => "blue",
            Self::Lone => "lone",
            Self::Double => "double",
            Self::Spec => "spec",
        }
    }

    /// The scope a ping names, `None` for a word this client does not know.
    pub(crate) fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.name() == word)
    }

    /// Who a player of `team` (their configstring's `t`) and duel side `duel_side`
    /// (`ds`) pings for under `gametype` (`g_gametype`): a spectator for spectators, a
    /// team game's player for their team, a Power Duel player for their side, anyone
    /// else for everyone. `None` for a team game's player on no team.
    pub(crate) fn of(gametype: i32, team: i32, duel_side: i32) -> Option<Self> {
        if team == TEAM_SPECTATOR {
            return Some(Self::Spec);
        }
        if gametype >= GT_TEAM {
            return match team {
                TEAM_RED => Some(Self::Red),
                TEAM_BLUE => Some(Self::Blue),
                _ => None,
            };
        }
        if gametype == GT_POWERDUEL {
            return match duel_side {
                DUELTEAM_LONE => Some(Self::Lone),
                DUELTEAM_DOUBLE => Some(Self::Double),
                _ => None,
            };
        }
        Some(Self::All)
    }

    /// Whether a reader pinging for `self` sees a ping for `ping`: their own team's (or
    /// side's, or everyone's in a game without teams); a spectator sees spectators' and,
    /// in a game without teams, the players' too, never a team's. Players never see
    /// spectators' pings, so a spectator cannot guide them.
    pub(crate) fn sees(self, ping: Self) -> bool {
        self == ping || (self == Self::Spec && ping == Self::All)
    }
}

/// The scope of the player in `slot` of `game`, from its configstring and the
/// server's `g_gametype`.
pub(crate) fn scope_in(game: &sjk_protocol::GameState, slot: usize) -> Option<Scope> {
    let number = |bytes: Option<&[u8]>, key: &str| {
        bytes
            .and_then(|bytes| sjk_client::LegacyClientInfo::new(bytes).integer(key))
            .unwrap_or(0)
    };
    let player = game.config_string(crate::player_identity::CS_PLAYERS + slot)?;
    let gametype = number(game.config_string(0), "g_gametype");
    Scope::of(
        gametype,
        number(Some(player), "t"),
        number(Some(player), "ds"),
    )
}

/// One ping on show.
struct Mark {
    point: Vec3,
    slot: u8,
    own: bool,
    born: Instant,
    /// The name shown above it, with its colour codes.
    name: String,
    /// The distance shown under it, written again each frame into the same string.
    distance: String,
}

/// Why the player's own ping was not sent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Held {
    /// Less than a second after the last.
    TooSoon,
    /// The window's pings are spent.
    TooMany,
}

/// The pings on show and what limits them.
pub(crate) struct WorldPings {
    marks: Vec<Mark>,
    /// When the player's own pings of the last [`SEND_WINDOW`] went.
    sent: VecDeque<Instant>,
    /// When each slot's pings of the last [`RECEIVE_WINDOW`] came.
    received: [VecDeque<Instant>; SLOTS],
    sounded: Option<Instant>,
    /// Shapes and text ids, submitted with the HUD's.
    pub(crate) list: DrawList,
    /// The last ping outcome looked at (`player_identity::ping_outcome`).
    pub(crate) outcome_seen: u64,
    /// The last refusal said in the chat, and when, so a burst says it once.
    pub(crate) refusal_said: Option<(String, Instant)>,
    /// The game server the pings belong to.
    pub(crate) server: Option<std::net::SocketAddr>,
    /// World shots draw the pings without a game.
    #[cfg(test)]
    pub(crate) forced: bool,
}

impl Default for WorldPings {
    fn default() -> Self {
        Self {
            marks: Vec::with_capacity(MAX_MARKS),
            sent: VecDeque::with_capacity(SEND_PER_WINDOW + 1),
            received: std::array::from_fn(|_| VecDeque::new()),
            sounded: None,
            list: DrawList::new(MAX_MARKS * 8),
            outcome_seen: 0,
            refusal_said: None,
            server: None,
            #[cfg(test)]
            forced: false,
        }
    }
}

impl WorldPings {
    /// Whether the player may send a ping at `now`; when so, it counts.
    pub(crate) fn may_send(&mut self, now: Instant) -> Result<(), Held> {
        while self
            .sent
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= SEND_WINDOW)
        {
            self.sent.pop_front();
        }
        if self
            .sent
            .back()
            .is_some_and(|at| now.saturating_duration_since(*at) < SEND_GAP)
        {
            return Err(Held::TooSoon);
        }
        if self.sent.len() >= SEND_PER_WINDOW {
            return Err(Held::TooMany);
        }
        self.sent.push_back(now);
        Ok(())
    }

    /// Whether a ping from `slot` received at `now` may show; when so, it counts.
    pub(crate) fn admit(&mut self, slot: u8, now: Instant) -> bool {
        let Some(times) = self.received.get_mut(usize::from(slot)) else {
            return false;
        };
        while times
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= RECEIVE_WINDOW)
        {
            times.pop_front();
        }
        if times
            .back()
            .is_some_and(|at| now.saturating_duration_since(*at) < RECEIVE_GAP)
            || times.len() >= RECEIVE_PER_WINDOW
        {
            return false;
        }
        times.push_back(now);
        true
    }

    /// Show a ping of `slot` (named `name`) at `point`: the sender's oldest goes when
    /// they have [`PER_SENDER`] on show, the oldest of all at [`MAX_MARKS`].
    pub(crate) fn place(&mut self, point: Vec3, slot: u8, own: bool, name: &str, now: Instant) {
        self.expire(now);
        let theirs = self.marks.iter().filter(|mark| mark.slot == slot).count();
        if theirs >= PER_SENDER
            && let Some(oldest) = self.marks.iter().position(|mark| mark.slot == slot)
        {
            self.marks.remove(oldest);
        }
        if self.marks.len() >= MAX_MARKS {
            self.marks.remove(0);
        }
        self.marks.push(Mark {
            point,
            slot,
            own,
            born: now,
            name: name.to_owned(),
            distance: String::with_capacity(12),
        });
    }

    /// Drop the pings older than [`LIFE`].
    pub(crate) fn expire(&mut self, now: Instant) {
        self.marks
            .retain(|mark| now.saturating_duration_since(mark.born) < LIFE);
    }

    /// Forget every ping and limit (another server, or none).
    pub(crate) fn clear(&mut self) {
        self.marks.clear();
        self.received.iter_mut().for_each(VecDeque::clear);
        self.list.clear();
    }

    /// How many pings show.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.marks.len()
    }

    /// Whether a ping sound may play at `now`; when so, it counts.
    pub(crate) fn sound_due(&mut self, now: Instant) -> bool {
        if self
            .sounded
            .is_some_and(|at| now.saturating_duration_since(at) < SOUND_GAP)
        {
            return false;
        }
        self.sounded = Some(now);
        true
    }

    /// Lay the pings out for `camera` at `now`; nothing when not `shown`.
    pub(crate) fn build(&mut self, camera: Option<Camera>, now: Instant, shown: bool) {
        self.list.clear();
        self.expire(now);
        let Some(camera) = camera.filter(|_| shown) else {
            return;
        };
        let unit = crate::ui_scale::height_scale(camera.viewport[1]);
        let margin = 44.0 * unit;
        for (index, mark) in self.marks.iter_mut().enumerate() {
            let age = now.saturating_duration_since(mark.born);
            let Some((at, on_screen)) = screen_place(camera, mark.point, margin) else {
                continue;
            };
            let alpha = fade(age);
            let metres = (mark.point.distance(camera.eye) / UNITS_PER_METRE).round();
            mark.distance.clear();
            let _ = write!(mark.distance, "{metres:.0} m");
            let colour = if mark.own {
                Color::new(1.0, 0.93, 0.74, alpha)
            } else {
                crate::sjk_chat_look::gold(alpha)
            };
            let pop = 1.0 + 0.5 * (1.0 - (age.as_secs_f32() / POP.as_secs_f32()).min(1.0)).powi(2);
            let radius = if on_screen { 13.0 } else { 10.0 } * unit * pop;
            let full = std::f32::consts::TAU;
            let ring = |radius: f32, width: f32, color: Color| DrawCommand::Arc {
                center: at,
                radius,
                width,
                start: 0.0,
                sweep: full,
                color,
                knockout: None,
            };
            let _ = self.list.push(ring(
                radius,
                6.0 * unit,
                Color::new(0.0, 0.0, 0.0, 0.45 * alpha),
            ));
            let _ = self.list.push(ring(radius, 3.0 * unit, colour));
            let dot = 6.0 * unit;
            let _ = self.list.push(DrawCommand::RoundedRect {
                rect: Rect::new(at[0] - dot * 0.5, at[1] - dot * 0.5, dot, dot),
                radius: dot * 0.5,
                color: colour,
            });
            let pulse = age.as_secs_f32() / PULSE.as_secs_f32();
            if pulse < 1.0 {
                let _ = self.list.push(ring(
                    radius * (1.0 + 2.0 * pulse),
                    2.0 * unit,
                    Color::new(colour.r, colour.g, colour.b, alpha * (1.0 - pulse)),
                ));
            }
            let id = u32::try_from(index).unwrap_or(0) * 2;
            let width = (260.0 * unit).min(camera.viewport[0]);
            let x = (at[0] - width * 0.5).clamp(0.0, (camera.viewport[0] - width).max(0.0));
            let text = |rect: Rect, id: u32, size: f32, color: Color| DrawCommand::Text {
                rect,
                text: TextId(id),
                size,
                color,
                align: TextAlign::Center,
                overflow: TextOverflow::Ellipsis,
                weight: FontWeight::Regular,
                letter_spacing: 0.0,
            };
            let name_size = 18.0 * unit;
            let distance_size = 15.0 * unit;
            // Off screen the marker sits on the border: its words go to the inner side.
            let below = !on_screen && at[1] < camera.viewport[1] * 0.5;
            if on_screen {
                let _ = self.list.push(text(
                    Rect::new(
                        x,
                        at[1] - radius - 4.0 * unit - name_size * 1.4,
                        width,
                        name_size * 1.4,
                    ),
                    id,
                    name_size,
                    Color::new(1.0, 1.0, 1.0, alpha),
                ));
            }
            let distance_y = if on_screen || below {
                at[1] + radius + 3.0 * unit
            } else {
                at[1] - radius - 3.0 * unit - distance_size * 1.4
            };
            let _ = self.list.push(text(
                Rect::new(x, distance_y, width, distance_size * 1.4),
                id + 1,
                distance_size,
                colour,
            ));
        }
    }

    /// The text of id `id` of [`Self::list`]: a ping's name or distance.
    pub(crate) fn text(&self, id: TextId) -> &str {
        let index = (id.0 / 2) as usize;
        self.marks.get(index).map_or("", |mark| {
            if id.0.is_multiple_of(2) {
                mark.name.as_str()
            } else {
                mark.distance.as_str()
            }
        })
    }
}

/// How opaque a ping `age` old is: whole until the last [`FADE`] of [`LIFE`].
fn fade(age: Duration) -> f32 {
    let left = LIFE.saturating_sub(age);
    (left.as_secs_f32() / FADE.as_secs_f32()).clamp(0.0, 1.0)
}

/// Where `point` goes on `camera`'s screen, and whether it is on it: its projection when
/// that lies `margin` pixels inside the edges, else the place on that inset border
/// toward it (a point behind the eye lands on the side it lies on, the bottom when
/// straight behind). `None` for a degenerate camera.
pub(crate) fn screen_place(camera: Camera, point: Vec3, margin: f32) -> Option<([f32; 2], bool)> {
    let forward = (camera.target - camera.eye).normalize_or_zero();
    let right = forward.cross(camera.up).normalize_or_zero();
    if forward == Vec3::ZERO || right == Vec3::ZERO {
        return None;
    }
    let up = right.cross(forward);
    let [width, height] = camera.viewport;
    let (cx, cy) = (width * 0.5, height * 0.5);
    let delta = point - camera.eye;
    let depth = delta.dot(forward);
    let (dx, dy) = (delta.dot(right), -delta.dot(up));
    let tan = (camera.fov.to_radians() * 0.5).tan();
    if depth > 0.01 && tan > 0.0 {
        let scale = cy / (tan * depth);
        let (x, y) = (cx + dx * scale, cy + dy * scale);
        if (margin..=width - margin).contains(&x) && (margin..=height - margin).contains(&y) {
            return Some(([x, y], true));
        }
    }
    let (dx, dy) = if dx.abs() < 1e-3 && dy.abs() < 1e-3 {
        (0.0, 1.0)
    } else {
        (dx, dy)
    };
    let (reach_x, reach_y) = ((cx - margin).max(1.0), (cy - margin).max(1.0));
    let scale = (reach_x / dx.abs().max(1e-6)).min(reach_y / dy.abs().max(1e-6));
    Some(([cx + dx * scale, cy + dy * scale], false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_follow_the_game_and_the_team() {
        // FFA, Holocron, Jedi Master, Duel: everyone.
        for gametype in [0, 1, 2, 3] {
            assert_eq!(Scope::of(gametype, 0, 0), Some(Scope::All));
            assert_eq!(Scope::of(gametype, TEAM_SPECTATOR, 0), Some(Scope::Spec));
        }
        // Team FFA, Siege, CTF, CTY: the team.
        for gametype in [6, 7, 8, 9] {
            assert_eq!(Scope::of(gametype, TEAM_RED, 0), Some(Scope::Red));
            assert_eq!(Scope::of(gametype, TEAM_BLUE, 0), Some(Scope::Blue));
            assert_eq!(Scope::of(gametype, TEAM_SPECTATOR, 0), Some(Scope::Spec));
            assert_eq!(
                Scope::of(gametype, 0, 0),
                None,
                "a team game's player on no team"
            );
        }
        // Power Duel: the side.
        assert_eq!(Scope::of(GT_POWERDUEL, 0, DUELTEAM_LONE), Some(Scope::Lone));
        assert_eq!(
            Scope::of(GT_POWERDUEL, 0, DUELTEAM_DOUBLE),
            Some(Scope::Double)
        );
        assert_eq!(Scope::of(GT_POWERDUEL, 0, 0), None);
        for scope in Scope::ALL {
            assert_eq!(Scope::parse(scope.name()), Some(scope));
            // The hub's rule: 1 to 16 of a to z.
            assert!((1..=16).contains(&scope.name().len()));
            assert!(scope.name().bytes().all(|byte| byte.is_ascii_lowercase()));
        }
        assert_eq!(Scope::parse("purple"), None);
    }

    #[test]
    fn teammates_see_each_other_and_spectators_never_guide_players() {
        use Scope::*;
        assert!(All.sees(All));
        assert!(Red.sees(Red) && Blue.sees(Blue) && Lone.sees(Lone) && Double.sees(Double));
        assert!(!Red.sees(Blue) && !Blue.sees(Red) && !Lone.sees(Double));
        // A spectator sees a game without teams, and other spectators.
        assert!(Spec.sees(All) && Spec.sees(Spec));
        // Never a team's or side's calls.
        assert!(!Spec.sees(Red) && !Spec.sees(Blue) && !Spec.sees(Lone));
        // Players never see a spectator's ping.
        for player in [All, Red, Blue, Lone, Double] {
            assert!(!player.sees(Spec));
        }
    }

    #[test]
    fn the_player_pings_once_a_second_four_in_ten() {
        let mut pings = WorldPings::default();
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        assert_eq!(pings.may_send(at(0)), Ok(()));
        assert_eq!(pings.may_send(at(500)), Err(Held::TooSoon));
        assert_eq!(pings.may_send(at(1000)), Ok(()));
        assert_eq!(pings.may_send(at(2000)), Ok(()));
        assert_eq!(pings.may_send(at(3000)), Ok(()));
        assert_eq!(pings.may_send(at(4000)), Err(Held::TooMany));
        // The first leaves the window ten seconds after it went.
        assert_eq!(pings.may_send(at(9999)), Err(Held::TooMany));
        assert_eq!(pings.may_send(at(10_000)), Ok(()));
    }

    #[test]
    fn each_sender_is_limited_on_receipt() {
        let mut pings = WorldPings::default();
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        assert!(pings.admit(3, at(0)));
        assert!(!pings.admit(3, at(200)), "too soon");
        assert!(pings.admit(4, at(200)), "another sender has their own");
        for ms in [600, 1200, 1800, 2400] {
            assert!(pings.admit(3, at(ms)));
        }
        assert!(!pings.admit(3, at(3000)), "five in ten seconds");
        assert!(pings.admit(3, at(10_000)));
        assert!(!pings.admit(40, at(0)), "not a slot");
    }

    #[test]
    fn a_sender_keeps_two_on_show_and_pings_end() {
        let mut pings = WorldPings::default();
        let t0 = Instant::now();
        for second in 0..3 {
            let now = t0 + Duration::from_secs(second);
            pings.place(Vec3::splat(second as f32), 3, false, "Sol", now);
        }
        pings.place(Vec3::ZERO, 4, false, "Fox", t0 + Duration::from_secs(3));
        assert_eq!(pings.len(), 3);
        // Sol's first went; the newest two stay.
        let sol: Vec<f32> = pings
            .marks
            .iter()
            .filter(|mark| mark.slot == 3)
            .map(|mark| mark.point.x)
            .collect();
        assert_eq!(sol, [1.0, 2.0]);
        pings.expire(t0 + Duration::from_secs(1) + LIFE);
        assert_eq!(pings.len(), 2, "pings last seven seconds");
        pings.clear();
        assert_eq!(pings.len(), 0);
        // At most 24 in all, the oldest going first.
        for slot in 0..30 {
            pings.place(Vec3::ZERO, slot, false, "x", t0);
        }
        assert_eq!(pings.len(), MAX_MARKS);
        assert_eq!(pings.marks[0].slot, 6);
    }

    #[test]
    fn pings_fade_over_their_last_second_and_a_half() {
        assert_eq!(fade(Duration::ZERO), 1.0);
        assert_eq!(fade(LIFE - FADE), 1.0);
        assert!((fade(LIFE - FADE / 2) - 0.5).abs() < 1e-3);
        assert_eq!(fade(LIFE), 0.0);
    }

    fn camera() -> Camera {
        Camera {
            eye: Vec3::ZERO,
            target: Vec3::X,
            up: Vec3::Z,
            fov: 90.0,
            viewport: [1920.0, 1080.0],
        }
    }

    #[test]
    fn pings_project_or_sit_on_the_border_toward_them() {
        let camera = camera();
        // Straight ahead: the centre.
        let (at, on) = screen_place(camera, Vec3::new(100.0, 0.0, 0.0), 40.0).unwrap();
        assert!(on && (at[0] - 960.0).abs() < 0.01 && (at[1] - 540.0).abs() < 0.01);
        // Up and to the left (`right` is -Y here) shows up and left of the centre.
        let (at, on) = screen_place(camera, Vec3::new(100.0, 10.0, 10.0), 40.0).unwrap();
        assert!(on && at[0] < 960.0 && at[1] < 540.0);
        // Far to the right: on the right border, at the inset.
        let (at, on) = screen_place(camera, Vec3::new(10.0, -100.0, 0.0), 40.0).unwrap();
        assert!(!on && (at[0] - 1880.0).abs() < 0.01 && (at[1] - 540.0).abs() < 0.01);
        // Behind and to the left: the left border.
        let (at, on) = screen_place(camera, Vec3::new(-100.0, 50.0, 0.0), 40.0).unwrap();
        assert!(!on && (at[0] - 40.0).abs() < 0.01);
        // Straight behind: the bottom.
        let (at, on) = screen_place(camera, Vec3::new(-100.0, 0.0, 0.0), 40.0).unwrap();
        assert!(!on && (at[1] - 1040.0).abs() < 0.01);
    }

    #[test]
    fn a_ping_draws_its_ring_name_and_distance() {
        let mut pings = WorldPings::default();
        let t0 = Instant::now();
        pings.place(Vec3::new(393.7, 0.0, 0.0), 3, false, "^2Sol", t0);
        pings.build(Some(camera()), t0 + Duration::from_secs(2), true);
        let texts: Vec<&str> = pings
            .list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(pings.text(*text)),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["^2Sol", "10 m"]);
        assert!(
            pings
                .list
                .commands()
                .iter()
                .any(|command| matches!(command, DrawCommand::Arc { .. }))
        );
        // Not shown (a menu, the scoreboard): nothing.
        pings.build(Some(camera()), t0, false);
        assert!(pings.list.commands().is_empty());
        // Behind: the border, with the distance only.
        pings.place(Vec3::new(-393.7, 0.0, 0.0), 4, false, "Fox", t0);
        pings.build(Some(camera()), t0, true);
        let texts = pings
            .list
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::Text { .. }))
            .count();
        assert_eq!(texts, 3);
    }
}
