//! One frame of the menu-file status HUD, as OpenJK codemp `CG_DrawHUD`
//! (`cg_draw.c`) and its helpers draw it: `CG_DrawHealth`, `CG_DrawArmor`,
//! `CG_DrawSaberStyle`, `CG_DrawAmmo` and `CG_DrawForcePower` for the menu
//! HUD, and `CG_DrawSimple*` for the text-only HUD a nonzero `cg_hudFiles`
//! selects. Output is pictures and text runs in the 640x480 screen, written
//! into fixed storage without allocating.

use super::layout::{Layout, Piece, Side, TICS};
use std::fmt::Write as _;

/// Most pictures one frame can draw (two frames, 16 tics, 4 three-digit
/// numbers, a saber style and the painted backgrounds).
pub(crate) const MAX_PICTURES: usize = 96;
/// Text runs: the score line and the infinite-ammo mark, or the simple HUD.
pub(crate) const MAX_TEXTS: usize = 5;

/// `colorTable` entries (`shared/qcommon/q_color.c`) the HUD uses.
const WHITE: [f32; 4] = [1.0; 4];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const LIGHT_GREY: [f32; 4] = [0.75, 0.75, 0.75, 1.0];
const HUD_GREEN: [f32; 4] = [0.0, 0.613, 0.097, 1.0];
const HUD_RED: [f32; 4] = [0.835, 0.015, 0.015, 1.0];
const ICON_BLUE: [f32; 4] = [0.567, 0.685, 1.0, 0.75];
const HUD_ORANGE: [f32; 4] = [1.0, 0.658, 0.062, 1.0];

/// `weaponData[].ammoIndex` (`bg_weapons.c`), by `weapon_t`.
pub(crate) const AMMO_INDEX: [usize; 19] =
    [0, 0, 0, 0, 2, 2, 3, 3, 4, 3, 4, 5, 7, 8, 9, 4, 2, 0, 0];
/// `ammoData[].max` (`bg_weapons.c`), by `ammo_t`.
pub(crate) const AMMO_MAX: [i32; 10] = [0, 100, 300, 300, 300, 25, 800, 10, 10, 10];
/// Weapons whose `energyPerShot` and `altEnergyPerShot` are both zero: the
/// stun baton, melee, saber, the Bryar pistol, the emplaced gun and turret.
const fn infinite_ammo(weapon: u8) -> bool {
    matches!(weapon, 0..=4 | 17 | 18)
}

/// `WEAPON_FIRING` (`bg_public.h` `weaponstate_t`).
const WEAPON_FIRING: u8 = 3;

/// The player values the HUD reads, from the snapshot and prediction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Readout {
    pub(crate) health: i32,
    pub(crate) max_health: i32,
    pub(crate) armor: i32,
    pub(crate) force: i32,
    pub(crate) weapon: u8,
    /// `ps.ammo`, indexed by `ammo_t`.
    pub(crate) ammo: [i32; 10],
    /// `fd.saberDrawAnimLevel`.
    pub(crate) saber_style: u8,
    pub(crate) weapon_state: u8,
    pub(crate) weapon_time: i32,
    /// `EF_DOUBLE_AMMO` on the player's entity.
    pub(crate) double_ammo: bool,
    /// `PERS_SCORE`.
    pub(crate) score: i32,
    /// `g_gametype` is `GT_DUEL` / `GT_POWERDUEL`.
    pub(crate) duel: bool,
    pub(crate) power_duel: bool,
    pub(crate) fraglimit: i32,
    /// `cg.time`.
    pub(crate) time: i32,
}

/// The timers `cg` keeps across frames for the HUD.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Timers {
    /// `cg.oldammo` / `cg.oldAmmoTime`: ammo that just rose shows yellow.
    old_ammo: i32,
    old_ammo_time: i32,
    /// `cg.HUDArmorFlag` / `cg.HUDTickFlashTime`: the last armor tic blinks
    /// below a quarter of maximum armor.
    armor_flag: bool,
    tick_flash_time: i32,
}

/// One picture: the [`Piece`]'s rectangle and edge, with its draw colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Picture {
    pub(crate) rect: [f32; 4],
    pub(crate) side: Side,
    pub(crate) picture: u16,
    pub(crate) color: [f32; 4],
}

/// A string centred on, or starting at, `x`, its top at `y`.
#[derive(Debug, Default)]
pub(crate) struct TextRun {
    pub(crate) text: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// Line height in 640x480 units (point size times scale).
    pub(crate) size: f32,
    pub(crate) color: [f32; 4],
    pub(crate) centred: bool,
    pub(crate) side: Option<Side>,
}

/// One frame's pictures and text, reused frame to frame.
pub(crate) struct Frame {
    pub(crate) pictures: [Picture; MAX_PICTURES],
    pub(crate) picture_count: usize,
    pub(crate) texts: [TextRun; MAX_TEXTS],
    pub(crate) text_count: usize,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            pictures: [Picture {
                rect: [0.0; 4],
                side: Side::Left,
                picture: 0,
                color: WHITE,
            }; MAX_PICTURES],
            picture_count: 0,
            texts: std::array::from_fn(|_| TextRun {
                text: String::with_capacity(32),
                ..TextRun::default()
            }),
            text_count: 0,
        }
    }
}

/// `FONT_MEDIUM` (`ergoec`) and `FONT_SMALL` (`ocr_a`) point sizes.
const MEDIUM_POINTS: f32 = 20.0;
const SMALL_POINTS: f32 = 18.0;

impl Frame {
    pub(crate) fn clear(&mut self) {
        self.picture_count = 0;
        self.text_count = 0;
    }

    fn picture(&mut self, piece: &Piece, color: [f32; 4]) {
        let Some(picture) = piece.picture else {
            return;
        };
        self.picture_at(piece.rect, piece.side, picture, color);
    }

    fn picture_at(&mut self, rect: [f32; 4], side: Side, picture: u16, color: [f32; 4]) {
        if self.picture_count == MAX_PICTURES {
            return;
        }
        self.pictures[self.picture_count] = Picture {
            rect,
            side,
            picture,
            color,
        };
        self.picture_count += 1;
    }

    fn text(&mut self) -> Option<&mut TextRun> {
        let run = self.texts.get_mut(self.text_count)?;
        self.text_count += 1;
        run.text.clear();
        Some(run)
    }

    /// `CG_DrawNumField(x, y, 3, value, w, h, NUM_FONT_SMALL, qfalse)`:
    /// right-aligned in three cells of `w`, one unit between digits, drawn
    /// with the `gfx/2d/numbers/t_*` pictures from `digits` (0-9, minus).
    fn number(&mut self, piece: &Piece, value: i32, color: [f32; 4], digits: u16) {
        const WIDTH: usize = 3;
        let value = value.clamp(-99, 999);
        let mut buffer = [0_u8; 4];
        let length = format_into(&mut buffer, value).min(WIDTH);
        let [mut x, y, char_width, char_height] = piece.rect;
        x += 2.0 + char_width * (WIDTH - length) as f32;
        for &character in &buffer[..length] {
            let frame = if character == b'-' {
                10
            } else {
                u16::from(character - b'0')
            };
            self.picture_at(
                [x, y, char_width, char_height],
                piece.side,
                digits + frame,
                color,
            );
            x += 1.0 + char_width;
        }
    }

    /// The retail menu HUD (`cg_hudFiles` names a menu list).
    pub(crate) fn menu_hud(
        &mut self,
        layout: &Layout,
        readout: &Readout,
        timers: &mut Timers,
        digits: u16,
        score_label: &str,
    ) {
        self.clear();
        if let Some(left) = &layout.left {
            for piece in &left.painted {
                self.picture(piece, piece.color);
            }
            for piece in left.scanline.iter().chain(&left.frame) {
                self.picture(piece, WHITE);
            }
            self.armor(layout, readout, timers, digits);
            self.health(layout, readout, digits);
        }
        if let Some(right) = &layout.right {
            for piece in &right.painted {
                self.picture(piece, piece.color);
            }
            if !readout.power_duel
                && let Some(piece) = &layout.score_line
                && let Some(run) = self.text()
            {
                let _ = if readout.duel {
                    write!(
                        run.text,
                        "{score_label}: {}/{}",
                        readout.score, readout.fraglimit
                    )
                } else {
                    write!(run.text, "{score_label}: {}", readout.score)
                };
                *run = TextRun {
                    text: std::mem::take(&mut run.text),
                    x: piece.rect[0],
                    y: piece.rect[1],
                    size: MEDIUM_POINTS * 0.7,
                    color: piece.color,
                    centred: true,
                    side: Some(piece.side),
                };
            }
            for piece in right.scanline.iter().chain(&right.frame) {
                self.picture(piece, WHITE);
            }
            self.force(layout, readout, digits);
            if readout.weapon == 3 {
                self.saber_style(layout, readout);
            } else {
                self.ammo(layout, readout, timers, digits);
            }
        }
    }

    /// `CG_DrawHealth`.
    fn health(&mut self, layout: &Layout, readout: &Readout, digits: u16) {
        let amount = readout.health.min(readout.max_health);
        let inc = readout.max_health / TICS as i32;
        let mut current = amount;
        for piece in layout.health_tics.iter().rev() {
            let Some(piece) = piece else {
                continue;
            };
            let mut color = WHITE;
            if current <= 0 {
                break;
            } else if current < inc {
                color[3] *= current as f32 / inc as f32;
            }
            self.picture(piece, color);
            current -= inc;
        }
        if let Some(piece) = &layout.health_amount {
            self.number(piece, readout.health, piece.color, digits);
        }
    }

    /// `CG_DrawArmor`, including the blink of the last tic at low armor.
    fn armor(&mut self, layout: &Layout, readout: &Readout, timers: &mut Timers, digits: u16) {
        let inc = readout.max_health / TICS as i32;
        let mut current = readout.armor;
        for (index, piece) in layout.armor_tics.iter().enumerate().rev() {
            let Some(piece) = piece else {
                continue;
            };
            let mut color = WHITE;
            if current <= 0 {
                break;
            } else if current < inc {
                color[3] *= current as f32 / inc as f32;
            }
            if index != TICS - 1 || current >= inc || timers.armor_flag {
                self.picture(piece, color);
            }
            current -= inc;
        }
        if let Some(piece) = &layout.armor_amount {
            self.number(piece, readout.armor, piece.color, digits);
        }
        if readout.armor != 0 {
            if (readout.armor as f32) < readout.max_health as f32 / 4.0 {
                if timers.tick_flash_time < readout.time {
                    timers.tick_flash_time = readout.time + 400;
                    timers.armor_flag = !timers.armor_flag;
                }
            } else {
                timers.armor_flag = true;
            }
        } else {
            timers.armor_flag = false;
        }
    }

    /// `CG_DrawSaberStyle`. Stock draws fast, medium or strong; a HUD that
    /// provides the Desann, Tavion, dual or staff picture gets it instead,
    /// as EternalJK draws them.
    fn saber_style(&mut self, layout: &Layout, readout: &Readout) {
        let style = usize::from(readout.saber_style);
        let specific = (4..=7)
            .contains(&style)
            .then(|| layout.saber_styles[style].as_ref())
            .flatten();
        let stock = match style {
            1 | 5 => layout.saber_styles[1].as_ref(),
            2 | 6 | 7 => layout.saber_styles[2].as_ref(),
            3 | 4 => layout.saber_styles[3].as_ref(),
            _ => None,
        };
        if let Some(piece) = specific.or(stock) {
            self.picture(piece, WHITE);
        }
    }

    /// `CG_DrawAmmo`.
    fn ammo(&mut self, layout: &Layout, readout: &Readout, timers: &mut Timers, digits: u16) {
        let weapon = readout.weapon;
        if weapon == 0 {
            return;
        }
        let index = AMMO_INDEX.get(usize::from(weapon)).copied().unwrap_or(0);
        let ammo = readout.ammo[index];
        if ammo < 0 {
            return;
        }
        if timers.old_ammo < ammo {
            timers.old_ammo_time = readout.time + 200;
        }
        timers.old_ammo = ammo;
        let (mut value, inc) = if infinite_ammo(weapon) {
            if let Some(piece) = &layout.ammo_infinite
                && let Some(run) = self.text()
            {
                run.text.push_str("--");
                // NUM_FONT_SMALL passed as a UI_* style is UI_RIGHT, which
                // CG_DrawProportionalString treats as centring.
                *run = TextRun {
                    text: std::mem::take(&mut run.text),
                    x: piece.rect[0],
                    y: piece.rect[1],
                    size: MEDIUM_POINTS,
                    color: piece.color,
                    centred: true,
                    side: Some(piece.side),
                };
            }
            (8.0, (8 / TICS) as f32)
        } else {
            let firing = readout.weapon_state == WEAPON_FIRING && readout.weapon_time > 100;
            let color = |piece: Option<&Piece>| {
                if firing {
                    LIGHT_GREY
                } else if ammo > 0 {
                    if timers.old_ammo_time > readout.time {
                        YELLOW
                    } else {
                        piece.map_or(WHITE, |piece| piece.color)
                    }
                } else {
                    RED
                }
            };
            match &layout.ammo_amount {
                Some(piece) => {
                    let maximum =
                        AMMO_MAX[index] as f32 * if readout.double_ammo { 2.0 } else { 1.0 };
                    self.number(piece, ammo, color(Some(piece)), digits);
                    (ammo as f32, maximum / TICS as f32)
                }
                None => (ammo as f32, 0.0),
            }
        };
        for piece in layout.ammo_tics.iter().rev() {
            let Some(piece) = piece else {
                continue;
            };
            let mut color = WHITE;
            if value <= 0.0 {
                break;
            } else if value < inc {
                color[3] = value / inc;
            }
            self.picture(piece, color);
            value -= inc;
        }
    }

    /// `CG_DrawForcePower`. The out-of-Force flash is not driven: SJK does
    /// not track `cg.forceHUDTotalFlashTime`.
    fn force(&mut self, layout: &Layout, readout: &Readout, digits: u16) {
        let inc = 100.0 / TICS as f32;
        let mut value = readout.force as f32;
        for piece in layout.force_tics.iter().rev() {
            let Some(piece) = piece else {
                continue;
            };
            let mut color = WHITE;
            if value <= 0.0 {
                break;
            } else if value < inc {
                color[3] = value / inc;
            }
            self.picture(piece, color);
            value -= inc;
        }
        if let Some(piece) = &layout.force_amount {
            self.number(piece, readout.force, piece.color, digits);
        }
    }

    /// The text HUD of a nonzero `cg_hudFiles` (`CG_DrawHUD`'s first branch).
    pub(crate) fn simple_hud(&mut self, readout: &Readout, timers: &mut Timers) {
        self.clear();
        let y = 480.0 - 80.0;
        self.simple_text(16.0, y + 40.0, HUD_RED, |text| {
            let _ = write!(text, "{}", readout.health);
        });
        self.simple_text(18.0 + 14.0, y + 54.0, HUD_GREEN, |text| {
            let _ = write!(text, "{}", readout.armor);
        });
        self.simple_text(640.0 - (18.0 + 14.0 + 32.0), y + 54.0, ICON_BLUE, |text| {
            let _ = write!(text, "{}", readout.force);
        });
        if readout.weapon == 3 {
            let (name, color, offset) = match readout.saber_style {
                2 => ("MEDIUM", YELLOW, 16.0),
                3 => ("STRONG", HUD_RED, 16.0),
                4 => ("DESANN", HUD_RED, 16.0),
                5 => ("TAVION", ICON_BLUE, 16.0),
                6 => ("AKIMBO", HUD_ORANGE, 16.0),
                7 => ("STAFF", HUD_ORANGE, 16.0),
                _ => ("FAST", ICON_BLUE, 0.0),
            };
            self.simple_text(640.0 - (offset + 48.0), y + 40.0, color, |text| {
                text.push_str(name);
            });
        } else if readout.weapon != 0 {
            let index = AMMO_INDEX
                .get(usize::from(readout.weapon))
                .copied()
                .unwrap_or(0);
            let ammo = readout.ammo[index];
            if ammo < 0 || infinite_ammo(readout.weapon) {
                self.simple_text(640.0 - 48.0, y + 40.0, HUD_ORANGE, |text| {
                    text.push_str("--");
                });
                return;
            }
            if timers.old_ammo < ammo {
                timers.old_ammo_time = readout.time + 200;
            }
            timers.old_ammo = ammo;
            let color = if readout.weapon_state == WEAPON_FIRING && readout.weapon_time > 100 {
                LIGHT_GREY
            } else if ammo > 0 {
                if timers.old_ammo_time > readout.time {
                    YELLOW
                } else {
                    HUD_ORANGE
                }
            } else {
                RED
            };
            self.simple_text(640.0 - 48.0, y + 40.0, color, |text| {
                let _ = write!(text, "{ammo}");
            });
        }
    }

    /// `CG_DrawProportionalString(x, y, ..., UI_SMALLFONT|UI_DROPSHADOW)`.
    fn simple_text(&mut self, x: f32, y: f32, color: [f32; 4], fill: impl FnOnce(&mut String)) {
        if let Some(run) = self.text() {
            fill(&mut run.text);
            run.x = x;
            run.y = y;
            run.size = SMALL_POINTS;
            run.color = color;
            run.centred = false;
            run.side = Some(if x > 320.0 { Side::Right } else { Side::Left });
        }
    }
}

/// Decimal digits of `value` (-99..=999) into `buffer`; returns the length.
fn format_into(buffer: &mut [u8; 4], value: i32) -> usize {
    let mut length = 0;
    let mut magnitude = value.unsigned_abs();
    let mut reversed = [0_u8; 4];
    loop {
        reversed[length] = b'0' + (magnitude % 10) as u8;
        length += 1;
        magnitude /= 10;
        if magnitude == 0 || length == 3 {
            break;
        }
    }
    let mut out = 0;
    if value < 0 {
        buffer[0] = b'-';
        out = 1;
    }
    for index in (0..length).rev() {
        buffer[out] = reversed[index];
        out += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;

    const DIGITS: u16 = 100;

    fn layout() -> Layout {
        let source = r#"
menuDef { name lefthud rect 0 368 112 112
  itemDef { name frame background "f" rect 0 0 112 112 }
  itemDef { name health_tic1 background "h1" rect 0 0 1 1 }
  itemDef { name health_tic2 background "h2" rect 0 0 1 1 }
  itemDef { name health_tic3 background "h3" rect 0 0 1 1 }
  itemDef { name health_tic4 background "h4" rect 0 0 1 1 }
  itemDef { name armor_tic4 background "a4" rect 0 0 1 1 }
  itemDef { name healthamount forecolor 1 0 0 1 rect 59 98 6 12 }
}
menuDef { name righthud rect 640 368 -112 112
  itemDef { name ammo_tic1 background "m1" rect 0 0 1 1 }
  itemDef { name ammo_tic2 background "m2" rect 0 0 1 1 }
  itemDef { name ammoamount forecolor 1 .5 0 1 rect -83 98 6 12 }
  itemDef { name ammoinfinite forecolor 1 .5 0 1 rect -75 87 6 12 }
  itemDef { name saberstyle_fast background "sf" rect 0 0 1 1 }
  itemDef { name saberstyle_medium background "sm" rect 0 0 1 1 }
  itemDef { name saberstyle_staff background "ss" rect 0 0 1 1 }
  itemDef { name score_line rect -150 92 6 12 }
}"#;
        Layout::resolve(&parse(source).menus)
    }

    fn readout() -> Readout {
        Readout {
            health: 100,
            max_health: 100,
            weapon: 3,
            saber_style: 1,
            time: 1_000,
            ..Readout::default()
        }
    }

    fn pictures(frame: &Frame, layout: &Layout) -> Vec<(String, f32)> {
        frame.pictures[..frame.picture_count]
            .iter()
            .map(|picture| {
                let name = layout
                    .pictures
                    .get(usize::from(picture.picture))
                    .cloned()
                    .unwrap_or_else(|| format!("#{}", picture.picture - DIGITS));
                (name, picture.color[3])
            })
            .collect()
    }

    #[test]
    fn health_tics_fade_the_partial_one_and_print_the_amount() {
        let layout = layout();
        let mut frame = Frame::default();
        let mut timers = Timers::default();
        let mut values = readout();
        values.health = 60; // inc 25: tics 4 and 3 full, 2 at 10/25, 1 gone.
        frame.menu_hud(&layout, &values, &mut timers, DIGITS, "Score");
        let drawn = pictures(&frame, &layout);
        assert_eq!(drawn[0].0, "f");
        assert_eq!(drawn[1], ("h4".to_owned(), 1.0));
        assert_eq!(drawn[2], ("h3".to_owned(), 1.0));
        assert_eq!(drawn[3], ("h2".to_owned(), 0.4));
        // Then "60" right-aligned in three 6-unit cells starting at x 59.
        assert_eq!(drawn[4].0, "#6");
        assert_eq!(drawn[5].0, "#0");
        let six = frame.pictures[4];
        assert_eq!(six.rect, [59.0 + 2.0 + 6.0, 466.0, 6.0, 12.0]);
        assert_eq!(frame.pictures[5].rect[0], 59.0 + 2.0 + 6.0 + 7.0);
        assert_eq!(six.color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn saber_shows_the_style_and_score_line() {
        let layout = layout();
        let mut frame = Frame::default();
        let mut timers = Timers::default();
        let mut values = readout();
        values.score = 7;
        values.saber_style = 7;
        frame.menu_hud(&layout, &values, &mut timers, DIGITS, "Score");
        let drawn = pictures(&frame, &layout);
        assert!(drawn.iter().any(|(name, _)| name == "ss"));
        assert!(!drawn.iter().any(|(name, _)| name == "sm"));
        assert_eq!(frame.text_count, 1);
        assert_eq!(frame.texts[0].text, "Score: 7");
        assert!(frame.texts[0].centred);
        assert_eq!(frame.texts[0].x, 490.0);
        values.saber_style = 6; // Dual: no dual picture, stock medium.
        values.duel = true;
        values.fraglimit = 3;
        frame.menu_hud(&layout, &values, &mut timers, DIGITS, "Score");
        assert!(
            pictures(&frame, &layout)
                .iter()
                .any(|(name, _)| name == "sm")
        );
        assert_eq!(frame.texts[0].text, "Score: 7/3");
    }

    #[test]
    fn ammo_tics_follow_the_weapon_maximum_and_flash_yellow_on_pickup() {
        let layout = layout();
        let mut frame = Frame::default();
        let mut timers = Timers::default();
        let mut values = readout();
        values.weapon = 5; // Blaster: AMMO_BLASTER (2), max 300, inc 75.
        values.ammo[2] = 100;
        frame.menu_hud(&layout, &values, &mut timers, DIGITS, "Score");
        let drawn = pictures(&frame, &layout);
        // Number first (yellow: ammo rose from 0 this frame), then tics 2, 1.
        let hundred = frame
            .pictures
            .iter()
            .take(frame.picture_count)
            .find(|picture| picture.picture == DIGITS + 1 && picture.side == Side::Right)
            .unwrap();
        assert_eq!(hundred.color, YELLOW);
        let tics: Vec<_> = drawn
            .iter()
            .filter(|(name, _)| name.starts_with('m'))
            .collect();
        assert_eq!(tics[0], &("m2".to_owned(), 1.0));
        assert!((tics[1].1 - 25.0 / 75.0).abs() < 1e-6);
        values.time += 300;
        frame.menu_hud(&layout, &values, &mut timers, DIGITS, "Score");
        let one = frame
            .pictures
            .iter()
            .take(frame.picture_count)
            .find(|picture| picture.picture == DIGITS + 1 && picture.side == Side::Right)
            .unwrap();
        assert_eq!(one.color, [1.0, 0.5, 0.0, 1.0]);
    }

    #[test]
    fn infinite_weapons_print_dashes() {
        let layout = layout();
        let mut frame = Frame::default();
        let mut values = readout();
        values.weapon = 4; // Bryar pistol: no energy per shot.
        frame.menu_hud(&layout, &values, &mut Timers::default(), DIGITS, "Score");
        assert_eq!(frame.texts[1].text, "--");
        assert_eq!(frame.texts[1].size, 20.0);
    }

    #[test]
    fn last_armor_tic_blinks_below_a_quarter() {
        let layout = layout();
        let mut frame = Frame::default();
        let mut timers = Timers::default();
        let mut values = readout();
        values.armor = 10; // Below 25: tic 4 is partial and blinks.
        let mut shown = Vec::new();
        for step in 0..4 {
            values.time = 1_000 + step * 401;
            frame.menu_hud(&layout, &values, &mut timers, DIGITS, "Score");
            shown.push(
                pictures(&frame, &layout)
                    .iter()
                    .any(|(name, _)| name == "a4"),
            );
        }
        assert_eq!(shown, [false, true, false, true]);
    }

    #[test]
    fn numbers_clamp_and_sign() {
        let digits = |value| {
            let mut buffer = [0; 4];
            let length = format_into(&mut buffer, value);
            buffer[..length].to_vec()
        };
        assert_eq!(digits(0), b"0");
        assert_eq!(digits(-42), b"-42");
        assert_eq!(digits(999), b"999");
    }

    #[test]
    fn simple_hud_prints_the_retail_strings() {
        let mut frame = Frame::default();
        let mut values = readout();
        values.armor = 25;
        values.force = 80;
        frame.simple_hud(&values, &mut Timers::default());
        let texts: Vec<_> = frame.texts[..frame.text_count]
            .iter()
            .map(|run| (run.text.as_str(), run.x, run.y))
            .collect();
        assert_eq!(
            texts,
            [
                ("100", 16.0, 440.0),
                ("25", 32.0, 454.0),
                ("80", 576.0, 454.0),
                ("FAST", 592.0, 440.0)
            ]
        );
    }
}
