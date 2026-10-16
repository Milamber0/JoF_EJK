//! In-match NPC bodies and unarmed player display copies — the viewer half of `CG_G2AnimEntModelLoad`
//! (`codemp/cgame/cg_players.c:7046-7308`, run from `CG_G2Animated` when an
//! `ET_NPC` entity has no Ghoul2 instance yet or its `modelindex` changed,
//! `:7502-7589`).
//!
//! The client adapter resolves an NPC's appearance from `CS_MODELS`
//! (`sjk_client::legacy_npc_appearance`) and stores it on the scene entity;
//! this module notices an actor whose appearance has no mesh keyed by its
//! entity id — a fresh spawn, or an entity number reused for a different
//! NPC — and builds one through the same path the clientinfo refresh uses
//! for a player who changed model. Saber hilts follow `npcSaber1/2`
//! (`:7184-7203`), so an existing mesh only gets its hilt names refreshed.
//!
//! Bounded per frame: one entity scan with no allocation, and at most one
//! mesh build, so a wave of spawns costs one hitch per NPC instead of one
//! per wave.

use super::*;
use sjk_client::legacy_npc_saber_names_borrowed;

/// What one frame's scan decided to do.
enum Work {
    Build(EntityId, Appearance, [Option<String>; 2]),
    Rename(usize, [Option<String>; 2]),
}

/// `true` when the hilt catalog names on a mesh differ from `wanted`
/// (catalog names are lower-cased; `.sab` lookup is case-insensitive).
fn names_differ(current: &[Option<String>; 2], wanted: [Option<&str>; 2]) -> bool {
    current
        .iter()
        .zip(wanted)
        .any(|(current, wanted)| match (current, wanted) {
            (Some(current), Some(wanted)) => !current.eq_ignore_ascii_case(wanted),
            (None, None) => false,
            _ => true,
        })
}

fn owned_lowercase(names: [Option<&str>; 2]) -> [Option<String>; 2] {
    names.map(|name| name.map(str::to_ascii_lowercase))
}

impl GpuState {
    /// Give every NPC actor in the world a mesh of its own appearance.
    pub(crate) fn refresh_npc_actors(&mut self) {
        let Some(work) = self.scan_npc_actors() else {
            return;
        };
        match work {
            Work::Rename(index, names) => self.actor_meshes[index].saber_names = names,
            Work::Build(entity_id, appearance, names) => {
                if let Err(error) = self.build_npc_actor(entity_id, &appearance, names) {
                    self.clientinfo_watch.failed.insert(appearance.clone());
                    log::progress(format_args!(
                        "npc {}: could not load {}/{}: {error}",
                        entity_id.get() - 1,
                        appearance.model,
                        appearance.variant
                    ));
                }
            }
        }
    }

    /// The first NPC actor whose mesh is missing, stale, or misnamed.
    fn scan_npc_actors(&self) -> Option<Work> {
        let (world, game_state, snapshot) = match (&self.live_session, &self.demo_session) {
            (Some(session), _) => (
                &self.live_world,
                session.game_state(),
                session.latest_snapshot(),
            ),
            (None, Some(demo)) => (demo.world(), demo.game_state(), demo.latest_snapshot()),
            (None, None) => return None,
        };
        for entity in world
            .entities()
            .filter(|entity| entity.kind == EntityKind::Actor)
        {
            let Some(appearance) = entity.appearance() else {
                continue;
            };
            let Ok(number) = u16::try_from(entity.id.get().saturating_sub(1)) else {
                continue;
            };
            // Snapshot entities are in wire-number order. Also refresh unarmed
            // replicated player display copies, whose clientNum names the
            // real player but whose entity number is outside the player slots.
            let Ok(at) = snapshot
                .entities
                .binary_search_by_key(&number, sjk_protocol::EntityState::number)
            else {
                continue;
            };
            let state = &snapshot.entities[at];
            if !refreshable(number, state.entity_type(), state.weapon()) {
                continue;
            }
            let wanted = if state.entity_type() == 13 {
                legacy_npc_saber_names_borrowed(game_state, state)
            } else {
                [None, None]
            };
            let index = self
                .actor_meshes
                .iter()
                .position(|mesh| !mesh.corpse_pool && mesh.entity_id == Some(entity.id));
            match index {
                Some(index) if self.actor_meshes[index].appearance == *appearance => {
                    if names_differ(&self.actor_meshes[index].saber_names, wanted) {
                        return Some(Work::Rename(index, owned_lowercase(wanted)));
                    }
                }
                _ if self.clientinfo_watch.failed.contains(appearance) => {}
                _ => {
                    return Some(Work::Build(
                        entity.id,
                        appearance.clone(),
                        owned_lowercase(wanted),
                    ));
                }
            }
        }
        None
    }

    fn build_npc_actor(
        &mut self,
        entity_id: EntityId,
        appearance: &Appearance,
        names: [Option<String>; 2],
    ) -> Result<(), Box<dyn Error>> {
        for name in names.iter().flatten() {
            self.load_hilt(name)?;
        }
        let mesh = self.build_live_actor(appearance, entity_id, names)?;
        let index = self
            .actor_meshes
            .iter()
            .position(|mesh| !mesh.corpse_pool && mesh.entity_id == Some(entity_id));
        match index {
            Some(index) => self.actor_meshes[index] = mesh,
            None => {
                self.actor_meshes.push(mesh);
                self.actor_groups.push(Vec::with_capacity(4));
            }
        }
        log::progress(format_args!(
            "npc {} wears {}/{}",
            entity_id.get() - 1,
            appearance.model,
            appearance.variant
        ));
        Ok(())
    }
}

/// Ordinary player and corpse refresh keep their own lifetime rules. A static,
/// unarmed display copy needs independent pose storage and live model changes.
fn refreshable(number: u16, kind: u8, weapon: u8) -> bool {
    kind == 13 || (kind == 1 && number >= 32 && weapon == 0)
}
