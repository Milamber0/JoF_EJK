//! Stock multiplayer camera damping and a defined view when collision collapses the orbit.

use glam::Vec3;

/// The camera's state is presentation-only and resets on view/vehicle/teleport changes.
#[derive(Default)]
pub(crate) struct State {
    previous: Option<Previous>,
}

struct Previous {
    position: Vec3,
    target: Vec3,
    /// Undamped target and camera location of this frame (`cam.*.ideal`).
    ideal_target: Vec3,
    ideal_position: Vec3,
    /// Traced camera location before the sideways offset.
    traced_position: Vec3,
    yaw: f32,
    time: i64,
    identity: (u16, u16, u32),
}

/// Inputs after held-player and vehicle framing overrides; angles use up-positive pitch.
pub(crate) struct Frame {
    pub(crate) focus: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) range: f32,
    pub(crate) vertical: f32,
    pub(crate) horizontal: f32,
    pub(crate) camera_damp: f32,
    pub(crate) target_damp: f32,
    pub(crate) time: i64,
    pub(crate) identity: (u16, u16, u32),
    pub(crate) unrestrained: bool,
    pub(crate) hyperspace: bool,
    /// `cg_cameraFPS`: below [`CAMERA_MIN_FPS`] the stock damping per 50 ms,
    /// otherwise EternalJK's frame-rate independent damping at this rate.
    pub(crate) camera_fps: f32,
}

/// EternalJK's `CAMERA_MIN_FPS`: lower `cg_cameraFPS` values keep stock damping.
pub(crate) const CAMERA_MIN_FPS: f32 = 15.0;

/// EternalJK's `CG_DampPosition` with `cg_cameraFPS`: the offset `damp` from the
/// ideal point decays by `remaining` (the part left per emulated frame) over
/// `milliseconds`, and the ideal point's own movement (`ideal_delta` since the
/// last frame) is compensated, so the result does not depend on the frame rate.
/// With a steady frame rate equal to `fps` it equals the per-frame original.
fn damp_offset(damp: Vec3, ideal_delta: Vec3, remaining: f32, milliseconds: f32, fps: f32) -> Vec3 {
    if milliseconds <= 0.0 {
        return damp;
    }
    let frames = milliseconds * fps / 1000.0;
    let shift = ideal_delta / frames * (remaining / (1.0 - remaining));
    (damp + shift) * remaining.powf(frames) - shift
}

/// Fraction remaining after `cg.time - cameraLastFrame`, in milliseconds.
fn remaining(damp: f32, milliseconds: f32) -> f32 {
    if damp >= 1.0 {
        0.0
    } else {
        (1.0 - damp.max(0.0)).powf(milliseconds / 50.0)
    }
}

impl State {
    /// Two four-unit hull traces, with stock pitch/yaw damping and vehicle snap policy.
    pub(crate) fn update(
        &mut self,
        f: Frame,
        mut trace: impl FnMut(Vec3, Vec3) -> Vec3,
    ) -> (Vec3, Vec3) {
        let previous = self
            .previous
            .as_ref()
            .filter(|p| p.identity == f.identity && p.time <= f.time);
        let reset = previous.is_none();
        // CG_ResetThirdPersonViewDamp clamps to 89; subsequent ordinary frames to 80.
        let limit = if reset { 89.0_f32 } else { 80.0_f32 }.to_radians();
        let pitch = if f.unrestrained && !reset {
            f.pitch
        } else {
            f.pitch.clamp(-limit, limit)
        };
        let forward = Vec3::new(
            f.yaw.cos() * pitch.cos(),
            f.yaw.sin() * pitch.cos(),
            pitch.sin(),
        );
        let ideal_target = f.focus + Vec3::Z * f.vertical;
        let ideal_position = ideal_target - forward * f.range;
        let (target, position) = if let Some(p) = previous {
            let dt = (f.time - p.time) as f32;
            let target_damp = if f.identity.1 != 0 || f.hyperspace {
                1.0
            } else {
                f.target_damp
            };
            let eternal = f.camera_fps >= CAMERA_MIN_FPS;
            let target = if eternal && target_damp > 0.0 && target_damp < 1.0 {
                ideal_target
                    + damp_offset(
                        p.target - p.ideal_target,
                        ideal_target - p.ideal_target,
                        1.0 - target_damp,
                        dt,
                        f.camera_fps,
                    )
            } else if target_damp < 0.0 {
                p.target
            } else {
                ideal_target + (p.target - ideal_target) * remaining(target_damp, dt)
            };
            let target = trace(f.focus, target);
            let yaw_delta = ((f.yaw - p.yaw).to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
            // Zero elapsed server time must not turn 0/0 into an undefined view.
            let speed = if dt > 0.0 { yaw_delta.abs() / dt } else { 0.0 };
            let stiff = ((speed - 1.0) * 0.5).clamp(0.0, 0.75);
            let damp = if f.hyperspace {
                1.0
            } else if f.camera_damp == 0.0 {
                0.0
            } else {
                let base = if f.identity.1 != 0 {
                    1.0
                } else {
                    f.camera_damp
                };
                let damp = base + (1.0 - base) * (pitch.to_degrees().abs() / 115.0).powi(2);
                damp + (1.0 - damp) * stiff
            };
            let position = if eternal && damp > 0.0 && damp < 1.0 {
                ideal_position
                    + damp_offset(
                        p.traced_position - p.ideal_position,
                        ideal_position - p.ideal_position,
                        1.0 - damp,
                        dt,
                        f.camera_fps,
                    )
            } else if damp < 0.0 {
                p.position
            } else {
                ideal_position + (p.position - ideal_position) * remaining(damp, dt)
            };
            (target, trace(target, position))
        } else {
            let target = trace(f.focus, ideal_target);
            (target, trace(target, ideal_position))
        };
        // CG_OffsetThirdPersonView falls back to camerafwd on a collapsed or axial
        // look vector. Never feed coincident eye/target into the renderer's look-at.
        let diff = target - position;
        let direction = if diff.length_squared() < 0.0001 || diff.x == 0.0 || diff.y == 0.0 {
            forward
        } else {
            diff.normalize()
        };
        let left = Vec3::new(-direction.y, direction.x, 0.0).normalize_or(Vec3::new(
            -f.yaw.sin(),
            f.yaw.cos(),
            0.0,
        ));
        let traced_position = position;
        let position = position + left * f.horizontal;
        self.previous = Some(Previous {
            position,
            target,
            ideal_target,
            ideal_position,
            traced_position,
            yaw: f.yaw,
            time: f.time,
            identity: f.identity,
        });
        (position, position + direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eternal_damping_matches_the_per_frame_original_at_its_rate() {
        // One frame of 8 ms at 125 fps: damp_{n+1} = f * (damp_n - delta).
        let damp = Vec3::new(10.0, 0.0, 0.0);
        let delta = Vec3::new(2.0, 0.0, 0.0);
        let one = damp_offset(damp, delta, 0.7, 8.0, 125.0);
        assert!((one - (damp - delta) * 0.7).length() < 1e-4, "{one}");
    }

    #[test]
    fn eternal_damping_does_not_depend_on_the_frame_rate() {
        // A still ideal point: 40 ms in one step or in five equal the same decay.
        let damp = Vec3::new(30.0, -6.0, 4.0);
        let whole = damp_offset(damp, Vec3::ZERO, 0.7, 40.0, 125.0);
        let mut steps = damp;
        for _ in 0..5 {
            steps = damp_offset(steps, Vec3::ZERO, 0.7, 8.0, 125.0);
        }
        assert!((whole - steps).length() < 1e-3, "{whole} {steps}");
        // And it catches up far faster than stock's 50 ms intervals.
        assert!(whole.length() < damp.length() * 0.2);
        assert!(remaining(0.3, 40.0) > 0.7);
        // No time passed: nothing moves.
        assert_eq!(damp_offset(damp, Vec3::ONE, 0.7, 0.0, 125.0), damp);
    }

    #[test]
    fn eternal_damping_follows_a_moving_ideal_at_any_frame_rate() {
        // The ideal point moves 3 units per 8 ms: one 40 ms step equals five 8 ms steps.
        let damp = Vec3::new(20.0, 5.0, 0.0);
        let per_frame = Vec3::new(3.0, 0.0, 1.0);
        let whole = damp_offset(damp, per_frame * 5.0, 0.7, 40.0, 125.0);
        let mut steps = damp;
        for _ in 0..5 {
            steps = damp_offset(steps, per_frame, 0.7, 8.0, 125.0);
        }
        assert!((whole - steps).length() < 1e-3, "{whole} {steps}");
    }

    fn frame(focus: Vec3, time: i64, camera_fps: f32) -> Frame {
        Frame {
            focus,
            yaw: 0.3,
            pitch: 0.1,
            range: 80.0,
            vertical: 16.0,
            horizontal: 0.0,
            camera_damp: 0.3,
            target_damp: 0.5,
            time,
            identity: (0, 0, 0),
            unrestrained: false,
            hyperspace: false,
            camera_fps,
        }
    }

    /// Two frames 50 ms apart with the focus moved; no collision.
    fn second_position(camera_fps: f32) -> (Vec3, Vec3, Vec3) {
        let mut state = State::default();
        let (first, _) = state.update(frame(Vec3::ZERO, 1_000, camera_fps), |_, end| end);
        let moved = Vec3::new(40.0, 10.0, 0.0);
        let (second, _) = state.update(frame(moved, 1_050, camera_fps), |_, end| end);
        let ideal = state.previous.as_ref().unwrap().ideal_position;
        (first, second, ideal)
    }

    #[test]
    fn camera_fps_zero_keeps_the_stock_damping() {
        let (first, second, ideal) = second_position(0.0);
        // CG_DampPosition's stock path: one 50 ms step keeps (1 - damp) of the offset,
        // damp raised by the pitch term.
        let damp = 0.3 + 0.7 * (0.1_f32.to_degrees() / 115.0).powi(2);
        let expected = ideal + (first - ideal) * (1.0 - damp);
        assert!((second - expected).length() < 1e-3, "{second} {expected}");
        // The EternalJK path lands elsewhere for the same movement.
        let (_, eternal, _) = second_position(125.0);
        assert!((eternal - second).length() > 0.1);
    }

    #[test]
    fn no_elapsed_time_leaves_the_camera_where_it_was() {
        for camera_fps in [0.0, 125.0] {
            let mut state = State::default();
            let (first, _) = state.update(frame(Vec3::ZERO, 1_000, camera_fps), |_, end| end);
            let (same, look) = state.update(frame(Vec3::ZERO, 1_000, camera_fps), |_, end| end);
            assert!(same.is_finite() && look.is_finite());
            assert!(
                (same - first).length() < 1e-4,
                "{camera_fps}: {same} {first}"
            );
        }
    }
}
