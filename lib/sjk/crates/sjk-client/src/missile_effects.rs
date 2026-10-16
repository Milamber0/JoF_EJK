//! Legacy per-frame missile effect dispatch behind the client adapter.
//!
//! `CG_Missile` selects primary/alternate trail callbacks and dynamic lights
//! in `codemp/cgame/cg_ents.c:2493-2587`. The callback table is populated by
//! `CG_RegisterWeapon`: baton/melee/saber have no callback
//! (`cg_weaponinit.c:136-171`), concussion is at `:173-196`, Bryar at
//! `:206-230`, blaster/emplaced at `:248-272`, disruptor at `:281-304`,
//! bowcaster at `:330-353`, repeater at `:363-386`, DEMP2 at `:396-419`,
//! flechette at `:431-454`, rocket (including its 125-unit RGB 1/1/0.5 light)
//! at `:462-486`, thermal at `:498-521`, tripmine at `:534-557`, detpack at
//! `:568-591`, and turret at `:598-607`. The callbacks and registered EFX are
//! in `fx_blaster.c:33-43`, `fx_bryarpistol.c:37-47,86-106,157-167,224-234`,
//! `fx_bowcaster.c:33-43,73-83`, `fx_heavyrepeater.c:33-43,141-155`,
//! `fx_demp2.c:33-43`, `fx_flechette.c:33-43,79-89`, and
//! `fx_rocketlauncher.c:33-43,73-83`.
//! The weapon-local paths are registered alongside that table; the global
//! `CG_RegisterEffects` startup call and turret-shot registration are at
//! `cg_main.c:998-1004,1148-1154`.

use crate::legacy_evaluate_trajectory;
use sjk_protocol::{GameState, MAX_LEGACY_ENTITIES, Snapshot};

const ET_MISSILE: u8 = 3; // codemp/game/bg_public.h:1248
const EF_NODRAW: u32 = 1 << 8; // codemp/game/bg_public.h:647
const EF_ALT_FIRING: u32 = 1 << 10; // codemp/game/bg_public.h:649
const EF_JETPACK_ACTIVE: u32 = 1 << 11; // codemp/game/bg_public.h:650
const WP_SABER: u8 = 3; // codemp/game/bg_weapons.h:34
const WP_NUM_WEAPONS: u8 = 19; // codemp/game/bg_weapons.h:54
const G2_MODEL_PART: u8 = 50; // codemp/game/bg_public.h:189
const CS_EFFECTS: usize = 1_355; // codemp/game/bg_public.h:145-148
const MAX_FX: usize = 64; // codemp/qcommon/q_shared.h:923

/// The generic point light authored by one legacy missile weapon mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyMissileLight {
    pub origin: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
}

/// One logical `missileTrailFunc` invocation for a presented frame.
///
/// Charged Bryar alt fire additionally repeats `bryar/crackleShot`
/// `generic1 - 1` times (`fx_bryarpistol.c:96-105`). Keeping that multiplicity in
/// this request preserves a one-request-per-missile oracle and avoids a
/// transient allocation proportional to charge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyMissileEffectRequest<'a> {
    pub entity_number: u16,
    pub effect_name: &'a str,
    pub extra_effect_name: Option<&'a str>,
    pub extra_repetitions: u8,
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub light: Option<LegacyMissileLight>,
}

/// Deterministic accounting for the allocation-free frame adapter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyMissileEffectMetrics {
    pub decoded_missiles: usize,
    pub trail_eligible_missiles: usize,
    pub play_requests: usize,
    pub light_requests: usize,
    pub no_trail_missiles: usize,
    pub custom_effect_requests: usize,
    pub missing_custom_effects: usize,
    pub suppressed_early_returns: usize,
    pub unsupported_vehicle_overrides: usize,
}

#[derive(Clone, Copy, Debug)]
enum EffectRef {
    Static(&'static str),
    ConfigString(usize),
    Vehicle(usize),
}

#[derive(Clone, Copy, Debug)]
struct PendingRequest {
    entity_number: u16,
    effect: EffectRef,
    extra_effect: Option<&'static str>,
    extra_repetitions: u8,
    origin: [f32; 3],
    direction: [f32; 3],
    light: Option<LegacyMissileLight>,
}

/// Per-mode BaseJKA projectile-think and dynamic-light selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyMissileMode {
    pub effect_name: Option<&'static str>,
    pub extra_effect_name: Option<&'static str>,
    pub light_radius: f32,
    pub light_color: [f32; 3],
}

const NONE: LegacyMissileMode = LegacyMissileMode {
    effect_name: None,
    extra_effect_name: None,
    light_radius: 0.0,
    light_color: [0.0; 3],
};
const BRYAR: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("bryar/shot"),
    extra_effect_name: None,
    ..NONE
};
const BRYAR_ALT: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("bryar/shot"),
    extra_effect_name: Some("bryar/crackleShot"),
    ..NONE
};
const BLASTER: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("blaster/shot"),
    ..NONE
};
const BOWCASTER: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("bowcaster/shot"),
    ..NONE
};
const REPEATER: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("repeater/projectile"),
    ..NONE
};
const REPEATER_ALT: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("repeater/alt_projectile"),
    ..NONE
};
const DEMP2: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("demp2/projectile"),
    ..NONE
};
const FLECHETTE: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("flechette/shot"),
    ..NONE
};
const FLECHETTE_ALT: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("flechette/alt_shot"),
    ..NONE
};
const ROCKET: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("rocket/shot"),
    light_radius: 125.0,
    light_color: [1.0, 1.0, 0.5],
    ..NONE
};
const CONCUSSION: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("concussion/shot"),
    ..NONE
};
const TURRET: LegacyMissileMode = LegacyMissileMode {
    effect_name: Some("turret/shot"),
    ..NONE
};

/// Return the exact BaseJKA `weaponInfo_t` projectile-think selection.
///
/// Numeric IDs follow `weapon_t` in `codemp/game/bg_weapons.h:30-60`.
/// Zero-function modes deliberately return [`NONE`].
pub fn legacy_missile_mode(weapon: u8, alternate: bool) -> LegacyMissileMode {
    match (weapon, alternate) {
        (4 | 16, false) => BRYAR,
        (4 | 16, true) => BRYAR_ALT,
        (5 | 17, _) => BLASTER,
        (7, _) => BOWCASTER,
        (8, false) => REPEATER,
        (8, true) => REPEATER_ALT,
        (9, false) => DEMP2,
        (10, false) => FLECHETTE,
        (10, true) => FLECHETTE_ALT,
        (11, _) => ROCKET,
        (15, _) => CONCUSSION,
        (18, false) => TURRET,
        _ => NONE,
    }
}

/// Map-lifetime effect cache and fixed-capacity per-frame request pool.
pub struct LegacyMissileEffects {
    config_effects: [Option<Box<str>>; MAX_FX],
    vehicles: Option<crate::vehicle_missile_effects::VehicleMissiles>,
    requests: Vec<PendingRequest>,
}

impl LegacyMissileEffects {
    /// Construct an empty runtime for menu/backdrop scenes.
    pub fn empty() -> Self {
        Self {
            config_effects: std::array::from_fn(|_| None),
            vehicles: None,
            requests: Vec::with_capacity(MAX_LEGACY_ENTITIES),
        }
    }

    /// Cache `CS_EFFECTS` once per gamestate for CG_Missile's custom-effect
    /// override (`cg_ents.c:2493-2529`).
    pub fn from_game_state(game_state: &GameState) -> Self {
        Self {
            config_effects: std::array::from_fn(|index| {
                game_state
                    .config_string(CS_EFFECTS + index)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(|name| name.to_ascii_lowercase().into_boxed_str())
            }),
            ..Self::empty()
        }
    }

    /// Authored presentation at the vehicle weapon index carried by a missile.
    pub fn vehicle_weapon(
        &self,
        index: usize,
    ) -> Option<&sjk_game_jka::vehicle_presentation::WeaponPresentation> {
        self.vehicles.as_ref()?.weapon(index)
    }

    /// Rigid vehicle projectile assets to preload outside the frame loop.
    pub fn vehicle_model_paths(&self) -> impl Iterator<Item = &str> {
        self.vehicles
            .iter()
            .flat_map(|vehicles| vehicles.weapons())
            .filter_map(|weapon| weapon.model.as_deref())
    }

    /// Load vehicle weapon presentation in the same model registration order as cgame.
    pub fn load_vehicles(&mut self, game: &GameState, vfs: &sjk_vfs::VirtualFileSystem) {
        self.vehicles = Some(crate::vehicle_missile_effects::VehicleMissiles::load(
            vfs, game,
        ));
    }

    /// Register a changed vehicle model and return its effects for loading-time precache.
    pub fn refresh_vehicle(&mut self, index: usize, game: &GameState) -> Vec<String> {
        let Some(vehicles) = self.vehicles.as_mut() else {
            return Vec::new();
        };
        vehicles.refresh(index, game);
        vehicles.effect_names().map(str::to_owned).collect()
    }

    /// Replace one `CS_EFFECTS` slot while retaining entity latches and deadlines.
    pub fn refresh_config_string(&mut self, index: usize, game_state: &GameState) -> Option<&str> {
        let slot = index.checked_sub(CS_EFFECTS)?;
        let effect = self.config_effects.get_mut(slot)?;
        *effect = game_state
            .config_string(index)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| name.to_ascii_lowercase().into_boxed_str());
        effect.as_deref()
    }

    /// Evaluate all visible missiles once for the rendered `cg.time`.
    ///
    /// Origin uses the shared BG_EvaluateTrajectory adapter. Projectile-think
    /// direction is normalized `pos.trDelta` with +Z fallback, matching every
    /// stock `FX_*ProjectileThink` callback cited in this module's docs.
    pub fn update(
        &mut self,
        snapshot: &Snapshot,
        presentation_time: i32,
    ) -> LegacyMissileEffectMetrics {
        self.requests.clear();
        let mut metrics = LegacyMissileEffectMetrics::default();
        for entity in &snapshot.entities {
            if entity.entity_type() != ET_MISSILE {
                continue;
            }
            metrics.decoded_missiles += 1;
            let mut weapon = entity.weapon();
            if weapon > WP_NUM_WEAPONS && weapon != G2_MODEL_PART {
                weapon = 0;
            }
            if weapon == WP_SABER && entity.e_flags() & EF_NODRAW != 0 {
                metrics.suppressed_early_returns += 1;
                continue;
            }
            let origin = legacy_evaluate_trajectory(
                entity.trajectory_base(),
                entity.trajectory_delta(),
                entity.trajectory_type(),
                entity.trajectory_time(),
                entity.trajectory_duration(),
                presentation_time,
            );
            let direction = normalize_or_up(entity.trajectory_delta());
            let override_index = usize::from(entity.other_entity_num2());
            if override_index != 0 && weapon != WP_SABER {
                if entity.e_flags() & EF_JETPACK_ACTIVE != 0 {
                    let presentation = self
                        .vehicles
                        .as_ref()
                        .and_then(|vehicles| vehicles.weapon(override_index));
                    if presentation.is_some_and(|weapon| weapon.shot_effect.is_some()) {
                        self.requests.push(PendingRequest {
                            entity_number: entity.number(),
                            effect: EffectRef::Vehicle(override_index),
                            extra_effect: None,
                            extra_repetitions: 0,
                            origin,
                            direction,
                            light: None,
                        });
                        metrics.trail_eligible_missiles += 1;
                        metrics.play_requests += 1;
                        metrics.custom_effect_requests += 1;
                        continue;
                    }
                    if presentation.is_some_and(|weapon| weapon.model.is_some()) {
                        metrics.no_trail_missiles += 1;
                        continue;
                    }
                    metrics.unsupported_vehicle_overrides += 1;
                    // With no vehicle override stock tries the CS_EFFECTS slot below.
                }
                if override_index >= MAX_FX || self.config_effects[override_index].is_none() {
                    metrics.missing_custom_effects += 1;
                    continue;
                }
                self.requests.push(PendingRequest {
                    entity_number: entity.number(),
                    effect: EffectRef::ConfigString(override_index),
                    extra_effect: None,
                    extra_repetitions: 0,
                    origin,
                    direction,
                    light: None,
                });
                metrics.trail_eligible_missiles += 1;
                metrics.play_requests += 1;
                metrics.custom_effect_requests += 1;
                continue;
            }
            let mode = legacy_missile_mode(weapon, entity.e_flags() & EF_ALT_FIRING != 0);
            let Some(effect) = mode.effect_name else {
                metrics.no_trail_missiles += 1;
                continue;
            };
            let light = (mode.light_radius > 0.0).then_some(LegacyMissileLight {
                origin,
                radius: mode.light_radius,
                color: mode.light_color,
            });
            self.requests.push(PendingRequest {
                entity_number: entity.number(),
                effect: EffectRef::Static(effect),
                extra_effect: mode.extra_effect_name,
                extra_repetitions: if mode.extra_effect_name.is_some() {
                    entity.generic1().saturating_sub(1)
                } else {
                    0
                },
                origin,
                direction,
                light,
            });
            metrics.trail_eligible_missiles += 1;
            metrics.play_requests += 1;
            metrics.light_requests += usize::from(light.is_some());
        }
        metrics
    }

    /// Iterate the current fixed-pool output without allocating or cloning.
    pub fn requests(&self) -> impl ExactSizeIterator<Item = LegacyMissileEffectRequest<'_>> {
        self.requests.iter().map(|request| {
            let effect_name = match request.effect {
                EffectRef::Static(name) => name,
                EffectRef::Vehicle(index) => self
                    .vehicles
                    .as_ref()
                    .and_then(|v| v.weapon(index))
                    .and_then(|w| w.shot_effect.as_deref())
                    .expect("pending vehicle effects are registered"),
                EffectRef::ConfigString(index) => self.config_effects[index]
                    .as_deref()
                    .expect("pending custom effects are registered"),
            };
            LegacyMissileEffectRequest {
                entity_number: request.entity_number,
                effect_name,
                extra_effect_name: request.extra_effect,
                extra_repetitions: request.extra_repetitions,
                origin: request.origin,
                direction: request.direction,
                light: request.light,
            }
        })
    }

    /// Cached effect names used to preload EFX definitions at map load.
    pub fn effect_names(&self) -> impl Iterator<Item = &str> {
        self.config_effects
            .iter()
            .filter_map(Option::as_deref)
            .chain(
                self.vehicles
                    .iter()
                    .flat_map(|vehicles| vehicles.effect_names()),
            )
            .chain(
                (0..=18)
                    .flat_map(|weapon| [false, true].map(move |alternate| (weapon, alternate)))
                    .filter_map(|(weapon, alternate)| {
                        legacy_missile_mode(weapon, alternate).effect_name
                    }),
            )
    }
}

fn normalize_or_up(value: [f32; 3]) -> [f32; 3] {
    let length_squared = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    if length_squared <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        let inverse = length_squared.sqrt().recip();
        value.map(|component| component * inverse)
    }
}
