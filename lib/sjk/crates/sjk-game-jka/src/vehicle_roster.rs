//! The roster's vehicles: an `NPC_Vehicle` spawner of the map placed as `SP_NPC_Vehicle`
//! places it ([`crate::vehicle_spawn::place`]) — spawning at once, after its delay, or when
//! used — and `npc spawn vehicle` let through.
//!
//! Every kind of vehicle the game makes is spawned: speeders, animals, walkers and
//! fighters.

use crate::npc_roster::{Fired, NpcFiles, NpcRoster};
use crate::npc_spawn::NpcHost;
use crate::npc_spawners::NpcSpawner;
use crate::vehicle_spawn::Placement;

impl NpcRoster {
    /// `SP_NPC_Vehicle` for a map spawner, at `level_time`.
    pub(crate) fn place_vehicle(
        &mut self,
        mut spawner: NpcSpawner,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) {
        let name = spawner.npc_type.clone().unwrap_or_default();
        let Some(table) = self.vehicle_table.as_mut() else {
            host.print(&format!(
                "NPC_Vehicle {}: the level has no vehicle definitions\n",
                String::from_utf8_lossy(&name)
            ));
            self.vehicles.push(spawner);
            return;
        };
        let placement = crate::vehicle_spawn::place(&spawner, level_time, table, files.parms, host);
        if placement == Placement::Refused {
            return;
        }
        let Some(number) = host.spawn_hidden() else {
            host.print("^1ERROR: no entity left for an NPC spawner\n");
            return;
        };
        match placement {
            Placement::At(at) => spawner.spawn_at = Some(at),
            Placement::WhenUsed => spawner.usable = true,
            Placement::Now | Placement::Refused => {}
        }
        self.spawners.push((number, spawner));
        if placement == Placement::Now {
            let mut fired = Fired::new();
            let at = self.spawners.len() - 1;
            self.spawn(at, level_time, files, host, &mut fired);
            // A spawner spent at once fires its target as the map loads; nothing uses it
            // yet but the roster's own spawners.
            for name in fired {
                let _ = self.use_targets(&name, level_time, files, host);
            }
        }
    }
}
