//! `BG_G2PlayerAngles` as the server calls it: `G_G2PlayerAngles`
//! (`codemp/game/w_saber.c:895-990`), from `WP_SaberPositionUpdate`.
//!
//! The server hands the shared function fresh swing state every frame — `tYawAngle`
//! and `lYawAngle` start at the view's yaw, `tPitchAngle` at zero, the three "swinging"
//! flags cleared — so nothing but `corrTime`, `lastHeadAngles` and `lookTime` carries
//! from one frame to the next ([`AngleMemory`]). What comes out is the yaw the player's
//! skeleton is placed with (`properAngles`, which the saber's bolt is read in) and the
//! five spine commands it sets with `G2API_SetBoneAngles`.
//!
//! Sources: `bg_pmove.c:8742-9463` (`BG_UpdateLookAngles`, `BG_G2ClientNeckAngles`,
//! `BG_G2ClientSpineAngles`, `BG_SwingAngles`, `BG_G2PlayerAngles`).

use crate::player_angle_flags::{FLAGS, SPECIAL_MOVES};
use crate::player_angle_math::{
    angle_mod, angle_subtract, normalize, normalized_angle, swing_angles, vector_angles,
};

/// `FRAMETIME` (`g_local.h:54`): the frame the server's swing is timed by.
const FRAMETIME: f32 = 100.0;
/// `BOTH_STAND1`, the stance only `WeaponReadyAnim` with it lets the yaw drift in.
const BOTH_STAND1: u16 = 915;
/// `ENTITYNUM_NONE`: not standing on anything.
const ENTITYNUM_NONE: u16 = 1023;
/// `EF_DEAD`.
const EF_DEAD: u32 = 1 << 1;
/// `WP_SABER`, `WP_EMPLACED_GUN`.
const WP_SABER: u8 = 3;

/// The flag bits of [`FLAGS`].
const FLIP: u8 = 1;
const SPIN: u8 = 2;
const SPECIAL_JUMP: u8 = 4;
const DEATH: u8 = 8;
const SPECIAL_ATTACK: u8 = 16;
const KNOCKDOWN: u8 = 32;
const ROLL: u8 = 64;
const LOCK_BREAK: u8 = 128;

fn flags(animation: u16) -> u8 {
    FLAGS.get(usize::from(animation)).copied().unwrap_or(0)
}

/// `WeaponReadyAnim` (`bg_misc.c:245`), by weapon.
fn weapon_ready(weapon: u8) -> u16 {
    // TORSO_DROPWEAP1, TORSO_WEAPONREADY3, BOTH_STAND2, TORSO_WEAPONREADY2,
    // TORSO_WEAPONREADY10, BOTH_STAND1, TORSO_WEAPONREADY1.
    const TABLE: [u16; 19] = [
        1396, 1402, 1402, 917, 1401, 1402, 1402, 1402, 1402, 1402, 1402, 1402, 1404, 1404, 1404,
        1402, 1401, 915, 1400,
    ];
    TABLE.get(usize::from(weapon)).copied().unwrap_or(1402)
}

/// What the entity state says about the player, as `ent->s` holds it when the server
/// poses it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AngleInputs {
    /// `lerpOrigin`: `ps.origin`.
    pub origin: [f32; 3],
    /// `lerpAngles`: `ps.viewangles`.
    pub view: [f32; 3],
    /// `pos.trDelta`: the velocity.
    pub velocity: [f32; 3],
    /// `legsAnim`, `torsoAnim`.
    pub legs: u16,
    /// The torso's.
    pub torso: u16,
    /// `ciLegs`, `ciTorso`: the player-state animations when they can differ from
    /// the entity's, for example after an NPC takes a blow since its last move.
    /// `None` uses the entity's animations for both sets of spine-correction gates.
    pub client_animations: Option<[u16; 2]>,
    /// `weapon`.
    pub weapon: u8,
    /// `eFlags`.
    pub eflags: u32,
    /// `angles2[YAW]`: `ps.movementDir`.
    pub movement_dir: i32,
    /// `groundEntityNum`.
    pub ground: u16,
    /// `saberMove`.
    pub saber_move: u32,
    /// `forceFrame` (the saber lock's frame) or a vehicle: the skeleton is held.
    pub held: bool,
    /// Where the player looks at another entity, if it does (`ps.hasLookTarget`): the
    /// target's origin.
    pub look_target: Option<[f32; 3]>,
}

/// The three things `G_G2PlayerAngles` keeps between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AngleMemory {
    /// `client->corrTime`.
    pub corr_time: i32,
    /// `client->lastHeadAngles`.
    pub last_head_angles: [f32; 3],
    /// `client->lookTime`: head turning is clamped and eased until then.
    pub look_time: i32,
}

/// What the frame's pose is set with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerAngles {
    /// `legsAngles`, which the server poses the skeleton with (`properAngles`).
    pub legs: [f32; 3],
    /// `lower_lumbar`, `upper_lumbar`, `thoracic`, `cervical`, `cranium`, as
    /// `BONE_ANGLES_POSTMULT` commands in pitch/yaw/roll order; `None` for a bone the
    /// frame leaves as it was.
    pub bones: [Option<[f32; 3]>; 5],
}

/// `BG_G2ClientSpineAngles`' motion correction wants the `Motion` bolt when legs and
/// torso play different animations and neither is a flip, spin, special, death, roll or
/// knockdown (`bg_pmove.c:8889-8923`). Both the entity's and the `ci` tracks must
/// permit correction; their animations can differ between server moves.
pub fn corrects_for_motion(inputs: &AngleInputs) -> bool {
    let (legs, torso) = (flags(inputs.legs), flags(inputs.torso));
    let [ci_legs, ci_torso] = inputs
        .client_animations
        .unwrap_or([inputs.legs, inputs.torso]);
    let forbidden = SPIN | SPECIAL_JUMP | DEATH | SPECIAL_ATTACK | KNOCKDOWN;
    inputs.legs != inputs.torso
        && ci_legs != ci_torso
        && flags(ci_legs) & (FLIP | forbidden) == 0
        && flags(ci_torso) & forbidden == 0
        && legs & (FLIP | SPIN | SPECIAL_JUMP | DEATH | SPECIAL_ATTACK | KNOCKDOWN | ROLL) == 0
        && torso & (SPIN | SPECIAL_JUMP | DEATH | SPECIAL_ATTACK | KNOCKDOWN) == 0
        && !SPECIAL_MOVES
            .get(inputs.saber_move as usize)
            .copied()
            .unwrap_or(false)
        && inputs.eflags & EF_DEAD == 0
        && !inputs.held
}

/// The `Motion` bolt's angles from its raw matrix (`G2API_GetBoltMatrix_NoRecNoRot`,
/// rows normalized, `bg_pmove.c:8964-8982`).
pub fn motion_angles(mut matrix: [[f32; 4]; 3]) -> [f32; 3] {
    for row in &mut matrix {
        let mut basis = [row[0], row[1], row[2]];
        normalize(&mut basis);
        row[..3].copy_from_slice(&basis);
    }
    let mut angles = vector_angles([-matrix[0][1], -matrix[1][1], -matrix[2][1]]);
    angles[2] = -vector_angles([-matrix[0][0], -matrix[1][0], -matrix[2][0]])[0];
    angles
}

/// Whether `BG_G2PlayerAngles` takes its early way out (`bg_pmove.c:9119-9139`): a vehicle,
/// a held frame or a lock break — the legs facing the view, and the spine straightened for
/// a player (an NPC's keeps its angles).
pub fn held_still(inputs: &AngleInputs) -> bool {
    inputs.held || (flags(inputs.legs) | flags(inputs.torso)) & LOCK_BREAK != 0
}

/// `G_G2PlayerAngles` for a humanoid player. `motion` answers the `Motion` bolt's raw
/// matrix at the player's origin with no rotation — only called when
/// [`corrects_for_motion`] says the spine needs it.
pub fn player_angles(
    inputs: &AngleInputs,
    memory: &mut AngleMemory,
    time: i32,
    motion: impl FnOnce() -> [[f32; 4]; 3],
) -> PlayerAngles {
    // "a vehicle or riding a vehicle", a held frame, a lock break: the view's yaw and
    // roll, and every spine bone straight.
    if held_still(inputs) {
        return PlayerAngles {
            legs: [0.0, inputs.view[1], inputs.view[2]],
            bones: [Some([0.0; 3]); 5],
        };
    }
    // `G_G2PlayerAngles`' look direction: at a target, or — quirk kept — the player's
    // own origin read as angles, pitch zeroed.
    let mut look = match inputs.look_target {
        Some(target) => {
            memory.look_time = time + 1000;
            vector_angles(std::array::from_fn(|axis| {
                target[axis] - inputs.origin[axis]
            }))
        }
        None => inputs.origin,
    };
    look[0] = 0.0;
    if time + 2000 < memory.corr_time {
        memory.corr_time = 0;
    }
    // Only the legs' yaw and the spine leave this function on the server. The torso's
    // swing and the head's angles are computed and then overwritten before anything
    // reads them (`BG_G2ClientNeckAngles` writes `headAngles`), and the legs' pitch and
    // roll are zeroed after the velocity tilts them (`bg_pmove.c:9290-9301`).
    let head_yaw = angle_mod(inputs.view[1]);
    // Fresh swing state every frame, as the server passes it: the legs start at the
    // view's yaw and swing only when the stance is not the idle one.
    let mut legs_yawing = inputs.legs != BOTH_STAND1 || inputs.torso != weapon_ready(inputs.weapon);
    let mut legs_yaw = inputs.view[1];
    let direction = if inputs.eflags & EF_DEAD != 0 {
        0
    } else {
        inputs.movement_dir
    };
    let mut velocity = inputs.velocity;
    if flags(inputs.legs) & ROLL != 0
        || (inputs.weapon == WP_SABER
            && SPECIAL_MOVES
                .get(inputs.saber_move as usize)
                .copied()
                .unwrap_or(false))
    {
        velocity = [0.0; 3];
    }
    normalize(&mut velocity);
    // "crazy velocity-based leg angle calculation"
    let mut yaw = head_yaw;
    let mut toward = [
        inputs.origin[0] + velocity[0],
        inputs.origin[1] + velocity[1],
        inputs.origin[2],
    ];
    if inputs.ground == ENTITYNUM_NONE || inputs.held {
        toward = inputs.origin;
    }
    let away: [f32; 3] = std::array::from_fn(|axis| inputs.origin[axis] - toward[axis]);
    if away != [0.0; 3] {
        let away = vector_angles(away);
        let (negative, positive) = if away[1] <= yaw {
            (yaw - away[1], (360.0 - yaw) + away[1])
        } else {
            (yaw + (360.0 - away[1]), away[1] - yaw)
        };
        let (mut difference, add) = if negative < positive {
            (negative, false)
        } else {
            (positive, true)
        };
        if difference > 90.0 {
            difference = 180.0 - difference;
        }
        if difference > 60.0 {
            difference = 60.0;
        }
        // "Slight hack for when playing is running backward"
        if direction == 3 || direction == 5 {
            difference = -difference;
        }
        if add {
            yaw -= difference
        } else {
            yaw += difference
        }
    }
    swing_angles(
        yaw,
        0.0,
        90.0,
        0.65,
        &mut legs_yaw,
        &mut legs_yawing,
        FRAMETIME,
    );
    let legs = [0.0, legs_yaw, 0.0];
    // `BG_G2ClientSpineAngles`.
    let mut view = [(f64::from(inputs.view[0]) * 0.5) as f32, 0.0, 0.0];
    view[1] = angle_delta(inputs.view[1], legs[1]);
    if corrects_for_motion(inputs) {
        let motion = motion_angles(motion());
        for axis in 0..3 {
            view[axis] = normalized_angle(view[axis] - normalized_angle(motion[axis]));
        }
    }
    let mut thoracic = [view[0] * 0.20, view[1] * 0.20, view[2] * 0.20];
    let upper_lumbar = [view[0] * 0.40, view[1] * 0.35, view[2] * 0.35];
    let lower_lumbar = [view[0] * 0.40, view[1] * 0.45, view[2] * 0.45];
    // The look: its difference from the view, eased and clamped while a look lasts.
    let eye: [f32; 3] = std::array::from_fn(|axis| normalized_angle(inputs.view[axis]));
    let look: [f32; 3] =
        std::array::from_fn(|axis| angle_subtract(normalized_angle(look[axis]), eye[axis]));
    let look = update_look(memory, look, time);
    // `BG_G2ClientNeckAngles`.
    let (minimum, maximum) = ([-25.0, -55.0, -10.0], [50.0, 50.0, 10.0]);
    let clamped: [f32; 3] = std::array::from_fn(|axis| {
        let value = look[axis];
        if value < minimum[axis] {
            minimum[axis]
        } else if value > maximum[axis] {
            maximum[axis]
        } else {
            value
        }
    });
    // In double, as C evaluates `(thoracic + lA * 0.4) * 0.5f`.
    for (axis, share) in [(0, 0.4_f64), (1, 0.1), (2, 0.1)] {
        let part = f64::from(clamped[axis]) * share;
        thoracic[axis] = if thoracic[axis] != 0.0 {
            ((f64::from(thoracic[axis]) + part) * 0.5) as f32
        } else {
            part as f32
        };
    }
    let neck = [clamped[0] * 0.2, clamped[1] * 0.3, clamped[2] * 0.3];
    let cranium = [
        (f64::from(clamped[0]) * 0.4) as f32,
        (f64::from(clamped[1]) * 0.6) as f32,
        (f64::from(clamped[2]) * 0.6) as f32,
    ];
    PlayerAngles {
        legs,
        bones: [
            Some(lower_lumbar),
            Some(upper_lumbar),
            Some(thoracic),
            Some(neck),
            Some(cranium),
        ],
    }
}

/// `BG_UpdateLookAngles` with the server's limits (`lookSpeed` 1.5; pitch ±50, yaw
/// ±70, roll ±30).
fn update_look(memory: &mut AngleMemory, mut look: [f32; 3], time: i32) -> [f32; 3] {
    if memory.look_time > time {
        let limits = [50.0, 70.0, 30.0];
        for axis in 0..3 {
            if look[axis] > limits[axis] {
                look[axis] = limits[axis];
            } else if look[axis] < -limits[axis] {
                look[axis] = -limits[axis];
            }
        }
        let old = memory.last_head_angles;
        let difference: [f32; 3] =
            std::array::from_fn(|axis| normalized_angle(look[axis] - old[axis]));
        if difference.iter().map(|value| value * value).sum::<f32>() != 0.0 {
            for axis in 0..3 {
                look[axis] = normalized_angle(old[axis] + (difference[axis] * 0.1 * 1.5));
            }
        }
    }
    memory.last_head_angles = look;
    look
}

/// `AngleDelta`: `AngleNormalize180(a - b)`.
fn angle_delta(left: f32, right: f32) -> f32 {
    normalized_angle(left - right)
}
