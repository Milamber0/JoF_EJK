//! The Force wheel: the retail Force selection bar as JoF EternalJK draws it
//! (`CG_DrawForceSelect`, JoF EJK `cg_draw.c`), with JoF JA+'s Stasis, Repulse and
//! Dash ([`sjk_client::force_wheel`]).
//!
//! In the classic and game HUD styles the Force selector is this row of icons: the
//! selected entry large in the middle, up to three neighbours on each side in wheel
//! order, and its name under it in the small font, in `CT_ICON_BLUE`. SJK's radial
//! HUD keeps its own list ([`super::selection`]). Sizes and places are retail's
//! 640x480 ones, anchored to the bottom centre and scaled with `cg_hudScale`; icons
//! stay square on wide screens, as EternalJK's `widthRatioCoef` keeps them. Like
//! retail, the bar shows for `WEAPON_SELECT_TIME` without fading.

use super::icons::assets::decode;
use crate::player_menu::force_icons::POWER_ICONS;
use sjk_client::force_wheel;
use sjk_client::selection::SelectionView;
use sjk_shader::ShaderCatalog;
use sjk_ui::{Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextOverflow, TextureId};
use sjk_vfs::VirtualFileSystem;

/// Icon slots: one per wheel entry, then the flamethrower.
pub(crate) const ICONS: usize = force_wheel::MAX_SLOTS + 1;
const _: () = assert!(ICONS as u32 <= crate::ui_renderer::FORCE_WHEEL_ICON_CELLS);
/// The flamethrower's slot: JA+ merc mode's Force Lightning.
const FLAMETHROWER: usize = force_wheel::MAX_SLOTS;
/// `FP_LEVITATION`, whose icon Stasis borrows (it has none of its own).
const JUMP: usize = 1;
/// `FP_LIGHTNING`.
const LIGHTNING: u8 = 7;
/// JoF EJK's own pictures (`CG_RegisterGraphics`), from `jofclient-assets.pk3`,
const JOF_ICONS: [(usize, &str); 3] = [
    (force_wheel::REPULSE as usize, "gfx/jof/force_repulse"),
    (force_wheel::DASH as usize, "gfx/jof/force_dash"),
    (FLAMETHROWER, "gfx/jof/force_flamethrower"),
];
/// `colorTable[CT_ICON_BLUE]` (`q_color.c`).
const ICON_BLUE: Color = Color::new(0.567, 0.685, 1.0, 0.75);
/// `fonts/ocr_a.fontdat` line height: the small font at scale 1.
const SMALL_FONT_HEIGHT: f32 = 21.0;

/// Upload the wheel's pictures once per installed world; absent ones draw nothing.
pub(crate) fn load(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    mut upload: impl FnMut(TextureId, &[u8]),
) -> [Option<TextureId>; ICONS] {
    std::array::from_fn(|slot| {
        let name = POWER_ICONS.get(slot).copied().or_else(|| {
            JOF_ICONS
                .iter()
                .find(|(jof, _)| *jof == slot)
                .map(|(_, name)| *name)
        })?;
        let pixels = decode(vfs, shaders, name)?;
        let id = TextureId(crate::ui_renderer::FORCE_WHEEL_ICON_FIRST + slot as u32);
        upload(id, pixels.as_raw());
        Some(id)
    })
}

/// `CG_ForceSelectIcon`: the picture of wheel entry `slot` (the quick wheel's
/// Force page shows it too).
pub(crate) fn icon(
    icons: &[Option<TextureId>; ICONS],
    slot: u8,
    flamethrower: bool,
) -> Option<TextureId> {
    let index = match slot {
        force_wheel::STASIS => JUMP,
        LIGHTNING if flamethrower => FLAMETHROWER,
        _ => usize::from(slot),
    };
    icons.get(index).copied().flatten()
}

/// The name the bar shows under wheel entry `slot` (retail's, as Dark Rage and
/// Sense); `flamethrower` names Lightning the flamethrower.
pub(crate) fn name(slot: u8, flamethrower: bool) -> &'static str {
    super::selection::text(super::selection::name_id(slot, flamethrower))
}

/// Icons drawn left and right of the centre one, at most three each side.
fn side_counts(count: usize) -> (usize, usize) {
    const SIDE_MAX: usize = 3;
    let hold = count.saturating_sub(1);
    if hold == 0 {
        (0, 0)
    } else if count > 2 * SIDE_MAX {
        (SIDE_MAX, SIDE_MAX)
    } else {
        (hold / 2, hold - hold / 2)
    }
}

/// Where the bar's pictures go, in pixels, with the wheel slot each shows. As in
/// retail, only entries with a picture (`drawn`) take room on the sides.
fn placements(
    view: &SelectionView,
    viewport: [f32; 2],
    user_scale: f32,
    drawn: impl Fn(u8) -> bool,
) -> Option<([(u8, Rect); 7], usize)> {
    let (slots, count) = force_wheel::build(view.available);
    let current = slots[..count]
        .iter()
        .position(|&slot| slot == view.selected)?;
    let scale = viewport[1] / 480.0 * user_scale.clamp(0.25, 2.0);
    let (small, big, pad) = (30.0 * scale, 60.0 * scale, 12.0 * scale);
    let centre = viewport[0] / 2.0;
    let y = viewport[1] - (480.0 - 425.0) * scale;
    let (left, right) = side_counts(count);
    let mut out = [(0, Rect::new(0.0, 0.0, 0.0, 0.0)); 7];
    let mut placed = 0;
    let mut place = |slot: u8, rect: Rect| {
        out[placed] = (slot, rect);
        placed += 1;
    };
    let mut x = centre - (big / 2.0 + pad + small);
    let mut index = current;
    for _ in 0..left {
        index = (index + count - 1) % count;
        if drawn(slots[index]) {
            place(slots[index], Rect::new(x, y, small, small));
            x -= small + pad;
        }
    }
    place(
        slots[current],
        Rect::new(centre - big / 2.0, y - (big - small) / 2.0, big, big),
    );
    let mut x = centre + big / 2.0 + pad;
    let mut index = current;
    for _ in 0..right {
        index = (index + 1) % count;
        if drawn(slots[index]) {
            place(slots[index], Rect::new(x, y, small, small));
            x += small + pad;
        }
    }
    Some((out, placed))
}

/// Draw the bar for a Force selection `view`; `flamethrower` shows Force Lightning
/// as JA+ merc mode's flamethrower.
pub(super) fn emit(
    draw: &mut DrawList,
    view: SelectionView,
    icons: &[Option<TextureId>; ICONS],
    flamethrower: bool,
    viewport: [f32; 2],
    user_scale: f32,
) {
    let drawn = |slot| icon(icons, slot, flamethrower).is_some();
    let Some((placements, count)) = placements(&view, viewport, user_scale, drawn) else {
        return;
    };
    let white = Color::new(1.0, 1.0, 1.0, 1.0);
    for &(slot, rect) in &placements[..count] {
        if let Some(texture) = icon(icons, slot, flamethrower) {
            let _ = draw.push(DrawCommand::TexturedQuad {
                rect,
                texture,
                color: white,
            });
        }
    }
    let scale = viewport[1] / 480.0 * user_scale.clamp(0.25, 2.0);
    let y = viewport[1] - (480.0 - 425.0 - 30.0) * scale;
    let _ = draw.push(DrawCommand::Text {
        rect: Rect::new(
            viewport[0] / 2.0 - 160.0 * scale,
            y,
            320.0 * scale,
            SMALL_FONT_HEIGHT * scale,
        ),
        text: super::selection::name_id(view.selected, flamethrower),
        size: SMALL_FONT_HEIGHT * scale,
        color: ICON_BLUE,
        align: TextAlign::Center,
        overflow: TextOverflow::Clip,
        weight: FontWeight::Regular,
        letter_spacing: 0.0,
    });
}

/// The bar for `view` with the given pictures, for the menu snapshots
/// (`menu_snapshot`), and the names its text ids stand for.
#[cfg(test)]
pub(crate) fn snapshot(
    view: SelectionView,
    icons: &[Option<TextureId>; ICONS],
    flamethrower: bool,
    viewport: [f32; 2],
) -> (DrawList, fn(sjk_ui::TextId) -> &'static str) {
    let mut draw = DrawList::new(16);
    emit(&mut draw, view, icons, flamethrower, viewport, 1.0);
    (draw, super::selection::text)
}

/// Shader names of the wheel's pictures, by icon slot, for the menu snapshots.
#[cfg(test)]
pub(crate) fn picture_names() -> impl Iterator<Item = (usize, &'static str)> {
    POWER_ICONS
        .iter()
        .copied()
        .enumerate()
        .chain(JOF_ICONS.iter().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(available: u32, selected: u8) -> SelectionView {
        SelectionView {
            inventory: false,
            available,
            selected,
            alpha: 1.0,
        }
    }

    #[test]
    fn side_counts_follow_cg_draw_force_select() {
        assert_eq!(side_counts(1), (0, 0));
        assert_eq!(side_counts(2), (0, 1));
        assert_eq!(side_counts(5), (2, 2));
        assert_eq!(side_counts(6), (2, 3));
        assert_eq!(side_counts(7), (3, 3));
        assert_eq!(side_counts(17), (3, 3));
    }

    #[test]
    fn the_bar_sits_at_retail_places_on_a_480_line_screen() {
        // Heal, Push, Pull: Push selected in the middle.
        let (placed, count) = placements(
            &view(1 | (1 << 3) | (1 << 4), 3),
            [640.0, 480.0],
            1.0,
            |_| true,
        )
        .expect("selected power is on the wheel");
        assert_eq!(count, 3);
        let (centre_slot, centre) = placed[1];
        assert_eq!(centre_slot, 3);
        assert_eq!(
            [centre.x, centre.y, centre.width],
            [320.0 - 30.0, 425.0 - 15.0, 60.0]
        );
        assert_eq!(placed[0].0, 0);
        assert_eq!(
            [placed[0].1.x, placed[0].1.y],
            [320.0 - 30.0 - 12.0 - 30.0, 425.0]
        );
        assert_eq!(placed[2].0, 4);
        assert_eq!(placed[2].1.x, 320.0 + 30.0 + 12.0);
    }

    #[test]
    fn wide_screens_keep_square_icons_centred_and_hud_scale_shrinks_them() {
        let (placed, _) = placements(&view(1 << 3, 3), [1920.0, 1080.0], 0.5, |_| true).unwrap();
        let (_, centre) = placed[0];
        assert_eq!(centre.width, centre.height);
        assert_eq!(centre.x + centre.width / 2.0, 960.0);
        assert_eq!(centre.width, 60.0 * 1080.0 / 480.0 * 0.5);
    }

    #[test]
    fn a_side_power_without_a_picture_takes_no_room() {
        // Wheel Heal, Push, Pull, Grip with Push selected: Heal would be the one
        // left icon, but without its picture nothing is placed there.
        let available = 1 | (1 << 3) | (1 << 4) | (1 << 6);
        let (placed, count) =
            placements(&view(available, 3), [640.0, 480.0], 1.0, |slot| slot != 0).unwrap();
        let slots: Vec<u8> = placed[..count].iter().map(|(slot, _)| *slot).collect();
        assert_eq!(slots, [3, 4, 6]);
        assert_eq!([placed[1].1.x, placed[2].1.x], [362.0, 404.0]);
    }

    #[test]
    fn a_selection_off_the_wheel_draws_nothing() {
        assert!(placements(&view(1 << 3, 4), [640.0, 480.0], 1.0, |_| true).is_none());
    }

    #[test]
    fn the_bar_never_offers_illuminate_in_any_state() {
        // Every bit the server could send, slot 21 (once Illuminate's) included: the
        // wheel holds the 14 selectable powers and JoF's three, no more, and nothing
        // is named Illuminate.
        let (slots, count) = force_wheel::build(u32::MAX);
        assert_eq!(count, 14 + 3);
        for slot in &slots[..count] {
            assert_ne!(name(*slot, false), "Illuminate");
            assert_ne!(name(*slot, true), "Illuminate");
        }
        assert_eq!(name(21, false), "Flamethrower");
        assert!(placements(&view(1 << 21, 21), [640.0, 480.0], 1.0, |_| true).is_none());
        assert!(
            (0..32u8)
                .map(|slot| name(slot, false))
                .all(|name| name != "Illuminate")
        );
    }

    #[test]
    fn stasis_borrows_jump_and_merc_lightning_is_the_flamethrower() {
        let mut icons = [None; ICONS];
        icons[JUMP] = Some(TextureId(1));
        icons[usize::from(LIGHTNING)] = Some(TextureId(7));
        icons[FLAMETHROWER] = Some(TextureId(99));
        assert_eq!(icon(&icons, force_wheel::STASIS, false), Some(TextureId(1)));
        assert_eq!(icon(&icons, LIGHTNING, false), Some(TextureId(7)));
        assert_eq!(icon(&icons, LIGHTNING, true), Some(TextureId(99)));
        assert_eq!(icon(&icons, force_wheel::DASH, false), None);
    }
}
