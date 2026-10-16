//! Which controls make the saber move a leader is seen doing: the inverse of
//! `PM_SaberAttackForMovement` (`bg_saber.c:2182-2560`) for the moves a player chooses,
//! mirrored left for right when the mimic faces the leader as a reflection.

use sjk_game_jka::saber_move_data::SABER_MOVES;

/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
pub(super) const BUTTON_ATTACK: u16 = 1;
pub(super) const BUTTON_ALT_ATTACK: u16 = 128;

// `saberMoveName_t` values (`bg_saber.h`); SJK keeps them crate-private in
// `saber_move_data::movement`.
const LS_A_TL2BR: u16 = 4;
const LS_A_T2B: u16 = 10;
const LS_A_BACKSTAB: u16 = 11;
const LS_A_BACK: u16 = 12;
const LS_A_BACK_CR: u16 = 13;
const LS_A_LUNGE: u16 = 15;
const LS_A_JUMP_T__B_: u16 = 16;
const LS_A_FLIP_STAB: u16 = 17;
const LS_A_FLIP_SLASH: u16 = 18;
const LS_JUMPATTACK_DUAL: u16 = 19;
const LS_JUMPATTACK_ARIAL_LEFT: u16 = 20;
const LS_JUMPATTACK_ARIAL_RIGHT: u16 = 21;
const LS_JUMPATTACK_CART_LEFT: u16 = 22;
const LS_JUMPATTACK_CART_RIGHT: u16 = 23;
const LS_JUMPATTACK_STAFF_LEFT: u16 = 24;
const LS_JUMPATTACK_STAFF_RIGHT: u16 = 25;
const LS_BUTTERFLY_LEFT: u16 = 26;
const LS_BUTTERFLY_RIGHT: u16 = 27;
const LS_A_BACKFLIP_ATK: u16 = 28;
const LS_SPINATTACK_DUAL: u16 = 29;
const LS_SPINATTACK: u16 = 30;
const LS_KICK_F: u16 = 36;
const LS_KICK_B: u16 = 37;
const LS_KICK_R: u16 = 38;
const LS_KICK_L: u16 = 39;
const LS_KICK_F_AIR: u16 = 43;
const LS_KICK_B_AIR: u16 = 44;
const LS_KICK_R_AIR: u16 = 45;
const LS_KICK_L_AIR: u16 = 46;
const LS_STABDOWN: u16 = 47;
const LS_STABDOWN_STAFF: u16 = 48;
const LS_STABDOWN_DUAL: u16 = 49;
const LS_DUAL_SPIN_PROTECT: u16 = 50;
const LS_STAFF_SOULCAL: u16 = 51;
const LS_A1_SPECIAL: u16 = 52;
const LS_A2_SPECIAL: u16 = 53;
const LS_A3_SPECIAL: u16 = 54;
const LS_DUAL_FB: u16 = 59;
const LS_DUAL_LR: u16 = 60;
const LS_HILT_BASH: u16 = 61;
const LS_S_TL2BR: u16 = 62;
const LS_S_T2B: u16 = 68;
const LS_T1_FIRST: u16 = 76;
const LS_T1_LAST: u16 = 117;
const LS_B1_FIRST: u16 = 118;
const LS_B1_LAST: u16 = 124;

/// `saberQuadrant_t`.
const Q_BR: u8 = 0;
const Q_R: u8 = 1;
const Q_TR: u8 = 2;
const Q_TL: u8 = 4;
const Q_L: u8 = 5;
const Q_BL: u8 = 6;

const FULL: i8 = 127;

/// The controls held for a saber move.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Press {
    pub(super) forward: i8,
    pub(super) right: i8,
    /// Jump (positive) or crouch (negative) the move needs; `None` leaves it to the
    /// copied jumping and crouching.
    pub(super) up: Option<i8>,
    pub(super) buttons: u16,
    /// The keys matter through the whole move (a special); a directional attack's only
    /// where a move is chosen.
    pub(super) whole_move: bool,
}

/// What to hold while the leader is in `saber_move`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Plan {
    /// No attack: let go of the buttons.
    Release,
    /// A bounce off another blade: keep holding what was held.
    Keep,
    Hold(Press),
}

/// Keys that start the directional attack beginning in `quadrant`: right and forward
/// give the top-left start (`LS_A_TL2BR`), right alone the left one (`LS_A_L2R`), and so
/// on round the quadrants.
fn keys_for_quadrant(quadrant: u8) -> (i8, i8) {
    match quadrant {
        Q_TL => (FULL, FULL),
        Q_L => (0, FULL),
        Q_BL => (-FULL, FULL),
        Q_BR => (-FULL, -FULL),
        Q_R => (0, -FULL),
        Q_TR => (FULL, -FULL),
        _ => (FULL, 0), // Q_T (3): the overhead.
    }
}

/// The same quadrant seen in a mirror: left and right swap.
fn mirror_quadrant(quadrant: u8) -> u8 {
    match quadrant {
        Q_BR => Q_BL,
        Q_BL => Q_BR,
        Q_R => Q_L,
        Q_L => Q_R,
        Q_TR => Q_TL,
        Q_TL => Q_TR,
        other => other,
    }
}

/// A special's mirror image: its left form for its right one.
fn mirror_special(saber_move: u16) -> u16 {
    match saber_move {
        LS_JUMPATTACK_ARIAL_LEFT => LS_JUMPATTACK_ARIAL_RIGHT,
        LS_JUMPATTACK_ARIAL_RIGHT => LS_JUMPATTACK_ARIAL_LEFT,
        LS_JUMPATTACK_CART_LEFT => LS_JUMPATTACK_CART_RIGHT,
        LS_JUMPATTACK_CART_RIGHT => LS_JUMPATTACK_CART_LEFT,
        LS_JUMPATTACK_STAFF_LEFT => LS_JUMPATTACK_STAFF_RIGHT,
        LS_JUMPATTACK_STAFF_RIGHT => LS_JUMPATTACK_STAFF_LEFT,
        LS_BUTTERFLY_LEFT => LS_BUTTERFLY_RIGHT,
        LS_BUTTERFLY_RIGHT => LS_BUTTERFLY_LEFT,
        LS_KICK_R => LS_KICK_L,
        LS_KICK_L => LS_KICK_R,
        LS_KICK_R_AIR => LS_KICK_L_AIR,
        LS_KICK_L_AIR => LS_KICK_R_AIR,
        other => other,
    }
}

fn hold(forward: i8, right: i8, up: Option<i8>, buttons: u16) -> Plan {
    Plan::Hold(Press {
        forward,
        right,
        up,
        buttons,
        whole_move: true,
    })
}

/// A move of an attack chain: an attack or special, a start or a transition.
pub(super) fn in_chain(saber_move: u32) -> bool {
    (u32::from(LS_A_TL2BR)..=u32::from(LS_S_T2B)).contains(&saber_move)
        || (u32::from(LS_T1_FIRST)..=u32::from(LS_T1_LAST)).contains(&saber_move)
}

/// An attack or special: at its end the next move is chosen from the controls held
/// (`PM_WeaponLightsaber`).
pub(super) fn is_attack(saber_move: u32) -> bool {
    (u32::from(LS_A_TL2BR)..=u32::from(LS_HILT_BASH)).contains(&saber_move)
}

/// What to hold while the leader is seen in `saber_move`.
pub(super) fn plan(saber_move: u32, mirror: bool) -> Plan {
    let Ok(saber_move) = u16::try_from(saber_move) else {
        return Plan::Release;
    };
    // A directional attack, its start or a transition into the next one: hold attack
    // with the keys of the attack it begins (the start's and transition's end quadrant,
    // the attack's own start quadrant).
    let quadrant = match saber_move {
        LS_A_TL2BR..=LS_A_T2B => SABER_MOVES
            .get(usize::from(saber_move))
            .map(|m| m.start_quad),
        LS_S_TL2BR..=LS_S_T2B | LS_T1_FIRST..=LS_T1_LAST => {
            SABER_MOVES.get(usize::from(saber_move)).map(|m| m.end_quad)
        }
        _ => None,
    };
    if let Some(quadrant) = quadrant {
        let quadrant = if mirror {
            mirror_quadrant(quadrant)
        } else {
            quadrant
        };
        let (forward, right) = keys_for_quadrant(quadrant);
        return Plan::Hold(Press {
            forward,
            right,
            up: None,
            buttons: BUTTON_ATTACK,
            whole_move: false,
        });
    }
    if (LS_B1_FIRST..=LS_B1_LAST).contains(&saber_move) {
        return Plan::Keep;
    }
    let special = if mirror {
        mirror_special(saber_move)
    } else {
        saber_move
    };
    const UP: Option<i8> = Some(FULL);
    const DOWN: Option<i8> = Some(-FULL);
    match special {
        LS_A_BACKSTAB | LS_A_BACK => hold(-FULL, 0, None, BUTTON_ATTACK),
        LS_A_BACK_CR => hold(-FULL, 0, DOWN, BUTTON_ATTACK),
        // The lunge and the spins come from a crouch (`PMF_DUCKED`) with forward.
        LS_A_LUNGE | LS_SPINATTACK | LS_SPINATTACK_DUAL => hold(FULL, 0, DOWN, BUTTON_ATTACK),
        // The DFA and flip over, from a jump in progress; the dual and staff jump
        // attacks, from the ground with jump.
        LS_A_JUMP_T__B_
        | LS_A_FLIP_STAB
        | LS_A_FLIP_SLASH
        | LS_JUMPATTACK_DUAL
        | LS_JUMPATTACK_STAFF_LEFT
        | LS_JUMPATTACK_STAFF_RIGHT => hold(FULL, 0, UP, BUTTON_ATTACK),
        LS_JUMPATTACK_ARIAL_LEFT | LS_JUMPATTACK_CART_LEFT | LS_BUTTERFLY_LEFT => {
            hold(0, -FULL, UP, BUTTON_ATTACK)
        }
        LS_JUMPATTACK_ARIAL_RIGHT | LS_JUMPATTACK_CART_RIGHT | LS_BUTTERFLY_RIGHT => {
            hold(0, FULL, UP, BUTTON_ATTACK)
        }
        LS_A_BACKFLIP_ATK => hold(-FULL, 0, UP, BUTTON_ATTACK),
        LS_KICK_F | LS_KICK_F_AIR => hold(FULL, 0, None, BUTTON_ALT_ATTACK),
        LS_KICK_B | LS_KICK_B_AIR => hold(-FULL, 0, None, BUTTON_ALT_ATTACK),
        LS_KICK_R | LS_KICK_R_AIR => hold(0, FULL, None, BUTTON_ALT_ATTACK),
        LS_KICK_L | LS_KICK_L_AIR => hold(0, -FULL, None, BUTTON_ALT_ATTACK),
        LS_STABDOWN | LS_STABDOWN_STAFF | LS_STABDOWN_DUAL | LS_DUAL_FB => {
            hold(FULL, 0, None, BUTTON_ATTACK)
        }
        LS_DUAL_LR => hold(0, FULL, None, BUTTON_ATTACK),
        // The katas: both buttons, standing still (`PM_CanDoKata`).
        LS_A1_SPECIAL | LS_A2_SPECIAL | LS_A3_SPECIAL | LS_DUAL_SPIN_PROTECT | LS_STAFF_SOULCAL => {
            hold(0, 0, None, BUTTON_ATTACK | BUTTON_ALT_ATTACK)
        }
        // Other specials (the roll stab, pull attacks, the hilt bash) depend on what is
        // around the player: attack alone.
        LS_A_BACKSTAB..=LS_HILT_BASH => hold(0, 0, None, BUTTON_ATTACK),
        _ => Plan::Release,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_directional_attack_is_asked_for_by_its_keys() {
        // `PM_SaberAttackForMovement`: right+forward TL2BR, right L2R, right+back BL2TR,
        // left+back BR2TL, left R2L, left+forward TR2BL, forward T2B.
        let expected = [
            (4, (FULL, FULL)),
            (5, (0, FULL)),
            (6, (-FULL, FULL)),
            (7, (-FULL, -FULL)),
            (8, (0, -FULL)),
            (9, (FULL, -FULL)),
            (10, (FULL, 0)),
        ];
        for (attack, keys) in expected {
            let Plan::Hold(press) = plan(attack, false) else {
                panic!("{attack}");
            };
            assert_eq!((press.forward, press.right), keys, "attack {attack}");
            assert_eq!(press.buttons, BUTTON_ATTACK);
            // Its start (LS_S_*, 58 after the attack) asks for the same keys.
            assert_eq!(
                plan(attack + 58, false),
                plan(attack, false),
                "start {attack}"
            );
        }
    }

    #[test]
    fn a_transition_asks_for_the_attack_it_leads_into() {
        // LS_T1_BR__TL (79) leads into the attack starting top left: TL2BR.
        assert_eq!(plan(79, false), plan(4, false));
    }

    #[test]
    fn the_mirror_swaps_left_and_right() {
        for (attack, mirrored) in [(4, 9), (5, 8), (6, 7), (10, 10)] {
            assert_eq!(plan(attack, true), plan(mirrored, false), "{attack}");
        }
        assert_eq!(
            plan(u32::from(LS_KICK_R), true),
            plan(u32::from(LS_KICK_L), false)
        );
        assert_eq!(
            plan(u32::from(LS_JUMPATTACK_ARIAL_LEFT), true),
            plan(u32::from(LS_JUMPATTACK_ARIAL_RIGHT), false)
        );
    }

    #[test]
    fn specials_hold_their_extra_controls_and_reactions_let_go() {
        let Plan::Hold(dfa) = plan(u32::from(LS_A_JUMP_T__B_), false) else {
            panic!()
        };
        assert_eq!((dfa.forward, dfa.up), (FULL, Some(FULL)));
        let Plan::Hold(lunge) = plan(u32::from(LS_A_LUNGE), false) else {
            panic!()
        };
        assert_eq!(lunge.up, Some(-FULL));
        let Plan::Hold(kata) = plan(u32::from(LS_A2_SPECIAL), false) else {
            panic!()
        };
        assert_eq!(kata.buttons, BUTTON_ATTACK | BUTTON_ALT_ATTACK);
        assert_eq!(plan(1, false), Plan::Release, "ready");
        assert_eq!(plan(69, false), Plan::Release, "a return");
        assert_eq!(plan(152, false), Plan::Release, "a parry");
        assert_eq!(plan(120, false), Plan::Keep, "a bounce");
    }
}
