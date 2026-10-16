//! The pose a player dies in (`G_PickDeathAnim`, OpenJK `codemp/game/g_combat.c:1388-1757`,
//! with `G_CheckSpecialDeathAnim`, `g_combat.c:896-1386`):
//! - a lock lost to a finishing blow has its own pose (`gGAvoidDismember`);
//! - a player already in a death pose flops, and keeps it;
//! - a roll, a flip, a knockdown and a get-up each have their own;
//! - otherwise the pose goes by where the blow landed and how hard it was;
//! - where the model lacks the pose, one is drawn among all twenty-five (`BG_PickAnim`).
//!
//! - in space (`inSpaceIndex`), before all of it, the choking pose (`BOTH_CHOKE3`).

use crate::means_of_death::MOD_SABER;
use crate::player_death::{DeathRequest, Rng};
use crate::pmove_anim::AnimationLengths;
use sjk_protocol::PlayerState;

const BOTH_DEATH1: u16 = 9;
/// Clutching the throat: a death in space.
const BOTH_CHOKE3: u16 = 1_322;
const BOTH_DEATH25: u16 = 33;
const BOTH_DEATH14: u16 = 22;
const BOTH_DEATHBACKWARD1: u16 = 37;
const BOTH_DEATHBACKWARD2: u16 = 38;
const BOTH_DEATH_ROLL: u16 = 45;
const BOTH_DEATH_FLIP: u16 = 46;
const BOTH_DEATH_SPIN_180: u16 = 49;
const BOTH_DEATH_LYING_UP: u16 = 50;
const BOTH_DEATH_LYING_DN: u16 = 51;
const BOTH_DEATH_FALLING_DN: u16 = 52;
const BOTH_DEATH_FALLING_UP: u16 = 53;
const BOTH_DEATH_CROUCHED: u16 = 54;
const BOTH_KNOCKDOWN1: u16 = 1_219;
const BOTH_KNOCKDOWN2: u16 = 1_220;
const BOTH_KNOCKDOWN3: u16 = 1_221;
const BOTH_KNOCKDOWN4: u16 = 1_222;
const BOTH_KNOCKDOWN5: u16 = 1_223;
const BOTH_GETUP1: u16 = 1_224;
const BOTH_GETUP2: u16 = 1_225;
const BOTH_GETUP3: u16 = 1_226;
const BOTH_GETUP4: u16 = 1_227;
const BOTH_GETUP5: u16 = 1_228;
const BOTH_FORCE_GETUP_F1: u16 = 1_231;
const BOTH_FORCE_GETUP_F2: u16 = 1_232;
const BOTH_FORCE_GETUP_B1: u16 = 1_233;
const BOTH_FORCE_GETUP_B2: u16 = 1_234;
const BOTH_FORCE_GETUP_B3: u16 = 1_235;
const BOTH_FORCE_GETUP_B4: u16 = 1_236;
const BOTH_FORCE_GETUP_B5: u16 = 1_237;
const BOTH_RIGHTHANDCHOPPEDOFF: u16 = 1_254;
/// `ps.legsAnim`, `ps.legsTimer`.
const PS_LEGS_ANIM: usize = 13;
const PS_LEGS_TIMER: usize = 21;

/// The death and dead poses a dying player flops in (`g_combat.c:1446-1538`): the
/// twenty-five deaths but the last six, the thrown, lying, stumbling and falling deaths,
/// and the finished poses but the last six.
fn flops(legs: u16) -> bool {
    matches!(legs, 9..=27 | 34 | 35 | 37..=44 | 55..=73 | 80..=87)
}

/// `G_PickDeathAnim` for a player or an NPC, `None` for one already in a death pose: it
/// keeps the pose, and `player_die` sets none. `yaw` is `r.currentAngles`' yaw, which the
/// hit's location is judged by: nothing sets a player's (0), an NPC's is its view's.
pub(crate) fn pick(
    rng: &mut Rng,
    lengths: &dyn AnimationLengths,
    state: &PlayerState,
    request: &DeathRequest,
    yaw: f32,
) -> Option<u16> {
    if request.in_space {
        return Some(BOTH_CHOKE3);
    }
    if request.avoid_dismember {
        return Some(BOTH_RIGHTHANDCHOPPEDOFF);
    }
    let legs = state.raw_field(PS_LEGS_ANIM).unwrap_or(0) as u16;
    if flops(legs) {
        return None;
    }
    let animation =
        special(lengths, state).map_or_else(|| by_location(rng, state, request, yaw), i32::from);
    if animation == -1
        || !lengths
            .timing(animation as u16)
            .is_some_and(|timing| timing.frame_count > 0)
    {
        return Some(pick_plain(rng, lengths));
    }
    Some(animation as u16)
}

/// `G_CheckSpecialDeathAnim`: from a roll, a flip, a knockdown or a get-up (`G_InKnockDown`),
/// by how far into it the player is.
fn special(lengths: &dyn AnimationLengths, state: &PlayerState) -> Option<u16> {
    let legs = state.raw_field(PS_LEGS_ANIM).unwrap_or(0) as u16;
    let timer = state.raw_field(PS_LEGS_TIMER).unwrap_or(0) as i32;
    if crate::pmove_roll_anim::in_roll(legs) && timer > 0 {
        return Some(BOTH_DEATH_ROLL);
    }
    if crate::pmove_roll_anim::flipping(legs) {
        return Some(BOTH_DEATH_FLIP);
    }
    // `bgAllAnims[..].anims[legsAnim].numFrames * fabs(frameLerp)`: how long the pose is.
    let length = lengths.timing(legs).map_or(0, |timing| timing.length_ms());
    let into = length - timer;
    // Crouched: thrown back, or where it is.
    let crouched = || {
        let (forward, _) = crate::pmove::flight::flight_axes(state.view_angles());
        let velocity = state.velocity();
        let thrown = forward.x * velocity[0] + forward.y * velocity[1] + forward.z * velocity[2];
        if thrown < -150.0 {
            BOTH_DEATHBACKWARD1
        } else {
            BOTH_DEATH_CROUCHED
        }
    };
    // Lying: partly up once `up` into the pose, else down.
    let lying = |up: i32, back: bool| lying_or_falling(into > up, back);
    // Falling down past `down`: still partly up above `standing`, else lying.
    let falling = |down: i32, standing: i32, back: bool| {
        (into > down).then(|| lying_or_falling(timer > standing, back))
    };
    match legs {
        BOTH_KNOCKDOWN1 => falling(100, 600, true),
        BOTH_KNOCKDOWN2 => falling(700, 600, true),
        BOTH_KNOCKDOWN3 => falling(100, 1_300, false),
        BOTH_KNOCKDOWN4 if into > 300 => Some(lying_or_falling(timer > 350, true)),
        BOTH_KNOCKDOWN4 => Some(crouched()),
        BOTH_KNOCKDOWN5 => (timer < 750).then_some(BOTH_DEATH_LYING_DN),
        BOTH_GETUP1 => (timer >= 350).then(|| {
            if timer < 800 {
                crouched()
            } else {
                lying(450, true)
            }
        }),
        BOTH_GETUP2 => (timer >= 150).then(|| {
            if timer < 850 {
                crouched()
            } else {
                lying(500, true)
            }
        }),
        BOTH_GETUP3 => (timer >= 250).then(|| {
            if timer < 600 {
                crouched()
            } else {
                lying(150, false)
            }
        }),
        // The get-up from the front ends on its back once down.
        BOTH_GETUP4 => (timer >= 250).then(|| {
            if timer < 600 {
                crouched()
            } else if into > 850 {
                BOTH_DEATH_FALLING_DN
            } else {
                BOTH_DEATH_LYING_UP
            }
        }),
        BOTH_GETUP5 => (timer > 850).then(|| lying(1_500, false)),
        BOTH_FORCE_GETUP_B1 => (timer >= 325).then(|| {
            if timer < 725 {
                BOTH_DEATH_SPIN_180
            } else if timer < 900 {
                crouched()
            } else {
                lying(50, true)
            }
        }),
        BOTH_FORCE_GETUP_B2 => (timer >= 575).then(|| {
            if timer < 875 {
                BOTH_DEATH_SPIN_180
            } else if timer < 900 {
                crouched()
            } else {
                BOTH_DEATH_FALLING_UP
            }
        }),
        BOTH_FORCE_GETUP_B3 => (timer >= 150).then_some(if timer < 775 {
            BOTH_DEATHBACKWARD2
        } else {
            BOTH_DEATH_FALLING_UP
        }),
        BOTH_FORCE_GETUP_B4 => (timer >= 325).then(|| lying(150, true)),
        BOTH_FORCE_GETUP_B5 => (timer >= 550).then(|| {
            if timer < 1_025 {
                BOTH_DEATHBACKWARD2
            } else {
                lying(50, true)
            }
        }),
        BOTH_FORCE_GETUP_F1 => (timer >= 275).then(|| {
            if timer < 750 {
                BOTH_DEATH14
            } else {
                lying(100, false)
            }
        }),
        BOTH_FORCE_GETUP_F2 => (timer >= 1_200).then(|| lying(225, false)),
        // The reference's cases for the crouched get-ups and `BOTH_FORCE_GETUP_B6` are
        // behind `G_InKnockDown`, which does not list them: they are never reached.
        _ => None,
    }
}

/// Still partly up (falling) or down (lying), on its back (`back`) or its front.
fn lying_or_falling(partly_up: bool, back: bool) -> u16 {
    match (partly_up, back) {
        (true, true) => BOTH_DEATH_FALLING_UP,
        (true, false) => BOTH_DEATH_FALLING_DN,
        (false, true) => BOTH_DEATH_LYING_UP,
        (false, false) => BOTH_DEATH_LYING_DN,
    }
}

/// `g_combat.c:1546-1747`: by the hit's location — the feet and legs, the back, either
/// side of the chest, the chest and waist, the head — with the blow's size against the
/// player's full health and the game's generator deciding among the poses; -1 for none.
fn by_location(rng: &mut Rng, state: &PlayerState, request: &DeathRequest, yaw: f32) -> i32 {
    use crate::damage::{HitLocation, hit_location};
    let death = |number: u16| i32::from(BOTH_DEATH1 + number - 1);
    let max_health = f64::from(state.max_health());
    let damage = f64::from(request.damage);
    let still = state.velocity() == [0.0; 3];
    let legs_or_back = |rng: &mut Rng| {
        if rng.irand(0, 2) == 0 {
            death(4)
        } else if rng.irand(0, 1) == 0 {
            death(5)
        } else {
            death(15)
        }
    };
    match hit_location(yaw, request.bounds, request.point) {
        HitLocation::FootRight | HitLocation::FootLeft => {
            if request.means == MOD_SABER && rng.irand(0, 2) == 0 {
                death(10)
            } else {
                legs_or_back(rng)
            }
        }
        HitLocation::LegRight | HitLocation::LegLeft => legs_or_back(rng),
        HitLocation::Back => {
            if still {
                death(17)
            } else {
                legs_or_back(rng)
            }
        }
        HitLocation::ChestRight
        | HitLocation::ArmRight
        | HitLocation::HandRight
        | HitLocation::BackRight => {
            if damage <= max_health * 0.25 {
                death(9)
            } else if damage <= max_health * 0.5 {
                death(3)
            } else if damage <= max_health * 0.75 {
                death(6)
            } else if rng.irand(0, 1) != 0 {
                death(8)
            } else {
                [death(9), death(3), death(6)][rng.irand(0, 2) as usize]
            }
        }
        HitLocation::ChestLeft
        | HitLocation::ArmLeft
        | HitLocation::HandLeft
        | HitLocation::BackLeft => {
            if damage <= max_health * 0.25 {
                death(11)
            } else if damage <= max_health * 0.5 {
                death(7)
            } else if damage <= max_health * 0.75 {
                death(12)
            } else if rng.irand(0, 1) != 0 {
                death(14)
            } else {
                [death(11), death(7), death(12)][rng.irand(0, 2) as usize]
            }
        }
        HitLocation::Chest | HitLocation::Waist => {
            if damage <= max_health * 0.25 || still {
                if rng.irand(0, 1) == 0 {
                    death(18)
                } else {
                    death(19)
                }
            } else if damage <= max_health * 0.5 {
                death(2)
            } else if damage <= max_health * 0.75 {
                if rng.irand(0, 1) == 0 {
                    death(1)
                } else {
                    death(16)
                }
            } else {
                death(10)
            }
        }
        HitLocation::Head => {
            if damage <= max_health * 0.5 {
                death(17)
            } else {
                death(13)
            }
        }
        HitLocation::None => -1,
    }
}

/// `BG_PickAnim(BOTH_DEATH1, BOTH_DEATH25)`: draws until one the model has.
/// A skeleton with none of them (a droid's) gets 0 after a thousand draws — the reference's
/// -1, "guess we just don't have a death anim then", which no animation below 1 is played
/// for; the thousandth draw counts as none even when it finds one, as `BG_PickAnim`'s
/// `count == 1000` has it.
fn pick_plain(rng: &mut Rng, lengths: &dyn AnimationLengths) -> u16 {
    for count in 1..=1_000 {
        let animation = rng.irand(i32::from(BOTH_DEATH1), i32::from(BOTH_DEATH25)) as u16;
        if lengths
            .timing(animation)
            .is_some_and(|timing| timing.frame_count > 0)
        {
            return if count < 1_000 { animation } else { 0 };
        }
    }
    0
}
