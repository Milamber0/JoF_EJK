//! Lightsaber creation's sabers on their own. Retail's `saber.menu` and
//! `ingame_saber.menu` drew the hilt alone (`isSaber` model items, painted by
//! `Item_Model_Paint` in `codemp/ui/ui_shared.c`): laid on its side with
//! `angles = { model_angle + time / model_rotation, 0, 90 }`, so it turns about
//! its own length a degree every `model_rotation 20` ms from `model_angle 180`,
//! its blades lit by `UI_SaberDrawBlades`, and a second saber's item 50 units
//! below the first. Here each saber lies along the preview camera's horizontal,
//! centred on its whole length (hilt and blades together), so a staff and a
//! single saber both fill the band, and the camera stands back far enough for
//! the longest of them.

use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
use glam::{Mat4, Quat, Vec3};

/// Turn about the saber's length, degrees per second (`model_rotation 20`).
const ROLL_DEGREES_PER_SECOND: f32 = 50.0;
/// Angle the turn starts from (`model_angle 180`).
const START_DEGREES: f32 = 180.0;
/// Distance between the two blade lines of dual sabers, the first above.
const DUAL_GAP: f32 = 14.0;
/// Half the height a saber takes across its line: hilt and blade glow.
const HALF_THICKNESS: f32 = 5.0;
/// Room left around the sabers in the frame.
const MARGIN: f32 = 4.0;
/// Vertical field of view, as the model preview's.
const FIELD_OF_VIEW_DEGREES: f32 = 30.0;
/// Height of the showcase's focus above the actor lending its light, where
/// the model preview frames the body's middle.
pub(super) const FOCUS_ABOVE_ORIGIN: f32 = 8.0;

/// A saber's extent along its first blade's line, in hilt units measured
/// along that blade from the hilt origin: from the far end of the hilt or of
/// a backward blade (`back`) to the tip ahead (`front`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Span {
    pub(super) back: f32,
    pub(super) front: f32,
}

impl Span {
    fn middle(self) -> f32 {
        (self.back + self.front) * 0.5
    }

    fn length(self) -> f32 {
        self.front - self.back
    }
}

/// What a hilt needs to lie on the showcase: its first blade's hilt-local
/// direction and a point of that blade's line, and its span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Line {
    pub(super) axis: Vec3,
    pub(super) through: Vec3,
    pub(super) span: Span,
}

impl Line {
    /// The line of a hilt whose first blade leaves `socket` along
    /// `direction`, with `points` (the mesh's vertices and every blade's
    /// root and tip, hilt-local) giving its extent. `None` without a blade.
    pub(super) fn new(
        socket: Vec3,
        direction: Vec3,
        points: impl IntoIterator<Item = Vec3>,
    ) -> Option<Self> {
        let axis = direction.try_normalize()?;
        let (back, front) = points
            .into_iter()
            .map(|point| point.dot(axis))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), t| {
                (low.min(t), high.max(t))
            });
        (back <= front).then_some(Self {
            axis,
            through: socket,
            span: Span { back, front },
        })
    }
}

/// Where the sabers are shown: the point the camera looks at, the side it
/// looks from, and the camera's right and up there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct View {
    pub(super) focus: Vec3,
    pub(super) facing: Vec3,
    /// Half the width and height the camera must take in.
    pub(super) half: [f32; 2],
}

impl View {
    /// The showcase of the sabers on `lines` (right hand, left hand) at
    /// `focus`, seen from `facing` (horizontal).
    pub(super) fn new(focus: Vec3, facing: Vec3, lines: [Option<Line>; 2]) -> Self {
        let facing = Vec3::new(facing.x, facing.y, 0.0).normalize_or(Vec3::X);
        let longest = lines
            .iter()
            .flatten()
            .map(|line| line.span.length())
            .fold(0.0, f32::max);
        let shown = lines.iter().flatten().count();
        let rows = if shown > 1 { DUAL_GAP * 0.5 } else { 0.0 };
        Self {
            focus,
            facing,
            half: [longest * 0.5 + MARGIN, rows + HALF_THICKNESS + MARGIN],
        }
    }

    /// The camera's right, along which the sabers lie.
    pub(super) fn right(&self) -> Vec3 {
        Vec3::Z.cross(self.facing).normalize_or(Vec3::Y)
    }

    /// Height of `hand`'s blade line above the focus: one saber on it, the
    /// first of two above the second.
    pub(super) fn offset(hand: usize, shown: usize) -> f32 {
        if shown < 2 {
            0.0
        } else if hand == 0 {
            DUAL_GAP * 0.5
        } else {
            -DUAL_GAP * 0.5
        }
    }

    /// The grip and orientation that lay the saber on `line` across the
    /// view, its length centred `height` above the focus, turned `degrees`
    /// about its own blade line.
    pub(super) fn pose(&self, line: &Line, height: f32, degrees: f32) -> (Vec3, Quat) {
        let right = self.right();
        let rotation = Quat::from_axis_angle(right, degrees.to_radians())
            * Quat::from_rotation_arc(line.axis, right);
        // The blade line runs through `through`; keep it on the centre line
        // while the hilt turns about it.
        let lateral = line.through - line.axis * line.through.dot(line.axis);
        let centre = self.focus + Vec3::Z * height;
        let grip = centre - right * line.span.middle() - rotation * lateral;
        (grip, rotation)
    }

    /// The camera for a target of `aspect` (width over height): in front, far
    /// enough back to take in [`Self::half`].
    pub(super) fn camera(&self, aspect: f32) -> (Mat4, Vec3, Vec3) {
        let tangent = (FIELD_OF_VIEW_DEGREES * 0.5).to_radians().tan();
        let [half_width, half_height] = self.half;
        let distance = (half_height / tangent).max(half_width / (tangent * aspect.max(0.1)));
        let eye = self.focus + self.facing * distance;
        let view = look_at_mat4(eye, self.focus, Vec3::Z);
        let projection = perspective(
            FIELD_OF_VIEW_DEGREES.to_radians(),
            aspect,
            1.0,
            distance * 4.0,
        );
        (
            projection * view,
            eye,
            (self.focus - eye).normalize_or(Vec3::X),
        )
    }
}

/// The turn about the saber's length `seconds` into the showcase.
pub(super) fn roll_degrees(seconds: f32) -> f32 {
    (START_DEGREES + seconds * ROLL_DEGREES_PER_SECOND) % 360.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ndc(view_projection: Mat4, point: Vec3) -> Vec3 {
        let clip = view_projection * point.extend(1.0);
        clip.truncate() / clip.w
    }

    /// A single saber: hilt from 0 to 10 along +Z, blade 40 from the top.
    fn single() -> Line {
        let socket = Vec3::new(0.5, 0.0, 10.0);
        Line::new(
            socket,
            Vec3::Z,
            [Vec3::ZERO, Vec3::Z * 10.0, socket + Vec3::Z * 40.0],
        )
        .unwrap()
    }

    #[test]
    fn the_span_runs_from_the_hilt_end_to_the_farthest_tip() {
        let line = single();
        assert_eq!(
            line.span,
            Span {
                back: 0.0,
                front: 50.0
            }
        );
        // A staff: a second blade backwards from the bottom.
        let staff = Line::new(
            Vec3::Z * 10.0,
            Vec3::Z,
            [Vec3::Z * 10.0 + Vec3::Z * 40.0, Vec3::NEG_Z * 40.0],
        )
        .unwrap();
        assert_eq!(
            staff.span,
            Span {
                back: -40.0,
                front: 50.0
            }
        );
        assert!(Line::new(Vec3::ZERO, Vec3::ZERO, [Vec3::ZERO]).is_none());
    }

    #[test]
    fn a_saber_lies_across_the_view_centred_on_its_length() {
        let focus = Vec3::new(100.0, 200.0, 30.0);
        let line = single();
        let view = View::new(focus, Vec3::X, [Some(line), None]);
        let right = view.right();
        for degrees in [0.0, 90.0, 217.0] {
            let (grip, rotation) = view.pose(&line, 0.0, degrees);
            // The blade's direction is the view's right.
            assert!((rotation * line.axis - right).length() < 1e-4);
            // The middle of the saber's length is on the focus, whatever the turn.
            let middle =
                line.through + line.axis * (line.span.middle() - line.through.dot(line.axis));
            assert!(
                (grip + rotation * middle - focus).length() < 1e-3,
                "{degrees}"
            );
        }
    }

    #[test]
    fn dual_sabers_stack_the_first_above_the_second() {
        assert_eq!(View::offset(0, 1), 0.0);
        assert!(View::offset(0, 2) > 0.0);
        assert!(View::offset(1, 2) < 0.0);
        let line = single();
        let one = View::new(Vec3::ZERO, Vec3::X, [Some(line), None]);
        let two = View::new(Vec3::ZERO, Vec3::X, [Some(line), Some(line)]);
        assert!(two.half[1] > one.half[1]);
        assert_eq!(two.half[0], one.half[0]);
    }

    #[test]
    fn the_camera_takes_in_the_whole_saber_with_its_tip_to_the_right() {
        let focus = Vec3::new(-50.0, 10.0, 64.0);
        let line = single();
        for (facing, aspect) in [
            (Vec3::X, 400.0 / 206.0),
            (Vec3::new(-1.0, 1.0, 0.0), 350.0 / 130.0),
        ] {
            let view = View::new(focus, facing, [Some(line), None]);
            let (view_projection, eye, forward) = view.camera(aspect);
            assert!(forward.dot(view.facing) < -0.99);
            assert!((eye - focus).dot(view.facing) > 0.0);
            let (grip, rotation) = view.pose(&line, 0.0, 0.0);
            let base = ndc(view_projection, grip + rotation * Vec3::ZERO);
            let tip = ndc(
                view_projection,
                grip + rotation * (line.through + line.axis * 40.0),
            );
            assert!(tip.x > base.x, "the blade points right: {base} {tip}");
            for point in [base, tip] {
                assert!(point.x.abs() < 1.0 && point.y.abs() < 1.0, "{point}");
            }
            // It fills the width, give or take the margin.
            assert!(tip.x - base.x > 1.6, "{base} {tip}");
        }
    }

    #[test]
    fn the_turn_starts_at_retail_s_model_angle() {
        assert_eq!(roll_degrees(0.0), 180.0);
        assert_eq!(roll_degrees(1.0), 230.0);
        assert!(roll_degrees(1_000.0) < 360.0);
    }
}
