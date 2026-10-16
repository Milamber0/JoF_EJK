//! The items as the NPCs read and take them on this server
//! ([`sjk_game_jka::npc_weapon_pickup`]): the game's placed and dropped items (the pickups
//! the players take too), offered in the order they were spawned; a taken one gone as a
//! player's pickup leaves it (a dropped one freed, a placed one back in its time).
//!
//! The items are offered in the order the game keeps them, not by entity number: two items
//! at exactly the same distance from an NPC's search would be taken in that order. Item
//! targets (`G_UseTargets`) are not fired: this server's items carry none.

use super::host::ServerHost;
use sjk_game_jka::npc_weapon_pickup::NpcItem;

/// `s.eFlags`.
const ES_EFLAGS: usize = 19;

impl ServerHost<'_> {
    /// [`sjk_game_jka::npc_spawn::NpcHost::npc_item`]: the `index`-th item the game keeps.
    pub(super) fn npc_item_at(&self, index: usize) -> Option<NpcItem> {
        let (id, pickup) = self.pickups.get(index)?;
        Some(NpcItem {
            number: id.legacy_number(),
            item: pickup.item,
            origin: pickup.origin,
            position: sjk_game_jka::npc_weapon_pickup::item_position(
                &pickup.state,
                self.level_time,
            ),
            mins: pickup.bounds.0,
            maxs: pickup.bounds.1,
            contents: pickup.contents,
            entity_flags: pickup.state.raw_field(ES_EFLAGS).unwrap_or(0),
            dropped: pickup.dropped.is_some(),
            count: pickup.dropped.map_or(0, |dropped| dropped.count),
            // A dropped item's `activator` and `s.time` are never set on this server (nor
            // by `LaunchItem`).
            belongs_to_client_zero: false,
            time: 0,
            allow_npc: pickup.allow_npc,
        })
    }

    /// [`sjk_game_jka::npc_spawn::NpcHost::npc_item_refuses`]: a tossed weapon's thrower.
    pub(super) fn npc_item_refused(&mut self, number: u16, toucher: u16, level_time: i32) -> bool {
        let Some((_, pickup)) = self
            .pickups
            .iter_mut()
            .find(|(id, _)| id.legacy_number() == number)
        else {
            return false;
        };
        sjk_game_jka::dropped_items::refuses(pickup, toucher, level_time)
    }

    /// [`sjk_game_jka::npc_spawn::NpcHost::npc_took_item`]: `Touch_Item`'s end — a dropped
    /// item freed at the next frame, a placed one out of the world until it comes again
    /// (`random` wandering its time, drawn only where the map gave one).
    pub(super) fn npc_item_taken(&mut self, number: u16, respawn: i32, level_time: i32) {
        let Some(at) = self
            .pickups
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
        else {
            return;
        };
        let random = self.pickups[at].1.random;
        let spread = if random != 0.0 {
            self.deaths.rng.flrand(-1.0, 1.0)
        } else {
            0.0
        };
        let (id, pickup) = &mut self.pickups[at];
        if pickup.dropped.is_some() {
            sjk_game_jka::dropped_items::taken(pickup, respawn, level_time);
        } else {
            let _ = sjk_game_jka::items::taken(pickup, respawn, level_time, spread);
        }
        let mut state = pickup.state.clone();
        let _ = state.set_number(number);
        if let Some(slot) = self.pool.state_mut(*id) {
            *slot = state;
        }
    }

    /// [`sjk_game_jka::npc_spawn::NpcHost::client_buttons`]: the buttons of the command player
    /// `number` last thought with.
    pub(super) fn player_buttons(&self, number: u16) -> u16 {
        self.peer(number)
            .map_or(0, |peer| peer.last_command.buttons)
    }
}
