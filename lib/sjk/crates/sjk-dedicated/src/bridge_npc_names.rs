//! What this server lifts of the stock NPC and entity limits outside `g_stockRules 1`
//! (`bridge_stock_rules`), with the rules in `sjk_game_jka::npc_names`:
//!
//! - `npc spawn tauntaun` / `swoop` / any vehicle's name spawns the vehicle, where the
//!   reference wants `npc spawn vehicle <name>`;
//! - friendly names find their NPC or vehicle (`mine_monster` is `Minemonster`, `xwing`
//!   is `X-Wing`); a name several answer is refused with them;
//! - the NPC definitions are read however large, and an MD3 NPC (the mouse droid, the
//!   remote, the seeker, which MP refuses) spawns with its actual rigid model through stock-compatible presentation;
//! - the level's entities are not held to protocol 26's 1024 (what a legacy client cannot
//!   be sent is withheld from it and counted, `bridge_links`), nor its vehicle table to 16.

use super::*;
use sjk_game_jka::npc_names::{Resolved, npc_names, resolve};
use sjk_game_jka::npc_parms::NpcParms;
use sjk_game_jka::vehicle_parms::VehicleCapacity;

/// The level's entities outside the stock rules: sixteen times the stock budget.
const NATIVE_ENTITY_BUDGET: usize = 16 * sjk_game_jka::entity_pool::JKA_PROFILE_BUDGET;
/// The vehicle table outside the stock rules.
const NATIVE_VEHICLES: VehicleCapacity = VehicleCapacity {
    vehicles: 256,
    weapons: 256,
};

impl NativeGame {
    /// The NPC definitions the level spawns from: the stock ones, or the native ones.
    pub(super) fn level_npc_parms(&self, map: &crate::map::LoadedMap) -> std::sync::Arc<NpcParms> {
        if self.stock_rules() {
            map.npc_parms.clone()
        } else {
            map.npc_parms_native.clone()
        }
    }

    /// `BG_VehicleLoadParms`' table size.
    pub(super) fn vehicle_capacity(&self) -> VehicleCapacity {
        if self.stock_rules() {
            VehicleCapacity::REFERENCE
        } else {
            NATIVE_VEHICLES
        }
    }

    /// How many entities the level may open (`G_Spawn`'s "no free entities").
    pub(in crate::bridge) fn entity_budget(&self) -> usize {
        if self.stock_rules() {
            sjk_game_jka::entity_pool::JKA_PROFILE_BUDGET
        } else {
            NATIVE_ENTITY_BUDGET
        }
    }

    /// The type `npc spawn [vehicle] <name>` spawns, and whether as a vehicle: the words
    /// as they are under the stock rules; otherwise the name resolved. `None` when it is
    /// refused here (said to `client`).
    pub(super) fn npc_spawn_name(
        &mut self,
        requested: &[u8],
        vehicle: bool,
        client: usize,
    ) -> Option<(Vec<u8>, bool)> {
        if vehicle || requested.is_empty() || self.stock_rules() {
            return Some((requested.to_vec(), vehicle));
        }
        let map = self.map.as_ref()?;
        let npcs = npc_names(map.npc_parms_native.text());
        let vehicles = map.vehicle_files.vehicle_names();
        match resolve(requested, &npcs, &vehicles) {
            Resolved::Npc(name) => Some((name, false)),
            Resolved::Vehicle(name) => Some((name, true)),
            Resolved::Unknown => Some((requested.to_vec(), false)),
            Resolved::Ambiguous(names) => {
                let names: Vec<String> = names
                    .iter()
                    .map(|name| String::from_utf8_lossy(name).into_owned())
                    .collect();
                let text = format!(
                    "print \"npc spawn {}: which one? {}\n\"",
                    String::from_utf8_lossy(requested),
                    names.join(", ")
                );
                self.told.push(Told::One(client, text.into_bytes()));
                None
            }
        }
    }
}
