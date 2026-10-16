//! The weapon selection row, `CG_DrawWeaponSelect` (`codemp/cgame/cg_weapons.c`) as
//! JoF EternalJK draws it: the selected weapon's icon, 80 units square, between up to
//! `sideMax` 40-unit icons on each side, 12 units apart, with the weapon's name in gold
//! `FONT_SMALL` over the bottom of the row, for `WEAPON_SELECT_TIME` after a change.
//!
//! The geometry is retail's 640x480 screen as SJK's game-data HUD maps it
//! (`menu_hud::gpu::to_pixels`): `cg_hudScale` units of height / 480 per unit,
//! measured from the bottom edge and here from the centre, so icons stay square on a
//! wide screen as EternalJK's `widthRatioCoef` keeps them. EternalJK also shows more
//! icons per side on wider screens; that choice is [`side_max`].
//!
//! Only the most recent of the weapon, Force and inventory selectors is drawn
//! (`CG_Draw2D`); SJK hides this row when a Force or inventory cycle follows it.

use crate::GpuState;
use sjk_client::{LegacyWeaponInventory, legacy_weapon_data, legacy_weapon_selectable};
use sjk_ui::{Color, DrawCommand, DrawList, Rect};
use sjk_vfs::VirtualFileSystem;
use std::time::Duration;

/// `WEAPON_SELECT_TIME` (`cg_local.h:50`): how long the row stays after a change.
pub(crate) const SHOW: Duration = Duration::from_millis(1_400);
/// Side and centre icon sizes and the gap between icons, in 480-line units.
const SMALL: f32 = 40.0;
const BIG: f32 = 80.0;
const PAD: f32 = 12.0;
/// The row's centre and its `y` (`x = 320; y = 410;`); icons sit at `y + 10`.
const X: f32 = 320.0;
const Y: f32 = 410.0;
/// The name's colour (`textColor` in `CG_DrawWeaponSelect`).
pub(crate) const NAME_COLOR: [f32; 4] = [0.875, 0.718, 0.121, 1.0];
/// `FONT_SMALL` (`ocr_a`) line height at scale 1: its `mHeight`.
const NAME_LINE: f32 = 21.0;
/// Where `CG_DrawProportionalString(320, y + 45, ...)` puts the name's baseline:
/// `RE_Font_DrawString` (`tr_font.cpp`) adds `mHeight - mDescender / 2` (21 - 2 for
/// `ocr_a`) to the given top.
const NAME_BASELINE: f32 = Y + 45.0 + NAME_LINE - 2.0;

const WP_SABER: u8 = 3;
const WP_FLECHETTE: u8 = 10;
const WP_ROCKET: u8 = 11;
const WP_THERMAL: u8 = 12;
const WP_TRIP_MINE: u8 = 13;
const WP_CONCUSSION: u8 = 15;
/// `WP_NUM_WEAPONS`.
const WEAPONS: u8 = 19;
/// `LAST_USEABLE_WEAPON` (`WP_BRYAR_OLD`): where the row wraps.
const WHEEL_MAX: u8 = 16;
/// `SS_DUAL` and `SS_STAFF` (`bg_public.h`), the saber styles with their own icon.
pub(crate) const SS_DUAL: u8 = 6;
pub(crate) const SS_STAFF: u8 = 7;
/// Most icons a side ever shows (`sideMax` on ultra-wide screens).
const SIDE_LIMIT: usize = 7;

/// The weapons drawn beside the selected one, nearest first.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Row {
    left: [u8; SIDE_LIMIT],
    left_len: usize,
    right: [u8; SIDE_LIMIT],
    right_len: usize,
}

impl Row {
    /// Weapons left of the selected one, nearest first.
    pub(crate) fn left(&self) -> &[u8] {
        &self.left[..self.left_len]
    }

    /// Weapons right of the selected one, nearest first.
    pub(crate) fn right(&self) -> &[u8] {
        &self.right[..self.right_len]
    }
}

/// EternalJK's icons per side (`sideMax`) for `viewport`: 3 on 4:3 and 16:10, 5 up
/// to 16:9 and 7 wider; one more on 4:3 to 16:9 with the text HUD (`cg_hudFiles 1`).
pub(crate) fn side_max(viewport: [f32; 2], hud_files: i64) -> usize {
    // cgs.widthRatioCoef: the 4:3 width over the screen's.
    let coef = 640.0 * viewport[1] / (480.0 * viewport[0].max(1.0));
    let text_hud = usize::from(hud_files == 1);
    if coef >= 0.8 {
        3 + text_hud
    } else if coef >= 0.625 {
        5 + text_hud
    } else {
        7
    }
}

fn owned(inventory: &LegacyWeaponInventory, weapon: u8) -> bool {
    weapon < 32 && inventory.owned & (1 << weapon) != 0
}

/// Thermal detonators and trip mines leave the row when none are left.
fn hidden_when_empty(inventory: &LegacyWeaponInventory, weapon: u8) -> bool {
    matches!(weapon, WP_THERMAL | WP_TRIP_MINE) && !legacy_weapon_selectable(inventory, weapon)
}

/// `CG_WeaponCheck`: whether the weapon has the ammo for either firing mode; the row
/// draws an empty one with its `_na` icon.
pub(crate) fn has_ammo(inventory: &LegacyWeaponInventory, weapon: u8) -> bool {
    legacy_weapon_data(weapon).is_none_or(|data| {
        let ammo = inventory.ammo.get(data.ammo_index).copied().unwrap_or(0);
        ammo >= data.primary_cost.max(0) as u32 || ammo >= data.alternate_cost.max(0) as u32
    })
}

/// The icons beside `selected`, walked as `CG_DrawWeaponSelect` walks them: backwards
/// on the left and forwards on the right, Concussion between Flechette and Rocket, and
/// thermal detonators and trip mines skipped when empty. `None` when nothing is owned.
pub(crate) fn row(inventory: &LegacyWeaponInventory, selected: u8, side_max: usize) -> Option<Row> {
    let side_max = side_max.min(SIDE_LIMIT);
    // A selected empty thermal or trip mine still shows, unhighlighted, until switched.
    let mut count = usize::from(hidden_when_empty(inventory, selected));
    count += (1..WEAPONS)
        .filter(|&weapon| owned(inventory, weapon) && !hidden_when_empty(inventory, weapon))
        .count();
    if count == 0 {
        return None;
    }
    let others = count - 1;
    let (left_count, right_count) = if others == 0 {
        (0, 0)
    } else if count > 2 * side_max {
        (side_max, side_max)
    } else {
        (others / 2, others - others / 2)
    };
    let mut row = Row::default();
    let mut drew_concussion = false;
    // Every walk visits each weapon at most twice before its side is full; the bound
    // only guards against an inventory the wheel cannot reach (emplaced, turret).
    let steps = 4 * usize::from(WEAPONS);

    let mut weapon = if selected == WP_CONCUSSION {
        WP_FLECHETTE
    } else {
        selected.wrapping_sub(1)
    };
    if !(1..=WEAPONS).contains(&weapon) {
        weapon = WHEEL_MAX;
    }
    for _ in 0..steps {
        if row.left_len == left_count {
            break;
        }
        if weapon == WP_CONCUSSION {
            weapon -= 1;
        } else if weapon == WP_FLECHETTE && !drew_concussion && selected != WP_CONCUSSION {
            weapon = WP_CONCUSSION;
        }
        if weapon < 1 {
            weapon = WHEEL_MAX;
        }
        let shown = owned(inventory, weapon) && !hidden_when_empty(inventory, weapon);
        if shown {
            row.left[row.left_len] = weapon;
            row.left_len += 1;
        }
        // Concussion is reached only through the Flechette swap; Rocket follows it.
        if weapon == WP_CONCUSSION {
            drew_concussion = true;
            weapon = WP_ROCKET;
        }
        weapon -= 1;
    }

    let mut weapon = if selected == WP_CONCUSSION {
        WP_ROCKET
    } else {
        selected + 1
    };
    if weapon > WHEEL_MAX {
        weapon = 1;
    }
    for _ in 0..steps {
        if row.right_len == right_count {
            break;
        }
        if weapon == WP_CONCUSSION {
            weapon += 1;
        } else if weapon == WP_ROCKET && !drew_concussion && selected != WP_CONCUSSION {
            weapon = WP_CONCUSSION;
        }
        if weapon > WHEEL_MAX {
            weapon = 1;
        }
        let shown = owned(inventory, weapon) && !hidden_when_empty(inventory, weapon);
        if shown {
            row.right[row.right_len] = weapon;
            row.right_len += 1;
        }
        if weapon == WP_CONCUSSION {
            drew_concussion = true;
            weapon = WP_FLECHETTE;
        }
        weapon += 1;
    }
    Some(row)
}

/// Retail's 640x480 screen in pixels: `unit` pixels per unit, from the bottom edge
/// and the centre.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Screen {
    viewport: [f32; 2],
    unit: f32,
}

impl Screen {
    fn new(viewport: [f32; 2], scale: f32) -> Self {
        Self {
            viewport,
            unit: viewport[1] / 480.0 * scale,
        }
    }

    fn x(self, x: f32) -> f32 {
        self.viewport[0] * 0.5 + (x - X) * self.unit
    }

    fn y(self, y: f32) -> f32 {
        self.viewport[1] - (480.0 - y) * self.unit
    }

    fn square(self, x: f32, y: f32, size: f32) -> Rect {
        Rect::new(self.x(x), self.y(y), size * self.unit, size * self.unit)
    }
}

/// Place the selected icon and its neighbours: `emit(weapon, rect)`.
pub(crate) fn place_icons(
    row: &Row,
    selected: u8,
    viewport: [f32; 2],
    scale: f32,
    mut emit: impl FnMut(u8, Rect),
) {
    let screen = Screen::new(viewport, scale);
    let small_y = Y + 10.0;
    for (index, &weapon) in row.left().iter().enumerate() {
        let x = X - (BIG / 2.0 + PAD + SMALL) - index as f32 * (SMALL + PAD);
        emit(weapon, screen.square(x, small_y, SMALL));
    }
    emit(
        selected,
        screen.square(X - BIG / 2.0, Y - (BIG - SMALL) / 2.0 + 10.0, BIG),
    );
    for (index, &weapon) in row.right().iter().enumerate() {
        let x = X + BIG / 2.0 + PAD + index as f32 * (SMALL + PAD);
        emit(weapon, screen.square(x, small_y, SMALL));
    }
}

/// Where the name goes: its centre `x`, its baseline `y` and its line height, in
/// pixels (`FONT_SMALL` at scale 1).
pub(crate) fn name_placement(viewport: [f32; 2], scale: f32) -> (f32, f32, f32) {
    let screen = Screen::new(viewport, scale);
    (
        viewport[0] * 0.5,
        screen.y(NAME_BASELINE),
        NAME_LINE * screen.unit,
    )
}

/// What the row shows this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shown {
    pub(crate) selected: u8,
    pub(crate) row: Row,
    pub(crate) inventory: LegacyWeaponInventory,
    /// `fd.saberDrawAnimLevel`, for the staff and dual saber icons.
    pub(crate) saber_style: u8,
    /// `cg_hudScale`.
    pub(crate) scale: f32,
}

/// Map-lifetime names and this frame's row.
pub(crate) struct State {
    /// Retail's `SP_INGAME_<item classname>` names, by weapon.
    names: [String; WEAPONS as usize],
    pub(crate) shown: Option<Shown>,
}

/// Item classnames by weapon (`bg_itemlist`, `bg_misc.c`).
const CLASSNAMES: [&str; WEAPONS as usize] = [
    "",
    "weapon_stun_baton",
    "weapon_melee",
    "weapon_saber",
    "weapon_blaster_pistol",
    "weapon_blaster",
    "weapon_disruptor",
    "weapon_bowcaster",
    "weapon_repeater",
    "weapon_demp2",
    "weapon_flechette",
    "weapon_rocket_launcher",
    "weapon_thermal",
    "weapon_trip_mine",
    "weapon_det_pack",
    "weapon_concussion_rifle",
    "weapon_bryar_pistol",
    "weapon_emplaced",
    "weapon_turretwp",
];

impl State {
    pub(crate) fn new() -> Self {
        Self {
            names: std::array::from_fn(|weapon| {
                crate::ingame_menu::weapon_name(weapon as u8).to_owned()
            }),
            shown: None,
        }
    }

    /// Read the names `CG_DrawWeaponSelect` shows (`SE_GetStringTextString` of
    /// `SP_INGAME_<CLASSNAME>`); a weapon the string table lacks keeps SJK's name.
    pub(crate) fn load(vfs: &VirtualFileSystem) -> Self {
        let strings =
            sjk_client::string_table::load_referenced(vfs, &["strings/english/sp_ingame.str"]);
        let mut state = Self::new();
        for (name, classname) in state.names.iter_mut().zip(CLASSNAMES).skip(1) {
            let key = format!("SP_INGAME_{}", classname.to_ascii_uppercase());
            if let Some(text) = strings.get(&key).filter(|text| !text.is_empty()) {
                name.clone_from(text);
            }
        }
        state
    }

    /// The name of `weapon`.
    pub(crate) fn name(&self, weapon: u8) -> &str {
        self.names
            .get(usize::from(weapon))
            .map_or("", String::as_str)
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

/// Draw the row's icons into the HUD's list.
pub(crate) fn emit_icons(
    list: &mut DrawList,
    icons: &crate::hud::icons::Icons,
    shown: &Shown,
    viewport: [f32; 2],
) {
    let white = Color::new(1.0, 1.0, 1.0, 1.0);
    place_icons(
        &shown.row,
        shown.selected,
        viewport,
        shown.scale,
        |weapon, rect| {
            let empty = !has_ammo(&shown.inventory, weapon);
            let style = if weapon == WP_SABER {
                shown.saber_style
            } else {
                0
            };
            if let Some(texture) = icons.weapon_select(weapon, empty, style) {
                let _ = list.push(DrawCommand::TexturedQuad {
                    rect,
                    texture,
                    color: white,
                });
            }
        },
    );
}

impl GpuState {
    /// Append the selected weapon's name: `CG_DrawProportionalString(320, y + 45, name,
    /// UI_CENTER | UI_SMALLFONT, textColor)`, in `FONT_SMALL` when the game fonts are
    /// on and at its size in the bundled font otherwise.
    pub(crate) fn append_weapon_select_name(&mut self, viewport: [f32; 2]) {
        let Some(shown) = self.hud.weapon_select.shown else {
            return;
        };
        let (vertices, font) = self.game_fonts.target(
            crate::game_font::RetailFont::Small,
            &mut self.text_vertices,
            &self.ui_font,
        );
        let (x, baseline, line) = name_placement(viewport, shown.scale);
        let scale = crate::ui_scale::glyph_scale(font, line, 1.0);
        let name = self.hud.weapon_select.name(shown.selected);
        let width = crate::text::visible_text_width(font, name, scale);
        // A capital's ink ends on the baseline, in either font.
        let ink = font.glyph(crate::text::TextFace::Regular, b'H');
        let top = baseline - (ink.offset_y + ink.height) * scale;
        crate::text::append_text_style(
            vertices,
            font,
            name,
            [x - width * 0.5, top],
            scale,
            viewport,
            crate::text::TextFace::Regular,
            NAME_COLOR,
            0.0,
        );
    }

    /// Sample the row for this frame (`CG_Draw2D`'s conditions): within
    /// `WEAPON_SELECT_TIME` of a change, alive, playing (not spectating or following),
    /// not on an emplaced gun, without the scoreboard held. Every HUD style shows it,
    /// as EternalJK does; SJK's own layouts hide their weapon name and ammo while it
    /// shows, so nothing sits behind its icons.
    pub(crate) fn sample_weapon_select(&self, intermission: bool) -> Option<Shown> {
        let recent = self
            .weapon_selected_at
            .is_some_and(|selected| selected.elapsed() < SHOW);
        let draw_2d = self
            .console
            .as_ref()
            .and_then(|c| c.bool_cvar("cg_draw2D"))
            .unwrap_or(true);
        if !recent
            || intermission
            || !draw_2d
            || self.gameplay_input.held(crate::input::GameButton::Scores)
        {
            return None;
        }
        let player = &self
            .live_session
            .as_ref()
            .map(sjk_client::ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(crate::demo_playback::Session::latest_snapshot)
            })?
            .player;
        let inventory = LegacyWeaponInventory::from_player_state(player);
        // PERS_TEAM 3 is TEAM_SPECTATOR.
        if player.health() <= 0
            || player.team() == 3
            || inventory.spectator
            || inventory.following
            || inventory.emplaced
        {
            return None;
        }
        let selected = self.selected_weapon.unwrap_or_else(|| player.weapon());
        let viewport = [
            self.configuration.width as f32,
            self.configuration.height as f32,
        ];
        let hud_files = self
            .console
            .as_ref()
            .and_then(|c| c.integer_cvar("cg_hudFiles"))
            .unwrap_or(0);
        let row = row(&inventory, selected, side_max(viewport, hud_files))?;
        Some(Shown {
            selected,
            row,
            inventory,
            saber_style: player.saber_draw_style(),
            scale: crate::runtime_settings::hud_scale(self.console.as_ref()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory(weapons: &[u8]) -> LegacyWeaponInventory {
        let mut inventory = LegacyWeaponInventory {
            owned: 0,
            ammo: [999; 16],
            detpack_planted: false,
            following: false,
            spectator: false,
            emplaced: false,
        };
        for &weapon in weapons {
            inventory.owned |= 1 << weapon;
        }
        inventory
    }

    #[test]
    fn neighbours_are_split_and_walked_outward() {
        // Saber, pistol, blaster, disruptor, bowcaster: blaster selected.
        let owned = inventory(&[3, 4, 5, 6, 7]);
        let row = row(&owned, 5, 5).unwrap();
        assert_eq!(row.left(), [4, 3]);
        assert_eq!(row.right(), [6, 7]);
    }

    #[test]
    fn the_row_wraps_and_stops_at_side_max() {
        let owned = inventory(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
        let row = row(&owned, 1, 3).unwrap();
        // 11 owned > 2 * 3: three each side, the left wrapping past the stun baton
        // to the rocket launcher; Concussion, not owned, is passed over.
        assert_eq!(row.left(), [11, 10, 9]);
        assert_eq!(row.right(), [2, 3, 4]);
    }

    #[test]
    fn concussion_sits_between_flechette_and_rocket() {
        let owned = inventory(&[10, 11, 15]);
        let rocket = row(&owned, 11, 5).unwrap();
        assert_eq!(rocket.left(), [15]);
        assert_eq!(rocket.right(), [10]);
        let concussion = row(&owned, 15, 5).unwrap();
        assert_eq!(concussion.left(), [10]);
        assert_eq!(concussion.right(), [11]);
        // From Flechette the left walk reaches Rocket first; Concussion then
        // follows Flechette on the right, as cg_weapons.c's loops do.
        let flechette = row(&owned, 10, 5).unwrap();
        assert_eq!(flechette.left(), [11]);
        assert_eq!(flechette.right(), [15]);
    }

    #[test]
    fn empty_thermals_and_mines_leave_the_row() {
        let mut owned = inventory(&[3, 5, 12, 13]);
        // AMMO_THERMAL (7) and AMMO_TRIPMINE (8) empty.
        owned.ammo[7] = 0;
        owned.ammo[8] = 0;
        // Two weapons left: none on the left, the saber on the right (past both).
        let row = row(&owned, 5, 5).unwrap();
        assert!(row.left().is_empty());
        assert_eq!(row.right(), [3]);
        assert!(super::row(&inventory(&[]), 0, 5).is_none());
    }

    #[test]
    fn empty_weapons_use_their_na_icon() {
        let mut owned = inventory(&[5]);
        assert!(has_ammo(&owned, 5));
        owned.ammo[2] = 0; // AMMO_BLASTER
        assert!(!has_ammo(&owned, 5));
        assert!(has_ammo(&owned, 3), "the saber needs no ammo");
    }

    #[test]
    fn wider_screens_show_more_icons() {
        assert_eq!(side_max([1024.0, 768.0], 0), 3);
        assert_eq!(side_max([1680.0, 1050.0], 0), 3);
        assert_eq!(side_max([1920.0, 1080.0], 0), 5);
        assert_eq!(side_max([3840.0, 2160.0], 0), 5);
        assert_eq!(side_max([2560.0, 1080.0], 0), 7);
        assert_eq!(side_max([1920.0, 1080.0], 1), 6);
        assert_eq!(side_max([1024.0, 768.0], 1), 4);
    }

    #[test]
    fn icons_follow_retail_units_from_the_bottom_centre() {
        let row = row(&inventory(&[3, 4, 5, 6, 7]), 5, 5).unwrap();
        let mut placed = Vec::new();
        place_icons(&row, 5, [1920.0, 1080.0], 1.0, |weapon, rect| {
            placed.push((weapon, rect))
        });
        // 2.25 pixels per unit at 1080 lines.
        assert_eq!(placed.len(), 5);
        assert_eq!(
            placed[0],
            (4, Rect::new(960.0 - 92.0 * 2.25, 945.0, 90.0, 90.0))
        );
        assert_eq!(placed[1].1.x, 960.0 - 144.0 * 2.25);
        assert_eq!(placed[2], (5, Rect::new(870.0, 900.0, 180.0, 180.0)));
        assert_eq!(
            placed[3].1,
            Rect::new(960.0 + 52.0 * 2.25, 945.0, 90.0, 90.0)
        );
        assert_eq!(placed[4].1.x, 960.0 + 104.0 * 2.25);
    }

    #[test]
    fn a_4k_screen_doubles_1080p_and_the_hud_scale_grows_from_the_bottom() {
        let row = row(&inventory(&[3, 5]), 5, 5).unwrap();
        let mut big = None;
        place_icons(&row, 5, [3840.0, 2160.0], 1.0, |weapon, rect| {
            if weapon == 5 {
                big = Some(rect);
            }
        });
        assert_eq!(big, Some(Rect::new(1740.0, 1800.0, 360.0, 360.0)));
        let mut scaled = None;
        place_icons(&row, 5, [1920.0, 1080.0], 1.5, |weapon, rect| {
            if weapon == 5 {
                scaled = Some(rect);
            }
        });
        let scaled = scaled.unwrap();
        assert_eq!(scaled.bottom(), 1080.0);
        assert_eq!(scaled.width, 270.0);
    }

    #[test]
    fn the_name_baseline_is_six_units_above_the_bottom() {
        let (x, baseline, line) = name_placement([1920.0, 1080.0], 1.0);
        assert_eq!(x, 960.0);
        assert_eq!(baseline, 1080.0 - 6.0 * 2.25);
        assert_eq!(line, 21.0 * 2.25);
    }
}
