//! The level's vehicles on this server: vehicle NPCs (`CLASS_VEHICLE`), spawned by the
//! roster as the map's `NPC_Vehicle` spawners and `npc spawn vehicle` make them
//! ([`sjk_game_jka::vehicle_spawn`]), and moved by their own `Pmove` path
//! ([`sjk_game_jka::pmove::vehicle`]). Protocol 26 sees each as the `ET_NPC` of its pool
//! slot, its model configstring the vehicle's name (`$<name>`), which a stock cgame
//! resolves through its own `ext_data/vehicles` files — as a stock server sends it.
