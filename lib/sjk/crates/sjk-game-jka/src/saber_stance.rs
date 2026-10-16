//! The stance a player's sabers allow: what `ClientThink_real` keeps true every think
//! (`g_active.c:1914-1993`), the style cycle's key (`Cmd_SaberAttackCycle_f`,
//! `g_cmds.c:2757-2975`) with its pair and staff branches, and the style `G_SetSaber`
//! settles after a saber is set (`g_cmds.c:1347-1352`).
//!
//! `fd.saberAnimLevelBase` is off the wire; it lives in the movement state, and a
//! caller passes it in and writes it back.

use crate::generic_commands::SaberSound;
use crate::saber_definition::{
    SS_DUAL, SS_FAST, SS_NONE, SaberDefinition, style_valid, use_first_valid_style,
};
use sjk_protocol::PlayerState;

const PS_WEAPON_TIME: usize = 10;
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_DRAW_ANIM_LEVEL: usize = 25;
const PS_PM_FLAGS: usize = 38;
const PS_WEAPON: usize = 47;
const PS_SABER_HOLSTERED: usize = 81;
const PS_SABER_IN_FLIGHT: usize = 88;
const PMF_FOLLOW: u32 = 4_096;
const WP_SABER: u32 = 3;
/// `SS_STAFF`.
const SS_STAFF: i32 = 7;
/// `SFL2_NO_MANUAL_DEACTIVATE`, `SFL2_NO_MANUAL_DEACTIVATE2`.
const SFL2_NO_MANUAL_DEACTIVATE: u32 = 1 << 7;
const SFL2_NO_MANUAL_DEACTIVATE2: u32 = 1 << 16;

fn field(state: &PlayerState, index: usize) -> u32 {
    state.raw_field(index).unwrap_or(0)
}

/// Sets the style, and the drawn style with it.
fn set_level_and_draw(state: &mut PlayerState, level: i32) {
    state.set_raw_field(PS_SABER_ANIM_LEVEL, level as u32);
    state.set_raw_field(PS_SABER_DRAW_ANIM_LEVEL, level as u32);
}

/// `WP_SaberCanTurnOffSomeBlades`: unless every blade is always on.
pub fn can_turn_off_some_blades(saber: &SaberDefinition) -> bool {
    let first = saber.flags2 & SFL2_NO_MANUAL_DEACTIVATE != 0;
    if saber.blade_style2_start > 0 && saber.num_blades > saber.blade_style2_start {
        !(first && saber.flags2 & SFL2_NO_MANUAL_DEACTIVATE2 != 0)
    } else {
        !first
    }
}

/// Whether the blades may not be switched off by hand (`SFL2_NO_MANUAL_DEACTIVATE`, or
/// its second-style flag where the saber has a second style).
fn always_on(saber: &SaberDefinition) -> bool {
    saber.flags2 & SFL2_NO_MANUAL_DEACTIVATE != 0
        || (saber.blade_style2_start > 0 && saber.flags2 & SFL2_NO_MANUAL_DEACTIVATE2 != 0)
}

/// `ClientThink_real`'s stance upkeep outside a siege class's stances: two sabers use the
/// dual style (the fast one with the second off, any style they both allow otherwise); a
/// saber that teaches only the staff style always uses it, and a staff with one blade off
/// its single-blade style. Followers are left alone.
pub fn upkeep(state: &mut PlayerState, sabers: &[SaberDefinition; 2], base: &mut u8) {
    if field(state, PS_PM_FLAGS) & PMF_FOLLOW != 0 {
        return;
    }
    let holstered = field(state, PS_SABER_HOLSTERED) as i32;
    if sabers[0].is_held() && sabers[1].is_held() {
        if holstered == 1 {
            *base = SS_DUAL as u8;
            set_level_and_draw(state, SS_FAST);
        } else {
            if !style_valid(sabers, holstered, field(state, PS_SABER_ANIM_LEVEL) as i32) {
                *base = SS_DUAL as u8;
                state.set_raw_field(PS_SABER_ANIM_LEVEL, SS_DUAL as u32);
            }
            let level = field(state, PS_SABER_ANIM_LEVEL);
            state.set_raw_field(PS_SABER_DRAW_ANIM_LEVEL, level);
        }
        return;
    }
    if sabers[0].styles_learned == 1 << SS_STAFF {
        *base = SS_STAFF as u8;
    }
    if i32::from(*base) == SS_STAFF {
        if holstered == 1 && sabers[0].single_blade_style != SS_NONE {
            set_level_and_draw(state, sabers[0].single_blade_style);
        } else {
            set_level_and_draw(state, SS_STAFF);
        }
    }
}

/// `Cmd_SaberAttackCycle_f` outside a siege class's stances, for a living player on the
/// map (the caller refuses the dead, spectators and the intermission). A pair switches
/// its second saber off and on; a staff its second blade; otherwise the style goes up
/// to the saber offense level and round, to the first one the sabers allow, set now or
/// queued (`saberCycleQueue`) behind a swing, with `base` following.
pub fn attack_cycle(
    state: &mut PlayerState,
    sabers: &[SaberDefinition; 2],
    offense: u8,
    queue: &mut u32,
    base: &mut u8,
    sounds: &mut Vec<SaberSound>,
) {
    if field(state, PS_WEAPON) != WP_SABER {
        return;
    }
    let holstered = field(state, PS_SABER_HOLSTERED);
    let idle = field(state, PS_WEAPON_TIME) as i32 <= 0;
    if sabers[0].is_held() && sabers[1].is_held() {
        if can_turn_off_some_blades(&sabers[1]) {
            if holstered == 1 {
                sounds.push(SaberSound { hand: 1, on: true });
                state.set_raw_field(PS_SABER_HOLSTERED, 0);
                state.set_raw_field(PS_SABER_ANIM_LEVEL, SS_DUAL as u32);
            } else if holstered == 0 && !always_on(&sabers[1]) {
                sounds.push(SaberSound { hand: 1, on: false });
                state.set_raw_field(PS_SABER_HOLSTERED, 1);
                state.set_raw_field(PS_SABER_ANIM_LEVEL, SS_FAST as u32);
            }
            return;
        }
    } else if sabers[0].num_blades > 1 && can_turn_off_some_blades(&sabers[0]) {
        if holstered == 1 {
            if field(state, PS_SABER_IN_FLIGHT) != 0 {
                return;
            }
            sounds.push(SaberSound { hand: 0, on: true });
            state.set_raw_field(PS_SABER_HOLSTERED, 0);
            if sabers[0].styles_forbidden != 0 {
                // The style is looked for from none at all.
                let mut select = 0;
                use_first_valid_style(sabers, 0, &mut select);
                if idle {
                    state.set_raw_field(PS_SABER_ANIM_LEVEL, select as u32);
                } else {
                    *queue = select as u32;
                }
            }
        } else if holstered == 0 && !always_on(&sabers[0]) {
            sounds.push(SaberSound { hand: 0, on: false });
            state.set_raw_field(PS_SABER_HOLSTERED, 1);
            if sabers[0].single_blade_style != SS_NONE {
                if idle {
                    state.set_raw_field(PS_SABER_ANIM_LEVEL, sabers[0].single_blade_style as u32);
                } else {
                    *queue = sabers[0].single_blade_style as u32;
                }
            }
        }
        return;
    }
    let mut select = if *queue != 0 {
        *queue as i32
    } else {
        field(state, PS_SABER_ANIM_LEVEL) as i32
    };
    select += 1;
    if select > i32::from(offense) {
        select = 1;
    }
    use_first_valid_style(sabers, holstered as i32, &mut select);
    *base = select as u8;
    if idle {
        state.set_raw_field(PS_SABER_ANIM_LEVEL, select as u32);
    } else {
        *queue = select as u32;
    }
}

/// `G_SetSaber`'s end: a style the sabers now held forbid moves to the first they allow,
/// and `base` and the queued style follow it.
pub fn settle_after_set(
    state: &mut PlayerState,
    sabers: &[SaberDefinition; 2],
    queue: &mut u32,
    base: &mut u8,
) {
    let holstered = field(state, PS_SABER_HOLSTERED) as i32;
    let mut level = field(state, PS_SABER_ANIM_LEVEL) as i32;
    if !style_valid(sabers, holstered, level) {
        use_first_valid_style(sabers, holstered, &mut level);
        state.set_raw_field(PS_SABER_ANIM_LEVEL, level as u32);
        *base = level as u8;
        *queue = level as u32;
    }
}

/// [`upkeep`] for a player whose movement holds `saberAnimLevelBase`: the movement is
/// restarted from the state only where the upkeep changed something.
pub fn upkeep_player(
    state: &mut PlayerState,
    movement: &mut crate::pmove::Predictor,
    sabers: &[SaberDefinition; 2],
) {
    let mut base = movement.state().saber_anim_level_base;
    let before = (
        field(state, PS_SABER_ANIM_LEVEL),
        field(state, PS_SABER_DRAW_ANIM_LEVEL),
        base,
    );
    upkeep(state, sabers, &mut base);
    if before
        != (
            field(state, PS_SABER_ANIM_LEVEL),
            field(state, PS_SABER_DRAW_ANIM_LEVEL),
            base,
        )
    {
        *movement = movement.reseeded(state);
        movement.set_saber_anim_level_base(base);
    }
}
