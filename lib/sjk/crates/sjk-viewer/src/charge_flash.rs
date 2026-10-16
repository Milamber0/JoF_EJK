//! The glow on a charging weapon's muzzle (`CG_AddPlayerWeapon`, `cg_weapons.c`
//! "Do special charge bits"): while the Bryar pistols charge their alt fire, the
//! bowcaster its primary, or the DEMP2 its alt fire, a sprite at `tag_flash` grows
//! over a second (`gfx/effects/bryarFrontFlash`, `greenFrontFlash`,
//! `gfx/misc/lightningFlash` at 1.75 times the size), flickering by up to half again.
//! It is drawn each frame, as `FX_AddSprite` with a one-millisecond life.

use crate::Particle;
use crate::effect_runtime::EffectLibrary;
use crate::particle_types::PrimitiveShape;
use glam::Vec3;
use std::time::{Duration, Instant};

const WP_BRYAR_PISTOL: u8 = 4;
const WP_BOWCASTER: u8 = 7;
const WP_DEMP2: u8 = 9;
const WP_BRYAR_OLD: u8 = 16;
/// `weaponstate_t` (`bg_public.h`).
const WEAPON_CHARGING: u8 = 4;
const WEAPON_CHARGING_ALT: u8 = 5;
/// `ET_PLAYER`.
const ET_PLAYER: u8 = 1;
const CLIENTS: usize = 64;

/// The shaders the sprites use, for loading with the other code shaders.
pub(crate) const SHADERS: [&str; 3] = [
    "gfx/effects/bryarFrontFlash",
    "gfx/effects/greenFrontFlash",
    "gfx/misc/lightningFlash",
];

/// One player's weapon, its state (`modelindex2` in the entity) and when its charge
/// began (`constantLight`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Charge {
    pub(crate) weapon: u8,
    pub(crate) state: u8,
    pub(crate) since: i32,
}

/// The sprite's shader and size (`3 * val * scale`, with `val` the charge's
/// seconds capped at 1 plus up to 0.5 of flicker), or `None` when not charging.
pub(crate) fn sprite(charge: Charge, time: i32, flicker: f32) -> Option<(&'static str, f32)> {
    let (shader, scale) = match (charge.weapon, charge.state) {
        (WP_BRYAR_PISTOL | WP_BRYAR_OLD, WEAPON_CHARGING_ALT) => (SHADERS[0], 1.0),
        (WP_BOWCASTER, WEAPON_CHARGING) => (SHADERS[1], 1.0),
        (WP_DEMP2, WEAPON_CHARGING_ALT) => (SHADERS[2], 1.75),
        _ => return None,
    };
    let value =
        ((time - charge.since) as f32 * 0.001).clamp(0.0, 1.0) + flicker.clamp(0.0, 1.0) * 0.5;
    Some((shader, 3.0 * value * scale))
}

/// Every player's charge state this frame; the local player's from prediction.
#[derive(Clone, Copy)]
pub(crate) struct Charges {
    entries: [Option<Charge>; CLIENTS],
}

impl Charges {
    pub(crate) const EMPTY: Self = Self {
        entries: [None; CLIENTS],
    };

    pub(crate) fn collect(
        snapshot: Option<&sjk_protocol::Snapshot>,
        predicted: Option<&sjk_client::pmove::MovementState>,
    ) -> Self {
        let mut charges = Self::EMPTY;
        let Some(snapshot) = snapshot else {
            return charges;
        };
        for state in snapshot
            .entities
            .iter()
            .filter(|state| state.entity_type() == ET_PLAYER)
        {
            if let Some(slot) = charges.entries.get_mut(usize::from(state.number())) {
                *slot = Some(Charge {
                    weapon: state.weapon(),
                    state: state.model_index2(),
                    since: state.constant_light() as i32,
                });
            }
        }
        let local = usize::from(snapshot.player.client_num());
        if let Some(slot) = charges.entries.get_mut(local) {
            *slot = Some(predicted.map_or(
                Charge {
                    weapon: snapshot.player.weapon(),
                    state: snapshot.player.weapon_state(),
                    since: snapshot.player.weapon_charge_time(),
                },
                |state| Charge {
                    weapon: state.weapon,
                    state: state.weapon_state,
                    since: state.weapon_charge_time,
                },
            ));
        }
        charges
    }

    pub(crate) fn of(&self, client: u16) -> Option<Charge> {
        self.entries.get(usize::from(client)).copied().flatten()
    }
}

/// Add this frame's sprite for `charge` at the muzzle `origin`.
pub(crate) fn push(
    particles: &mut Vec<Particle>,
    effects: &mut EffectLibrary,
    charge: Charge,
    origin: Vec3,
    time: i32,
    now: Instant,
) {
    // `Q_flrand(0, 1)`, varied per frame.
    let flicker = ((time as u32).wrapping_mul(2_654_435_761) >> 8) as f32 / (1 << 24) as f32;
    let Some((shader, size)) = sprite(charge, time, flicker) else {
        return;
    };
    if !crate::particle_types::frame_billboard_fits(particles.len()) {
        return;
    }
    let constant = |value| {
        crate::effect_envelope::Envelope::from_values(
            value,
            value,
            0.0,
            sjk_effect::CurveFlags::default(),
        )
    };
    particles.push(Particle {
        motion: crate::particle_motion::Motion::new(origin, Vec3::ZERO, Vec3::ZERO, 0.0, 0.0, now),
        spawned_at: now,
        delay: Duration::ZERO,
        lifetime: Duration::from_secs(60),
        size: constant(size),
        start_length: 1.0,
        end_length: 1.0,
        streak: None,
        trace_streak: false,
        normal: None,
        alpha: constant(0.7),
        use_alpha: true,
        set_shader_time: false,
        rgb: [constant(1.0); 3],
        seed: time as u32,
        shader: effects.shader(shader),
        physics: crate::particle_physics::State::new(
            effects.code_primitive_definition(),
            0,
            0,
            0.0,
            sjk_effect::PrimitiveFlags::default(),
        ),
        shape: PrimitiveShape::FrameBillboard,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_charging_modes_glow_and_grow_for_a_second() {
        let charge = |weapon, state| Charge {
            weapon,
            state,
            since: 1_000,
        };
        assert!(sprite(charge(WP_BRYAR_OLD, WEAPON_CHARGING), 1_500, 0.0).is_none());
        assert_eq!(
            sprite(charge(WP_BRYAR_OLD, WEAPON_CHARGING_ALT), 1_500, 0.0),
            Some((SHADERS[0], 1.5))
        );
        assert_eq!(
            sprite(charge(WP_BOWCASTER, WEAPON_CHARGING), 3_000, 0.0),
            Some((SHADERS[1], 3.0))
        );
        let (shader, size) = sprite(charge(WP_DEMP2, WEAPON_CHARGING_ALT), 3_000, 1.0).unwrap();
        assert_eq!(shader, SHADERS[2]);
        assert!((size - 3.0 * 1.5 * 1.75).abs() < 1e-4);
    }
}
