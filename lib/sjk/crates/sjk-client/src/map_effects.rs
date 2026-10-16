//! Protocol-26 `ET_FX` scheduling behind the legacy world boundary.
//!
//! This mirrors `CG_FX` in `codemp/cgame/cg_ents.c:3666-3733`: the off state,
//! one-shot value latch, `miscTime` deadline, effect configstring cache, and
//! `AngleVectors` direction are all evaluated here. Server construction of the
//! fields is documented by `codemp/game/g_misc.c:2454-2475,2501-2561,2644-2676`;
//! the state constants are from `codemp/game/bg_public.h:1387-1392`.

use crate::legacy_evaluate_trajectory;
use sjk_protocol::{GameState, MAX_LEGACY_ENTITIES, Snapshot};

const ET_FX: u8 = 17; // codemp/game/bg_public.h:1262
const FX_STATE_OFF: u8 = 0; // codemp/game/bg_public.h:1389
const FX_STATE_ONE_SHOT_LIMIT: u8 = 10; // codemp/game/bg_public.h:1391
const CS_EFFECTS: usize = 1_355; // codemp/game/bg_public.h:145-148
const MAX_FX: usize = 64; // shared/qcommon/q_shared.h:936

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EntitySideState {
    misc_time: i32,
    muzzle_flash_time: u8,
    random_state: u32,
}

#[derive(Clone, Copy, Debug)]
struct PendingRequest {
    entity_number: u16,
    effect_index: usize,
    origin: [f32; 3],
    direction: [f32; 3],
    portal: bool,
}

/// One resolved request sent from the JKA adapter to the generic EFX player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyMapEffectRequest<'a> {
    pub entity_number: u16,
    pub effect_name: &'a str,
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub portal: bool,
}

/// Per-frame accounting used by the deterministic presentation harness.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyMapEffectMetrics {
    pub decoded_entities: usize,
    pub play_requests: usize,
    pub off_states: usize,
    pub cooldown_states: usize,
    pub latched_one_shots: usize,
    pub missing_effects: usize,
}

/// Fixed-capacity, allocation-free runtime state for all legacy entity slots.
/// Side-state survives an entity leaving the PVS, matching `centity_t`:
/// `CG_ResetEntity` does not clear `miscTime` or `muzzleFlashTime`
/// (`codemp/cgame/cg_snapshot.c:42-70`). A replacement gamestate constructs a
/// new adapter and therefore performs the map/session reset.
pub struct LegacyMapEffects {
    effects: [Option<Box<str>>; MAX_FX],
    entities: Box<[EntitySideState]>,
    requests: Vec<PendingRequest>,
}

impl LegacyMapEffects {
    /// Empty registry for the client-shell backdrop before a server gamestate.
    pub fn empty() -> Self {
        Self {
            effects: std::array::from_fn(|_| None),
            entities: vec![EntitySideState::default(); MAX_LEGACY_ENTITIES].into_boxed_slice(),
            requests: Vec::with_capacity(MAX_LEGACY_ENTITIES),
        }
    }

    /// Register all `CS_EFFECTS` strings once when a gamestate becomes active.
    ///
    /// This is the owned equivalent of `cgs.gameEffects[MAX_FX]` populated by
    /// `CG_FX` (`codemp/cgame/cg_ents.c:3709-3720`).
    pub fn from_game_state(game_state: &GameState) -> Self {
        let effects = std::array::from_fn(|index| {
            game_state
                .config_string(CS_EFFECTS + index)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase().into_boxed_str())
        });
        Self {
            effects,
            ..Self::empty()
        }
    }

    /// Replace one `CS_EFFECTS` slot while retaining entity latches and deadlines.
    pub fn refresh_config_string(&mut self, index: usize, game_state: &GameState) -> Option<&str> {
        let slot = index.checked_sub(CS_EFFECTS)?;
        let effect = self.effects.get_mut(slot)?;
        *effect = game_state
            .config_string(index)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| name.to_ascii_lowercase().into_boxed_str());
        effect.as_deref()
    }

    /// Evaluate all visible `ET_FX` states at integer `cg.time`.
    ///
    /// The stock client uses process-global `Q_flrand`; this adapter instead
    /// advances an independently seeded RNG per entity. That is the sole
    /// intentional deviation: re-fire jitter differs, while effect identity,
    /// contents, bounds, and the server-authored delay/random interval do not.
    /// Per-entity seeding makes demo replays bit-identical across runs.
    pub fn update(
        &mut self,
        snapshot: &Snapshot,
        presentation_time: i32,
    ) -> LegacyMapEffectMetrics {
        self.requests.clear();
        let mut metrics = LegacyMapEffectMetrics::default();
        for entity in &snapshot.entities {
            if entity.entity_type() != ET_FX {
                continue;
            }
            metrics.decoded_entities += 1;
            let number = usize::from(entity.number());
            let Some(side) = self.entities.get_mut(number) else {
                metrics.missing_effects += 1;
                continue;
            };
            match schedule(
                side,
                entity.model_index2(),
                entity.speed(),
                entity.time(),
                presentation_time,
                entity.number(),
            ) {
                Schedule::Off => metrics.off_states += 1,
                Schedule::Cooldown => metrics.cooldown_states += 1,
                Schedule::Latched => metrics.latched_one_shots += 1,
                Schedule::Play => {
                    let effect_index = match usize::try_from(entity.model_index()) {
                        Ok(index) if index < MAX_FX && self.effects[index].is_some() => index,
                        _ => {
                            metrics.missing_effects += 1;
                            continue;
                        }
                    };
                    let origin = legacy_evaluate_trajectory(
                        entity.trajectory_base(),
                        entity.trajectory_delta(),
                        entity.trajectory_type(),
                        entity.trajectory_time(),
                        entity.trajectory_duration(),
                        presentation_time,
                    );
                    self.requests.push(PendingRequest {
                        entity_number: entity.number(),
                        effect_index,
                        origin,
                        direction: angle_vectors_forward(entity.angles()),
                        portal: entity.is_portal_entity(),
                    });
                    metrics.play_requests += 1;
                }
            }
        }
        metrics
    }

    /// Iterate the current frame's requests without cloning names or growing a
    /// transient collection.
    pub fn requests(&self) -> impl ExactSizeIterator<Item = LegacyMapEffectRequest<'_>> {
        self.requests.iter().map(|request| LegacyMapEffectRequest {
            entity_number: request.entity_number,
            effect_name: self.effects[request.effect_index]
                .as_deref()
                .expect("requests only contain registered effect indices"),
            origin: request.origin,
            direction: request.direction,
            portal: request.portal,
        })
    }

    /// Registered effect names, used to preload EFX graphs and their shaders.
    pub fn effect_names(&self) -> impl Iterator<Item = &str> {
        self.effects.iter().filter_map(Option::as_deref)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Schedule {
    Off,
    Cooldown,
    Latched,
    Play,
}

fn schedule(
    side: &mut EntitySideState,
    state: u8,
    speed: f32,
    random_millis: i32,
    time: i32,
    entity_number: u16,
) -> Schedule {
    // CG_FX checks miscTime first, even before an off/one-shot state.
    if side.misc_time > time {
        return Schedule::Cooldown;
    }
    if state == FX_STATE_OFF {
        return Schedule::Off;
    }
    if state < FX_STATE_ONE_SHOT_LIMIT {
        if side.muzzle_flash_time == state {
            return Schedule::Latched;
        }
        side.muzzle_flash_time = state;
    }
    let random = next_random(side, entity_number);
    // `speed` makes the complete C expression float before assignment back to
    // `int miscTime`; retaining that conversion also preserves large-time
    // float granularity from the stock client.
    side.misc_time = (time as f32 + speed + random * random_millis as f32) as i32;
    Schedule::Play
}

fn next_random(side: &mut EntitySideState, entity_number: u16) -> f32 {
    if side.random_state == 0 {
        side.random_state = u32::from(entity_number)
            .wrapping_add(1)
            .wrapping_mul(0x9e37_79b9);
    }
    let mut value = side.random_state;
    value ^= value << 13;
    value ^= value >> 17;
    value ^= value << 5;
    side.random_state = value;
    value as f32 / u32::MAX as f32
}

/// Forward vector from JKA pitch/yaw degrees (`AngleVectors`,
/// `shared/qcommon/q_math.c:1319-1366`, used at `cg_ents.c:3702-3707`).
fn angle_vectors_forward(angles: [f32; 3]) -> [f32; 3] {
    let pitch = angles[0].to_radians();
    let yaw = angles[1].to_radians();
    let (pitch_sine, pitch_cosine) = pitch.sin_cos();
    let (yaw_sine, yaw_cosine) = yaw.sin_cos();
    let mut forward = [
        pitch_cosine * yaw_cosine,
        pitch_cosine * yaw_sine,
        -pitch_sine,
    ];
    if forward == [0.0; 3] {
        forward[1] = 1.0;
    }
    forward
}
