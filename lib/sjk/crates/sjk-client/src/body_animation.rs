//! The animation a body left on respawn plays (`CG_BodyQueueCopy`,
//! OpenJK `codemp/cgame/cg_servercmds.c:1256-1296`).
//!
//! A player may respawn a second after dying (`player_die`, `g_combat.c`:
//! `respawnTime = level.time + 1000`), while most death animations run for 1.3 to
//! 6 seconds. The body the respawn leaves behind (`ircg`) therefore usually copies a
//! player still in the middle of dying. cgame duplicates the source's Ghoul2
//! instance (`G2API_DuplicateGhoul2Instance`), then sets one animation on
//! `upper_lumbar`, `model_root` and `Motion` ([`legacy_body_queue_command`]): the
//! source's torso death animation from the frame after the one it last showed
//! (`ci->frame + 1`) to the end, held there (`BONE_ANIM_OVERRIDE_FREEZE`) and blended
//! in over 150 ms from the duplicated pose. `lower_lumbar` keeps the override it was
//! duplicated with. A source in no death animation plays `BOTH_DEAD1` from its
//! first frame instead.

use sjk_model::{AnimationConfig, BoneAnimationCommand, OverrideEndBehavior};

/// `G2API_SetBoneAnim`'s blend time in `CG_BodyQueueCopy`.
const BODY_BLEND_MILLIS: u16 = 150;

/// Resolve a body death pose without permitting a respawn's standing animation.
///
/// For a body whose source pose is unavailable (never presented here): the
/// completed pose of its own death animation, or `BOTH_DEAD1` as
/// `CG_BodyQueueCopy` falls back to.
pub fn legacy_body_frame(config: &AnimationConfig, clip: usize) -> Option<usize> {
    if crate::animation_selection::death_animation(clip) {
        let sequence = config.get_exact(crate::legacy_animation_name(clip)?)?;
        Some(sequence.first_frame + sequence.frame_count.saturating_sub(1))
    } else {
        Some(config.get_exact("BOTH_DEAD1")?.first_frame)
    }
}

/// The `G2API_SetBoneAnim` command `CG_BodyQueueCopy` installs at `time_millis`.
///
/// `torso_clip` is the source's torso animation and `torso_frame` its last presented
/// torso frame (`ci->frame`, the floored `lower_lumbar` frame `CG_TriggerAnimSounds`
/// keeps), `None` when it was never presented.
pub fn legacy_body_queue_command(
    config: &AnimationConfig,
    torso_clip: usize,
    torso_frame: Option<f32>,
    time_millis: i64,
) -> Option<BoneAnimationCommand> {
    let dying = crate::animation_selection::death_animation(torso_clip);
    let (clip, name) = if dying {
        (torso_clip, crate::legacy_animation_name(torso_clip)?)
    } else {
        let name = "BOTH_DEAD1";
        let clip = crate::legacy_animation::NAMES
            .iter()
            .position(|known| *known == name)?;
        (clip, name)
    };
    let sequence = config.get_exact(name)?;
    let first = sequence.first_frame as i64;
    let end = first + sequence.frame_count as i64;
    let start = if dying {
        // `aNum = ci->frame + 1`, stepped back inside the animation; a frame from
        // another animation before it (or none, `ci->frame` 0) takes the last frame.
        let mut frame = torso_frame.map_or(0, |frame| frame.floor() as i64) + 1;
        while frame >= end {
            frame -= 1;
        }
        if frame < first - 1 { end - 1 } else { frame }
    } else {
        first
    };
    Some(BoneAnimationCommand {
        clip,
        start_frame: i32::try_from(start).ok()?,
        end_frame: i32::try_from(end).ok()?,
        // `animSpeed = 50.0f / anim->frameLerp`, ignoring the source's speed.
        speed: crate::LegacyGhoul2PosePolicy::animation_speed(sequence, 1_000),
        time_millis,
        set_frame: None,
        end_behavior: OverrideEndBehavior::Freeze,
        blend: true,
        blend_millis: BODY_BLEND_MILLIS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH_DEATH1: usize = 9;
    const BOTH_STAND1: usize = 915;

    fn index(name: &str) -> usize {
        crate::legacy_animation::NAMES
            .iter()
            .position(|known| *known == name)
            .unwrap()
    }

    fn config() -> AnimationConfig {
        AnimationConfig::parse(
            b"BOTH_DEATH1 100 41 -1 20\nBOTH_DEAD1 141 1 -1 20\nBOTH_STAND1 0 40 0 20\n",
        )
        .unwrap()
    }

    #[test]
    fn indices_match_the_animation_table() {
        assert_eq!(index("BOTH_DEATH1"), BOTH_DEATH1);
        assert_eq!(index("BOTH_STAND1"), BOTH_STAND1);
    }

    #[test]
    fn a_body_copied_mid_death_continues_one_frame_on() {
        let command =
            legacy_body_queue_command(&config(), BOTH_DEATH1, Some(119.6), 11_000).unwrap();
        assert_eq!(command.clip, BOTH_DEATH1);
        assert_eq!((command.start_frame, command.end_frame), (120, 141));
        // 20 fps: frameLerp 50 ms, one frame per 50 ms.
        assert_eq!(command.speed, 1.0);
        assert_eq!(command.time_millis, 11_000);
        assert_eq!(command.end_behavior, OverrideEndBehavior::Freeze);
        assert!(command.blend);
        assert_eq!(command.blend_millis, 150);
    }

    #[test]
    fn a_finished_death_stays_on_its_last_frame() {
        let command =
            legacy_body_queue_command(&config(), BOTH_DEATH1, Some(140.0), 11_000).unwrap();
        assert_eq!(command.start_frame, 140);
    }

    #[test]
    fn a_frame_from_elsewhere_takes_the_last_frame() {
        for frame in [None, Some(12.0)] {
            let command = legacy_body_queue_command(&config(), BOTH_DEATH1, frame, 0).unwrap();
            assert_eq!(command.start_frame, 140);
        }
        // `aNum < firstFrame - 1` only: the frame just before the animation starts it.
        let command = legacy_body_queue_command(&config(), BOTH_DEATH1, Some(98.0), 0).unwrap();
        assert_eq!(command.start_frame, 99);
    }

    #[test]
    fn a_source_in_no_death_plays_dead1() {
        let command = legacy_body_queue_command(&config(), BOTH_STAND1, Some(10.0), 500).unwrap();
        assert_eq!(command.clip, index("BOTH_DEAD1"));
        assert_eq!((command.start_frame, command.end_frame), (141, 142));
    }

    #[test]
    fn without_a_source_pose_the_body_holds_the_completed_pose() {
        let config = config();
        assert_eq!(legacy_body_frame(&config, BOTH_DEATH1), Some(140));
        assert_eq!(legacy_body_frame(&config, BOTH_STAND1), Some(141));
    }
}
