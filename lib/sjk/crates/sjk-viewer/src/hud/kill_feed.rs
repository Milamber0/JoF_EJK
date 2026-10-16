//! The kill feed (`cg_killfeed`): the last kills at the top right, newest at the
//! top, each `killer [icon] victim`, or `[skull] victim` for a suicide or a death
//! to the world.
//!
//! Entries come from the shared obituary tracker ([`sjk_client::ObituaryTracker`]),
//! never a second event observer. Names are read when the kill arrives, as the
//! console's line is printed then ([`sjk_client::obituary_name`]), and keep their
//! colour codes. A kill's icon is the HUD's cause-of-death picture for its means of
//! death: an icon pack's `hud/mod/*` when one is installed, else the weapon's own
//! `gfx/hud/w_icon_*` (`MOD_ITEMS` in [`super::icons::assets`], the weapon each
//! `MOD_*` belongs to in OpenJK's `g_weapon.c`). A dark Force kill (lightning or
//! grip, told apart by the killer's active powers) falls back to that power's
//! holocron, and a player knocked to their death (`KILLED_FORCETOSS`) to Force
//! Push's. Suicides, deaths to the world and causes no weapon deals get a skull
//! drawn from shapes; a weapon whose picture did not load gets a short word.
//!
//! The feed stands under whatever the HUD already draws at the top right (FPS,
//! team overlay, duel portrait, snapshot, inventory, powerup column) and keeps
//! right of the top centre's vote and timer. It is drawn with the HUD's own draw
//! list, so it hides with the HUD (scoreboard, menus, intermission, `cg_draw2D 0`).
//! Entries live in a fixed ring of [`CAPACITY`]; after a kill is taken in, frames
//! only measure and emit, with no formatting or allocation.

use super::icons::Icons;
use crate::console::ViewerConsole;
use crate::text::{TextFace, UiFont};
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_ui::{
    Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextId, TextOverflow, TextureId,
    Theme,
};

/// Entries shown at most; a new kill pushes the oldest out.
pub(crate) const CAPACITY: usize = 5;
/// An entry stays fully visible this long after its kill (server milliseconds)...
pub(crate) const HOLD_MS: i32 = 5_000;
/// ...then fades out over this long.
pub(crate) const FADE_MS: i32 = 1_000;
/// A kill stamped this far after the HUD's clock belongs to an earlier timeline
/// (a map or demo restart); it is not shown. The HUD's clock runs a little behind
/// the newest snapshot, so a kill is a few milliseconds ahead of it at first.
const FUTURE_MS: i32 = 1_000;
/// Draw commands one entry can take: its plate and outline, two names and the
/// skull's seven shapes.
pub(crate) const ENTRY_COMMANDS: usize = 11;
/// First text id of the feed: three per ring slot (killer, victim, word).
pub(crate) const TEXT_FIRST: u32 = 500;
/// Last text id of the feed.
pub(crate) const TEXT_LAST: u32 = TEXT_FIRST + 3 * CAPACITY as u32 - 1;

/// `FP_PUSH`, `FP_GRIP` and `FP_LIGHTNING` (`forcePowers_t`), the holocrons that
/// stand in for a Force toss and a dark Force kill.
const FP_PUSH: u8 = 3;
const FP_GRIP: u8 = 6;
const FP_LIGHTNING: u8 = 7;

// Geometry in 1080-line pixels, grown with the window and `cg_hudScale`.
/// Gap from the right edge, as the team overlay's.
const RIGHT: f32 = 32.0;
/// Top of the feed when nothing stands at the top right.
const TOP: f32 = 32.0;
/// Gap under what stands above.
const GAP_BELOW: f32 = 10.0;
/// Room kept right of the screen's centre: the vote panel's half width and a gap.
const CENTRE_CLEARANCE: f32 = 320.0;
const ROW_GAP: f32 = 4.0;
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 4.0;
/// Between a name and the icon.
const SPACE: f32 = 8.0;
/// Longest a name may draw before it ends in an ellipsis.
const NAME_MAX: f32 = 240.0;
/// Name line box.
const TEXT: f32 = 22.0;
const RADIUS: f32 = 4.0;
/// `cg_killfeedIconSize` is in the 480-line virtual screen.
const VIRTUAL_TO_1080: f32 = 1_080.0 / 480.0;

/// Register the feed's options.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, value, help) in [
        (
            "cg_killfeed",
            1,
            "Kill feed at the top right: killer, weapon icon, victim",
        ),
        (
            "cg_killfeedColors",
            0,
            "Tint kill feed icons by cause of death",
        ),
    ] {
        cvars.register(CvarDefinition::new(
            name,
            value as i64,
            CvarFlags::ARCHIVE,
            help,
        ))?;
    }
    for (name, value, help) in [
        (
            "cg_killfeedX",
            0.0,
            "Kill feed leftward offset in virtual units",
        ),
        (
            "cg_killfeedY",
            0.0,
            "Kill feed downward offset in virtual units",
        ),
        (
            "cg_killfeedTextSize",
            0.8,
            "Kill feed name size; 0.8 is the default size, zero uses it",
        ),
        (
            "cg_killfeedIconSize",
            12.0,
            "Kill feed icon size in virtual units; zero uses 18",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}

/// The feed's options, sampled once per HUD update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Options {
    /// `cg_killfeed`.
    pub(crate) enabled: bool,
    /// `cg_killfeedX` (leftward) and `cg_killfeedY` (downward), in 640x480 units.
    pub(crate) offset: [f32; 2],
    /// Name size multiplier (`cg_killfeedTextSize` over its 0.8 default).
    pub(crate) text_scale: f32,
    /// Icon size in 480-line virtual units (`cg_killfeedIconSize`).
    pub(crate) icon_size: f32,
    /// Tint pictures by cause of death (`cg_killfeedColors`).
    pub(crate) colors: bool,
    /// The FPS readout stands at the top right this frame.
    pub(crate) fps: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self::read(None)
    }
}

impl Options {
    fn read(console: Option<&ViewerConsole>) -> Self {
        let f = |name, fallback| crate::cgame_options::scalar(console, name, fallback);
        Self {
            enabled: super::family::integer(console, "cg_killfeed", 1) != 0,
            offset: [f("cg_killfeedx", 0.0), f("cg_killfeedy", 0.0)],
            text_scale: match f("cg_killfeedtextsize", 0.8) {
                0.0 => 1.0,
                value => (value / 0.8).clamp(0.25, 3.0),
            },
            icon_size: match f("cg_killfeediconsize", 12.0) {
                0.0 => 18.0,
                value => value.clamp(4.0, 48.0),
            },
            colors: super::family::integer(console, "cg_killfeedcolors", 0) != 0,
            fps: super::family::fps(console),
        }
    }
}

/// How one means of death shows when its picture is missing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cause {
    /// A Force power whose holocron stands in for the missing picture.
    pub(crate) holocron: Option<u8>,
    /// The word drawn when no picture loaded; `None` draws the skull instead
    /// (deaths to the world and causes no weapon deals).
    pub(crate) word: Option<&'static str>,
}

/// `meansOfDeath_t` (`bg_public.h`) for a kill by another player.
pub(crate) fn cause(means: u8) -> Cause {
    let word = match means {
        1 => "STUN",
        2 => "MELEE",
        3 => "SABER",
        4 | 5 => "BRYAR",
        6 => "BLASTER",
        7 => "TURBOLASER",
        8..=10 => "DISRUPTOR",
        11 => "BOWCASTER",
        12..=14 => "REPEATER",
        15 | 16 => "DEMP2",
        17 | 18 => "FLECHETTE",
        19..=22 => "ROCKET",
        23 | 24 => "THERMAL",
        25 | 26 => "MINE",
        27 => "DETPACK",
        28 => "VEHICLE",
        29 | 30 => "CONCUSSION",
        // MOD_FORCE_DARK: Force Lightning and Grip.
        31 => "FORCE",
        32 => "SENTRY",
        // MOD_FALLING with a killer: `CG_Obituary`'s `KILLED_FORCETOSS`.
        38 => "FORCE",
        // MOD_UNKNOWN, water, slime, lava, crush, telefrag, suicide, the target
        // laser, trigger_hurt, a team change and mods' own causes.
        _ => {
            return Cause {
                holocron: None,
                word: None,
            };
        }
    };
    Cause {
        holocron: match means {
            31 => Some(FP_LIGHTNING),
            38 => Some(FP_PUSH),
            _ => None,
        },
        word: Some(word),
    }
}

/// What stands between (or before) the names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mark {
    /// A loaded picture.
    Picture(TextureId),
    /// A short word in place of a picture that did not load.
    Word(&'static str),
    /// The drawn skull.
    Skull,
}

/// The mark of a kill by `means`: the skull for a suicide or death to the world
/// (`solo`), else the cause-of-death picture, the holocron standing in for it,
/// the cause's word, or the skull for causes no weapon deals.
pub(crate) fn mark(
    means: u8,
    attacker_force: u32,
    solo: bool,
    picture: Option<TextureId>,
    holocron: impl Fn(u8) -> Option<TextureId>,
) -> Mark {
    if solo {
        return Mark::Skull;
    }
    let cause = cause(means);
    if let Some(picture) =
        picture.or_else(|| holocron_power(means, attacker_force).and_then(holocron))
    {
        return Mark::Picture(picture);
    }
    cause.word.map_or(Mark::Skull, Mark::Word)
}

/// The holocron of a dark Force kill (`MOD_FORCE_DARK` covers two powers): Force
/// Grip's when only the attacker's grip was active as the kill landed, else
/// Force Lightning's. Other causes keep their own ([`cause`]).
pub(crate) fn holocron_power(means: u8, attacker_force: u32) -> Option<u8> {
    let power = cause(means).holocron?;
    let grip = attacker_force & (1 << FP_GRIP) != 0;
    let lightning = attacker_force & (1 << FP_LIGHTNING) != 0;
    Some(if power == FP_LIGHTNING && grip && !lightning {
        FP_GRIP
    } else {
        power
    })
}

/// Opacity of an entry `age` milliseconds after its kill.
pub(crate) fn alpha(age: i32) -> f32 {
    if !(-FUTURE_MS..HOLD_MS + FADE_MS).contains(&age) {
        0.0
    } else if age <= HOLD_MS {
        1.0
    } else {
        1.0 - (age - HOLD_MS) as f32 / FADE_MS as f32
    }
}

struct Entry {
    killer: String,
    victim: String,
    means: u8,
    /// The killer's active Force powers (see [`holocron_power`]).
    attacker_force: u32,
    /// A suicide or a death to the world: no killer.
    solo: bool,
    /// The viewed player killed or died.
    local: bool,
    /// Server time of the kill.
    start: i32,
}

impl Entry {
    fn new() -> Self {
        Self {
            // Room for a 35-character name of two-byte letters and colour codes.
            killer: String::with_capacity(96),
            victim: String::with_capacity(96),
            means: 0,
            attacker_force: 0,
            solo: true,
            local: false,
            start: i32::MIN,
        }
    }
}

/// The feed's retained entries and options.
pub(crate) struct Feed {
    entries: [Entry; CAPACITY],
    /// Ring slot of the newest entry.
    newest: usize,
    len: usize,
    /// Obituaries of the tracker already taken in.
    consumed: u64,
    /// The HUD's server time at the last update.
    now: i32,
    options: Options,
}

impl Default for Feed {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| Entry::new()),
            newest: CAPACITY - 1,
            len: 0,
            consumed: 0,
            now: 0,
            options: Options::default(),
        }
    }
}

impl Feed {
    /// Read the options for this frame.
    pub(crate) fn sample(&mut self, console: Option<&ViewerConsole>) {
        self.options = Options::read(console);
    }

    /// The options read by the last [`Feed::sample`].
    pub(crate) fn options(&self) -> Options {
        self.options
    }

    /// Take in the tracker's new obituaries and advance the clock to `now`, the
    /// HUD's server time; `local` is the viewed player's client number.
    pub(crate) fn observe(
        &mut self,
        tracker: &sjk_client::ObituaryTracker,
        game: &sjk_protocol::GameState,
        now: i32,
        local: u16,
    ) {
        // A new tracker (a new connection), or the clock gone back past the newest
        // kill (a map or demo restart): what is shown belongs to another timeline.
        if tracker.decoded() < self.consumed {
            self.consumed = 0;
            self.len = 0;
        }
        if now.saturating_add(FUTURE_MS) < self.now {
            self.len = 0;
        }
        self.now = now;
        let fresh = tracker
            .decoded()
            .saturating_sub(self.consumed)
            .min(tracker.feed().len() as u64) as usize;
        for offset in (0..fresh).rev() {
            if let Some(event) = tracker.feed().newest(offset)
                && alpha(now.saturating_sub(event.server_time)) > 0.0
            {
                self.push(&event, game, local);
            }
        }
        self.consumed = tracker.decoded();
    }

    fn push(
        &mut self,
        event: &sjk_client::ObituaryEvent,
        game: &sjk_protocol::GameState,
        local: u16,
    ) {
        self.newest = (self.newest + 1) % CAPACITY;
        self.len = (self.len + 1).min(CAPACITY);
        let entry = &mut self.entries[self.newest];
        // The console's two-part line (`victim message`) has no killer.
        entry.solo = event.attacker_message.is_none();
        entry.means = event.means_of_death;
        entry.attacker_force = event.attacker_force;
        entry.start = event.server_time;
        entry.local = event.target == local || (!entry.solo && event.attacker == local);
        entry.victim.clear();
        entry
            .victim
            .push_str(&sjk_client::obituary_name(game, event.target));
        entry.killer.clear();
        if !entry.solo {
            entry
                .killer
                .push_str(&sjk_client::obituary_name(game, event.attacker));
        }
    }

    /// Ring slots of the entries still showing, newest first, with their opacity.
    fn shown(&self) -> impl Iterator<Item = (usize, f32)> + '_ {
        (0..self.len).filter_map(move |age_rank| {
            let slot = (self.newest + CAPACITY - age_rank) % CAPACITY;
            let alpha = alpha(self.now.saturating_sub(self.entries[slot].start));
            (alpha > 0.0).then_some((slot, alpha))
        })
    }

    /// Entries showing now.
    #[cfg(test)]
    pub(crate) fn showing(&self) -> usize {
        self.shown().count()
    }

    /// The text of feed id `id` (`TEXT_FIRST..=TEXT_LAST`).
    pub(crate) fn text(&self, id: u32) -> &str {
        let index = id.saturating_sub(TEXT_FIRST) as usize;
        let Some(entry) = self.entries.get(index / 3) else {
            return "";
        };
        match index % 3 {
            0 => &entry.killer,
            1 => &entry.victim,
            _ => cause(entry.means).word.unwrap_or(""),
        }
    }

    /// Emit the showing entries into `list` in `area`; returns the bottom of the
    /// last one (`area.top` when none shows).
    pub(super) fn emit(
        &self,
        list: &mut DrawList,
        area: Area,
        art: Art<'_>,
        font: &UiFont,
        theme: Theme,
    ) -> f32 {
        if !self.options.enabled {
            return area.top;
        }
        let k = area.scale;
        let size = TEXT * k * self.options.text_scale;
        let word_size = size * 0.75;
        let icon = self.options.icon_size * VIRTUAL_TO_1080 * k;
        let row = size.max(icon) + 2.0 * PAD_Y * k;
        let face = TextFace::Semibold;
        let measure = |text: &str, size: f32| {
            crate::text::visible_text_width_style(
                font,
                text,
                size / font.height.max(1.0),
                face,
                0.0,
            )
        };
        let room = (area.right - area.left).max(0.0);
        let mut y = area.top;
        for (slot, alpha) in self.shown() {
            let entry = &self.entries[slot];
            let mark = mark(
                entry.means,
                entry.attacker_force,
                entry.solo,
                art.icons.means(entry.means),
                |power| art.holocrons.get(usize::from(power)).copied().flatten(),
            );
            let mark_width = match mark {
                Mark::Word(word) => measure(word, word_size),
                Mark::Picture(_) | Mark::Skull => icon,
            };
            let names = if entry.solo { 1.0 } else { 2.0 };
            let fixed = 2.0 * PAD_X * k + mark_width + names * SPACE * k;
            let name_max = ((room - fixed) / names).clamp(0.0, NAME_MAX * k);
            let victim = measure(&entry.victim, size).min(name_max);
            let killer = if entry.solo {
                0.0
            } else {
                measure(&entry.killer, size).min(name_max)
            };
            let plate = Rect::new(
                area.right - (fixed + killer + victim),
                y,
                fixed + killer + victim,
                row,
            );
            let _ = list.push(DrawCommand::RoundedRect {
                rect: plate,
                radius: RADIUS * k,
                color: Color::new(0.0, 0.0, 0.0, if entry.local { 0.55 } else { 0.38 } * alpha),
            });
            if entry.local {
                let _ = list.push(DrawCommand::Border {
                    rect: plate,
                    radius: RADIUS * k,
                    width: (1.5 * k).max(1.0),
                    color: Color::new(1.0, 1.0, 1.0, 0.55 * alpha),
                });
            }
            let text_y = y + (row - size) * 0.5;
            let mut x = plate.x + PAD_X * k;
            let name = |list: &mut DrawList, part: u32, x: f32, width: f32| {
                let _ = list.push(DrawCommand::Text {
                    // A pixel of slack: the renderer measures the same run and must
                    // not find it wider than its box.
                    rect: Rect::new(x, text_y, width + 1.0, size),
                    text: TextId(TEXT_FIRST + 3 * slot as u32 + part),
                    size,
                    color: Color::new(1.0, 1.0, 1.0, theme.foreground.a * alpha),
                    align: TextAlign::Start,
                    overflow: TextOverflow::Ellipsis,
                    weight: FontWeight::Semibold,
                    letter_spacing: 0.0,
                });
            };
            if !entry.solo {
                name(list, 0, x, killer);
                x += killer + SPACE * k;
            }
            let icon_rect = Rect::new(x, y + (row - icon) * 0.5, icon, icon);
            match mark {
                Mark::Picture(texture) => {
                    let [r, g, b] = if self.options.colors {
                        super::icons::assets::tint(usize::from(entry.means))
                    } else {
                        [1.0; 3]
                    };
                    let _ = list.push(DrawCommand::TexturedQuad {
                        rect: icon_rect,
                        texture,
                        color: Color::new(r, g, b, alpha),
                    });
                }
                Mark::Skull => skull(list, icon_rect, alpha),
                Mark::Word(_) => {
                    let mut muted = theme.muted;
                    muted.a *= alpha;
                    let _ = list.push(DrawCommand::Text {
                        rect: Rect::new(
                            x,
                            y + (row - word_size) * 0.5,
                            mark_width + 1.0,
                            word_size,
                        ),
                        text: TextId(TEXT_FIRST + 3 * slot as u32 + 2),
                        size: word_size,
                        color: muted,
                        align: TextAlign::Start,
                        overflow: TextOverflow::Clip,
                        weight: FontWeight::Semibold,
                        letter_spacing: 0.0,
                    });
                }
            }
            x += mark_width + SPACE * k;
            name(list, 1, x, victim);
            y += row + ROW_GAP * k;
        }
        if y > area.top { y - ROW_GAP * k } else { y }
    }
}

/// The pictures the feed draws from.
#[derive(Clone, Copy)]
pub(super) struct Art<'a> {
    /// The HUD's icons: the cause-of-death pictures.
    pub(super) icons: &'a Icons,
    /// The Force powers' holocrons, by `forcePowers_t`.
    pub(super) holocrons: &'a [Option<TextureId>],
}

/// Where the feed draws this frame, in window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Area {
    /// Right edge of the entries.
    pub(super) right: f32,
    /// Top of the newest entry.
    pub(super) top: f32,
    /// Entries end in an ellipsis rather than pass this (the top centre's widgets).
    pub(super) left: f32,
    /// Window pixels per 1080-line pixel, with `cg_hudScale`.
    pub(super) scale: f32,
}

impl Area {
    /// The feed's area under `stack_bottom`, the lowest edge of what the HUD
    /// already draws at the top right (zero: nothing). `scale` sizes the feed;
    /// `dpi_scale` is the layout's, which sizes the vote panel kept clear.
    pub(super) fn below(
        viewport: [f32; 2],
        scale: f32,
        dpi_scale: f32,
        stack_bottom: f32,
        options: Options,
    ) -> Self {
        let [width, height] = viewport;
        let under = if stack_bottom > 0.0 {
            stack_bottom + GAP_BELOW * scale
        } else {
            0.0
        };
        Self {
            right: width - RIGHT * scale - options.offset[0] * width / 640.0,
            top: (TOP * scale).max(under) + options.offset[1] * height / 480.0,
            left: width * 0.5 + CENTRE_CLEARANCE * dpi_scale,
            scale,
        }
    }
}

/// A skull from rounded shapes, filling `rect`: no game picture shows one.
fn skull(list: &mut DrawList, rect: Rect, alpha: f32) {
    let d = rect.width;
    let at =
        |x: f32, y: f32, w: f32, h: f32| Rect::new(rect.x + x * d, rect.y + y * d, w * d, h * d);
    let bone = Color::new(0.93, 0.91, 0.86, alpha);
    let hollow = Color::new(0.05, 0.05, 0.06, alpha);
    for (shape, radius, color) in [
        // Cranium, then the jaw.
        (at(0.12, 0.04, 0.76, 0.64), 0.36, bone),
        (at(0.28, 0.56, 0.44, 0.36), 0.08, bone),
        // Eyes and nose.
        (at(0.24, 0.30, 0.20, 0.20), 0.10, hollow),
        (at(0.56, 0.30, 0.20, 0.20), 0.10, hollow),
        (at(0.46, 0.52, 0.08, 0.10), 0.03, hollow),
        // Gaps between the teeth.
        (at(0.40, 0.74, 0.04, 0.18), 0.0, hollow),
        (at(0.56, 0.74, 0.04, 0.18), 0.0, hollow),
    ] {
        let _ = list.push(DrawCommand::RoundedRect {
            rect: shape,
            radius: radius * d,
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_client::ObituaryTracker;
    use sjk_protocol::{EntityState, GameState, LEGACY_ENTITY_FIELDS, PlayerState, Snapshot};

    const CS_PLAYERS: usize = 1_131;

    #[test]
    fn weapons_keep_their_word_and_world_causes_the_skull() {
        assert_eq!(cause(3).word, Some("SABER"));
        assert_eq!(cause(10).word, Some("DISRUPTOR"));
        assert_eq!(cause(20).word, Some("ROCKET"));
        assert_eq!(cause(31).holocron, Some(FP_LIGHTNING));
        assert_eq!(cause(38).holocron, Some(FP_PUSH));
        for world in [0, 33, 34, 35, 36, 37, 39, 40, 41, 42, 43, 200] {
            assert_eq!(
                cause(world),
                Cause {
                    holocron: None,
                    word: None
                },
                "{world}"
            );
        }
    }

    /// Each weapon's `MOD_*` falls back to that weapon's own HUD picture, as
    /// OpenJK's `g_weapon.c` gives each weapon's shots their means of death.
    #[test]
    fn causes_fall_back_to_the_picture_of_their_weapon() {
        use super::super::icons::assets::{MOD_ITEMS, WEAPONS};
        // (MOD_*, weapon_t)
        // (MOD_*, weapon_t): the stun baton, melee, the saber, the Bryar pistol and
        // its alt fire, the blaster, the disruptor with its splash and sniper shot,
        // the bowcaster, the repeater and its alt fire, the DEMP2, the flechette and
        // its mines, rockets (homing too), thermals, trip and timed mines, the det
        // pack and the concussion rifle.
        for (means, weapon) in [
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 4),
            (6, 5),
            (8, 6),
            (9, 6),
            (10, 6),
            (11, 7),
            (12, 8),
            (13, 8),
            (14, 8),
            (15, 9),
            (16, 9),
            (17, 10),
            (18, 10),
            (19, 11),
            (22, 11),
            (23, 12),
            (24, 12),
            (25, 13),
            (26, 13),
            (27, 14),
            (29, 15),
            (30, 15),
        ] {
            assert_eq!(MOD_ITEMS[means], WEAPONS[weapon], "MOD {means}");
        }
        // The sentry gun is an item, not a weapon.
        assert_eq!(
            crate::pickups::simple::ICONS[MOD_ITEMS[32]],
            "gfx/hud/i_icon_sentrygun"
        );
        assert_eq!(
            crate::pickups::simple::ICONS[MOD_ITEMS[3]],
            "gfx/hud/w_icon_lightsaber"
        );
    }

    #[test]
    fn the_mark_falls_back_from_picture_to_holocron_to_word() {
        let picture = Some(TextureId(7));
        let holocron = |power: u8| (power == FP_LIGHTNING).then_some(TextureId(9));
        let none = |_: u8| None;
        assert_eq!(
            mark(3, 0, false, picture, none),
            Mark::Picture(TextureId(7))
        );
        assert_eq!(mark(3, 0, false, None, none), Mark::Word("SABER"));
        assert_eq!(
            mark(31, 0, false, None, holocron),
            Mark::Picture(TextureId(9))
        );
        assert_eq!(mark(31, 0, false, None, none), Mark::Word("FORCE"));
        // Pushed into lava by someone: no weapon, the skull.
        assert_eq!(mark(35, 0, false, None, holocron), Mark::Skull);
        // Suicides and deaths to the world are always the skull.
        assert_eq!(mark(20, 0, true, picture, holocron), Mark::Skull);
        assert_eq!(mark(38, 0, true, None, holocron), Mark::Skull);
    }

    /// `MOD_FORCE_DARK` is Force Lightning's holocron unless the attacker's grip
    /// alone was active.
    #[test]
    fn a_grip_kill_shows_the_grip_holocron() {
        let grip = 1 << FP_GRIP;
        let lightning = 1 << FP_LIGHTNING;
        assert_eq!(holocron_power(31, 0), Some(FP_LIGHTNING));
        assert_eq!(holocron_power(31, lightning), Some(FP_LIGHTNING));
        assert_eq!(holocron_power(31, grip), Some(FP_GRIP));
        assert_eq!(holocron_power(31, grip | lightning), Some(FP_LIGHTNING));
        // Other causes keep their own holocron, or none.
        assert_eq!(holocron_power(38, grip), Some(FP_PUSH));
        assert_eq!(holocron_power(3, grip), None);
        let picture = |power: u8| Some(TextureId(u32::from(power)));
        assert_eq!(
            mark(31, grip, false, None, picture),
            Mark::Picture(TextureId(u32::from(FP_GRIP)))
        );
    }

    #[test]
    fn entries_hold_then_fade() {
        assert_eq!(alpha(0), 1.0);
        assert_eq!(alpha(-20), 1.0);
        assert_eq!(alpha(HOLD_MS), 1.0);
        assert!((alpha(HOLD_MS + FADE_MS / 2) - 0.5).abs() < 1e-6);
        assert_eq!(alpha(HOLD_MS + FADE_MS), 0.0);
        assert_eq!(alpha(-FUTURE_MS - 1), 0.0);
    }

    /// An obituary event entity: `target` killed by `attacker` with `means`.
    fn obituary(number: u16, target: u16, attacker: u16, means: u8) -> EntityState {
        let mut entity = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
        entity.set_raw_field(8, 18 + u32::from(sjk_client::EV_OBITUARY));
        entity.set_raw_field(59, u32::from(target));
        entity.set_raw_field(39, u32::from(attacker));
        entity.set_raw_field(42, u32::from(means));
        entity
    }

    fn game() -> GameState {
        let mut game = GameState::empty_local(0);
        for (client, info) in [
            (0, &b"n\\^1Sol\\t\\0"[..]),
            (1, b"n\\{JoF}emiah{I}\\t\\0"),
            (2, b"n\\R\xe9mi\\t\\0"),
        ] {
            game.replace_config_string(CS_PLAYERS + client, info.to_vec())
                .unwrap();
        }
        game
    }

    fn snapshot(time: i32, entities: Vec<EntityState>) -> Snapshot {
        Snapshot {
            message_sequence: 0,
            reliable_acknowledge: 0,
            server_commands: Vec::new(),
            server_time: time,
            delta_from: None,
            flags: 0,
            area_mask: Vec::new(),
            player: PlayerState::default(),
            vehicle_player: None,
            entities,
            consumed_bits: 0,
        }
    }

    /// Feed `events` (`target`, `attacker`, `MOD_*`), one snapshot each at
    /// `time`, through a real tracker; the viewed player is client 0.
    fn observe(
        feed: &mut Feed,
        tracker: &mut ObituaryTracker,
        game: &GameState,
        events: &[(u16, u16, u8)],
        time: i32,
    ) {
        for (index, (target, attacker, means)) in events.iter().enumerate() {
            let entity = obituary(100 + index as u16, *target, *attacker, *means);
            tracker.observe(&snapshot(time, vec![entity]), game);
        }
        feed.observe(tracker, game, time, 0);
    }

    #[test]
    fn kills_keep_names_from_the_moment_they_happened() {
        let mut game = game();
        let mut tracker = ObituaryTracker::new();
        let mut feed = Feed::default();
        observe(&mut feed, &mut tracker, &game, &[(2, 1, 3)], 1_000);
        // The victim leaves after dying; the feed keeps the name it showed.
        game.replace_config_string(CS_PLAYERS + 2, Vec::new())
            .unwrap();
        feed.observe(&tracker, &game, 1_100, 0);
        assert_eq!(feed.showing(), 1);
        let slot = feed.newest as u32;
        assert_eq!(feed.text(TEXT_FIRST + 3 * slot), "{JoF}emiah{I}");
        assert_eq!(feed.text(TEXT_FIRST + 3 * slot + 1), "Rémi");
        assert_eq!(feed.text(TEXT_FIRST + 3 * slot + 2), "SABER");
        assert!(!feed.entries[feed.newest].solo);
    }

    #[test]
    fn suicides_and_world_deaths_have_no_killer() {
        let game = game();
        let mut tracker = ObituaryTracker::new();
        let mut feed = Feed::default();
        // A fall to the world (ENTITYNUM_WORLD), then a rocket suicide.
        observe(
            &mut feed,
            &mut tracker,
            &game,
            &[(1, 1022, 38), (0, 0, 19)],
            500,
        );
        assert_eq!(feed.showing(), 2);
        for (slot, _) in feed.shown() {
            assert!(feed.entries[slot].solo);
            assert!(feed.entries[slot].killer.is_empty());
        }
        // The viewed player (client 0) killed themselves: highlighted.
        assert!(feed.entries[feed.newest].local);
        assert_eq!(feed.text(TEXT_FIRST + 3 * feed.newest as u32 + 1), "^1Sol");
    }

    #[test]
    fn the_ring_keeps_the_newest_five_and_expires_them() {
        let game = game();
        let mut tracker = ObituaryTracker::new();
        let mut feed = Feed::default();
        let kills: Vec<_> = (0..7).map(|n| (2, 1, n as u8 + 1)).collect();
        observe(&mut feed, &mut tracker, &game, &kills, 2_000);
        assert_eq!(feed.showing(), CAPACITY);
        // Newest first: means 7, 6, 5, 4, 3.
        let means: Vec<u8> = feed
            .shown()
            .map(|(slot, _)| feed.entries[slot].means)
            .collect();
        assert_eq!(means, [7, 6, 5, 4, 3]);
        feed.observe(&tracker, &game, 2_000 + HOLD_MS + FADE_MS / 2, 0);
        assert_eq!(feed.showing(), CAPACITY);
        feed.observe(&tracker, &game, 2_000 + HOLD_MS + FADE_MS, 0);
        assert_eq!(feed.showing(), 0);
    }

    #[test]
    fn old_or_foreign_kills_are_not_shown() {
        let game = game();
        let mut tracker = ObituaryTracker::new();
        let mut first = Feed::default();
        observe(&mut first, &mut tracker, &game, &[(2, 1, 3)], 10_000);
        // A HUD made later (a new map's) does not replay a kill from long ago...
        let mut late = Feed::default();
        late.observe(&tracker, &game, 10_000 + HOLD_MS + FADE_MS, 0);
        assert_eq!(late.showing(), 0);
        // ...and a clock gone back (a restart) clears what showed.
        first.observe(&tracker, &game, 2_000, 0);
        assert_eq!(first.showing(), 0);
    }

    #[test]
    fn the_area_stands_under_the_stack_and_right_of_the_centre() {
        let options = Options::default();
        let free = Area::below([1920.0, 1080.0], 1.0, 1.0, 0.0, options);
        assert_eq!(free.top, TOP);
        assert_eq!(free.right, 1920.0 - RIGHT);
        assert_eq!(free.left, 960.0 + CENTRE_CLEARANCE);
        let under = Area::below([1920.0, 1080.0], 1.0, 1.0, 300.0, options);
        assert_eq!(under.top, 300.0 + GAP_BELOW);
        let nudged = Area::below(
            [1920.0, 1080.0],
            1.0,
            1.0,
            0.0,
            Options {
                offset: [10.0, 20.0],
                ..options
            },
        );
        assert_eq!(nudged.right, 1920.0 - RIGHT - 30.0);
        assert_eq!(nudged.top, TOP + 45.0);
    }

    /// Five long-named kills with every kind of mark draw inside their area and
    /// their command budget, without the skull or a word overflowing it.
    #[test]
    fn a_full_feed_draws_inside_its_area_and_budget() {
        let font = crate::text::test_font();
        let mut game = game();
        let long = format!("n\\^3{}\\t\\0", "W".repeat(35));
        game.replace_config_string(CS_PLAYERS + 3, long.into_bytes())
            .unwrap();
        let mut tracker = ObituaryTracker::new();
        let mut feed = Feed::default();
        // Saber, a fall, a dark Force kill, lava by a player, a suicide.
        observe(
            &mut feed,
            &mut tracker,
            &game,
            &[(3, 1, 3), (3, 1022, 38), (1, 3, 31), (3, 1, 35), (0, 0, 39)],
            100,
        );
        // Only the saber's picture loaded; no holocron.
        let icons = Icons::with_means(&[(3, TextureId(1))]);
        let holocrons = [None; 8];
        for viewport in [[1920.0, 1080.0], [1280.0, 1024.0], [3840.0, 2160.0]] {
            let scale = crate::ui_scale::height_scale(viewport[1]);
            let area = Area::below(
                viewport,
                scale,
                scale.max(2.0 / 3.0),
                200.0 * scale,
                options_on(),
            );
            let mut list = DrawList::new(CAPACITY * ENTRY_COMMANDS);
            let bottom = feed.emit(
                &mut list,
                area,
                Art {
                    icons: &icons,
                    holocrons: &holocrons,
                },
                &font,
                Theme::default(),
            );
            assert!(list.len() <= CAPACITY * ENTRY_COMMANDS, "{}", list.len());
            assert!(bottom > area.top && bottom < viewport[1] * 0.5, "{bottom}");
            let mut texts = 0;
            for command in list.commands() {
                let rect = match *command {
                    DrawCommand::RoundedRect { rect, .. }
                    | DrawCommand::Border { rect, .. }
                    | DrawCommand::TexturedQuad { rect, .. } => rect,
                    DrawCommand::Text { rect, .. } => {
                        texts += 1;
                        rect
                    }
                    other => panic!("unexpected {other:?}"),
                };
                assert!(rect.x >= area.left - 0.5, "{rect:?} left of {}", area.left);
                assert!(rect.right() <= area.right + 1.5, "{rect:?}");
                assert!(rect.y >= area.top - 0.5 && rect.bottom() <= bottom + 0.5);
            }
            // Three kills with a killer and two solo deaths: eight names, plus the
            // word for the dark Force kill (no holocron loaded); the saber draws its
            // picture and the other three their skull.
            assert_eq!(texts, 9);
            let skull_shapes = list
                .commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::RoundedRect { .. }))
                .count();
            // Five plates and three skulls of seven shapes.
            assert_eq!(skull_shapes, 5 + 3 * 7);
        }
    }

    fn options_on() -> Options {
        Options {
            enabled: true,
            ..Options::default()
        }
    }

    #[test]
    fn a_feed_turned_off_draws_nothing() {
        let font = crate::text::test_font();
        let game = game();
        let mut tracker = ObituaryTracker::new();
        let mut feed = Feed::default();
        observe(&mut feed, &mut tracker, &game, &[(2, 1, 3)], 100);
        feed.options.enabled = false;
        let mut list = DrawList::new(64);
        let area = Area::below([1920.0, 1080.0], 1.0, 1.0, 0.0, feed.options);
        let icons = Icons::default();
        feed.emit(
            &mut list,
            area,
            Art {
                icons: &icons,
                holocrons: &[],
            },
            &font,
            Theme::default(),
        );
        assert!(list.is_empty());
    }

    #[test]
    fn feed_text_ids_stay_in_their_range() {
        let feed = Feed::default();
        assert_eq!(TEXT_LAST, 514);
        assert_eq!(feed.text(TEXT_LAST), "");
        assert_eq!(feed.text(TEXT_LAST + 1), "");
    }
}
