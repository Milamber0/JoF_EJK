//! The items a level places on this server: spawned as the level starts
//! (`FinishSpawningItem`) and run each frame (`G_RunItem`, `RespawnItem`).

use super::*;

impl NativeGame {
    /// `FinishSpawningItem` for every item the map placed: dropped to the floor (or
    /// left hanging), a trigger in the world from the game's first moment.
    pub(super) fn spawn_items(&mut self) {
        let Some(map) = self.map.take() else { return };
        for placed in &map.items {
            let mut trace =
                |start: [f32; 3], mins: [f32; 3], maxs: [f32; 3], end: [f32; 3], mask: u32| {
                    WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    }
                    .trace(start, mins, maxs, end, mask)
                };
            // What the game type does without (`FinishSpawningItem`): the flags outside the
            // flag games, the duels' health and armour, and the rest.
            if sjk_game_jka::items::removed_by_gametype(
                placed.item,
                self.gametype,
                self.settings.saber_only(),
                self.settings.disabled != 0,
            ) {
                continue;
            }
            let Some(pickup) = sjk_game_jka::items::finish_spawning(placed, &mut trace) else {
                continue;
            };
            if let Some(number) = self.pool.spawn_entity(pickup.state.clone(), 0) {
                self.pool.set_bounds(number, pickup.bounds);
                self.items.push((number, pickup));
            }
        }
        self.map = Some(map);
    }

    /// The items' respawns at `server_time` (`RespawnItem`): a trigger and drawn again,
    /// with `EV_ITEM_RESPAWN` on itself.
    pub(super) fn run_items(&mut self, server_time: i32) {
        // `G_RunItem` for the dropped ones: their flight through the map, their end.
        let previous_time = self.previous_frame_time;
        let flag_game = bridge_ctf::flag_game(self.gametype);
        let mut gone_flags = Vec::new();
        let Self {
            items, pool, map, ..
        } = self;
        items.retain_mut(|(number, pickup)| {
            if pickup.dropped.is_none() {
                return true;
            }
            let run = match map {
                Some(map) => {
                    let world = WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    };
                    sjk_game_jka::dropped_items::run(
                        pickup,
                        server_time,
                        previous_time,
                        &mut |start, mins, maxs, end, mask| {
                            world.trace(start, mins, maxs, end, mask)
                        },
                        &|point| world.point_contents(point),
                    )
                }
                None => sjk_game_jka::dropped_items::run(
                    pickup,
                    server_time,
                    previous_time,
                    &mut |_, _, _, end, _| sjk_game_jka::pmove::MovementTrace::miss(end),
                    &|_| 0,
                ),
            };
            // A dropped flag whose time is up, or that is lost, goes home instead
            // (`Team_DroppedFlagThink`, `Team_FreeEntity`), which frees it.
            let gone = matches!(
                run,
                sjk_game_jka::dropped_items::DroppedRun::Expired
                    | sjk_game_jka::dropped_items::DroppedRun::Lost
            );
            if gone
                && flag_game
                && sjk_game_jka::items::ITEMS[pickup.item].kind == sjk_game_jka::items::Kind::Team
            {
                gone_flags.push((
                    pickup.item,
                    run == sjk_game_jka::dropped_items::DroppedRun::Lost,
                ));
                return true;
            }
            if run != sjk_game_jka::dropped_items::DroppedRun::Stays {
                pool.free(*number, server_time);
                return false;
            }
            pool.set_state(*number, &pickup.state);
            true
        });
        for (item, lost) in gone_flags {
            self.flag_expired(item, lost, server_time);
        }
        for index in 0..self.items.len() {
            let (_, pickup) = &self.items[index];
            if pickup.respawn_at == 0 || pickup.respawn_at > server_time {
                continue;
            }
            bridge_ctf::respawn_item(&mut self.items, &mut self.pool, index, server_time);
        }
    }
}
