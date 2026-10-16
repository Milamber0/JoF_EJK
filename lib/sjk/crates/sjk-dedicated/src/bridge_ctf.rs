//! Capture the flag on the server: its [`FlagWorld`] over the peers, the map's items and
//! the pool, and the flag's parts of an item's touch, a blow, a death, a dropped item's
//! run — the rules themselves are [`sjk_game_jka::ctf`]'s.

use super::{ES_EVENT, ES_EVENT_PARM, NativeGame, Peer};
use crate::visibility::Eye;
use sjk_game_jka::ctf::{self, FlagPlayer, FlagTouch, FlagWorld, Flags};
use sjk_game_jka::entity_id::EntityId;
use sjk_game_jka::entity_pool::EntityPool;
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::items::{ITEMS, Kind, Pickup};
use sjk_server::ServerWorld;

/// `EV_GLOBAL_ITEM_PICKUP`: a team item taken, heard by everyone.
const EV_GLOBAL_ITEM_PICKUP: u32 = 23;
/// `s.pos.trBase`.
const ES_POS_BASE: [usize; 3] = [2, 1, 4];

/// Whether the game is played for flags.
pub(super) fn flag_game(gametype: i32) -> bool {
    matches!(gametype, ctf::GT_CTF | ctf::GT_CTY)
}

/// An item's `s.pos.trBase`.
fn base_of(pickup: &Pickup) -> [f32; 3] {
    ES_POS_BASE.map(|index| f32::from_bits(pickup.state.raw_field(index).unwrap_or(0)))
}

/// The flag items of `team`, in entity number order: `(index, number, dropped)`.
fn team_flags(
    items: &[(EntityId, Pickup)],
    team: i32,
) -> impl Iterator<Item = (usize, EntityId, bool)> + '_ {
    // Items are kept in the order they were spawned, which is their number order.
    items
        .iter()
        .enumerate()
        .filter(move |(_, (_, pickup))| {
            ITEMS[pickup.item].kind == Kind::Team
                && ctf::flag_team(ITEMS[pickup.item].tag) == Some(team)
        })
        .map(|(index, (number, pickup))| (index, *number, pickup.dropped.is_some()))
}

/// `RespawnItem` for item `index`: back, drawn, with `EV_ITEM_RESPAWN` on itself — its
/// sequence stepped from the event the entity carries now.
pub(super) fn respawn_item(
    items: &mut [(EntityId, Pickup)],
    pool: &mut EntityPool,
    index: usize,
    level_time: i32,
) {
    let (number, pickup) = &mut items[index];
    let number = *number;
    let event = sjk_game_jka::items::respawn(pickup, level_time);
    let current = pool
        .state(number)
        .map_or(0, |current| current.raw_field(ES_EVENT).unwrap_or(0));
    let mut state = pickup.state.clone();
    let bits = (current & 0x300).wrapping_add(0x100) & 0x300;
    state.set_raw_field(ES_EVENT, event.event | bits);
    state.set_raw_field(ES_EVENT_PARM, event.parameter);
    pool.set_state(number, &state);
    pool.raise_event(number, level_time);
}

/// Everything a flag's touch reaches on the server.
struct ServerFlags<'a> {
    world: &'a mut ServerWorld<(), Peer>,
    peers: &'a crate::players::PlayerRoster,
    items: &'a mut Vec<(EntityId, Pickup)>,
    pool: &'a mut EntityPool,
    team_scores: &'a mut [i32; 2],
    map: Option<&'a super::LoadedMap>,
    status: &'a mut Option<Vec<u8>>,
    level_time: i32,
    /// `AddScore` runs: no warmup is on.
    scoring: bool,
}

impl FlagWorld for ServerFlags<'_> {
    fn slots(&self) -> u16 {
        self.peers.places() as u16
    }

    fn player(&mut self, number: u16) -> Option<FlagPlayer<'_>> {
        let handle = self.peers.at(usize::from(number))?;
        let peer = self.world.entity_mut(handle).filter(|peer| peer.begun)?;
        let (bottom, top) = peer.movement.box_bounds();
        let origin = peer.state.origin();
        let (absmin, absmax) = (
            std::array::from_fn(|axis| origin[axis] + bottom[axis] - 1.0),
            std::array::from_fn(|axis| origin[axis] + top[axis] + 1.0),
        );
        let team = peer.session.team;
        Some(FlagPlayer {
            state: &mut peer.state,
            health: peer.health,
            team,
            connected: true,
            team_state: &mut peer.team_state,
            absmin,
            absmax,
            current_origin: origin,
        })
    }

    fn add_score(&mut self, number: u16, points: i32) {
        let Some(handle) = self.peers.at(usize::from(number)).filter(|_| self.scoring) else {
            return;
        };
        if let Some(peer) = self.world.entity_mut(handle) {
            peer.state.persistent[0] = (peer.state.persistent[0] as i32 + points) as u32;
        }
    }

    fn team_scores(&mut self) -> &mut [i32; 2] {
        self.team_scores
    }

    fn raise(&mut self, event: EventEntity) {
        let _ = self
            .pool
            .spawn_temporary(event.state(), self.level_time, None);
    }

    fn reset_flag(&mut self, team: i32) -> Option<[f32; 3]> {
        let flags: Vec<(usize, EntityId, bool)> = team_flags(self.items, team).collect();
        let mut base = None;
        for (index, _, dropped) in &flags {
            if !dropped {
                base = Some(base_of(&self.items[*index].1));
                respawn_item(self.items, self.pool, *index, self.level_time);
            }
        }
        for (_, number, dropped) in flags {
            if dropped {
                self.pool.free(number, self.level_time);
                self.items.retain(|(ours, _)| *ours != number);
            }
        }
        base
    }

    fn base_flag(&mut self, team: i32) -> Option<[f32; 3]> {
        team_flags(self.items, team)
            .find(|(_, _, dropped)| !dropped)
            .map(|(index, _, _)| self.items[index].1.origin)
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.map
            .is_none_or(|map| Eye::new(&map.bsp, &map.areas, from).sees_point(&map.bsp, to))
    }

    fn flag_status(&mut self, value: Vec<u8>) {
        *self.status = Some(value);
    }
}

impl NativeGame {
    /// `run` with the server's [`FlagWorld`] and flags; afterwards the flag status is
    /// published and — the scores having changed — the ranks recalculated.
    fn with_flags<T>(
        &mut self,
        level_time: i32,
        run: impl FnOnce(&mut Flags, &mut dyn FlagWorld) -> T,
    ) -> T {
        let mut status = None;
        let mut flags = self.flags;
        let scoring = self.scoring();
        let Self {
            server,
            world,
            players,
            items,
            pool,
            team_scores,
            map,
            ..
        } = self;
        let world = server.world_mut(*world).expect("the game's world");
        let mut flag_world = ServerFlags {
            world,
            peers: players,
            items,
            pool,
            team_scores,
            map: map.as_ref(),
            status: &mut status,
            level_time,
            scoring,
        };
        let result = run(&mut flags, &mut flag_world);
        self.flags = flags;
        if let Some(value) = status {
            self.publish_config_string(ctf::CS_FLAGSTATUS, &value);
        }
        self.calculate_ranks();
        result
    }

    /// `Touch_Item` for a flag, item `index`, by `client`: `Pickup_Team`, and for a flag
    /// taken the pickup's event, the global pickup sound and the flag gone from where it
    /// stood. Returns whether the items may have changed (a flag returned or captured).
    pub(super) fn touch_flag(&mut self, client: usize, index: usize, level_time: i32) -> bool {
        let (number, pickup) = &self.items[index];
        let (number, row, dropped) = (
            number.legacy_number(),
            ITEMS[pickup.item],
            pickup.dropped.is_some(),
        );
        let (gametype, intermission) = (self.gametype, self.match_end.intermission_time != 0);
        let Some(peer) = self.peer(client) else {
            return false;
        };
        if !ctf::can_grab(gametype, row.tag, dropped, &peer.state) {
            return false;
        }
        let Some(team) = ctf::flag_team(row.tag) else {
            return false;
        };
        let flag = FlagTouch {
            team,
            dropped,
            base: base_of(pickup),
            current_origin: pickup.origin,
        };
        let respawn = self.with_flags(level_time, |flags, world| {
            ctf::touch(flags, world, flag, client as u16, level_time, intermission)
        });
        if respawn == 0 {
            return true;
        }
        let Some(peer) = self.peer_mut(client) else {
            return false;
        };
        if peer.accepted.predict_item_pickup {
            sjk_game_jka::items::add_predictable_event(
                &mut peer.state,
                sjk_game_jka::items::EV_ITEM_PICKUP,
                u32::from(number),
            );
        } else {
            let bits = (peer.state.raw_field(super::EFLAGS_EVENT).unwrap_or(0) & 0x300)
                .wrapping_add(0x100)
                & 0x300;
            peer.state.set_raw_field(
                super::EFLAGS_EVENT,
                sjk_game_jka::items::EV_ITEM_PICKUP | bits,
            );
            peer.state
                .set_raw_field(super::EFLAGS_EVENT_PARM, u32::from(number));
            peer.entity.event_raised(level_time);
        }
        peer.movement = peer.movement.reseeded(&peer.state);
        let item = self.items[index].1.item;
        let sound = EventEntity {
            event: EV_GLOBAL_ITEM_PICKUP,
            parameter: item as u32,
            origin: flag.base,
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        };
        let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
        let (_, pickup) = &mut self.items[index];
        if dropped {
            sjk_game_jka::dropped_items::taken(pickup, respawn, level_time);
        } else {
            let _ = sjk_game_jka::items::taken(pickup, respawn, level_time, 0.0);
        }
        self.publish_item(index);
        false
    }

    /// An item's own state in the pool, the event the entity carries kept.
    pub(super) fn publish_item(&mut self, index: usize) {
        let (number, pickup) = &self.items[index];
        let mut state = pickup.state.clone();
        let _ = state.set_number(number.legacy_number());
        if let Some(slot) = self.pool.state_mut(*number) {
            for field in [ES_EVENT, ES_EVENT_PARM] {
                state.set_raw_field(field, slot.raw_field(field).unwrap_or(0));
            }
            *slot = state;
        }
    }

    /// `Team_CheckHurtCarrier` after a blow on `target` by `attacker` that landed.
    pub(super) fn flag_blow(&mut self, attacker: usize, target: usize, level_time: i32) {
        if !flag_game(self.gametype) || attacker == target {
            return;
        }
        let Some((state, team)) = self
            .peer(target)
            .map(|peer| (peer.state.clone(), peer.session.team))
        else {
            return;
        };
        if let Some(peer) = self.peer_mut(attacker) {
            ctf::check_hurt_carrier(
                &state,
                team,
                &mut peer.team_state,
                peer.session.team,
                level_time,
            );
        }
    }

    /// `player_die`'s flags (`g_combat.c:2651-2688`): `Team_FragBonuses` for `target`
    /// killed by `attacker` — the flags it carried still on it — then the flag a suicide
    /// or a fall sends home (`Team_ReturnFlag`).
    pub(super) fn flag_death(
        &mut self,
        target: usize,
        attacker: Option<u16>,
        carried: [u32; 2],
        returned: Option<usize>,
        level_time: i32,
    ) {
        if !flag_game(self.gametype) {
            return;
        }
        if let Some(attacker) =
            attacker.filter(|attacker| self.peer(usize::from(*attacker)).is_some())
        {
            let held = self.peer_mut(target).map(|peer| {
                let held = [
                    peer.state.powerups[ctf::PW_REDFLAG],
                    peer.state.powerups[ctf::PW_BLUEFLAG],
                ];
                (
                    peer.state.powerups[ctf::PW_REDFLAG],
                    peer.state.powerups[ctf::PW_BLUEFLAG],
                ) = (carried[0], carried[1]);
                held
            });
            self.with_flags(level_time, |_, world| {
                ctf::frag_bonuses(world, target as u16, attacker, level_time)
            });
            if let (Some(held), Some(peer)) = (held, self.peer_mut(target)) {
                (
                    peer.state.powerups[ctf::PW_REDFLAG],
                    peer.state.powerups[ctf::PW_BLUEFLAG],
                ) = (held[0], held[1]);
            }
        }
        if let Some(team) = returned.and_then(|flag| ctf::flag_team(flag as i32)) {
            self.with_flags(level_time, |flags, world| {
                ctf::return_flag(flags, world, team)
            });
        }
    }

    /// `LaunchItem`'s flag: `Team_CheckDroppedItem`.
    pub(super) fn flag_launched(&mut self, item: usize, level_time: i32) {
        if !flag_game(self.gametype) || ITEMS[item].kind != Kind::Team {
            return;
        }
        let tag = ITEMS[item].tag;
        self.with_flags(level_time, |flags, world| {
            ctf::flag_dropped(flags, world, tag)
        });
    }

    /// A dropped flag's thirty seconds up (`Team_DroppedFlagThink`), or lost where
    /// nothing may lie (`Team_FreeEntity`): it goes home.
    pub(super) fn flag_expired(&mut self, item: usize, lost: bool, level_time: i32) {
        let Some(team) = ctf::flag_team(ITEMS[item].tag) else {
            return;
        };
        self.with_flags(level_time, |flags, world| {
            if lost {
                ctf::return_flag(flags, world, team)
            } else {
                ctf::dropped_flag_expired(flags, world, team)
            }
        });
    }

    /// `Team_InitGame`: both flags at home, which every client is told.
    pub(super) fn init_flags(&mut self) {
        self.flags = Flags::default();
        if flag_game(self.gametype) {
            let value = self.flags.status_string();
            self.publish_config_string(ctf::CS_FLAGSTATUS, &value);
        }
    }
}
