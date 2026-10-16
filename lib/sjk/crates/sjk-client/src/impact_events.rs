//! Byte-exact legacy impact-event extraction for client presentation.
//!
//! Event numbers and event-toggle semantics mirror `codemp/game/bg_public.h`
//! lines 783-790, 847-849, and 918-920. Missile normals are decoded with the
//! fixed `bytedirs` table and `ByteToDir` bounds behavior from
//! `shared/qcommon/q_math.c` lines 37-119 and 147-153. This module is a JKA
//! compatibility adapter; renderer/runtime crates remain format-agnostic.

use sjk_protocol::Snapshot;

const ET_EVENTS: u8 = 18; // codemp/game/bg_public.h:1265
const MAX_GENTITIES: usize = 1_024; // codemp/game/bg_public.h:1196
const EVENT_VALUE_MASK: u16 = 0xff; // codemp/game/bg_public.h:783-790
const EF_ALT_FIRING: u32 = 1 << 10; // codemp/game/bg_public.h:649

pub const EV_SABER_HIT: u16 = 30; // codemp/game/bg_public.h:847
pub const EV_SABER_BLOCK: u16 = 31; // codemp/game/bg_public.h:848
pub const EV_SABER_CLASHFLARE: u16 = 32; // codemp/game/bg_public.h:849
pub const EV_DISRUPTOR_MAIN_SHOT: u16 = 35; // codemp/game/bg_public.h:852
pub const EV_DISRUPTOR_SNIPER_SHOT: u16 = 36; // codemp/game/bg_public.h:853
pub const EV_DISRUPTOR_SNIPER_MISS: u16 = 37; // codemp/game/bg_public.h:854
pub const EV_DISRUPTOR_HIT: u16 = 38; // codemp/game/bg_public.h:855
// Ordinals count from EV_NONE = 0 at bg_public.h:805; the 16 EV_USE_ITEM*
// entries (45..60) are one enumerator each, so EV_PAIN is 89 (see
// sound_events.rs) and the missile events sit at 85..87.
pub const EV_CONC_ALT_IMPACT: u16 = 84; // codemp/game/bg_public.h:917
pub const EV_MISSILE_HIT: u16 = 85; // codemp/game/bg_public.h:918
pub const EV_MISSILE_MISS: u16 = 86; // codemp/game/bg_public.h:919
pub const EV_MISSILE_MISS_METAL: u16 = 87; // codemp/game/bg_public.h:920

/// Visual impact class selected by `CG_EntityEvent`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyImpactKind {
    SaberHit,
    SaberBlock,
    SaberClashFlare,
    /// Trace-weapon beam from `start` (`origin2`) to `origin` (`cg_event.c:2442-2470`).
    DisruptorMainShot,
    /// Sniper beam; `full_charge` carries `shouldtarget` (`cg_event.c:2472-2500`).
    DisruptorSniperShot,
    /// Sniper wall miss: `weapon != 0` is a main-fire wall hit (`cg_event.c:2502-2515`).
    DisruptorSniperMiss,
    /// Sniper hit: `weapon != 0` is a player hit (`cg_event.c:2517-2530`).
    DisruptorHit,
    MissileHitPlayer,
    MissileHitWall,
    MissileHitMetal,
    /// The concussion rifle's alt beam (`cg_event.c:2860-2881`): from `start`
    /// (`origin2`) along `shot` (`angles`, its length the shot's), with rings along
    /// it facing `ring_direction` (`angles2`) and a wall hit at `origin`.
    ConcussionAltShot,
}

/// One newly decoded impact event, in the fields consumed by cgame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyImpactEvent {
    pub entity_number: u16,
    pub event: u16,
    pub kind: LegacyImpactKind,
    pub origin: [f32; 3],
    /// `origin2`: the muzzle a trace-weapon beam starts from.
    pub start: [f32; 3],
    pub direction: [f32; 3],
    pub event_parameter: u8,
    pub weapon: u8,
    pub alternate: bool,
    pub charge: u8,
    /// `shouldtarget`: a fully charged sniper shot.
    pub full_charge: bool,
    /// `angles`, the concussion alt shot's vector.
    pub shot: [f32; 3],
    /// `angles2`, the facing of the concussion alt shot's rings.
    pub ring_direction: [f32; 3],
}

/// Allocation-free event de-duplicator equivalent to centity `previousEvent`.
///
/// The caller provides a reused output vector with capacity for the protocol's
/// 1024-entity ceiling. Disappearing event entities clear their signature so a
/// later reuse of the same slot is observed.
pub struct LegacyImpactTracker {
    signatures: [u16; MAX_GENTITIES],
    active: [bool; MAX_GENTITIES],
}

impl LegacyImpactTracker {
    pub fn new() -> Self {
        Self {
            signatures: [0; MAX_GENTITIES],
            active: [false; MAX_GENTITIES],
        }
    }

    pub fn observe(&mut self, snapshot: &Snapshot, output: &mut Vec<LegacyImpactEvent>) {
        output.clear();
        self.active.fill(false);
        for entity in &snapshot.entities {
            let number = usize::from(entity.number());
            if number >= MAX_GENTITIES {
                continue;
            }
            self.active[number] = true;
            let event_entity = entity.entity_type() >= ET_EVENTS;
            let signature = if event_entity {
                u16::from(entity.entity_type())
            } else {
                entity.event()
            };
            let event = if event_entity {
                u16::from(entity.entity_type() - ET_EVENTS)
            } else {
                signature & EVENT_VALUE_MASK
            };
            if self.signatures[number] == signature {
                continue;
            }
            self.signatures[number] = signature;
            if event == 0 {
                continue;
            }
            let Some(kind) = kind(event) else {
                continue;
            };
            // CG_EntityEvent receives cent->lerpOrigin as `position` for
            // missile events. G_TempEntity writes pos.trBase but deliberately
            // does not write s.origin (codemp/game/g_utils.c:1058-1077).
            // Saber event producers explicitly copy their point into s.origin
            // and cg_event.c reads that field directly.
            let origin = if event_entity
                && matches!(
                    kind,
                    LegacyImpactKind::SaberHit
                        | LegacyImpactKind::SaberBlock
                        | LegacyImpactKind::SaberClashFlare
                ) {
                entity.event_origin()
            } else {
                entity.trajectory_base()
            };
            let direction = if matches!(
                kind,
                LegacyImpactKind::MissileHitPlayer
                    | LegacyImpactKind::MissileHitWall
                    | LegacyImpactKind::MissileHitMetal
                    | LegacyImpactKind::DisruptorSniperMiss
                    | LegacyImpactKind::DisruptorHit
                    | LegacyImpactKind::ConcussionAltShot
            ) {
                legacy_byte_to_direction(entity.event_parameter())
            } else {
                let value = entity.angles();
                if value == [0.0; 3] {
                    [0.0, 1.0, 0.0]
                } else {
                    value
                }
            };
            output.push(LegacyImpactEvent {
                entity_number: entity.number(),
                event,
                kind,
                origin,
                start: entity.origin2(),
                direction,
                event_parameter: entity.event_parameter(),
                weapon: entity.weapon(),
                alternate: entity.e_flags() & EF_ALT_FIRING != 0,
                charge: entity.generic1(),
                full_charge: entity.should_target(),
                shot: entity.angles(),
                ring_direction: entity.angles2(),
            });
        }
        for (index, active) in self.active.iter().copied().enumerate() {
            if !active {
                self.signatures[index] = 0;
            }
        }
    }
}

impl Default for LegacyImpactTracker {
    fn default() -> Self {
        Self::new()
    }
}

pub const fn kind(event: u16) -> Option<LegacyImpactKind> {
    match event & EVENT_VALUE_MASK {
        EV_SABER_HIT => Some(LegacyImpactKind::SaberHit),
        EV_SABER_BLOCK => Some(LegacyImpactKind::SaberBlock),
        EV_SABER_CLASHFLARE => Some(LegacyImpactKind::SaberClashFlare),
        EV_DISRUPTOR_MAIN_SHOT => Some(LegacyImpactKind::DisruptorMainShot),
        EV_DISRUPTOR_SNIPER_SHOT => Some(LegacyImpactKind::DisruptorSniperShot),
        EV_DISRUPTOR_SNIPER_MISS => Some(LegacyImpactKind::DisruptorSniperMiss),
        EV_DISRUPTOR_HIT => Some(LegacyImpactKind::DisruptorHit),
        EV_CONC_ALT_IMPACT => Some(LegacyImpactKind::ConcussionAltShot),
        EV_MISSILE_HIT => Some(LegacyImpactKind::MissileHitPlayer),
        EV_MISSILE_MISS => Some(LegacyImpactKind::MissileHitWall),
        EV_MISSILE_MISS_METAL => Some(LegacyImpactKind::MissileHitMetal),
        _ => None,
    }
}

/// Whether the stock event branch latches `cg_saberFlashTime/Pos`.
///
/// Saber-on-saber blocks require nonzero `eventParm`
/// (`codemp/cgame/cg_event.c:2287-2356`); `EV_SABER_CLASHFLARE` always
/// latches (`cg_event.c:2372-2377`). Custom saber `SFL2_NO_CLASH_FLARE`
/// policy is resolved by cgame before this write and remains adapter metadata
/// work outside this event-only predicate.
pub const fn legacy_impact_latches_saber_flare(event: LegacyImpactEvent) -> bool {
    matches!(event.kind, LegacyImpactKind::SaberClashFlare)
        || matches!(event.kind, LegacyImpactKind::SaberBlock) && event.event_parameter != 0
}

pub use sjk_protocol::{legacy_byte_to_direction, legacy_direction_to_byte};
