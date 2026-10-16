//! What the flying machines' AI share (NPC plan step 8): the sentry's
//! ([`crate::npc_sentry`]), the imperial probe's ([`crate::npc_probe`]), the seeker's
//! ([`crate::npc_seeker`]) and the remote's ([`crate::npc_remote`]). Their muzzles read from
//! the server-side model at the machine's whole `r.currentAngles` (`G2API_AddBolt`,
//! `G2API_GetBoltMatrix`, `BG_GiveMeVectorFromMatrix`), their bolts made by `CreateMissile`
//! (`g_missile.c:297-325`) with their own weapon, damage and means, their strafe (the same
//! in `Sentry_Strafe`, `ImperialProbe_Strafe` and `Remote_Strafe`), the idle every one of
//! them falls back on (`NPC_BSIdle`, `NPC_AI_Default.c:171-188`) and the blows they deal
//! themselves (`G_Damage` on the machine with its enemy, or nobody, to blame).
//!
//! Every place is an entity's number inside the game; the missile is handed to the host,
//! which owns the missiles ([`NpcHost::launch_missile`]).

use crate::damage::{Attacker, DamageRequest};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::weapon_fire::Missile;
use sjk_protocol::UserCommand;

/// `MASK_SOLID`; `BUTTON_WALKING`; `SCF_CHASE_ENEMIES`, `SCF_LOOK_FOR_ENEMIES`.
pub(crate) const MASK_SOLID: u32 = 0x1 | 0x1000;
pub(crate) const BUTTON_WALKING: u16 = 16;
pub(crate) const SCF_CHASE_ENEMIES: u32 = 0x400;
pub(crate) const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
/// `s.loopSound`, `s.owner`, `s.angles` (the entity's wire fields).
pub(crate) const ES_LOOP_SOUND: usize = 55;
pub(crate) const ES_OWNER: usize = 40;
/// `ps.electrifyTime`, `ps.torsoTimer`.
pub(crate) const PS_ELECTRIFY_TIME: usize = 73;
pub(crate) const PS_TORSO_TIMER: usize = 20;
/// `ENTITYNUM_NONE`.
pub(crate) const ENTITYNUM_NONE: u16 = 1_023;
/// `STAT_MAX_HEALTH`: a player attacker's handicap.
const STAT_MAX_HEALTH: usize = 8;
/// `WP_BRYAR_PISTOL`, `WP_BLASTER`: the weapons their bolts are drawn as.
pub(crate) const WP_BRYAR_PISTOL: u32 = 4;
pub(crate) const WP_BLASTER: u32 = 5;
/// `CreateMissile`'s `life` every machine gives its bolt.
const MISSILE_LIFE_MS: i32 = 10_000;

/// What a machine keeps on its entity beyond `gNPC_t`: `fly_sound_debounce_time` (the
/// sentry's pause before it closes up) and `random` (a seeker's place about its owner).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MachineMemory {
    /// `ent->fly_sound_debounce_time`.
    pub fly_sound_debounce_time: i32,
    /// `ent->random`.
    pub random: f32,
}

/// A bolt a machine fires: `s.weapon`, `damage`, `methodOfDeath` (its `dflags`,
/// `DAMAGE_DEATH_KNOCKBACK`, is never passed on) and its speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MachineBolt {
    pub weapon: u32,
    pub damage: i32,
    pub means: u32,
    pub speed: f32,
}

/// `BG_GiveMeVectorFromMatrix(matrix, ORIGIN)`.
pub(crate) fn matrix_origin(matrix: &[[f32; 4]; 3]) -> [f32; 3] {
    [matrix[0][3], matrix[1][3], matrix[2][3]]
}

/// `vel *= decay`, and zero below `floor` (the machines' friction on one axis).
pub(crate) fn damped(value: f32, decay: f32, floor: f32) -> f32 {
    if value == 0.0 {
        return value;
    }
    let value = value * decay;
    if f64::from(value).abs() < f64::from(floor) {
        0.0
    } else {
        value
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G2API_AddBolt(NPC->ghoul2, 0, name)` and its `G2API_GetBoltMatrix` at the machine's
    /// `r.currentAngles` and `r.currentOrigin` (`modelScale` its own): the muzzle's origin.
    pub(crate) fn machine_muzzle(&mut self, me: usize, name: &'static str) -> [f32; 3] {
        let index = self.add_bolt(me, name);
        matrix_origin(&self.bolt_matrix(me, index))
    }

    /// `CreateMissile(origin, dir, speed, 10000, NPC, qfalse)` with the class AI's fields: the
    /// origin snapped (`SnapVector` on the caller's own vector, which is returned), its
    /// delta snapped, owned by `owner` (`r.ownerNum`), spawned now.
    pub(crate) fn machine_missile(
        &mut self,
        me: usize,
        origin: [f32; 3],
        direction: [f32; 3],
        bolt: MachineBolt,
        owner: u16,
    ) -> [f32; 3] {
        let origin = crate::weapon_fire::snap_vector(origin);
        let mut missile: Missile = crate::weapon_fire::create_missile_by(
            owner,
            origin,
            direction,
            bolt.speed,
            self.level_time,
            false,
        );
        missile.free_at = self.level_time + MISSILE_LIFE_MS;
        missile
            .state
            .set_raw_field(crate::npc_spawn::es::WEAPON, bolt.weapon);
        missile.damage = bolt.damage;
        missile.method_of_death = bolt.means;
        let number = self.actors[me].number;
        self.host.launch_missile(number, missile);
        origin
    }

    /// `Sentry_Strafe`, `ImperialProbe_Strafe` and `Remote_Strafe` (`NPC_AI_Sentry.c:357-384`,
    /// `NPC_AI_ImperialProbe.c:167-193`, `NPC_AI_Remote.c:162-190`): a side drawn (the C
    /// library's `rand()`), and — where the world leaves `distance` of room that way — a push
    /// of `push` to that side and `upward` up (a hiss with it, for the remote), held three
    /// seconds and more.
    pub(crate) fn machine_strafe(
        &mut self,
        me: usize,
        distance: i32,
        push: i32,
        upward: f32,
        hiss: Option<&[u8]>,
    ) {
        let npc = &self.actors[me];
        let right = crate::pmove::flight::flight_axes(npc.mind.eye_angles)
            .1
            .to_array();
        let side = if self.level.crt.next() & 1 != 0 {
            -1
        } else {
            1
        };
        let (origin, number) = (npc.current_origin, npc.number);
        let reach = (distance * side) as f32;
        let end: [f32; 3] = std::array::from_fn(|axis| origin[axis] + reach * right[axis]);
        let trace = self.trace_bodies(origin, [0.0; 3], [0.0; 3], end, number, MASK_SOLID);
        if trace.fraction <= 0.9 {
            return;
        }
        let push = (push * side) as f32;
        let velocity = self.actors[me].player.velocity();
        let mut velocity: [f32; 3] =
            std::array::from_fn(|axis| velocity[axis] + push * right[axis]);
        if let Some(hiss) = hiss {
            self.creature_sound(number, crate::npc_creature::CHAN_AUTO, hiss);
        }
        velocity[2] += upward;
        self.actors[me].player.set_velocity(velocity);
        let stand =
            ((self.level_time + 3_000) as f32 + self.host.rng().flrand(0.0, 1.0) * 500.0) as i32;
        self.actors[me].mind.stand_time = stand;
    }

    /// The push toward its enemy a hunting machine gives itself, `speed` along the way to it
    /// (`VectorMA(velocity, speed, forward, velocity)`).
    pub(crate) fn machine_advance(&mut self, me: usize, target: [f32; 3], speed: f32) {
        let npc = &mut self.actors[me];
        let (forward, _) = crate::npc_jedi_patrol::normalized(crate::npc_senses::subtract(
            target,
            npc.current_origin,
        ));
        let velocity = npc.player.velocity();
        npc.player.set_velocity(std::array::from_fn(|axis| {
            velocity[axis] + speed * forward[axis]
        }));
    }

    /// A hunting machine that cannot see its enemy: its goal the enemy, within `radius`, and
    /// the older navigator's way there (`NPC_GetMoveDirection`, [`crate::npc_nav_old`]); `None`
    /// where it found none.
    pub(crate) fn machine_seek_unseen(
        &mut self,
        me: usize,
        radius: i32,
        command: &mut UserCommand,
    ) -> Option<[f32; 3]> {
        let npc = &mut self.actors[me];
        npc.mind.goal = npc.mind.enemy;
        npc.mind.tactics.goal_radius = radius;
        self.get_move_direction(me, command)
            .map(|(forward, _)| forward)
    }

    /// `VectorMA(velocity, speed, forward, velocity)`: a hunting machine's push along `forward`.
    pub(crate) fn machine_push(&mut self, me: usize, forward: [f32; 3], speed: f32) {
        let npc = &mut self.actors[me];
        let velocity = npc.player.velocity();
        npc.player.set_velocity(std::array::from_fn(|axis| {
            velocity[axis] + speed * forward[axis]
        }));
    }

    /// `NPC_BSIdle` (`NPC_AI_Default.c:171-188`): to its goal, if it has one; its angles; a
    /// walk.
    pub(crate) fn machine_idle(&mut self, me: usize, command: &mut UserCommand) {
        if self.update_goal(me, command).is_some() {
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
        command.buttons |= BUTTON_WALKING;
    }

    /// `G_Damage(NPC, inflictor, attacker, dir, point, damage, dflags, mod)` a machine deals
    /// itself: `blamed` its inflictor and attacker (its enemy, itself, or nobody).
    pub(crate) fn machine_self_damage(
        &mut self,
        me: usize,
        blamed: Option<u16>,
        place: Option<([f32; 3], [f32; 3])>,
        damage: i32,
        flags: u32,
        means: u32,
    ) {
        let attacker = blamed.map(|number| self.machine_attacker(number));
        let request = DamageRequest {
            level_time: self.level_time,
            attacker,
            direction: place.map(|place| place.0),
            point: place.map(|place| place.1),
            damage,
            flags,
            means,
        };
        let number = self.actors[me].number;
        self.host
            .noting_damage(number, blamed.unwrap_or(ENTITYNUM_NONE), &request);
        self.damage(
            me,
            crate::npc_damage::NpcBlow {
                request,
                spared_by_master: false,
                surface: None,
            },
        );
    }

    /// What `G_Damage` reads of entity `number` as the attacker: an NPC's own, or a
    /// player's (its handicap, its team).
    fn machine_attacker(&mut self, number: u16) -> Attacker {
        if let Some(at) = self.actor_at(number) {
            let npc = &self.actors[at];
            return Attacker {
                npc: true,
                client: number,
                max_health: npc.player.stats[STAT_MAX_HEALTH] as i32,
                team: npc.session_team,
                saber_knockback: [0.0; 4],
            };
        }
        let team = self.body(number).map_or(0, |body| body.session_team);
        let max_health = self.host.player_force().and_then(|players| {
            players.force_begin();
            let most = players
                .force_player(number)
                .map(|player| player.state.stats[STAT_MAX_HEALTH] as i32);
            players.force_end();
            most
        });
        Attacker {
            npc: false,
            client: number,
            max_health: max_health.unwrap_or(100),
            team,
            saber_knockback: [0.0; 4],
        }
    }

    /// `s.loopSound` set to the sound `name`.
    pub(crate) fn machine_loop_sound(&mut self, me: usize, name: &[u8]) {
        let sound = self.host.sound_index(name);
        self.actors[me]
            .state
            .set_raw_field(ES_LOOP_SOUND, u32::from(sound));
    }
}
