//! The beam of an armed trip mine (`CG_General`, `cg_ents.c:1789-1814`): a stuck mine
//! (`ET_GENERAL`, `WP_TRIP_MINE`, `time == -1`) with `EF_FIRING` plays
//! `tripMine/laserMP` (or `tripMine/glowbit` in proximity mode, `bolt2 == 1`) every
//! cgame frame, 6.6 units out along its facing, pointed along `pos.trDelta`, the
//! surface normal the game stores there. Beams far from the camera are not played
//! (`CULL_DISTANCE`).

use crate::effect_runtime::EffectLibrary;
use crate::{GameAudio, Particle, combat_effects, effect_runtime};
use glam::Vec3;
use sjk_protocol::Snapshot;
use sjk_vfs::VirtualFileSystem;
use std::time::Instant;

const ET_GENERAL: u8 = 0;
const WP_TRIP_MINE: u8 = 13;
const EF_FIRING: u32 = 1 << 9;
const LASER: &str = "tripMine/laserMP";
const GLOW: &str = "tripMine/glowbit";

/// One armed mine's beam: where it starts, which way it points and which effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Beam {
    pub(crate) origin: [f32; 3],
    pub(crate) direction: [f32; 3],
    pub(crate) effect: &'static str,
}

/// The beam of `state`, when it is an armed, stuck trip mine.
pub(crate) fn beam(state: &sjk_protocol::EntityState) -> Option<Beam> {
    if state.entity_type() != ET_GENERAL
        || state.weapon() != WP_TRIP_MINE
        || state.time() != -1
        || state.e_flags() & EF_FIRING == 0
    {
        return None;
    }
    // A stuck mine is stationary: `lerpOrigin` and `lerpAngles` are the bases.
    let [pitch, yaw, _] = state.angular_trajectory_base().map(f32::to_radians);
    // `AnglesToAxis`: axis[0], the facing.
    let forward = Vec3::new(
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        -pitch.sin(),
    );
    let origin = Vec3::from_array(state.trajectory_base()) + forward * 6.6;
    Some(Beam {
        origin: origin.to_array(),
        direction: state.trajectory_delta(),
        effect: if state.bolt2() == 1 { GLOW } else { LASER },
    })
}

/// `FX_MAX_TRACE_DIST` (`FxScheduler.h`): how far an `org2fromTrace` line reaches.
const TRACE_DISTANCE: f32 = 16_384.0;
/// New beam traces per cgame tick. A stuck mine never moves, so its trace is kept
/// ([`Beams`]); a mine beyond this budget shows its beam on a later tick.
const TRACE_BUDGET: usize = 4;
/// Traced beams kept; when full, the oldest entry is replaced.
const MAX_CACHED: usize = 64;

#[derive(Clone, Copy)]
struct CachedBeam {
    number: u16,
    origin: [f32; 3],
    direction: [f32; 3],
    end: Vec3,
}

/// The traced end of each armed mine's beam, kept while the mine stays where it is.
#[derive(Default)]
pub(crate) struct Beams {
    cached: Vec<CachedBeam>,
    next: usize,
}

impl Beams {
    /// The end of `beam` on entity `number`: kept from an earlier tick, traced now
    /// while `budget` allows, or `None` until a later tick.
    fn end(
        &mut self,
        number: u16,
        beam: &Beam,
        budget: &mut usize,
        trace: impl FnOnce() -> Vec3,
    ) -> Option<Vec3> {
        let slot = self
            .cached
            .iter()
            .position(|cached| cached.number == number);
        if let Some(index) = slot
            && self.cached[index].origin == beam.origin
            && self.cached[index].direction == beam.direction
        {
            return Some(self.cached[index].end);
        }
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let entry = CachedBeam {
            number,
            origin: beam.origin,
            direction: beam.direction,
            end: trace(),
        };
        match slot {
            Some(index) => self.cached[index] = entry,
            None if self.cached.len() < MAX_CACHED => self.cached.push(entry),
            None => {
                self.cached[self.next] = entry;
                self.next = (self.next + 1) % MAX_CACHED;
            }
        }
        Some(entry.end)
    }
}

/// The beam's `Line` is an `org2fromTrace` primitive: it runs from the mine to
/// whatever solid its facing hits (`CFxScheduler::CreateEffect`). A spawned line
/// would trace that itself when first drawn; a mine replays its beam every cgame
/// frame, so the lines just spawned are given the kept trace's end instead.
fn trace_end(
    origin: Vec3,
    direction: Vec3,
    bsp: &sjk_bsp::Bsp,
    scratch: &mut sjk_bsp::TraceScratch,
) -> Vec3 {
    let direction = direction.normalize_or(Vec3::Z);
    Vec3::from_array(
        bsp.trace_box_with(
            scratch,
            origin.to_array(),
            (origin + direction * TRACE_DISTANCE).to_array(),
            sjk_bsp::Aabb::POINT,
            // MASK_SOLID.
            0x0000_0001,
        )
        .end_position,
    )
}

/// Beyond this distance from the viewer to any point of the beam it is not played:
/// a beam a few units wide is far under a pixel by then, and a map holds a handful
/// of mines. The test is on the whole traced segment, not the mine, because a beam
/// reaches up to [`TRACE_DISTANCE`] away from it.
const CULL_DISTANCE: f32 = 8_192.0;

/// Whether any point of the segment `start..end` is within [`CULL_DISTANCE`] of `eye`.
fn near_eye(eye: Vec3, start: Vec3, end: Vec3) -> bool {
    let along = end - start;
    let length_squared = along.length_squared();
    let t = if length_squared > 0.0 {
        ((eye - start).dot(along) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (start + along * t).distance_squared(eye) <= CULL_DISTANCE * CULL_DISTANCE
}

/// Play every armed mine's beam on the reference cgame cadence (`effect_cadence.rs`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn(
    beams: &mut Beams,
    snapshot: &Snapshot,
    bsp: &sjk_bsp::Bsp,
    scratch: &mut sjk_bsp::TraceScratch,
    eye: Vec3,
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    now: Instant,
    presentation_time: i32,
) {
    if !auxiliary.continuous.due(now) {
        return;
    }
    let mut budget = TRACE_BUDGET;
    for state in &snapshot.entities {
        let Some(beam) = beam(state) else {
            continue;
        };
        let origin = Vec3::from_array(beam.origin);
        let Some(end) = beams.end(state.number(), &beam, &mut budget, || {
            trace_end(origin, Vec3::from_array(beam.direction), bsp, scratch)
        }) else {
            continue;
        };
        if !near_eye(eye, origin, end) {
            continue;
        }
        let first = particles.len();
        effect_runtime::spawn_effect(
            particles,
            auxiliary,
            effects,
            vfs,
            beam.effect,
            origin,
            now,
            u32::from(state.number()) ^ (presentation_time as u32).rotate_left(11),
            0,
            audio,
            combat_effects::rotation_from_direction(beam.direction),
        );
        for particle in particles.get_mut(first..).into_iter().flatten() {
            if let Some(streak) = &mut particle.streak {
                *streak = end - particle.motion.sample().origin;
                particle.trace_streak = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

    fn mine(flags: u32, time: i32, bolt2: u32) -> EntityState {
        let mut state = EntityState::zero(1, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(14, u32::from(WP_TRIP_MINE));
        state.set_raw_field(19, flags);
        state.set_raw_field(65, time as u32);
        state.set_raw_field(63, bolt2);
        state
    }

    #[test]
    fn only_armed_stuck_mines_have_a_beam() {
        assert!(beam(&mine(0, -1, 0)).is_none());
        assert!(beam(&mine(EF_FIRING, 0, 0)).is_none());
        assert_eq!(beam(&mine(EF_FIRING, -1, 0)).unwrap().effect, LASER);
        assert_eq!(beam(&mine(EF_FIRING, -1, 1)).unwrap().effect, GLOW);
    }

    #[test]
    fn far_beams_are_not_played() {
        let start = Vec3::ZERO;
        let end = Vec3::new(16_384.0, 0.0, 0.0);
        // Near the mine, near the far end and beside the middle of a long beam.
        for eye in [
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::new(16_000.0, 500.0, 0.0),
            Vec3::new(8_000.0, 8_000.0, 0.0),
        ] {
            assert!(near_eye(eye, start, end), "{eye}");
        }
        assert!(!near_eye(Vec3::new(8_000.0, 9_000.0, 0.0), start, end));
        assert!(!near_eye(Vec3::new(-9_000.0, 0.0, 0.0), start, end));
        // A beam that stops at a wall beside the mine is a point.
        assert!(near_eye(Vec3::new(0.0, 0.0, 100.0), start, start));
        assert!(!near_eye(Vec3::new(0.0, 0.0, 9_000.0), start, start));
    }

    #[test]
    fn beams_are_traced_once_and_within_the_budget() {
        let mut beams = Beams::default();
        let traced = std::cell::Cell::new(0);
        let trace = || {
            traced.set(traced.get() + 1);
            Vec3::X
        };
        let beam = beam(&mine(EF_FIRING, -1, 0)).unwrap();
        let mut budget = 1;
        assert_eq!(beams.end(1, &beam, &mut budget, trace), Some(Vec3::X));
        // Kept: no new trace, no budget.
        assert_eq!(beams.end(1, &beam, &mut budget, trace), Some(Vec3::X));
        assert_eq!(traced.get(), 1);
        // Another mine waits for the next tick once the budget is spent.
        assert_eq!(beams.end(2, &beam, &mut budget, trace), None);
        // A moved mine is traced again.
        let mut moved = beam;
        moved.origin[0] += 8.0;
        let mut budget = 1;
        assert!(beams.end(1, &moved, &mut budget, trace).is_some());
        assert_eq!(traced.get(), 2);
    }
}
