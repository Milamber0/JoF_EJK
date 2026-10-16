//! Room for new effect particles when the pool is full.
//!
//! Effects stop at [`MAX_PARTICLES`]. Stock FX has a cap of its own (`MAX_EFFECTS`
//! 1800, `codemp/client/FxPrimitives.h`), and once its list is full
//! `FX_GetValidEffect` (`codemp/client/FxUtil.cpp`) frees `effectList[0]` and reuses
//! it, so every new primitive replaces the previous new one. A rocket barrage fills
//! either cap in a second: JoF's HD `rocket/shot` keeps about 1,200 particles alive
//! per rocket in flight (six fire puffs for half a second and two or three smoke puffs
//! for two to three seconds, played every 8 ms). Stock then shows only the newest
//! puff, and SJK refused the new ones: both make the trails vanish while the old smoke
//! lingers.
//!
//! [`Room::make_room`] runs once at the start of each frame. When fewer slots than its
//! headroom are free, it removes the effect particles closest to the end of their
//! lives, the faded tail of old smoke, so the frame's new particles (trail puffs at
//! the rockets, impacts) always fit. The headroom grows when a frame still ran out
//! (refusals are counted by [`effect_fits`]) and shrinks back slowly. Selection is
//! linear (`select_nth_unstable`) over preallocated scratch: no allocation, no
//! quadratic work. Per-frame billboards (talk balloons, pickup icons, ropes) are never
//! removed; they keep their reserved slots ([`crate::particle_types::FRAME_BILLBOARD_RESERVE`]).

use crate::particle_types::{MAX_PARTICLES, PARTICLE_POOL, Particle, PrimitiveShape};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

/// Effect spawns refused since the last [`take_refused`].
static REFUSED: AtomicU32 = AtomicU32::new(0);

/// Slots [`Room`] keeps free at least once the pool is busy: a few frames of a
/// barrage's trail puffs and an explosion.
pub(crate) const MIN_HEADROOM: usize = 256;

/// The most it keeps free, so a burst never empties the pool of live effects.
pub(crate) const MAX_HEADROOM: usize = MAX_PARTICLES / 4;

/// Whether an effect particle fits in a pool holding `len` particles. A refusal is
/// counted, and [`Room::make_room`] makes more room the next frame.
pub(crate) fn effect_fits(len: usize) -> bool {
    if len < MAX_PARTICLES {
        return true;
    }
    REFUSED.fetch_add(1, Ordering::Relaxed);
    false
}

/// Effect spawns refused since the last call, for [`Room::make_room`].
pub(crate) fn take_refused() -> u32 {
    REFUSED.swap(0, Ordering::Relaxed)
}

/// Keeps room for each frame's new effect particles.
pub(crate) struct Room {
    headroom: usize,
    /// `(remaining microseconds, pool index)` of the effect particles; scratch.
    order: Vec<(u64, u32)>,
    /// Pool indices chosen for removal; scratch, all false between calls.
    doomed: Vec<bool>,
    /// Particles removed before their time, for the frame report.
    pub(crate) evicted: u64,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            headroom: MIN_HEADROOM,
            order: Vec::with_capacity(PARTICLE_POOL),
            doomed: vec![false; PARTICLE_POOL],
            evicted: 0,
        }
    }
}

impl Room {
    /// The slots kept free now.
    #[cfg(test)]
    pub(crate) fn headroom(&self) -> usize {
        self.headroom
    }

    /// Free room for this frame's spawns; `refused` effect spawns found the pool full
    /// since the last call. Returns how many particles were removed.
    pub(crate) fn make_room(
        &mut self,
        particles: &mut Vec<Particle>,
        now: Instant,
        refused: u32,
    ) -> usize {
        self.adapt(refused);
        let free = MAX_PARTICLES.saturating_sub(particles.len());
        let need = self.headroom.saturating_sub(free);
        if need == 0 {
            return 0;
        }
        if self.doomed.len() < particles.len() {
            self.doomed.resize(particles.len(), false);
        }
        self.order.clear();
        for (index, particle) in particles.iter().enumerate() {
            if matches!(particle.shape, PrimitiveShape::FrameBillboard) {
                continue;
            }
            let end = particle.spawned_at + particle.delay + particle.lifetime;
            let remaining = end.saturating_duration_since(now).as_micros();
            self.order
                .push((u64::try_from(remaining).unwrap_or(u64::MAX), index as u32));
        }
        let count = need.min(self.order.len());
        if count == 0 {
            return 0;
        }
        // The `count` soonest to end, in linear time; ties fall to the older slot.
        if count < self.order.len() {
            self.order.select_nth_unstable(count - 1);
        }
        let chosen = &self.order[..count];
        for &(_, index) in chosen {
            self.doomed[index as usize] = true;
        }
        let doomed = &self.doomed;
        let mut index = 0;
        particles.retain(|_| {
            let keep = !doomed[index];
            index += 1;
            keep
        });
        for &(_, index) in chosen {
            self.doomed[index as usize] = false;
        }
        self.evicted += count as u64;
        count
    }

    /// Grow the headroom by twice what was refused; otherwise ease back toward the
    /// minimum by a sixteenth of the excess per frame.
    fn adapt(&mut self, refused: u32) {
        if refused > 0 {
            let grown = self.headroom.saturating_add(2 * refused as usize);
            self.headroom = grown.min(MAX_HEADROOM);
        } else if self.headroom > MIN_HEADROOM {
            self.headroom -= (self.headroom - MIN_HEADROOM).div_ceil(16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use std::sync::Arc;
    use std::time::Duration;

    /// A particle `age` into a life of `life`, at `x`.
    fn particle(now: Instant, age: u64, life: u64, x: f32, shape: PrimitiveShape) -> Particle {
        let constant = |value| {
            crate::effect_envelope::Envelope::from_values(
                value,
                value,
                0.0,
                sjk_effect::CurveFlags::default(),
            )
        };
        let spawned_at = now - Duration::from_millis(age);
        Particle {
            motion: crate::particle_motion::Motion::new(
                Vec3::new(x, 0.0, 0.0),
                Vec3::ZERO,
                Vec3::ZERO,
                0.0,
                0.0,
                spawned_at,
            ),
            spawned_at,
            delay: Duration::ZERO,
            lifetime: Duration::from_millis(life),
            size: constant(4.0),
            start_length: 1.0,
            end_length: 1.0,
            streak: None,
            trace_streak: false,
            normal: None,
            alpha: constant(1.0),
            use_alpha: true,
            set_shader_time: false,
            rgb: [constant(1.0); 3],
            seed: 0,
            shader: Arc::from("gfx/effects/test"),
            physics: crate::particle_physics::State::new(
                Arc::new(sjk_effect::EffectDefinition::default()),
                0,
                0,
                0.0,
                sjk_effect::PrimitiveFlags::default(),
            ),
            shape,
        }
    }

    fn remaining(particle: &Particle, now: Instant) -> Duration {
        (particle.spawned_at + particle.delay + particle.lifetime).saturating_duration_since(now)
    }

    #[test]
    fn a_full_pool_frees_its_headroom_from_the_soonest_to_end() {
        let now = Instant::now() + Duration::from_secs(10);
        // Ages and lives spread so remaining lives are all different.
        let mut particles: Vec<Particle> = (0..MAX_PARTICLES as u64)
            .map(|i| {
                particle(
                    now,
                    i % 997,
                    3_000 + (i * 7) % 1_901,
                    i as f32,
                    PrimitiveShape::Billboard,
                )
            })
            .collect();
        let mut lives: Vec<Duration> = particles.iter().map(|p| remaining(p, now)).collect();
        lives.sort();
        let cutoff = lives[MIN_HEADROOM - 1];
        let mut room = Room::default();
        assert_eq!(room.make_room(&mut particles, now, 0), MIN_HEADROOM);
        assert_eq!(particles.len(), MAX_PARTICLES - MIN_HEADROOM);
        // Everything left lives at least as long as the last one removed.
        assert!(particles.iter().all(|p| remaining(p, now) >= cutoff));
        // With room to spare nothing more goes.
        assert_eq!(room.make_room(&mut particles, now, 0), 0);
        assert_eq!(room.evicted, MIN_HEADROOM as u64);
    }

    #[test]
    fn per_frame_billboards_are_never_removed() {
        let now = Instant::now() + Duration::from_secs(10);
        let mut particles: Vec<Particle> = (0..MAX_PARTICLES - 40)
            .map(|i| particle(now, 100, 2_000, i as f32, PrimitiveShape::Billboard))
            .collect();
        // Balloons and icons that would otherwise be the soonest to end.
        particles
            .extend((0..40).map(|i| particle(now, 0, 1, i as f32, PrimitiveShape::FrameBillboard)));
        let mut room = Room::default();
        room.make_room(&mut particles, now, 0);
        let billboards = particles
            .iter()
            .filter(|p| matches!(p.shape, PrimitiveShape::FrameBillboard))
            .count();
        assert_eq!(billboards, 40);
        assert_eq!(particles.len(), MAX_PARTICLES - MIN_HEADROOM);
    }

    #[test]
    fn headroom_grows_with_refusals_and_eases_back() {
        let now = Instant::now();
        let mut particles = Vec::new();
        let mut room = Room::default();
        room.make_room(&mut particles, now, 100);
        assert_eq!(room.headroom(), MIN_HEADROOM + 200);
        room.make_room(&mut particles, now, 10_000);
        assert_eq!(room.headroom(), MAX_HEADROOM);
        for _ in 0..400 {
            room.make_room(&mut particles, now, 0);
        }
        assert_eq!(room.headroom(), MIN_HEADROOM);
    }

    /// A barrage: every frame `per_frame` new long-lived puffs, as rocket trails add.
    /// Without room the pool fills and new puffs stop; with it every frame's puffs fit.
    #[test]
    fn a_barrage_keeps_spawning_new_puffs() {
        let start = Instant::now();
        let per_frame = 120;
        let run = |with_room: bool| {
            let mut particles = Vec::with_capacity(PARTICLE_POOL);
            let mut room = Room::default();
            let mut last_frame_spawned = 0;
            for frame in 0..200_u64 {
                let now = start + Duration::from_millis(frame * 8);
                if with_room {
                    // A local count, so parallel tests touching `REFUSED` don't matter.
                    room.make_room(&mut particles, now, 0);
                }
                particles.retain(|p: &Particle| remaining(p, now) > Duration::ZERO);
                last_frame_spawned = 0;
                for i in 0..per_frame {
                    if particles.len() >= MAX_PARTICLES {
                        break;
                    }
                    particles.push(particle(now, 0, 2_500, i as f32, PrimitiveShape::Billboard));
                    last_frame_spawned += 1;
                }
            }
            last_frame_spawned
        };
        assert_eq!(run(false), 0, "a full pool refuses the newest puffs");
        assert_eq!(run(true), per_frame, "room is made for every new puff");
    }

    #[test]
    fn refusals_are_counted_for_the_next_frame() {
        assert!(effect_fits(MAX_PARTICLES - 1));
        assert!(!effect_fits(MAX_PARTICLES));
        assert!(take_refused() >= 1);
    }
}

#[cfg(test)]
#[path = "particle_room_barrage.rs"]
mod barrage;
