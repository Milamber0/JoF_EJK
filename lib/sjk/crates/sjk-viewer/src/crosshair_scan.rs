//! Allocation-free player targeting for the crosshair-name HUD projection.

use glam::Vec3;
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_client::{CrosshairCandidate, CrosshairName};
use sjk_protocol::{GameState, Snapshot};
#[path = "crosshair_classification.rs"]
pub(crate) mod classification;
#[path = "crosshair_dynamic.rs"]
pub(crate) mod dynamic;

const CONTENTS_SOLID: u32 = 1;
const CONTENTS_FOG: u32 = 8;
const EF_DEAD: u32 = 1 << 1;
const PW_CLOAKED: u32 = 1 << 11;

/// Retained target/time corresponding to `cg.crosshairClientNum/Time`.
#[derive(Default)]
pub(crate) struct State {
    client: Option<u16>,
    acquired_at: i32,
    /// Time of the last [`State::scan`].
    scanned_at: i32,
    /// Current trace classification, independent of the retained player name.
    pub(crate) color: Option<[f32; 4]>,
    /// Endpoint of the same trace used to identify the target.
    pub(crate) endpoint: Vec3,
    /// Renderer map distance, resolved at world installation rather than during drawing.
    pub(crate) distance_cull: Option<f32>,
}

/// Inputs that suppress the otherwise retained one-second name fade.
pub(crate) struct Suppression {
    pub(crate) scoreboard: bool,
    pub(crate) intermission: bool,
}

impl State {
    /// Retain the map's distanceCull (rd-vanilla/tr_bsp.cpp:1896,1956-1960).
    pub(crate) fn new(bsp: &Bsp) -> Self {
        let distance = sjk_entity::parse_entity_lump(bsp.entities())
            .ok()
            .and_then(|entities| {
                entities
                    .first()
                    .and_then(|world| world.get("distanceCull"))
                    .and_then(|value| value.parse::<f32>().ok())
            })
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(6000.0);
        Self {
            distance_cull: Some(distance),
            ..Self::default()
        }
    }

    /// The player the crosshair is on this frame, with no retention: the one
    /// the last [`State::scan`] at `now` found.
    pub(crate) fn hit_now(&self, now: i32) -> Option<u16> {
        self.client.filter(|_| self.acquired_at == now)
    }

    /// Where the last trace ended, if a scan ran within a second of `now`
    /// (`cg_crosshairPos`, which JoF EternalJK's `teleGun` reads).
    pub(crate) fn endpoint_near(&self, now: i32) -> Option<Vec3> {
        (self.scanned_at != 0 && now.abs_diff(self.scanned_at) <= 1_000).then_some(self.endpoint)
    }

    /// Stock `CG_CrosshairPlayer` retains a target for at most one second.
    pub(crate) fn chat_client(&self, now: i32) -> Option<u16> {
        self.client
            .filter(|_| now.saturating_sub(self.acquired_at) <= 1_000)
    }

    /// Scan BSP occlusion, then current client entities up to that hit.
    ///
    /// This is the fixed-scratch equivalent of `CG_ScanForCrosshairEntity`'s
    /// `CG_Trace` from `codemp/cgame/cg_draw.c:6080-6295`.
    pub(crate) fn scan(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        ray: dynamic::Ray,
        now: i32,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
        suppression: Suppression,
    ) -> Option<CrosshairName> {
        self.scanned_at = now;
        let start = ray.start;
        let direction = ray.forward.normalize_or_zero();
        let end = start + direction * ray.distance;
        let world = bsp.trace_box_with(
            scratch,
            start.to_array(),
            end.to_array(),
            Aabb::new([0.0; 3], [0.0; 3]).expect("point bounds are valid"),
            CONTENTS_SOLID,
        );
        self.color = None;
        let mut nearest = ray.distance * world.fraction;
        let local = snapshot.player.client_num();
        let mut hit = None;
        // CG_Trace clips against cg_solidEntities, which holds the permanent
        // baselines too (`CG_BuildSolidList`): a `misc_bsp` wall hides a player.
        for entity in sjk_client::legacy_scene_entities(game, snapshot) {
            if entity.number() == local {
                continue;
            }
            if let Some(collider) = crate::local_prediction::movers::Collider::from_entity(
                entity,
                snapshot.server_time,
                now,
                false,
            ) {
                let trace = crate::movement_collision::target_trace(
                    bsp,
                    &collider,
                    start.to_array(),
                    end.to_array(),
                );
                let distance = trace.fraction * ray.distance;
                if trace.all_solid || distance < nearest {
                    nearest = distance;
                    hit = Some(entity);
                    if trace.all_solid {
                        break;
                    }
                }
            }
        }
        self.endpoint = start + direction * nearest;
        if let Some(entity) = hit {
            if !entity.client_bitflag(local) {
                self.color = classification::classify(entity, snapshot, game);
            }
            let point = start + direction * nearest;
            if bsp.point_contents(point.to_array(), CONTENTS_FOG) == 0
                && !entity.client_bitflag(local)
                && entity.number() < 32
            {
                self.client = Some(entity.number());
                self.acquired_at = now;
            }
        }
        let client = self.client?;
        let target = snapshot
            .entities
            .iter()
            .find(|entity| entity.number() == client)?;
        sjk_client::crosshair_name(CrosshairCandidate {
            client_num: client,
            local_client: local,
            local_team: snapshot.player.team(),
            target_team: client_team(game, client),
            now,
            acquired_at: self.acquired_at,
            spectator: snapshot.player.is_spectator(),
            intermission: suppression.intermission,
            scoreboard: suppression.scoreboard,
            in_fog: false,
            cloaked: target.powerups() & PW_CLOAKED != 0,
            mind_tricked: target.client_bitflag(local),
            dead: target.e_flags() & EF_DEAD != 0,
        })
    }
}

fn client_team(game: &GameState, client: u16) -> u8 {
    let Some(info) = game.config_string(1_131 + usize::from(client)) else {
        return 3;
    };
    sjk_client::LegacyClientInfo::new(info)
        .integer("t")
        .unwrap_or(3) as u8
}
