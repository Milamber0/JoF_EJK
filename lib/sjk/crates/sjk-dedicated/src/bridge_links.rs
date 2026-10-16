//! The game's entities linked for the snapshots (`SV_LinkEntity`, `sv_world.cpp:288-340`):
//! each at the place its trajectory gives at the frame (`r.currentOrigin`, as the game's
//! own frame leaves it), its box grown by a unit, the clusters and areas that box touches.
//! Worked out once for every client, and again only when the pool or the frame moves on.
//!
//! Also where the pool's native identities meet protocol 26: an entity the legacy
//! adapter cannot number ([`sjk_game_jka::entity_pool::PoolEntity::legacy_number`]) is
//! withheld from every legacy client's snapshot, counted, and reported the first time.

use super::*;
use crate::visibility::ClusterLink;

/// The links of the pool's entities, in the order `EntityPool::linked` walks them.
#[derive(Debug, Default)]
pub(super) struct PoolLinks {
    /// The frame and pool generation they were worked out for.
    key: Option<(i32, u64)>,
    links: Vec<ClusterLink>,
    /// How many times an entity beyond protocol 26's numbering was withheld from a
    /// legacy client's snapshot.
    withheld: u64,
    /// Which entities the snapshot being built already carries, kept to be reused: a
    /// portal's view adds only what is not in yet ("don't double add an entity through
    /// portals").
    sent: Vec<bool>,
}

impl PoolLinks {
    /// The links for this frame and pool, worked out afresh if either moved on.
    pub(super) fn refresh(
        &mut self,
        map: &crate::map::LoadedMap,
        pool: &EntityPool,
        level_time: i32,
    ) -> &[ClusterLink] {
        let key = (level_time, pool.generation());
        if self.key != Some(key) {
            self.links.clear();
            for entity in pool.linked() {
                let origin = current_origin(entity.state, level_time);
                let low = std::array::from_fn(|axis| origin[axis] + entity.bounds.0[axis] - 1.0);
                let high = std::array::from_fn(|axis| origin[axis] + entity.bounds.1[axis] + 1.0);
                self.links.push(ClusterLink::new(&map.bsp, low, high));
            }
            self.key = Some(key);
        }
        &self.links
    }
}

impl NativeGame {
    /// The game's own entities a client is sent, in number order, as
    /// `SV_AddEntitiesVisibleFromPoint` filters them: never an `EF_PERMANENT` one, not
    /// the one client a temp entity is not for, always a broadcast one, and otherwise only
    /// what no closed door cuts off and the eye's cluster can see.
    pub(super) fn pool_entities_for(
        &self,
        client: usize,
        eye: &crate::visibility::Eye<'_>,
        mut send: impl FnMut(&EntityState),
    ) {
        let mut kept = self.pool_links.borrow_mut();
        if let Some(map) = self.map.as_ref() {
            kept.refresh(map, &self.pool, self.last_frame_time);
        }
        let PoolLinks { links, sent, .. } = &mut *kept;
        let links = self.map.as_ref().map(|_| &links[..]);
        sent.clear();
        let mut withheld = 0;
        let mut portals = false;
        for (index, entity) in self.pool.linked().enumerate() {
            sent.push(false);
            if entity.permanent || entity.not_for == Some(client as u16) {
                continue;
            }
            // Beyond what a legacy client can represent: never aliased onto a number it
            // knows, never sent.
            if entity.legacy_number().is_none() {
                withheld += 1;
                continue;
            }
            if entity.broadcast || links.is_none_or(|links| links[index].visible_from(eye)) {
                sent[index] = true;
                portals |= entity.state.raw_field(ES_ENTITY_TYPE)
                    == Some(sjk_game_jka::map_scenery::ET_PORTAL);
            }
        }
        // A portal surface in view (`SVF_PORTAL`) adds what its camera sees
        // (`SV_AddEntitiesVisibleFromPoint(ent->s.origin2, ..., qtrue)`); the whole is then
        // sent in number order, as the reference sorts it after its portals.
        if portals && let (Some(map), Some(links)) = (self.map.as_ref(), links) {
            let cameras: Vec<[f32; 3]> = self
                .pool
                .linked()
                .enumerate()
                .filter(|(index, entity)| {
                    sent[*index]
                        && entity.state.raw_field(ES_ENTITY_TYPE)
                            == Some(sjk_game_jka::map_scenery::ET_PORTAL)
                })
                .map(|(_, entity)| {
                    ES_ORIGIN2
                        .map(|field| f32::from_bits(entity.state.raw_field(field).unwrap_or(0)))
                })
                .collect();
            for camera in cameras {
                let view = crate::visibility::Eye::new(&map.bsp, &map.areas, camera);
                for (index, entity) in self.pool.linked().enumerate() {
                    if sent[index]
                        || entity.permanent
                        || entity.not_for == Some(client as u16)
                        || entity.legacy_number().is_none()
                    {
                        continue;
                    }
                    if links[index].visible_from(&view) {
                        sent[index] = true;
                    }
                }
            }
        }
        for (index, entity) in self.pool.linked().enumerate() {
            if sent[index] {
                send(entity.state);
            }
        }
        if withheld > 0 {
            if kept.withheld == 0 {
                eprintln!(
                    "legacy snapshots: the game has entities beyond protocol 26's {} and withholds them from legacy clients",
                    sjk_protocol::LEGACY_GAME_ENTITIES
                );
            }
            kept.withheld += withheld;
        }
    }
}

/// `s.origin2`: where a portal surface's camera is.
const ES_ORIGIN2: [usize; 3] = [56, 60, 53];

/// `r.currentOrigin` of an entity from its wire trajectory at `level_time`
/// (`BG_EvaluateTrajectory`).
fn current_origin(state: &EntityState, level_time: i32) -> [f32; 3] {
    let read = |index: usize| state.raw_field(index).unwrap_or(0);
    let float = |index: usize| f32::from_bits(read(index));
    sjk_game_jka::trajectory::legacy_evaluate_trajectory(
        ES_POS_BASE.map(float),
        ES_POS_DELTA.map(float),
        read(ES_POS_TYPE) as u8,
        read(ES_POS_TIME) as i32,
        read(ES_POS_DURATION) as i32,
        level_time,
    )
}
