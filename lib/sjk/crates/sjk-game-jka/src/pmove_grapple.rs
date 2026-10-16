//! JA+'s grapple hook: the pull towards the anchor and the hang on the rope.
//!
//! JA+ is closed source. The hook's movement is part of its `PmoveSingle`, so a client
//! can predict it; EternalJK's client reproduces the pull for JA+ servers
//! (`codemp/game/bg_pmove.c:4108-4188` `PM_GetGrappleAnim`/`PM_GrappleMove`, selected at
//! `:13127-13130`; `cgs.hookpull` 800 at `cgame/cg_servercmds.c:253`). Where the two
//! differ, this follows the JA+ 2.4 B7 game module's own movement code, read from the
//! binary; its pull performs the same arithmetic as EternalJK's.
//!
//! The JA+ game fires the hook, stores the anchor in `lastHitLoc` and flags a player it
//! pulls with `PMF_GRAPPLE`. A client-plugin user who lets go of the hook key stays on
//! the rope instead: the game clears the flag and sets entity flag bit 16
//! ([`EF_ROPE_HANG`]) until use is pressed. Each move then runs, after the water-jump
//! check and before the water, walk and air moves:
//!
//! - pulled: [`pull_velocity`] replaces the velocity, then an air move carries it;
//! - hanging: an air move, then [`rope_velocity`] swings the player on the rope.
//!
//! Stock codemp has neither flag, so only a JA+ server gets [`GrappleRules`].

use super::*;
use crate::pmove_anim::{SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS};
use sjk_protocol::{GameState, InfoString, ServerDialect, ServerProfile};

/// `PMF_GRAPPLE` (EternalJK `bg_public.h:575`): the game pulls the player to a hook.
pub const PMF_GRAPPLE: u16 = 32_768;
/// JA+'s entity flag for a player hanging on the hook's rope (`EF_NOT_USED_2` in
/// stock codemp).
pub const EF_ROPE_HANG: u32 = 1 << 16;
/// `BUTTON_USE`: on JA+ it lets go of the hook.
const BUTTON_USE: u16 = 32;
/// How far short of the anchor the pull aims, along the view (`bg_pmove.c:4162`).
const ANCHOR_STANDOFF: f32 = 16.0;
/// Inside this distance the pull slows in proportion (`bg_pmove.c:4175-4178`).
const SLOW_RADIUS: f32 = 100.0;

const BOTH_FORCEJUMP1: u16 = 1_151;
const BOTH_FORCEJUMPBACK1: u16 = 1_154;
const BOTH_FORCEJUMPLEFT1: u16 = 1_157;
const BOTH_FORCEJUMPRIGHT1: u16 = 1_160;

/// The server's hook movement, when it has one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrappleRules {
    /// Pull speed in units per second (`cgs.hookpull`).
    pub pull_speed: f32,
}

impl GrappleRules {
    /// JA+ pulls at 800 units per second (`cg_servercmds.c:253`; a constant in the JA+
    /// module); other servers have no predicted hook.
    pub fn from_dialect(dialect: &ServerDialect) -> Option<Self> {
        matches!(dialect, ServerDialect::JaPlus { .. }).then_some(Self { pull_speed: 800.0 })
    }

    /// The rules for the server whose `CS_SERVERINFO` `game` carries.
    pub fn from_game_state(game: &GameState) -> Option<Self> {
        let info = game
            .config_string(0)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| InfoString::parse(text).ok())?;
        Self::from_dialect(&ServerProfile::from_server_info(&info).dialect)
    }
}

/// Which hook movement a move runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HookMove {
    Pull,
    Hang,
}

/// The pull velocity towards `anchor` from `origin`, viewing along `forward`
/// (`PM_GrappleMove`, `bg_pmove.c:4161-4180`).
pub(crate) fn pull_velocity(
    rules: GrappleRules,
    anchor: [f32; 3],
    origin: [f32; 3],
    forward: Vec3,
) -> [f32; 3] {
    let target = Vec3::from(anchor) + forward * -ANCHOR_STANDOFF;
    let offset = target - Vec3::from(origin);
    let length = offset.length();
    let direction = vector_normalize(offset);
    let speed = if length <= SLOW_RADIUS {
        rules.pull_speed / 80.0 * length
    } else {
        rules.pull_speed
    };
    (direction * speed).to_array()
}

/// The velocity after a move's swing on the rope from `anchor`, as the JA+ module
/// computes it after the air move.
///
/// The rope is as long as the distance from the anchor to where the move began; when
/// it has a length, gravity pulls along the rope in proportion to the anchor's height
/// difference, and the component of the velocity along the rope is removed, so the
/// player swings about the anchor. The sums run in the x87's extended precision and
/// are rounded to `float` where the module stores them.
pub(crate) fn rope_velocity(
    anchor: [f32; 3],
    origin: [f32; 3],
    move_start: [f32; 3],
    velocity: [f32; 3],
    gravity: f32,
    seconds: f32,
) -> [f32; 3] {
    let height = (anchor[2] - origin[2]).abs();
    let from_start = [0, 1, 2].map(|axis| f64::from(anchor[axis] - move_start[axis]));
    let rope = (from_start[2] * from_start[2]
        + from_start[1] * from_start[1]
        + from_start[0] * from_start[0])
        .sqrt() as f32;
    if rope <= 0.0 {
        return velocity;
    }
    let direction = vector_normalize(Vec3::from(anchor) - Vec3::from(origin)).to_array();
    let swing =
        (f64::from(gravity) * f64::from(height) / f64::from(rope) * f64::from(seconds)) as f32;
    let carried = [0, 1, 2].map(|axis| swing * direction[axis] + velocity[axis]);
    let along = -(f64::from(carried[2]) * f64::from(direction[2])
        + f64::from(carried[1]) * f64::from(direction[1])
        + f64::from(carried[0]) * f64::from(direction[0]));
    [0, 1, 2].map(|axis| (along * f64::from(direction[axis]) + f64::from(carried[axis])) as f32)
}

/// The jump pose for hook movement at `velocity` while facing `yaw`
/// (`PM_GetGrappleAnim`, `bg_pmove.c:4110-4146`), if any.
pub(crate) fn pull_animation(yaw: f32, velocity: [f32; 3]) -> Option<u16> {
    let (facing, right) = flight::flight_axes([0.0, yaw, 0.0]);
    let velocity = Vec3::from(velocity);
    let along_right = right.dot(velocity);
    let along_facing = facing.dot(velocity);
    if along_right.abs() > along_facing.abs() * 1.5 {
        if along_right > 150.0 {
            Some(BOTH_FORCEJUMPRIGHT1)
        } else if along_right < -150.0 {
            Some(BOTH_FORCEJUMPLEFT1)
        } else {
            None
        }
    } else if along_facing > 150.0 {
        Some(BOTH_FORCEJUMP1)
    } else if along_facing < -150.0 {
        Some(BOTH_FORCEJUMPBACK1)
    } else {
        None
    }
}

impl Predictor {
    /// The hook movement this move runs, if any. Use lets go of the hook in the JA+
    /// game before the move, so a pull with use held is predicted as no pull, as
    /// EternalJK does (`bg_pmove.c:4157-4159`), and likewise a hang.
    pub(super) fn hook_move(&self, command: &UserCommand) -> Option<HookMove> {
        self.config.grapple?;
        if command.buttons & BUTTON_USE != 0 {
            None
        } else if self.state.movement_flags & PMF_GRAPPLE != 0 {
            Some(HookMove::Pull)
        } else if self.state.entity_flags & EF_ROPE_HANG != 0 {
            Some(HookMove::Hang)
        } else {
            None
        }
    }

    /// Replace the velocity with the pull to the anchor; the air move follows.
    pub(super) fn grapple_pull(&mut self, ground: &mut GroundState) {
        let Some(rules) = self.config.grapple else {
            return;
        };
        let (forward, _) = flight::flight_axes(self.state.view_angles);
        self.state.velocity = pull_velocity(
            rules,
            self.state.last_hit_location,
            self.state.origin,
            forward,
        );
        ground.ground_plane = false;
        self.grapple_pose();
    }

    /// Swing on the rope after the air move that began at `move_start`.
    pub(super) fn rope_hang(&mut self, move_start: [f32; 3], seconds: f32) {
        self.state.velocity = rope_velocity(
            self.state.last_hit_location,
            self.state.origin,
            move_start,
            self.state.velocity,
            self.state.gravity,
            seconds,
        );
        self.grapple_pose();
    }

    /// The JA+ module poses the legs only, with `OVERRIDE | HOLD` (EternalJK poses
    /// both halves unless a weapon is busy).
    fn grapple_pose(&mut self) {
        if let Some(animation) = pull_animation(self.state.view_angles[1], self.state.velocity)
            && let Some(lengths) = self.animation_lengths.as_deref()
        {
            crate::pmove_anim::set_animation(
                &mut self.state,
                SETANIM_LEGS,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                lengths,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JA_PLUS: GrappleRules = GrappleRules { pull_speed: 800.0 };

    #[test]
    fn far_anchors_pull_at_full_speed_short_of_the_anchor() {
        // Looking along +y, 25 degrees up, at an anchor 343 units ahead: the pull aims
        // 16 units back along the view and runs at 800 (a JA+ server sent
        // [16, 693, 393]: this, less the air move's 6.4 of gravity, snapped).
        let (forward, _) = flight::flight_axes([-25.0, 90.0, 0.0]);
        let velocity = pull_velocity(
            JA_PLUS,
            [1692.0, 1983.0, 124.0],
            [1684.3661, 1640.0, -71.875],
            forward,
        );
        let speed = Vec3::from(velocity).length();
        assert!((speed - 800.0).abs() < 1e-3, "{speed}");
        assert_eq!(velocity.map(|axis| axis.round()), [16.0, 693.0, 399.0]);
    }

    #[test]
    fn near_anchors_slow_in_proportion() {
        let forward = Vec3::X;
        // 50 units from the standoff point: 10 units/s per unit of distance.
        let velocity = pull_velocity(JA_PLUS, [100.0, 0.0, 0.0], [34.0, 0.0, 0.0], forward);
        assert_eq!(velocity, [500.0, 0.0, 0.0]);
        // At the standoff point there is nothing left to pull.
        let velocity = pull_velocity(JA_PLUS, [100.0, 0.0, 0.0], [84.0, 0.0, 0.0], forward);
        assert_eq!(velocity, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn the_rope_keeps_only_motion_across_it() {
        // Hanging straight below the anchor: the rope removes the fall and keeps the
        // sideways motion.
        let velocity = rope_velocity(
            [0.0, 0.0, 100.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [30.0, 0.0, -50.0],
            800.0,
            0.008,
        );
        assert_eq!(velocity, [30.0, 0.0, 0.0]);
        // Off to the side, gravity's share along the rope is removed with the rest of
        // the radial motion: what remains is perpendicular to the rope.
        let velocity = rope_velocity(
            [0.0, 0.0, 100.0],
            [100.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [0.0, 0.0, -20.0],
            800.0,
            0.008,
        );
        let rope = Vec3::new(-1.0, 0.0, 1.0).normalize();
        assert!(Vec3::from(velocity).dot(rope).abs() < 1e-4, "{velocity:?}");
        // No rope length, no swing.
        let velocity = rope_velocity(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -10.0],
            [0.0, 0.0, 0.0],
            [1.0, 2.0, 3.0],
            800.0,
            0.008,
        );
        assert_eq!(velocity, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn the_pose_follows_the_motion_relative_to_the_facing() {
        assert_eq!(
            pull_animation(90.0, [0.0, 400.0, 300.0]),
            Some(BOTH_FORCEJUMP1)
        );
        assert_eq!(
            pull_animation(90.0, [0.0, -400.0, 0.0]),
            Some(BOTH_FORCEJUMPBACK1)
        );
        // Facing +y, right is +x.
        assert_eq!(
            pull_animation(90.0, [400.0, 0.0, 0.0]),
            Some(BOTH_FORCEJUMPRIGHT1)
        );
        assert_eq!(
            pull_animation(90.0, [-400.0, 0.0, 0.0]),
            Some(BOTH_FORCEJUMPLEFT1)
        );
        assert_eq!(pull_animation(90.0, [0.0, 100.0, 0.0]), None);
    }

    #[test]
    fn only_ja_plus_servers_have_the_hook() {
        let ja_plus = ServerDialect::JaPlus {
            capabilities: sjk_protocol::JaPlusCapabilities(0),
        };
        assert_eq!(GrappleRules::from_dialect(&ja_plus), Some(JA_PLUS));
        assert_eq!(GrappleRules::from_dialect(&ServerDialect::BaseJka), None);
        assert_eq!(
            GrappleRules::from_dialect(&ServerDialect::TaystJk {
                ja_pro_capabilities: sjk_protocol::JaProCapabilities(1 << 30),
                tayst_capabilities: sjk_protocol::TaystJkCapabilities(0),
            }),
            None
        );
    }
}
