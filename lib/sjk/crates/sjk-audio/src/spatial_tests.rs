use super::*;

const NEAR: Attenuation = Attenuation::Linear {
    full_volume_distance: 256.0,
    falloff_per_unit: 0.0008,
};

/// Quake axes: +X forward, +Y left, +Z up, so a yaw-0 listener's right is -Y.
fn listener() -> Listener {
    Listener {
        origin: [0.0; 3],
        forward: [1.0, 0.0, 0.0],
        right: [0.0, -1.0, 0.0],
        up: [0.0, 0.0, 1.0],
        velocity: [0.0; 3],
    }
}

#[test]
fn sound_on_the_right_is_louder_in_the_right_channel() {
    let [left, right] = LegacyQuakeSpatial::default().gains(&listener(), [0.0, -100.0, 0.0], NEAR);
    assert!(right > left, "left {left}, right {right}");
}

#[test]
fn sound_on_the_left_is_louder_in_the_left_channel() {
    let [left, right] = LegacyQuakeSpatial::default().gains(&listener(), [0.0, 100.0, 0.0], NEAR);
    assert!(left > right, "left {left}, right {right}");
}
