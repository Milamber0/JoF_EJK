//! `animation.cfg` parsing on synthetic text.

use crate::AnimationConfig;

#[test]
fn sequences_parse_with_comments() {
    let config = AnimationConfig::parse(
        b"// Format: enum, targetFrame, frameCount, loopFrame, frameSpeed\n\
          BOTH_STAND1\t10\t5\t-1\t20\n",
    )
    .expect("parse");
    let stand = config.get("both_stand1").expect("stand");
    assert_eq!((stand.first_frame, stand.frame_count), (10, 5));
}

#[test]
fn a_sequence_without_frames_is_absent_instead_of_refused() {
    let config = AnimationConfig::parse(
        b"BOTH_ATTACK1\t86\t28\t-1\t24\n\
          BOTH_DEATH1\t37\t0\t-1\t20\n\
          BOTH_STAND1\t0\t1\t-1\t20\n",
    )
    .expect("BG_ParseAnimationFile accepts zero-frame lines");
    assert!(config.get("BOTH_DEATH1").is_none());
    assert!(config.get("BOTH_ATTACK1").is_some());
    assert!(config.get("BOTH_STAND1").is_some());
    assert_eq!(config.len(), 2);
}

#[test]
fn a_line_without_an_animation_name_adds_nothing() {
    let config = AnimationConfig::parse(
        b"\r\n//\r\n// Format:  targetFrame, frameCount, loopFrame, frameSpeed\r\n//\r\n\
          0\t11\t0\t30\t// fix me - invalid enum -",
    )
    .expect("BG_ParseAnimationFile skips tokens that name no animation");
    assert!(config.is_empty());
}
