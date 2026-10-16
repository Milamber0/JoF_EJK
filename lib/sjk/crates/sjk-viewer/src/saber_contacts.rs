//! Presentation-only blade/architecture contact. Never feeds simulation or combat traces.
use crate::{decal_store::DecalStore, saber::Instance};
use glam::Vec3;
use std::{sync::Arc, time::Instant};

pub(crate) const MATERIALS: [&str; 2] = ["gfx/damage/rivetmark", "gfx/effects/saberdamageglow"];

#[derive(Clone, Copy, Default)]
struct Contact {
    point: Vec3,
    normal: Vec3,
    base: Vec3,
    time: i64,
    seen: u64,
    spark: i64,
    sound: Option<i64>,
    valid: bool,
}

/// Fixed O(1) source slots for held and thrown blades; allocated once per map.
pub(crate) struct Runtime {
    states: Box<[Contact]>,
    frame: u64,
    last_time: Option<i64>,
    materials: [Arc<str>; 2],
    pub(crate) enabled: bool,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            states: vec![Contact::default(); 1024 * 24].into_boxed_slice(),
            frame: 0,
            last_time: None,
            materials: MATERIALS.map(Arc::from),
            enabled: true,
        }
    }
}
impl Runtime {
    pub(crate) fn clear(&mut self) {
        self.states.fill(Contact::default());
        self.last_time = None;
    }

    /// One scratch-backed MASK_SOLID trace per displayed blade and presentation time.
    /// Sustained contacts connect only across consecutive frames on the same plane.
    pub(crate) fn update(
        &mut self,
        blades: &mut [Instance],
        bsp: &sjk_bsp::Bsp,
        scratch: &mut sjk_bsp::TraceScratch,
        world: &crate::decal_marks::DecalSurfaces,
        decals: &mut DecalStore,
        time: i64,
        now: Instant,
        slots: [u8; 2],
        mut impact: impl FnMut(Vec3, Vec3, crate::saber_rgb::BladeColor, bool, bool, bool, u32),
    ) {
        if !self.enabled {
            if self.last_time.is_some() {
                self.clear();
            }
            return;
        }
        let repeated = self.last_time == Some(time);
        if self.last_time.is_some_and(|last| time < last) {
            self.clear();
        }
        self.last_time = Some(time);
        if !repeated {
            self.frame = self.frame.wrapping_add(1);
        }
        for pair in blades.chunks_exact_mut(2) {
            let instance = pair[0];
            let Some((key, blade, color, no_light)) = instance.contact() else {
                continue;
            };
            let Some(state) = self.states.get_mut(key) else {
                continue;
            };
            let base = Vec3::from_array(blade.base);
            let tip = base + Vec3::from_array(blade.direction) * (blade.length + 1.);
            let trace = bsp.trace_box_with(
                scratch,
                base.to_array(),
                tip.to_array(),
                sjk_bsp::Aabb::POINT,
                0x1001,
            );
            // CG_AddSaberBlade clips before checking sky or NO_WALL_MARKS.
            // Do not change the extension state: leaving the wall restores the blade.
            if trace.fraction < 1. {
                let length = base.distance(Vec3::from_array(trace.end_position));
                for instance in pair {
                    instance.clip_length(length);
                }
            }
            if trace.fraction >= 1.
                || !instance.wall_marks()
                || trace.start_solid
                || trace.surface_flags & crate::particle_physics::SURF_NOIMPACT != 0
            {
                state.valid = false;
                continue;
            }
            let point = Vec3::from_array(trace.end_position);
            let normal = Vec3::from_array(trace.plane.map_or([0., 0., 1.], |p| p.normal))
                .normalize_or(Vec3::Z);
            if repeated {
                impact(point, normal, color, no_light, false, false, key as u32);
                continue;
            }
            let markable = trace.surface_flags & crate::particle_physics::SURF_NOMARKS == 0;
            let continuous = connected(*state, point, normal, base, time, self.frame);
            // Distance sampling prevents stationary contact from consuming the decal ring.
            if markable && continuous && point.distance_squared(state.point) >= 0.04 {
                decals.saber_cut(
                    world,
                    bsp,
                    [state.point, point],
                    normal,
                    now,
                    &self.materials,
                    slots,
                );
                state.point = point;
            } else if markable && !continuous {
                state.point = point;
                let offset = normal.any_orthonormal_vector() * 0.1;
                decals.saber_cut(
                    world,
                    bsp,
                    [point - offset, point + offset],
                    normal,
                    now,
                    &self.materials,
                    slots,
                );
            }
            let touching = state.seen != 0 && state.seen.wrapping_add(1) == self.frame;
            let spark = !touching || time - state.spark >= 40;
            if spark {
                state.spark = time;
            }
            // Independent of frame rate and spark cadence; never replay paused frames.
            // Stock sounds only once the blade was already in the wall the frame
            // before (`trail.haveOldPos`), never on the first touch.
            let sound = markable && sound_due(touching, state.sound, time);
            if sound {
                state.sound = Some(time);
            }
            impact(point, normal, color, no_light, spark, sound, key as u32);
            state.normal = normal;
            state.base = base;
            state.time = time;
            state.seen = self.frame;
            state.valid = markable;
        }
    }
}
fn connected(old: Contact, point: Vec3, normal: Vec3, base: Vec3, time: i64, frame: u64) -> bool {
    old.valid
        && old.seen.wrapping_add(1) == frame
        && time > old.time
        && time - old.time <= 100
        && old.normal.dot(normal) > 0.995
        && (point - old.point).dot(normal).abs() < 0.5
        && base.distance_squared(old.base) < 64. * 64.
        && point.distance_squared(old.point) < 64. * 64.
}

/// Finish the existing visual submissions before their light/decal uploads.
pub(crate) fn frame(
    gpu: &mut crate::GpuState,
    time: i64,
    now: Instant,
    audio: &mut Option<crate::GameAudio>,
) {
    let slots = MATERIALS.map(|name| {
        crate::effect_blend::slot(
            gpu.particle_atlas
                .layers_for(name, 0.)
                .iter()
                .next()
                .map_or(crate::ParticleBlend::Add, |l| l.blend),
        ) as u8
    });
    let contacts = &mut gpu.effect_aux.saber_contacts;
    let decals = &mut gpu.effect_aux.decals;
    contacts.update(
        &mut gpu.saber_instances,
        &gpu.bsp,
        &mut gpu.trace_scratch,
        &gpu.decal_surfaces,
        decals,
        time,
        now,
        slots,
        |point, normal, color, no_light, spark, sound, seed| {
            if sound {
                // The contact's key is its blade's: entity number times 24, and more.
                if let Some(audio) = audio
                    .as_mut()
                    .filter(|audio| !audio.silenced(sjk_audio::SourceId(seed / 24), None))
                {
                    audio.play_on_channel(
                        SOUNDS[(seed ^ time as u32) as usize % SOUNDS.len()],
                        1.,
                        point.to_array(),
                        WALL_SOUND_SOURCE,
                        WALL_SOUND_CHANNEL,
                    );
                }
            }
            if !no_light {
                gpu.dynamic_lights
                    .push_radiant(crate::dynamic_lights::PointLight {
                        origin: (point + normal * 5.).to_array(),
                        radius: 42.,
                        color: crate::saber_submission::lights::rgb(color).map(|c| c * 1.5),
                    });
            }
            if spark {
                spawn_sparks(
                    &mut gpu.particles,
                    &mut gpu.effects,
                    point,
                    normal,
                    now,
                    seed ^ (time as u32),
                );
            }
        },
    );
    gpu.saber_instances.retain(Instance::visible);
}

// OpenJK codemp cg_players.c:6246–6249 uses these three wall sounds at 100 ms.
pub(crate) const SOUNDS: [&str; 3] = [
    "sound/weapons/saber/saberhitwall1",
    "sound/weapons/saber/saberhitwall2",
    "sound/weapons/saber/saberhitwall3",
];
/// Stock starts every wall hit on entity `-1`, `CHAN_WEAPON`
/// (`S_StartSound(trace.endpos, -1, CHAN_WEAPON, ...)`). The software mixer's
/// `S_PickChannel` then always replaces the previous sound of that entity and
/// channel, so all blades share one wall-hit voice and each new hit cuts the
/// last one off instead of stacking another copy every 100 ms.
const WALL_SOUND_SOURCE: sjk_audio::SourceId = sjk_audio::SourceId(0x6000_0000);
/// `CHAN_WEAPON`.
const WALL_SOUND_CHANNEL: sjk_audio::ChannelId = sjk_audio::ChannelId(2);

/// A wall hit sounds once the blade stayed in the wall since the previous
/// frame, at most every 100 ms per blade (`hitWallDebounceTime`).
fn sound_due(touching: bool, previous: Option<i64>, time: i64) -> bool {
    touching && previous.is_none_or(|last| time.saturating_sub(last) >= 100)
}
#[path = "saber_contact_sparks.rs"]
mod sparks;
use sparks::spawn_sparks;

#[cfg(test)]
mod tests {
    use super::sound_due;

    #[test]
    fn first_touch_is_silent() {
        assert!(!sound_due(false, None, 1_000));
        assert!(sound_due(true, None, 1_008));
    }

    #[test]
    fn sustained_contact_sounds_every_100_ms() {
        assert!(!sound_due(true, Some(1_000), 1_099));
        assert!(sound_due(true, Some(1_000), 1_100));
    }
}
