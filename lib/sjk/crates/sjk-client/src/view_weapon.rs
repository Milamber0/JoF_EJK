//! BaseJKA first-person view weapon policy.
//!
//! Mirrors `CG_AddViewWeapon` (`codemp/cgame/cg_weapons.c:782-905`): the
//! `_hand.md3` rig carries the animated `tag_weapon`/`tag_barrel*` tags, the
//! item `view_model` hangs off `tag_weapon`, and the torso animation picks
//! the hand frame through `CG_MapTorsoToWeaponFrame` (`:157-216`). The gun
//! placement follows `CG_CalculateWeaponPosition` (`:225-270`) with the
//! default cvars `cg_weaponBob 1` and `cg_gunX/Y/Z 0`. SJK's `cg_fovViewmodel`
//! defaults to EternalJK's 80 (retail has no such cvar, which behaves as 0), so
//! the first-person view differs from retail unless it is set to 0.
//! Model paths are `bg_itemlist` `view_model` entries
//! (`codemp/game/bg_misc.c:1092-1456`); the `_hand` / `_barrel` derivations
//! come from `CG_RegisterWeapon` (`cg_weaponinit.c:97-126`).

use crate::LegacyViewBobSample;
use sjk_model::AnimationConfig;
use sjk_protocol::Snapshot;

const WP_STUN_BATON: u8 = 1; // codemp/game/bg_weapons.h:33
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_EMPLACED_GUN: u8 = 17;
const PM_SPECTATOR: u8 = 4; // codemp/game/bg_public.h:430
const TEAM_SPECTATOR: u8 = 3; // codemp/game/bg_public.h:1073
/// `cg.xyspeed` is clamped at 270 (`cg_view.c:1548-1551`).

/// Models `CG_RegisterWeapon` loads for one first-person weapon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyViewModel {
    /// `bg_itemlist[].view_model`; drawn on the hand rig's `tag_weapon`.
    pub gun: &'static str,
    /// Tag-only animation rig (`<view_model>_hand.md3`, 15 frames).
    pub hand: &'static str,
    /// `(model, hand tag)` barrels drawn with zero local angles.
    pub barrels: &'static [(&'static str, &'static str)],
}

const fn model(
    gun: &'static str,
    hand: &'static str,
    barrels: &'static [(&'static str, &'static str)],
) -> LegacyViewModel {
    LegacyViewModel { gun, hand, barrels }
}

const BATON_BARRELS: &[(&str, &str)] = &[
    ("models/weapons2/stun_baton/baton_barrel.md3", "tag_barrel"),
    (
        "models/weapons2/stun_baton/baton_barrel2.md3",
        "tag_barrel2",
    ),
    (
        "models/weapons2/stun_baton/baton_barrel3.md3",
        "tag_barrel3",
    ),
];

/// View models by `weapon_t`. The saber registers no hand model
/// (`cg_weaponinit.c:119-126`) and is drawn by the Ghoul2 player model
/// instead, so it yields `None` like the unarmed slots.
///
/// `WP_MELEE` registers no hand model either, though its item's view model is the
/// baton (`bg_misc.c` `weapon_melee`). `CG_AddViewWeapon` then hangs the baton on
/// handle 0, whose `R_LerpTag` is the identity, so it sits at the view origin; the
/// baton's geometry (x from -6.7 to -0.6) is all behind the eye, and nothing shows.
/// Drawing it on the stun baton's own rig showed a baton in first-person melee.
pub fn legacy_view_model(weapon: u8) -> Option<LegacyViewModel> {
    Some(match weapon {
        WP_STUN_BATON => model(
            "models/weapons2/stun_baton/baton.md3",
            "models/weapons2/stun_baton/baton_hand.md3",
            BATON_BARRELS,
        ),
        4 => model(
            "models/weapons2/blaster_pistol/blaster_pistol.md3",
            "models/weapons2/blaster_pistol/blaster_pistol_hand.md3",
            &[],
        ),
        5 | WP_EMPLACED_GUN | 18 => model(
            "models/weapons2/blaster_r/blaster.md3",
            "models/weapons2/blaster_r/blaster_hand.md3",
            &[],
        ),
        6 => model(
            "models/weapons2/disruptor/disruptor.md3",
            "models/weapons2/disruptor/disruptor_hand.md3",
            &[(
                "models/weapons2/disruptor/disruptor_barrel.md3",
                "tag_barrel",
            )],
        ),
        7 => model(
            "models/weapons2/bowcaster/bowcaster.md3",
            "models/weapons2/bowcaster/bowcaster_hand.md3",
            &[],
        ),
        8 => model(
            "models/weapons2/heavy_repeater/heavy_repeater.md3",
            "models/weapons2/heavy_repeater/heavy_repeater_hand.md3",
            &[(
                "models/weapons2/heavy_repeater/heavy_repeater_barrel.md3",
                "tag_barrel",
            )],
        ),
        9 => model(
            "models/weapons2/demp2/demp2.md3",
            "models/weapons2/demp2/demp2_hand.md3",
            &[],
        ),
        10 => model(
            "models/weapons2/golan_arms/golan_arms.md3",
            "models/weapons2/golan_arms/golan_arms_hand.md3",
            &[(
                "models/weapons2/golan_arms/golan_arms_barrel.md3",
                "tag_barrel",
            )],
        ),
        11 => model(
            "models/weapons2/merr_sonn/merr_sonn.md3",
            "models/weapons2/merr_sonn/merr_sonn_hand.md3",
            &[(
                "models/weapons2/merr_sonn/merr_sonn_barrel.md3",
                "tag_barrel",
            )],
        ),
        12 => model(
            "models/weapons2/thermal/thermal.md3",
            "models/weapons2/thermal/thermal_hand.md3",
            &[],
        ),
        13 => model(
            "models/weapons2/laser_trap/laser_trap.md3",
            "models/weapons2/laser_trap/laser_trap_hand.md3",
            &[],
        ),
        14 => model(
            "models/weapons2/detpack/det_pack.md3",
            "models/weapons2/detpack/det_pack_hand.md3",
            &[],
        ),
        15 => model(
            "models/weapons2/concussion/c_rifle.md3",
            "models/weapons2/concussion/c_rifle_hand.md3",
            &[(
                "models/weapons2/concussion/c_rifle_barrel.md3",
                "tag_barrel",
            )],
        ),
        16 => model(
            "models/weapons2/briar_pistol/briar_pistol.md3",
            "models/weapons2/briar_pistol/briar_pistol_hand.md3",
            &[],
        ),
        WP_MELEE => return None,
        _ => return None,
    })
}

/// Every weapon number that owns a view model, for load-time preloading.
pub fn legacy_view_weapons() -> impl Iterator<Item = (u8, LegacyViewModel)> {
    (1..=18).filter_map(|weapon| Some((weapon, legacy_view_model(weapon)?)))
}

/// `CG_AddViewWeapon` / `CG_AddPlayerWeapon` early returns
/// (`cg_weapons.c:796-813`, `:432-440`): spectators, intermission, third
/// person, scoped zoom, emplaced guns and the saber draw nothing.
pub fn legacy_view_weapon_visible(player: &Snapshot, third_person: bool) -> Option<u8> {
    legacy_view_weapon_visible_as(player, third_person, player.player.weapon())
}

/// Apply `CG_AddViewWeapon` visibility to a predicted weapon selection.
///
/// `cg.predictedPlayerState.weapon` is the value consumed by cgame after
/// `CG_PredictPlayerState`; the remaining visibility fields stay authoritative.
pub fn legacy_view_weapon_visible_as(
    player: &Snapshot,
    third_person: bool,
    weapon: u8,
) -> Option<u8> {
    let state = &player.player;
    let hidden = state.team() == TEAM_SPECTATOR
        || state.movement_type() == crate::intermission::PM_INTERMISSION
        || state.movement_type() == PM_SPECTATOR
        || third_person
        || state.zoom_mode() != 0
        || weapon == WP_EMPLACED_GUN
        || weapon == WP_SABER;
    (!hidden).then_some(weapon)
}

/// `CG_MapTorsoToWeaponFrame` (`cg_weapons.c:183-215`): hand frame for one
/// integral torso frame of the named humanoid sequence starting at
/// `first_frame`; `None` is the reference's `-1`.
pub fn map_torso_to_weapon_frame(sequence: &str, first_frame: u32, frame: i32) -> Option<u32> {
    let offset = u32::try_from(frame.checked_sub(i32::try_from(first_frame).ok()?)?).ok()?;
    match sequence {
        "TORSO_DROPWEAP1" if offset < 5 => Some(offset + 6),
        "TORSO_RAISEWEAP1" if offset < 4 => Some(offset + 6 + 4),
        "BOTH_ATTACK1" | "BOTH_ATTACK2" | "BOTH_ATTACK3" | "BOTH_ATTACK4" | "BOTH_ATTACK10"
        | "BOTH_THERMAL_THROW"
            if offset < 6 =>
        {
            Some(1 + offset)
        }
        _ => None,
    }
}

/// Hand rig frames chosen from the fractional torso frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyViewWeaponFrames {
    pub frame: u32,
    pub old_frame: u32,
    /// Weight of `old_frame`; `1 - back_lerp` weights `frame`.
    pub back_lerp: f32,
}

/// `cg_weapons.c:872-889`: `ceil`/`floor` of the `lower_lumbar` bone frame
/// map to `frame`/`oldframe`, and an unmapped frame collapses to the idle
/// frame (an unmapped `oldframe` alone snaps to `frame`).
pub fn legacy_view_weapon_frames(
    sequence: &str,
    first_frame: u32,
    torso_frame: f32,
) -> LegacyViewWeaponFrames {
    let floor = torso_frame.floor();
    let frame = map_torso_to_weapon_frame(sequence, first_frame, torso_frame.ceil() as i32);
    let old_frame = map_torso_to_weapon_frame(sequence, first_frame, floor as i32);
    match (frame, old_frame) {
        (Some(frame), Some(old_frame)) => LegacyViewWeaponFrames {
            frame,
            old_frame,
            back_lerp: 1.0 - (torso_frame - floor),
        },
        (Some(frame), None) => LegacyViewWeaponFrames {
            frame,
            old_frame: frame,
            back_lerp: 0.0,
        },
        (None, _) => IDLE_FRAMES,
    }
}

const IDLE_FRAMES: LegacyViewWeaponFrames = LegacyViewWeaponFrames {
    frame: 0,
    old_frame: 0,
    back_lerp: 0.0,
};

/// Sequences `CG_MapTorsoToWeaponFrame` recognises (`cg_weapons.c:183-208`).
const MAPPED_SEQUENCES: [&str; 8] = [
    "TORSO_DROPWEAP1",
    "TORSO_RAISEWEAP1",
    "BOTH_ATTACK1",
    "BOTH_ATTACK2",
    "BOTH_ATTACK3",
    "BOTH_ATTACK4",
    "BOTH_ATTACK10",
    "BOTH_THERMAL_THROW",
];

/// Load-time resolution of the mapped sequences against one humanoid
/// `animation.cfg`, so per-frame lookups neither hash nor allocate.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyViewWeaponAnimations {
    /// `(torso clip index, first frame, sequence name)` per mapped sequence.
    entries: Vec<(usize, u32, &'static str)>,
}

impl LegacyViewWeaponAnimations {
    /// Resolve the mapped sequences; ones the config lacks are skipped.
    pub fn new(config: &AnimationConfig) -> Self {
        let entries = MAPPED_SEQUENCES
            .into_iter()
            .filter_map(|name| {
                let clip = crate::legacy_animation::NAMES
                    .iter()
                    .position(|n| *n == name)?;
                let first_frame = u32::try_from(config.get(name)?.first_frame).ok()?;
                Some((clip, first_frame, name))
            })
            .collect();
        Self { entries }
    }

    /// Hand frames for the torso clip playing at fractional `torso_frame`.
    pub fn frames(&self, torso_clip: usize, torso_frame: f32) -> LegacyViewWeaponFrames {
        self.entries
            .iter()
            .find(|(clip, _, _)| *clip == torso_clip)
            .map_or(IDLE_FRAMES, |(_, first_frame, name)| {
                legacy_view_weapon_frames(name, *first_frame, torso_frame)
            })
    }
}

/// Hand rig placement in world space; angles are `[pitch, yaw, roll]` degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyViewWeaponPose {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

/// `CG_CalculateWeaponPosition` (`cg_weapons.c:225-270`) with the view bob
/// fields from `CG_CalcViewValues` (`cg_view.c:1543-1551`). `landing` is the
/// current [`crate::LegacyFirstPersonView::landing_offset`]; the weapon
/// follows a quarter of it (`cg_fallingBob`, `:242-250`).
pub fn legacy_view_weapon_pose(
    view_origin: [f32; 3],
    view_angles: [f32; 3],
    bob: LegacyViewBobSample,
    landing: f32,
    time_millis: i64,
) -> LegacyViewWeaponPose {
    let scale = if bob.odd_leg {
        -bob.xy_speed
    } else {
        bob.xy_speed
    };
    let [mut pitch, mut yaw, mut roll] = view_angles;
    roll += scale * bob.fraction_sin * 0.005;
    yaw += scale * bob.fraction_sin * 0.01;
    pitch += bob.xy_speed * bob.fraction_sin * 0.005;
    let drift = (bob.xy_speed + 40.0) * (time_millis as f32 * 0.001).sin() * 0.01;
    let mut origin = view_origin;
    origin[2] += landing * 0.25;
    LegacyViewWeaponPose {
        origin,
        angles: [pitch + drift, yaw + drift, roll + drift],
    }
}

#[cfg(test)]
mod melee_tests {
    use super::*;

    #[test]
    fn melee_draws_no_view_model_but_the_baton_does() {
        assert_eq!(legacy_view_model(WP_MELEE), None);
        assert!(legacy_view_weapons().all(|(weapon, _)| weapon != WP_MELEE));
        let baton = legacy_view_model(WP_STUN_BATON).unwrap();
        assert_eq!(baton.gun, "models/weapons2/stun_baton/baton.md3");
        assert_eq!(baton.barrels.len(), 3);
    }
}
