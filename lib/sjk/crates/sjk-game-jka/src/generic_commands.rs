//! The commands a client's keys send in a usercmd's `generic_cmd`
//! (`ClientThink_real`, `g_active.c:3109-3330`, after the move): the saber switched off
//! and on (`Cmd_ToggleSaber_f`), its style cycled (`Cmd_SaberAttackCycle_f`), the Force
//! powers' own keys, the holdables, the taunts (`G_SetTauntAnim`).
//!
//! The style cycle is [`crate::saber_stance::attack_cycle`]'s. Not yet: the holdables,
//! mind trick and the team powers' keys.

use crate::pmove_anim::AnimationLengths;
use sjk_protocol::{PlayerState, UserCommand};

/// `genCmds_t` (`q_shared.h:1390-1422`).
pub const GENCMD_SABERSWITCH: u8 = 1;
pub const GENCMD_ENGAGE_DUEL: u8 = 2;
pub const GENCMD_FORCE_HEAL: u8 = 3;
pub const GENCMD_FORCE_SPEED: u8 = 4;
pub const GENCMD_FORCE_THROW: u8 = 5;
pub const GENCMD_FORCE_PULL: u8 = 6;
pub const GENCMD_FORCE_DISTRACT: u8 = 7;
pub const GENCMD_FORCE_RAGE: u8 = 8;
pub const GENCMD_FORCE_PROTECT: u8 = 9;
pub const GENCMD_FORCE_ABSORB: u8 = 10;
pub const GENCMD_FORCE_HEALOTHER: u8 = 11;
pub const GENCMD_FORCE_FORCEPOWEROTHER: u8 = 12;
pub const GENCMD_FORCE_SEEING: u8 = 13;
pub const GENCMD_SABERATTACKCYCLE: u8 = 26;
pub const GENCMD_TAUNT: u8 = 27;
pub const GENCMD_BOW: u8 = 28;
pub const GENCMD_MEDITATE: u8 = 29;
pub const GENCMD_FLOURISH: u8 = 30;
pub const GENCMD_GLOAT: u8 = 31;

const PS_WEAPON_TIME: usize = 10;
const PS_GROUND_ENTITY: usize = 16;
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_ROCKET_LOCK_INDEX: usize = 24;
const PS_WEAPON: usize = 47;
const PS_EXTERNAL_EVENT: usize = 56;
const PS_EXTERNAL_EVENT_PARM: usize = 64;
const PS_ROCKET_TARGET_TIME: usize = 71;
const PS_ROCKET_LOCK_TIME: usize = 79;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_SABER_HOLSTERED: usize = 81;
/// `saberEntityNum`.
const PS_SABER_ENTITY: usize = 31;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_FORCE_DODGE_ANIM: usize = 89;
const PS_SABER_LOCK_TIME: usize = 107;
const PS_GRIP_CRIPPLE: usize = 111;
const PS_DUEL_TIME: usize = 118;
const WP_SABER: u32 = 3;
const HANDEXTEND_NONE: u32 = 0;
const HANDEXTEND_TAUNT: u32 = 10;
const ENTITY_NUMBER_NONE: u32 = 1_023;
const EV_TAUNT: u32 = 115;
const EVENT_BITS: u32 = 0x300;
const EVENT_BIT1: u32 = 0x100;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;

/// `lastGenCmd`, `lastGenCmdTime`: what the last command taken was, and until when the
/// same one is not taken again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GenericMemory {
    pub last: u8,
    pub last_time: i32,
}

/// Whether the command is taken this think: any but the last, or the last again once 300
/// ms have passed since it was taken (push and pull at once).
pub fn take(memory: &mut GenericMemory, command: u8, level_time: i32) -> bool {
    if command == 0 || (command == memory.last && memory.last_time >= level_time) {
        return false;
    }
    memory.last = command;
    if command != GENCMD_FORCE_THROW && command != GENCMD_FORCE_PULL {
        memory.last_time = level_time + 300;
    }
    true
}

/// A saber's sound a command made: the hand whose saber makes it, and its on-sound
/// (`true`) or off-sound. A sound plays where its index is set, and a second hand's
/// off-sound only where that hand holds a saber (`saber[1].model[0]`); an empty second
/// hand still has the default on-sound, which plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaberSound {
    pub hand: u8,
    pub on: bool,
}

impl SaberSound {
    /// Both hands' on- or off-sounds, the first's first.
    pub fn both(on: bool) -> [Self; 2] {
        [Self { hand: 0, on }, Self { hand: 1, on }]
    }

    /// Whether it plays for a player whose second hand does or does not hold a saber.
    pub fn plays(self, second_held: bool) -> bool {
        self.on || self.hand == 0 || second_held
    }
}

/// `Cmd_ToggleSaber_f` (`g_cmds.c:2677-2750`) for a single saber: not while gripped with
/// it off, nor with the hands busy, another weapon out, a duel starting or a lock on; with
/// the weapon idle, off (400 ms of weapon time, the off-sound) or on (the on-sounds of
/// both sabers, the second being the default one). A saber in flight is turned off in
/// the air: returns whether the caller is to knock it down (`saberKnockDown`), which
/// it does with the saber's world.
pub fn toggle_saber(
    state: &mut PlayerState,
    level_time: i32,
    sounds: &mut Vec<SaberSound>,
) -> bool {
    let field = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    if field(state, PS_GRIP_CRIPPLE) != 0 && field(state, PS_SABER_HOLSTERED) != 0 {
        return false;
    }
    if field(state, PS_SABER_IN_FLIGHT) != 0 {
        return field(state, PS_SABER_ENTITY) != 0;
    }
    if field(state, PS_FORCE_HAND_EXTEND) != HANDEXTEND_NONE || field(state, PS_WEAPON) != WP_SABER
    {
        return false;
    }
    if field(state, PS_DUEL_TIME) as i32 >= level_time
        || field(state, PS_SABER_LOCK_TIME) as i32 >= level_time
    {
        return false;
    }
    if (field(state, PS_WEAPON_TIME) as i32) < 1 {
        if field(state, PS_SABER_HOLSTERED) == 2 {
            state.set_raw_field(PS_SABER_HOLSTERED, 0);
            sounds.extend(SaberSound::both(true));
        } else {
            state.set_raw_field(PS_SABER_HOLSTERED, 2);
            sounds.extend(SaberSound::both(false));
            state.set_raw_field(PS_WEAPON_TIME, 400);
        }
    }
    false
}

/// `G_SetTauntAnim` (`g_active.c:1598-1850`), `taunt` the key's
/// `TAUNT_*` (0 the taunt, 1 the bow, 2 meditate, 3 the flourish, 4 the gloat): not while
/// moving; only the taunt outside duels; with nothing else playing, the pose held on the
/// ground (`HANDEXTEND_TAUNT` for its length) and `EV_TAUNT` (but for the bow and
/// meditation). The saber goes off or on as the taunt wants.
#[allow(clippy::too_many_arguments)]
pub fn taunt(
    state: &mut PlayerState,
    sabers: &[crate::saber_definition::SaberDefinition; 2],
    command: &UserCommand,
    taunt: u32,
    gametype: i32,
    level_time: i32,
    lengths: &dyn AnimationLengths,
    hand_extend_time: &mut i32,
    sounds: &mut Vec<SaberSound>,
) {
    if command.up_move != 0 || command.forward_move != 0 || command.right_move != 0 {
        return;
    }
    if taunt != 0 && gametype != GT_DUEL && gametype != GT_POWERDUEL {
        return;
    }
    // `BG_ClearRocketLock`.
    state.set_raw_field(PS_ROCKET_LOCK_INDEX, ENTITY_NUMBER_NONE);
    state.set_raw_field(PS_ROCKET_LOCK_TIME, (-1.0f32).to_bits());
    state.set_raw_field(PS_ROCKET_TARGET_TIME, 0);
    let field = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    let idle = (field(state, PS_TORSO_TIMER) as i32) < 1
        && field(state, PS_FORCE_HAND_EXTEND) == HANDEXTEND_NONE
        && (field(state, PS_LEGS_TIMER) as i32) < 1
        && (field(state, PS_WEAPON_TIME) as i32) < 1
        && (field(state, PS_SABER_LOCK_TIME) as i32) < level_time;
    if !idle {
        return;
    }
    let saber = field(state, PS_WEAPON) == WP_SABER;
    let style = field(state, PS_SABER_ANIM_LEVEL);
    let holstered = field(state, PS_SABER_HOLSTERED);
    let second_held = sabers[1].is_held();
    let named = |name: &str| crate::legacy_animation_index(name).map(|index| index as u16);
    // The sabers' own pose for this taunt (`tauntAnim` .. `gloatAnim`): the first's, else
    // a held second's.
    let own = |slot: usize| {
        let (first, second) = (sabers[0].anims[slot], sabers[1].anims[slot]);
        let anim = if first != -1 {
            first
        } else if second_held {
            second
        } else {
            -1
        };
        (anim != -1).then_some(anim as u16)
    };
    // Sabers put away: the second's off-sound where it alone was on, else the first's.
    let put_away = |state: &mut PlayerState, sounds: &mut Vec<SaberSound>| {
        if holstered == 1 && second_held {
            sounds.push(SaberSound { hand: 1, on: false });
        } else if holstered == 0 {
            sounds.push(SaberSound { hand: 0, on: false });
        }
        state.set_raw_field(PS_SABER_HOLSTERED, 2);
    };
    // Both sabers out: the second's on-sound where it alone was off, else the first's.
    let bring_out_pair = |state: &mut PlayerState, sounds: &mut Vec<SaberSound>| {
        if holstered == 1 && second_held {
            sounds.push(SaberSound { hand: 1, on: true });
        } else if holstered == 2 {
            sounds.push(SaberSound { hand: 0, on: true });
        }
        state.set_raw_field(PS_SABER_HOLSTERED, 0);
    };
    // Every blade out, with the first saber's on-sound for any that was off.
    let bring_out_all = |state: &mut PlayerState, sounds: &mut Vec<SaberSound>| {
        if holstered != 0 {
            sounds.push(SaberSound { hand: 0, on: true });
        }
        state.set_raw_field(PS_SABER_HOLSTERED, 0);
    };
    let animation = match taunt {
        0 if !saber => named("BOTH_ENGAGETAUNT"),
        0 => match own(3) {
            Some(anim) => Some(anim),
            None => match style {
                // `SS_FAST`, `SS_TAVION`: the gesture, the sabers put away.
                1 | 5 => {
                    put_away(state, sounds);
                    named("BOTH_GESTURE1")
                }
                // `SS_MEDIUM`, `SS_STRONG`, `SS_DESANN`.
                2..=4 => named("BOTH_ENGAGETAUNT"),
                6 => {
                    bring_out_pair(state, sounds);
                    named("BOTH_DUAL_TAUNT")
                }
                7 => {
                    bring_out_all(state, sounds);
                    named("BOTH_STAFF_TAUNT")
                }
                _ => None,
            },
        },
        1 | 2 => {
            let animation = own(if taunt == 1 { 4 } else { 5 }).or_else(|| {
                named(if taunt == 1 {
                    "BOTH_BOW"
                } else {
                    "BOTH_MEDITATE"
                })
            });
            put_away(state, sounds);
            animation
        }
        3 if saber => {
            bring_out_pair(state, sounds);
            own(6).or_else(|| match style {
                1 | 5 => named("BOTH_SHOWOFF_FAST"),
                2 => named("BOTH_SHOWOFF_MEDIUM"),
                3 | 4 => named("BOTH_SHOWOFF_STRONG"),
                6 => named("BOTH_SHOWOFF_DUAL"),
                7 => named("BOTH_SHOWOFF_STAFF"),
                _ => None,
            })
        }
        4 => match own(7) {
            Some(anim) => Some(anim),
            None => match style {
                1 | 5 => named("BOTH_VICTORY_FAST"),
                2 => named("BOTH_VICTORY_MEDIUM"),
                3 | 4 => {
                    bring_out_all(state, sounds);
                    named("BOTH_VICTORY_STRONG")
                }
                6 => {
                    bring_out_pair(state, sounds);
                    named("BOTH_VICTORY_DUAL")
                }
                7 => {
                    bring_out_all(state, sounds);
                    named("BOTH_VICTORY_STAFF")
                }
                _ => None,
            },
        },
        _ => None,
    };
    let Some(animation) = animation else { return };
    if field(state, PS_GROUND_ENTITY) != ENTITY_NUMBER_NONE {
        state.set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_TAUNT);
        state.set_raw_field(PS_FORCE_DODGE_ANIM, u32::from(animation));
        // `BG_AnimLength`.
        *hand_extend_time = level_time + lengths.length_ms(animation).unwrap_or(0);
    }
    if taunt != 1 && taunt != 2 {
        // `G_AddEvent(ent, EV_TAUNT, taunt)`: the player's external event.
        let bits =
            (field(state, PS_EXTERNAL_EVENT) & EVENT_BITS).wrapping_add(EVENT_BIT1) & EVENT_BITS;
        state.set_raw_field(PS_EXTERNAL_EVENT, EV_TAUNT | bits);
        state.set_raw_field(PS_EXTERNAL_EVENT_PARM, taunt);
    }
}
