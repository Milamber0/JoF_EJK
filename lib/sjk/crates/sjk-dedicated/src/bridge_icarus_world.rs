//! The server as the world a script acts on ([`ScriptWorld`]): the script entities and
//! players of `bridge_icarus`, the server's pool, sounds, names and players behind the
//! game's script code.

use super::{PendingUse, ScriptOwner, ScriptSlot, SlotKind};
use crate::bridge::{NativeGame, Told};
use sjk_game_jka::damage::{Attacker, DamageRequest};
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::player_death::DeathRequest;
use sjk_game_jka::ref_tags::RefTags;
use sjk_game_jka::script_entity::{EntityKind, ScriptEntity};
use sjk_game_jka::script_runner;
use sjk_game_jka::script_world::{ClientAction, ClientView, NameField, NpcAction, ScriptWorld};
use sjk_icarus::Icarus;

/// `EV_GENERAL_SOUND`, `EV_GLOBAL_SOUND`, `EV_PLAYDOORSOUND`.
const EV_GENERAL_SOUND: u32 = 76;
const EV_GLOBAL_SOUND: u32 = 77;
const EV_PLAYDOORSOUND: u32 = 71;
/// `s.saberEntityNum`, where `G_Sound` puts the channel.
const ES_SABER_ENTITY_NUM: usize = 37;
/// `STAT_HEALTH`, `STAT_ARMOR`, `STAT_MAX_HEALTH`.
const STAT_HEALTH: usize = 0;
const STAT_ARMOR: usize = 5;
const STAT_MAX_HEALTH: usize = 8;
/// A player's `r.contents` and box (`CONTENTS_BODY`; the standing box).
const CONTENTS_BODY: u32 = 0x100;

/// The server lent to the game's script code for one call.
pub(in crate::bridge) struct ServerWorld<'a> {
    pub(in crate::bridge) game: &'a mut NativeGame,
}

impl ServerWorld<'_> {
    /// The script entities of the level in the reference's number order: the players,
    /// then the rest in map order.
    fn owners(&self) -> impl Iterator<Item = ScriptOwner> + '_ {
        let scripts = &self.game.scripts;
        let clients = (0..scripts.clients.len())
            .filter(|&client| scripts.clients[client].is_some())
            .map(|client| ScriptOwner::Client(client as u16));
        let slots = (0..scripts.slots.len())
            .filter(|&index| scripts.slots[index].is_some())
            .map(|index| ScriptOwner::Entity(index as u32));
        clients.chain(slots)
    }

    fn slot(&self, id: ScriptOwner) -> Option<&ScriptSlot> {
        match id {
            ScriptOwner::Entity(index) => self
                .game
                .scripts
                .slots
                .get(index as usize)
                .and_then(Option::as_ref),
            ScriptOwner::Client(_) => None,
        }
    }

    /// `G_UseTargets2`'s part for the script entities, for every name fired while the
    /// interpreter was busy: each of that name is used (`GlobalUse`), but the one that
    /// fired it ("Entity used itself.").
    pub(in crate::bridge) fn use_pending(&mut self, icarus: &mut Icarus<ScriptOwner>) {
        while !self.game.scripts.pending.is_empty() {
            let PendingUse {
                name,
                from,
                activator,
            } = self.game.scripts.pending.remove(0);
            let named: Vec<ScriptOwner> = self
                .owners()
                .filter(|&owner| {
                    self.entity(owner).is_some_and(|ent| {
                        ent.targetname
                            .as_deref()
                            .is_some_and(|own| own.eq_ignore_ascii_case(&name))
                    })
                })
                .collect();
            for owner in named {
                if Some(owner) == from {
                    self.print("WARNING: Entity used itself.\n");
                    continue;
                }
                script_runner::use_entity(self, icarus, owner, from.or(activator), activator);
            }
        }
    }

    /// The wire client number a script owner stands for, `usize::MAX` for none.
    fn client_number(owner: Option<ScriptOwner>) -> usize {
        match owner {
            Some(ScriptOwner::Client(client)) => usize::from(client),
            _ => usize::MAX,
        }
    }

    /// Where an entity is, and the number clients know it by.
    fn place(&self, id: ScriptOwner) -> Option<([f32; 3], Option<u16>)> {
        match id {
            ScriptOwner::Client(client) => self
                .game
                .peer(usize::from(client))
                .map(|peer| (peer.state.origin(), Some(client))),
            ScriptOwner::Entity(_) => self.slot(id).map(|slot| {
                (
                    slot.entity.motion.origin,
                    slot.pool.map(|pool| pool.legacy_number()),
                )
            }),
        }
    }

    /// `player_die` for a player a script killed: `health` is what was set before it.
    fn player_die(&mut self, client: usize, health: i32, damage: i32, means: u32) {
        let level_time = self.game.last_frame_time;
        let Some(peer) = self.game.peer_mut(client) else {
            return;
        };
        if !peer.playing() {
            return;
        }
        let request = DeathRequest {
            means,
            damage,
            ..DeathRequest::suicide(
                level_time,
                client as u16,
                peer.state.origin(),
                peer.saber_off_sounds(),
                health,
            )
        };
        self.game.die(client, request);
    }
}

impl ScriptWorld for ServerWorld<'_> {
    type Id = ScriptOwner;

    fn level_time(&self) -> i32 {
        self.game.last_frame_time
    }

    fn previous_time(&self) -> i32 {
        self.game.previous_frame_time
    }

    fn server_time(&self) -> u32 {
        self.game.last_frame_time as u32
    }

    fn developer(&self) -> i32 {
        self.game.cvars.integer(b"developer")
    }

    fn print(&mut self, text: &str) {
        eprint!("{text}");
    }

    fn broadcast_command(&mut self, command: &str) {
        self.game
            .told
            .push(Told::Everyone(command.as_bytes().to_vec()));
    }

    fn read_file(&mut self, path: &str) -> Option<Vec<u8>> {
        self.game
            .map
            .as_ref()?
            .files
            .read(path)
            .ok()
            .flatten()
            .map(|asset| asset.bytes.to_vec())
    }

    fn engine_random(&mut self, min: f32, max: f32) -> f32 {
        self.game.scripts.random(min, max)
    }

    fn sound_index(&mut self, name: &str) -> i32 {
        let told = &mut self.game.told;
        i32::from(
            self.game
                .sounds
                .index(name.as_bytes(), &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                }),
        )
    }

    fn sound_set_index(&mut self, name: &str) -> i32 {
        i32::from(self.game.map_effects.sound_set(name))
    }

    fn timescale(&self) -> f32 {
        self.game.cvars.var(b"timescale").map_or(1.0, |var| {
            sjk_icarus::cnum::atof(&String::from_utf8_lossy(&var.string)) as f32
        })
    }

    fn set_timescale(&mut self, value: &str) {
        self.game.cvars.set(b"timescale", value.as_bytes());
    }

    fn gravity(&self) -> f32 {
        self.game.gravity()
    }

    fn tags(&self) -> &RefTags {
        &self.game.scripts.tags
    }

    fn animation(&self, name: &str) -> Option<i32> {
        sjk_game_jka::legacy_animation::NAMES
            .iter()
            .position(|known| known.eq_ignore_ascii_case(name))
            .map(|index| index as i32)
    }

    fn client_animations(&self, id: ScriptOwner) -> Option<(String, String)> {
        let ScriptOwner::Client(client) = id else {
            return None;
        };
        let peer = self.game.peer(usize::from(client))?;
        let name = |anim: u16| {
            sjk_game_jka::legacy_animation::NAMES
                .get(usize::from(anim))
                .copied()
                .unwrap_or_default()
                .to_owned()
        };
        Some((
            name(peer.state.leg_animation() & !0x800),
            name(peer.state.torso_animation() & !0x800),
        ))
    }

    fn entity(&self, id: ScriptOwner) -> Option<&ScriptEntity<ScriptOwner>> {
        match id {
            ScriptOwner::Client(client) => self
                .game
                .scripts
                .clients
                .get(usize::from(client))
                .and_then(Option::as_ref),
            ScriptOwner::Entity(index) => self
                .game
                .scripts
                .slots
                .get(index as usize)
                .and_then(Option::as_ref)
                .map(|slot| &slot.entity),
        }
    }

    fn entity_mut(&mut self, id: ScriptOwner) -> Option<&mut ScriptEntity<ScriptOwner>> {
        match id {
            ScriptOwner::Client(client) => self
                .game
                .scripts
                .clients
                .get_mut(usize::from(client))
                .and_then(Option::as_mut),
            ScriptOwner::Entity(index) => self
                .game
                .scripts
                .slots
                .get_mut(index as usize)
                .and_then(Option::as_mut)
                .map(|slot| &mut slot.entity),
        }
    }

    fn find(
        &self,
        after: Option<ScriptOwner>,
        field: NameField,
        name: &str,
    ) -> Option<ScriptOwner> {
        let mut owners = self.owners();
        if let Some(after) = after {
            owners.by_ref().find(|&owner| owner == after)?;
        }
        owners.find(|&owner| {
            self.entity(owner).is_some_and(|ent| {
                let value = match field {
                    NameField::Targetname => ent.targetname.as_deref(),
                    NameField::ScriptTargetname => ent.script_targetname.as_deref(),
                    NameField::NpcTargetname => None,
                };
                value.is_some_and(|value| value.eq_ignore_ascii_case(name))
            })
        })
    }

    fn spawn(&mut self, classname: &str) -> Option<ScriptOwner> {
        let index = self.game.scripts.slots.len() as u32;
        let entity = ScriptEntity::new(classname, EntityKind::Other, [0.0; 3], [0.0; 3]);
        self.game.scripts.slots.push(Some(ScriptSlot {
            entity,
            kind: SlotKind::Unseen,
            pool: None,
            model: 0,
            published: None,
        }));
        Some(ScriptOwner::Entity(index))
    }

    fn free(&mut self, _icarus: &mut Icarus<ScriptOwner>, id: ScriptOwner) {
        let ScriptOwner::Entity(index) = id else {
            return;
        };
        let Some(slot) = self
            .game
            .scripts
            .slots
            .get_mut(index as usize)
            .and_then(Option::take)
        else {
            return;
        };
        if let Some(pool) = slot.pool {
            self.game.pool.free(pool, self.game.last_frame_time);
        }
        // A freed trigger touches nobody again.
        if let SlotKind::Multiple(trigger) = slot.kind
            && let Some(trigger) = self.game.multiples.get_mut(trigger)
        {
            trigger.contents = 0;
            trigger.inactive = true;
        }
    }

    fn next_new_script_name(&mut self) -> i32 {
        self.game.scripts.new_names += 1;
        self.game.scripts.new_names - 1
    }

    fn use_targets(
        &mut self,
        icarus: &mut Icarus<ScriptOwner>,
        ent: ScriptOwner,
        activator: Option<ScriptOwner>,
        target: &str,
    ) {
        let level_time = self.game.last_frame_time;
        // The server's own entities of that name first; the script entities are queued
        // by the same call and used here, the one firing excepted.
        let queued = self.game.scripts.pending.len();
        self.game
            .fire_targets(target, Self::client_number(activator), level_time);
        for pending in self.game.scripts.pending.iter_mut().skip(queued) {
            pending.from = Some(ent);
        }
        self.use_pending(icarus);
    }

    fn die(&mut self, _icarus: &mut Icarus<ScriptOwner>, victim: ScriptOwner, damage: i32) {
        if let ScriptOwner::Client(client) = victim {
            self.player_die(
                usize::from(client),
                0,
                damage,
                sjk_game_jka::means_of_death::MOD_UNKNOWN,
            );
        }
    }

    fn spot_would_telefrag(&self, mover: ScriptOwner, dest: [f32; 3]) -> bool {
        let Some(ent) = self.entity(mover) else {
            return false;
        };
        let (low, high): ([f32; 3], [f32; 3]) = (
            std::array::from_fn(|axis| dest[axis] + ent.mins[axis]),
            std::array::from_fn(|axis| dest[axis] + ent.maxs[axis]),
        );
        if ent.contents & CONTENTS_BODY == 0 {
            return false;
        }
        (0..self.game.players.places()).any(|client| {
            if mover == ScriptOwner::Client(client as u16) {
                return false;
            }
            self.game
                .peer(client)
                .filter(|peer| peer.playing() && peer.health > 0)
                .is_some_and(|peer| {
                    let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
                    (0..3).all(|axis| {
                        origin[axis] + bounds.0[axis] <= high[axis]
                            && origin[axis] + bounds.1[axis] >= low[axis]
                    })
                })
        })
    }

    fn link(&mut self, _id: ScriptOwner) {}

    fn sound(&mut self, id: ScriptOwner, channel: i32, index: i32) {
        let Some((origin, _)) = self.place(id) else {
            return;
        };
        let mut extra = [(0, 0); 12];
        extra[0] = (ES_SABER_ENTITY_NUM, channel as u32);
        let event = EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: index as u32,
            origin,
            client: None,
            broadcast: false,
            extra,
        };
        let _ = self
            .game
            .pool
            .spawn_temporary(event.state(), self.game.last_frame_time, None);
    }

    fn global_sound(&mut self, origin: [f32; 3], index: i32) {
        let event = EventEntity {
            event: EV_GLOBAL_SOUND,
            parameter: index as u32,
            origin,
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        };
        let _ = self
            .game
            .pool
            .spawn_temporary(event.state(), self.game.last_frame_time, None);
    }

    fn door_sound(&mut self, id: ScriptOwner, _sound_set: i32, kind: u32) {
        let Some(pool) = self.slot(id).and_then(|slot| slot.pool) else {
            return;
        };
        let level_time = self.game.last_frame_time;
        self.game.raise_on(pool, EV_PLAYDOORSOUND, kind, level_time);
    }

    fn cache_roff(&mut self, name: &str) -> i32 {
        self.game.scripts.report_once("roff", || format!("play(PLAY_ROFF, \"{name}\"): this server plays no ROFFs; the script goes on as without the file"));
        0
    }

    fn play_roff(&mut self, _id: ScriptOwner, _roff: i32) {}

    fn lock_doors(&mut self, _id: ScriptOwner, locked: bool) {
        self.game.scripts.report_once("lockdoors", || {
            format!(
                "SET_INACTIVE \"{}\": doors are no script entities on this server",
                if locked { "locked" } else { "unlocked" }
            )
        });
    }

    fn push_mover(
        &mut self,
        _icarus: &mut Icarus<ScriptOwner>,
        id: ScriptOwner,
        move_by: [f32; 3],
        _turn_by: [f32; 3],
    ) -> Result<(), Option<ScriptOwner>> {
        let Some(slot) = self
            .slot(id)
            .filter(|slot| slot.kind == SlotKind::FuncStatic && slot.entity.contents != 0)
        else {
            return Ok(());
        };
        let (from, bounds, model, number) = (
            slot.entity.motion.origin,
            (slot.entity.mins, slot.entity.maxs),
            slot.model,
            slot.pool
                .map(|pool| pool.legacy_number())
                .unwrap_or(u16::MAX),
        );
        let to: [f32; 3] = std::array::from_fn(|axis| from[axis] + move_by[axis]);
        match self
            .game
            .push_scripted_brush(bounds, model, number, from, to)
        {
            Some(client) => Err(Some(ScriptOwner::Client(client))),
            None => Ok(()),
        }
    }

    fn mover_blocked(
        &mut self,
        _icarus: &mut Icarus<ScriptOwner>,
        id: ScriptOwner,
        obstacle: Option<ScriptOwner>,
    ) {
        // `Blocked_Mover`: a living player in the way takes the mover's damage.
        let Some(ScriptOwner::Client(client)) = obstacle else {
            return;
        };
        let Some(slot) = self.slot(id) else { return };
        let (damage, number) = (
            slot.entity.motion.damage,
            slot.pool.map(|pool| pool.legacy_number()).unwrap_or(0),
        );
        if damage != 0 {
            let attacker = Attacker {
                npc: false,
                client: number,
                max_health: 100,
                team: 0,
                saber_knockback: [0.0; 4],
            };
            let request = DamageRequest {
                level_time: self.game.last_frame_time,
                attacker: Some(attacker),
                direction: None,
                point: None,
                damage,
                flags: 0,
                means: sjk_game_jka::movers::MOD_CRUSH,
            };
            let _ = self.game.hurt(usize::from(client), request);
        }
    }

    fn run_own_think(&mut self, _icarus: &mut Icarus<ScriptOwner>, _id: ScriptOwner) {}

    fn client(&self, id: ScriptOwner) -> Option<ClientView> {
        let ScriptOwner::Client(client) = id else {
            return None;
        };
        let peer = self.game.peer(usize::from(client))?;
        let stat = |index: usize| peer.state.stats[index] as i32;
        Some(ClientView {
            health: peer.health,
            stat_health: stat(STAT_HEALTH),
            armor: stat(STAT_ARMOR),
            max_health: stat(STAT_MAX_HEALTH),
            velocity: peer.state.velocity(),
            legs_timer: peer.state.legs_timer(),
            torso_timer: peer.state.torso_timer(),
            view_height: peer.state.view_height(),
            spectator: !peer.playing(),
            temp_spectating: false,
            saber_holstered: i32::from(peer.state.saber_holstered()),
            sabers_off: peer.state.saber_holstered() != 0,
            eye_point: peer.state.origin(),
        })
    }

    fn client_action(
        &mut self,
        _icarus: &mut Icarus<ScriptOwner>,
        id: ScriptOwner,
        action: ClientAction,
    ) {
        let ScriptOwner::Client(client) = id else {
            return;
        };
        let client = usize::from(client);
        match action {
            ClientAction::SetHealth { health, stat } => {
                if let Some(peer) = self.game.peer_mut(client) {
                    peer.health = health;
                    peer.state.stats[STAT_HEALTH] = stat as u32;
                }
            }
            ClientAction::SetArmor(armor) => {
                if let Some(peer) = self.game.peer_mut(client) {
                    peer.state.stats[STAT_ARMOR] = armor as u32;
                }
            }
            ClientAction::Die => {
                if let Some(peer) = self.game.peer_mut(client) {
                    peer.health = -999;
                    peer.state.stats[STAT_HEALTH] = (-999i32) as u32;
                }
                self.player_die(client, -999, 100_000, sjk_game_jka::damage::MOD_FALLING);
            }
            ClientAction::Teleport(origin) => {
                if let Some(peer) = self.game.peer_mut(client) {
                    peer.state
                        .set_origin([origin[0], origin[1], origin[2] + 1.0]);
                    peer.state.set_velocity([0.0; 3]);
                    peer.movement = peer.movement.reseeded(&peer.state);
                }
            }
            other => {
                let what = format!("{other:?}");
                let kind = what
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                self.game
                    .scripts
                    .report_once(&format!("client {kind}"), || {
                        format!("{what} on a player: not applied by this server yet")
                    });
            }
        }
    }

    fn npc_action(
        &mut self,
        _icarus: &mut Icarus<ScriptOwner>,
        _task: i32,
        _id: ScriptOwner,
        action: NpcAction,
    ) -> bool {
        self.game.scripts.report_once("npc", || {
            format!("{action:?}: NPCs are no script entities on this server yet")
        });
        true
    }
}
