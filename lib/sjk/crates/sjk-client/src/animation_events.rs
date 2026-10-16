//! Multiplayer `animevents.cfg` audio cues, independent of movement and the wire.
use sjk_model::{AnimationConfig, AnimationSequence};
use sjk_vfs::VirtualFileSystem;

pub mod footsteps;
mod parser;

/// One model-authored audio action. Paths are normalized once during loading.
#[derive(Clone, Debug)]
pub enum Cue {
    /// A random registered sound on a legacy channel.
    Sound { paths: Vec<String>, channel: u8 },
    /// Ground contact from a named Ghoul2 foot bolt. `extra` marks SJK's own
    /// contacts ([`footsteps::JUMP_LAND_STEPS`]), which the model's table does not have.
    Footstep {
        right: bool,
        heavy: bool,
        extra: bool,
    },
}

/// Absolute animation frame and probability (`0` means always).
#[derive(Clone, Debug)]
pub struct Event {
    /// Absolute GLA frame, including the animation's configured first frame.
    pub frame: i32,
    /// Percentage chance; zero means always, as in codemp.
    pub probability: u8,
    /// Audio-only payload; gameplay animation events are not executed here.
    pub cue: Cue,
    kind: String,
    order: usize,
}

/// Upper and lower event tables from one mounted model's configuration.
#[derive(Default, Debug)]
pub struct Events {
    /// Legs first, torso second; sorted by absolute keyframe.
    pub tracks: [Vec<Event>; 2],
}

impl Events {
    /// Load the model's event file, falling back to its skeleton's shared table.
    /// Includes remain within the mounted VFS and are bounded against cycles.
    pub fn load(
        vfs: &VirtualFileSystem,
        model: &str,
        skeleton: &str,
        config: &AnimationConfig,
    ) -> Self {
        let mut result = Self::default();
        let model_path = format!("{model}/animevents.cfg");
        let path = if vfs.contains(&model_path).unwrap_or(false) {
            model_path
        } else {
            format!("{skeleton}/animevents.cfg")
        };
        parser::load(&mut result, vfs, &path, config, &mut Vec::new());
        result.add_jump_land_steps(config);
        for track in &mut result.tracks {
            track.sort_by_key(|event| event.frame);
        }
        result
    }

    /// SJK's jump and landing contacts ([`footsteps::JUMP_LAND_STEPS`]): both feet at
    /// the listed frame of each listed animation the model has, unless its own table
    /// already steps somewhere in that animation.
    fn add_jump_land_steps(&mut self, config: &AnimationConfig) {
        let legs = &mut self.tracks[0];
        for &(name, key) in footsteps::JUMP_LAND_STEPS {
            let Some(sequence) = config.get_exact(name) else {
                continue;
            };
            if key as usize >= sequence.frame_count {
                continue;
            }
            let first = sequence.first_frame as i32;
            let last = first + sequence.frame_count as i32;
            if legs.iter().any(|event| {
                matches!(event.cue, Cue::Footstep { .. }) && (first..last).contains(&event.frame)
            }) {
                continue;
            }
            for right in [false, true] {
                let order = legs.len();
                legs.push(Event {
                    frame: first + key,
                    probability: 0,
                    cue: Cue::Footstep {
                        right,
                        heavy: true,
                        extra: true,
                    },
                    kind: "AEV_FOOTSTEP".to_owned(),
                    order,
                });
            }
        }
    }

    /// Sound assets to read on the appearance loader, before frame playback.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.tracks
            .iter()
            .flatten()
            .flat_map(|event| match &event.cue {
                Cue::Sound { paths, .. } => paths.as_slice(),
                Cue::Footstep { .. } => &[],
            })
            .map(String::as_str)
    }
}

/// Small replay-stable random stream; advances per occurrence, never per frame.
#[derive(Clone, Debug)]
pub struct Random(u64);
impl Default for Random {
    fn default() -> Self {
        Self(0x853c49e6748fea9b)
    }
}
impl Random {
    /// Select a bounded variant without allocation or shared state.
    pub fn index(&mut self, count: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545f4914f6cdd1d) >> 32) as usize % count.max(1)
    }
}

/// Per-actor event latch. A stopped frame cannot replay a cue.
#[derive(Default)]
pub struct Cursor {
    previous: [Option<(usize, i32)>; 2],
    random: Random,
}
impl Cursor {
    /// Forget an absent actor, teleport or backwards seek without firing old cues.
    pub fn reset(&mut self) {
        self.previous = [None; 2];
    }

    /// Visit only nearby keyframes, matching codemp's three-frame skip tolerance.
    pub fn advance(
        &mut self,
        events: &Events,
        config: &AnimationConfig,
        frames: [Option<(usize, i32)>; 2],
        mut emit: impl FnMut(&Cue, usize),
    ) {
        for (track, current) in frames.into_iter().enumerate() {
            let previous = std::mem::replace(&mut self.previous[track], current);
            let (Some((clip, frame)), Some((old_clip, old))) = (current, previous) else {
                continue;
            };
            if frame == old {
                continue;
            }
            let sequence =
                crate::legacy_animation_name(clip).and_then(|name| config.get_exact(name));
            let entries = &events.tracks[track];
            let mut selected = [0usize; 42];
            let mut count = 0;
            // Every eligible event is within three frames of one endpoint.
            for (range, (lo, hi)) in [
                (old.saturating_sub(3), old.saturating_add(3)),
                (frame.saturating_sub(3), frame.saturating_add(3)),
            ]
            .into_iter()
            .enumerate()
            {
                let start = entries.partition_point(|event| event.frame < lo);
                for (offset, event) in entries[start..]
                    .iter()
                    .take_while(|event| event.frame <= hi)
                    .enumerate()
                {
                    if range == 1 && event.frame.abs_diff(old) <= 3 {
                        continue;
                    }
                    if matches_frame(event.frame, old, frame, clip == old_clip, sequence) {
                        // Two seven-frame intervals, at most three supported event kinds per key.
                        selected[count] = start + offset;
                        count += 1;
                    }
                }
            }
            selected[..count].sort_unstable_by_key(|&i| entries[i].order);
            for &i in &selected[..count] {
                let event = &entries[i];
                if event.probability == 0 || self.random.index(100) < usize::from(event.probability)
                {
                    emit(&event.cue, self.random.index(1 << 16));
                }
            }
        }
    }
}

// Separate predicate mirrors CG_PlayerAnimEvents and is reference-checkable.
/// Whether one keyframe is reached by a changed rendered animation frame.
pub fn matches_frame(
    key: i32,
    old: i32,
    frame: i32,
    same: bool,
    sequence: Option<&AnimationSequence>,
) -> bool {
    if frame == old {
        return false;
    }
    if key == frame {
        return true;
    }
    if !same || old.abs_diff(frame) <= 1 || (old.abs_diff(key) > 3 && frame.abs_diff(key) > 3) {
        return false;
    }
    let Some(sequence) = sequence else {
        return false;
    };
    let inside = key >= sequence.first_frame as i32
        && key < (sequence.first_frame + sequence.frame_count) as i32;
    if sequence.frames_per_second < 0.0 {
        old > key && (frame < key || (sequence.loop_frame != -1 && inside && frame > old))
    } else {
        old < key && (frame > key || (sequence.loop_frame != -1 && inside && frame < old))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(events: &Events) -> Vec<(i32, bool, bool)> {
        events.tracks[0]
            .iter()
            .filter_map(|event| match event.cue {
                Cue::Footstep { right, extra, .. } => Some((event.frame, right, extra)),
                Cue::Sound { .. } => None,
            })
            .collect()
    }

    #[test]
    fn jumps_and_the_forward_landing_get_both_feet_unless_authored() {
        let config = AnimationConfig::parse(
            b"BOTH_JUMP1 100 8 -1 20
BOTH_LAND1 200 5 -1 20
BOTH_LANDBACK1 300 6 -1 20
",
        )
        .unwrap();
        let mut events = Events::default();
        // A model that authored its own landing step keeps it alone.
        events.tracks[0].push(Event {
            frame: 202,
            probability: 0,
            cue: Cue::Footstep {
                right: false,
                heavy: false,
                extra: false,
            },
            kind: "AEV_FOOTSTEP".to_owned(),
            order: 0,
        });
        events.add_jump_land_steps(&config);
        events.tracks[0].sort_by_key(|event| event.frame);
        let mut found = steps(&events);
        found.sort();
        assert_eq!(
            found,
            [(101, false, true), (101, true, true), (202, false, false)]
        );
        // The takeoff's frame is reached and both feet step.
        let mut cursor = Cursor::default();
        let mut cues = Vec::new();
        for frame in [100, 101] {
            cursor.advance(&events, &config, [Some((0, frame)), None], |cue, _| {
                cues.push(matches!(cue, Cue::Footstep { extra: true, .. }));
            });
        }
        assert_eq!(cues, [true, true]);
    }
}
