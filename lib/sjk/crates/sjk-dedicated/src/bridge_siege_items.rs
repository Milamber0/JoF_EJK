//! Siege's carried and breakable objectives on the server (`misc_siege_item`): the rules
//! are [`sjk_game_jka::siege_items`]'; here an item gets its entity, thinks every frame
//! (`G_RunThink`), is walked into by players (`G_TouchTriggers`), used by name, struck, and
//! let go when its carrier dies or leaves. The goal triggers that take an item are
//! `bridge_multiples`' (through [`NativeGame::siege_touch_hooks`]).

use super::*;
use sjk_game_jka::siege_items::{
    self, CarrierView, Painted, Registries, SiegeItem, SiegeItemWorld, Touched,
    Toucher as ItemToucher, Trace,
};

/// One siege item of the level, with the entity clients are sent it by.
#[derive(Clone, Debug)]
pub(super) struct PlacedItem {
    pub(super) id: EntityId,
    pub(super) item: SiegeItem,
}

/// `PMF_FOLLOW`: a player following another carries nothing.
const PMF_FOLLOW: u16 = 4_096;
/// `CHAN_AUTO`, the pickup sound's channel.
const CHAN_AUTO: u32 = 0;

/// The level's registries as an item spawn asks them.
struct ItemRegistries<'a> {
    game: &'a mut NativeGame,
}

impl Registries for ItemRegistries<'_> {
    fn sound(&mut self, name: &str) -> u16 {
        let (sounds, told) = (&mut self.game.sounds, &mut self.game.told);
        sounds.index(name.as_bytes(), &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }

    fn effect(&mut self, name: &str) -> u16 {
        let (effects, told) = (&mut self.game.map_effects.effects, &mut self.game.told);
        effects.index(name.as_bytes(), &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }

    fn icon(&mut self, name: &str) -> u16 {
        self.game.icon_index(name.as_bytes())
    }

    fn model(&mut self, name: &str) -> u16 {
        let (models, told) = (&mut self.game.models, &mut self.game.told);
        models.index(name.as_bytes(), &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }
}

/// The server as the world an item thinks in; `solids` are the level's brushes and the
/// solid items, where the item's traces meet them.
struct ItemWorld<'a> {
    game: &'a mut NativeGame,
    solids: Vec<BoxObstacle>,
    level_time: i32,
    /// A client leaving the game (`ClientDisconnect`): no carrier any more.
    leaving: Option<u16>,
}

impl SiegeItemWorld for ItemWorld<'_> {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> Trace {
        let solids: Vec<BoxObstacle> = self
            .solids
            .iter()
            .copied()
            .filter(|solid| solid.entity != pass)
            .collect();
        let traced = match self.game.map.as_ref() {
            Some(map) => {
                let world = WithPlayers {
                    world: WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    },
                    players: &solids,
                };
                world.trace(start, mins, maxs, end, mask)
            }
            None => sjk_game_jka::pmove::MovementTrace::miss(end),
        };
        Trace {
            fraction: traced.fraction,
            end: traced.end_position,
            normal: traced.plane_normal,
            start_solid: traced.start_solid,
            all_solid: traced.all_solid,
            entity: if traced.fraction == 1.0 && !traced.start_solid {
                siege_items::ENTITYNUM_NONE
            } else {
                traced.entity_number
            },
        }
    }

    fn point_contents(&mut self, point: [f32; 3], _pass: u16) -> u32 {
        self.game.map.as_ref().map_or(0, |map| {
            WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            }
            .point_contents(point)
        })
    }

    fn carrier(&mut self, number: u16) -> Option<CarrierView> {
        if self.leaving == Some(number) {
            return None;
        }
        let peer = self.game.peer(usize::from(number))?;
        Some(CarrierView {
            in_game: true,
            origin: peer.state.origin(),
            view_angles: peer.state.view_angles(),
            health: peer.health,
            team: peer.session.team,
            following: peer.state.movement_flags() & PMF_FOLLOW != 0,
        })
    }

    fn use_targets(&mut self, _item: u16, name: &str) {
        self.game.fire_targets(name, usize::MAX, self.level_time);
    }

    fn play_effect(&mut self, effect: u16, origin: [f32; 3]) {
        let event = siege_items::effect_event(effect, origin);
        let _ = self
            .game
            .pool
            .spawn_temporary(event.state(), self.level_time, None);
    }

    fn irand(&mut self, min: i32, max: i32) -> i32 {
        self.game.deaths.rng.irand(min, max)
    }

    fn release(&mut self, carrier: u16) {
        if let Some(peer) = self.game.peer_mut(usize::from(carrier)) {
            peer.siege_hands.holding = 0;
        }
    }
}

impl NativeGame {
    /// `SP_misc_siege_item` for a map entity, while the siege entities spawn.
    pub(super) fn spawn_siege_item(&mut self, entity: &sjk_entity::Entity) {
        let siege = self.siege.is_some();
        let spawned = siege_items::spawn(entity, siege, &mut ItemRegistries { game: self });
        let item = match spawned {
            None => return,
            Some(Err(refused)) => {
                eprintln!("misc_siege_item not spawned (the reference stops the map): {refused:?}");
                return;
            }
            Some(Ok(item)) => item,
        };
        let Some(id) = self.pool.spawn_entity(
            EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
            self.last_frame_time,
        ) else {
            return;
        };
        let Some(siege) = self.siege.as_mut() else {
            return;
        };
        siege.items.push(PlacedItem { id, item });
        self.publish_siege_item(siege_items_len(self) - 1);
    }

    /// An item's entity as clients are sent it.
    fn publish_siege_item(&mut self, index: usize) {
        let Some(placed) = self
            .siege
            .as_ref()
            .and_then(|siege| siege.items.get(index))
            .cloned()
        else {
            return;
        };
        if let Some(state) = self.pool.state_mut(placed.id) {
            siege_items::project(&placed.item, state);
        }
        self.pool
            .set_bounds(placed.id, (placed.item.mins, placed.item.maxs));
        self.pool.set_broadcast(
            placed.id,
            placed.item.svflags & siege_items::SVF_BROADCAST != 0,
        );
    }

    /// The solid items and the level's brushes, as an item's traces meet them.
    fn item_solids(&self, items: &[PlacedItem]) -> Vec<BoxObstacle> {
        let mut solids: Vec<BoxObstacle> =
            crate::collision::brush_obstacles(&self.breakables, &self.doors, self.last_frame_time)
                .chain(self.stock.obstacles())
                .chain(self.scripts.brushes())
                .collect();
        solids.extend(
            items
                .iter()
                .filter(|placed| !placed.item.freed && placed.item.contents != 0)
                .map(|placed| BoxObstacle {
                    entity: placed.id.legacy_number(),
                    origin: placed.item.origin,
                    bounds: (placed.item.mins, placed.item.maxs),
                    contents: placed.item.contents,
                    model: None,
                }),
        );
        solids
    }

    /// Runs `call` on the level's items, the server lent as their world; then publishes
    /// them and frees those broken or delivered.
    fn with_siege_items(
        &mut self,
        level_time: i32,
        call: impl FnOnce(&mut Vec<PlacedItem>, &mut ItemWorld<'_>),
    ) {
        self.with_siege_items_leaving(level_time, None, call);
    }

    /// [`Self::with_siege_items`] while client `leaving` leaves the game.
    fn with_siege_items_leaving(
        &mut self,
        level_time: i32,
        leaving: Option<u16>,
        call: impl FnOnce(&mut Vec<PlacedItem>, &mut ItemWorld<'_>),
    ) {
        let Some(mut items) = self
            .siege
            .as_mut()
            .map(|siege| std::mem::take(&mut siege.items))
        else {
            return;
        };
        let solids = self.item_solids(&items);
        call(
            &mut items,
            &mut ItemWorld {
                game: self,
                solids,
                level_time,
                leaving,
            },
        );
        let freed: Vec<EntityId> = items
            .iter()
            .filter(|placed| placed.item.freed)
            .map(|placed| placed.id)
            .collect();
        items.retain(|placed| !placed.item.freed);
        for id in freed {
            self.pool.free(id, level_time);
        }
        let count = items.len();
        if let Some(siege) = self.siege.as_mut() {
            siege.items = items;
        }
        for index in 0..count {
            self.publish_siege_item(index);
        }
    }

    /// `G_RunThink` for the items that think, in the level's order.
    pub(super) fn run_siege_items(&mut self, level_time: i32) {
        self.with_siege_items(level_time, |items, world| {
            for placed in items
                .iter_mut()
                .filter(|placed| placed.item.thinks && !placed.item.freed)
            {
                siege_items::think(
                    &mut placed.item,
                    placed.id.legacy_number(),
                    world,
                    level_time,
                );
            }
        });
    }

    /// `G_TouchTriggers` for the items player `client` stands in (`EntityContact` on their
    /// boxes, grown a unit as a link grows them): a pickup takes the item, plays its sound
    /// on the player and fires its `target2`.
    pub(super) fn touch_siege_items(&mut self, client: usize, level_time: i32) {
        let Some(siege) = self.siege.as_ref() else {
            return;
        };
        if siege.items.is_empty() {
            return;
        }
        let round_begun = siege.round.begun;
        let Some(peer) = self.peer(client) else {
            return;
        };
        let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
        let who = ItemToucher {
            number: client as u16,
            player: true,
            health: peer.health,
            carrying: peer.siege_hands.holding != 0,
            spectator: !peer.playing(),
            team: peer.session.team,
        };
        let low: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.0[axis]);
        let high: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.1[axis]);
        let mut picked = Vec::new();
        if let Some(siege) = self.siege.as_mut() {
            for placed in siege.items.iter_mut() {
                let item = &mut placed.item;
                if item.freed || !item.touch || item.contents & siege_items::CONTENTS_TRIGGER == 0 {
                    continue;
                }
                // `EntityContact`: the player's box overlaps the item's.
                let touching = (0..3).all(|axis| {
                    item.origin[axis] + item.mins[axis] < high[axis]
                        && item.origin[axis] + item.maxs[axis] > low[axis]
                });
                if !touching {
                    continue;
                }
                let carrying = who.carrying || !picked.is_empty();
                if let Touched::PickedUp { sound, fire } = siege_items::touch(
                    item,
                    Some(ItemToucher { carrying, ..who }),
                    round_begun,
                    false,
                ) {
                    picked.push((placed.id.legacy_number(), sound, fire));
                }
            }
        }
        for (number, sound, fire) in picked {
            if let Some(sound) = sound {
                let mut event =
                    sjk_game_jka::knockdown::entity_sound(origin, client as u16, CHAN_AUTO);
                event.parameter = u32::from(sound);
                let _ = self.pool.spawn_temporary(event.state(), level_time, None);
            }
            if let Some(peer) = self.peer_mut(client) {
                peer.siege_hands.holding = number;
            }
            if let Some(target) = fire {
                self.fire_targets(&target, usize::MAX, level_time);
            }
        }
    }

    /// `player_die` and `ClientDisconnect` on a carrier: its item thinks at once, so that
    /// it drops (or goes home) before anything else happens to the player.
    pub(super) fn siege_carrier_gone(&mut self, client: usize, level_time: i32) {
        self.siege_carrier_think(client, None, level_time);
    }

    /// `ClientDisconnect` on a carrier: its item, carried by nobody now, goes home.
    pub(super) fn siege_carrier_left(&mut self, client: usize) {
        let level_time = self.last_frame_time;
        self.siege_carrier_think(client, Some(client as u16), level_time);
    }

    fn siege_carrier_think(&mut self, client: usize, leaving: Option<u16>, level_time: i32) {
        let Some(holding) = self
            .peer(client)
            .map(|peer| peer.siege_hands.holding)
            .filter(|holding| *holding != 0)
        else {
            return;
        };
        self.with_siege_items_leaving(level_time, leaving, |items, world| {
            if let Some(placed) = items
                .iter_mut()
                .find(|placed| placed.id.legacy_number() == holding && placed.item.thinks)
            {
                siege_items::think(&mut placed.item, holding, world, level_time);
            }
        });
    }

    /// `G_UseTargets2` reaching the items called `name` (`SiegeItemUse`): each appears, on
    /// its `paintarget` where it has one.
    pub(super) fn use_siege_items(&mut self, name: &str, level_time: i32) {
        let named = |placed: &PlacedItem| {
            placed
                .item
                .targetname
                .as_deref()
                .is_some_and(|own| own.eq_ignore_ascii_case(name))
                && placed.item.on_use == siege_items::OnUse::Activate
        };
        if !self
            .siege
            .as_ref()
            .is_some_and(|siege| siege.items.iter().any(named))
        {
            return;
        }
        let paints: Vec<Option<Painted>> = self
            .siege
            .as_ref()
            .map(|siege| {
                siege
                    .items
                    .iter()
                    .map(|placed| {
                        placed
                            .item
                            .paintarget
                            .as_deref()
                            .and_then(|target| self.paint_target(target))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.with_siege_items(level_time, |items, world| {
            for (placed, paint) in items.iter_mut().zip(paints) {
                if named(placed) {
                    siege_items::use_item(
                        &mut placed.item,
                        placed.id.legacy_number(),
                        world,
                        paint,
                    );
                }
            }
        });
    }

    /// `G_Find(NULL, targetname, name)` for a `paintarget`: the first entity of the lump so
    /// named, where it stands. Such an entity (an `info_notnull`, a `target_position`) is
    /// nothing a trace meets and faces nowhere (`r.currentAngles` is only set by movers and
    /// players).
    fn paint_target(&self, name: &str) -> Option<Painted> {
        let named = self
            .stock
            .named
            .iter()
            .find(|named| named.name.eq_ignore_ascii_case(name))?;
        Some(Painted {
            number: siege_items::ENTITYNUM_NONE,
            origin: named.origin,
            facing: [0.0; 3],
        })
    }

    /// A goal trigger took the item `client` carries: its `target3` where it has one, and
    /// it goes (`SiegeItemRemoveOwner`, `G_FreeEntity`).
    pub(super) fn deliver_siege_item(
        &mut self,
        client: usize,
        target3: Option<String>,
        level_time: i32,
    ) {
        let Some(holding) = self
            .peer(client)
            .map(|peer| peer.siege_hands.holding)
            .filter(|holding| *holding != 0)
        else {
            return;
        };
        if let Some(target) = target3 {
            self.fire_targets(&target, usize::MAX, level_time);
        }
        self.with_siege_items(level_time, |items, world| {
            if let Some(placed) = items
                .iter_mut()
                .find(|placed| placed.id.legacy_number() == holding)
            {
                siege_items::remove_owner(&mut placed.item, world, Some(client as u16));
                placed.item.thinks = false;
                placed.item.freed = true;
            }
        });
    }

    /// A blow on entity `number` when it is a siege item (`G_Damage`). Whether it was.
    pub(super) fn strike_siege_item(&mut self, number: u16, request: DamageRequest) -> bool {
        if !self.siege.as_ref().is_some_and(|siege| {
            siege
                .items
                .iter()
                .any(|placed| placed.id.legacy_number() == number)
        }) {
            return false;
        }
        let attacker = request
            .attacker
            .map(|attacker| attacker.client)
            .and_then(|number| self.blow_attacker(number));
        let blow = sjk_game_jka::map_turret_world::ObjectBlow {
            attacker,
            damage: request.damage,
            flags: request.flags,
            means: request.means,
            siege: self.gametype == GAMETYPE_SIEGE,
            siege_round_begun: self.siege.as_ref().is_some_and(|siege| siege.round.begun),
            friendly_fire_objectives: self.integer_cvar(b"g_ff_objectives", 0) != 0,
        };
        self.hurt_siege_item(number, blow, request.level_time)
    }

    /// `G_Damage` on item `number`: what comes off its health (`object_take`), its pain or
    /// its death. Whether it was an item.
    pub(super) fn hurt_siege_item(
        &mut self,
        number: u16,
        blow: sjk_game_jka::map_turret_world::ObjectBlow,
        level_time: i32,
    ) -> bool {
        let Some(siege) = self.siege.as_ref() else {
            return false;
        };
        let Some(index) = siege
            .items
            .iter()
            .position(|placed| placed.id.legacy_number() == number)
        else {
            return false;
        };
        let take_damage = siege.items[index].item.take_damage;
        let Some(take) = sjk_game_jka::map_turret_world::object_take(number, take_damage, 0, &blow)
        else {
            return true;
        };
        self.with_siege_items(level_time, |items, world| {
            let item = &mut items[index].item;
            item.health -= take;
            if item.health <= 0 {
                siege_items::die(item, number, world);
            } else {
                siege_items::pain(item, level_time);
            }
        });
        true
    }

    /// The solid items (a breakable one that cannot be picked up), as players' traces meet
    /// them.
    pub(super) fn siege_item_obstacles(&self) -> Vec<BoxObstacle> {
        let Some(siege) = self.siege.as_ref() else {
            return Vec::new();
        };
        siege
            .items
            .iter()
            .filter(|placed| placed.item.contents & siege_items::CONTENTS_SOLID != 0)
            .map(|placed| BoxObstacle {
                entity: placed.id.legacy_number(),
                origin: placed.item.origin,
                bounds: (placed.item.mins, placed.item.maxs),
                contents: placed.item.contents,
                model: None,
            })
            .collect()
    }

    /// The item player `client` carries, as a goal trigger reads it.
    pub(super) fn carried_item(
        &self,
        client: usize,
    ) -> Option<(Option<String>, i32, Option<String>)> {
        let holding = self.peer(client)?.siege_hands.holding;
        let placed = self
            .siege
            .as_ref()?
            .items
            .iter()
            .find(|placed| placed.id.legacy_number() == holding && !placed.item.freed)?;
        Some((
            placed.item.goaltarget.clone(),
            placed.item.team_no_complete,
            placed.item.target3.clone(),
        ))
    }
}

/// How many items the level has.
fn siege_items_len(game: &NativeGame) -> usize {
    game.siege.as_ref().map_or(0, |siege| siege.items.len())
}
