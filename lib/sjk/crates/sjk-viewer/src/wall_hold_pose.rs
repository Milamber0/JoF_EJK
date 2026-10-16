//! A player holding a grabbed wall is drawn facing it, whatever the view does.
//!
//! `CG_G2PlayerAngles` turns a player's model with its view yaw. A stock server turns
//! that yaw to the wall for as long as a wall rebound holds it
//! (`PM_AdjustAngleForWallJump`), but a JA+ server leaves the view free there
//! (`DebugMelee::free_wall_look` in `sjk-game-jka`), so a player looking
//! around on a wall turned its body away from the wall and seemed to float beside it.
//! While the rebound lasts the model takes its yaw from the wall instead
//! ([`sjk_client::pmove::wall_hold_yaw`], the stock server's facing); the camera, the
//! predicted move and the kick off the wall are unchanged. Remote players and the local
//! player (from its predicted state) go through the same rule.

use sjk_client::pmove::{MovementCollision, in_wall_rebound, wall_hold_yaw};
use sjk_runtime::{EntityId, PoseState};

/// One actor's wall facing: the last one found during the current rebound.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WallHoldFacing {
    held: Option<(EntityId, f32)>,
}

impl WallHoldFacing {
    /// `pose` with its view yaw turned to the held wall while `legs` is a wall rebound
    /// or its hold. A frame whose check misses the wall (an interpolated view turned
    /// past it, a wall on a mover) keeps the rebound's last facing; leaving the rebound
    /// returns the model to the view.
    pub(crate) fn apply(
        &mut self,
        mut pose: PoseState,
        entity: Option<EntityId>,
        legs: usize,
        origin: [f32; 3],
        collision: &impl MovementCollision,
    ) -> PoseState {
        let rebound = u16::try_from(legs)
            .ok()
            .filter(|&legs| in_wall_rebound(legs));
        let (Some(entity), Some(legs)) = (entity, rebound) else {
            self.held = None;
            return pose;
        };
        if self.held.is_some_and(|(held, _)| held != entity) {
            self.held = None;
        }
        if let Some(yaw) = wall_hold_yaw(legs, pose.view_angles_degrees[1], origin, collision) {
            self.held = Some((entity, yaw));
        }
        if let Some((_, yaw)) = self.held {
            pose.view_angles_degrees[1] = yaw;
        }
        pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_client::pmove::MovementTrace;

    /// `BOTH_FORCEWALLREBOUND_FORWARD`, `BOTH_FORCEWALLHOLD_FORWARD`, `BOTH_INAIR1`.
    const REBOUND_FORWARD: usize = 875;
    const HOLD_FORWARD: usize = 879;
    const IN_AIR: usize = 1_139;

    /// A wall 40 units east of the origin when `present`, facing west.
    struct East {
        present: bool,
    }

    impl MovementCollision for East {
        fn trace(
            &self,
            start: [f32; 3],
            _minimums: [f32; 3],
            maximums: [f32; 3],
            end: [f32; 3],
            _content_mask: u32,
        ) -> MovementTrace {
            let reach = 40.0 - maximums[0];
            if !self.present || end[0] <= reach || start[0] > reach {
                return MovementTrace::miss(end);
            }
            MovementTrace {
                fraction: (reach - start[0]) / (end[0] - start[0]),
                plane_normal: [-1.0, 0.0, 0.0],
                ..MovementTrace::miss(end)
            }
        }
    }

    fn pose(view_yaw: f32) -> PoseState {
        PoseState {
            view_angles_degrees: [10.0, view_yaw, 0.0],
            ..Default::default()
        }
    }

    fn yaw(
        facing: &mut WallHoldFacing,
        entity: u64,
        legs: usize,
        view_yaw: f32,
        wall: bool,
    ) -> f32 {
        let posed = facing.apply(
            pose(view_yaw),
            Some(EntityId::new(entity)),
            legs,
            [0.0; 3],
            &East { present: wall },
        );
        assert_eq!(posed.view_angles_degrees[0], 10.0, "pitch is the view's");
        posed.view_angles_degrees[1].rem_euclid(360.0)
    }

    #[test]
    fn the_model_stays_on_the_wall_while_the_view_turns() {
        let mut facing = WallHoldFacing::default();
        // Grabbed facing the wall, then looking 40 degrees either side: the body keeps
        // facing east, onto the wall.
        assert_eq!(yaw(&mut facing, 1, REBOUND_FORWARD, 0.0, true), 0.0);
        assert_eq!(yaw(&mut facing, 1, REBOUND_FORWARD, 40.0, true), 0.0);
        assert_eq!(yaw(&mut facing, 1, HOLD_FORWARD, 320.0, true), 0.0);
        // Kicked off: the model follows the view again.
        assert_eq!(yaw(&mut facing, 1, IN_AIR, 40.0, true), 40.0);
    }

    #[test]
    fn a_missed_check_keeps_the_rebounds_facing_and_only_its_own() {
        let mut facing = WallHoldFacing::default();
        assert_eq!(yaw(&mut facing, 1, REBOUND_FORWARD, 10.0, true), 0.0);
        // The wall is not found this frame: the facing found before holds.
        assert_eq!(yaw(&mut facing, 1, REBOUND_FORWARD, 70.0, false), 0.0);
        // Another player on this mesh, or a new rebound after leaving it, starts afresh.
        assert_eq!(yaw(&mut facing, 2, REBOUND_FORWARD, 70.0, false), 70.0);
        assert_eq!(yaw(&mut facing, 2, IN_AIR, 70.0, true), 70.0);
        assert_eq!(yaw(&mut facing, 2, REBOUND_FORWARD, 70.0, false), 70.0);
    }
}
