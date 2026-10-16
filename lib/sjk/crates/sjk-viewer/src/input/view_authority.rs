//! Adopt authoritative delta changes without replacing outstanding local mouse motion.
//!
//! OpenJK codemp g_client.c:1171-1182 sets desired minus command angles;
//! bg_pmove.c:7842-7843,7895-7906 adds that offset back to the raw command.
//!
//! The camera is the view stock renders, `cg.predictedPlayerState.viewangles`
//! (`cg_view.c:1520,1586`): the raw mouse accumulator (`cl.viewangles`, sent as the
//! command's angles, `cl_input.cpp:1319`) plus the `delta_angles` the view was last
//! folded with. While the local view is predicted that is the *predicted* state's delta
//! (`cg_predict.c:1281`), observed every frame, so a move whose pmove holds the view (a
//! DFA, a lunge, a wall run: `PM_SetPMViewAngle`) holds it on the frame it is predicted
//! instead of turning locally and snapping back when the snapshot confirms it. A command
//! subtracts [`Authority::command_delta`] from the camera, which keeps its angles the
//! raw mouse accumulator whichever source the delta came from.

/// Largest local view pitch, matching stock's `PM_UpdateViewAngles` clamp at just under a
/// right angle. Mouse motion and server-driven `delta_angles` changes share this limit: if
/// only one of them clamps, the unclamped path can drive the view past vertical.
pub(crate) const PITCH_LIMIT: f32 = 1.54;

/// Last accepted local-view baseline; not reset when a menu releases held buttons.
#[derive(Default)]
pub(crate) struct Authority {
    previous: Option<(i32, [i32; 3])>,
}

impl Authority {
    /// Present a predicted wall hold from its actual view, including frames with no
    /// new simulation slice. Delta changes alone cannot cancel fresh mouse movement
    /// when a preview is reused at the same command time.
    pub(crate) fn observe_prediction(
        &mut self,
        state: &sjk_game_jka::pmove::MovementState,
        pitch: &mut f32,
        yaw: &mut f32,
    ) {
        self.observe(
            i32::from(state.client_num),
            state.delta_angles,
            state.view_angles,
            true,
            pitch,
            yaw,
        );
        if state.wall_view_locked {
            *pitch = (-state.view_angles[0].to_radians()).clamp(-PITCH_LIMIT, PITCH_LIMIT);
            *yaw = state.view_angles[1].to_radians();
        }
    }

    /// Observe a snapshot once, preserving newer mouse motion on ordinary input echoes.
    ///
    /// Follow/intermission retain their existing authoritative presentation paths. Leaving
    /// either mode (or changing client) starts a new local-view baseline at server angles.
    pub(crate) fn observe(
        &mut self,
        client: i32,
        delta: [i32; 3],
        angles: [f32; 3],
        local_view: bool,
        pitch: &mut f32,
        yaw: &mut f32,
    ) {
        if !local_view {
            self.previous = None;
            return;
        }
        match self.previous {
            Some((old_client, old)) if old_client == client => {
                // Signed short difference handles both wrap and differing high bits.
                let dp = delta[0].wrapping_sub(old[0]) as i16;
                let dy = delta[1].wrapping_sub(old[1]) as i16;
                // Do not even round-trip floating-point values on an ordinary echo.
                if dp != 0 {
                    *pitch -= f32::from(dp) * (std::f32::consts::TAU / 65536.0);
                }
                if dy != 0 {
                    *yaw += f32::from(dy) * (std::f32::consts::TAU / 65536.0);
                }
            }
            _ => {
                *pitch = -angles[0].to_radians();
                *yaw = angles[1].to_radians();
            }
        }
        // A server that holds the view rewrites `delta_angles` every snapshot to cancel the
        // command angles it keeps receiving. Applying those rewrites unclamped let the local
        // pitch escalate past vertical — an owner session reached -121 degrees while
        // `delta_angles[PITCH]` climbed monotonically and wrapped, which is the visible
        // "camera jerks fully up/down". Bounding here breaks that loop, and matches the clamp
        // mouse motion has always had.
        *pitch = pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.previous = Some((client, delta));
    }

    /// The `delta_angles` folded into the camera so far, which a command subtracts from it
    /// (`cl.viewangles` is the camera less this); `None` without a local view, when the
    /// snapshot's delta serves.
    pub(crate) fn command_delta(&self) -> Option<[i32; 3]> {
        self.previous.map(|(_, delta)| delta)
    }
}

#[cfg(test)]
mod wall_tests {
    use super::*;
    use sjk_game_jka::pmove::MovementState;

    #[test]
    fn repeated_wall_view_cancels_mouse_motion_without_a_delta_change() {
        for step in [8, 7, 4, 3] {
            let mut authority = Authority::default();
            let state = MovementState {
                command_time: step,
                view_angles: [12.0, 90.0, 0.0],
                wall_view_locked: true,
                ..MovementState::default()
            };
            let (mut pitch, mut yaw) = (0.0, 0.0);
            authority.observe_prediction(&state, &mut pitch, &mut yaw);
            for _ in 0..3 {
                pitch += 0.3;
                yaw -= 0.5;
                authority.observe_prediction(&state, &mut pitch, &mut yaw);
                assert_eq!(pitch, -12.0_f32.to_radians());
                assert_eq!(yaw, 90.0_f32.to_radians());
            }
            // A released wall, and JA+'s free look, retain local mouse movement.
            let free = MovementState {
                wall_view_locked: false,
                ..state
            };
            pitch += 0.1;
            yaw += 0.2;
            let expected = (pitch, yaw);
            authority.observe_prediction(&free, &mut pitch, &mut yaw);
            assert_eq!((pitch, yaw), expected);
        }
    }
}
