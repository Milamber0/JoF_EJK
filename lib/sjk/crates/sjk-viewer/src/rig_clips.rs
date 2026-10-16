//! Clips: a rig model's own animations, read from its glTF and played on its own
//! skeleton (emotes), in place of the pose retargeted from the driver while they
//! run. Keys are converted to the game's axes and units once, at load.

use glam::{Affine3A, Mat3A, Quat, Vec3A};
use sjk_model::gltf::{GltfAnimation, GltfPath};

/// How long a clip takes to fade in over the driver's pose, and out again.
pub(crate) const FADE_SECONDS: f32 = 0.25;

/// glTF axes and metres to the game's, for keys and rest poses.
#[derive(Clone, Copy, Debug)]
pub(super) struct Convert {
    axes: Mat3A,
    rotation: Quat,
    scale: f32,
}

impl Convert {
    pub(super) fn new(axes: Mat3A, scale: f32) -> Self {
        Self {
            axes,
            rotation: Quat::from_mat3a(&axes),
            scale,
        }
    }

    fn translation(&self, value: Vec3A) -> Vec3A {
        self.axes * value * self.scale
    }

    fn rotation(&self, value: Quat) -> Quat {
        (self.rotation * value * self.rotation.conjugate()).normalize()
    }

    fn scale(&self, value: Vec3A) -> Vec3A {
        (self.axes * value).abs()
    }
}

/// A joint's rest transform relative to its parent, in game space.
#[derive(Clone, Copy, Debug)]
pub(super) struct Rest {
    pub(super) translation: Vec3A,
    pub(super) rotation: Quat,
    pub(super) scale: Vec3A,
}

impl Rest {
    pub(super) fn of(local: &Affine3A) -> Self {
        let (scale, rotation, translation) = local.to_scale_rotation_translation();
        Self {
            translation: translation.into(),
            rotation,
            scale: scale.into(),
        }
    }
}

#[derive(Debug)]
struct Track<T> {
    times: Vec<f32>,
    values: Vec<T>,
    step: bool,
}

impl<T: Copy> Track<T> {
    /// The value at `time`, held before the first key and after the last.
    fn sample(&self, time: f32, mix: impl Fn(T, T, f32) -> T) -> T {
        let next = self.times.partition_point(|&key| key <= time);
        if next == 0 {
            return self.values[0];
        }
        if next >= self.times.len() || self.step {
            return self.values[next - 1];
        }
        let (from, to) = (self.times[next - 1], self.times[next]);
        let fraction = if to > from {
            (time - from) / (to - from)
        } else {
            0.0
        };
        mix(self.values[next - 1], self.values[next], fraction)
    }
}

#[derive(Debug, Default)]
struct JointTracks {
    translation: Option<Track<Vec3A>>,
    rotation: Option<Track<Quat>>,
    scale: Option<Track<Vec3A>>,
}

/// One animation of a rig model.
#[derive(Debug)]
pub(crate) struct Clip {
    pub(crate) name: String,
    /// Seconds, to its last key.
    pub(crate) duration: f32,
    joints: Vec<JointTracks>,
}

impl Clip {
    /// `animation` for a skeleton of `joints` joints.
    pub(super) fn new(animation: &GltfAnimation, joints: usize, convert: Convert) -> Self {
        let mut tracks: Vec<JointTracks> = (0..joints).map(|_| JointTracks::default()).collect();
        let mut duration = 0.0f32;
        for channel in &animation.channels {
            let Some(slot) = tracks.get_mut(channel.joint) else {
                continue;
            };
            duration = duration.max(channel.times.last().copied().unwrap_or(0.0));
            let times = channel.times.clone();
            let step = channel.step;
            let vector = |value: &[f32; 4]| Vec3A::new(value[0], value[1], value[2]);
            match channel.path {
                GltfPath::Translation => {
                    slot.translation = Some(Track {
                        times,
                        values: channel
                            .values
                            .iter()
                            .map(|v| convert.translation(vector(v)))
                            .collect(),
                        step,
                    });
                }
                GltfPath::Rotation => {
                    slot.rotation = Some(Track {
                        times,
                        values: channel
                            .values
                            .iter()
                            .map(|v| convert.rotation(Quat::from_xyzw(v[0], v[1], v[2], v[3])))
                            .collect(),
                        step,
                    });
                }
                GltfPath::Scale => {
                    slot.scale = Some(Track {
                        times,
                        values: channel
                            .values
                            .iter()
                            .map(|v| convert.scale(vector(v)))
                            .collect(),
                        step,
                    });
                }
            }
        }
        Self {
            name: animation.name.clone(),
            duration,
            joints: tracks,
        }
    }

    /// A clip of `joints.len()` joints from `frames`, each one local transform per
    /// joint, keyed at `times` (a clip borrowed from another rig, [`super::RigModel`]).
    pub(super) fn from_frames(name: &str, times: &[f32], frames: &[Vec<Affine3A>]) -> Self {
        let count = frames.first().map_or(0, Vec::len);
        let joints = (0..count)
            .map(|joint| {
                let mut translations = Vec::with_capacity(frames.len());
                let mut rotations: Vec<Quat> = Vec::with_capacity(frames.len());
                let mut scales = Vec::with_capacity(frames.len());
                for frame in frames {
                    let (scale, rotation, translation) =
                        frame[joint].to_scale_rotation_translation();
                    // Neighbouring keys on the same side, so they blend the short way.
                    let rotation = match rotations.last() {
                        Some(previous) if previous.dot(rotation) < 0.0 => -rotation,
                        _ => rotation,
                    };
                    translations.push(Vec3A::from(translation));
                    rotations.push(rotation);
                    scales.push(Vec3A::from(scale));
                }
                let track = |values| Track {
                    times: times.to_vec(),
                    values,
                    step: false,
                };
                JointTracks {
                    translation: Some(track(translations)),
                    rotation: Some(Track {
                        times: times.to_vec(),
                        values: rotations,
                        step: false,
                    }),
                    scale: Some(track(scales)),
                }
            })
            .collect();
        Self {
            name: name.to_owned(),
            duration: times.last().copied().unwrap_or(0.0),
            joints,
        }
    }

    /// Every key time of the clip, in order.
    pub(super) fn key_times(&self) -> Vec<f32> {
        let mut times: Vec<f32> = self
            .joints
            .iter()
            .flat_map(|tracks| {
                [
                    tracks.translation.as_ref().map(|t| t.times.as_slice()),
                    tracks.rotation.as_ref().map(|t| t.times.as_slice()),
                    tracks.scale.as_ref().map(|t| t.times.as_slice()),
                ]
            })
            .flatten()
            .flatten()
            .copied()
            .collect();
        times.sort_by(f32::total_cmp);
        times.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
        times
    }

    /// Whether the clip moves joint `index` itself.
    pub(super) fn animates(&self, index: usize) -> bool {
        self.joints.get(index).is_some_and(|tracks| {
            tracks.translation.is_some() || tracks.rotation.is_some() || tracks.scale.is_some()
        })
    }

    /// Joint `index`'s transform relative to its parent at `time`; `rest` fills in
    /// what the clip does not animate.
    pub(super) fn local(&self, index: usize, time: f32, rest: &Rest) -> Affine3A {
        let Some(tracks) = self.joints.get(index) else {
            return Affine3A::from_scale_rotation_translation(
                rest.scale.into(),
                rest.rotation,
                rest.translation.into(),
            );
        };
        let translation = tracks
            .translation
            .as_ref()
            .map_or(rest.translation, |track| {
                track.sample(time, |a, b, f| a.lerp(b, f))
            });
        let rotation = tracks.rotation.as_ref().map_or(rest.rotation, |track| {
            track.sample(time, |a, b, f| a.slerp(b, f))
        });
        let scale = tracks.scale.as_ref().map_or(rest.scale, |track| {
            track.sample(time, |a, b, f| a.lerp(b, f))
        });
        Affine3A::from_scale_rotation_translation(scale.into(), rotation, translation.into())
    }
}

/// A clip playing on one actor this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClipPlay {
    pub(crate) clip: usize,
    /// Seconds into the clip.
    pub(crate) time: f32,
    /// 0 is the driver's pose, 1 the clip's.
    pub(crate) weight: f32,
}

impl ClipPlay {
    /// `clip` (of `duration` seconds) `seconds` after it started, `remaining` seconds
    /// before it is stopped (`None`: it plays until the player moves).
    pub(crate) fn new(
        clip: usize,
        duration: f32,
        seconds: f32,
        remaining: Option<f32>,
        looping: bool,
    ) -> Self {
        let time = if looping && duration > 0.0 {
            seconds % duration
        } else {
            seconds.min(duration)
        };
        let fade_in = (seconds / FADE_SECONDS).clamp(0.0, 1.0);
        let fade_out = remaining.map_or(1.0, |left| (left / FADE_SECONDS).clamp(0.0, 1.0));
        Self {
            clip,
            time,
            weight: fade_in.min(fade_out),
        }
    }
}

/// `a` towards `b` by `weight`: scale and translation straight, rotation the short
/// way round.
pub(super) fn blend(a: &Affine3A, b: &Affine3A, weight: f32) -> Affine3A {
    let (scale_a, rotation_a, translation_a) = a.to_scale_rotation_translation();
    let (scale_b, rotation_b, translation_b) = b.to_scale_rotation_translation();
    Affine3A::from_scale_rotation_translation(
        scale_a.lerp(scale_b, weight),
        rotation_a.slerp(rotation_b, weight),
        translation_a.lerp(translation_b, weight),
    )
}
