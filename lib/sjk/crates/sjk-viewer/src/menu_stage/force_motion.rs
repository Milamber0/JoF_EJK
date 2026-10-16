//! The Force page powers the preview's model shows by moving
//! ([`crate::player_menu::force_pose::Effect`]): a Force jump as high as the drafted
//! level jumps, the saber thrown out and caught again, and the saber skills' swings and
//! parries played one after another.
//!
//! The jump lasts as long as a real jump to the level's height under the game's gravity
//! (800 units/s², `force_pose::JUMP_HEIGHTS`), as the game plays a jump made by holding
//! the jump key while standing still: the body takes off, keeps the crouch in the air
//! and lands ([`jump_at`]), its sabers off in the air, a moment standing,
//! and again, as Creyon's videos of the game show it (13/10/2026). The model
//! is seen to rise in the picture and come back down to where it stood, at most
//! [`MAX_RISE`] units at the top of the jump, so the picture never zooms out and the
//! model never sinks below its spot.
//!
//! The throw flies the first saber out of the hand as a thrown saber leaves it
//! (`w_saber.c`: 400 units/s, spinning 800°/s about the vertical, lying flat), out to
//! the side and towards the camera, then back into the hand, while the model holds
//! `BOTH_SABERPULL`; a moment in the hand, and again.

use super::throw::Pose;
use crate::player_menu::force_pose::{Move, Takeover};
use glam::{Quat, Vec3};

/// `g_gravity`'s default.
const GRAVITY: f32 = 800.0;
/// Seconds the landing pose plays, then the model stands, before the next jump. The
/// landing is as long as `BOTH_LAND1` takes to reach its last frame (four frames at 20 a
/// second); held 0.5 s it stood crouched a moment too long (Creyon, 13/10/2026).
const LANDING_SECONDS: f32 = 0.2;
const STANDING_SECONDS: f32 = 0.7;
/// How fast Speed's run plays: `CG_PlayerAnimation` runs a speeding player's legs at
/// 1.7 times their speed (`legacy_animation_speed` in `sjk-client`); at the stance's
/// speed the run looked like no Force at all (Creyon, 13/10/2026).
pub(super) const SPEED_RUN_RATE: f32 = 1.7;
/// How far the model is seen to rise at the top of a jump, in units.
/// The framing leaves little room over the head, and 20 took it out of the picture
/// (Creyon, 13/10/2026).
pub(super) const MAX_RISE: f32 = 8.0;

/// Seconds a jump to `height` spends in the air: up at `sqrt(2 g h)` and down again.
pub(super) fn airborne_seconds(height: f32) -> f32 {
    2.0 * (2.0 * GRAVITY * height.max(0.0)).sqrt() / GRAVITY
}

/// The animation a jump to `height` plays `seconds` after the preview began; `None`
/// stands. The body takes off in `BOTH_JUMP1` (`PM_JumpForDir` with no direction) and
/// keeps its crouch, its last frame held, the whole time in the air, then lands in
/// `BOTH_LAND1`, arms and all, as Creyon's videos of the game (13/10/2026) show it:
/// the crouch held up there (`BOTH_INAIR1`'s hanging legs looked like a jetpack), the
/// hands where the jump holds them (the torso kept in the stance held them elsewhere).
pub(super) fn jump_at(height: f32, seconds: f32) -> Option<&'static str> {
    let airborne = airborne_seconds(height);
    let period = airborne + LANDING_SECONDS + STANDING_SECONDS;
    let t = seconds.max(0.0) % period;
    if t < airborne {
        Some("BOTH_JUMP1")
    } else if t < airborne + LANDING_SECONDS {
        Some("BOTH_LAND1")
    } else {
        None
    }
}

/// How far the model is seen to rise `seconds` into a jump to `height`: the jump's own
/// height scaled so its top is [`MAX_RISE`], never below where it stands.
pub(super) fn rise_seen(height: f32, seconds: f32) -> f32 {
    if height <= 0.0 {
        return 0.0;
    }
    MAX_RISE * (jump_height_at(height, seconds) / height).clamp(0.0, 1.0)
}

/// How high over the ground a jump to `height` is `seconds` after the preview began.
pub(super) fn jump_height_at(height: f32, seconds: f32) -> f32 {
    let airborne = airborne_seconds(height);
    let period = airborne + LANDING_SECONDS + STANDING_SECONDS;
    let t = seconds.max(0.0) % period;
    if t >= airborne {
        return 0.0;
    }
    let launch = (2.0 * GRAVITY * height.max(0.0)).sqrt();
    (launch * t - 0.5 * GRAVITY * t * t).max(0.0)
}

/// Where a script of moves is: the animation it plays (`None` while it stands in the
/// stance), how that takes over, and whether the power's effect plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Step {
    pub(super) animation: Option<&'static str>,
    pub(super) takeover: Takeover,
    pub(super) effect: bool,
}

/// Where `moves` are `seconds` after the preview began: each move for as long as
/// `length` says it lasts and its `keep` after, the stance for its hold after that
/// (taking over as the move's `settle` says), then again from the first.
pub(super) fn script_at(moves: &[Move], length: impl Fn(&str) -> f32, seconds: f32) -> Step {
    let stance = |takeover| Step {
        animation: None,
        takeover,
        effect: false,
    };
    let Some(first) = moves.first() else {
        return stance(crate::player_menu::force_pose::GAME_TAKEOVER);
    };
    let spans = moves
        .iter()
        .map(|step| (step, length(step.animation).max(0.05) + step.keep));
    let period: f32 = spans.clone().map(|(step, length)| length + step.hold).sum();
    if period <= 0.0 {
        return stance(first.takeover);
    }
    // A held move never ends (`keep` infinite): the remainder is the time itself.
    let mut t = if period.is_finite() {
        seconds.max(0.0) % period
    } else {
        seconds.max(0.0)
    };
    for (step, length) in spans {
        if t < length {
            return Step {
                animation: Some(step.animation),
                takeover: step.takeover,
                effect: step.effect,
            };
        }
        t -= length;
        if t < step.hold {
            return Step {
                effect: step.effect && t < step.linger,
                ..stance(step.settle)
            };
        }
        t -= step.hold;
    }
    stance(first.takeover)
}

/// The animation the saber skills' `moves` play `seconds` after the preview began, in
/// `stance` between them, and how it takes over ([`script_at`]).
pub(super) fn moves_at(
    moves: &[Move],
    stance: &'static str,
    length: impl Fn(&str) -> f32,
    seconds: f32,
) -> (&'static str, Takeover) {
    let step = script_at(moves, length, seconds);
    (step.animation.unwrap_or(stance), step.takeover)
}

/// Whether a shell worn with no gesture to time it (Sense's) shows `seconds` after the
/// preview began: on for a while, off for the preview's wait, and on again (Creyon,
/// 13/10/2026: "force sense turning off after some seconds and then redo the preview
/// again turning it on").
pub(super) fn lone_shell_on(seconds: f32) -> bool {
    use crate::player_menu::force_pose::{PREVIEW_PAUSE, SPEED_ON};
    seconds.max(0.0) % (SPEED_ON + PREVIEW_PAUSE) < SPEED_ON
}

/// Seconds a jumping preview's blades take to go off after the take-off, and to light
/// again after landing.
const BLADE_OFF_SECONDS: f32 = 0.15;
const BLADE_ON_SECONDS: f32 = 0.25;

/// How far the blades are drawn in (0 out, 1 off) `seconds` into a jump to `height`:
/// off in the air and lit again on landing, so the blade held over the head does not
/// pass through it as it does in the game's jump (Creyon, 13/10/2026).
pub(super) fn blade_in_at(height: f32, seconds: f32) -> f32 {
    let airborne = airborne_seconds(height);
    let period = airborne + LANDING_SECONDS + STANDING_SECONDS;
    let t = seconds.max(0.0) % period;
    if t < airborne {
        (t / BLADE_OFF_SECONDS).min(1.0)
    } else {
        1.0 - ((t - airborne) / BLADE_ON_SECONDS).min(1.0)
    }
}

/// Seconds the thrown saber flies out, flies back, then rests in the hand. It goes out
/// as far as 0.4 s at the game's 400 units a second takes it ([`THROW_REACH`]), but
/// fast at first and slowing to its far end, so it is out longer before it comes back
/// (Creyon, 13/10/2026: "a bit longer before it returns") without leaving the picture.
const THROW_OUT: f32 = 0.75;
const THROW_BACK: f32 = 0.4;
const THROW_REST: f32 = crate::player_menu::force_pose::PREVIEW_PAUSE;
/// `VectorScale(dir, 400, trDelta)` and `apos.trDelta[1] = 800`.
const THROW_SPEED: f32 = 400.0;
const THROW_SPIN: f32 = 800.0;
/// How far the thrown saber goes, in units.
const THROW_REACH: f32 = THROW_SPEED * 0.4;

/// Whether the thrown saber is out of the hand `seconds` after the preview began: the
/// throwing arm stays out until it is caught (`BOTH_SABERPULL` held while
/// `saberInFlight`; Creyon, 13/10/2026: "it must stay extended until it comes back").
pub(super) fn saber_out_at(seconds: f32) -> bool {
    seconds.max(0.0) % (THROW_OUT + THROW_BACK + THROW_REST) < THROW_OUT + THROW_BACK
}

/// Where the thrown saber is `seconds` after the preview began, thrown along
/// `direction` from `hand` (its pose in the hand), its blade along `blade` in the
/// hilt's frame; `None` while it is in the hand.
pub(super) fn throw_at(hand: Pose, blade: Vec3, direction: Vec3, seconds: f32) -> Option<Pose> {
    let period = THROW_OUT + THROW_BACK + THROW_REST;
    let t = seconds.max(0.0) % period;
    if t >= THROW_OUT + THROW_BACK {
        return None;
    }
    let far = hand.grip + direction.normalize_or(Vec3::X) * THROW_REACH;
    let flat = Quat::from_rotation_arc(blade.normalize_or(Vec3::Y), Vec3::X);
    let spinning = Quat::from_rotation_z((THROW_SPIN * t).to_radians()) * flat;
    if t < THROW_OUT {
        return Some(Pose {
            grip: hand.grip.lerp(far, slowing(t / THROW_OUT)),
            rotation: spinning,
        });
    }
    let back = (t - THROW_OUT) / THROW_BACK;
    // The last third of the way back it turns into the hand's grip.
    let settle = smoothstep((back - 0.67) / 0.33);
    Some(Pose {
        grip: far.lerp(hand.grip, back),
        rotation: spinning.slerp(hand.rotation, settle),
    })
}

/// From 0 to 1, fast at first and slowing to a stop at the end.
fn slowing(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A level 3 jump stays in the air as long as the game's (about 2 s for 418
    /// units), then lands and stands, and again; a lower one is shorter.
    #[test]
    fn the_jump_lasts_as_long_as_the_level_s() {
        let airborne = airborne_seconds(418.0);
        assert!((airborne - 2.045).abs() < 0.01, "{airborne}");
        assert!(airborne_seconds(130.0) < airborne);
        // The take-off's crouch the whole time in the air, the landing, then the stance.
        assert_eq!(jump_at(418.0, 0.05), Some("BOTH_JUMP1"));
        assert_eq!(jump_at(418.0, airborne / 2.0), Some("BOTH_JUMP1"));
        assert_eq!(jump_at(418.0, airborne + 0.1), Some("BOTH_LAND1"));
        assert_eq!(jump_at(418.0, airborne + LANDING_SECONDS + 0.1), None);
        let period = airborne + LANDING_SECONDS + STANDING_SECONDS;
        assert_eq!(jump_at(418.0, period + airborne / 2.0), Some("BOTH_JUMP1"));
    }

    /// Up from the ground to the level's height at the top, and down again.
    #[test]
    fn the_model_rises_to_the_jump_s_height() {
        let airborne = airborne_seconds(418.0);
        assert_eq!(jump_height_at(418.0, 0.0), 0.0);
        assert!((jump_height_at(418.0, airborne / 2.0) - 418.0).abs() < 1.0);
        assert!(jump_height_at(418.0, airborne * 0.25) > 200.0);
        assert_eq!(jump_height_at(418.0, airborne + 0.1), 0.0);
        // Seen rising at most MAX_RISE, back to its spot on landing, never under it.
        assert!((rise_seen(418.0, airborne / 2.0) - MAX_RISE).abs() < 0.1);
        assert_eq!(rise_seen(418.0, airborne + 0.1), 0.0);
        for step in 0..100 {
            let rise = rise_seen(130.0, step as f32 * 0.03);
            assert!((0.0..=MAX_RISE).contains(&rise));
        }
    }

    /// Each animation for its length and its keep, then the stance for its hold, then
    /// the next, each taking over as its move says.
    #[test]
    fn moves_play_in_turn_with_their_holds() {
        use crate::player_menu::force_pose::{GAME_TAKEOVER, PARRY_TAKEOVER, SETTLE_TAKEOVER};
        const MOVES: [Move; 2] = [
            Move {
                animation: "BOTH_P1_S1_T_",
                keep: 0.2,
                hold: 0.5,
                takeover: PARRY_TAKEOVER,
                settle: SETTLE_TAKEOVER,
                effect: true,
                linger: 0.0,
            },
            Move {
                animation: "BOTH_A3_T__B_",
                keep: 0.0,
                hold: 0.0,
                takeover: GAME_TAKEOVER,
                settle: GAME_TAKEOVER,
                effect: false,
                linger: 0.0,
            },
        ];
        let length = |_: &str| 0.3;
        let stance = "BOTH_SABERSLOW_STANCE";
        let at = |seconds| moves_at(&MOVES, stance, length, seconds);
        assert_eq!(at(0.1), ("BOTH_P1_S1_T_", PARRY_TAKEOVER));
        // Its last frame kept, then the stance.
        assert_eq!(at(0.45).0, "BOTH_P1_S1_T_");
        assert_eq!(at(0.6), (stance, SETTLE_TAKEOVER));
        assert_eq!(at(1.1), ("BOTH_A3_T__B_", GAME_TAKEOVER));
        assert_eq!(at(1.3 + 0.1).0, "BOTH_P1_S1_T_");
        // The effect plays with its move, not in the stance after it.
        assert!(script_at(&MOVES, length, 0.1).effect);
        assert!(!script_at(&MOVES, length, 0.6).effect);
        assert_eq!(script_at(&MOVES, length, 0.6).animation, None);
    }

    /// An effect that lingers stays on into the stance for its while, then goes off
    /// until the move plays again.
    #[test]
    fn a_lingering_effect_stays_on_a_while() {
        use crate::player_menu::force_pose::{HOVER_TAKEOVER, SETTLE_TAKEOVER};
        const TRICK: [Move; 1] = [Move {
            animation: "BOTH_FORCEPUSH",
            keep: 0.0,
            hold: 3.0,
            takeover: HOVER_TAKEOVER,
            settle: SETTLE_TAKEOVER,
            effect: true,
            linger: 2.0,
        }];
        let at = |seconds| script_at(&TRICK, |_| 0.5, seconds);
        assert!(at(0.2).effect);
        assert!(at(1.0).effect && at(1.0).animation.is_none());
        assert!(at(2.4).effect);
        assert!(!at(2.6).effect);
        assert!(at(3.6).effect && at(3.6).animation.is_some());
    }

    /// Sense's shell shows a while, goes off for the wait, and shows again.
    #[test]
    fn a_lone_shell_goes_on_and_off() {
        use crate::player_menu::force_pose::{PREVIEW_PAUSE, SPEED_ON};
        assert!(lone_shell_on(0.0));
        assert!(lone_shell_on(SPEED_ON - 0.01));
        assert!(!lone_shell_on(SPEED_ON + 0.01));
        assert!(lone_shell_on(SPEED_ON + PREVIEW_PAUSE + 0.01));
    }

    /// A held move plays as long as the preview lasts.
    #[test]
    fn a_held_move_never_ends() {
        use crate::player_menu::force_pose::HOVER_TAKEOVER;
        const HELD: [Move; 1] = [Move {
            animation: "BOTH_FORCEGRIP_HOLD",
            keep: f32::INFINITY,
            hold: 0.0,
            takeover: HOVER_TAKEOVER,
            settle: HOVER_TAKEOVER,
            effect: true,
            linger: 0.0,
        }];
        for seconds in [0.0, 5.0, 500.0] {
            let step = script_at(&HELD, |_| 0.5, seconds);
            assert_eq!(step.animation, Some("BOTH_FORCEGRIP_HOLD"));
            assert!(step.effect);
        }
    }

    /// The throwing arm is out while the saber is, then rests with it in the hand.
    #[test]
    fn the_arm_stays_out_while_the_saber_flies() {
        assert!(saber_out_at(0.0));
        assert!(saber_out_at(THROW_OUT + THROW_BACK - 0.01));
        assert!(!saber_out_at(THROW_OUT + THROW_BACK + 0.01));
        assert!(saber_out_at(THROW_OUT + THROW_BACK + THROW_REST + 0.01));
    }

    /// The blades go off at the take-off, stay off in the air and light again on
    /// landing, out for the stand before the next jump.
    #[test]
    fn the_blades_go_off_for_the_jump() {
        let airborne = airborne_seconds(130.0);
        assert_eq!(blade_in_at(130.0, 0.0), 0.0);
        assert!(blade_in_at(130.0, BLADE_OFF_SECONDS / 2.0) > 0.4);
        assert_eq!(blade_in_at(130.0, airborne * 0.5), 1.0);
        assert_eq!(blade_in_at(130.0, airborne - 0.01), 1.0);
        let lighting = blade_in_at(130.0, airborne + BLADE_ON_SECONDS / 2.0);
        assert!(lighting > 0.0 && lighting < 1.0);
        assert_eq!(blade_in_at(130.0, airborne + BLADE_ON_SECONDS), 0.0);
    }

    /// Out along the throw, spinning flat, back into the hand, then a rest in it.
    #[test]
    fn the_saber_flies_out_and_back_to_the_hand() {
        let hand = Pose {
            grip: Vec3::new(10.0, 0.0, 30.0),
            rotation: Quat::IDENTITY,
        };
        let out = throw_at(hand, Vec3::Y, Vec3::X, THROW_OUT).unwrap();
        assert!((out.grip.x - (10.0 + THROW_REACH)).abs() < 1e-3);
        // Fast at first, slowing to its far end: past half way at half the time, and
        // hardly moving near the end.
        let mid = throw_at(hand, Vec3::Y, Vec3::X, THROW_OUT * 0.5).unwrap();
        assert!(mid.grip.x > 10.0 + THROW_REACH * 0.8);
        let late = throw_at(hand, Vec3::Y, Vec3::X, THROW_OUT * 0.9).unwrap();
        assert!(out.grip.x - late.grip.x < 1.0);
        let caught = throw_at(hand, Vec3::Y, Vec3::X, THROW_OUT + THROW_BACK - 1e-4).unwrap();
        assert!(caught.grip.distance(hand.grip) < 0.1);
        assert!(caught.rotation.angle_between(hand.rotation) < 0.01);
        assert_eq!(
            throw_at(hand, Vec3::Y, Vec3::X, THROW_OUT + THROW_BACK + 0.1),
            None
        );
    }
}
