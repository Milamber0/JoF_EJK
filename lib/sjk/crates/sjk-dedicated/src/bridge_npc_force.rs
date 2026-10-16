//! The players as a Jedi NPC reaches and reads them on this server: its AI's view of a
//! player's client ([`NpcHost::jedi_player`], `NPC_AI_Jedi.c`'s reads of `enemy->client`),
//! of a thrown saber or missile coming at it ([`NpcHost::entity_motion`]), and its Force
//! powers' reach beyond the NPCs ([`PlayerForce`], `WP_ForcePowersUpdate` for an NPC,
//! `w_force.c:4969-5480`): grip, lightning, drain, push and pull on the players, the missiles
//! a push turns back, the weapons a pull tears away.
//!
//! A power's blow on a player is dealt once the NPC's turn is over
//! ([`NpcOutcome::ForceBlow`]: the player's side of `G_Damage` needs the whole game). What
//! else a power's use leaves behind is done as it ends ([`PlayerForce::force_end`]): the
//! loops its freed trackers played are stopped (`kls`), the weapons its pulls tore away
//! become pickups, and every player it changed restarts its movement from its state.

use super::super::super::bridge_force::{
    linked_box, missile_candidates, player_candidates, reflect_from, remember_touched,
    toss_weapon_of,
};
use super::*;
use sjk_game_jka::force_dark::OtherPlayer;
use sjk_game_jka::force_throw::{Candidate, ThrowPlayer};
use sjk_game_jka::npc_force_update::PlayerForce;
use sjk_game_jka::npc_jedi_glue::{EntityMotion, JediBlade, JediClient};
use sjk_game_jka::npc_missile_block::IncomingEntity;

/// Wire fields: `s.eType`, `s.pos.trType`; `ET_MISSILE`; `MOD_SABER`.
const ES_TYPE: usize = 8;
const ES_POS_TYPE: usize = 23;
const ET_MISSILE: u32 = 3;
const MOD_SABER: u32 = 3;

/// `SS_DUAL`, `SS_STAFF`: the styles whose sabers are off only when fully holstered.
const SS_DUAL: u8 = 6;
const SS_STAFF: u8 = 7;

/// What the NPCs' Force powers leave for their use's end, kept between frames so that no
/// use allocates: the players they touched with their states before, the weapons their
/// pulls tore away, the sound trackers they freed; and how deep in uses the NPCs are.
#[derive(Default)]
pub(in super::super) struct ForceTouch {
    touched: Vec<u16>,
    before: Vec<PlayerState>,
    tossed: Vec<Pickup>,
    stopped: Vec<(u16, u16)>,
    depth: u32,
}

impl ServerHost<'_> {
    /// Player `number`'s client as a Jedi NPC's AI reads it: a begun, playing peer.
    pub(super) fn jedi_client_of(&self, number: u16) -> Option<JediClient> {
        let peer = self
            .peer(number)
            .filter(|peer| peer.begun && peer.body_active())?;
        let state = &peer.state;
        let (mins, maxs) = peer.movement.box_bounds();
        let (blades, blades_old) = (&peer.blades.0, &peer.blades_old.0);
        let read = |blade: Option<sjk_game_jka::server_skeleton::Blade>| {
            blade.map_or(([0.0; 3], [0.0; 3]), |blade| (blade.base, blade.direction))
        };
        let blade = |saber: usize| {
            let ((point, dir), (point_old, dir_old)) =
                (read(blades[saber][0]), read(blades_old[saber][0]));
            JediBlade {
                point,
                dir,
                point_old,
                dir_old,
            }
        };
        // `BG_SabersOff` (`bg_saber.c`): a dual or staff style is off only when both halves are.
        let holstered = state.saber_holstered();
        let sabers_off = holstered != 0
            && !(matches!(
                peer.movement.state().saber_anim_level_base,
                SS_DUAL | SS_STAFF
            ) && holstered < 2);
        Some(JediClient {
            number,
            origin: state.origin(),
            mins,
            maxs,
            // A client's `r.currentAngles` are its view.
            current_angles: state.view_angles(),
            health: peer.health,
            enemy: peer.npc_enemy,
            // A player's `painDebounceTime` and `attackDebounceTime` are never written by the
            // multiplayer game (only NPCs', `NPC_reactions.c:363-368`, `NPC_combat.c:1043`,
            // and other entities'): always 0.
            pain_debounce_time: 0,
            attack_debounce_time: 0,
            view_angles: state.view_angles(),
            velocity: state.velocity(),
            legs_anim: state.leg_animation(),
            ground_entity: state.ground_entity_num(),
            weapon: state.weapon(),
            weapon_time: state.weapon_time(),
            weapon_state: state.weapon_state(),
            saber_move: state.saber_move(),
            saber_lock_time: state.saber_lock_time(),
            sabers_off,
            saber_in_flight: state.saber_in_flight(),
            saber_entity_num: state.saber_entity_num(),
            // `ps.saberEntityState`: set once the throw's flight has started.
            saber_entity_state: i32::from(peer.throw_memory.started),
            force_powers_active: state.force_powers_active(),
            grip_being_gripped: peer.force.grip_being_gripped,
            muzzle_point: peer.muzzle.0,
            muzzle_point_old: peer.muzzle.1,
            blades: [blade(0), blade(1)],
        })
    }

    /// Entity `number`'s place, trajectory delta and weapon: a missile in flight, or a
    /// player's saber entity (in hand or thrown); `None` for anything else.
    pub(super) fn entity_motion_of(&self, number: u16) -> Option<EntityMotion> {
        if let Some((_, missile)) = self
            .missiles
            .iter()
            .find(|(id, _)| id.legacy_number() == number)
        {
            return Some(motion(missile.current, &missile.state));
        }
        let world = self.server.world(self.world)?;
        let peer = (0..self.roster.places()).find_map(|client| {
            world
                .entity(self.roster.at(client)?)
                .filter(|peer| peer.saber_entity.is_some() && peer.saber_number() == number)
        })?;
        let state = self.pool.state(peer.saber_entity?)?;
        Some(motion(peer.saber_cut.entity.0, state))
    }

    /// What may come at an NPC in the box (`WP_SaberStartMissileBlockCheck`'s
    /// `EntitiesInBox`): the missiles in flight, and the players' sabers out of their hands
    /// — flying or knocked down — whose linked boxes meet it.
    pub(super) fn incoming_of(
        &self,
        mins: [f32; 3],
        maxs: [f32; 3],
        out: &mut Vec<IncomingEntity>,
    ) {
        let linked = self.missiles.iter().filter(|(_, missile)| missile.linked);
        out.extend(
            linked
                .map(|(id, missile)| IncomingEntity::of_missile(id.legacy_number(), missile))
                .filter(|entity| entity.meets(mins, maxs)),
        );
        let Some(world) = self.server.world(self.world) else {
            return;
        };
        for client in 0..self.roster.places() {
            let Some(peer) = self
                .roster
                .at(client)
                .and_then(|handle| world.entity(handle))
                .filter(|peer| peer.begun && peer.body_active())
            else {
                continue;
            };
            let (Some(id), flight) = (peer.saber_entity, &peer.flight) else {
                continue;
            };
            let Some(state) = self.pool.state(id) else {
                continue;
            };
            let knocked = state.raw_field(ES_TYPE).unwrap_or(0) == ET_MISSILE;
            let flying =
                peer.state.saber_in_flight() && peer.state.saber_entity_num() == id.legacy_number();
            if !(knocked || flying) {
                continue;
            }
            let read = |index: usize| state.raw_field(index).unwrap_or(0);
            let entity = IncomingEntity {
                number: id.legacy_number(),
                owner: client as u16,
                origin: flight.current,
                mins: flight.mins,
                maxs: flight.maxs,
                delta: ES_POS_DELTA.map(|index| f32::from_bits(read(index))),
                trajectory: read(ES_POS_TYPE) as u8,
                weapon: read(ES_WEAPON) as i32,
                next_think: flight.next_think,
                clip_mask: flight.clip_mask,
                method_of_death: MOD_SABER,
                ..IncomingEntity::default()
            };
            if entity.meets(mins, maxs) {
                out.push(entity);
            }
        }
    }

    /// A begun, playing peer by its place, its state remembered as touched by the power.
    fn reach(&mut self, number: u16) -> Option<&mut crate::peer::Peer> {
        let handle = self.roster.at(usize::from(number))?;
        let peer = self
            .server
            .world_mut(self.world)?
            .entity_mut(handle)
            .filter(|peer| peer.begun && peer.body_active())?;
        remember_touched(
            &mut self.force_touch.touched,
            &mut self.force_touch.before,
            number,
            &peer.state,
        );
        Some(peer)
    }
}

impl PlayerForce for ServerHost<'_> {
    fn force_player(&mut self, number: u16) -> Option<OtherPlayer<'_>> {
        let peer = self.reach(number)?;
        let (absmin, absmax) = linked_box(peer);
        Some(OtherPlayer {
            state: &mut peer.state,
            force: &mut peer.force,
            health: &mut peer.health,
            knockdown: &mut peer.knockdown,
            other_killer: &mut peer.wounds.other_killer,
            npc_class: None,
            team: peer.session.team,
            absmin,
            absmax,
        })
    }

    fn force_throw_player(&mut self, number: u16) -> Option<ThrowPlayer<'_>> {
        let peer = self.reach(number)?;
        Some(ThrowPlayer {
            state: &mut peer.state,
            force: &mut peer.force,
            health: peer.health,
            knockdown: &mut peer.knockdown,
            other_killer: &mut peer.wounds.other_killer,
            npc_class: None,
            invulnerable_until: &mut peer.invulnerable_until,
            command: peer.last_command,
            team: peer.session.team,
            push_effect_until: &mut peer.push_effect_until,
            lock_hits: &mut peer.lock.hits,
        })
    }

    /// Deferred to the end of the NPC's turn ([`NpcOutcome::ForceBlow`]), in order: a player's
    /// `G_Damage` reaches the whole game (its death, the scores, the log). The NPC's hit
    /// counter gains its share then (`strike`'s `record_hit`), so nothing here.
    fn force_damage(&mut self, victim: u16, request: DamageRequest) -> (i32, Option<u32>) {
        self.outcomes.push(NpcOutcome::ForceBlow(victim, request));
        (0, None)
    }

    fn force_entities_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3], out: &mut Vec<Candidate>) {
        if let Some(world) = self.server.world(self.world) {
            player_candidates(world, self.roster, mins, maxs, out);
        }
        missile_candidates(self.missiles, mins, maxs, out);
    }

    fn force_reflect_missile(
        &mut self,
        missile: u16,
        thrower: u16,
        origin: [f32; 3],
        forward: [f32; 3],
    ) {
        let Some(owner) = self
            .missiles
            .iter()
            .find(|(number, _)| number.legacy_number() == missile)
            .map(|(_, missile)| missile.owner)
        else {
            return;
        };
        // The shooter where it stands: a player, else a body among the solids (an NPC as
        // the frame began).
        let shooter = self
            .peer(owner)
            .map(|peer| peer.state.origin())
            .or_else(|| {
                self.solids
                    .iter()
                    .find(|body| body.entity == owner)
                    .map(|body| body.origin)
            });
        reflect_from(
            self.missiles,
            self.crt,
            missile,
            (thrower, origin),
            forward,
            shooter,
            self.level_time,
        );
    }

    /// The weapon out of the player's hand at once; its pickup spawned as the use ends.
    fn force_toss_weapon(&mut self, victim: u16, direction: [f32; 3], speed: f32) {
        let (level_time, gametype) = (self.level_time, self.gametype);
        let Some(peer) = self.reach(victim) else {
            return;
        };
        if let Some(dropped) = toss_weapon_of(peer, direction, speed, gametype, level_time) {
            self.force_touch.tossed.push(dropped);
        }
    }

    fn force_pool(&mut self) -> &mut EntityPool {
        self.pool
    }

    fn force_loop_stopped(&mut self, player: u16, tracker: u16) {
        self.force_touch.stopped.push((player, tracker));
    }

    fn force_since_last_frame(&self) -> i32 {
        self.since_last_frame
    }

    fn force_begin(&mut self) {
        let touch = &mut *self.force_touch;
        if touch.depth == 0 {
            touch.touched.clear();
            touch.tossed.clear();
            touch.stopped.clear();
        }
        touch.depth += 1;
    }

    fn force_end(&mut self) {
        let touch = &mut *self.force_touch;
        touch.depth = touch.depth.saturating_sub(1);
        if touch.depth != 0 {
            return;
        }
        for (player, tracker) in touch.stopped.drain(..) {
            self.told.push(Told::Everyone(
                format!("kls {player} {tracker}").into_bytes(),
            ));
        }
        for dropped in touch.tossed.drain(..) {
            if let Some(number) = self
                .pool
                .spawn_entity(dropped.state.clone(), self.level_time)
            {
                self.pool.set_bounds(number, dropped.bounds);
                self.pickups.push((number, dropped));
            }
        }
        let Some(world) = self.server.world_mut(self.world) else {
            return;
        };
        for (at, number) in touch.touched.iter().enumerate() {
            let peer = self
                .roster
                .at(usize::from(*number))
                .and_then(|handle| world.entity_mut(handle));
            if let Some(peer) = peer
                && peer.state != touch.before[at]
            {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        touch.touched.clear();
    }
}

/// An entity's motion from its wire state, at `origin`.
fn motion(origin: [f32; 3], state: &EntityState) -> EntityMotion {
    let read = |index: usize| state.raw_field(index).unwrap_or(0);
    EntityMotion {
        origin,
        delta: ES_POS_DELTA.map(|index| f32::from_bits(read(index))),
        weapon: read(ES_WEAPON) as i32,
    }
}
