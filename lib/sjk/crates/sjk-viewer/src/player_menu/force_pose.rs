//! What the live model does on the SJK UI's Force page for the power under the
//! pointer: the gesture the game plays when that power is used, looped, so the page
//! shows the power as well as describing it.
//!
//! Where multiplayer plays a gesture it is that one (`PM_Weapon`'s hand extensions,
//! `codemp/game/bg_pmove.c`, set by `w_force.c`): Push, and Mind Trick, which stock
//! plays as a push (`ForceTelepathy`), `BOTH_FORCEPUSH`; Pull `BOTH_FORCEPULL`; Grip
//! `BOTH_FORCEGRIP_HOLD`; Lightning `BOTH_FORCELIGHTNING_HOLD`; Drain
//! `BOTH_FORCE_DRAIN_HOLD`, as the JoF server plays it. Heal, Rage, Protect and Absorb
//! have none in multiplayer, only their shells; the models carry single player's poses
//! for them (`BOTH_FORCEHEAL_START`, `BOTH_FORCE_RAGE`, `BOTH_FORCE_PROTECT`,
//! `BOTH_FORCE_ABSORB`, which `CG_Player` also knows), and the team powers take the
//! quick heal's. Speed runs (`BOTH_RUN1`), so its trail stands behind the model; Saber
//! Throw holds `BOTH_SABERPULL`; Jump plays a Force jump's poses (its [`Effect`]);
//! Sense keeps the stance and wears its shell. Saber Offense swings the attacks one after
//! another, each back to the stance (`BOTH_A3_T__B_` then `BOTH_R3_B__S1`, and on round
//! the quadrants), and Saber Defense raises the parries in turn (`BOTH_P1_S1_T_`, `_TR`,
//! `_TL`, `_BL`, `_BR`): with a single saber both in the strong (red) style, back to its
//! stance `BOTH_SABERSLOW_STANCE` between them (Creyon, 13/10/2026); the staff and the
//! dual sabers in their own.
//! A model without the sequence keeps the stance (`set_stage_stance`).
//!
//! The model also wears the power's shell ([`shell`]) and gives off its effect
//! ([`effect`]), drawn in the preview by `menu_stage::force_fx`.

use super::saber::SaberStyle;
use sjk_client::ForcePower;

/// How a shell's colour reaches its shader, as `force_overlays.rs` passes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tint {
    /// `shaderRGBA` for the shader's `rgbGen entity`.
    Shader,
    /// `RF_RGB_TINT`: the colour replaces the stages' own.
    Rgb,
    /// `RF_RGB_TINT` and `RF_FORCE_ENT_ALPHA`: colour and alpha both replaced.
    Rgba,
}

/// A Force shell the live model wears, as `CG_Player` draws it over a player using the
/// power (`sjk_client::legacy_force_overlays`): the body drawn again with `shader`,
/// tinted `rgba` as `tint` says.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Shell {
    pub(crate) shader: &'static str,
    pub(crate) rgba: [u8; 4],
    pub(crate) tint: Tint,
}

/// The shell `power` puts on its user, where it has one: Protect's green
/// `forceprotect`, Absorb's blue `personalshield`, Rage's red electric body, the team
/// powers' `ysalimarishell` (green for Team Heal, blue for Team Energize, at the
/// strength a fresh one starts at: `EV_TEAM_POWER` gives it a second, alpha a
/// millisecond over 8), and Sense's `sightshell` in free-for-all's yellow, which is how
/// its user sees the others.
pub(crate) fn shell(power: ForcePower) -> Option<Shell> {
    let (shader, rgba, tint) = match power {
        ForcePower::Protect => ("gfx/misc/forceprotect", [0, 128, 0, 254], Tint::Shader),
        ForcePower::Absorb => ("gfx/misc/personalshield", [0, 0, 255, 254], Tint::Shader),
        ForcePower::Rage => ("gfx/misc/fullbodyelectric2", [255, 0, 0, 255], Tint::Rgb),
        ForcePower::TeamHeal => ("powerups/ysalimarishell", [0, 255, 0, 125], Tint::Rgba),
        ForcePower::TeamForce => ("powerups/ysalimarishell", [0, 0, 255, 125], Tint::Rgba),
        ForcePower::See => ("powerups/sightshell", [255, 255, 0, 255], Tint::Shader),
        _ => return None,
    };
    Some(Shell { shader, rgba, tint })
}

/// The animation (by `animation.cfg` name) the live model loops for `power`; `None`
/// keeps the stance. Jump, Saber Throw and the saber skills Offense and Defense are
/// driven by their [`Effect`]s.
pub(super) fn pose(power: ForcePower, _style: SaberStyle) -> Option<&'static str> {
    Some(match power {
        ForcePower::Push | ForcePower::Telepathy => "BOTH_FORCEPUSH",
        ForcePower::Pull => "BOTH_FORCEPULL",
        ForcePower::Grip => "BOTH_FORCEGRIP_HOLD",
        // What the JoF server plays while draining (Creyon's trace, 12/10/2026).
        ForcePower::Drain => "BOTH_FORCE_DRAIN_HOLD",
        ForcePower::Lightning => "BOTH_FORCELIGHTNING_HOLD",
        ForcePower::Heal => "BOTH_FORCEHEAL_START",
        ForcePower::TeamHeal | ForcePower::TeamForce => "BOTH_FORCEHEAL_QUICK",
        // Running, so the speed trail's copies stand behind the model.
        ForcePower::Speed => "BOTH_RUN1",
        ForcePower::Rage => "BOTH_FORCE_RAGE",
        ForcePower::Protect => "BOTH_FORCE_PROTECT",
        ForcePower::Absorb => "BOTH_FORCE_ABSORB",
        ForcePower::SaberThrow => "BOTH_SABERPULL",
        ForcePower::Levitation
        | ForcePower::See
        | ForcePower::SaberOffense
        | ForcePower::SaberDefense => return None,
    })
}

/// How a new animation takes over from the old when the pointer moves to another power:
/// a quarter of a second, eased in and out.
pub(crate) const HOVER_TAKEOVER: Takeover = Takeover {
    seconds: 0.25,
    eased: true,
};

/// How a gesture's end hands over to the stance: at an even pace from its last frame,
/// at once. Push and Pull end still moving; keeping their last frame a moment, then
/// easing out of it, stopped the hand dead before it went back to the saber (Creyon,
/// 13/10/2026: "freezes a small moment before grabbing the saber").
pub(crate) const GESTURE_SETTLE: Takeover = Takeover {
    seconds: 0.3,
    eased: false,
};

/// A gesture played once, straight back into the stance, and the stance for
/// [`PREVIEW_PAUSE`] before it plays again: one use of the power, then a wait. Played
/// back to back, Push, Pull, Mind Trick, the team powers and Absorb were used over and
/// over (Creyon, 13/10/2026).
const fn once(animation: &'static str) -> [Move; 1] {
    lasting(animation, 0.0)
}

/// [`once`], the power's effect and shell staying on `linger` seconds into the stance
/// after the gesture, then off until it is used again (Creyon, 13/10/2026: Mind Trick's
/// confusion, Protect's and Absorb's shells "turning off after some time and then when
/// the preview starts over again it turns on").
const fn lasting(animation: &'static str, linger: f32) -> [Move; 1] {
    [Move {
        animation,
        keep: 0.0,
        hold: linger + PREVIEW_PAUSE,
        takeover: HOVER_TAKEOVER,
        settle: GESTURE_SETTLE,
        effect: true,
        linger,
    }]
}

/// Seconds a lasting power stays on after its gesture.
const POWER_LINGER: f32 = 2.5;

const PUSH: [Move; 1] = once("BOTH_FORCEPUSH");
/// Mind Trick: the push gesture, its confusion over the head staying on a while after
/// and then gone before it is used again (Creyon, 13/10/2026: "the mind trick effect
/// should stay on for some seconds then goes away").
const MIND_TRICK: [Move; 1] = lasting("BOTH_FORCEPUSH", POWER_LINGER);
const PULL: [Move; 1] = once("BOTH_FORCEPULL");
/// Grip: the hand held out gripping a while (`BOTH_FORCEGRIP_HOLD`, its puffs on), let
/// go (`BOTH_FORCEGRIP_RELEASE`), then the wait, and again (Creyon, 13/10/2026: "grip
/// isnt stopping").
const GRIP: [Move; 2] = [
    Move {
        animation: "BOTH_FORCEGRIP_HOLD",
        keep: HOLD_ON,
        hold: 0.0,
        takeover: HOVER_TAKEOVER,
        settle: HOVER_TAKEOVER,
        effect: true,
        linger: 0.0,
    },
    Move {
        animation: "BOTH_FORCEGRIP_RELEASE",
        keep: 0.0,
        hold: PREVIEW_PAUSE,
        takeover: GAME_TAKEOVER,
        settle: GESTURE_SETTLE,
        effect: false,
        linger: 0.0,
    },
];
/// Drain: the hand comes up (`BOTH_FORCE_DRAIN_START`), drains a while (`_HOLD`, its
/// beam on), lets go (`_RELEASE`), then the wait, and again (Creyon, 13/10/2026: "drain
/// isnt even stopping and redoing the preview").
const DRAIN: [Move; 3] = [
    Move {
        animation: "BOTH_FORCE_DRAIN_START",
        keep: 0.0,
        hold: 0.0,
        takeover: HOVER_TAKEOVER,
        settle: HOVER_TAKEOVER,
        effect: true,
        linger: 0.0,
    },
    Move {
        animation: "BOTH_FORCE_DRAIN_HOLD",
        keep: HOLD_ON,
        hold: 0.0,
        takeover: GAME_TAKEOVER,
        settle: GAME_TAKEOVER,
        effect: true,
        linger: 0.0,
    },
    Move {
        animation: "BOTH_FORCE_DRAIN_RELEASE",
        keep: 0.0,
        hold: PREVIEW_PAUSE,
        takeover: GAME_TAKEOVER,
        settle: GESTURE_SETTLE,
        effect: false,
        linger: 0.0,
    },
];
/// Lightning: the hands come up (`BOTH_FORCELIGHTNING_START`), the beam held a while
/// (`_HOLD`), let go (`_RELEASE`), then the wait, and again, rather than held as long as
/// the pointer stays (Creyon, 13/10/2026: "Rage and Lightning also need to stop after
/// some time and redo when the preview restarts").
const LIGHTNING: [Move; 3] = [
    Move {
        animation: "BOTH_FORCELIGHTNING_START",
        keep: 0.0,
        hold: 0.0,
        takeover: HOVER_TAKEOVER,
        settle: HOVER_TAKEOVER,
        effect: true,
        linger: 0.0,
    },
    Move {
        animation: "BOTH_FORCELIGHTNING_HOLD",
        keep: HOLD_ON,
        hold: 0.0,
        takeover: GAME_TAKEOVER,
        settle: GAME_TAKEOVER,
        effect: true,
        linger: 0.0,
    },
    Move {
        animation: "BOTH_FORCELIGHTNING_RELEASE",
        keep: 0.0,
        hold: PREVIEW_PAUSE,
        takeover: GAME_TAKEOVER,
        settle: GESTURE_SETTLE,
        effect: false,
        linger: 0.0,
    },
];
/// Seconds Grip, Drain and Lightning are held in their previews.
const HOLD_ON: f32 = 2.5;
/// The team powers and Rage stay on a while after their gesture, as Protect does: their
/// shells (and Rage's spray) were gone as soon as the gesture ended (Creyon, 13/10/2026:
/// "the effect of rage, team heal and team energize is too short").
const QUICK_HEAL: [Move; 1] = lasting("BOTH_FORCEHEAL_QUICK", POWER_LINGER);
/// Speed: the run, fast with the two copies behind it for [`SPEED_ON`], then at a
/// run's own pace without them for [`PREVIEW_PAUSE`], and on again (Creyon, 13/10/2026:
/// "turning it off also slows the speed and when turning it on increases the speed").
const RUN: [Move; 2] = [
    Move {
        animation: "BOTH_RUN1",
        keep: SPEED_ON,
        hold: 0.0,
        takeover: HOVER_TAKEOVER,
        settle: HOVER_TAKEOVER,
        effect: true,
        linger: 0.0,
    },
    Move {
        animation: "BOTH_RUN1",
        keep: PREVIEW_PAUSE,
        hold: 0.0,
        takeover: HOVER_TAKEOVER,
        settle: HOVER_TAKEOVER,
        effect: false,
        linger: 0.0,
    },
];
/// Seconds Speed stays on in its preview.
pub(crate) const SPEED_ON: f32 = 3.0;
/// Rage's gesture is long (almost 3 s): its shell and spray stay on a second after it,
/// then go off for the wait, so the preview is seen to end and start again (Creyon,
/// 13/10/2026).
const RAGE: [Move; 1] = lasting("BOTH_FORCE_RAGE", 1.0);
const PROTECT: [Move; 1] = lasting("BOTH_FORCE_PROTECT", POWER_LINGER);
const ABSORB: [Move; 1] = lasting("BOTH_FORCE_ABSORB", POWER_LINGER);
/// Heal whole, as single player's models play it: down onto a knee
/// (`BOTH_FORCEHEAL_START`), healing there a while, back up (`BOTH_FORCEHEAL_STOP`),
/// then a wait (Creyon, 13/10/2026: "make the movement complete in heal").
const HEAL: [Move; 2] = [
    Move {
        animation: "BOTH_FORCEHEAL_START",
        keep: 1.2,
        hold: 0.0,
        takeover: HOVER_TAKEOVER,
        settle: HOVER_TAKEOVER,
        effect: true,
        linger: 0.0,
    },
    Move {
        animation: "BOTH_FORCEHEAL_STOP",
        keep: 0.0,
        hold: PREVIEW_PAUSE,
        takeover: SETTLE_TAKEOVER,
        settle: SETTLE_TAKEOVER,
        effect: false,
        linger: 0.0,
    },
];

/// What the live model plays for `power` ([`pose`]'s animation, scripted): once and a
/// wait, held, or Heal's whole move; `None` keeps the stance.
pub(super) fn gesture(power: ForcePower, style: SaberStyle) -> Option<&'static [Move]> {
    pose(power, style)?;
    Some(match power {
        ForcePower::Push => &PUSH,
        ForcePower::Telepathy => &MIND_TRICK,
        ForcePower::Pull => &PULL,
        ForcePower::Grip => &GRIP,
        ForcePower::Drain => &DRAIN,
        ForcePower::Lightning => &LIGHTNING,
        ForcePower::Heal => &HEAL,
        ForcePower::TeamHeal | ForcePower::TeamForce => &QUICK_HEAL,
        ForcePower::Speed => &RUN,
        ForcePower::Rage => &RAGE,
        ForcePower::Protect => &PROTECT,
        ForcePower::Absorb => &ABSORB,
        // Saber Throw's arm follows the saber ([`Effect::Throw`]).
        _ => return None,
    })
}

/// How high each level's Force jump goes: `forceJumpHeight` plus the step height and
/// the crouch difference, as `bg_pmove.c` notes beside it (66, 130, 226, 418).
pub(crate) const JUMP_HEIGHTS: [f32; 4] = [66.0, 130.0, 226.0, 418.0];

/// What the live model gives off for a power, as `CG_Player` draws it on a player
/// using it (`force_power_submission.rs` does it in the world).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Effect {
    /// A beam from the left hand along the aim, re-played on the 8 ms cadence:
    /// Lightning's `force/lightning` (`force/lightningwide` at level 3) and Drain's
    /// `mp/drain` (`mp/drainwide`), as `activeForcePass` picks them.
    Beam(&'static str),
    /// `CG_ForcePushBlur` puffs at the left hand every frame: Push and Pull while the
    /// hand is out, Grip's red and white pair twice (`CG_ForceGripEffect`), and JoF
    /// EJK's quick heal (`CG_ForceAnimFx`, `BOTH_FORCEHEAL_QUICK`) for the team powers.
    Puffs { grip: bool },
    /// JoF EJK's `CG_ForceAnimFx` spray from the chest every 120 ms while the pose
    /// plays: `force/rage2` for Rage, `force/heal2` for Heal.
    Chest(&'static str),
    /// `force/confusion_old` over the head: what a mind trick looks like to the one
    /// tricked (`CG_Player`).
    OverHead(&'static str),
    /// `cg_speedTrail`: two see-through copies of the body behind the runner, at
    /// alpha 100 and 50.
    SpeedTrail,
    /// A Force jump's poses, timed as a jump to `height` lasts, played in place.
    Jump { height: f32 },
    /// The saber thrown out spinning and caught again.
    Throw,
    /// Animations played one after another, each once, and again from the first,
    /// `stance` held between them.
    Moves {
        moves: &'static [Move],
        stance: &'static str,
    },
}

/// How a move takes over from the pose before it: over `seconds`, eased in and out or
/// at an even pace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Takeover {
    pub(crate) seconds: f32,
    pub(crate) eased: bool,
}

/// The game's takeover between a player's animations: 100 ms at an even pace, as
/// Ghoul2's `BONE_ANIM_BLEND` mixes them (`legacy_blend_millis` in
/// `sjk-client`). The saber skills' swings flow into one another so; a longer, eased
/// takeover slowed each one as it began and hurried it after (Creyon, 13/10/2026: "weird
/// flickering and movement in the offense").
pub(crate) const GAME_TAKEOVER: Takeover = Takeover {
    seconds: 0.1,
    eased: false,
};

/// A parry's takeover: the parries are two frames each, so the blend into one is the
/// whole of the block's movement, and the game's 100 ms made it a snap (Creyon,
/// 13/10/2026: "the blocking is going too fast, must be more natural").
pub(crate) const PARRY_TAKEOVER: Takeover = Takeover {
    seconds: 0.25,
    eased: true,
};

/// A swing's start from the stance, after the return before it: the return ends and
/// the start begins all but still, a few units apart (seven between `BOTH_R3_B__S1` and
/// `BOTH_S3_S1_TL`), so the game's 100 ms made the saber jerk there (Creyon,
/// 13/10/2026: "the second swing has some weird stuttering in the saber"); eased over
/// a fifth of a second it settles.
pub(crate) const START_TAKEOVER: Takeover = Takeover {
    seconds: 0.25,
    eased: true,
};

/// How the stance takes over after a return, before it holds: the return ends all but
/// still, and the stance stands a little off where it ends.
pub(crate) const SETTLE_TAKEOVER: Takeover = Takeover {
    seconds: 0.25,
    eased: true,
};

/// One animation of [`Effect::Moves`]: how long its last frame is kept after it plays,
/// how long the stance holds after that, how it takes over from the pose before it,
/// and how the stance takes over from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Move {
    pub(crate) animation: &'static str,
    pub(crate) keep: f32,
    pub(crate) hold: f32,
    pub(crate) takeover: Takeover,
    pub(crate) settle: Takeover,
    /// Whether the power's effect (puffs, spray, beam) plays while this move does.
    pub(crate) effect: bool,
    /// Seconds the effect stays on into the stance after it.
    pub(crate) linger: f32,
}

const fn swing(animation: &'static str) -> Move {
    Move {
        animation,
        keep: 0.0,
        hold: 0.0,
        takeover: GAME_TAKEOVER,
        settle: GAME_TAKEOVER,
        effect: false,
        linger: 0.0,
    }
}

/// A swing's start from the stance ([`START_TAKEOVER`]).
const fn start(animation: &'static str) -> Move {
    Move {
        animation,
        keep: 0.0,
        hold: 0.0,
        takeover: START_TAKEOVER,
        settle: START_TAKEOVER,
        effect: false,
        linger: 0.0,
    }
}

/// A return, then the skill's stance for `pause` before the next swing starts: one
/// swing at a time, as the strong style fights, each back to the stance (Creyon,
/// 13/10/2026: "when 1 swing is done it should return to its idle state"; waiting where
/// the return ended looked frozen).
const fn back(animation: &'static str, pause: f32) -> Move {
    Move {
        animation,
        keep: 0.0,
        hold: pause,
        takeover: GAME_TAKEOVER,
        settle: SETTLE_TAKEOVER,
        effect: false,
        linger: 0.0,
    }
}

/// Seconds the model waits between two swings.
pub(crate) const SWING_PAUSE: f32 = 0.3;

/// Seconds a preview waits once it has played before it plays again (Creyon,
/// 13/10/2026: "not every preview should spam after each other. when the first preview is
/// done wait a few seconds like 2-3").
pub(crate) const PREVIEW_PAUSE: f32 = 1.5;

/// Seconds the stance holds between two parries.
const PARRY_PAUSE: f32 = 0.3;

/// A parry raised over [`PARRY_TAKEOVER`], kept a moment as a block is held, then the
/// stance again before the next.
const fn parry(animation: &'static str, hold: f32) -> Move {
    Move {
        animation,
        keep: 0.3,
        hold,
        takeover: PARRY_TAKEOVER,
        settle: PARRY_TAKEOVER,
        effect: false,
        linger: 0.0,
    }
}

/// Each style's first three attacks (down from the top, from the top left, from the
/// left), looped (Creyon, 13/10/2026), as the game chains one from the stance
/// (`PM_SaberAnimTransitionAnim`, `saberMoveData`): the start from the stance
/// (`LS_S_*`), the attack (`LS_A_*`) and the return to the stance (`LS_R_*`). Without
/// the start the attack began from a pose the stance never passes through, and the
/// blend between them moved oddly (Creyon, 13/10/2026). The single saber swings in the
/// strong (red) style, as Creyon asked (12/10/2026); the staff and the dual sabers swing
/// in their own, which have no start of their own in the game's animation set.
const SINGLE_ATTACKS: [Move; 9] = [
    start("BOTH_S3_S1_T_"),
    swing("BOTH_A3_T__B_"),
    back("BOTH_R3_B__S1", SWING_PAUSE),
    // The A+S swing: moving left and back while attacking (`PM_SaberAttackForMovement`'s
    // `LS_A_BL2TR`), up from the bottom left (Creyon, 13/10/2026).
    start("BOTH_S3_S1_BL"),
    swing("BOTH_A3_BL_TR"),
    back("BOTH_R3_TR_S1", SWING_PAUSE),
    start("BOTH_S3_S1__L"),
    swing("BOTH_A3__L__R"),
    back("BOTH_R3__R_S1", PREVIEW_PAUSE),
];
const STAFF_ATTACKS: [Move; 6] = [
    swing("BOTH_A7_T__B_"),
    back("BOTH_R7_B__S7", SWING_PAUSE),
    swing("BOTH_A7_TL_BR"),
    back("BOTH_R7_BR_S7", SWING_PAUSE),
    swing("BOTH_A7__L__R"),
    back("BOTH_R7__R_S7", PREVIEW_PAUSE),
];
const DUAL_ATTACKS: [Move; 6] = [
    swing("BOTH_A6_T__B_"),
    back("BOTH_R6_B__S6", SWING_PAUSE),
    swing("BOTH_A6_TL_BR"),
    back("BOTH_R6_BR_S6", SWING_PAUSE),
    swing("BOTH_A6__L__R"),
    back("BOTH_R6__R_S6", PREVIEW_PAUSE),
];

/// Each style's parries, top then round the corners (`LS_PARRY_UP`, `_UR`, `_UL`,
/// `_LL`, `_LR`): a single saber's are one set for every style, the strong one's
/// included; the staff and the dual sabers have their own (`PM_SetSaberMove`).
const SINGLE_PARRIES: [Move; 5] = [
    parry("BOTH_P1_S1_T_", PARRY_PAUSE),
    parry("BOTH_P1_S1_TR", PARRY_PAUSE),
    parry("BOTH_P1_S1_TL", PARRY_PAUSE),
    parry("BOTH_P1_S1_BL", PARRY_PAUSE),
    parry("BOTH_P1_S1_BR", PREVIEW_PAUSE),
];
const STAFF_PARRIES: [Move; 5] = [
    parry("BOTH_P7_S7_T_", PARRY_PAUSE),
    parry("BOTH_P7_S7_TR", PARRY_PAUSE),
    parry("BOTH_P7_S7_TL", PARRY_PAUSE),
    parry("BOTH_P7_S7_BL", PARRY_PAUSE),
    parry("BOTH_P7_S7_BR", PREVIEW_PAUSE),
];
const DUAL_PARRIES: [Move; 5] = [
    parry("BOTH_P6_S6_T_", PARRY_PAUSE),
    parry("BOTH_P6_S6_TR", PARRY_PAUSE),
    parry("BOTH_P6_S6_TL", PARRY_PAUSE),
    parry("BOTH_P6_S6_BL", PARRY_PAUSE),
    parry("BOTH_P6_S6_BR", PREVIEW_PAUSE),
];

/// The effect `power` shows at `level` (0 to 3), with the sabers held in `style`, if any.
pub(super) fn effect(power: ForcePower, level: u8, style: SaberStyle) -> Option<Effect> {
    let wide = level >= 3;
    Some(match power {
        ForcePower::Lightning => Effect::Beam(if wide {
            "force/lightningwide"
        } else {
            "force/lightning"
        }),
        ForcePower::Drain => Effect::Beam(if wide { "mp/drainwide" } else { "mp/drain" }),
        ForcePower::Push | ForcePower::Pull | ForcePower::TeamHeal | ForcePower::TeamForce => {
            Effect::Puffs { grip: false }
        }
        ForcePower::Grip => Effect::Puffs { grip: true },
        ForcePower::Rage => Effect::Chest("force/rage2"),
        ForcePower::Heal => Effect::Chest("force/heal2"),
        ForcePower::Telepathy => Effect::OverHead("force/confusion_old"),
        ForcePower::Speed => Effect::SpeedTrail,
        ForcePower::Levitation => Effect::Jump {
            height: JUMP_HEIGHTS[usize::from(level.min(3))],
        },
        ForcePower::SaberThrow => Effect::Throw,
        ForcePower::SaberOffense => Effect::Moves {
            moves: match style {
                SaberStyle::Single => &SINGLE_ATTACKS,
                SaberStyle::Staff => &STAFF_ATTACKS,
                SaberStyle::Dual => &DUAL_ATTACKS,
            },
            stance: skill_stance(style),
        },
        ForcePower::SaberDefense => Effect::Moves {
            moves: match style {
                SaberStyle::Single => &SINGLE_PARRIES,
                SaberStyle::Staff => &STAFF_PARRIES,
                SaberStyle::Dual => &DUAL_PARRIES,
            },
            stance: skill_stance(style),
        },
        _ => return None,
    })
}

/// The stance the saber skills swing and block from and come back to between their
/// moves: with a single saber the strong (red) style's (Creyon, 13/10/2026: "the saber
/// offense in red idle stance when not swinging, not in medium"), the staff and the dual
/// sabers their own idle stances.
fn skill_stance(style: SaberStyle) -> &'static str {
    match style {
        SaberStyle::Single => "BOTH_SABERSLOW_STANCE",
        SaberStyle::Staff | SaberStyle::Dual => style.stance(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect_single(power: ForcePower, level: u8) -> Option<Effect> {
        effect(power, level, SaberStyle::Single)
    }

    /// Each power's script plays its pose; one-shot ones wait before playing again,
    /// held ones never end, and Heal goes down, heals and gets up.
    #[test]
    fn gestures_play_the_pose_then_wait() {
        for power in ForcePower::ALL {
            let Some(moves) = gesture(power, SaberStyle::Single) else {
                assert!(
                    matches!(power, ForcePower::SaberThrow)
                        || pose(power, SaberStyle::Single).is_none()
                );
                continue;
            };
            assert!(
                moves
                    .iter()
                    .any(|step| Some(step.animation) == pose(power, SaberStyle::Single)),
                "{power:?}"
            );
            // A wait with the power off before it is used again: the stance after the
            // last move, or Speed's run at its own pace.
            let last = moves[moves.len() - 1];
            let off = if last.effect { 0.0 } else { last.keep };
            assert!(
                last.keep.is_infinite() || last.hold - last.linger + off >= PREVIEW_PAUSE,
                "{power:?}"
            );
        }
        // Lightning, Grip and Drain hold a while, let go and wait, and again.
        for power in [ForcePower::Lightning, ForcePower::Grip, ForcePower::Drain] {
            let moves = gesture(power, SaberStyle::Single).unwrap();
            let last = moves[moves.len() - 1];
            assert!(last.animation.ends_with("_RELEASE"), "{power:?}");
            assert!(!last.effect && last.hold >= PREVIEW_PAUSE, "{power:?}");
            assert!(moves.iter().all(|step| step.keep.is_finite()), "{power:?}");
        }
        // Protect's and Absorb's shells and Mind Trick's confusion stay on a while after
        // the gesture, then go off; Speed runs fast with its copies, then slower without.
        for power in [
            ForcePower::Protect,
            ForcePower::Absorb,
            ForcePower::Telepathy,
            ForcePower::Rage,
            ForcePower::TeamHeal,
            ForcePower::TeamForce,
        ] {
            let step = gesture(power, SaberStyle::Single).unwrap()[0];
            assert!(step.linger > 0.0 && step.hold > step.linger, "{power:?}");
        }
        let run = gesture(ForcePower::Speed, SaberStyle::Single).unwrap();
        assert!(run[0].effect && !run[1].effect);
        assert_eq!(run[0].animation, run[1].animation);
        let heal = gesture(ForcePower::Heal, SaberStyle::Single).unwrap();
        assert_eq!(heal[1].animation, "BOTH_FORCEHEAL_STOP");
        assert!(heal[0].effect && !heal[1].effect);
    }

    #[test]
    fn multiplayer_gestures_are_the_games_own() {
        let pose = |power| pose(power, SaberStyle::Single);
        assert_eq!(pose(ForcePower::Push), Some("BOTH_FORCEPUSH"));
        assert_eq!(pose(ForcePower::SaberOffense), None);
        assert_eq!(pose(ForcePower::SaberDefense), None);
        // `ForceTelepathy` extends the hand as a push.
        assert_eq!(pose(ForcePower::Telepathy), Some("BOTH_FORCEPUSH"));
        assert_eq!(pose(ForcePower::Pull), Some("BOTH_FORCEPULL"));
        assert_eq!(pose(ForcePower::Drain), Some("BOTH_FORCE_DRAIN_HOLD"));
        assert_eq!(
            pose(ForcePower::Lightning),
            Some("BOTH_FORCELIGHTNING_HOLD")
        );
        assert_eq!(pose(ForcePower::See), None);
    }

    /// Every power shows something: a pose, a shell or an effect.
    #[test]
    fn every_power_shows_something() {
        for power in ForcePower::ALL {
            for style in [SaberStyle::Single, SaberStyle::Staff, SaberStyle::Dual] {
                let shown = pose(power, style).is_some()
                    || shell(power).is_some()
                    || effect(power, 1, style).is_some();
                assert!(shown, "{power:?} {style:?}");
            }
        }
    }

    /// The saber skills' animations are all the game's, each attack followed by the
    /// return from where it ends, in every style.
    #[test]
    fn the_saber_skills_play_the_games_swings_and_parries() {
        let names = sjk_game_jka::legacy_animation::NAMES;
        for style in [SaberStyle::Single, SaberStyle::Staff, SaberStyle::Dual] {
            for power in [ForcePower::SaberOffense, ForcePower::SaberDefense] {
                let Some(Effect::Moves { moves, stance }) = effect(power, 1, style) else {
                    panic!("{power:?} {style:?}");
                };
                assert!(names.contains(&stance), "{stance}");
                for step in moves {
                    assert!(names.contains(&step.animation), "{}", step.animation);
                }
            }
            let Some(Effect::Moves { moves: attacks, .. }) =
                effect(ForcePower::SaberOffense, 1, style)
            else {
                unreachable!();
            };
            // A start from the stance leads into the attack from where it ends:
            // `BOTH_S3_S1_TL` into `BOTH_A3_TL_BR`.
            for pair in attacks.windows(2) {
                if let Some(quadrant) = pair[0].animation.strip_prefix("BOTH_S3_S1_") {
                    assert!(
                        pair[1]
                            .animation
                            .starts_with(&format!("BOTH_A3_{quadrant}"))
                    );
                }
            }
            let swings = attacks
                .iter()
                .filter(|step| !step.animation.starts_with("BOTH_S"))
                .copied()
                .collect::<Vec<_>>();
            for pair in swings.chunks(2) {
                // `BOTH_A2_TL_BR` ends bottom right, where `BOTH_R2_BR_S1` returns from.
                let end = &pair[0].animation[pair[0].animation.len() - 2..];
                assert!(pair[1].animation.contains(&format!("_{end}_")), "{pair:?}");
            }
        }
        // With a single saber the skills swing and block from the strong style's stance
        // and come back to it; the staff and the dual sabers to their own.
        for style in [SaberStyle::Single, SaberStyle::Staff, SaberStyle::Dual] {
            for power in [ForcePower::SaberOffense, ForcePower::SaberDefense] {
                let Some(Effect::Moves { stance, .. }) = effect(power, 1, style) else {
                    unreachable!();
                };
                let expected = match style {
                    SaberStyle::Single => "BOTH_SABERSLOW_STANCE",
                    _ => style.stance(),
                };
                assert_eq!(stance, expected);
            }
        }
    }

    #[test]
    fn beams_widen_at_level_three() {
        assert_eq!(
            effect_single(ForcePower::Lightning, 2),
            Some(Effect::Beam("force/lightning"))
        );
        assert_eq!(
            effect_single(ForcePower::Lightning, 3),
            Some(Effect::Beam("force/lightningwide"))
        );
        assert_eq!(
            effect_single(ForcePower::Drain, 1),
            Some(Effect::Beam("mp/drain"))
        );
        assert_eq!(
            effect_single(ForcePower::Drain, 3),
            Some(Effect::Beam("mp/drainwide"))
        );
        assert_eq!(
            effect_single(ForcePower::Grip, 1),
            Some(Effect::Puffs { grip: true })
        );
        assert_eq!(
            effect_single(ForcePower::Speed, 1),
            Some(Effect::SpeedTrail)
        );
        assert_eq!(
            effect_single(ForcePower::SaberThrow, 1),
            Some(Effect::Throw)
        );
    }

    /// A level 3 jump goes as high as the game's (418 units with the step and crouch).
    #[test]
    fn the_jump_goes_as_high_as_the_level() {
        assert_eq!(
            effect_single(ForcePower::Levitation, 3),
            Some(Effect::Jump { height: 418.0 })
        );
        assert_eq!(
            effect_single(ForcePower::Levitation, 1),
            Some(Effect::Jump { height: 130.0 })
        );
        assert_eq!(
            effect_single(ForcePower::Levitation, 0),
            Some(Effect::Jump { height: 66.0 })
        );
    }

    #[test]
    fn the_shells_are_the_overlays_the_game_draws() {
        assert_eq!(
            shell(ForcePower::Protect).map(|shell| shell.shader),
            Some("gfx/misc/forceprotect")
        );
        assert_eq!(
            shell(ForcePower::Absorb).map(|shell| shell.rgba),
            Some([0, 0, 255, 254])
        );
        assert!(shell(ForcePower::Rage).is_some_and(|shell| shell.tint == Tint::Rgb));
        // The team powers' shell takes its colour and alpha from the tint: green for
        // Team Heal, blue for Team Energize (`LegacyOverlayTint::Rgba`).
        let team = |power| shell(power).map(|shell| (shell.rgba, shell.tint));
        assert_eq!(
            team(ForcePower::TeamHeal),
            Some(([0, 255, 0, 125], Tint::Rgba))
        );
        assert_eq!(
            team(ForcePower::TeamForce),
            Some(([0, 0, 255, 125], Tint::Rgba))
        );
        for power in [ForcePower::Push, ForcePower::Lightning, ForcePower::Speed] {
            assert_eq!(shell(power), None, "{power:?}");
        }
    }
}
